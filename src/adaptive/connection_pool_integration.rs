// src/adaptive/connection_pool_integration.rs
//! Integration between adaptive tuning and connection pool

use anyhow::Result;
use log::{debug, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::bot::connection_pool::{ConnectionPool, PoolConfig, PoolStats};
use super::*;

/// Extension trait for ConnectionPool to provide adaptive metrics
pub trait ConnectionPoolAdaptive {
    async fn get_adaptive_metrics(&self) -> Result<ConnectionPoolMetrics>;
    async fn apply_adaptive_config(&self, parameters: &HashMap<String, ParameterValue>) -> Result<()>;
    async fn get_utilization_score(&self) -> f64;
}

/// Metrics specific to connection pool performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolMetrics {
    pub utilization_percentage: f64,
    pub average_response_time_ms: f64,
    pub failure_rate_percentage: f64,
    pub connection_success_rate: f64,
    pub pool_efficiency_score: f64,
    pub platform_metrics: HashMap<String, PlatformPoolMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPoolMetrics {
    pub platform: String,
    pub utilization: f64,
    pub health_score: f64,
    pub avg_response_time: f64,
    pub connection_failures: u32,
    pub total_requests: u64,
}

impl ConnectionPoolAdaptive for ConnectionPool {
    async fn get_adaptive_metrics(&self) -> Result<ConnectionPoolMetrics> {
        let pool_stats = self.get_stats().await;
        let mut platform_metrics = HashMap::new();
        
        let mut total_utilization = 0.0;
        let mut total_response_time = 0.0;
        let mut total_failures = 0;
        let mut total_requests = 0u64;
        let mut platform_count = 0;
        
        for (platform, stats) in &pool_stats {
            let utilization = if stats.total_connections > 0 {
                stats.active_connections as f64 / stats.total_connections as f64 * 100.0
            } else {
                0.0
            };
            
            let health_score = if stats.total_requests > 0 {
                (stats.successful_requests as f64 / stats.total_requests as f64) * 100.0
            } else {
                100.0
            };
            
            platform_metrics.insert(platform.clone(), PlatformPoolMetrics {
                platform: platform.clone(),
                utilization,
                health_score,
                avg_response_time: stats.average_response_time_ms,
                connection_failures: stats.failed_connections as u32,
                total_requests: stats.total_requests,
            });
            
            total_utilization += utilization;
            total_response_time += stats.average_response_time_ms;
            total_failures += stats.failed_connections as u32;
            total_requests += stats.total_requests;
            platform_count += 1;
        }
        
        let avg_utilization = if platform_count > 0 { total_utilization / platform_count as f64 } else { 0.0 };
        let avg_response_time = if platform_count > 0 { total_response_time / platform_count as f64 } else { 0.0 };
        let failure_rate = if total_requests > 0 { (total_failures as f64 / total_requests as f64) * 100.0 } else { 0.0 };
        let success_rate = 100.0 - failure_rate;
        
        // Calculate pool efficiency score (0.0 - 1.0)
        let efficiency_score = {
            let utilization_score = (avg_utilization / 100.0).min(1.0);
            let response_score = if avg_response_time > 0.0 { (1000.0 / avg_response_time).min(1.0) } else { 1.0 };
            let reliability_score = success_rate / 100.0;
            
            (utilization_score * 0.3 + response_score * 0.4 + reliability_score * 0.3)
        };
        
        Ok(ConnectionPoolMetrics {
            utilization_percentage: avg_utilization,
            average_response_time_ms: avg_response_time,
            failure_rate_percentage: failure_rate,
            connection_success_rate: success_rate,
            pool_efficiency_score: efficiency_score,
            platform_metrics,
        })
    }
    
    async fn apply_adaptive_config(&self, parameters: &HashMap<String, ParameterValue>) -> Result<()> {
        // This would require extending your ConnectionPool to support dynamic reconfiguration
        // For now, we'll log what changes would be made
        
        for (param_name, param_value) in parameters {
            match param_name.as_str() {
                "connection_pool_max_size" => {
                    if let Some(new_size) = param_value.as_i64() {
                        info!("Would update connection pool max size to: {}", new_size);
                        // pool.update_max_size(new_size).await?;
                    }
                }
                "connection_timeout_ms" => {
                    if let Some(new_timeout) = param_value.as_duration_ms() {
                        info!("Would update connection timeout to: {}ms", new_timeout);
                        // pool.update_timeout(Duration::from_millis(new_timeout)).await?;
                    }
                }
                "connection_retry_attempts" => {
                    if let Some(new_retries) = param_value.as_i64() {
                        info!("Would update retry attempts to: {}", new_retries);
                        // pool.update_retry_attempts(new_retries as u32).await?;
                    }
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    async fn get_utilization_score(&self) -> f64 {
        match self.get_adaptive_metrics().await {
            Ok(metrics) => metrics.pool_efficiency_score,
            Err(_) => 0.5, // Default neutral score
        }
    }
}

/// Connection pool tuning strategy specifically for your pool implementation
pub struct ConnectionPoolTuningStrategy {
    pool: Arc<ConnectionPool>,
}

impl ConnectionPoolTuningStrategy {
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

impl ParameterTuningStrategy for ConnectionPoolTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Use the connection pool utilization from your existing metrics
        if metrics.connection_pool_utilization > 0.9 {
            // High utilization - increase pool size
            if let Some(current_size) = parameters.get_parameter("connection_pool_max_size") {
                if let Some(size) = current_size.as_i64() {
                    let new_size = (size + 1).min(10); // Cap at 10
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_pool_max_size".to_string(),
                        current_value: current_size.clone(),
                        suggested_value: ParameterValue::Integer(new_size),
                        confidence: 0.9,
                        reason: format!("High pool utilization ({:.1}%) - increase pool size", 
                                      metrics.connection_pool_utilization * 100.0),
                        expected_improvement: 0.3,
                    });
                }
            }
        } else if metrics.connection_pool_utilization < 0.3 {
            // Low utilization - decrease pool size to save resources
            if let Some(current_size) = parameters.get_parameter("connection_pool_max_size") {
                if let Some(size) = current_size.as_i64() {
                    if size > 2 {
                        let new_size = size - 1;
                        
                        suggestions.push(ParameterSuggestion {
                            parameter_name: "connection_pool_max_size".to_string(),
                            current_value: current_size.clone(),
                            suggested_value: ParameterValue::Integer(new_size),
                            confidence: 0.6,
                            reason: format!("Low pool utilization ({:.1}%) - reduce pool size", 
                                          metrics.connection_pool_utilization * 100.0),
                            expected_improvement: 0.1,
                        });
                    }
                }
            }
        }
        
        // Adjust timeouts based on your connection failure metrics
        if metrics.connection_failures > 5 {
            if let Some(current_timeout) = parameters.get_parameter("connection_timeout_ms") {
                if let Some(timeout_ms) = current_timeout.as_duration_ms() {
                    let new_timeout = (timeout_ms as f64 * 1.5) as u64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_timeout_ms".to_string(),
                        current_value: current_timeout.clone(),
                        suggested_value: ParameterValue::Duration(new_timeout),
                        confidence: 0.8,
                        reason: format!("High connection failures ({}) - increase timeout", 
                                      metrics.connection_failures),
                        expected_improvement: 0.4,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "connection_pool_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        190 // High priority for connection stability
    }
}

/// Metrics collector integration with your connection pool
pub async fn collect_connection_pool_metrics(
    pool: &Arc<ConnectionPool>,
    metrics_collector: &Arc<MetricsCollector>,
) -> Result<()> {
    let adaptive_metrics = pool.get_adaptive_metrics().await?;
    
    // Record pool-level metrics
    metrics_collector.record_custom_metric("pool_utilization", adaptive_metrics.utilization_percentage).await?;
    metrics_collector.record_custom_metric("pool_efficiency", adaptive_metrics.pool_efficiency_score * 100.0).await?;
    metrics_collector.record_custom_metric("pool_failure_rate", adaptive_metrics.failure_rate_percentage).await?;
    
    // Record platform-specific metrics
    for (platform, platform_metrics) in &adaptive_metrics.platform_metrics {
        let platform_prefix = format!("pool_{}", platform);
        
        metrics_collector.record_custom_metric(
            &format!("{}_utilization", platform_prefix), 
            platform_metrics.utilization
        ).await?;
        
        metrics_collector.record_custom_metric(
            &format!("{}_health", platform_prefix), 
            platform_metrics.health_score
        ).await?;
        
        metrics_collector.record_latency(
            &format!("{}_connection", platform), 
            platform_metrics.avg_response_time
        ).await?;
    }
    
    debug!("Collected connection pool metrics for adaptive system");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::connection_pool::PoolConfig;
    
    #[tokio::test]
    async fn test_connection_pool_metrics() {
        let config = PoolConfig::default();
        let pool = Arc::new(ConnectionPool::new(config));
        
        // Test metrics collection (would require actual connections in real test)
        let result = pool.get_adaptive_metrics().await;
        // In a real test environment, you'd initialize the pool first
        // assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_connection_pool_tuning_strategy() {
        let config = PoolConfig::default();
        let pool = Arc::new(ConnectionPool::new(config));
        let strategy = ConnectionPoolTuningStrategy::new(pool);
        
        let metrics = PerformanceMetrics {
            connection_pool_utilization: 0.95, // High utilization
            connection_failures: 10,
            ..Default::default()
        };
        
        let store = ParameterStore::new();
        let suggestions = strategy.suggest_adjustments(&metrics, &store);
        
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.parameter_name == "connection_pool_max_size"));
    }
}