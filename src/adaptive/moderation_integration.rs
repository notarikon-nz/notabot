// src/adaptive/moderation_integration.rs
//! Integration between adaptive tuning and moderation system

use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;

use crate::bot::moderation::ModerationSystem;
use super::*;

/// Extension trait for ModerationSystem to provide adaptive metrics
pub trait ModerationSystemAdaptive {
    async fn get_adaptive_metrics(&self) -> Result<ModerationMetrics>;
    async fn get_filter_effectiveness(&self) -> HashMap<String, FilterEffectiveness>;
    async fn apply_adaptive_parameters(&self, parameters: &HashMap<String, ParameterValue>) -> Result<()>;
}

/// Metrics specific to moderation performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationMetrics {
    pub total_messages_processed: u64,
    pub spam_detection_rate: f64,
    pub false_positive_rate: f64,
    pub filter_efficiency_score: f64,
    pub average_processing_time_ms: f64,
    pub user_appeal_rate: f64,
    pub filter_metrics: HashMap<String, FilterMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterMetrics {
    pub filter_name: String,
    pub triggered_count: u64,
    pub success_rate: f64,
    pub false_positive_rate: f64,
    pub average_confidence: f64,
    pub user_appeals: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterEffectiveness {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub user_satisfaction: f64,
}

impl ModerationSystemAdaptive for ModerationSystem {
    async fn get_adaptive_metrics(&self) -> Result<ModerationMetrics> {
        let filter_stats = self.get_filter_stats().await;
        let mut filter_metrics = HashMap::new();
        
        // Extract metrics from your existing filter stats
        let total_violations = filter_stats.get("total_violations")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        let enabled_filters = filter_stats.get("enabled_filters")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        // Calculate spam detection rate (this would need more detailed tracking)
        let spam_detection_rate = if total_violations > 0 { 85.0 } else { 0.0 }; // Placeholder
        
        // Get per-filter metrics from your detailed stats
        if let Some(filter_details) = filter_stats.get("filter_details").and_then(|v| v.as_object()) {
            for (filter_name, details) in filter_details {
                if let Some(violations) = details.get("violations").and_then(|v| v.as_u64()) {
                    filter_metrics.insert(filter_name.clone(), FilterMetrics {
                        filter_name: filter_name.clone(),
                        triggered_count: violations,
                        success_rate: 85.0, // Would calculate from actual data
                        false_positive_rate: 5.0, // Would track appeals/reversals
                        average_confidence: 0.85, // Would track from AI system
                        user_appeals: 0, // Would track from appeal system
                    });
                }
            }
        }
        
        // Calculate overall efficiency score
        let filter_efficiency = if enabled_filters > 0 {
            (spam_detection_rate / 100.0) * 0.7 + ((100.0 - 5.0) / 100.0) * 0.3 // Detection rate + (100 - false positive rate)
        } else {
            0.0
        };
        
        Ok(ModerationMetrics {
            total_messages_processed: total_violations * 10, // Estimate from violations
            spam_detection_rate,
            false_positive_rate: 5.0, // Would track from appeals
            filter_efficiency_score: filter_efficiency,
            average_processing_time_ms: 15.0, // Would measure actual processing time
            user_appeal_rate: 2.0, // Would track from appeals
            filter_metrics,
        })
    }
    
    async fn get_filter_effectiveness(&self) -> HashMap<String, FilterEffectiveness> {
        let mut effectiveness = HashMap::new();
        
        let filter_stats = self.get_filter_stats().await;
        if let Some(filter_details) = filter_stats.get("filter_details").and_then(|v| v.as_object()) {
            for (filter_name, _details) in filter_details {
                // Calculate effectiveness metrics (would need detailed tracking)
                effectiveness.insert(filter_name.clone(), FilterEffectiveness {
                    accuracy: 0.85,  // Would calculate: (TP + TN) / (TP + TN + FP + FN)
                    precision: 0.90, // Would calculate: TP / (TP + FP)
                    recall: 0.80,    // Would calculate: TP / (TP + FN)
                    f1_score: 0.85,  // Would calculate: 2 * (precision * recall) / (precision + recall)
                    user_satisfaction: 0.88, // Would calculate from user feedback
                });
            }
        }
        
        effectiveness
    }
    
    async fn apply_adaptive_parameters(&self, parameters: &HashMap<String, ParameterValue>) -> Result<()> {
        for (param_name, param_value) in parameters {
            match param_name.as_str() {
                "ai_confidence_threshold" => {
                    if let Some(threshold) = param_value.as_f64() {
                        info!("Would update AI confidence threshold to: {:.2}", threshold);
                        // In a real implementation, you'd update your AI system's threshold
                        // self.update_ai_confidence_threshold(threshold).await?;
                    }
                }
                "pattern_matching_timeout_ms" => {
                    if let Some(timeout) = param_value.as_duration_ms() {
                        info!("Would update pattern matching timeout to: {}ms", timeout);
                        // self.update_pattern_timeout(Duration::from_millis(timeout)).await?;
                    }
                }
                "filter_max_checks_per_second" => {
                    if let Some(rate) = param_value.as_i64() {
                        info!("Would update filter rate limit to: {} checks/sec", rate);
                        // self.update_filter_rate_limit(rate as u32).await?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Moderation-focused tuning strategy
pub struct ModerationTuningStrategy {
    moderation_system: Arc<ModerationSystem>,
}

impl ModerationTuningStrategy {
    pub fn new(moderation_system: Arc<ModerationSystem>) -> Self {
        Self { moderation_system }
    }
}

impl ParameterTuningStrategy for ModerationTuningStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Adjust AI confidence based on pattern match rate
        if metrics.pattern_match_rate < 0.7 {
            if let Some(current_threshold) = parameters.get_parameter("ai_confidence_threshold") {
                if let Some(threshold) = current_threshold.as_f64() {
                    let new_threshold = (threshold - 0.05).max(0.1);
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "ai_confidence_threshold".to_string(),
                        current_value: current_threshold.clone(),
                        suggested_value: ParameterValue::Float(new_threshold),
                        confidence: 0.8,
                        reason: format!("Low pattern match rate ({:.1}%) - reduce AI threshold for better coverage", 
                                      metrics.pattern_match_rate * 100.0),
                        expected_improvement: 0.25,
                    });
                }
            }
        } else if metrics.pattern_match_rate > 0.95 {
            // Very high match rate might indicate too many false positives
            if let Some(current_threshold) = parameters.get_parameter("ai_confidence_threshold") {
                if let Some(threshold) = current_threshold.as_f64() {
                    let new_threshold = (threshold + 0.05).min(0.95);
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "ai_confidence_threshold".to_string(),
                        current_value: current_threshold.clone(),
                        suggested_value: ParameterValue::Float(new_threshold),
                        confidence: 0.7,
                        reason: format!("Very high pattern match rate ({:.1}%) - increase threshold to reduce false positives", 
                                      metrics.pattern_match_rate * 100.0),
                        expected_improvement: 0.15,
                    });
                }
            }
        }
        
        // Adjust processing timeout based on AI processing time
        if metrics.ai_processing_time_ms > 1000.0 {
            if let Some(current_timeout) = parameters.get_parameter("pattern_matching_timeout_ms") {
                if let Some(timeout_ms) = current_timeout.as_duration_ms() {
                    let new_timeout = (timeout_ms as f64 * 1.3) as u64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "pattern_matching_timeout_ms".to_string(),
                        current_value: current_timeout.clone(),
                        suggested_value: ParameterValue::Duration(new_timeout),
                        confidence: 0.8,
                        reason: format!("High AI processing time ({:.1}ms) - increase timeout", 
                                      metrics.ai_processing_time_ms),
                        expected_improvement: 0.2,
                    });
                }
            }
        }
        
        // Adjust moderation queue based on queue length
        if metrics.moderation_queue_length > 100 {
            if let Some(current_batch_size) = parameters.get_parameter("message_processing_batch_size") {
                if let Some(batch_size) = current_batch_size.as_i64() {
                    let new_batch_size = (batch_size as f64 * 1.5) as i64;
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "message_processing_batch_size".to_string(),
                        current_value: current_batch_size.clone(),
                        suggested_value: ParameterValue::Integer(new_batch_size),
                        confidence: 0.9,
                        reason: format!("Large moderation queue ({} messages) - increase batch size", 
                                      metrics.moderation_queue_length),
                        expected_improvement: 0.3,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "moderation_tuning"
    }
    
    fn get_priority(&self) -> u8 {
        210 // Very high priority for moderation effectiveness
    }
}

/// Enhanced AI moderation strategy that learns from user feedback
pub struct EnhancedAIModerationStrategy {
    moderation_system: Arc<ModerationSystem>,
    user_feedback_weight: f32,
}

impl EnhancedAIModerationStrategy {
    pub fn new(moderation_system: Arc<ModerationSystem>) -> Self {
        Self {
            moderation_system,
            user_feedback_weight: 0.3, // 30% weight for user feedback
        }
    }
    
    /// Process user feedback for adaptive learning
    pub async fn process_user_feedback(&self, feedback_type: &str, filter_name: &str, user_confidence: f32) -> Result<()> {
        // This would integrate with your user feedback system
        match feedback_type {
            "false_positive" => {
                info!("Processing false positive feedback for filter: {} (confidence: {:.2})", filter_name, user_confidence);
                // Would adjust filter sensitivity down
            }
            "missed_spam" => {
                info!("Processing missed spam feedback for filter: {} (confidence: {:.2})", filter_name, user_confidence);
                // Would adjust filter sensitivity up
            }
            "appeal_granted" => {
                info!("Appeal granted for filter: {} (confidence: {:.2})", filter_name, user_confidence);
                // Would reduce filter aggressiveness
            }
            _ => {}
        }
        Ok(())
    }
}

impl ParameterTuningStrategy for EnhancedAIModerationStrategy {
    fn suggest_adjustments(&self, metrics: &PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion> {
        let mut suggestions = Vec::new();
        
        // Use AI processing time to adjust learning rate
        if metrics.ai_processing_time_ms > 500.0 {
            if let Some(current_rate) = parameters.get_parameter("learning_rate") {
                if let Some(rate) = current_rate.as_f64() {
                    let new_rate = (rate * 0.8).max(0.0001); // Reduce learning rate to speed up processing
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "learning_rate".to_string(),
                        current_value: current_rate.clone(),
                        suggested_value: ParameterValue::Float(new_rate),
                        confidence: 0.7,
                        reason: "High AI processing time - reduce learning rate for faster inference".to_string(),
                        expected_improvement: 0.25,
                    });
                }
            }
        }
        
        // Adjust confidence threshold based on system health
        if metrics.system_health_score < 0.8 {
            if let Some(current_threshold) = parameters.get_parameter("ai_confidence_threshold") {
                if let Some(threshold) = current_threshold.as_f64() {
                    let new_threshold = (threshold + 0.1).min(0.95); // Be more conservative when system health is low
                    
                    suggestions.push(ParameterSuggestion {
                        parameter_name: "ai_confidence_threshold".to_string(),
                        current_value: current_threshold.clone(),
                        suggested_value: ParameterValue::Float(new_threshold),
                        confidence: 0.8,
                        reason: format!("Low system health ({:.1}%) - increase AI threshold for stability", 
                                      metrics.system_health_score * 100.0),
                        expected_improvement: 0.2,
                    });
                }
            }
        }
        
        suggestions
    }
    
    fn get_strategy_name(&self) -> &str {
        "enhanced_ai_moderation"
    }
    
    fn get_priority(&self) -> u8 {
        200 // High priority for AI optimization
    }
}

/// Collect moderation metrics for adaptive system
pub async fn collect_moderation_metrics(
    moderation_system: &Arc<ModerationSystem>,
    metrics_collector: &Arc<MetricsCollector>,
) -> Result<()> {
    let adaptive_metrics = moderation_system.get_adaptive_metrics().await?;
    
    // Record moderation-level metrics
    metrics_collector.record_custom_metric("spam_detection_rate", adaptive_metrics.spam_detection_rate).await?;
    metrics_collector.record_custom_metric("false_positive_rate", adaptive_metrics.false_positive_rate).await?;
    metrics_collector.record_custom_metric("filter_efficiency", adaptive_metrics.filter_efficiency_score * 100.0).await?;
    metrics_collector.record_custom_metric("user_appeal_rate", adaptive_metrics.user_appeal_rate).await?;
    
    // Record AI processing metrics
    metrics_collector.record_latency("ai_processing", adaptive_metrics.average_processing_time_ms).await?;
    
    // Record per-filter metrics
    for (filter_name, filter_metrics) in &adaptive_metrics.filter_metrics {
        let filter_prefix = format!("filter_{}", filter_name);
        
        metrics_collector.record_custom_metric(
            &format!("{}_success_rate", filter_prefix), 
            filter_metrics.success_rate
        ).await?;
        
        metrics_collector.record_custom_metric(
            &format!("{}_confidence", filter_prefix), 
            filter_metrics.average_confidence * 100.0
        ).await?;
        
        metrics_collector.record_custom_metric(
            &format!("{}_triggers", filter_prefix), 
            filter_metrics.triggered_count as f64
        ).await?;
    }
    
    debug!("Collected moderation metrics for adaptive system");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_moderation_tuning_strategy() {
        let moderation_system = Arc::new(ModerationSystem::new());
        let strategy = ModerationTuningStrategy::new(moderation_system);
        
        let metrics = PerformanceMetrics {
            pattern_match_rate: 0.6, // Low match rate
            ai_processing_time_ms: 1200.0, // High processing time
            moderation_queue_length: 150, // Large queue
            ..Default::default()
        };
        
        let store = ParameterStore::new();
        let suggestions = strategy.suggest_adjustments(&metrics, &store);
        
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.parameter_name == "ai_confidence_threshold"));
        assert!(suggestions.iter().any(|s| s.parameter_name == "pattern_matching_timeout_ms"));
    }
    
    #[tokio::test]
    async fn test_enhanced_ai_strategy() {
        let moderation_system = Arc::new(ModerationSystem::new());
        let strategy = EnhancedAIModerationStrategy::new(moderation_system.clone());
        
        // Test user feedback processing
        let result = strategy.process_user_feedback("false_positive", "test_filter", 0.8).await;
        assert!(result.is_ok());
        
        // Test parameter suggestions
        let metrics = PerformanceMetrics {
            ai_processing_time_ms: 600.0, // High processing time
            system_health_score: 0.7, // Low health
            ..Default::default()
        };
        
        let store = ParameterStore::new();
        let suggestions = strategy.suggest_adjustments(&metrics, &store);
        
        assert!(!suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_moderation_metrics() {
        let moderation_system = Arc::new(ModerationSystem::new());
        
        let result = moderation_system.get_adaptive_metrics().await;
        assert!(result.is_ok());
        
        let metrics = result.unwrap();
        assert!(metrics.filter_efficiency_score >= 0.0);
        assert!(metrics.filter_efficiency_score <= 1.0);
    }
}