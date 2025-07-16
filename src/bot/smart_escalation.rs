use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use log::{debug, info, warn};

use crate::types::{ModerationAction, ChatMessage};
use crate::bot::points::UserPoints;

/// Smart escalation system that considers user history and behavior patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartEscalation {
    /// Base escalation pattern
    pub base_escalation: BaseEscalation,
    /// Weight given to user's historical behavior (0.0-1.0)
    pub history_weight: f32,
    /// Whether to consider community reports
    pub community_reports_enabled: bool,
    /// Time after which violations are "forgiven"
    pub forgiveness_period: Duration,
    /// Whether context matters (different rules for different situations)
    pub context_sensitive: bool,
    /// Rehabilitation program - reduce penalties for reformed users
    pub rehabilitation_enabled: bool,
    /// Minimum violations before smart escalation kicks in
    pub smart_threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseEscalation {
    pub levels: Vec<EscalationLevel>,
    pub offense_window: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLevel {
    pub violation_count: u32,
    pub action: ModerationAction,
    pub description: String,
}

/// Enhanced user behavior profile for smart escalation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBehaviorProfile {
    pub user_id: String,
    pub total_violations: u32,
    pub violation_history: Vec<ViolationEvent>,
    pub behavior_score: f32, // 0.0 (terrible) to 1.0 (excellent)
    pub rehabilitation_progress: f32, // 0.0 to 1.0
    pub community_standing: CommunityStanding,
    pub context_violations: HashMap<String, u32>, // Context -> violation count
    pub positive_actions: Vec<PositiveAction>,
    pub last_violation: Option<DateTime<Utc>>,
    pub account_age: Duration,
    pub watch_time: u64, // minutes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationEvent {
    pub timestamp: DateTime<Utc>,
    pub filter_name: String,
    pub severity: ViolationSeverity,
    pub action_taken: ModerationAction,
    pub context: String, // Channel, game being played, etc.
    pub was_appealed: bool,
    pub appeal_result: Option<AppealResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationSeverity {
    Minor,     // Spam, caps
    Moderate,  // Inappropriate language
    Major,     // Harassment, serious offense
    Severe,    // Hate speech, doxxing
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppealResult {
    Upheld,        // Violation was correct
    Overturned,    // False positive
    Reduced,       // Penalty reduced
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityStanding {
    pub reputation_score: f32, // Based on community interactions
    pub helpful_actions: u32,  // Times user helped others
    pub reports_against: u32,  // Times reported by other users
    pub reports_by: u32,       // Times user reported others (accuracy matters)
    pub report_accuracy: f32,  // Percentage of accurate reports
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositiveAction {
    pub timestamp: DateTime<Utc>,
    pub action_type: PositiveActionType,
    pub impact_score: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)] // Add Copy if the enum is simple enough
pub enum PositiveActionType {
    HelpedNewcomer,
    QualityContent,
    CommunitySupport,
    AccurateReport,
    VoluntaryCompliance,
    Donation,
    LongTermEngagement,
}

impl Default for SmartEscalation {
    fn default() -> Self {
        Self {
            base_escalation: BaseEscalation {
                levels: vec![
                    EscalationLevel {
                        violation_count: 1,
                        action: ModerationAction::WarnUser { 
                            message: "Please follow chat rules (first warning)".to_string() 
                        },
                        description: "First offense warning".to_string(),
                    },
                    EscalationLevel {
                        violation_count: 2,
                        action: ModerationAction::TimeoutUser { duration_seconds: 300 },
                        description: "Short timeout".to_string(),
                    },
                    EscalationLevel {
                        violation_count: 3,
                        action: ModerationAction::TimeoutUser { duration_seconds: 1800 },
                        description: "Extended timeout".to_string(),
                    },
                    EscalationLevel {
                        violation_count: 5,
                        action: ModerationAction::TimeoutUser { duration_seconds: 7200 },
                        description: "Long timeout".to_string(),
                    },
                ],
                offense_window: Duration::hours(24),
            },
            history_weight: 0.3,
            community_reports_enabled: true,
            forgiveness_period: Duration::days(30),
            context_sensitive: true,
            rehabilitation_enabled: true,
            smart_threshold: 3,
        }
    }
}

impl UserBehaviorProfile {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            total_violations: 0,
            violation_history: Vec::new(),
            behavior_score: 0.8, // Start with benefit of doubt
            rehabilitation_progress: 0.0,
            community_standing: CommunityStanding {
                reputation_score: 0.5,
                helpful_actions: 0,
                reports_against: 0,
                reports_by: 0,
                report_accuracy: 0.0,
            },
            context_violations: HashMap::new(),
            positive_actions: Vec::new(),
            last_violation: None,
            account_age: Duration::zero(),
            watch_time: 0,
        }
    }

    /// Update behavior score based on recent actions
    pub fn update_behavior_score(&mut self) {
        let now = Utc::now();
        let recent_cutoff = now - Duration::days(30);
        
        // Count recent violations
        let recent_violations = self.violation_history.iter()
            .filter(|v| v.timestamp > recent_cutoff)
            .count() as f32;
        
        // Count recent positive actions
        let recent_positive = self.positive_actions.iter()
            .filter(|p| p.timestamp > recent_cutoff)
            .map(|p| p.impact_score)
            .sum::<f32>();
        
        // Calculate base score
        let violation_penalty = (recent_violations * 0.1).min(0.5);
        let positive_bonus = (recent_positive * 0.05).min(0.3);
        
        // Factor in account age and watch time
        let account_bonus = (self.account_age.num_days() as f32 / 365.0 * 0.1).min(0.2);
        let engagement_bonus = (self.watch_time as f32 / 10000.0 * 0.1).min(0.1);
        
        // Community standing impact
        let community_bonus = (self.community_standing.reputation_score - 0.5) * 0.2;
        
        self.behavior_score = (0.8 - violation_penalty + positive_bonus + account_bonus + engagement_bonus + community_bonus)
            .max(0.0)
            .min(1.0);
        
        debug!("Updated behavior score for {}: {:.2}", self.user_id, self.behavior_score);
    }

    /// Calculate rehabilitation progress
    pub fn calculate_rehabilitation(&mut self) {
        if let Some(last_violation) = self.last_violation {
            let days_clean = Utc::now().signed_duration_since(last_violation).num_days();
            
            // Rehabilitation increases over time without violations
            self.rehabilitation_progress = (days_clean as f32 / 30.0).min(1.0);
            
            // Positive actions accelerate rehabilitation
            let recent_positive_actions = self.positive_actions.iter()
                .filter(|p| p.timestamp > last_violation)
                .count() as f32;
            
            self.rehabilitation_progress = (self.rehabilitation_progress + recent_positive_actions * 0.1).min(1.0);
        }
    }

    /// Add a positive action to the user's profile
    pub fn add_positive_action(&mut self, action_type: PositiveActionType) {
        let impact_score = match action_type {
            PositiveActionType::HelpedNewcomer => 0.3,
            PositiveActionType::QualityContent => 0.2,
            PositiveActionType::CommunitySupport => 0.4,
            PositiveActionType::AccurateReport => 0.5,
            PositiveActionType::VoluntaryCompliance => 0.6,
            PositiveActionType::Donation => 0.3,
            PositiveActionType::LongTermEngagement => 0.2,
        };

        self.positive_actions.push(PositiveAction {
            timestamp: Utc::now(),
            action_type: action_type.clone(), // Clone before moving
            impact_score,
        });

        // Update scores
        self.update_behavior_score();
        self.calculate_rehabilitation();
        
        info!("Added positive action for {}: {:?} (+{:.2})", self.user_id, action_type, impact_score);
    }

    /// Record a violation in the user's history
    pub fn record_violation(&mut self, violation: ViolationEvent) {
        self.violation_history.push(violation.clone());
        self.total_violations += 1;
        self.last_violation = Some(violation.timestamp);
        
        // Update context-specific violations
        *self.context_violations.entry(violation.context.clone()).or_insert(0) += 1;
        
        // Reset rehabilitation progress
        self.rehabilitation_progress *= 0.5; // Partial reset, not complete
        
        self.update_behavior_score();
        
        warn!("Recorded violation for {}: {} in context {}", 
              self.user_id, violation.filter_name, violation.context);
    }

    /// Get context-specific violation count
    pub fn get_context_violations(&self, context: &str) -> u32 {
        self.context_violations.get(context).copied().unwrap_or(0)
    }
}

/// Smart escalation calculator that goes beyond simple violation counting
pub struct SmartEscalationCalculator {
    config: SmartEscalation,
    user_profiles: HashMap<String, UserBehaviorProfile>,
}

impl SmartEscalationCalculator {
    pub fn new(config: SmartEscalation) -> Self {
        Self {
            config,
            user_profiles: HashMap::new(),
        }
    }

    /// Calculate the appropriate moderation action using smart escalation
    pub fn calculate_action(
        &mut self,
        user_id: &str,
        _filter_name: &str,
        severity: ViolationSeverity,
        context: &str,
        user_points: Option<&UserPoints>,
        message: &ChatMessage,
    ) -> ModerationAction {
        // Create profile if it doesn't exist
        if !self.user_profiles.contains_key(user_id) {
            let mut new_profile = UserBehaviorProfile::new(user_id.to_string());
            
            // Initialize with user points data if available
            if let Some(points) = user_points {
                new_profile.account_age = Utc::now().signed_duration_since(points.first_seen);
                new_profile.watch_time = points.minutes_watched;
            }
            
            self.user_profiles.insert(user_id.to_string(), new_profile);
        }

        // Update profile with current message info
        Self::update_profile_from_message_static(
            self.user_profiles.get_mut(user_id).unwrap(), 
            message
        );

        // Count recent violations and get base action
        let (recent_violations, base_action) = {
            let profile = self.user_profiles.get(user_id).unwrap();
            let cutoff = Utc::now() - self.config.base_escalation.offense_window;
            let recent_violations = profile.violation_history.iter()
                .filter(|v| v.timestamp > cutoff)
                .count() as u32;
            
            let base_action = self.get_base_escalation_action(recent_violations + 1);
            (recent_violations, base_action)
        };

        // Apply smart modifications if threshold is met
        if recent_violations >= self.config.smart_threshold {
            let profile = self.user_profiles.get(user_id).unwrap();
            self.apply_smart_modifications(base_action, profile, &severity, context)
        } else {
            base_action
        }
    }

    // Replace the helper method with this static version:
    fn update_profile_from_message_static(profile: &mut UserBehaviorProfile, message: &ChatMessage) {
        // Update account age if we have better info
        if profile.account_age == Duration::zero() {
            profile.account_age = Duration::days(30); // Default assumption
        }
        
        // Update community standing based on badges
        if message.is_mod {
            profile.community_standing.reputation_score = 
                (profile.community_standing.reputation_score + 0.9) / 2.0;
        }
        
        if message.is_subscriber {
            profile.community_standing.reputation_score = 
                (profile.community_standing.reputation_score + 0.7) / 2.0;
        }
    }

    /// Get base escalation action from configuration
    fn get_base_escalation_action(&self, violation_count: u32) -> ModerationAction {
        for level in self.config.base_escalation.levels.iter().rev() {
            if violation_count >= level.violation_count {
                return level.action.clone();
            }
        }
        
        // Default to first level if no match
        self.config.base_escalation.levels.first()
            .map(|l| l.action.clone())
            .unwrap_or(ModerationAction::WarnUser { 
                message: "Please follow chat rules".to_string() 
            })
    }

    /// Apply smart modifications to the base action
    fn apply_smart_modifications(
        &self,
        base_action: ModerationAction,
        profile: &UserBehaviorProfile,
        severity: &ViolationSeverity,
        context: &str,
    ) -> ModerationAction {
        let mut modification_factor = 1.0;

        // Behavior score influence
        let behavior_modifier = (profile.behavior_score - 0.5) * self.config.history_weight;
        modification_factor += behavior_modifier;

        // Rehabilitation progress reduces penalties
        if self.config.rehabilitation_enabled {
            modification_factor -= profile.rehabilitation_progress * 0.3;
        }

        // Context-sensitive adjustments
        if self.config.context_sensitive {
            let context_violations = profile.get_context_violations(context);
            if context_violations > 5 {
                modification_factor += 0.2; // Escalate for repeat context offenses
            }
        }

        // Community standing influence
        let community_modifier = (profile.community_standing.reputation_score - 0.5) * 0.2;
        modification_factor += community_modifier;

        // Severity-based adjustments
        let severity_modifier = match severity {
            ViolationSeverity::Minor => -0.2,
            ViolationSeverity::Moderate => 0.0,
            ViolationSeverity::Major => 0.3,
            ViolationSeverity::Severe => 0.5,
        };
        modification_factor += severity_modifier;

        // Account age bonus (newer accounts get stricter treatment)
        if profile.account_age < Duration::days(7) {
            modification_factor += 0.2;
        } else if profile.account_age > Duration::days(365) {
            modification_factor -= 0.1;
        }

        // Apply modifications to the base action
        self.modify_action(base_action, modification_factor, profile)
    }

    /// Modify the action based on the calculated factor
    fn modify_action(
        &self,
        base_action: ModerationAction,
        factor: f32,
        profile: &UserBehaviorProfile,
    ) -> ModerationAction {
        match base_action {
            ModerationAction::WarnUser { ref message } => { // Use ref to borrow instead of move
                if factor > 1.3 {
                    // Escalate to timeout
                    ModerationAction::TimeoutUser { duration_seconds: 300 }
                } else if factor < 0.7 && profile.behavior_score > 0.8 {
                    // More lenient warning for good users
                    ModerationAction::WarnUser { 
                        message: format!("{} (You have a good track record - please keep it up!)", message)
                    }
                } else {
                    base_action // Now this works because we didn't move the message
                }
            }
            ModerationAction::TimeoutUser { duration_seconds } => {
                let modified_duration = (duration_seconds as f32 * factor.max(0.1)) as u64;
                
                // Minimum 30 seconds, maximum 24 hours
                let final_duration = modified_duration.max(30).min(86400);
                
                if final_duration != duration_seconds {
                    debug!("Modified timeout duration from {}s to {}s (factor: {:.2})", 
                           duration_seconds, final_duration, factor);
                }
                
                ModerationAction::TimeoutUser { duration_seconds: final_duration }
            }
            other => other, // Don't modify other action types
        }
    }

    /// Record a violation and update user profile
    pub fn record_violation(
        &mut self,
        user_id: &str,
        filter_name: &str,
        severity: ViolationSeverity,
        action_taken: ModerationAction,
        context: &str,
    ) {
        let violation = ViolationEvent {
            timestamp: Utc::now(),
            filter_name: filter_name.to_string(),
            severity,
            action_taken,
            context: context.to_string(),
            was_appealed: false,
            appeal_result: None,
        };

        if let Some(profile) = self.user_profiles.get_mut(user_id) {
            profile.record_violation(violation);
        }
    }

    /// Record a positive action for a user
    pub fn record_positive_action(&mut self, user_id: &str, action_type: PositiveActionType) {
        if let Some(profile) = self.user_profiles.get_mut(user_id) {
            profile.add_positive_action(action_type);
        }
    }

    /// Handle appeal result
    pub fn handle_appeal(&mut self, user_id: &str, violation_index: usize, result: AppealResult) {
        if let Some(profile) = self.user_profiles.get_mut(user_id) {
            if let Some(violation) = profile.violation_history.get_mut(violation_index) {
                violation.was_appealed = true;
                violation.appeal_result = Some(result.clone());
                
                match result {
                    AppealResult::Overturned => {
                        // False positive - improve behavior score significantly
                        profile.behavior_score = (profile.behavior_score + 0.2).min(1.0);
                        profile.add_positive_action(PositiveActionType::VoluntaryCompliance);
                        info!("Appeal overturned for {}: violation was false positive", user_id);
                    }
                    AppealResult::Reduced => {
                        // Partial forgiveness
                        profile.behavior_score = (profile.behavior_score + 0.1).min(1.0);
                        info!("Appeal partially successful for {}: penalty reduced", user_id);
                    }
                    AppealResult::Upheld => {
                        // No change, but mark as reviewed
                        info!("Appeal upheld for {}: violation was correct", user_id);
                    }
                }
            }
        }
    }

    /// Get user behavior profile
    pub fn get_user_profile(&self, user_id: &str) -> Option<&UserBehaviorProfile> {
        self.user_profiles.get(user_id)
    }

    /// Get effectiveness statistics
    pub fn get_effectiveness_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        
        let total_users = self.user_profiles.len();
        let users_with_violations = self.user_profiles.values()
            .filter(|p| !p.violation_history.is_empty())
            .count();
        
        let average_behavior_score = if total_users > 0 {
            self.user_profiles.values()
                .map(|p| p.behavior_score)
                .sum::<f32>() / total_users as f32
        } else {
            0.0
        };
        
        let rehabilitation_active = self.user_profiles.values()
            .filter(|p| p.rehabilitation_progress > 0.1)
            .count();
        
        stats.insert("total_users".to_string(), serde_json::Value::Number(total_users.into()));
        stats.insert("users_with_violations".to_string(), serde_json::Value::Number(users_with_violations.into()));
        
        // Fix: Handle the Number creation properly
        if let Some(score_number) = serde_json::Number::from_f64(average_behavior_score as f64) {
            stats.insert("average_behavior_score".to_string(), serde_json::Value::Number(score_number));
        } else {
            stats.insert("average_behavior_score".to_string(), serde_json::Value::Number(serde_json::Number::from(0)));
        }
        
        stats.insert("rehabilitation_active".to_string(), serde_json::Value::Number(rehabilitation_active.into()));
        
        stats
    }

    /// Clean up old user profiles to prevent memory bloat
    pub fn cleanup_old_profiles(&mut self, cutoff: Duration) {
        let cutoff_time = Utc::now() - cutoff;
        
        self.user_profiles.retain(|_, profile| {
            profile.last_violation.map_or(true, |last| last > cutoff_time) ||
            !profile.positive_actions.is_empty()
        });
    }
}