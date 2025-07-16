use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;
use std::sync::Arc;
use log::{info, debug, warn};
use chrono::Timelike;

/// Real-time analytics for filter performance and effectiveness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterAnalytics {
    pub filter_id: String,
    pub filter_type: String,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    
    // Performance metrics
    pub total_triggers: u64,
    pub true_positives: u64,
    pub false_positives: u64,
    pub false_negatives: u64, // Reported by users/mods
    
    // Timing metrics
    pub average_response_time_ms: f64,
    pub peak_response_time_ms: f64,
    pub response_time_history: VecDeque<f64>,
    
    // Effectiveness metrics
    pub accuracy: f64,        // (TP) / (TP + FP)
    pub precision: f64,       // TP / (TP + FP)
    pub recall: f64,          // TP / (TP + FN)
    pub f1_score: f64,        // Harmonic mean of precision and recall
    
    // Trend analysis
    pub hourly_triggers: VecDeque<HourlyMetric>,
    pub pattern_evolution: Vec<PatternChange>,
    
    // User feedback
    pub user_reports: Vec<UserReport>,
    pub moderator_reviews: Vec<ModeratorReview>,
    
    // Resource usage
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    
    // Adaptive suggestions
    pub optimization_suggestions: Vec<OptimizationSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyMetric {
    pub timestamp: DateTime<Utc>,
    pub triggers: u32,
    pub false_positives: u32,
    pub response_time_avg: f64,
    pub effectiveness_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternChange {
    pub timestamp: DateTime<Utc>,
    pub change_type: PatternChangeType,
    pub description: String,
    pub impact_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternChangeType {
    NewEvasionTechnique,    // Users found new ways around the filter
    LanguageEvolution,      // Natural language changes
    ContextShift,           // Filter effectiveness changed in different contexts
    ParameterDrift,         // Need to adjust thresholds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReport {
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub report_type: UserReportType,
    pub message_content: String,
    pub user_explanation: Option<String>,
    pub resolved: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserReportType {
    FalsePositive,         // "This shouldn't have been flagged"
    MissedViolation,       // "This should have been caught"
    TooHarsh,             // "Punishment was too severe"
    Appeal,               // "I want to appeal this decision"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeratorReview {
    pub timestamp: DateTime<Utc>,
    pub moderator_id: String,
    pub review_type: ModeratorReviewType,
    pub filter_accuracy: f32, // 0.0-1.0
    pub suggested_changes: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModeratorReviewType {
    RoutineAudit,
    FalsePositiveReview,
    EffectivenessCheck,
    UserComplaintReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub suggestion_type: OptimizationType,
    pub confidence: f64,
    pub description: String,
    pub expected_improvement: f64,
    pub implementation_difficulty: Difficulty,
    pub auto_implementable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationType {
    AdjustThreshold,
    AddException,
    ImprovePattern,
    ReduceScope,
    IncreaseScope,
    ReplaceAlgorithm,
    RemoveFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Difficulty {
    Trivial,     // Auto-implementable
    Easy,        // Simple config change
    Moderate,    // Requires testing
    Hard,        // Significant changes needed
    Expert,      // Requires specialized knowledge
}

/// Real-time analytics system for monitoring filter performance
pub struct FilterAnalyticsSystem {
    analytics: Arc<RwLock<HashMap<String, FilterAnalytics>>>,
    global_metrics: Arc<RwLock<GlobalMetrics>>,
    alert_thresholds: AlertThresholds,
    optimization_engine: Arc<RwLock<OptimizationEngine>>, // This should be wrapped
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMetrics {
    pub total_messages_processed: u64,
    pub total_violations_detected: u64,
    pub overall_accuracy: f64,
    pub system_health_score: f64,
    pub peak_load_messages_per_second: f64,
    pub current_load: f64,
    pub filters_auto_optimized: u32,
    pub user_satisfaction_score: f64, // Based on reports and feedback
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub accuracy_warning: f64,      // Warn if accuracy drops below this
    pub accuracy_critical: f64,     // Critical alert
    pub false_positive_rate: f64,   // Alert if FP rate exceeds this
    pub response_time_ms: f64,      // Alert if response time exceeds this
    pub user_complaint_rate: f64,   // Alert if complaints exceed this rate
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            accuracy_warning: 0.85,
            accuracy_critical: 0.70,
            false_positive_rate: 0.15,
            response_time_ms: 50.0,
            user_complaint_rate: 0.05, // 5% of users complaining
        }
    }
}

impl FilterAnalyticsSystem {
    pub fn new() -> Self {
        Self {
            analytics: Arc::new(RwLock::new(HashMap::new())),
            global_metrics: Arc::new(RwLock::new(GlobalMetrics {
                total_messages_processed: 0,
                total_violations_detected: 0,
                overall_accuracy: 1.0,
                system_health_score: 1.0,
                peak_load_messages_per_second: 0.0,
                current_load: 0.0,
                filters_auto_optimized: 0,
                user_satisfaction_score: 0.8,
            })),
            alert_thresholds: AlertThresholds::default(),
            optimization_engine: Arc::new(RwLock::new(OptimizationEngine::new())), // Wrap in Arc<RwLock<>>        
        }
    }

    /// Record a filter trigger event
    pub async fn record_trigger(
        &self,  // &self instead of &mut self
        filter_id: &str,
        filter_type: &str,
        is_true_positive: bool,
        response_time_ms: f64,
        _message_content: &str,
    ) {
        let mut analytics = self.analytics.write().await;
        let filter_analytics = analytics.entry(filter_id.to_string())
            .or_insert_with(|| FilterAnalytics::new(filter_id, filter_type));

        // Update basic metrics
        filter_analytics.total_triggers += 1;
        if is_true_positive {
            filter_analytics.true_positives += 1;
        } else {
            filter_analytics.false_positives += 1;
        }

        // Update timing metrics
        filter_analytics.response_time_history.push_back(response_time_ms);
        if filter_analytics.response_time_history.len() > 1000 {
            filter_analytics.response_time_history.pop_front();
        }
        
        filter_analytics.average_response_time_ms = 
            filter_analytics.response_time_history.iter().sum::<f64>() / 
            filter_analytics.response_time_history.len() as f64;
        
        filter_analytics.peak_response_time_ms = 
            filter_analytics.peak_response_time_ms.max(response_time_ms);

        // Update effectiveness metrics
        filter_analytics.update_effectiveness_metrics();

        // Update hourly metrics
        filter_analytics.update_hourly_metrics();

        // Check for alerts
        self.check_alerts(filter_analytics).await;

        // Generate optimization suggestions
        if filter_analytics.total_triggers % 100 == 0 {
            let mut opt_engine = self.optimization_engine.write().await;
            opt_engine.generate_suggestions(filter_analytics).await;
        }
        // Update global metrics
        let mut global = self.global_metrics.write().await;
        global.total_messages_processed += 1;
        if is_true_positive {
            global.total_violations_detected += 1;
        }
        
        drop(analytics);
        drop(global);
        
        debug!("Recorded trigger for filter '{}': TP={}, RT={:.2}ms", 
               filter_id, is_true_positive, response_time_ms);
    }

    /// Record user feedback
    pub async fn record_user_report(
        &self,
        filter_id: &str,
        user_id: &str,
        report_type: UserReportType,
        message_content: &str,
        explanation: Option<String>,
    ) {
        let mut analytics = self.analytics.write().await;
        if let Some(filter_analytics) = analytics.get_mut(filter_id) {
            let report = UserReport {
                timestamp: Utc::now(),
                user_id: user_id.to_string(),
                report_type: report_type.clone(),
                message_content: message_content.to_string(),
                user_explanation: explanation,
                resolved: false,
                resolution: None,
            };

            filter_analytics.user_reports.push(report);

            // Update metrics based on report type
            match report_type {
                UserReportType::FalsePositive => {
                    filter_analytics.false_positives += 1;
                    if filter_analytics.true_positives > 0 {
                        filter_analytics.true_positives -= 1;
                    }
                }
                UserReportType::MissedViolation => {
                    filter_analytics.false_negatives += 1;
                }
                _ => {}
            }

            filter_analytics.update_effectiveness_metrics();
            
            info!("User report recorded for filter '{}': {:?}", filter_id, report_type);
        }
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
    ) {
        let mut analytics = self.analytics.write().await;
        if let Some(filter_analytics) = analytics.get_mut(filter_id) {
            let review = ModeratorReview {
                timestamp: Utc::now(),
                moderator_id: moderator_id.to_string(),
                review_type,
                filter_accuracy: accuracy_rating,
                suggested_changes: suggestions,
                notes,
            };

            filter_analytics.moderator_reviews.push(review);
            
            info!("Moderator review recorded for filter '{}' by '{}': {:.2}", 
                  filter_id, moderator_id, accuracy_rating);
        }
    }

    /// Get real-time analytics for a specific filter
    pub async fn get_filter_analytics(&self, filter_id: &str) -> Option<FilterAnalytics> {
        self.analytics.read().await.get(filter_id).cloned()
    }

    /// Get comprehensive analytics dashboard data
    pub async fn get_dashboard_data(&self) -> AnalyticsDashboard {
        let analytics = self.analytics.read().await;
        let global = self.global_metrics.read().await;

        let filter_summaries: Vec<FilterSummary> = analytics.values()
            .map(|fa| FilterSummary {
                filter_id: fa.filter_id.clone(),
                filter_type: fa.filter_type.clone(),
                accuracy: fa.accuracy,
                total_triggers: fa.total_triggers,
                false_positive_rate: fa.false_positives as f64 / fa.total_triggers.max(1) as f64,
                average_response_time: fa.average_response_time_ms,
                health_status: if fa.accuracy > 0.9 { 
                    HealthStatus::Excellent 
                } else if fa.accuracy > 0.8 { 
                    HealthStatus::Good 
                } else if fa.accuracy > 0.7 { 
                    HealthStatus::Warning 
                } else { 
                    HealthStatus::Critical 
                },
                last_updated: fa.last_updated,
            })
            .collect();

        AnalyticsDashboard {
            global_metrics: global.clone(),
            filter_summaries,
            active_alerts: self.get_active_alerts().await,
            optimization_opportunities: self.get_optimization_opportunities().await,
            system_recommendations: self.generate_system_recommendations().await,
        }
    }

    /// Check for alert conditions
    async fn check_alerts(&self, filter_analytics: &FilterAnalytics) {
        let mut alerts = Vec::new();

        // Accuracy alerts
        if filter_analytics.accuracy < self.alert_thresholds.accuracy_critical {
            alerts.push(Alert {
                severity: AlertSeverity::Critical,
                filter_id: filter_analytics.filter_id.clone(),
                message: format!("Filter accuracy critically low: {:.1}%", filter_analytics.accuracy * 100.0),
                action_required: true,
            });
        } else if filter_analytics.accuracy < self.alert_thresholds.accuracy_warning {
            alerts.push(Alert {
                severity: AlertSeverity::Warning,
                filter_id: filter_analytics.filter_id.clone(),
                message: format!("Filter accuracy below threshold: {:.1}%", filter_analytics.accuracy * 100.0),
                action_required: false,
            });
        }

        // False positive rate alerts
        let fp_rate = filter_analytics.false_positives as f64 / filter_analytics.total_triggers.max(1) as f64;
        if fp_rate > self.alert_thresholds.false_positive_rate {
            alerts.push(Alert {
                severity: AlertSeverity::Warning,
                filter_id: filter_analytics.filter_id.clone(),
                message: format!("High false positive rate: {:.1}%", fp_rate * 100.0),
                action_required: true,
            });
        }

        // Response time alerts
        if filter_analytics.average_response_time_ms > self.alert_thresholds.response_time_ms {
            alerts.push(Alert {
                severity: AlertSeverity::Performance,
                filter_id: filter_analytics.filter_id.clone(),
                message: format!("Slow response time: {:.1}ms", filter_analytics.average_response_time_ms),
                action_required: false,
            });
        }

        // Log alerts
        for alert in alerts {
            match alert.severity {
                AlertSeverity::Critical => warn!("CRITICAL ALERT: {}", alert.message),
                AlertSeverity::Warning => warn!("WARNING: {}", alert.message),
                AlertSeverity::Performance => info!("PERFORMANCE: {}", alert.message),
                AlertSeverity::Info => debug!("INFO: {}", alert.message),
            }
        }
    }

    /// Get currently active alerts
    async fn get_active_alerts(&self) -> Vec<Alert> {
        let analytics = self.analytics.read().await;
        let mut alerts = Vec::new();

        for filter_analytics in analytics.values() {
            // Check various alert conditions
            if filter_analytics.accuracy < self.alert_thresholds.accuracy_warning {
                alerts.push(Alert {
                    severity: if filter_analytics.accuracy < self.alert_thresholds.accuracy_critical {
                        AlertSeverity::Critical
                    } else {
                        AlertSeverity::Warning
                    },
                    filter_id: filter_analytics.filter_id.clone(),
                    message: format!("Low accuracy: {:.1}%", filter_analytics.accuracy * 100.0),
                    action_required: true,
                });
            }

            let fp_rate = filter_analytics.false_positives as f64 / filter_analytics.total_triggers.max(1) as f64;
            if fp_rate > self.alert_thresholds.false_positive_rate {
                alerts.push(Alert {
                    severity: AlertSeverity::Warning,
                    filter_id: filter_analytics.filter_id.clone(),
                    message: format!("High false positive rate: {:.1}%", fp_rate * 100.0),
                    action_required: true,
                });
            }
        }

        alerts
    }

    /// Get optimization opportunities
    async fn get_optimization_opportunities(&self) -> Vec<OptimizationOpportunity> {
        let analytics = self.analytics.read().await;
        let mut opportunities = Vec::new();

        for filter_analytics in analytics.values() {
            // Low accuracy filters
            if filter_analytics.accuracy < 0.85 && filter_analytics.total_triggers > 50 {
                opportunities.push(OptimizationOpportunity {
                    filter_id: filter_analytics.filter_id.clone(),
                    opportunity_type: OptimizationOpportunityType::ImproveAccuracy,
                    potential_improvement: (0.95 - filter_analytics.accuracy) * 100.0,
                    description: "Filter accuracy can be improved through pattern refinement".to_string(),
                    estimated_effort: "Medium".to_string(),
                });
            }

            // High false positive rate
            let fp_rate = filter_analytics.false_positives as f64 / filter_analytics.total_triggers.max(1) as f64;
            if fp_rate > 0.2 {
                opportunities.push(OptimizationOpportunity {
                    filter_id: filter_analytics.filter_id.clone(),
                    opportunity_type: OptimizationOpportunityType::ReduceFalsePositives,
                    potential_improvement: (fp_rate - 0.1) * 100.0,
                    description: "Add exemptions or refine patterns to reduce false positives".to_string(),
                    estimated_effort: "Low".to_string(),
                });
            }

            // Slow response time
            if filter_analytics.average_response_time_ms > 25.0 {
                opportunities.push(OptimizationOpportunity {
                    filter_id: filter_analytics.filter_id.clone(),
                    opportunity_type: OptimizationOpportunityType::ImprovePerformance,
                    potential_improvement: filter_analytics.average_response_time_ms - 10.0,
                    description: "Optimize pattern matching algorithm for better performance".to_string(),
                    estimated_effort: "High".to_string(),
                });
            }
        }

        opportunities
    }

    /// Generate system-wide recommendations
    async fn generate_system_recommendations(&self) -> Vec<SystemRecommendation> {
        let analytics = self.analytics.read().await;
        let global = self.global_metrics.read().await;
        let mut recommendations = Vec::new();

        // Overall system health
        if global.overall_accuracy < 0.8 {
            recommendations.push(SystemRecommendation {
                priority: RecommendationPriority::High,
                category: "Accuracy".to_string(),
                title: "System-wide accuracy needs improvement".to_string(),
                description: "Multiple filters showing low accuracy. Consider comprehensive review.".to_string(),
                action_items: vec![
                    "Review filter patterns and thresholds".to_string(),
                    "Analyze user feedback patterns".to_string(),
                    "Consider implementing advanced pattern matching".to_string(),
                ],
            });
        }

        // Performance recommendations
        let avg_response_time: f64 = analytics.values()
            .map(|a| a.average_response_time_ms)
            .sum::<f64>() / analytics.len().max(1) as f64;

        if avg_response_time > 30.0 {
            recommendations.push(SystemRecommendation {
                priority: RecommendationPriority::Medium,
                category: "Performance".to_string(),
                title: "Consider performance optimizations".to_string(),
                description: format!("Average response time is {:.1}ms, consider optimization", avg_response_time),
                action_items: vec![
                    "Profile slow filters".to_string(),
                    "Consider caching strategies".to_string(),
                    "Optimize regex patterns".to_string(),
                ],
            });
        }

        // User satisfaction recommendations
        if global.user_satisfaction_score < 0.7 {
            recommendations.push(SystemRecommendation {
                priority: RecommendationPriority::High,
                category: "User Experience".to_string(),
                title: "User satisfaction needs attention".to_string(),
                description: "High number of user complaints or appeals".to_string(),
                action_items: vec![
                    "Review recent user reports".to_string(),
                    "Consider more lenient escalation".to_string(),
                    "Improve transparency of moderation actions".to_string(),
                ],
            });
        }

        recommendations
    }
}

impl FilterAnalytics {
    pub fn new(filter_id: &str, filter_type: &str) -> Self {
        let now = Utc::now();
        Self {
            filter_id: filter_id.to_string(),
            filter_type: filter_type.to_string(),
            created_at: now,
            last_updated: now,
            total_triggers: 0,
            true_positives: 0,
            false_positives: 0,
            false_negatives: 0,
            average_response_time_ms: 0.0,
            peak_response_time_ms: 0.0,
            response_time_history: VecDeque::new(),
            accuracy: 1.0,
            precision: 1.0,
            recall: 1.0,
            f1_score: 1.0,
            hourly_triggers: VecDeque::new(),
            pattern_evolution: Vec::new(),
            user_reports: Vec::new(),
            moderator_reviews: Vec::new(),
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
            optimization_suggestions: Vec::new(),
        }
    }

    pub fn update_effectiveness_metrics(&mut self) {
        self.last_updated = Utc::now();

        if self.total_triggers == 0 {
            return;
        }

        // Precision: TP / (TP + FP)
        self.precision = if self.true_positives + self.false_positives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_positives) as f64
        } else {
            1.0
        };

        // Recall: TP / (TP + FN)
        self.recall = if self.true_positives + self.false_negatives > 0 {
            self.true_positives as f64 / (self.true_positives + self.false_negatives) as f64
        } else {
            1.0
        };

        // F1 Score: Harmonic mean of precision and recall
        self.f1_score = if self.precision + self.recall > 0.0 {
            2.0 * (self.precision * self.recall) / (self.precision + self.recall)
        } else {
            0.0
        };

        // Accuracy: (TP + TN) / (TP + TN + FP + FN)
        // Note: We don't track TN (true negatives) as it's impractical for spam filtering
        // So we use precision as a proxy for accuracy
        self.accuracy = self.precision;
    }

    pub fn update_hourly_metrics(&mut self) {
        let now = Utc::now();
        let current_hour = now.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();

        // Check if we need to add a new hourly metric
        if self.hourly_triggers.is_empty() || 
           self.hourly_triggers.back().unwrap().timestamp < current_hour {
            
            let metric = HourlyMetric {
                timestamp: current_hour,
                triggers: 1,
                false_positives: if self.false_positives > 0 { 1 } else { 0 },
                response_time_avg: self.average_response_time_ms,
                effectiveness_score: self.accuracy,
            };
            
            self.hourly_triggers.push_back(metric);
            
            // Keep only last 168 hours (7 days)
            while self.hourly_triggers.len() > 168 {
                self.hourly_triggers.pop_front();
            }
        } else {
            // Update current hour
            if let Some(current_metric) = self.hourly_triggers.back_mut() {
                current_metric.triggers += 1;
                current_metric.response_time_avg = self.average_response_time_ms;
                current_metric.effectiveness_score = self.accuracy;
            }
        }
    }
}

/// Optimization engine for automatic filter improvement
pub struct OptimizationEngine {
    suggestions_cache: HashMap<String, Vec<OptimizationSuggestion>>,
}

impl OptimizationEngine {
    pub fn new() -> Self {
        Self {
            suggestions_cache: HashMap::new(),
        }
    }

    pub async fn generate_suggestions(&mut self, filter_analytics: &mut FilterAnalytics) {
        let mut suggestions = Vec::new();

        // Low accuracy suggestions
        if filter_analytics.accuracy < 0.8 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: OptimizationType::ImprovePattern,
                confidence: 0.8,
                description: "Pattern refinement needed - high false positive rate detected".to_string(),
                expected_improvement: (0.9 - filter_analytics.accuracy) * 100.0,
                implementation_difficulty: Difficulty::Moderate,
                auto_implementable: false,
            });
        }

        // High false positive rate
        let fp_rate = filter_analytics.false_positives as f64 / filter_analytics.total_triggers.max(1) as f64;
        if fp_rate > 0.15 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: OptimizationType::AddException,
                confidence: 0.9,
                description: format!("Add exemptions for common false positives (current rate: {:.1}%)", fp_rate * 100.0),
                expected_improvement: (fp_rate - 0.1) * 100.0,
                implementation_difficulty: Difficulty::Easy,
                auto_implementable: true,
            });
        }

        // Performance suggestions
        if filter_analytics.average_response_time_ms > 50.0 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: OptimizationType::ReplaceAlgorithm,
                confidence: 0.7,
                description: "Consider faster pattern matching algorithm".to_string(),
                expected_improvement: filter_analytics.average_response_time_ms - 20.0,
                implementation_difficulty: Difficulty::Hard,
                auto_implementable: false,
            });
        }

        // Low usage suggestions
        if filter_analytics.total_triggers < 10 && 
           Utc::now().signed_duration_since(filter_analytics.created_at).num_days() > 7 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: OptimizationType::RemoveFilter,
                confidence: 0.6,
                description: "Filter has very low usage - consider removal".to_string(),
                expected_improvement: 0.0,
                implementation_difficulty: Difficulty::Trivial,
                auto_implementable: true,
            });
        }

        filter_analytics.optimization_suggestions = suggestions.clone();
        self.suggestions_cache.insert(filter_analytics.filter_id.clone(), suggestions);
    }
}

