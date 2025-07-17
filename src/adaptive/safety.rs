// src/adaptive/safety.rs
//! Safety mechanisms to prevent harmful parameter changes

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;

use super::*;

/// Safety manager that validates and monitors parameter changes
pub struct SafetyManager {
    enabled: bool,
    max_changes_per_hour: u32,
    rollback_threshold_seconds: u64,
    change_history: Arc<RwLock<VecDeque<SafetyChange>>>,
    rollback_history: Arc<RwLock<Vec<RollbackEvent>>>,
    safety_rules: Vec<Box<dyn SafetyRule + Send + Sync>>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
}

/// Record of a parameter change for safety tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyChange {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parameter_name: String,
    pub old_value: ParameterValue,
    pub new_value: ParameterValue,
    pub performance_before: Option<PerformanceMetrics>,
    pub performance_after: Option<PerformanceMetrics>,
    pub safety_score: f64,
}

/// Record of a rollback event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackEvent {
    pub timestamp: u64,
    pub parameter_name: String,
    pub rolled_back_value: ParameterValue,
    pub restored_value: ParameterValue,
    pub reason: String,
    pub trigger_metrics: PerformanceMetrics,
}

/// Safety status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyStatus {
    pub is_safe: bool,
    pub recent_changes: usize,
    pub rollbacks_in_last_hour: usize,
    pub circuit_breaker_state: CircuitBreakerState,
    pub safety_score: f64,
    pub warnings: Vec<String>,
}

/// Safety statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyStatistics {
    pub total_interventions: usize,
    pub total_rollbacks: usize,
    pub changes_blocked: usize,
    pub average_safety_score: f64,
    pub most_problematic_parameter: Option<String>,
}

/// Circuit breaker to stop changes when system is unstable
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure_time: Option<Instant>,
    failure_threshold: u32,
    timeout_duration: Duration,
    half_open_max_calls: u32,
    half_open_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Blocking all changes
    HalfOpen, // Testing if system has recovered
}

impl Default for SafetyStatus {
    fn default() -> Self {
        Self {
            is_safe: true,
            recent_changes: 0,
            rollbacks_in_last_hour: 0,
            circuit_breaker_state: CircuitBreakerState::Closed,
            safety_score: 1.0,
            warnings: Vec::new(),
        }
    }
}

// Also add Default for CircuitBreakerState if not already present
impl Default for CircuitBreakerState {
    fn default() -> Self {
        CircuitBreakerState::Closed
    }
}

impl SafetyManager {
    pub fn new(enabled: bool, max_changes_per_hour: u32, rollback_threshold_seconds: u64) -> Result<Self> {
        let mut manager = Self {
            enabled,
            max_changes_per_hour,
            rollback_threshold_seconds,
            change_history: Arc::new(RwLock::new(VecDeque::new())),
            rollback_history: Arc::new(RwLock::new(Vec::new())),
            safety_rules: Vec::new(),
            circuit_breaker: Arc::new(RwLock::new(CircuitBreaker::new())),
        };
        
        // Initialize safety rules
        manager.initialize_safety_rules()?;
        
        info!("Safety manager initialized (enabled: {}, max changes/hour: {})", 
              enabled, max_changes_per_hour);
        
        Ok(manager)
    }
    
    /// Validate if a parameter change is safe
    pub async fn validate_parameter_change(&self, name: &str, value: &ParameterValue) -> Result<bool> {
        if !self.enabled {
            return Ok(true);
        }
        
        // Check circuit breaker first
        {
            let circuit_breaker = self.circuit_breaker.read().await;
            if matches!(circuit_breaker.state, CircuitBreakerState::Open) {
                warn!("Circuit breaker is open - blocking parameter change for {}", name);
                return Ok(false);
            }
        }
        
        // Check rate limits
        if !self.check_rate_limits().await? {
            warn!("Rate limit exceeded - blocking parameter change for {}", name);
            return Ok(false);
        }
        
        // Apply safety rules
        for rule in &self.safety_rules {
            if !rule.validate_change(name, value).await? {
                warn!("Safety rule '{}' rejected parameter change: {} = {}", 
                      rule.get_name(), name, value);
                return Ok(false);
            }
        }
        
        debug!("Parameter change validated: {} = {}", name, value);
        Ok(true)
    }
    
