//! Simple example OpenClaw agent implementation
//!
//! This demonstrates a basic agent that:
//! - Accepts trades within risk limits
//! - Sets conservative market parameters
//! - Detects basic anomalies

#![cfg(feature = "clawcolator")]

use percolator::clawcolator::*;
use percolator::{Result, MAX_ORACLE_PRICE};

/// Simple rule-based OpenClaw agent
pub struct SimpleClawAgent {
    /// Maximum position size this agent will accept
    max_position_size: u128,
    
    /// Maximum leverage (in basis points)
    max_leverage_bps: u64,
    
    /// Spread to apply (in basis points)
    spread_bps: u64,
}

impl SimpleClawAgent {
    pub fn new(max_position_size: u128, max_leverage_bps: u64, spread_bps: u64) -> Self {
        Self {
            max_position_size,
            max_leverage_bps,
            spread_bps,
        }
    }
}

impl OpenClawAgent for SimpleClawAgent {
    fn decide_trade(
        &self,
        context: &AgentContext,
        request: &TradeRequest,
    ) -> Result<TradeDecision> {
        // Check if system is in risk reduction mode
        if context.risk_reduction_mode {
            return Ok(TradeDecision::Reject {
                reason: TradeRejectionReason::RiskLimit,
            });
        }
        
        // Check position size limits
        let abs_size = request.size.abs() as u128;
        if abs_size > self.max_position_size {
            return Ok(TradeDecision::Reject {
                reason: TradeRejectionReason::RiskLimit,
            });
        }
        
        // Check leverage
        let notional = (abs_size * context.oracle_price as u128) / 1_000_000;
        let leverage_bps = if context.total_capital > 0 {
            ((notional * 10_000) / context.total_capital) as u64
        } else {
            return Ok(TradeDecision::Reject {
                reason: TradeRejectionReason::InsufficientLiquidity,
            });
        };
        
        if leverage_bps > self.max_leverage_bps {
            return Ok(TradeDecision::Reject {
                reason: TradeRejectionReason::RiskLimit,
            });
        }
        
        // Apply spread
        let spread_amount = (context.oracle_price as u128 * self.spread_bps as u128) / 10_000;
        let execution_price = if request.size > 0 {
            // Long: pay slightly above oracle
            context.oracle_price.saturating_add(spread_amount as u64)
        } else {
            // Short: receive slightly below oracle
            context.oracle_price.saturating_sub(spread_amount as u64)
        };
        
        // Ensure price is within bounds
        if execution_price == 0 || execution_price > MAX_ORACLE_PRICE {
            return Ok(TradeDecision::Reject {
                reason: TradeRejectionReason::MarketConditions,
            });
        }
        
        Ok(TradeDecision::Accept {
            price: execution_price,
            size: request.size,
        })
    }
    
    fn get_market_params(
        &self,
        _context: &AgentContext,
    ) -> Result<MarketParams> {
        Ok(MarketParams {
            max_leverage_bps: self.max_leverage_bps,
            max_position_size: self.max_position_size,
            spread_bps: self.spread_bps,
            funding_rate_bps_per_slot: 0, // No funding for simplicity
            min_margin_bps: 500, // 5% minimum margin
            active_capital_ratio_bps: 8000, // 80% active, 20% reserve
        })
    }
    
    fn decide_liquidity_allocation(
        &self,
        context: &AgentContext,
    ) -> Result<LiquidityAllocation> {
        // Keep 20% in reserve
        let reserve_ratio = 2000; // 20% in basis points
        let reserve_capital = (context.total_capital * reserve_ratio) / 10_000;
        let target_active_capital = context.total_capital.saturating_sub(reserve_capital);
        
        Ok(LiquidityAllocation {
            target_active_capital,
            reserve_capital,
            defensive_mode: context.risk_reduction_mode,
        })
    }
    
    fn assess_risk(
        &self,
        context: &AgentContext,
    ) -> Result<RiskAssessment> {
        // Simple risk calculation based on utilization
        let utilization_bps = if context.total_capital > 0 {
            let used_capital = (context.total_open_interest * context.oracle_price as u128) / 1_000_000;
            ((used_capital * 10_000) / context.total_capital) as u64
        } else {
            0
        };
        
        let risk_level = utilization_bps.min(10000); // Cap at 100%
        
        let mut actions = RiskActions::default();
        
        // If utilization > 80%, reduce exposure
        if utilization_bps > 8000u64 {
            actions.reduce_exposure = true;
        }
        
        // If utilization > 90%, increase margin
        if utilization_bps > 9000u64 {
            actions.increase_margin = Some(1000); // 10% margin
        }
        
        Ok(RiskAssessment {
            risk_level_bps: risk_level,
            actions,
        })
    }
    
