// src/adaptive/tuning_engine.rs
//! Core tuning engine that orchestrates parameter adjustments

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use super::*;

/// Main tuning engine that coordinates parameter adjustments
pub struct TuningEngine {
    config: AdaptiveConfig,
    metrics_collector: Arc<MetricsCollector>,
    parameter_store: Arc<RwLock<ParameterStore>>,
    safety_manager: Arc<SafetyManager>,
    strategies: Vec<Box<dyn ParameterTuningStrategy + Send + Sync>>,
    tuning_history: Arc<RwLock<Vec<TuningHistoryEntry>>>,
    last_tuning_run: Arc<RwLock<Option<std::time::Instant>>>,
}

/// Result of a tuning cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningResult {
    pub timestamp: u64,
    pub duration_ms: u64,
    pub changes: Vec<ParameterChange>,
    pub rejected_changes: Vec<RejectedChange>,
    pub performance_improvement: f64,
    pub summary: TuningSummary,
}

/// Summary of tuning results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningSummary {
    pub total_parameters_evaluated: usize,
    pub parameters_changed: usize,
    pub changes_rejected_by_safety: usize,
    pub overall_improvement_score: f64,
    pub dominant_strategy: String,
}

/// Record of a rejected parameter change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedChange {
    pub parameter_name: String,
    pub suggested_value: ParameterValue,
    pub rejection_reason: String,
    pub suggested_by: String,
}

/// Historical record of tuning cycles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningHistoryEntry {
    pub timestamp: u64,
    pub performance_before: PerformanceMetrics,
    pub performance_after: PerformanceMetrics,
    pub changes_applied: Vec<ParameterChange>,
    pub performance_improvement: f64,
    pub strategy_effectiveness: HashMap<String, f64>,
}

impl TuningEngine {
    pub fn new(
        config: AdaptiveConfig,
        metrics_collector: Arc<MetricsCollector>,
        parameter_store: Arc<RwLock<ParameterStore>>,
        safety_manager: Arc<SafetyManager>,
    ) -> Result<Self> {
        let mut engine = Self {
            config,
            metrics_collector,
            parameter_store,
            safety_manager,
            strategies: Vec::new(),
            tuning_history: Arc::new(RwLock::new(Vec::new())),
            last_tuning_run: Arc::new(RwLock::new(None)),
        };
        
        // Initialize tuning strategies
        engine.initialize_strategies()?;
        
        Ok(engine)
    }
    
    /// Run a complete tuning cycle
    pub async fn run_tuning_cycle(&self) -> Result<TuningResult> {
        let start_time = std::time::Instant::now();
        let timestamp = chrono::Utc::now().timestamp() as u64;
        
        debug!("Starting tuning cycle");
        
        // Update last run time
        {
            let mut last_run = self.last_tuning_run.write().await;
            *last_run = Some(start_time);
        }
        
        // Get current metrics and performance baseline
        let current_metrics = self.metrics_collector.get_current_metrics().await?;
        let performance_before = current_metrics.clone();
        
        // Collect suggestions from all strategies
        let suggestions = self.collect_strategy_suggestions(&current_metrics).await?;
        
        debug!("Collected {} parameter suggestions from {} strategies", 
               suggestions.len(), self.strategies.len());
        
        // Filter and prioritize suggestions
        let prioritized_suggestions = self.prioritize_suggestions(suggestions.clone()).await?;
        
        // Apply changes with safety checks
        let suggestions_for_apply = prioritized_suggestions.clone();
        let (applied_changes, rejected_changes) = self.apply_changes_safely(suggestions_for_apply).await?;
        
        // Wait a moment for changes to take effect
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        
        // Measure performance improvement
        let performance_after = self.metrics_collector.get_current_metrics().await?;
        let performance_improvement = self.calculate_performance_improvement(&performance_before, &performance_after);
        
        // Record tuning history
        self.record_tuning_history(performance_before, performance_after, applied_changes.clone(), performance_improvement).await?;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        let summary = TuningSummary {
            total_parameters_evaluated: prioritized_suggestions.len(),
            parameters_changed: applied_changes.len(),
            changes_rejected_by_safety: rejected_changes.len(),
            overall_improvement_score: performance_improvement,
            dominant_strategy: self.find_dominant_strategy(&applied_changes),
        };
        
        let result = TuningResult {
            timestamp,
            duration_ms,
            changes: applied_changes,
            rejected_changes,
            performance_improvement,
            summary,
        };
        
        info!("Tuning cycle completed in {}ms: {} changes applied, {:.2}% improvement", 
              duration_ms, result.changes.len(), performance_improvement * 100.0);
        
        Ok(result)
    }
    
