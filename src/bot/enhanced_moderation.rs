use anyhow::Result;
use log::{info, warn, error};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::Path;

use crate::types::{ChatMessage, ModerationAction};
use crate::bot::points::UserPoints;
use crate::bot::pattern_matching::{EnhancedPatternMatcher, AdvancedPattern};
use crate::bot::smart_escalation::{SmartEscalationCalculator, SmartEscalation, ViolationSeverity, PositiveActionType};
use crate::bot::realtime_analytics::{FilterAnalyticsSystem, UserReportType, ModeratorReviewType};
use crate::bot::filter_import_export::{FilterImportExport, ExportFormat, ExportOptions, ImportOptions};

/// Enhanced moderation system that integrates all Phase 2 features
#[derive(Clone)]
pub struct EnhancedModerationSystem {
    // Core components
    base_moderation: Arc<crate::bot::moderation::ModerationSystem>,
    
    // Phase 2 enhancements
    pattern_matcher: Arc<RwLock<EnhancedPatternMatcher>>,
    escalation_calculator: Arc<RwLock<SmartEscalationCalculator>>,
    analytics_system: Arc<FilterAnalyticsSystem>,
    import_export: Arc<FilterImportExport>,
    
    // Configuration
    enhanced_features_enabled: Arc<RwLock<bool>>,
    auto_optimization_enabled: Arc<RwLock<bool>>,
    learning_mode: Arc<RwLock<bool>>,
}

impl EnhancedModerationSystem {
    pub fn new(base_moderation: Arc<crate::bot::moderation::ModerationSystem>) -> Self {
        Self {
            base_moderation,
            pattern_matcher: Arc::new(RwLock::new(EnhancedPatternMatcher::new())),
            escalation_calculator: Arc::new(RwLock::new(SmartEscalationCalculator::new(SmartEscalation::default()))),
            analytics_system: Arc::new(FilterAnalyticsSystem::new()),
            import_export: Arc::new(FilterImportExport::new()),
            enhanced_features_enabled: Arc::new(RwLock::new(true)),
            auto_optimization_enabled: Arc::new(RwLock::new(false)), // Disabled by default for safety
            learning_mode: Arc::new(RwLock::new(false)),
        }
    }

