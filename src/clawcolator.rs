//! Clawcolator: Agent-First Fork of Percolator
//!
//! ⚠️ EDUCATIONAL USE ONLY - NOT PRODUCTION READY ⚠️
//!
//! Clawcolator delegates all market decisions to an autonomous OpenClaw agent,
//! while the protocol serves as a strict enforcement and protection layer.

#![no_std]
#![forbid(unsafe_code)]

// Re-export types we need from parent module
use crate::{
    RiskEngine, RiskParams, RiskError, Result, MatchingEngine, TradeExecution,
    MAX_ORACLE_PRICE, MAX_POSITION_ABS, U128, I128,
};

// Helper function (mirrored from percolator.rs)
#[inline]
fn saturating_abs_i128(val: i128) -> i128 {
    if val == i128::MIN {
        i128::MAX
    } else {
        val.abs()
    }
}

// ============================================================================
// Agent Context (read-only view of engine state)
// ============================================================================

/// Read-only context provided to the agent for decision-making
#[derive(Clone, Debug)]
pub struct AgentContext {
    /// Current slot
    pub current_slot: u64,
    
    /// Oracle price
    pub oracle_price: u64,
    
    /// Vault balance
    pub vault: u128,
    
    /// Insurance fund balance
    pub insurance_balance: u128,
    
    /// Total capital across all accounts
    pub total_capital: u128,
    
    /// Total positive PnL
    pub total_positive_pnl: u128,
    
    /// Total open interest
    pub total_open_interest: u128,
    
    /// Current risk parameters
    pub risk_params: RiskParams,
    
    /// Whether system is in risk-reduction-only mode
    pub risk_reduction_mode: bool,
    
    /// Last crank slot
    pub last_crank_slot: u64,
}

// ============================================================================
// Trade Request & Decision
// ============================================================================

/// Trade request from user
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeRequest {
    /// User account index
    pub user_idx: u16,
    
    /// Requested position size (positive = long, negative = short)
    pub size: i128,
    
    /// Requested price (optional, agent may override)
    pub requested_price: Option<u64>,
}

