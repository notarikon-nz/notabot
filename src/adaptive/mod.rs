// src/adaptive/mod.rs
//! Adaptive Performance Tuning System
//! 
//! This module provides automatic performance optimization for the NotaBot system.
//! It monitors key metrics and adjusts parameters in real-time to maintain optimal performance.

/*
    src/adaptive/
    ├── mod.rs                          # Main orchestrator
    ├── metrics.rs                      # Comprehensive metrics collection  
    ├── parameters.rs                   # Dynamic parameter management
    ├── tuning_engine.rs               # Core tuning logic + all strategies
    ├── safety.rs                      # Safety mechanisms & rollback
    ├── connection_pool_integration.rs # Your ConnectionPool integration
    ├── moderation_integration.rs      # Your ModerationSystem integration
    └── config_integration.rs          # Your ConfigurationManager integration
*/

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use serde::{Deserialize, Serialize};

pub mod metrics;
pub mod tuning_engine;
pub mod parameters;
pub mod safety;
pub mod connection_pool_integration;
pub mod moderation_integration;
pub mod config_integration;

pub use metrics::*;
pub use tuning_engine::*;
pub use parameters::*;
pub use safety::*;
pub use connection_pool_integration::*;
pub use moderation_integration::*;
pub use config_integration::*;

/// Main adaptive performance tuning system
pub struct AdaptivePerformanceSystem {
    metrics_collector: Arc<MetricsCollector>,
    tuning_engine: Arc<TuningEngine>,
    parameter_store: Arc<RwLock<ParameterStore>>,
    safety_manager: Arc<SafetyManager>,
    running: Arc<RwLock<bool>>,
    last_tuning_cycle: Arc<RwLock<chrono::DateTime<chrono::Utc>>>,
}

/// Configuration for the adaptive system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    pub enabled: bool,
    pub tuning_interval_seconds: u64,
    pub metrics_retention_hours: u64,
    pub safety_checks_enabled: bool,
    pub max_parameter_changes_per_hour: u32,
    pub rollback_threshold_seconds: u64,
    pub learning_mode: bool,
    pub strategies: StrategyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub latency_tuning: LatencyTuningConfig,
    pub memory_tuning: MemoryTuningConfig,
    pub error_rate_tuning: ErrorRateTuningConfig,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tuning_interval_seconds: 30,
            metrics_retention_hours: 24,
            safety_checks_enabled: true,
            max_parameter_changes_per_hour: 10,
            rollback_threshold_seconds: 300,
            learning_mode: false,
            strategies: StrategyConfig {
                latency_tuning: LatencyTuningConfig::default(),
                memory_tuning: MemoryTuningConfig::default(),
                error_rate_tuning: ErrorRateTuningConfig::default(),
            },
        }
    }
}

impl AdaptivePerformanceSystem {
    /// Create a new adaptive performance system
    pub fn new(config: AdaptiveConfig) -> Result<Self> {
        info!("Initializing Adaptive Performance Tuning System");
        
        let metrics_collector = Arc::new(MetricsCollector::new(
            config.metrics_retention_hours,
        )?);
        
        let parameter_store = Arc::new(RwLock::new(ParameterStore::new()));
        
        let safety_manager = Arc::new(SafetyManager::new(
            config.safety_checks_enabled,
            config.max_parameter_changes_per_hour,
            config.rollback_threshold_seconds,
        )?);
        
        let tuning_engine = Arc::new(TuningEngine::new(
            config.clone(),
            metrics_collector.clone(),
            parameter_store.clone(),
            safety_manager.clone(),
        )?);
        
        Ok(Self {
            metrics_collector,
            tuning_engine,
            parameter_store,
            safety_manager,
            running: Arc::new(RwLock::new(false)),
            last_tuning_cycle: Arc::new(RwLock::new(chrono::Utc::now())),
        })
    }
    
    /// Start the adaptive tuning system
    pub async fn start(&self, config: AdaptiveConfig) -> Result<()> {
        if !config.enabled {
            info!("Adaptive tuning system is disabled in configuration");
            return Ok(());
        }
        
        {
            let mut running = self.running.write().await;
            if *running {
                warn!("Adaptive tuning system is already running");
                return Ok(());
            }
            *running = true;
        }
        
        info!("Starting adaptive performance tuning system");
        
        // Start metrics collection
        self.metrics_collector.start().await?;
        
        // Start main tuning loop
        self.start_tuning_loop(config).await?;
        
        info!("Adaptive performance tuning system started successfully");
        Ok(())
    }
    
    /// Stop the adaptive tuning system
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping adaptive performance tuning system");
        
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        // Stop components
        self.metrics_collector.stop().await?;
        
