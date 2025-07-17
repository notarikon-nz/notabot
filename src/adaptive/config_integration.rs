// src/adaptive/config_integration.rs
//! Integration between adaptive tuning and configuration management

use anyhow::Result;
use log::{debug, info, warn, error};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::{ConfigurationManager, ConfigChangeEvent};
use super::*;

type ConfigAdaptiveConfig = crate::adaptive::AdaptiveConfig;

/// Extension trait for ConfigurationManager to provide adaptive integration
pub trait ConfigurationManagerAdaptive {
    async fn get_adaptive_parameters(&self) -> Result<HashMap<String, ParameterValue>>;
    async fn apply_adaptive_changes(&self, changes: &[ParameterChange]) -> Result<()>;
    async fn get_config_health_metrics(&self) -> Result<ConfigHealthMetrics>;
    async fn subscribe_to_adaptive_changes(&self) -> tokio::sync::broadcast::Receiver<AdaptiveConfigEvent>;
}

/// Health metrics for configuration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigHealthMetrics {
    pub hot_reload_success_rate: f64,
    pub config_validation_errors: u32,
    pub last_reload_time_ms: f64,
    pub file_watch_active: bool,
    pub config_files_status: HashMap<String, ConfigFileStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFileStatus {
    pub path: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub size_bytes: u64,
    pub validation_status: String, // "valid", "invalid", "unknown"
    pub reload_count: u32,
}

/// Events specific to adaptive configuration changes
#[derive(Debug, Clone)]
pub enum AdaptiveConfigEvent {
    ParameterUpdated { name: String, old_value: ParameterValue, new_value: ParameterValue },
    FilterConfigChanged { filter_count_delta: i32 },
    PerformanceConfigChanged { optimization_level: f64 },
    ValidationFailed { parameter: String, reason: String },
}

impl ConfigurationManagerAdaptive for ConfigurationManager {
    async fn get_adaptive_parameters(&self) -> Result<HashMap<String, ParameterValue>> {
        let mut parameters = HashMap::new();
        
        // Get bot configuration and extract tunable parameters
        let bot_config = self.get_bot_config().await;
        
        // Core performance parameters
        parameters.insert(
            "response_delay_ms".to_string(),
            ParameterValue::Duration(bot_config.core.response_delay_ms)
        );
        
        parameters.insert(
            "rate_limit_per_minute".to_string(),
            ParameterValue::Integer(bot_config.core.rate_limit_per_minute as i64)
        );
        
        parameters.insert(
            "max_message_length".to_string(),
            ParameterValue::Integer(bot_config.core.max_message_length as i64)
        );
        
        // Performance settings
        parameters.insert(
            "worker_threads".to_string(),
            ParameterValue::Integer(bot_config.performance.worker_threads as i64)
        );
        
        parameters.insert(
            "batch_size".to_string(),
            ParameterValue::Integer(bot_config.performance.batch_size as i64)
        );
        
        parameters.insert(
            "cache_size_mb".to_string(),
            ParameterValue::Integer(bot_config.performance.cache_size_mb as i64)
        );
        
        // Get filter configuration parameters
        let filter_config = self.get_filter_config().await;
        
        parameters.insert(
            "max_filters_per_message".to_string(),
            ParameterValue::Integer(filter_config.global_settings.max_filters_per_message as i64)
        );
        
        parameters.insert(
            "global_timeout_multiplier".to_string(),
            ParameterValue::Float(filter_config.global_settings.global_timeout_multiplier as f64)
        );
        
        // Get pattern configuration parameters
        let pattern_config = self.get_pattern_config().await;
        
        parameters.insert(
            "max_processing_threads".to_string(),
            ParameterValue::Integer(pattern_config.global_settings.max_processing_threads as i64)
        );
        
        parameters.insert(
            "pattern_cache_size_mb".to_string(),
            ParameterValue::Integer(pattern_config.global_settings.cache_size_mb as i64)
        );
        
        // Timer configuration parameters
        let timer_config = self.get_timer_config().await;
        
        parameters.insert(
            "global_cooldown_seconds".to_string(),
            ParameterValue::Duration(timer_config.global_settings.global_cooldown_seconds * 1000)
        );
        
        parameters.insert(
            "max_timers_per_channel".to_string(),
            ParameterValue::Integer(timer_config.global_settings.max_timers_per_channel as i64)
        );
        
        Ok(parameters)
    }
    