    /// Get tuning history
    pub async fn get_tuning_history(&self) -> Result<Vec<TuningHistoryEntry>> {
        let history = self.tuning_history.read().await;
        Ok(history.clone())
    }
    
    /// Force a specific parameter change (bypassing normal tuning)
    pub async fn force_parameter_change(&self, name: &str, value: ParameterValue, reason: &str) -> Result<()> {
        let mut store = self.parameter_store.write().await;
        
        // Check safety constraints
        if !self.safety_manager.validate_parameter_change(name, &value).await? {
            return Err(anyhow::anyhow!("Safety manager rejected forced parameter change"));
        }
        
        let old_value = store.set_parameter(name, value)?;
        
        info!("Forced parameter change: {} = {} (was: {}, reason: {})", 
              name, store.get_parameter(name).unwrap(), old_value, reason);
        
        Ok(())
    }
    
    /// Get effectiveness of each tuning strategy
    pub async fn get_strategy_effectiveness(&self) -> Result<HashMap<String, f64>> {
        let history = self.tuning_history.read().await;
        
        let mut strategy_scores = HashMap::new();
        let mut strategy_counts = HashMap::new();
        
        for entry in history.iter() {
            for (strategy, effectiveness) in &entry.strategy_effectiveness {
                let total_score = strategy_scores.entry(strategy.clone()).or_insert(0.0);
                let count = strategy_counts.entry(strategy.clone()).or_insert(0);
                
                *total_score += effectiveness;
                *count += 1;
            }
        }
        
        // Calculate average effectiveness
        let mut avg_effectiveness = HashMap::new();
        for (strategy, total_score) in strategy_scores {
            if let Some(count) = strategy_counts.get(&strategy) {
                avg_effectiveness.insert(strategy, total_score / *count as f64);
            }
        }
        
        Ok(avg_effectiveness)
    }
    
    /// Initialize all tuning strategies
    fn initialize_strategies(&mut self) -> Result<()> {
        info!("Initializing tuning strategies");
        
        // Add latency-based tuning strategy
        self.strategies.push(Box::new(LatencyTuningStrategy::new(
            self.config.strategies.latency_tuning.clone()
        )?));
        
        // Add memory-based tuning strategy  
        self.strategies.push(Box::new(MemoryTuningStrategy::new(
            self.config.strategies.memory_tuning.clone()
        )?));
        
        // Add error rate-based tuning strategy
        self.strategies.push(Box::new(ErrorRateTuningStrategy::new(
            self.config.strategies.error_rate_tuning.clone()
        )?));
        
        // Add load balancing strategy
        self.strategies.push(Box::new(LoadBalancingStrategy::new()?));
        
        // Add adaptive AI strategy
        self.strategies.push(Box::new(AdaptiveAIStrategy::new()?));
        
        info!("Initialized {} tuning strategies", self.strategies.len());
        Ok(())
    }
    
    /// Collect suggestions from all strategies
    async fn collect_strategy_suggestions(&self, metrics: &PerformanceMetrics) -> Result<Vec<ParameterSuggestion>> {
        let store = self.parameter_store.read().await;
        let mut all_suggestions = Vec::new();
        
        for strategy in &self.strategies {
            let suggestions = strategy.suggest_adjustments(metrics, &store);
            
            debug!("Strategy '{}' suggested {} parameter adjustments", 
                strategy.get_strategy_name(), suggestions.len());
            
            // Clone suggestions for the debug loop to avoid moving the original
            for suggestion in &suggestions {  // <- CHANGE: Add & here to borrow instead of move
                debug!("  {} -> {} (confidence: {:.2}, improvement: {:.2}%)", 
                    suggestion.parameter_name,
                    suggestion.suggested_value,
                    suggestion.confidence,
                    suggestion.expected_improvement * 100.0);
            }
            
            all_suggestions.extend(suggestions);  // <- Now this works because suggestions wasn't moved
        }
        
        Ok(all_suggestions)
    }
    