        info!("Adaptive performance tuning system stopped");
        Ok(())
    }
    
    /// Get access to the metrics collector for external integration
    pub async fn get_metrics_collector(&self) -> Result<Arc<MetricsCollector>> {
        Ok(self.metrics_collector.clone())
    }
    
    /// Check if the system is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
    
    /// Get current performance metrics
    pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics> {
        self.metrics_collector.get_current_metrics().await
    }
    
    /// Get current parameter values
    pub async fn get_current_parameters(&self) -> Result<HashMap<String, ParameterValue>> {
        let store = self.parameter_store.read().await;
        Ok(store.get_all_parameters())
    }
    
    /// Manually trigger a tuning cycle
    pub async fn trigger_tuning_cycle(&self) -> Result<TuningResult> {
        info!("Manually triggering tuning cycle");
        
        let result = self.tuning_engine.run_tuning_cycle().await?;
        
        {
            let mut last_cycle = self.last_tuning_cycle.write().await;
            *last_cycle = chrono::Utc::now();
        }
        
        info!("Manual tuning cycle completed: {:?}", result.summary);
        Ok(result)
    }
    
    /// Record a custom performance metric
    pub async fn record_metric(&self, metric_name: &str, value: f64) -> Result<()> {
        self.metrics_collector.record_custom_metric(metric_name, value).await
    }
    
    /// Get system health status
    pub async fn get_health_status(&self) -> Result<HealthStatus> {
        let metrics = self.get_performance_metrics().await?;
        let parameters = self.get_current_parameters().await?;
        let safety_status = self.safety_manager.get_status().await?;
        
        Ok(HealthStatus {
            overall_health: calculate_overall_health(&metrics, &safety_status),
            metrics_health: metrics.system_health_score,
            safety_status,
            active_parameters: parameters.len(),
            last_tuning_cycle: *self.last_tuning_cycle.read().await,
        })
    }
    
    /// Get detailed system statistics
    pub async fn get_statistics(&self) -> Result<AdaptiveSystemStats> {
        let metrics = self.get_performance_metrics().await?;
        let parameters = self.get_current_parameters().await?;
        let tuning_history = self.tuning_engine.get_tuning_history().await?;
        let safety_stats = self.safety_manager.get_statistics().await?;
        
        Ok(AdaptiveSystemStats {
            uptime_seconds: metrics.uptime_seconds,
            total_tuning_cycles: tuning_history.len(),
            active_parameters: parameters.len(),
            metrics_collected: metrics.total_metrics_collected,
            safety_interventions: safety_stats.total_interventions,
            last_performance_improvement: tuning_history
                .last()
                .map(|h| h.performance_improvement)
                .unwrap_or(0.0),
            current_optimization_level: calculate_optimization_level(&metrics),
        })
    }
    
    /// Export configuration and state for backup
    pub async fn export_state(&self) -> Result<AdaptiveSystemState> {
        let parameters = self.get_current_parameters().await?;
        let metrics = self.get_performance_metrics().await?;
        let tuning_history = self.tuning_engine.get_tuning_history().await?;
        
        Ok(AdaptiveSystemState {
            timestamp: chrono::Utc::now(),
            parameters,
            metrics,
            tuning_history,
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
    
    /// Import previously exported state
    pub async fn import_state(&self, state: AdaptiveSystemState) -> Result<()> {
        info!("Importing adaptive system state from {}", state.timestamp);
        
        // Validate state compatibility
        if state.version != env!("CARGO_PKG_VERSION") {
            warn!("State version mismatch: {} vs {}", state.version, env!("CARGO_PKG_VERSION"));
        }
        
        // Import parameters with safety checks
        for (name, value) in state.parameters {
            if self.safety_manager.validate_parameter_change(&name, &value).await? {
                let mut store = self.parameter_store.write().await;
                store.set_parameter(&name, value)?;
            } else {
                warn!("Skipping unsafe parameter import: {}", name);
            }
        }
        
        info!("Adaptive system state imported successfully");
        Ok(())
    }
    
    /// Start the main tuning loop
    async fn start_tuning_loop(&self, config: AdaptiveConfig) -> Result<()> {
        let running = self.running.clone();
        let tuning_engine = self.tuning_engine.clone();
        let last_cycle = self.last_tuning_cycle.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(config.tuning_interval_seconds));
            let mut cycle_count = 0;
            
            info!("Adaptive tuning loop started (interval: {}s)", config.tuning_interval_seconds);
            
            loop {
                interval.tick().await;
                
                if !*running.read().await {
                    info!("Tuning loop stopping - system shutdown requested");
                    break;
                }
                
                cycle_count += 1;
                debug!("Starting tuning cycle #{}", cycle_count);
                
                match tuning_engine.run_tuning_cycle().await {
                    Ok(result) => {
                        {
                            let mut last = last_cycle.write().await;
                            *last = chrono::Utc::now();
                        }
                        
                        if !result.changes.is_empty() {
                            info!("Tuning cycle #{} completed: {} parameters adjusted", 
                                  cycle_count, result.changes.len());
                            
                            for change in &result.changes {
                                debug!("Parameter {} changed from {:?} to {:?} (reason: {})", 
                                       change.parameter_name, 
                                       change.old_value, 
                                       change.new_value, 
                                       change.reason);
                            }
                        } else {
                            debug!("Tuning cycle #{} completed: no adjustments needed", cycle_count);
                        }
                    }
                    Err(e) => {
                        error!("Tuning cycle #{} failed: {}", cycle_count, e);
                        
                        // Sleep longer on error to avoid rapid retries
                        sleep(Duration::from_secs(config.tuning_interval_seconds * 2)).await;
                    }
                }
            }
            
            info!("Adaptive tuning loop stopped after {} cycles", cycle_count);
        });
        
        Ok(())
    }
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub overall_health: f64,
    pub metrics_health: f64,
    pub safety_status: SafetyStatus,
    pub active_parameters: usize,
    pub last_tuning_cycle: chrono::DateTime<chrono::Utc>,
}