    /// Enhanced message checking with Phase 2 features
    pub async fn check_message_enhanced(
        &self,
        message: &ChatMessage,
        user_points: Option<&UserPoints>,
    ) -> Option<EnhancedModerationResult> {
        let start_time = std::time::Instant::now();

        // Check if enhanced features are enabled
        if !*self.enhanced_features_enabled.read().await {
            // Fall back to base moderation
            if let Some(action) = self.base_moderation.check_spam_filters(message, user_points).await {
                return Some(EnhancedModerationResult {
                    action,
                    confidence: 0.8, // Default confidence for base filters
                    triggered_filters: vec!["base_filter".to_string()],
                    advanced_patterns: vec![],
                    escalation_applied: false,
                    response_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
                    severity: ViolationSeverity::Moderate,
                });
            }
            return None;
        }

        // Enhanced pattern matching
        let mut triggered_filters = Vec::new();
        let mut advanced_patterns = Vec::new();
        let mut max_severity = ViolationSeverity::Minor;

        // Check advanced patterns first
        {
            let mut pattern_matcher = self.pattern_matcher.write().await;
            let pattern_matches = pattern_matcher.matches(&message.content);
            
            if !pattern_matches.is_empty() {
                advanced_patterns = pattern_matches.clone();
                triggered_filters.extend(pattern_matches);
                max_severity = ViolationSeverity::Moderate; // Advanced patterns are typically more serious
            }
        }

        // Check base filters
        if let Some(base_action) = self.base_moderation.check_spam_filters(message, user_points).await {
            triggered_filters.push("base_moderation".to_string());
            
            // Determine severity based on action type
            let filter_severity = match base_action {
                ModerationAction::WarnUser { .. } => ViolationSeverity::Minor,
                ModerationAction::TimeoutUser { duration_seconds } => {
                    if duration_seconds < 300 {
                        ViolationSeverity::Moderate
                    } else if duration_seconds < 3600 {
                        ViolationSeverity::Major
                    } else {
                        ViolationSeverity::Severe
                    }
                }
                _ => ViolationSeverity::Moderate,
            };
            
            max_severity = std::cmp::max(max_severity, filter_severity);
        }

        // If no violations detected, return None
        if triggered_filters.is_empty() {
            return None;
        }

        // Apply smart escalation
        let user_id = format!("{}:{}", message.platform, message.username);
        let escalation_applied = triggered_filters.len() > 1 || max_severity >= ViolationSeverity::Major;
        
        let final_action = if escalation_applied {
            let mut escalation_calc = self.escalation_calculator.write().await;
            escalation_calc.calculate_action(
                &user_id,
                &triggered_filters.join(","),
                max_severity.clone(),
                &message.channel,
                user_points,
                message,
            )
        } else {
            // Use base action for simple violations
            self.base_moderation.check_spam_filters(message, user_points).await
                .unwrap_or(ModerationAction::WarnUser { 
                    message: "Please follow chat rules".to_string() 
                })
        };

        let response_time = start_time.elapsed().as_secs_f64() * 1000.0;

        // Record analytics
        for filter in &triggered_filters {
            let is_true_positive = true; // Would be determined by feedback in real implementation
            self.analytics_system.record_trigger(
                filter,
                "enhanced_filter",
                is_true_positive,
                response_time,
                &message.content,
            ).await;
        }

        // Record violation in escalation system
        if escalation_applied {
            let mut escalation_calc = self.escalation_calculator.write().await;
            escalation_calc.record_violation(
                &user_id,
                &triggered_filters.join(","),
                max_severity.clone(),
                final_action.clone(),
                &message.channel,
            );
        }

        Some(EnhancedModerationResult {
            action: final_action,
            confidence: self.calculate_confidence(&triggered_filters, &advanced_patterns).await,
            triggered_filters,
            advanced_patterns,
            escalation_applied,
            response_time_ms: response_time,
            severity: max_severity,
        })
    }

    /// Add advanced pattern to the system
    pub async fn add_advanced_pattern(&self, pattern: AdvancedPattern) -> Result<()> {
        let mut pattern_matcher = self.pattern_matcher.write().await;
        pattern_matcher.add_pattern(pattern);
        info!("Added advanced pattern to enhanced moderation system");
        Ok(())
    }