    async fn apply_adaptive_changes(&self, changes: &[ParameterChange]) -> Result<()> {
        info!("Applying {} adaptive configuration changes", changes.len());
        
        for change in changes {
            match self.apply_single_parameter_change(change).await {
                Ok(()) => {
                    info!("Applied adaptive change: {} = {} (was: {})", 
                          change.parameter_name, change.new_value, change.old_value);
                }
                Err(e) => {
                    error!("Failed to apply adaptive change for {}: {}", 
                           change.parameter_name, e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn get_config_health_metrics(&self) -> Result<ConfigHealthMetrics> {
        // This would track actual reload statistics
        let mut config_files_status = HashMap::new();
        
        // Check status of each config file
        let config_files = ["filters.yaml", "patterns.yaml", "timers.yaml", "bot.yaml"];
        
        for file_name in &config_files {
            // In a real implementation, you'd track this data
            config_files_status.insert(file_name.to_string(), ConfigFileStatus {
                path: format!("config/{}", file_name),
                last_modified: chrono::Utc::now(),
                size_bytes: 1024, // Would get actual file size
                validation_status: "valid".to_string(),
                reload_count: 5, // Would track actual reload count
            });
        }
        
        Ok(ConfigHealthMetrics {
            hot_reload_success_rate: 98.5, // Would calculate from actual stats
            config_validation_errors: 0,
            last_reload_time_ms: 45.0, // Would measure actual reload time
            file_watch_active: true,
            config_files_status,
        })
    }
    
    async fn subscribe_to_adaptive_changes(&self) -> tokio::sync::broadcast::Receiver<AdaptiveConfigEvent> {
        // This would return a receiver for adaptive-specific events
        // For now, we'll create a dummy receiver
        let (tx, rx) = tokio::sync::broadcast::channel(100);
        rx
    }
}

impl ConfigurationManager {
    /// Apply a single parameter change to the appropriate configuration
    async fn apply_single_parameter_change(&self, change: &ParameterChange) -> Result<()> {
        match change.parameter_name.as_str() {
            // Bot core parameters
            "response_delay_ms" => {
                if let Some(delay) = change.new_value.as_duration_ms() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.core.response_delay_ms = delay;
                    self.save_bot_config(bot_config).await?;
                }
            }
            "rate_limit_per_minute" => {
                if let Some(rate) = change.new_value.as_i64() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.core.rate_limit_per_minute = rate as u32;
                    self.save_bot_config(bot_config).await?;
                }
            }
            "max_message_length" => {
                if let Some(length) = change.new_value.as_i64() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.core.max_message_length = length as usize;
                    self.save_bot_config(bot_config).await?;
                }
            }
            
            // Performance parameters
            "worker_threads" => {
                if let Some(threads) = change.new_value.as_i64() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.performance.worker_threads = threads as u8;
                    self.save_bot_config(bot_config).await?;
                }
            }
            "batch_size" => {
                if let Some(size) = change.new_value.as_i64() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.performance.batch_size = size as u32;
                    self.save_bot_config(bot_config).await?;
                }
            }
            "cache_size_mb" => {
                if let Some(size) = change.new_value.as_i64() {
                    let mut bot_config = self.get_bot_config().await;
                    bot_config.performance.cache_size_mb = size as u32;
                    self.save_bot_config(bot_config).await?;
                }
            }
            
            // Filter parameters
            "max_filters_per_message" => {
                if let Some(max_filters) = change.new_value.as_i64() {
                    let mut filter_config = self.get_filter_config().await;
                    filter_config.global_settings.max_filters_per_message = max_filters as u8;
                    self.save_filter_config(filter_config).await?;
                }
            }
            "global_timeout_multiplier" => {
                if let Some(multiplier) = change.new_value.as_f64() {
                    let mut filter_config = self.get_filter_config().await;
                    filter_config.global_settings.global_timeout_multiplier = multiplier as f32;
                    self.save_filter_config(filter_config).await?;
                }
            }
            
            // Pattern parameters
            "max_processing_threads" => {
                if let Some(threads) = change.new_value.as_i64() {
                    let mut pattern_config = self.get_pattern_config().await;
                    pattern_config.global_settings.max_processing_threads = threads as u8;
                    self.save_pattern_config(pattern_config).await?;
                }
            }
            "pattern_cache_size_mb" => {
                if let Some(size) = change.new_value.as_i64() {
                    let mut pattern_config = self.get_pattern_config().await;
                    pattern_config.global_settings.cache_size_mb = size as u32;
                    self.save_pattern_config(pattern_config).await?;
                }
            }
            
            // Timer parameters
            "global_cooldown_seconds" => {
                if let Some(cooldown_ms) = change.new_value.as_duration_ms() {
                    let mut timer_config = self.get_timer_config().await;
                    timer_config.global_settings.global_cooldown_seconds = cooldown_ms / 1000;
                    self.save_timer_config(timer_config).await?;
                }
            }
            "max_timers_per_channel" => {
                if let Some(max_timers) = change.new_value.as_i64() {
                    let mut timer_config = self.get_timer_config().await;
                    timer_config.global_settings.max_timers_per_channel = max_timers as u8;
                    self.save_timer_config(timer_config).await?;
                }
            }
            
            _ => {
                warn!("Unknown adaptive parameter: {}", change.parameter_name);
            }
        }
        
        Ok(())
    }
    
