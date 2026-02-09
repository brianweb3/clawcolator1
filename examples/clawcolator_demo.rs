//! Clawcolator Demo - демонстрация работы агента
//!
//! Запуск: cargo run --features clawcolator --example clawcolator_demo

#![cfg(feature = "clawcolator")]

use percolator::clawcolator::*;
use percolator::{RiskParams, U128, Result, MAX_ORACLE_PRICE};

// Простой агент для демонстрации (упрощенная версия из тестов)
struct SimpleClawAgent {
    max_position_size: u128,
    max_leverage_bps: u64,
    spread_bps: u64,
}

impl SimpleClawAgent {
    fn new(max_position_size: u128, max_leverage_bps: u64, spread_bps: u64) -> Self {
        Self {
            max_position_size,
            max_leverage_bps,
            spread_bps,
        }
    }
}

impl OpenClawAgent for SimpleClawAgent {
    fn decide_trade(&self, context: &AgentContext, request: &TradeRequest) -> Result<TradeDecision> {
        if context.risk_reduction_mode {
            return Ok(TradeDecision::Reject { reason: TradeRejectionReason::RiskLimit });
        }
        
        let abs_size = request.size.abs() as u128;
        if abs_size > self.max_position_size {
            return Ok(TradeDecision::Reject { reason: TradeRejectionReason::RiskLimit });
        }
        
        let notional = (abs_size * context.oracle_price as u128) / 1_000_000;
        let leverage_bps = if context.total_capital > 0 {
            ((notional * 10_000) / context.total_capital) as u64
        } else {
            return Ok(TradeDecision::Reject { reason: TradeRejectionReason::InsufficientLiquidity });
        };
        
        if leverage_bps > self.max_leverage_bps {
            return Ok(TradeDecision::Reject { reason: TradeRejectionReason::RiskLimit });
        }
        
        let spread_amount = (context.oracle_price as u128 * self.spread_bps as u128) / 10_000;
        let execution_price = if request.size > 0 {
            context.oracle_price.saturating_add(spread_amount as u64)
        } else {
            context.oracle_price.saturating_sub(spread_amount as u64)
        };
        
        if execution_price == 0 || execution_price > MAX_ORACLE_PRICE {
            return Ok(TradeDecision::Reject { reason: TradeRejectionReason::MarketConditions });
        }
        
        Ok(TradeDecision::Accept { price: execution_price, size: request.size })
    }
    
    fn get_market_params(&self, _context: &AgentContext) -> Result<MarketParams> {
        Ok(MarketParams {
            max_leverage_bps: self.max_leverage_bps,
            max_position_size: self.max_position_size,
            spread_bps: self.spread_bps,
            funding_rate_bps_per_slot: 0,
            min_margin_bps: 500,
            active_capital_ratio_bps: 8000,
        })
    }
    
    fn decide_liquidity_allocation(&self, context: &AgentContext) -> Result<LiquidityAllocation> {
        let reserve_ratio = 2000;
        let reserve_capital = (context.total_capital * reserve_ratio) / 10_000;
        let target_active_capital = context.total_capital.saturating_sub(reserve_capital);
        Ok(LiquidityAllocation {
            target_active_capital,
            reserve_capital,
            defensive_mode: context.risk_reduction_mode,
        })
    }
    
    fn assess_risk(&self, context: &AgentContext) -> Result<RiskAssessment> {
        let utilization_bps = if context.total_capital > 0 {
            let used_capital = (context.total_open_interest * context.oracle_price as u128) / 1_000_000;
            ((used_capital * 10_000) / context.total_capital) as u64
        } else {
            0
        };
        
        let risk_level = utilization_bps.min(10000);
        let mut actions = RiskActions::default();
        if utilization_bps > 8000u64 {
            actions.reduce_exposure = true;
        }
        if utilization_bps > 9000u64 {
            actions.increase_margin = Some(1000);
        }
        
        Ok(RiskAssessment { risk_level_bps: risk_level, actions })
    }
    