    /// Record a parameter change for safety monitoring
    pub async fn record_parameter_change(
        &self,
        name: &str,
        old_value: ParameterValue,
        new_value: ParameterValue,
        performance_before: Option<PerformanceMetrics>,
    ) -> Result<()> {
        let safety_change = SafetyChange {
            timestamp: chrono::Utc::now(),  // <- CHANGE: Use chrono
            // instant: Instant::now(),        // <- ADD: For performance timing
            parameter_name: name.to_string(),
            old_value,
            new_value,
            performance_before,
            performance_after: None, // Will be updated later
            safety_score: 1.0, // Will be calculated later
        };
        
        let mut history = self.change_history.write().await;
        history.push_back(safety_change);
        
        // Keep history bounded (last 24 hours)
        let cutoff = chrono::Utc::now() - Duration::from_secs(86400);
        while let Some(front) = history.front() {
            if front.timestamp < cutoff {
                history.pop_front();
            } else {
                break;
            }
        }
        
        debug!("Recorded parameter change for safety monitoring: {}", name);
        Ok(())
    }
    
    /// Update performance metrics after a change
    pub async fn update_performance_after_change(
        &self,
        name: &str,
        performance_after: PerformanceMetrics,
    ) -> Result<()> {
        let mut history = self.change_history.write().await;
        
        // Find the most recent change for this parameter
        for change in history.iter_mut().rev() {
            if change.parameter_name == name && change.performance_after.is_none() {
                change.performance_after = Some(performance_after.clone());
                
                // Calculate safety score
                change.safety_score = self.calculate_safety_score(change).await;
                
                // Check if rollback is needed
                if change.safety_score < 0.3 {
                    warn!("Low safety score ({:.2}) for parameter {}, considering rollback", 
                          change.safety_score, name);
                    self.consider_rollback(change).await?;
                }
                
                break;
            }
        }
        
        Ok(())
    }
    
    /// Get current safety status
    pub async fn get_status(&self) -> Result<SafetyStatus> {
        let history = self.change_history.read().await;
        let rollback_history = self.rollback_history.read().await;
        let circuit_breaker = self.circuit_breaker.read().await;
        
        let one_hour_ago = chrono::Utc::now() - Duration::from_secs(3600);
        
        let recent_changes = history.iter()
            .filter(|c| c.timestamp > one_hour_ago)
            .count();
        
        let rollbacks_in_last_hour = rollback_history.iter()
            .filter(|r| {
                let rollback_time = chrono::DateTime::<chrono::Utc>::from_timestamp(r.timestamp as i64, 0)
                    .unwrap_or_else(chrono::Utc::now);
                let one_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
                rollback_time > one_hour_ago
            })
            .count();
        
        let safety_scores: Vec<f64> = history.iter()
            .filter(|c| c.timestamp > one_hour_ago)
            .map(|c| c.safety_score)
            .collect();
        
        let safety_score = if safety_scores.is_empty() {
            1.0
        } else {
            safety_scores.iter().sum::<f64>() / safety_scores.len() as f64
        };
        
        let mut warnings = Vec::new();
        
        if recent_changes > self.max_changes_per_hour as usize {
            warnings.push(format!("Rate limit exceeded: {} changes in last hour", recent_changes));
        }
        
        if rollbacks_in_last_hour > 0 {
            warnings.push(format!("{} rollbacks in last hour", rollbacks_in_last_hour));
        }
        
        if matches!(circuit_breaker.state, CircuitBreakerState::Open) {
            warnings.push("Circuit breaker is open".to_string());
        }
        
        let is_safe = warnings.is_empty() && safety_score > 0.5;
        
        Ok(SafetyStatus {
            is_safe,
            recent_changes,
            rollbacks_in_last_hour,
            circuit_breaker_state: circuit_breaker.state.clone(),
            safety_score,
            warnings,
        })
    }
    