// Additional supporting types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub filter_id: String,
    pub message: String,
    pub action_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Performance,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsDashboard {
    pub global_metrics: GlobalMetrics,
    pub filter_summaries: Vec<FilterSummary>,
    pub active_alerts: Vec<Alert>,
    pub optimization_opportunities: Vec<OptimizationOpportunity>,
    pub system_recommendations: Vec<SystemRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSummary {
    pub filter_id: String,
    pub filter_type: String,
    pub accuracy: f64,
    pub total_triggers: u64,
    pub false_positive_rate: f64,
    pub average_response_time: f64,
    pub health_status: HealthStatus,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Excellent,
    Good,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationOpportunity {
    pub filter_id: String,
    pub opportunity_type: OptimizationOpportunityType,
    pub potential_improvement: f64,
    pub description: String,
    pub estimated_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizationOpportunityType {
    ImproveAccuracy,
    ReduceFalsePositives,
    ImprovePerformance,
    ReduceComplexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRecommendation {
    pub priority: RecommendationPriority,
    pub category: String,
    pub title: String,
    pub description: String,
    pub action_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_analytics_recording() {
        let analytics_system = FilterAnalyticsSystem::new();
        
        // Record some events
        analytics_system.record_trigger(
            "test_filter",
            "blacklist",
            true,
            15.5,
            "test message"
        ).await;
        
        analytics_system.record_trigger(
            "test_filter",
            "blacklist",
            false,
            12.3,
            "false positive"
        ).await;
        
        // Check analytics
        let filter_analytics = analytics_system.get_filter_analytics("test_filter").await;
        assert!(filter_analytics.is_some());
        
        let fa = filter_analytics.unwrap();
        assert_eq!(fa.total_triggers, 2);
        assert_eq!(fa.true_positives, 1);
        assert_eq!(fa.false_positives, 1);
        assert_eq!(fa.precision, 0.5); // 1 TP / (1 TP + 1 FP)
    }

    #[tokio::test]
    async fn test_user_reporting() {
        let analytics_system = FilterAnalyticsSystem::new();
        
        // First record a trigger
        analytics_system.record_trigger(
            "test_filter",
            "blacklist",
            true,
            10.0,
            "test message"
        ).await;
        
        // Then record a user report
        analytics_system.record_user_report(
            "test_filter",
            "user123",
            UserReportType::FalsePositive,
            "test message",
            Some("This wasn't spam".to_string())
        ).await;
        
        let filter_analytics = analytics_system.get_filter_analytics("test_filter").await.unwrap();
        assert_eq!(filter_analytics.user_reports.len(), 1);
        assert_eq!(filter_analytics.false_positives, 1);
    }
}