    fn detect_anomalies(&self, context: &AgentContext) -> Result<AnomalyResponse> {
        let insurance_ratio = if context.vault > 0 {
            (context.insurance_balance * 10_000) / context.vault
        } else {
            0
        };
        
        if insurance_ratio < 500 {
            return Ok(AnomalyResponse {
                anomaly_type: AnomalyType::LiquidityCrisis,
                severity_bps: 5000,
                actions: AnomalyActions {
                    reduce_limits: Some(self.max_position_size / 2),
                    stop_trading: false,
                    freeze_market: false,
                    initiate_shutdown: false,
                },
            });
        }
        
        Ok(AnomalyResponse {
            anomaly_type: AnomalyType::Other,
            severity_bps: 0,
            actions: AnomalyActions::default(),
        })
    }
    
    fn should_shutdown(&self, context: &AgentContext) -> Result<bool> {
        let insurance_ratio = if context.vault > 0 {
            (context.insurance_balance * 10_000) / context.vault
        } else {
            0
        };
        Ok(insurance_ratio < 100)
    }
}

fn main() {
    println!("🦾 Clawcolator Demo\n");
    println!("{}", "=".repeat(50));
    
    // Создаем простого агента
    println!("\n1️⃣ Создание OpenClaw агента...");
    let agent = SimpleClawAgent::new(
        1_000_000,  // max_position_size
        1000,       // max_leverage_bps (10x)
        10,         // spread_bps (0.1%)
    );
    println!("   ✅ Агент создан с параметрами:");
    println!("      - Максимальный размер позиции: 1,000,000");
    println!("      - Максимальное плечо: 10x (1000 bps)");
    println!("      - Спред: 0.1% (10 bps)");
    
    // Создаем движок
    println!("\n2️⃣ Создание Clawcolator движка...");
    let base_params = RiskParams {
        warmup_period_slots: 100,
        maintenance_margin_bps: 500,  // 5%
        initial_margin_bps: 1000,     // 10%
        trading_fee_bps: 10,          // 0.1%
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
    
    let mut engine = ClawcolatorEngine::new(base_params);
    println!("   ✅ Движок создан");
    
    // Демонстрация принятия решения о сделке
    println!("\n3️⃣ Демонстрация принятия решения о сделке...");
    let context = AgentContext {
        current_slot: 1000,
        oracle_price: 1_000_000,
        vault: 10_000_000,
        insurance_balance: 1_000_000,
        total_capital: 9_000_000,
        total_positive_pnl: 0,
        total_open_interest: 0,
        risk_params: base_params,
        risk_reduction_mode: false,
        last_crank_slot: 999,
    };
    
    let request = TradeRequest {
        user_idx: 0,
        size: 1000,
        requested_price: None,
    };
    
    match agent.decide_trade(&context, &request) {
        Ok(TradeDecision::Accept { price, size }) => {
            println!("   ✅ Агент принял сделку:");
            println!("      - Цена исполнения: {}", price);
            println!("      - Размер: {}", size);
            println!("      - Спред: {} bps", ((price as i64 - context.oracle_price as i64) * 10_000 / context.oracle_price as i64));
        }
        Ok(TradeDecision::Reject { reason }) => {
            println!("   ❌ Агент отклонил сделку: {:?}", reason);
        }
        Ok(TradeDecision::RequestQuote { quote_price, max_size }) => {
            println!("   📊 Агент запросил котировку:");
            println!("      - Цена: {}", quote_price);
            println!("      - Макс. размер: {}", max_size);
        }
        Err(e) => {
            println!("   ⚠️ Ошибка: {:?}", e);
        }
    }
    
    // Демонстрация отклонения слишком большой сделки
    println!("\n4️⃣ Демонстрация отклонения слишком большой сделки...");
    let large_request = TradeRequest {
        user_idx: 0,
        size: 2_000_000, // Превышает max_position_size
        requested_price: None,
    };
    
    match agent.decide_trade(&context, &large_request) {
        Ok(TradeDecision::Reject { reason }) => {
            println!("   ✅ Агент правильно отклонил сделку:");
            println!("      - Причина: {:?}", reason);
            println!("      - Размер запроса: {} (превышает лимит 1,000,000)", large_request.size);
        }
        _ => {
            println!("   ⚠️ Неожиданное решение агента");
        }
    }
    
    // Демонстрация получения параметров рынка
    println!("\n5️⃣ Получение параметров рынка от агента...");
    match agent.get_market_params(&context) {
        Ok(params) => {
            println!("   ✅ Параметры рынка:");
            println!("      - Макс. плечо: {} bps ({}x)", params.max_leverage_bps, params.max_leverage_bps / 1000);
            println!("      - Макс. размер позиции: {}", params.max_position_size);
            println!("      - Спред: {} bps", params.spread_bps);
            println!("      - Funding rate: {} bps/slot", params.funding_rate_bps_per_slot);
            println!("      - Мин. маржа: {} bps ({}%)", params.min_margin_bps, params.min_margin_bps / 100);
            println!("      - Активный капитал: {} bps ({}%)", params.active_capital_ratio_bps, params.active_capital_ratio_bps / 100);
        }
        Err(e) => {
            println!("   ⚠️ Ошибка получения параметров: {:?}", e);
        }
    }
    
    // Демонстрация оценки риска
    println!("\n6️⃣ Оценка риска агентом...");
    match agent.assess_risk(&context) {
        Ok(assessment) => {
            println!("   ✅ Оценка риска:");
            println!("      - Уровень риска: {} bps ({}%)", assessment.risk_level_bps, assessment.risk_level_bps / 100);
            println!("      - Действия:");
            println!("        • Снизить экспозицию: {}", assessment.actions.reduce_exposure);
            println!("        • Хеджировать: {}", assessment.actions.hedge);
            if let Some(margin) = assessment.actions.increase_margin {
                println!("        • Увеличить маржу до: {} bps ({}%)", margin, margin / 100);
            }
        }
        Err(e) => {
            println!("   ⚠️ Ошибка оценки риска: {:?}", e);
        }
    }
    
    // Демонстрация обнаружения аномалий
    println!("\n7️⃣ Обнаружение аномалий...");
    match agent.detect_anomalies(&context) {
        Ok(response) => {
            println!("   ✅ Результат обнаружения:");
            println!("      - Тип аномалии: {:?}", response.anomaly_type);
            println!("      - Серьезность: {} bps ({}%)", response.severity_bps, response.severity_bps / 100);
            println!("      - Действия:");
            println!("        • Заморозить рынок: {}", response.actions.freeze_market);
            println!("        • Остановить торговлю: {}", response.actions.stop_trading);
            println!("        • Инициировать shutdown: {}", response.actions.initiate_shutdown);
            if let Some(limit) = response.actions.reduce_limits {
                println!("        • Снизить лимиты до: {}", limit);
            }
        }
        Err(e) => {
            println!("   ⚠️ Ошибка обнаружения аномалий: {:?}", e);
        }
    }
    
    // Демонстрация проверки shutdown
    println!("\n8️⃣ Проверка необходимости shutdown...");
    match agent.should_shutdown(&context) {
        Ok(should_shutdown) => {
            if should_shutdown {
                println!("   ⚠️ Агент рекомендует shutdown системы");
            } else {
                println!("   ✅ Система работает нормально, shutdown не требуется");
            }
        }
        Err(e) => {
            println!("   ⚠️ Ошибка проверки shutdown: {:?}", e);
        }
    }
    
    println!("\n{}", "=".repeat(50));
    println!("\n✅ Демонстрация завершена!");
    println!("\n💡 Clawcolator успешно делегирует все решения OpenClaw агенту,");
    println!("   а протокол обеспечивает безопасность и валидацию.");
}