    /// Get safety statistics
    pub async fn get_statistics(&self) -> Result<SafetyStatistics> {
        let history = self.change_history.read().await;
        let rollback_history = self.rollback_history.read().await;
        
        let total_rollbacks = rollback_history.len();
        let changes_blocked = 0; // Would need to track this separately
        
        let safety_scores: Vec<f64> = history.iter()
            .map(|c| c.safety_score)
            .collect();
        
        let average_safety_score = if safety_scores.is_empty() {
            1.0
        } else {
            safety_scores.iter().sum::<f64>() / safety_scores.len() as f64
        };
        
        // Find most problematic parameter
        let mut parameter_problems: HashMap<String, i32> = HashMap::new();
        for rollback in rollback_history.iter() {
            *parameter_problems.entry(rollback.parameter_name.clone()).or_insert(0) += 1;
        }
        
        let most_problematic_parameter = parameter_problems.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(param, _)| param);
        
        Ok(SafetyStatistics {
            total_interventions: total_rollbacks + changes_blocked,
            total_rollbacks,
            changes_blocked,
            average_safety_score,
            most_problematic_parameter,
        })
    }
    
    /// Trigger manual rollback of a parameter
    pub async fn trigger_rollback(&self, parameter_name: &str, reason: &str) -> Result<ParameterValue> {
        let history = self.change_history.read().await;
        
        // Find the most recent change for this parameter
        let recent_change = history.iter()
            .rev()
            .find(|c| c.parameter_name == parameter_name)
            .ok_or_else(|| anyhow::anyhow!("No recent changes found for parameter {}", parameter_name))?;
        
        let rollback_value = recent_change.old_value.clone();
        
        // Record the rollback
        let rollback_event = RollbackEvent {
            timestamp: chrono::Utc::now().timestamp() as u64,
            parameter_name: parameter_name.to_string(),
            rolled_back_value: recent_change.new_value.clone(),
            restored_value: rollback_value.clone(),
            reason: reason.to_string(),
            trigger_metrics: recent_change.performance_after.clone().unwrap_or_default(),
        };
        
        {
            let mut rollback_history = self.rollback_history.write().await;
            rollback_history.push(rollback_event);
        }
        
        // Update circuit breaker
        {
            let mut circuit_breaker = self.circuit_breaker.write().await;
            circuit_breaker.record_failure();
        }
        
        warn!("Rolled back parameter {} to {} (reason: {})", 
              parameter_name, rollback_value, reason);
        
        Ok(rollback_value)
    }
    
    /// Check if recent changes are within rate limits
    async fn check_rate_limits(&self) -> Result<bool> {
        let history = self.change_history.read().await;
        let one_hour_ago = chrono::Utc::now() - Duration::from_secs(3600);
        
        let recent_changes = history.iter()
            .filter(|c| c.timestamp > one_hour_ago)
            .count();
        
        Ok(recent_changes < self.max_changes_per_hour as usize)
    }
    
    /// Calculate safety score for a parameter change
    async fn calculate_safety_score(&self, change: &SafetyChange) -> f64 {
        let mut score = 1.0;
        
        if let (Some(before), Some(after)) = (&change.performance_before, &change.performance_after) {
            // Penalize if latency increased significantly
            if after.average_latency_ms > before.average_latency_ms * 1.5 {
                score -= 0.3;
            }
            
            // Penalize if memory usage increased significantly
            if after.memory_usage_percent > before.memory_usage_percent + 10.0 {
                score -= 0.3;
            }
            
            // Penalize if error rate increased
            if after.error_rate_percent > before.error_rate_percent + 1.0 {
                score -= 0.4;
            }
            
            // Penalize if system health decreased
            if after.system_health_score < before.system_health_score - 0.1 {
                score -= 0.2;
            }
        }
        
        f64::max(score, 0.0)
    }
    
    /// Consider if a rollback is needed based on safety score
    async fn consider_rollback(&self, change: &SafetyChange) -> Result<()> {
        if change.safety_score < 0.3 {
            let reason = format!("Automatic rollback due to low safety score: {:.2}", change.safety_score);
            self.trigger_rollback(&change.parameter_name, &reason).await?;
        }
        
        Ok(())
    }
    
    /// Initialize safety rules
    fn initialize_safety_rules(&mut self) -> Result<()> {
        // Add basic safety rules
        self.safety_rules.push(Box::new(ValueRangeRule::new()));
        self.safety_rules.push(Box::new(MemoryLimitRule::new()));
        self.safety_rules.push(Box::new(CriticalParameterRule::new()));
        self.safety_rules.push(Box::new(DependencyRule::new()));
        
        info!("Initialized {} safety rules", self.safety_rules.len());
        Ok(())
    }
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure_time: None,
            failure_threshold: 5,
            timeout_duration: Duration::from_secs(300), // 5 minutes
            half_open_max_calls: 3,
            half_open_calls: 0,
        }
    }
    
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());
        
        match self.state {
            CircuitBreakerState::Closed => {
                if self.failure_count >= self.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                    warn!("Circuit breaker opened after {} failures", self.failure_count);
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
                self.half_open_calls = 0;
                warn!("Circuit breaker reopened due to failure in half-open state");
            }
            _ => {}
        }
    }
    
    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::HalfOpen => {
                self.half_open_calls += 1;
                if self.half_open_calls >= self.half_open_max_calls {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.half_open_calls = 0;
                    info!("Circuit breaker closed after successful test period");
                }
            }
            CircuitBreakerState::Closed => {
                self.failure_count = 0; // Reset on success
            }
            _ => {}
        }
    }
    
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() > self.timeout_duration {
                        self.state = CircuitBreakerState::HalfOpen;
                        self.half_open_calls = 0;
                        info!("Circuit breaker moved to half-open state");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => self.half_open_calls < self.half_open_max_calls,
        }
    }
}