/// Detailed system statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveSystemStats {
    pub uptime_seconds: u64,
    pub total_tuning_cycles: usize,
    pub active_parameters: usize,
    pub metrics_collected: usize,
    pub safety_interventions: usize,
    pub last_performance_improvement: f64,
    pub current_optimization_level: f64,
}

/// Complete system state for backup/restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveSystemState {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parameters: HashMap<String, ParameterValue>,
    pub metrics: PerformanceMetrics,
    pub tuning_history: Vec<TuningHistoryEntry>,
    pub version: String,
}

/// Calculate overall system health
fn calculate_overall_health(metrics: &PerformanceMetrics, safety_status: &SafetyStatus) -> f64 {
    let metrics_weight = 0.6;
    let safety_weight = 0.4;
    
    let safety_health = if safety_status.is_safe { 1.0 } else { 0.5 };
    
    (metrics.system_health_score * metrics_weight) + (safety_health * safety_weight)
}

/// Calculate current optimization level
fn calculate_optimization_level(metrics: &PerformanceMetrics) -> f64 {
    // Combine multiple factors to determine optimization level
    let latency_score = if metrics.average_latency_ms < 100.0 { 1.0 } 
                       else if metrics.average_latency_ms < 500.0 { 0.8 }
                       else if metrics.average_latency_ms < 1000.0 { 0.6 }
                       else { 0.4 };
    
    let memory_score = if metrics.memory_usage_percent < 70.0 { 1.0 }
                      else if metrics.memory_usage_percent < 85.0 { 0.8 }
                      else if metrics.memory_usage_percent < 95.0 { 0.6 }
                      else { 0.4 };
    
    let error_score = if metrics.error_rate_percent < 1.0 { 1.0 }
                     else if metrics.error_rate_percent < 5.0 { 0.8 }
                     else if metrics.error_rate_percent < 10.0 { 0.6 }
                     else { 0.4 };
    
    (latency_score + memory_score + error_score) / 3.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_adaptive_system_creation() {
        let config = AdaptiveConfig::default();
        let system = AdaptivePerformanceSystem::new(config).unwrap();
        
        assert!(!system.is_running().await);
    }
    
    #[tokio::test]
    async fn test_adaptive_system_start_stop() {
        let config = AdaptiveConfig::default();
        let system = AdaptivePerformanceSystem::new(config.clone()).unwrap();
        
        system.start(config).await.unwrap();
        assert!(system.is_running().await);
        
        system.stop().await.unwrap();
        assert!(!system.is_running().await);
    }
    
    #[tokio::test]
    async fn test_health_status_calculation() {
        let metrics = PerformanceMetrics {
            system_health_score: 0.8,
            average_latency_ms: 150.0,
            memory_usage_percent: 60.0,
            error_rate_percent: 2.0,
            ..Default::default()
        };
        
        let safety_status = SafetyStatus {
            is_safe: true,
            ..Default::default()
        };
        
        let health = calculate_overall_health(&metrics, &safety_status);
        assert!(health > 0.8);
    }
    
    #[tokio::test]
    async fn test_optimization_level_calculation() {
        let metrics = PerformanceMetrics {
            average_latency_ms: 50.0,
            memory_usage_percent: 40.0,
            error_rate_percent: 0.5,
            ..Default::default()
        };
        
        let optimization_level = calculate_optimization_level(&metrics);
        assert!(optimization_level > 0.9);
    }
    
    #[tokio::test]
    async fn test_state_export_import() {
        let config = AdaptiveConfig::default();
        let system = AdaptivePerformanceSystem::new(config).unwrap();
        
        // Export state
        let state = system.export_state().await.unwrap();
        assert_eq!(state.version, env!("CARGO_PKG_VERSION"));
        
        // Import state should succeed
        system.import_state(state).await.unwrap();
    }
}