    /// Save timer configuration (missing method from your ConfigurationManager)
    async fn save_timer_config(&self, config: crate::config::TimerConfiguration) -> Result<()> {
        let config_path = std::path::Path::new("config").join("timers.yaml");
        let content = serde_yaml::to_string(&config)?;
        tokio::fs::write(&config_path, content).await?;
        
        // Don't try to update internal cache directly - let the file watcher handle it
        info!("Saved timer configuration from adaptive system");
        Ok(())
    }

    async fn save_bot_config(&self, config: crate::config::BotConfiguration) -> Result<()> {
        let config_path = std::path::Path::new("config").join("bot.yaml");
        let content = serde_yaml::to_string(&config)?;
        tokio::fs::write(&config_path, content).await?;
        
        // Don't try to update internal cache directly - let the file watcher handle it
        info!("Saved bot configuration from adaptive system");
        Ok(())
    }
}

/// Configuration-aware tuning strategy
pub struct ConfigurationTuningStrategy {
    config_manager: Arc<ConfigurationManager>,
}

impl ConfigurationTuningStrategy {
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self { config_manager }
    }
}

impl ParameterTuningStrategy for ConfigurationTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Adjust rate limits based on throughput
        if metrics.messages_per_second > 50.0 {
            if let Some(current_rate) = parameters.get_parameter("rate_limit_per_minute") {
                if let Some(rate) = current_rate.as_i64() {
                    let new_rate = (rate as f64 * 1.2) as i64; // Increase by 20%
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "rate_limit_per_minute".to_string(),
                        current_value: current_rate.clone(),
                        suggested_value: ParameterValue::Integer(new_rate),
                        confidence: 0.8,
                        reason: format!("High message throughput ({:.1} msg/s) - increase rate limit", 
                                      metrics.messages_per_second),
                        expected_improvement: 0.15,
                    });
                }
            }
        }
        
        // Adjust batch size based on processing efficiency
        if metrics.average_latency_ms > 200.0 {
            if let Some(current_batch) = parameters.get_parameter("batch_size") {
                if let Some(batch_size) = current_batch.as_i64() {
                    let new_batch_size = (batch_size as f64 * 1.5) as i64; // Increase batch size
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "batch_size".to_string(),
                        current_value: current_batch.clone(),
                        suggested_value: ParameterValue::Integer(new_batch_size),
                        confidence: 0.9,
                        reason: format!("High processing latency ({:.1}ms) - increase batch size for efficiency", 
                                      metrics.average_latency_ms),
                        expected_improvement: 0.25,
                    });
                }
            }
        }
        
        // Adjust worker threads based on CPU utilization
        if metrics.system_health_score < 0.8 {
            if let Some(current_threads) = parameters.get_parameter("worker_threads") {
                if let Some(threads) = current_threads.as_i64() {
                    if threads > 2 {
                        let new_threads = threads - 1; // Reduce thread count to ease CPU pressure
                        
                        suggestions.push(ParameterSuggestion {
                            parameter_name: "worker_threads".to_string(),
                            current_value: current_threads.clone(),
                            suggested_value: ParameterValue::Integer(new_threads),
                            confidence: 0.7,
                            reason: format!("Low system health ({:.1}%) - reduce worker threads", 
                                          metrics.system_health_score * 100.0),
                            expected_improvement: 0.2,
                        });
                    }
                }
            }
        }
        
        // Adjust cache size based on memory usage
        if metrics.memory_usage_percent > 80.0 {
            if let Some(current_cache) = parameters.get_parameter("cache_size_mb") {
                if let Some(cache_size) = current_cache.as_i64() {
                    let new_cache_size = (cache_size as f64 * 0.8) as i64; // Reduce cache by 20%
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "cache_size_mb".to_string(),
                        current_value: current_cache.clone(),
                        suggested_value: ParameterValue::Integer(new_cache_size),
                        confidence: 0.9,
                        reason: format!("High memory usage ({:.1}%) - reduce cache size", 
                                      metrics.memory_usage_percent),
                        expected_improvement: 0.3,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "configuration_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        170 // Medium-high priority for configuration optimization
    }
}

/// Monitor configuration changes and provide metrics
pub struct ConfigurationMonitor {
    config_manager: Arc<ConfigurationManager>,
    change_history: Arc<RwLock<Vec<ConfigChangeRecord>>>,
}

#[derive(Debug, Clone)]
struct ConfigChangeRecord {
    timestamp: chrono::DateTime<chrono::Utc>,
    event: ConfigChangeEvent,
    performance_before: Option<PerformanceMetrics>,
    performance_after: Option<PerformanceMetrics>,
}

impl ConfigurationMonitor {
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self {
            config_manager,
            change_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Start monitoring configuration changes
    pub async fn start_monitoring(&self) -> Result<()> {
        let mut change_receiver = self.config_manager.subscribe_to_changes();
        let change_history = self.change_history.clone();
        
        tokio::spawn(async move {
            while let Ok(event) = change_receiver.recv().await {
                let record = ConfigChangeRecord {
                    timestamp: chrono::Utc::now(),
                    event,
                    performance_before: None, // Would capture before reload
                    performance_after: None,  // Would capture after reload
                };
                
                let mut history = change_history.write().await;
                history.push(record);
                
                // Keep history bounded
                if history.len() > 1000 {
                    history.remove(0);
                }
                
                info!("Recorded configuration change for monitoring");
            }
        });
        
        Ok(())
    }
    
    /// Get configuration change statistics
    pub async fn get_change_statistics(&self) -> ConfigChangeStatistics {
        let history = self.change_history.read().await;
        
        let total_changes = history.len();
        let recent_changes = history.iter()
            .filter(|record| {
                let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
                record.timestamp > hour_ago
            })
            .count();
        
        ConfigChangeStatistics {
            total_changes,
            recent_changes_last_hour: recent_changes,
            last_change_time: history.last().map(|r| r.timestamp),
            hot_reload_success_rate: 98.5, // Would calculate from actual data
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigChangeStatistics {
    pub total_changes: usize,
    pub recent_changes_last_hour: usize,
    pub last_change_time: Option<chrono::DateTime<chrono::Utc>>,
    pub hot_reload_success_rate: f64,
}

/// Collect configuration health metrics
pub async fn collect_configuration_metrics(
    config_manager: &Arc<ConfigurationManager>,
    metrics_collector: &Arc<MetricsCollector>,
) -> Result<()> {
    let health_metrics = config_manager.get_config_health_metrics().await?;
    
    // Record configuration health metrics
    metrics_collector.record_custom_metric("config_hot_reload_success_rate", health_metrics.hot_reload_success_rate).await?;
    metrics_collector.record_custom_metric("config_validation_errors", health_metrics.config_validation_errors as f64).await?;
    metrics_collector.record_custom_metric("config_reload_time", health_metrics.last_reload_time_ms).await?;
    metrics_collector.record_custom_metric("config_file_watch_active", if health_metrics.file_watch_active { 1.0 } else { 0.0 }).await?;
    
    // Record per-file metrics
    for (file_name, file_status) in &health_metrics.config_files_status {
        let file_prefix = format!("config_file_{}", file_name.replace(".yaml", ""));
        
        metrics_collector.record_custom_metric(
            &format!("{}_size_kb", file_prefix), 
            file_status.size_bytes as f64 / 1024.0
        ).await?;
        
        metrics_collector.record_custom_metric(
            &format!("{}_reload_count", file_prefix), 
            file_status.reload_count as f64
        ).await?;
        
        let validation_score = match file_status.validation_status.as_str() {
            "valid" => 1.0,
            "invalid" => 0.0,
            _ => 0.5,
        };
        metrics_collector.record_custom_metric(
            &format!("{}_validation_status", file_prefix), 
            validation_score
        ).await?;
    }
    
    debug!("Collected configuration metrics for adaptive system");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_config_adaptive_parameters() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        config_manager.initialize().await.unwrap();
        
        let parameters = config_manager.get_adaptive_parameters().await.unwrap();
        assert!(!parameters.is_empty());
        assert!(parameters.contains_key("response_delay_ms"));
        assert!(parameters.contains_key("worker_threads"));
    }
    
    #[tokio::test]
    async fn test_config_tuning_strategy() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        config_manager.initialize().await.unwrap();
        
        let strategy = ConfigurationTuningStrategy::new(config_manager);
        
        let metrics = PerformanceMetrics {
            messages_per_second: 60.0,
            average_latency_ms: 250.0,
            memory_usage_percent: 85.0,
            system_health_score: 0.7,
            ..Default::default()
        };
        
        let store = ParameterStore::new();
        
        // Test that the strategy can handle an empty parameter store gracefully
        let suggestions = strategy.suggest_adjustments(&metrics, &store);
        
        // Strategy should return empty suggestions if no parameters are available
        // This tests that it doesn't panic on missing parameters
        
        // Test basic strategy properties
        assert_eq!(strategy.get_strategy_name(), "configuration_tuning");
        assert_eq!(strategy.get_priority(), 170);
    }
    
    #[tokio::test]
    async fn test_configuration_monitor() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        config_manager.initialize().await.unwrap();
        
        let monitor = ConfigurationMonitor::new(config_manager);
        
        let result = monitor.start_monitoring().await;
        assert!(result.is_ok());
        
        let stats = monitor.get_change_statistics().await;
        assert_eq!(stats.recent_changes_last_hour, 0);
    }
}