#[async_trait]
pub trait SafetyRule {
    async fn validate_change(&self, parameter_name: &str, value: &ParameterValue) -> Result<bool>;
    fn get_name(&self) -> &str;
}

/// Rule to validate parameter values are within acceptable ranges
pub struct ValueRangeRule;

impl ValueRangeRule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SafetyRule for ValueRangeRule {
    async fn validate_change(&self, parameter_name: &str, value: &ParameterValue) -> Result<bool> {
        // Define absolute safety limits for critical parameters
        match parameter_name {
            "connection_pool_max_size" => {
                if let Some(size) = value.as_i64() {
                    Ok(size >= 1 && size <= 20) // Never allow 0 connections or too many
                } else {
                    Ok(false)
                }
            }
            "connection_timeout_ms" => {
                if let Some(timeout) = value.as_duration_ms() {
                    Ok(timeout >= 1000 && timeout <= 300000) // 1 second to 5 minutes
                } else {
                    Ok(false)
                }
            }
            "memory_cache_size" => {
                if let Some(size) = value.as_i64() {
                    Ok(size >= 10 && size <= 50000) // Reasonable cache bounds
                } else {
                    Ok(false)
                }
            }
            "ai_confidence_threshold" => {
                if let Some(threshold) = value.as_f64() {
                    Ok(threshold >= 0.1 && threshold <= 0.99) // Keep within reasonable bounds
                } else {
                    Ok(false)
                }
            }
            _ => Ok(true), // Allow other parameters by default
        }
    }
    
    fn get_name(&self) -> &str {
        "value_range_rule"
    }
}

/// Rule to prevent memory-related parameters from causing OOM
pub struct MemoryLimitRule;

impl MemoryLimitRule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SafetyRule for MemoryLimitRule {
    async fn validate_change(&self, parameter_name: &str, value: &ParameterValue) -> Result<bool> {
        // Estimate memory impact of parameter changes
        match parameter_name {
            name if name.contains("cache_size") => {
                if let Some(size) = value.as_i64() {
                    // Assume each cache entry uses ~1KB on average
                    let estimated_memory_mb = size * 1024 / 1024 / 1024; // Convert to MB
                    Ok(estimated_memory_mb < 1000) // Don't allow more than 1GB for any single cache
                } else {
                    Ok(true)
                }
            }
            "processing_queue_max_size" => {
                if let Some(size) = value.as_i64() {
                    // Large queues can consume significant memory
                    Ok(size < 100000) // Reasonable queue size limit
                } else {
                    Ok(true)
                }
            }
            _ => Ok(true),
        }
    }
    
    fn get_name(&self) -> &str {
        "memory_limit_rule"
    }
}

