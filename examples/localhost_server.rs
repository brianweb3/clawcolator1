//! Clawcolator Localhost HTTP Server
//!
//! Запуск: cargo run --features localhost --example localhost_server
//!
//! API будет доступен на http://localhost:8080

#![cfg(all(feature = "localhost", feature = "clawcolator"))]

use std::net::SocketAddr;

use percolator::clawcolator::*;
use percolator::{RiskParams, U128, Result, MAX_ORACLE_PRICE};

// Простой агент для демонстрации
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

// Простой HTTP сервер на основе std::net
fn main() {
    println!("🦾 Clawcolator Localhost Server");
    println!("{}", "=".repeat(50));
    println!("\n🚀 Запуск сервера на http://localhost:8080\n");
    
    // Создаем агента
    let agent = SimpleClawAgent::new(1_000_000, 1000, 10);
    
    // Создаем движок
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
    
    let mut engine = ClawcolatorEngine::new(base_params);
    
    println!("✅ Clawcolator Engine инициализирован");
    println!("✅ OpenClaw Agent готов\n");
    
    println!("📡 API Endpoints:");
    println!("   GET  /health          - Проверка здоровья сервера");
    println!("   GET  /status          - Статус движка");
    println!("   POST /trade           - Выполнить сделку");
    println!("   GET  /market-params   - Получить параметры рынка");
    println!("   GET  /risk            - Оценка риска");
    println!("   GET  /anomalies       - Проверка аномалий");
    println!("\n{}", "=".repeat(50));
    println!("\n💡 Используйте curl или браузер для тестирования API");
    println!("   Пример: curl http://localhost:8080/health\n");
    
    // Простой HTTP сервер на std::net::TcpListener
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = std::net::TcpListener::bind(addr).expect("Failed to bind");
    
    println!("✅ Сервер запущен на {}", addr);
    println!("   Нажмите Ctrl+C для остановки\n");
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // Простая обработка HTTP запросов
                let mut buffer = [0; 1024];
                if let Ok(size) = stream.read(&mut buffer) {
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let response = handle_request(&request, &mut engine, &agent);
                    
                    let http_response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        response.len(),
                        response
                    );
                    
                    let _ = stream.write_all(http_response.as_bytes());
                }
            }
            Err(e) => {
                eprintln!("Ошибка соединения: {}", e);
            }
        }
    }
}

use std::io::{Read, Write};

fn handle_request(request: &str, engine: &mut ClawcolatorEngine, agent: &SimpleClawAgent) -> String {
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return r#"{"error": "Empty request"}"#.to_string();
    }
    
    let request_line = lines[0];
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    
    if parts.len() < 2 {
        return r#"{"error": "Invalid request"}"#.to_string();
    }
    
    let method = parts[0];
    let path = parts[1];
    
    match (method, path) {
        ("GET", "/health") => {
            r#"{"status": "ok", "service": "clawcolator"}"#.to_string()
        }
        ("GET", "/status") => {
            let context = engine.build_context(1_000_000);
            format!(
                r#"{{"vault": {}, "insurance": {}, "total_capital": {}, "total_open_interest": {}, "current_slot": {}}}"#,
                context.vault,
                context.insurance_balance,
                context.total_capital,
                context.total_open_interest,
                context.current_slot
            )
        }
        ("GET", "/market-params") => {
            let context = engine.build_context(1_000_000);
            match agent.get_market_params(&context) {
                Ok(params) => {
                    format!(
                        r#"{{"max_leverage_bps": {}, "max_position_size": {}, "spread_bps": {}, "funding_rate_bps_per_slot": {}, "min_margin_bps": {}, "active_capital_ratio_bps": {}}}"#,
                        params.max_leverage_bps,
                        params.max_position_size,
                        params.spread_bps,
                        params.funding_rate_bps_per_slot,
                        params.min_margin_bps,
                        params.active_capital_ratio_bps
                    )
                }
                Err(e) => format!(r#"{{"error": "{:?}"}}"#, e),
            }
        }
        ("GET", "/risk") => {
            let context = engine.build_context(1_000_000);
            match agent.assess_risk(&context) {
                Ok(assessment) => {
                    format!(
                        r#"{{"risk_level_bps": {}, "reduce_exposure": {}, "hedge": {}, "increase_margin": {}}}"#,
                        assessment.risk_level_bps,
                        assessment.actions.reduce_exposure,
                        assessment.actions.hedge,
                        assessment.actions.increase_margin.map(|m| m.to_string()).unwrap_or_else(|| "null".to_string())
                    )
                }
                Err(e) => format!(r#"{{"error": "{:?}"}}"#, e),
            }
        }
        ("GET", "/anomalies") => {
            let context = engine.build_context(1_000_000);
            match agent.detect_anomalies(&context) {
                Ok(response) => {
                    format!(
                        r#"{{"anomaly_type": "{:?}", "severity_bps": {}, "freeze_market": {}, "stop_trading": {}, "initiate_shutdown": {}}}"#,
                        response.anomaly_type,
                        response.severity_bps,
                        response.actions.freeze_market,
                        response.actions.stop_trading,
                        response.actions.initiate_shutdown
                    )
                }
                Err(e) => format!(r#"{{"error": "{:?}"}}"#, e),
            }
        }
        ("POST", "/trade") => {
            // Простой парсинг JSON из тела запроса
            let body_start = request.find("\r\n\r\n").unwrap_or(0) + 4;
            let body = &request[body_start..];
            
            // Простой парсинг: ищем "size" и "oracle_price"
            let size = extract_json_value(body, "size").unwrap_or(0);
            let oracle_price = extract_json_value(body, "oracle_price").unwrap_or(1_000_000) as u64;
            let user_idx = extract_json_value(body, "user_idx").unwrap_or(0) as u16;
            
            let context = engine.build_context(oracle_price);
            let request = TradeRequest {
                user_idx,
                size,
                requested_price: None,
            };
            
            match agent.decide_trade(&context, &request) {
                Ok(decision) => {
                    match decision {
                        TradeDecision::Accept { price, size } => {
                            format!(
                                r#"{{"decision": "accept", "price": {}, "size": {}}}"#,
                                price, size
                            )
                        }
                        TradeDecision::Reject { reason } => {
                            format!(
                                r#"{{"decision": "reject", "reason": "{:?}"}}"#,
                                reason
                            )
                        }
                        TradeDecision::RequestQuote { quote_price, max_size } => {
                            format!(
                                r#"{{"decision": "quote", "quote_price": {}, "max_size": {}}}"#,
                                quote_price, max_size
                            )
                        }
                    }
                }
                Err(e) => format!(r#"{{"error": "{:?}"}}"#, e),
            }
        }
        _ => {
            format!(
                r#"{{"error": "Not found", "path": "{}", "method": "{}"}}"#,
                path, method
            )
        }
    }
}

fn extract_json_value(json: &str, key: &str) -> Option<i128> {
    let pattern = format!("\"{}\":", key);
    if let Some(start) = json.find(&pattern) {
        let value_start = start + pattern.len();
        let value_str = json[value_start..]
            .trim_start()
            .split(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .next()?;
        value_str.parse().ok()
    } else {
        None
    }
}
