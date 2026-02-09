//! Localhost HTTP Server for Clawcolator
//!
//! Provides REST API for interacting with Clawcolator engine

#![cfg(all(feature = "localhost", feature = "clawcolator"))]

use crate::clawcolator::*;
use crate::{RiskParams, U128};

/// Simple in-memory server state
pub struct ServerState {
    pub engine: ClawcolatorEngine,
    pub agent: Box<dyn OpenClawAgent + Send + Sync>,
}

impl ServerState {
    pub fn new(agent: Box<dyn OpenClawAgent + Send + Sync>) -> Self {
        let base_params = RiskParams {
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
        };
        
        Self {
            engine: ClawcolatorEngine::new(base_params),
            agent,
        }
    }
}