/// Rule to protect critical parameters from dangerous changes
pub struct CriticalParameterRule;

impl CriticalParameterRule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SafetyRule for CriticalParameterRule {
    async fn validate_change(&self, parameter_name: &str, value: &ParameterValue) -> Result<bool> {
        // Define parameters that require extra caution
        let critical_parameters = [
            "worker_thread_count",
            "connection_pool_max_size", 
            "gc_threshold_percent",
        ];
        
        if critical_parameters.contains(&parameter_name) {
            // For critical parameters, be more conservative
            match parameter_name {
                "worker_thread_count" => {
                    if let Some(count) = value.as_i64() {
                        let cpu_count = num_cpus::get() as i64;
                        Ok(count >= 1 && count <= cpu_count * 2) // Don't exceed 2x CPU count
                    } else {
                        Ok(false)
                    }
                }
                "gc_threshold_percent" => {
                    if let Some(threshold) = value.as_f64() {
                        Ok(threshold >= 50.0 && threshold <= 95.0) // Keep GC reasonable
                    } else {
                        Ok(false)
                    }
                }
                _ => Ok(true),
            }
        } else {
            Ok(true)
        }
    }
    
    fn get_name(&self) -> &str {
        "critical_parameter_rule"
    }
}

/// Rule to validate parameter dependencies
pub struct DependencyRule;

impl DependencyRule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SafetyRule for DependencyRule {
    async fn validate_change(&self, parameter_name: &str, value: &ParameterValue) -> Result<bool> {
        // Check logical dependencies between parameters
        match parameter_name {
            "message_processing_batch_size" => {
                if let Some(batch_size) = value.as_i64() {
                    // Batch size should be reasonable
                    Ok(batch_size >= 1 && batch_size <= 1000)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(true),
        }
    }
    
    fn get_name(&self) -> &str {
        "dependency_rule"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_safety_manager_creation() {
        let manager = SafetyManager::new(true, 10, 300).unwrap();
        assert_eq!(manager.safety_rules.len(), 4);
    }
    
    #[tokio::test] 
    async fn test_parameter_validation() {
        let manager = SafetyManager::new(true, 10, 300).unwrap();
        
        // Valid parameter should pass
        let result = manager.validate_parameter_change(
            "connection_timeout_ms", 
            &ParameterValue::Duration(30000)
        ).await.unwrap();
        assert!(result);
        
        // Invalid parameter should fail
        let result = manager.validate_parameter_change(
            "connection_timeout_ms",
            &ParameterValue::Duration(500) // Too low
        ).await.unwrap();
        assert!(!result);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let mut breaker = CircuitBreaker::new();
        
        assert!(matches!(breaker.state, CircuitBreakerState::Closed));
        assert!(breaker.can_execute());
        
        // Record failures to open circuit
        for _ in 0..5 {
            breaker.record_failure();
        }
        
        assert!(matches!(breaker.state, CircuitBreakerState::Open));
        assert!(!breaker.can_execute());
    }
    
    #[tokio::test]
    async fn test_safety_rules() {
        let rule = ValueRangeRule::new();
        
        // Valid connection pool size
        let result = rule.validate_change(
            "connection_pool_max_size",
            &ParameterValue::Integer(5)
        ).await.unwrap();
        assert!(result);
        
        // Invalid connection pool size
        let result = rule.validate_change(
            "connection_pool_max_size", 
            &ParameterValue::Integer(0)
        ).await.unwrap();
        assert!(!result);
    }
    
    #[tokio::test]
    async fn test_rollback_mechanism() {
        let manager = SafetyManager::new(true, 10, 300).unwrap();
        
        // Record a parameter change
        manager.record_parameter_change(
            "test_param",
            ParameterValue::Integer(100),
            ParameterValue::Integer(200),
            None,
        ).await.unwrap();
        
        // Trigger rollback
        let rollback_value = manager.trigger_rollback(
            "test_param",
            "Test rollback"
        ).await.unwrap();
        
        assert_eq!(rollback_value, ParameterValue::Integer(100));
        
        let stats = manager.get_statistics().await.unwrap();
        assert_eq!(stats.total_rollbacks, 1);
    }
}