    fn detect_anomalies(
        &self,
        context: &AgentContext,
    ) -> Result<AnomalyResponse> {
        // Simple anomaly detection: check if insurance fund is too low
        let insurance_ratio = if context.vault > 0 {
            (context.insurance_balance * 10_000) / context.vault
        } else {
            0
        };
        
        // If insurance < 5% of vault, that's an anomaly
        if insurance_ratio < 500 {
            return Ok(AnomalyResponse {
                anomaly_type: AnomalyType::LiquidityCrisis,
                severity_bps: 5000, // Medium severity
                actions: AnomalyActions {
                    reduce_limits: Some(self.max_position_size / 2),
                    stop_trading: false,
                    freeze_market: false,
                    initiate_shutdown: false,
                },
            });
        }
        
        // No anomalies detected
        Ok(AnomalyResponse {
            anomaly_type: AnomalyType::Other,
            severity_bps: 0,
            actions: AnomalyActions::default(),
        })
    }
    
    fn should_shutdown(
        &self,
        context: &AgentContext,
    ) -> Result<bool> {
        // Shutdown if insurance fund is critically low (< 1% of vault)
        let insurance_ratio = if context.vault > 0 {
            (context.insurance_balance * 10_000) / context.vault
        } else {
            0
        };
        
        Ok(insurance_ratio < 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use percolator::{RiskParams, U128};
    
    fn default_params() -> RiskParams {
        RiskParams {
            warmup_period_slots: 100,
            maintenance_margin_bps: 500,
            initial_margin_bps: 1000,
            trading_fee_bps: 10,
            max_accounts: 1000,
            new_account_fee: U128::new(0),
            risk_reduction_threshold: U128::new(0),
            maintenance_fee_per_slot: U128::new(0),
            max_crank_staleness_slots: u64::MAX,
            liquidation_fee_bps: 50,
            liquidation_fee_cap: U128::new(100_000),
            liquidation_buffer_bps: 100,
            min_liquidation_abs: U128::new(100_000),
        }
    }
    
    #[test]
    fn test_simple_agent_trade_decision() {
        let agent = SimpleClawAgent::new(1_000_000, 1000, 10);
        
        let context = AgentContext {
            current_slot: 1000,
            oracle_price: 1_000_000,
            vault: 10_000_000,
            insurance_balance: 1_000_000,
            total_capital: 9_000_000,
            total_positive_pnl: 0,
            total_open_interest: 0,
            risk_params: default_params(),
            risk_reduction_mode: false,
            last_crank_slot: 999,
        };
        
        let request = TradeRequest {
            user_idx: 0,
            size: 1000,
            requested_price: None,
        };
        
        let decision = agent.decide_trade(&context, &request).unwrap();
        
        match decision {
            TradeDecision::Accept { price, size } => {
                assert_eq!(size, 1000);
                assert!(price > context.oracle_price); // Should have spread
            }
            _ => panic!("Expected Accept decision"),
        }
    }
    
    #[test]
    fn test_simple_agent_rejects_oversized_trade() {
        let agent = SimpleClawAgent::new(1_000_000, 1000, 10);
        
        let context = AgentContext {
            current_slot: 1000,
            oracle_price: 1_000_000,
            vault: 10_000_000,
            insurance_balance: 1_000_000,
            total_capital: 9_000_000,
            total_positive_pnl: 0,
            total_open_interest: 0,
            risk_params: default_params(),
            risk_reduction_mode: false,
            last_crank_slot: 999,
        };
        
        let request = TradeRequest {
            user_idx: 0,
            size: 2_000_000, // Exceeds max_position_size
            requested_price: None,
        };
        
        let decision = agent.decide_trade(&context, &request).unwrap();
        
        match decision {
            TradeDecision::Reject { reason } => {
                assert_eq!(reason, TradeRejectionReason::RiskLimit);
            }
            _ => panic!("Expected Reject decision"),
        }
    }
}