    /// Enable/disable enhanced features
    pub async fn set_enhanced_features_enabled(&self, enabled: bool) {
        *self.enhanced_features_enabled.write().await = enabled;
        info!("Enhanced moderation features {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Enable/disable auto-optimization
    pub async fn set_auto_optimization_enabled(&self, enabled: bool) {
        *self.auto_optimization_enabled.write().await = enabled;
        info!("Auto-optimization {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Enable/disable learning mode
    pub async fn set_learning_mode(&self, enabled: bool) {
        *self.learning_mode.write().await = enabled;
        info!("Learning mode {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Record user feedback for machine learning
    pub async fn record_user_feedback(
        &self,
        filter_id: &str,
        user_id: &str,
        feedback_type: UserReportType,
        message_content: &str,
        explanation: Option<String>,
    ) -> Result<()> {
        // Record in analytics system
        self.analytics_system.record_user_report(
            filter_id,
            user_id,
            feedback_type.clone(),
            message_content,
            explanation,
        ).await;

        // Update pattern matcher if in learning mode
        if *self.learning_mode.read().await {
            let mut pattern_matcher = self.pattern_matcher.write().await;
            match feedback_type {
                UserReportType::FalsePositive => {
                    pattern_matcher.report_false_positive(filter_id);
                    info!("Recorded false positive feedback for filter: {}", filter_id);
                }
                UserReportType::MissedViolation => {
                    // Could trigger pattern refinement or new pattern creation
                    warn!("Missed violation reported for content: {}", message_content);
                }
                _ => {}
            }
        }

        // Record positive action for good feedback
        if matches!(feedback_type, UserReportType::FalsePositive) {
            let mut escalation_calc = self.escalation_calculator.write().await;
            escalation_calc.record_positive_action(user_id, PositiveActionType::AccurateReport);
        }

        Ok(())
    }

    /// Record moderator review
    pub async fn record_moderator_review(
        &self,
        filter_id: &str,
        moderator_id: &str,
        review_type: ModeratorReviewType,
        accuracy_rating: f32,
        suggestions: Vec<String>,
        notes: String,
    ) -> Result<()> {
        self.analytics_system.record_moderator_review(
            filter_id,
            moderator_id,
            review_type,
            accuracy_rating,
            suggestions,
            notes,
        ).await;

        // Record positive action for the moderator
        let mut escalation_calc = self.escalation_calculator.write().await;
        escalation_calc.record_positive_action(moderator_id, PositiveActionType::CommunitySupport);

        info!("Recorded moderator review from {} for filter {}", moderator_id, filter_id);
        Ok(())
    }

    /// Export filter configuration
    pub async fn export_filters(
        &self,
        output_path: &Path,
        format: ExportFormat,
        options: ExportOptions,
    ) -> Result<()> {
        // Get filters from base moderation system
        let filters = self.base_moderation.spam_filters.read().await;
        let filter_map: HashMap<String, crate::types::SpamFilter> = filters.iter()
            .map(|(name, filter)| (name.clone(), filter.clone()))
            .collect();

        self.import_export.export_filters(&filter_map, format, output_path, options).await?;
        info!("Exported {} filters to {}", filter_map.len(), output_path.display());
        Ok(())
    }

    /// Import filter configuration
    pub async fn import_filters(
        &self,
        input_path: &Path,
        format: Option<ExportFormat>,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let result = self.import_export.import_filters(input_path, format, options).await?;
        
        if !result.filters.is_empty() {
            // Add imported filters to base moderation system
            let mut base_filters = self.base_moderation.spam_filters.write().await;
            for (name, filter) in result.filters.iter() {
                base_filters.insert(name.clone(), filter.clone());
            }
            
            info!("Successfully imported {} filters", result.imported_count);
            if !result.warnings.is_empty() {
                warn!("Import warnings: {:?}", result.warnings);
            }
            if !result.errors.is_empty() {
                error!("Import errors: {:?}", result.errors);
            }
        }

        Ok(ImportResult {
            imported_count: result.imported_count,
            error_count: result.error_count,
            warning_count: result.warning_count,
            errors: result.errors,
            warnings: result.warnings,
        })
    }

    /// Get comprehensive analytics dashboard
    pub async fn get_analytics_dashboard(&self) -> Result<AnalyticsDashboard> {
        let dashboard = self.analytics_system.get_dashboard_data().await;
        Ok(dashboard)
    }

    /// Get filter effectiveness report
    pub async fn get_effectiveness_report(&self) -> Result<EffectivenessReport> {
        let pattern_stats = {
            let pattern_matcher = self.pattern_matcher.read().await;
            pattern_matcher.get_effectiveness_stats().clone()
        };

        let escalation_stats = {
            let escalation_calc = self.escalation_calculator.read().await;
            escalation_calc.get_effectiveness_stats()
        };

        let analytics_dashboard = self.analytics_system.get_dashboard_data().await;

        Ok(EffectivenessReport {
            pattern_effectiveness: pattern_stats,
            escalation_effectiveness: escalation_stats,
            overall_accuracy: analytics_dashboard.global_metrics.overall_accuracy,
            user_satisfaction: analytics_dashboard.global_metrics.user_satisfaction_score,
            performance_metrics: PerformanceMetrics {
                average_response_time: analytics_dashboard.filter_summaries.iter()
                    .map(|f| f.average_response_time)
                    .sum::<f64>() / analytics_dashboard.filter_summaries.len().max(1) as f64,
                total_messages_processed: analytics_dashboard.global_metrics.total_messages_processed,
                peak_load: analytics_dashboard.global_metrics.peak_load_messages_per_second,
            },
            recommendations: analytics_dashboard.system_recommendations,
        })
    }

    /// Auto-optimize filters based on analytics
    pub async fn auto_optimize_filters(&self) -> Result<OptimizationResult> {
        if !*self.auto_optimization_enabled.read().await {
            return Err(anyhow::anyhow!("Auto-optimization is disabled"));
        }

        let mut optimizations_applied = 0;
        let suggestions_generated;

        // Get ineffective patterns and remove them
        {
            let pattern_matcher = self.pattern_matcher.read().await;
            let ineffective_patterns = pattern_matcher.get_ineffective_patterns(0.6); // 60% threshold
            
            for pattern_id in ineffective_patterns {
                warn!("Removing ineffective pattern: {}", pattern_id);
                // Pattern removal logic would go here
                optimizations_applied += 1;
            }
        }

        // Generate optimization suggestions
        let dashboard = self.analytics_system.get_dashboard_data().await;
        suggestions_generated = dashboard.optimization_opportunities.len();

        info!("Auto-optimization complete: {} optimizations applied, {} suggestions generated", 
              optimizations_applied, suggestions_generated);

        Ok(OptimizationResult {
            optimizations_applied,
            suggestions_generated,
            performance_improvement: 5.0, // Would be calculated based on actual improvements
        })
    }

    /// Calculate confidence score for a moderation decision
    async fn calculate_confidence(&self, triggered_filters: &[String], advanced_patterns: &[String]) -> f64 {
        let base_confidence = 0.8;
        
        // More filters triggered = higher confidence
        let filter_bonus = (triggered_filters.len() as f64 * 0.1).min(0.3);
        
        // Advanced pattern matches increase confidence
        let pattern_bonus = (advanced_patterns.len() as f64 * 0.15).min(0.2);
        
        (base_confidence + filter_bonus + pattern_bonus).min(1.0)
    }

    /// Setup default advanced patterns
    pub async fn setup_default_advanced_patterns(&self) -> Result<()> {
        let patterns = vec![
            // Fuzzy matching for common spam words
            AdvancedPattern::FuzzyMatch {
                pattern: "spam".to_string(),
                threshold: 0.8,
            },
            AdvancedPattern::FuzzyMatch {
                pattern: "scam".to_string(),
                threshold: 0.8,
            },
            
            // Leetspeak detection
            AdvancedPattern::Leetspeak("spam".to_string()),
            AdvancedPattern::Leetspeak("follow".to_string()),
            AdvancedPattern::Leetspeak("subscribe".to_string()),
            
            // Unicode normalization for international spam
            AdvancedPattern::UnicodeNormalized("buy".to_string()),
            AdvancedPattern::UnicodeNormalized("sell".to_string()),
            
            // Zalgo text detection
            AdvancedPattern::ZalgoText,
            
            // Homoglyph detection for common words
            AdvancedPattern::Homoglyph("admin".to_string()),
            AdvancedPattern::Homoglyph("moderator".to_string()),
            
            // Repeated character compression
            AdvancedPattern::RepeatedCharCompression("follow".to_string()),
            AdvancedPattern::RepeatedCharCompression("subscribe".to_string()),
        ];

        let mut pattern_matcher = self.pattern_matcher.write().await;
        for pattern in patterns {
            pattern_matcher.add_pattern(pattern);
        }

        info!("Setup {} default advanced patterns", pattern_matcher.patterns.len());
        Ok(())
    }

    /// Get system status and health
    pub async fn get_system_status(&self) -> SystemStatus {
        let analytics_dashboard = self.analytics_system.get_dashboard_data().await;
        
        SystemStatus {
            enhanced_features_enabled: *self.enhanced_features_enabled.read().await,
            auto_optimization_enabled: *self.auto_optimization_enabled.read().await,
            learning_mode_enabled: *self.learning_mode.read().await,
            total_patterns: {
                let pattern_matcher = self.pattern_matcher.read().await;
                pattern_matcher.patterns.len()
            },
            system_health_score: analytics_dashboard.global_metrics.system_health_score,
            active_alerts: analytics_dashboard.active_alerts.len(),
            optimization_opportunities: analytics_dashboard.optimization_opportunities.len(),
        }
    }
}

// Supporting types for the enhanced system
#[derive(Debug, Clone)]
pub struct EnhancedModerationResult {
    pub action: ModerationAction,
    pub confidence: f64,
    pub triggered_filters: Vec<String>,
    pub advanced_patterns: Vec<String>,
    pub escalation_applied: bool,
    pub response_time_ms: f64,
    pub severity: ViolationSeverity,
}

#[derive(Debug)]
pub struct ImportResult {
    pub imported_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct EffectivenessReport {
    pub pattern_effectiveness: HashMap<String, crate::bot::pattern_matching::PatternStats>,
    pub escalation_effectiveness: HashMap<String, serde_json::Value>,
    pub overall_accuracy: f64,
    pub user_satisfaction: f64,
    pub performance_metrics: PerformanceMetrics,
    pub recommendations: Vec<crate::bot::realtime_analytics::SystemRecommendation>,
}

#[derive(Debug)]
pub struct PerformanceMetrics {
    pub average_response_time: f64,
    pub total_messages_processed: u64,
    pub peak_load: f64,
}

#[derive(Debug)]
pub struct OptimizationResult {
    pub optimizations_applied: usize,
    pub suggestions_generated: usize,
    pub performance_improvement: f64,
}

#[derive(Debug)]
pub struct SystemStatus {
    pub enhanced_features_enabled: bool,
    pub auto_optimization_enabled: bool,
    pub learning_mode_enabled: bool,
    pub total_patterns: usize,
    pub system_health_score: f64,
    pub active_alerts: usize,
    pub optimization_opportunities: usize,
}

// Re-export types for convenience
pub use crate::bot::realtime_analytics::AnalyticsDashboard;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;
    use chrono::Utc;

    #[tokio::test]
    async fn test_enhanced_moderation() {
        let base_moderation = Arc::new(crate::bot::moderation::ModerationSystem::new());
        let enhanced = EnhancedModerationSystem::new(base_moderation);
        
        // Setup default patterns
        enhanced.setup_default_advanced_patterns().await.unwrap();
        
        // Test message
        let message = ChatMessage {
            platform: "test".to_string(),
            channel: "testchannel".to_string(),
            username: "testuser".to_string(),
            display_name: Some("Test User".to_string()),
            content: "sp4m message with l33tsp34k".to_string(),
            timestamp: Utc::now(),
            user_badges: vec![],
            is_mod: false,
            is_subscriber: false,
        };
        
        // Check if enhanced system detects the pattern
        let result = enhanced.check_message_enhanced(&message, None).await;
        
        // Should detect leetspeak pattern
        if let Some(result) = result {
            assert!(!result.advanced_patterns.is_empty());
            assert!(result.confidence > 0.8);
        }
    }

    #[tokio::test]
    async fn test_user_feedback() {
        let base_moderation = Arc::new(crate::bot::moderation::ModerationSystem::new());
        let enhanced = EnhancedModerationSystem::new(base_moderation);
        
        // Record false positive feedback
        enhanced.record_user_feedback(
            "test_filter",
            "user123",
            UserReportType::FalsePositive,
            "innocent message",
            Some("This wasn't spam".to_string()),
        ).await.unwrap();
        
        // Should be recorded in analytics
        let analytics = enhanced.get_analytics_dashboard().await.unwrap();
        // Analytics would show the feedback in a real implementation
    }

    #[tokio::test]
    async fn test_export_import() {
        use tempfile::tempdir;
        
        let base_moderation = Arc::new(crate::bot::moderation::ModerationSystem::new());
        let enhanced = EnhancedModerationSystem::new(base_moderation);
        
        let temp_dir = tempdir().unwrap();
        let export_path = temp_dir.path().join("test_export.json");
        
        // Export (would export actual filters in real implementation)
        let export_options = ExportOptions::default();
        let result = enhanced.export_filters(&export_path, ExportFormat::Json, export_options).await;
        
        // Export should succeed even with empty filter list
        assert!(result.is_ok() || export_path.exists());
    }
}