    /// Prioritize and filter suggestions
    async fn prioritize_suggestions(&self, suggestions: Vec<ParameterSuggestion>) -> Result<Vec<ParameterSuggestion>> {
        let store = self.parameter_store.read().await;
        let mut filtered_suggestions = Vec::new();
        
        // Group suggestions by parameter name
        let mut suggestions_by_param: HashMap<String, Vec<ParameterSuggestion>> = HashMap::new();
        for suggestion in suggestions {
            suggestions_by_param
                .entry(suggestion.parameter_name.clone())
                .or_insert_with(Vec::new)
                .push(suggestion);
        }
        
        // For each parameter, select the best suggestion
        for (param_name, param_suggestions) in suggestions_by_param {
            // Check if parameter can be tuned
            if !store.can_tune_parameter(&param_name)? {
                debug!("Skipping parameter '{}' - tuning frequency restrictions", param_name);
                continue;
            }
            
            // Find suggestion with highest confidence * expected_improvement score
            let best_suggestion = param_suggestions.into_iter()
                .max_by(|a, b| {
                    let score_a = a.confidence * a.expected_improvement;
                    let score_b = b.confidence * b.expected_improvement;
                    score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
                });
            
            if let Some(suggestion) = best_suggestion {
                // Only include suggestions with decent confidence and improvement
                if suggestion.confidence > 0.3 && suggestion.expected_improvement > 0.01 {
                    filtered_suggestions.push(suggestion);
                }
            }
        }
        
        // Sort by priority (confidence * improvement * impact)
        filtered_suggestions.sort_by(|a, b| {
            let priority_a = a.confidence * a.expected_improvement;
            let priority_b = b.confidence * b.expected_improvement;
            priority_b.partial_cmp(&priority_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Limit to reasonable number of changes per cycle
        filtered_suggestions.truncate(5);
        
        Ok(filtered_suggestions)
    }
    
    /// Apply parameter changes with safety checks
    async fn apply_changes_safely(&self, suggestions: Vec<ParameterSuggestion>) -> Result<(Vec<ParameterChange>, Vec<RejectedChange>)> {
        let mut applied_changes = Vec::new();
        let mut rejected_changes = Vec::new();

        for suggestion in suggestions {

            let param_name = suggestion.parameter_name.clone();
            let suggested_value = suggestion.suggested_value.clone();
            let reason = suggestion.reason.clone();        

            // Safety validation
            if !self.safety_manager.validate_parameter_change(&suggestion.parameter_name, &suggestion.suggested_value).await? {
                rejected_changes.push(RejectedChange {
                    parameter_name: param_name.clone(),
                    suggested_value: suggested_value.clone(),
                    rejection_reason: "Safety manager rejection".to_string(),
                    suggested_by: "tuning_engine".to_string(),
                });
                continue;
            }
            
            // Apply the change
            {
                let mut store = self.parameter_store.write().await;
                
                // Validate dependencies
                if let Ok(warnings) = ParameterUtils::validate_dependencies(&store, &suggestion.parameter_name, &suggestion.suggested_value) {
                    if !warnings.is_empty() {
                        warn!("Parameter change warnings for {}: {:?}", suggestion.parameter_name, warnings);
                    }
                }
                
                match store.set_parameter(&suggestion.parameter_name, suggestion.suggested_value.clone()) {
                    Ok(old_value) => {
                        applied_changes.push(ParameterChange {
                            timestamp: chrono::Utc::now().timestamp() as u64,
                            parameter_name: param_name,
                            old_value,
                            new_value: suggested_value,
                            reason: reason,
                            triggered_by: "tuning_engine".to_string(),
                        });
                        
                        info!("Applied parameter change: {} = {} (was: {}, reason: {})", 
                              suggestion.parameter_name, 
                              suggestion.suggested_value,
                              applied_changes.last().unwrap().old_value,
                              suggestion.reason);
                    }
                    Err(e) => {
                        rejected_changes.push(RejectedChange {
                            parameter_name: suggestion.parameter_name,
                            suggested_value: suggestion.suggested_value,
                            rejection_reason: format!("Parameter validation failed: {}", e),
                            suggested_by: "tuning_engine".to_string(),
                        });
                    }
                }
            }
        }
        
        Ok((applied_changes, rejected_changes))
    }
    
    /// Calculate performance improvement between two metric snapshots
    fn calculate_performance_improvement(&self, before: &PerformanceMetrics, after: &PerformanceMetrics) -> f64 {
        let mut improvement_factors = Vec::new();
        
        // Latency improvement (lower is better)
        if before.average_latency_ms > 0.0 {
            let latency_improvement = (before.average_latency_ms - after.average_latency_ms) / before.average_latency_ms;
            improvement_factors.push(latency_improvement * 0.3); // 30% weight
        }
        
        // Memory improvement (lower is better) 
        if before.memory_usage_percent > 0.0 {
            let memory_improvement = (before.memory_usage_percent - after.memory_usage_percent) / before.memory_usage_percent;
            improvement_factors.push(memory_improvement * 0.2); // 20% weight
        }
        
        // Error rate improvement (lower is better)
        if before.error_rate_percent > 0.0 {
            let error_improvement = (before.error_rate_percent - after.error_rate_percent) / before.error_rate_percent;
            improvement_factors.push(error_improvement * 0.25); // 25% weight
        }
        
        // Throughput improvement (higher is better)
        if before.messages_per_second > 0.0 {
            let throughput_improvement = (after.messages_per_second - before.messages_per_second) / before.messages_per_second;
            improvement_factors.push(throughput_improvement * 0.25); // 25% weight
        }
        
        if improvement_factors.is_empty() {
            0.0
        } else {
            improvement_factors.iter().sum::<f64>() / improvement_factors.len() as f64
        }
    }
    
    /// Record tuning history for analysis
    async fn record_tuning_history(
        &self, 
        performance_before: PerformanceMetrics,
        performance_after: PerformanceMetrics,
        changes_applied: Vec<ParameterChange>,
        performance_improvement: f64
    ) -> Result<()> {
        let mut strategy_effectiveness = HashMap::new();
        
        // Calculate effectiveness of each strategy based on changes applied
        for change in &changes_applied {
            // This is a simplified approach - in reality, you'd track which strategy suggested each change
            let effectiveness = performance_improvement / changes_applied.len() as f64;
            strategy_effectiveness.insert("composite".to_string(), effectiveness);
        }
        
        let entry = TuningHistoryEntry {
            timestamp: chrono::Utc::now().timestamp() as u64,
            performance_before,
            performance_after,
            changes_applied,
            performance_improvement,
            strategy_effectiveness,
        };
        
        let mut history = self.tuning_history.write().await;
        history.push(entry);
        
        // Keep history bounded
        if history.len() > 100 {
            history.remove(0);
        }
        
        Ok(())
    }
    
    /// Find which strategy contributed most to the changes
    fn find_dominant_strategy(&self, changes: &[ParameterChange]) -> String {
        if changes.is_empty() {
            return "none".to_string();
        }
        
        // Simplified analysis - look at parameter categories to infer strategy
        let mut strategy_counts = HashMap::new();
        
        for change in changes {
            let strategy = match change.parameter_name.as_str() {
                name if name.contains("latency") || name.contains("timeout") => "latency_tuning",
                name if name.contains("memory") || name.contains("cache") => "memory_tuning", 
                name if name.contains("error") || name.contains("retry") => "error_rate_tuning",
                name if name.contains("connection") || name.contains("pool") => "load_balancing",
                name if name.contains("ai") || name.contains("confidence") => "adaptive_ai",
                _ => "unknown",
            };
            
            *strategy_counts.entry(strategy).or_insert(0) += 1;
        }
        
        strategy_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(strategy, _)| strategy.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Latency-focused tuning strategy
pub struct LatencyTuningStrategy {
    config: LatencyTuningConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyTuningConfig {
    pub target_latency_ms: f64,
    pub aggressive_threshold_ms: f64,
    pub timeout_adjustment_factor: f64,
}

impl Default for LatencyTuningConfig {
    fn default() -> Self {
        Self {
            target_latency_ms: 100.0,
            aggressive_threshold_ms: 500.0,
            timeout_adjustment_factor: 1.2,
        }
    }
}

impl LatencyTuningStrategy {
    pub fn new(config: LatencyTuningConfig) -> Result<Self> {
        Ok(Self { config })
    }
}

impl ParameterTuningStrategy for LatencyTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        if metrics.average_latency_ms > self.config.target_latency_ms {
            let latency_ratio = metrics.average_latency_ms / self.config.target_latency_ms;
            
            // Suggest reducing connection timeout if latency is high
            if let Some(current_timeout) = parameters.get_parameter("connection_timeout_ms") {
                if let Some(timeout_ms) = current_timeout.as_duration_ms() {
                    let reduction_factor = if metrics.average_latency_ms > self.config.aggressive_threshold_ms { 0.8 } else { 0.9 };
                    let new_timeout = (timeout_ms as f64 * reduction_factor) as u64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_timeout_ms".to_string(),
                        current_value: current_timeout.clone(),
                        suggested_value: ParameterValue::Duration(new_timeout),
                        confidence: (latency_ratio - 1.0).min(1.0),
                        reason: format!("Reduce timeout to improve latency ({:.1}ms -> target {:.1}ms)", 
                                      metrics.average_latency_ms, self.config.target_latency_ms),
                        expected_improvement: (latency_ratio - 1.0) * 0.3,
                    });
                }
            }
            
            // Suggest increasing batch size if processing latency is high
            if let Some(current_batch_size) = parameters.get_parameter("message_processing_batch_size") {
                if let Some(batch_size) = current_batch_size.as_i64() {
                    let new_batch_size = (batch_size as f64 * 1.5) as i64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "message_processing_batch_size".to_string(),
                        current_value: current_batch_size.clone(),
                        suggested_value: ParameterValue::Integer(new_batch_size),
                        confidence: 0.7,
                        reason: "Increase batch size to improve processing efficiency".to_string(),
                        expected_improvement: 0.15,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "latency_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        200 // High priority
    }
}

/// Memory-focused tuning strategy
pub struct MemoryTuningStrategy {
    config: MemoryTuningConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTuningConfig {
    pub target_memory_percent: f64,
    pub critical_threshold_percent: f64,
    pub cache_reduction_factor: f64,
}

impl Default for MemoryTuningConfig {
    fn default() -> Self {
        Self {
            target_memory_percent: 70.0,
            critical_threshold_percent: 90.0,
            cache_reduction_factor: 0.8,
        }
    }
}

impl MemoryTuningStrategy {
    pub fn new(config: MemoryTuningConfig) -> Result<Self> {
        Ok(Self { config })
    }
}

impl ParameterTuningStrategy for MemoryTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        if metrics.memory_usage_percent > self.config.target_memory_percent {
            let memory_pressure = (metrics.memory_usage_percent - self.config.target_memory_percent) / 100.0;
            
            // Suggest reducing cache sizes
            if let Some(current_cache_size) = parameters.get_parameter("message_cache_size") {
                if let Some(cache_size) = current_cache_size.as_i64() {
                    let reduction_factor = if metrics.memory_usage_percent > self.config.critical_threshold_percent {
                        self.config.cache_reduction_factor
                    } else {
                        0.9
                    };
                    
                    let new_cache_size = (cache_size as f64 * reduction_factor) as i64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "message_cache_size".to_string(),
                        current_value: current_cache_size.clone(),
                        suggested_value: ParameterValue::Integer(new_cache_size),
                        confidence: memory_pressure.min(1.0),
                        reason: format!("Reduce cache size due to high memory usage ({:.1}%)", 
                                      metrics.memory_usage_percent),
                        expected_improvement: memory_pressure * 0.4,
                    });
                }
            }
            
            // Suggest lowering GC threshold
            if let Some(current_gc_threshold) = parameters.get_parameter("gc_threshold_percent") {
                if let Some(gc_threshold) = current_gc_threshold.as_f64() {
                    let new_threshold = (gc_threshold - 5.0).max(60.0);
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "gc_threshold_percent".to_string(),
                        current_value: current_gc_threshold.clone(),
                        suggested_value: ParameterValue::Float(new_threshold),
                        confidence: 0.8,
                        reason: "Lower GC threshold to free memory more aggressively".to_string(),
                        expected_improvement: 0.2,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "memory_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        180 // High priority
    }
}

/// Error rate-focused tuning strategy
pub struct ErrorRateTuningStrategy {
    config: ErrorRateTuningConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRateTuningConfig {
    pub target_error_rate_percent: f64,
    pub critical_error_rate_percent: f64,
    pub retry_increase_factor: f64,
}

impl Default for ErrorRateTuningConfig {
    fn default() -> Self {
        Self {
            target_error_rate_percent: 1.0,
            critical_error_rate_percent: 5.0,
            retry_increase_factor: 1.5,
        }
    }
}

impl ErrorRateTuningStrategy {
    pub fn new(config: ErrorRateTuningConfig) -> Result<Self> {
        Ok(Self { config })
    }
}

impl ParameterTuningStrategy for ErrorRateTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        if metrics.error_rate_percent > self.config.target_error_rate_percent {
            let error_severity = metrics.error_rate_percent / self.config.target_error_rate_percent;
            
            // Suggest increasing retry attempts
            if let Some(current_retries) = parameters.get_parameter("connection_retry_attempts") {
                if let Some(retry_count) = current_retries.as_i64() {
                    let new_retry_count = if metrics.error_rate_percent > self.config.critical_error_rate_percent {
                        (retry_count as f64 * self.config.retry_increase_factor) as i64
                    } else {
                        retry_count + 1
                    };
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_retry_attempts".to_string(),
                        current_value: current_retries.clone(),
                        suggested_value: ParameterValue::Integer(new_retry_count),
                        confidence: (error_severity - 1.0).min(1.0),
                        reason: format!("Increase retries due to high error rate ({:.2}%)", 
                                      metrics.error_rate_percent),
                        expected_improvement: (error_severity - 1.0) * 0.6,
                    });
                }
            }
            
            // Suggest increasing connection timeout to reduce timeout errors
            if let Some(current_timeout) = parameters.get_parameter("connection_timeout_ms") {
                if let Some(timeout_ms) = current_timeout.as_duration_ms() {
                    let new_timeout = (timeout_ms as f64 * 1.3) as u64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_timeout_ms".to_string(),
                        current_value: current_timeout.clone(),
                        suggested_value: ParameterValue::Duration(new_timeout),
                        confidence: 0.7,
                        reason: "Increase timeout to reduce connection errors".to_string(),
                        expected_improvement: 0.25,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "error_rate_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        220 // Highest priority - errors are critical
    }
}

/// Load balancing strategy
pub struct LoadBalancingStrategy;

impl LoadBalancingStrategy {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl ParameterTuningStrategy for LoadBalancingStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Adjust connection pool size based on utilization
        if metrics.connection_pool_utilization > 0.8 {
            if let Some(current_pool_size) = parameters.get_parameter("connection_pool_max_size") {
                if let Some(pool_size) = current_pool_size.as_i64() {
                    let new_pool_size = pool_size + 1;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "connection_pool_max_size".to_string(),
                        current_value: current_pool_size.clone(),
                        suggested_value: ParameterValue::Integer(new_pool_size),
                        confidence: 0.8,
                        reason: format!("Increase pool size due to high utilization ({:.1}%)", 
                                      metrics.connection_pool_utilization * 100.0),
                        expected_improvement: 0.15,
                    });
                }
            }
        } else if metrics.connection_pool_utilization < 0.3 {
            // Reduce pool size if utilization is very low
            if let Some(current_pool_size) = parameters.get_parameter("connection_pool_max_size") {
                if let Some(pool_size) = current_pool_size.as_i64() {
                    if pool_size > 2 {
                        let new_pool_size = pool_size - 1;
                        
                        suggestions.push(ParameterSuggestion {
                            parameter_name: "connection_pool_max_size".to_string(),
                            current_value: current_pool_size.clone(),
                            suggested_value: ParameterValue::Integer(new_pool_size),
                            confidence: 0.6,
                            reason: format!("Reduce pool size due to low utilization ({:.1}%)", 
                                          metrics.connection_pool_utilization * 100.0),
                            expected_improvement: 0.05,
                        });
                    }
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "load_balancing"
    }
    
    fn get_priority(&self) -> u8 {
        150 // Medium priority
    }
}

/// Adaptive AI strategy
pub struct AdaptiveAIStrategy;

impl AdaptiveAIStrategy {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl ParameterTuningStrategy for AdaptiveAIStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Adjust AI confidence threshold based on processing performance
        if metrics.ai_processing_time_ms > 1000.0 {
            if let Some(current_threshold) = parameters.get_parameter("ai_confidence_threshold") {
                if let Some(threshold) = current_threshold.as_f64() {
                    let new_threshold = (threshold + 0.05).min(0.95);
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "ai_confidence_threshold".to_string(),
                        current_value: current_threshold.clone(),
                        suggested_value: ParameterValue::Float(new_threshold),
                        confidence: 0.7,
                        reason: format!("Increase AI threshold due to slow processing ({:.1}ms)", 
                                      metrics.ai_processing_time_ms),
                        expected_improvement: 0.2,
                    });
                }
            }
        }
        
        // Adjust pattern matching timeout based on performance
        if metrics.pattern_match_rate < 0.8 {
            if let Some(current_timeout) = parameters.get_parameter("pattern_matching_timeout_ms") {
                if let Some(timeout_ms) = current_timeout.as_duration_ms() {
                    let new_timeout = (timeout_ms as f64 * 1.2) as u64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "pattern_matching_timeout_ms".to_string(),
                        current_value: current_timeout.clone(),
                        suggested_value: ParameterValue::Duration(new_timeout),
                        confidence: 0.6,
                        reason: format!("Increase pattern timeout due to low match rate ({:.1}%)", 
                                      metrics.pattern_match_rate * 100.0),
                        expected_improvement: 0.1,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "adaptive_ai"
    }
    
    fn get_priority(&self) -> u8 {
        160 // Medium-high priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_tuning_engine_creation() {
        let config = AdaptiveConfig::default();
        let metrics_collector = Arc::new(MetricsCollector::new(1).unwrap());
        let parameter_store = Arc::new(RwLock::new(ParameterStore::new()));
        let safety_manager = Arc::new(SafetyManager::new(true, 10, 300).unwrap());
        
        let engine = TuningEngine::new(
            config,
            metrics_collector,
            parameter_store,
            safety_manager,
        ).unwrap();
        
        assert_eq!(engine.strategies.len(), 5);
    }
    
    #[tokio::test]
    async fn test_strategy_suggestions() {
        let metrics = PerformanceMetrics {
            average_latency_ms: 250.0,
            memory_usage_percent: 85.0,
            error_rate_percent: 3.0,
            ..Default::default()
        };
        
        let store = ParameterStore::new();
        
        let latency_strategy = LatencyTuningStrategy::new(LatencyTuningConfig::default()).unwrap();
        let suggestions = latency_strategy.suggest_adjustments(&metrics, &store);
        
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.parameter_name.contains("timeout")));
    }
    
    #[tokio::test]
    async fn test_performance_improvement_calculation() {
        let config = AdaptiveConfig::default();
        let metrics_collector = Arc::new(MetricsCollector::new(1).unwrap());
        let parameter_store = Arc::new(RwLock::new(ParameterStore::new()));
        let safety_manager = Arc::new(SafetyManager::new(true, 10, 300).unwrap());
        
        let engine = TuningEngine::new(
            config,
            metrics_collector,
            parameter_store,
            safety_manager,
        ).unwrap();
        
        let before = PerformanceMetrics {
            average_latency_ms: 200.0,
            memory_usage_percent: 80.0,
            error_rate_percent: 2.0,
            messages_per_second: 100.0,
            ..Default::default()
        };
        
        let after = PerformanceMetrics {
            average_latency_ms: 150.0,
            memory_usage_percent: 70.0,
            error_rate_percent: 1.0,
            messages_per_second: 120.0,
            ..Default::default()
        };
        
        let improvement = engine.calculate_performance_improvement(&before, &after);
        assert!(improvement > 0.0);
    }
}