/// Agent's decision about a trade
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeDecision {
    /// Accept trade with specified execution details
    Accept {
        /// Execution price
        price: u64,
        /// Execution size (may be partial fill)
        size: i128,
    },
    
    /// Reject trade
    Reject {
        /// Reason for rejection
        reason: TradeRejectionReason,
    },
    
    /// Request quote (RFQ-style)
    RequestQuote {
        /// Agent's quote price
        quote_price: u64,
        /// Maximum size at this quote
        max_size: i128,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeRejectionReason {
    /// Market conditions not favorable
    MarketConditions,
    /// Risk limits exceeded
    RiskLimit,
    /// Insufficient liquidity
    InsufficientLiquidity,
    /// Anomaly detected
    AnomalyDetected,
    /// System shutdown
    SystemShutdown,
    /// Other reason
    Other,
}

// ============================================================================
// Market Parameters (dynamic, set by agent)
// ============================================================================

/// Dynamic market parameters controlled by agent
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarketParams {
    /// Maximum leverage (in basis points, e.g., 1000 = 10x)
    pub max_leverage_bps: u64,
    
    /// Maximum position size per account
    pub max_position_size: u128,
    
    /// Bid-ask spread (in basis points)
    pub spread_bps: u64,
    
    /// Funding rate per slot (in basis points)
    pub funding_rate_bps_per_slot: i64,
    
    /// Minimum margin requirement (in basis points)
    pub min_margin_bps: u64,
    
    /// Maximum active capital ratio (0-10000 bps = 0-100%)
    /// Agent can limit how much capital is actively trading
    pub active_capital_ratio_bps: u64,
}

impl Default for MarketParams {
    fn default() -> Self {
        Self {
            max_leverage_bps: 1000, // 10x default
            max_position_size: MAX_POSITION_ABS,
            spread_bps: 10, // 0.1% default
            funding_rate_bps_per_slot: 0,
            min_margin_bps: 500, // 5% default
            active_capital_ratio_bps: 10000, // 100% default
        }
    }
}

// ============================================================================
// Liquidity Allocation
// ============================================================================

/// Agent's decision about liquidity allocation
#[derive(Clone, Debug)]
pub struct LiquidityAllocation {
    /// Target active capital (amount to keep trading)
    pub target_active_capital: u128,
    
    /// Reserve capital (amount to keep as buffer)
    pub reserve_capital: u128,
    
    /// Whether to enter defensive mode
    pub defensive_mode: bool,
}

// ============================================================================
// Risk Assessment
// ============================================================================

/// Agent's risk assessment
#[derive(Clone, Debug)]
pub struct RiskAssessment {
    /// Overall risk level (0-10000, where 10000 = maximum risk)
    pub risk_level_bps: u64,
    
    /// Recommended actions
    pub actions: RiskActions,
}

#[derive(Clone, Debug, Default)]
pub struct RiskActions {
    /// Reduce exposure
    pub reduce_exposure: bool,
    
    /// Hedge positions
    pub hedge: bool,
    
    /// Close specific positions (max 16 positions per assessment)
    pub close_positions: [u16; 16],
    pub close_positions_len: usize,
    
    /// Increase margin requirements (None = no change)
    pub increase_margin: Option<u64>, // New margin bps
}

// ============================================================================
// Anomaly Detection
// ============================================================================

/// Types of anomalies agent can detect
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnomalyType {
    /// Oracle manipulation detected
    OracleManipulation,
    /// High volatility
    HighVolatility,
    /// Unusual trading patterns
    UnusualPatterns,
    /// Liquidity crisis
    LiquidityCrisis,
    /// Other anomaly
    Other,
}

/// Agent's response to detected anomaly
#[derive(Clone, Debug)]
pub struct AnomalyResponse {
    /// Type of anomaly
    pub anomaly_type: AnomalyType,
    
    /// Severity (0-10000)
    pub severity_bps: u64,
    
    /// Recommended actions
    pub actions: AnomalyActions,
}

#[derive(Clone, Debug, Default)]
pub struct AnomalyActions {
    /// Freeze market
    pub freeze_market: bool,
    
    /// Reduce position limits
    pub reduce_limits: Option<u128>, // New max position size
    
    /// Stop trading
    pub stop_trading: bool,
    
    /// Initiate shutdown
    pub initiate_shutdown: bool,
}

// ============================================================================
// OpenClaw Agent Trait
// ============================================================================

/// Trait for OpenClaw autonomous agent
///
/// The agent is the sole decision-maker for all market operations.
/// All decisions are validated by the protocol before execution.
pub trait OpenClawAgent {
    /// Decide whether to accept, reject, or quote a trade
    ///
    /// # Arguments
    /// * `context` - Read-only view of engine state
    /// * `request` - Trade request from user
    ///
    /// # Returns
    /// * `Ok(TradeDecision)` - Agent's decision
    /// * `Err(RiskError)` - Error in decision-making (treated as rejection)
    fn decide_trade(
        &self,
        context: &AgentContext,
        request: &TradeRequest,
    ) -> Result<TradeDecision>;
    
    /// Get current market parameters
    ///
    /// Agent dynamically sets market parameters.
    /// Protocol validates these parameters before applying.
    fn get_market_params(
        &self,
        context: &AgentContext,
    ) -> Result<MarketParams>;
    
    /// Decide liquidity allocation
    ///
    /// Agent determines how much capital should be actively trading
    /// vs. kept in reserve.
    fn decide_liquidity_allocation(
        &self,
        context: &AgentContext,
    ) -> Result<LiquidityAllocation>;
    
    /// Assess current risk level
    ///
    /// Agent evaluates system risk and recommends actions.
    fn assess_risk(
        &self,
        context: &AgentContext,
    ) -> Result<RiskAssessment>;
    
    /// Detect anomalies in market conditions
    ///
    /// Agent monitors for:
    /// - Oracle manipulation
    /// - High volatility
    /// - Unusual patterns
    /// - Liquidity issues
    fn detect_anomalies(
        &self,
        context: &AgentContext,
    ) -> Result<AnomalyResponse>;
    
    /// Decide if system should shutdown
    ///
    /// Agent can initiate controlled shutdown if market conditions
    /// are deemed unsafe.
    fn should_shutdown(
        &self,
        context: &AgentContext,
    ) -> Result<bool>;
}

// ============================================================================
// Clawcolator Engine
// ============================================================================

/// Clawcolator engine wrapper around RiskEngine
///
/// Delegates all market decisions to OpenClaw agent while enforcing
/// protocol invariants and safety checks.
pub struct ClawcolatorEngine {
    /// Underlying risk engine
    engine: RiskEngine,
    
    /// Current market parameters (set by agent)
    market_params: MarketParams,
    
    /// Whether system is shutdown
    shutdown: bool,
    
    /// Whether market is frozen
    market_frozen: bool,
}

impl ClawcolatorEngine {
    /// Create new Clawcolator engine
    pub fn new(base_params: RiskParams) -> Self {
        Self {
            engine: RiskEngine::new(base_params),
            market_params: MarketParams::default(),
            shutdown: false,
            market_frozen: false,
        }
    }
    
    /// Initialize in place (for Solana BPF)
    pub fn init_in_place(&mut self, base_params: RiskParams) {
        self.engine.init_in_place(base_params);
        self.market_params = MarketParams::default();
        self.shutdown = false;
        self.market_frozen = false;
    }
    
    /// Build agent context from current engine state
    pub fn build_context(&self, oracle_price: u64) -> AgentContext {
        AgentContext {
            current_slot: self.engine.current_slot,
            oracle_price,
            vault: self.engine.vault.get(),
            insurance_balance: self.engine.insurance_fund.balance.get(),
            total_capital: self.engine.c_tot.get(),
            total_positive_pnl: self.engine.pnl_pos_tot.get(),
            total_open_interest: self.engine.total_open_interest.get(),
            risk_params: self.engine.params,
            risk_reduction_mode: false, // TODO: implement risk reduction mode check
            last_crank_slot: self.engine.last_crank_slot,
        }
    }
    
    /// Execute trade with agent decision
    ///
    /// Flow:
    /// 1. Check if system is shutdown/frozen
    /// 2. Get agent's trade decision
    /// 3. Validate decision
    /// 4. Execute via underlying risk engine
    pub fn execute_trade<A: OpenClawAgent>(
        &mut self,
        agent: &A,
        user_idx: u16,
        oracle_price: u64,
        size: i128,
        now_slot: u64,
    ) -> Result<()> {
        // Check system state
        if self.shutdown {
            return Err(RiskError::Unauthorized);
        }
        if self.market_frozen {
            return Err(RiskError::Unauthorized);
        }
        
        // Build context
        let context = self.build_context(oracle_price);
        
        // Create trade request
        let request = TradeRequest {
            user_idx,
            size,
            requested_price: None,
        };
        
        // Get agent decision
        let decision = agent.decide_trade(&context, &request)?;
        
        // Process decision
        match decision {
            TradeDecision::Accept { price, size: exec_size } => {
                // Validate agent's decision
                self.validate_trade_execution(price, exec_size, size)?;
                
                // Execute via underlying engine
                // Note: We need to adapt this to work with agent's decision
                // For now, we'll use a simple matcher that respects agent's decision
                let matcher = AgentMatcher {
                    price,
                    size: exec_size,
                };
                
                // Find LP account (in Clawcolator, agent IS the LP)
                // For now, assume LP is account 0 (this needs proper design)
                let lp_idx = 0;
                
                self.engine.execute_trade(
                    &matcher,
                    lp_idx,
                    user_idx,
                    now_slot,
                    oracle_price,
                    size,
                )
            }
            
            TradeDecision::Reject { reason: _ } => {
                Err(RiskError::Unauthorized)
            }
            
            TradeDecision::RequestQuote { quote_price: _, max_size: _ } => {
                // RFQ - return error to indicate quote needed
                Err(RiskError::Unauthorized)
            }
        }
    }
    
    /// Validate trade execution from agent
    fn validate_trade_execution(
        &self,
        price: u64,
        exec_size: i128,
        requested_size: i128,
    ) -> Result<()> {
        // Price bounds
        if price == 0 || price > MAX_ORACLE_PRICE {
            return Err(RiskError::InvalidMatchingEngine);
        }
        
        // Size bounds
        if exec_size == 0 {
            return Ok(()); // No fill is valid
        }
        if exec_size == i128::MIN {
            return Err(RiskError::InvalidMatchingEngine);
        }
        if saturating_abs_i128(exec_size) as u128 > MAX_POSITION_ABS {
            return Err(RiskError::InvalidMatchingEngine);
        }
        
        // Must be same direction as requested
        if (exec_size > 0) != (requested_size > 0) {
            return Err(RiskError::InvalidMatchingEngine);
        }
        
        // Must be partial fill at most
        if saturating_abs_i128(exec_size) > saturating_abs_i128(requested_size) {
            return Err(RiskError::InvalidMatchingEngine);
        }
        
        // Check against market params
        if saturating_abs_i128(exec_size) as u128 > self.market_params.max_position_size {
            return Err(RiskError::Undercollateralized);
        }
        
        Ok(())
    }
    
    /// Update market parameters from agent
    pub fn update_market_params<A: OpenClawAgent>(
        &mut self,
        agent: &A,
    ) -> Result<()> {
        let context = self.build_context(0); // Oracle price not needed for params
        let params = agent.get_market_params(&context)?;
        
        // Validate parameters
        self.validate_market_params(&params)?;
        
        // Apply parameters
        self.market_params = params;
        
        // Update underlying engine params if needed
        // (some params map to RiskParams, others are Clawcolator-specific)
        
        Ok(())
    }
    
    /// Validate market parameters
    fn validate_market_params(&self, params: &MarketParams) -> Result<()> {
        // Max leverage must be reasonable (e.g., <= 100x = 10000 bps)
        if params.max_leverage_bps > 10000 {
            return Err(RiskError::Overflow);
        }
        
        // Max position size must be within bounds
        if params.max_position_size > MAX_POSITION_ABS {
            return Err(RiskError::Overflow);
        }
        
        // Active capital ratio must be <= 100%
        if params.active_capital_ratio_bps > 10000 {
            return Err(RiskError::Overflow);
        }
        
        // Min margin must be >= maintenance margin
        if params.min_margin_bps < self.engine.params.maintenance_margin_bps {
            return Err(RiskError::Undercollateralized);
        }
        
        Ok(())
    }
    
    /// Check for anomalies and apply agent's response
    pub fn check_anomalies<A: OpenClawAgent>(
        &mut self,
        agent: &A,
        oracle_price: u64,
    ) -> Result<()> {
        let context = self.build_context(oracle_price);
        let response = agent.detect_anomalies(&context)?;
        
        // Apply anomaly actions
        if response.actions.freeze_market {
            self.market_frozen = true;
        }
        
        if response.actions.stop_trading {
            self.market_frozen = true;
        }
        
        if response.actions.initiate_shutdown {
            self.shutdown = true;
        }
        
        if let Some(new_max_size) = response.actions.reduce_limits {
            if new_max_size <= MAX_POSITION_ABS {
                self.market_params.max_position_size = new_max_size;
            }
        }
        
        Ok(())
    }
    
    /// Check if agent wants to shutdown
    pub fn check_shutdown<A: OpenClawAgent>(
        &mut self,
        agent: &A,
        oracle_price: u64,
    ) -> Result<()> {
        let context = self.build_context(oracle_price);
        let should_shutdown = agent.should_shutdown(&context)?;
        
        if should_shutdown {
            self.shutdown = true;
        }
        
        Ok(())
    }
    
    /// Get underlying risk engine (for direct access when needed)
    pub fn risk_engine(&self) -> &RiskEngine {
        &self.engine
    }
    
    /// Get mutable underlying risk engine (use with caution)
    pub fn risk_engine_mut(&mut self) -> &mut RiskEngine {
        &mut self.engine
    }
}

// ============================================================================
// Agent Matcher (adapter for existing MatchingEngine trait)
// ============================================================================

/// Adapter that makes agent decisions compatible with MatchingEngine trait
struct AgentMatcher {
    price: u64,
    size: i128,
}

impl MatchingEngine for AgentMatcher {
    fn execute_match(
        &self,
        _lp_program: &[u8; 32],
        _lp_context: &[u8; 32],
        _lp_account_id: u64,
        _oracle_price: u64,
        _size: i128,
    ) -> Result<TradeExecution> {
        Ok(TradeExecution {
            price: self.price,
            size: self.size,
        })
    }
}
