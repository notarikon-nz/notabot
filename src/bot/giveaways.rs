use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use log::{info, warn, debug};
use rand::{thread_rng, Rng};
use uuid::Uuid;

use crate::types::{ChatMessage, GiveawayType, GiveawaySettings, GiveawayResult, GiveawayError, 
                  UserLevel, ActiveGiveaway, CompletedGiveaway, GiveawayWinner, GiveawayStatus};

/// Main giveaway system that manages all giveaway operations
pub struct GiveawaySystem {
    /// Currently active giveaway (only one at a time)
    active_giveaway: Arc<RwLock<Option<ActiveGiveaway>>>,
    
    /// Historical giveaways for analytics
    giveaway_history: Arc<RwLock<Vec<CompletedGiveaway>>>,
    
    /// Default settings for new giveaways
    default_settings: Arc<RwLock<GiveawaySettings>>,
    
    /// User activity tracking for active user giveaways
    user_activity: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    
    /// AI fraud detection scores (placeholder for now)
    fraud_scores: Arc<RwLock<HashMap<String, f32>>>,
    
    /// Statistics tracking
    statistics: Arc<RwLock<GiveawayStatistics>>,
}

/// Statistics for giveaway system performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GiveawayStatistics {
    pub total_giveaways: u64,
    pub successful_giveaways: u64,
    pub cancelled_giveaways: u64,
    pub total_participants: u64,
    pub total_winners: u64,
    pub average_participants_per_giveaway: f64,
    pub fraud_attempts_blocked: u64,
    pub most_popular_type: Option<String>,
    pub last_updated: DateTime<Utc>,
}

impl GiveawaySystem {
    /// Create a new giveaway system
    pub fn new() -> Self {
        Self {
            active_giveaway: Arc::new(RwLock::new(None)),
            giveaway_history: Arc::new(RwLock::new(Vec::new())),
            default_settings: Arc::new(RwLock::new(GiveawaySettings::default())),
            user_activity: Arc::new(RwLock::new(HashMap::new())),
            fraud_scores: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(GiveawayStatistics::default())),
        }
    }

    /// Start a new giveaway
    pub async fn start_giveaway(
        &self,
        giveaway_type: GiveawayType,
        creator: String,
        channel: String,
        platform: String,
        custom_settings: Option<GiveawaySettings>,
    ) -> GiveawayResult<Uuid> {
        let mut active_guard = self.active_giveaway.write().await;
        
        // Check if there's already an active giveaway
        if active_guard.is_some() {
            return Err(GiveawayError::GiveawayAlreadyActive);
        }

        // Use custom settings or defaults
        let settings = if let Some(custom) = custom_settings {
            custom
        } else {
            self.default_settings.read().await.clone()
        };

        // Validate giveaway configuration
        self.validate_giveaway_config(&giveaway_type, &settings)?;

        // Create new giveaway
        let mut giveaway = ActiveGiveaway::new(
            giveaway_type.clone(),
            settings,
            creator,
            channel,
            platform,
        );

        // Handle special setup for different giveaway types
        match &giveaway_type {
            GiveawayType::RandomNumber { min, max, auto_generate } => {
                if *auto_generate {
                    let mut rng = thread_rng();
                    giveaway.generated_number = Some(rng.gen_range(*min..=*max));
                    info!("Generated random number: {}", giveaway.generated_number.unwrap());
                }
            }
            GiveawayType::Keyword { keyword, .. } => {
                info!("Keyword giveaway started with keyword: '{}'", keyword);
            }
            GiveawayType::ActiveUser { duration_minutes, .. } => {
                info!("Active user giveaway started for {} minutes", duration_minutes);
            }
        }

        // Set status to active
        giveaway.status = GiveawayStatus::Active;
        let giveaway_id = giveaway.id;

        // Store the giveaway
        *active_guard = Some(giveaway);

        // Update statistics
        let mut stats = self.statistics.write().await;
        stats.total_giveaways += 1;
        stats.last_updated = Utc::now();

        info!("Started giveaway {} of type {:?}", giveaway_id, giveaway_type);
        Ok(giveaway_id)
    }

    /// End the current giveaway and select a winner
    pub async fn end_giveaway(&self, force: bool) -> GiveawayResult<Option<GiveawayWinner>> {
        let mut active_guard = self.active_giveaway.write().await;
        
        let mut giveaway = match active_guard.take() {
            Some(g) => g,
            None => return Err(GiveawayError::NoActiveGiveaway),
        };

        // Select winner if not forcing cancellation
        let winner = if force {
            giveaway.cancel(Some("Forced cancellation".to_string()));
            None
        } else {
            match self.select_winner(&mut giveaway).await {
                Ok(winner) => {
                    giveaway.complete_with_winner(winner.clone());
                    Some(winner)
                }
                Err(e) => {
                    warn!("Failed to select winner: {}", e);
                    giveaway.status = GiveawayStatus::Failed;
                    giveaway.end_time = Some(Utc::now());
                    None
                }
            }
        };

        // Move to history
        let completed = CompletedGiveaway::from(giveaway);
        self.giveaway_history.write().await.push(completed.clone());

        // Update statistics
        let mut stats = self.statistics.write().await;
        if completed.success {
            stats.successful_giveaways += 1;
            stats.total_winners += 1;
        } else {
            stats.cancelled_giveaways += 1;
        }
        stats.total_participants += completed.participant_count as u64;
        stats.average_participants_per_giveaway = 
            stats.total_participants as f64 / stats.total_giveaways as f64;
        stats.last_updated = Utc::now();

        info!("Giveaway {} ended. Winner: {:?}", completed.id, winner.as_ref().map(|w| &w.username));
        Ok(winner)
    }

    /// Cancel the current giveaway
    pub async fn cancel_giveaway(&self, reason: Option<String>) -> GiveawayResult<()> {
        self.end_giveaway(true).await?;
        info!("Giveaway cancelled: {}", reason.unwrap_or_else(|| "No reason given".to_string()));
        Ok(())
    }

    /// Process a chat message for giveaway participation
    pub async fn process_message(&self, message: &ChatMessage) -> GiveawayResult<()> {
        // Update user activity tracking
        self.update_user_activity(&message.platform, &message.username).await;

        let mut active_guard = self.active_giveaway.write().await;
        let giveaway = match active_guard.as_mut() {
            Some(g) => g,
            None => return Ok(()), // No active giveaway
        };

        // Skip if not the right platform (unless cross-platform)
        if giveaway.platform != "all" && giveaway.platform != message.platform {
            return Ok(());
        }

        // Skip if not the right channel
        if giveaway.channel != message.channel {
            return Ok(());
        }

        // Determine user level from message
        let user_level = self.determine_user_level(message).await;

        // Check if user is eligible by level
        if !giveaway.is_user_eligible_by_level(&user_level) {
            return Ok(());
        }

        // Get fraud score for user
        let fraud_score = self.get_fraud_score(&message.platform, &message.username).await;
        if fraud_score > giveaway.settings.max_fraud_score {
            debug!("User {} blocked from giveaway due to high fraud score: {}", 
                   message.username, fraud_score);
            return Ok(());
        }

        // Process based on giveaway type
        match &giveaway.giveaway_type {
            GiveawayType::ActiveUser { duration_minutes, min_messages } => {
                // Check if giveaway is still within time limit
                if giveaway.has_timed_out() {
                    return Ok(());
                }

                // For active user giveaways, just mark them as eligible
                giveaway.update_user_eligibility(
                    message.username.clone(),
                    message.platform.clone(),
                    user_level,
                    true,
                );
            }

            GiveawayType::Keyword { keyword, case_sensitive, anti_spam, .. } => {
                let message_text = if *case_sensitive {
                    message.content.clone()
                } else {
                    message.content.to_lowercase()
                };

                let target_keyword = if *case_sensitive {
                    keyword.clone()
                } else {
                    keyword.to_lowercase()
                };

                // Check if message contains the keyword
                if message_text.trim() == target_keyword || message_text.contains(&target_keyword) {
                    let user_key = format!("{}:{}", message.platform, message.username.to_lowercase());
                    
                    // Anti-spam check
                    if *anti_spam && giveaway.keyword_entries.contains_key(&user_key) {
                        debug!("User {} already entered keyword, ignoring repeat", message.username);
                        return Ok(());
                    }

                    // Record keyword entry
                    giveaway.keyword_entries.insert(user_key, Utc::now());

                    // Make user eligible
                    giveaway.update_user_eligibility(
                        message.username.clone(),
                        message.platform.clone(),
                        user_level,
                        true,
                    );

                    info!("User {} entered keyword giveaway", message.username);
                }
            }

            GiveawayType::RandomNumber {   .. } => {
                // Check if user typed the generated number
                if let Some(generated) = giveaway.generated_number {
                    if let Ok(typed_number) = message.content.trim().parse::<u32>() {
                        if typed_number == generated {
                            // Winner found!
                            let mut winner = GiveawayWinner::new(
                                message.username.clone(),
                                message.platform.clone(),
                                user_level,
                                1,
                            );
                            winner.display_name = message.display_name.clone();
                            winner.winning_entry = Some(typed_number.to_string());
                            winner.fraud_score = fraud_score;
                            winner.generate_channel_url();

                            giveaway.complete_with_winner(winner);
                            info!("Random number giveaway won by {} with number {}", 
                                  message.username, typed_number);
                            return Ok(());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Manually toggle a user's eligibility (moderator action)
    pub async fn toggle_user_eligibility(
        &self,
        platform: &str,
        username: &str,
    ) -> GiveawayResult<bool> {
        let mut active_guard = self.active_giveaway.write().await;
        let giveaway = match active_guard.as_mut() {
            Some(g) => g,
            None => return Err(GiveawayError::NoActiveGiveaway),
        };

        let user_key = format!("{}:{}", platform, username.to_lowercase());
        
        if let Some(status) = giveaway.eligible_users.get_mut(&user_key) {
            status.toggle_eligibility();
            info!("Toggled eligibility for {}: {}", username, status.eligible);
            Ok(status.eligible)
        } else {
            // User not in list, add them as eligible
            let user_level = UserLevel::Viewer; // Default level
            giveaway.update_user_eligibility(
                username.to_string(),
                platform.to_string(),
                user_level,
                true,
            );
            
            if let Some(status) = giveaway.eligible_users.get_mut(&user_key) {
                status.manual_override = true;
                info!("Added new user {} as eligible", username);
                Ok(true)
            } else {
                Err(GiveawayError::UserNotEligible {
                    reason: "Failed to add user".to_string(),
                })
            }
        }
    }

    /// Reset all user eligibility
    pub async fn reset_eligibility(&self) -> GiveawayResult<u32> {
        let mut active_guard = self.active_giveaway.write().await;
        let giveaway = match active_guard.as_mut() {
            Some(g) => g,
            None => return Err(GiveawayError::NoActiveGiveaway),
        };

        let previous_count = giveaway.participant_count;
        giveaway.reset_eligibility();

        info!("Reset eligibility for {} users", previous_count);
        Ok(previous_count)
    }

    /// Get current giveaway status
    pub async fn get_giveaway_status(&self) -> Option<GiveawayInfo> {
        let active_guard = self.active_giveaway.read().await;
        active_guard.as_ref().map(|g| GiveawayInfo {
            id: g.id,
            giveaway_type: g.giveaway_type.clone(),
            status: g.status.clone(),
            start_time: g.start_time,
            participant_count: g.participant_count,
            total_entries: g.get_total_weighted_entries(),
            generated_number: g.generated_number,
            creator: g.creator.clone(),
            channel: g.channel.clone(),
            platform: g.platform.clone(),
            eligible_users_count: g.get_eligible_users().len() as u32,
            time_remaining: self.calculate_time_remaining(g),
        })
    }

    /// Get list of eligible users (for UI display)
    pub async fn get_eligible_users(&self) -> Vec<EligibilityInfo> {
        let active_guard = self.active_giveaway.read().await;
        match active_guard.as_ref() {
            Some(giveaway) => {
                giveaway.eligible_users
                    .values()
                    .filter(|status| status.eligible)
                    .map(|status| EligibilityInfo {
                        username: status.username.clone(),
                        display_name: status.display_name.clone(),
                        platform: status.platform.clone(),
                        user_level: status.user_level.clone(),
                        entries: status.weighted_entries(
                            giveaway.settings.subscriber_luck_multiplier,
                            giveaway.settings.regular_luck_multiplier,
                        ),
                        entry_time: status.entry_time,
                        fraud_score: status.fraud_score,
                        manual_override: status.manual_override,
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Get giveaway statistics
    pub async fn get_statistics(&self) -> GiveawayStatistics {
        self.statistics.read().await.clone()
    }

    /// Get giveaway history
    pub async fn get_history(&self, limit: Option<usize>) -> Vec<CompletedGiveaway> {
        let history = self.giveaway_history.read().await;
        let limit = limit.unwrap_or(50);
        history.iter()
            .rev() // Most recent first
            .take(limit)
            .cloned()
            .collect()
    }

    // Private helper methods

    /// Validate giveaway configuration
    fn validate_giveaway_config(
        &self,
        giveaway_type: &GiveawayType,
        settings: &GiveawaySettings,
    ) -> GiveawayResult<()> {
        // Check if eligible user levels is not empty
        if settings.eligible_user_levels.is_empty() {
            return Err(GiveawayError::InvalidConfiguration {
                reason: "No eligible user levels specified".to_string(),
            });
        }

        // Validate based on giveaway type
        match giveaway_type {
            GiveawayType::ActiveUser { duration_minutes, .. } => {
                if *duration_minutes == 0 || *duration_minutes > 1440 { // Max 24 hours
                    return Err(GiveawayError::InvalidConfiguration {
                        reason: "Duration must be between 1 and 1440 minutes".to_string(),
                    });
                }
            }
            GiveawayType::Keyword { keyword, .. } => {
                if keyword.trim().is_empty() {
                    return Err(GiveawayError::InvalidConfiguration {
                        reason: "Keyword cannot be empty".to_string(),
                    });
                }
                if keyword.len() > 100 {
                    return Err(GiveawayError::InvalidConfiguration {
                        reason: "Keyword too long (max 100 characters)".to_string(),
                    });
                }
            }
            GiveawayType::RandomNumber { min, max, .. } => {
                if min >= max {
                    return Err(GiveawayError::InvalidConfiguration {
                        reason: "Minimum must be less than maximum".to_string(),
                    });
                }
                if *max - *min > 1000000 {
                    return Err(GiveawayError::InvalidConfiguration {
                        reason: "Range too large (max 1,000,000)".to_string(),
                    });
                }
            }
        }

        // Validate luck multipliers
        if settings.subscriber_luck_multiplier < 1.0 || settings.subscriber_luck_multiplier > 10.0 {
            return Err(GiveawayError::InvalidConfiguration {
                reason: "Subscriber luck multiplier must be between 1.0 and 10.0".to_string(),
            });
        }

        if settings.regular_luck_multiplier < 1.0 || settings.regular_luck_multiplier > 10.0 {
            return Err(GiveawayError::InvalidConfiguration {
                reason: "Regular luck multiplier must be between 1.0 and 10.0".to_string(),
            });
        }

        Ok(())
    }

    /// Select a winner from eligible users
    async fn select_winner(&self, giveaway: &mut ActiveGiveaway) -> GiveawayResult<GiveawayWinner> {
        let eligible_users = giveaway.get_eligible_users();
        
        if eligible_users.is_empty() {
            return Err(GiveawayError::WinnerSelectionFailed {
                reason: "No eligible users".to_string(),
            });
        }

        // For random number giveaways, winner should already be determined
        if let GiveawayType::RandomNumber { .. } = &giveaway.giveaway_type {
            if let Some(winner) = &giveaway.winner {
                return Ok(winner.clone());
            }
        }

        // Weighted random selection
        let total_entries = giveaway.get_total_weighted_entries();
        if total_entries == 0 {
            return Err(GiveawayError::WinnerSelectionFailed {
                reason: "No valid entries".to_string(),
            });
        }

        let mut rng = thread_rng();
        let winning_number = rng.gen_range(1..=total_entries);
        let mut current_total = 0;

        for status in eligible_users {
            let user_entries = status.weighted_entries(
                giveaway.settings.subscriber_luck_multiplier,
                giveaway.settings.regular_luck_multiplier,
            );
            current_total += user_entries;

            if current_total >= winning_number {
                // Found the winner!
                let mut winner = GiveawayWinner::new(
                    status.username.clone(),
                    status.platform.clone(),
                    status.user_level.clone(),
                    user_entries,
                );
                
                winner.display_name = status.display_name.clone();
                winner.fraud_score = status.fraud_score;
                winner.generate_channel_url();

                info!("Selected winner: {} with {} entries (rolled {})", 
                      winner.username, user_entries, winning_number);
                return Ok(winner);
            }
        }

        // This shouldn't happen, but just in case
        Err(GiveawayError::WinnerSelectionFailed {
            reason: "Random selection algorithm failed".to_string(),
        })
    }

    /// Update user activity tracking
    async fn update_user_activity(&self, platform: &str, username: &str) {
        let user_key = format!("{}:{}", platform, username.to_lowercase());
        let mut activity = self.user_activity.write().await;
        activity.insert(user_key, Utc::now());
    }

    /// Determine user level from chat message
    async fn determine_user_level(&self, message: &ChatMessage) -> UserLevel {
        // Check badges for platform-specific levels
        if message.user_badges.contains(&"broadcaster".to_string()) ||
           message.user_badges.contains(&"owner".to_string()) {
            return UserLevel::Owner;
        }

        if message.is_mod {
            return UserLevel::Moderator;
        }

        if message.user_badges.contains(&"vip".to_string()) {
            return UserLevel::VIP;
        }

        if message.is_subscriber {
            return UserLevel::Subscriber;
        }

        // Check if user is a "regular" based on activity/points
        // This would integrate with the points system
        if self.is_regular_user(&message.platform, &message.username).await {
            return UserLevel::Regular;
        }

        UserLevel::Viewer
    }

    /// Check if user is considered a "regular" (placeholder implementation)
    async fn is_regular_user(&self, _platform: &str, _username: &str) -> bool {
        // TODO: Integrate with points system to determine regulars
        // For now, return false - this would check user points/activity
        false
    }

    /// Get fraud score for user (placeholder implementation)
    async fn get_fraud_score(&self, platform: &str, username: &str) -> f32 {
        let user_key = format!("{}:{}", platform, username.to_lowercase());
        let scores = self.fraud_scores.read().await;
        scores.get(&user_key).copied().unwrap_or(0.0)
    }

    /// Calculate time remaining for active user giveaways
    fn calculate_time_remaining(&self, giveaway: &ActiveGiveaway) -> Option<i64> {
        match &giveaway.giveaway_type {
            GiveawayType::ActiveUser { duration_minutes, .. } => {
                let duration = chrono::Duration::minutes(*duration_minutes as i64);
                let elapsed = Utc::now().signed_duration_since(giveaway.start_time);
                let remaining = duration - elapsed;
                Some(remaining.num_seconds().max(0))
            }
            _ => None,
        }
    }

    /// Update default settings
    pub async fn update_default_settings(&self, settings: GiveawaySettings) -> GiveawayResult<()> {
        // Validate settings first
        self.validate_giveaway_config(&GiveawayType::ActiveUser { duration_minutes: 10, min_messages: None }, &settings)?;
        
        *self.default_settings.write().await = settings;
        info!("Updated default giveaway settings");
        Ok(())
    }

    /// Set fraud score for a user (for AI integration)
    pub async fn set_fraud_score(&self, platform: &str, username: &str, score: f32) {
        let user_key = format!("{}:{}", platform, username.to_lowercase());
        let mut scores = self.fraud_scores.write().await;
        scores.insert(user_key, score.clamp(0.0, 1.0));
    }

    /// Generate random number for random number giveaway
    pub async fn generate_random_number(&self, min: u32, max: u32) -> GiveawayResult<u32> {
        let mut active_guard = self.active_giveaway.write().await;
        let giveaway = match active_guard.as_mut() {
            Some(g) => g,
            None => return Err(GiveawayError::NoActiveGiveaway),
        };

        if !matches!(giveaway.giveaway_type, GiveawayType::RandomNumber { .. }) {
            return Err(GiveawayError::InvalidConfiguration {
                reason: "Not a random number giveaway".to_string(),
            });
        }

        let mut rng = thread_rng();
        let number = rng.gen_range(min..=max);
        giveaway.generated_number = Some(number);

        info!("Generated random number for giveaway: {}", number);
        Ok(number)
    }
}

/// Information about the current giveaway for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiveawayInfo {
    pub id: Uuid,
    pub giveaway_type: GiveawayType,
    pub status: GiveawayStatus,
    pub start_time: DateTime<Utc>,
    pub participant_count: u32,
    pub total_entries: u32,
    pub generated_number: Option<u32>,
    pub creator: String,
    pub channel: String,
    pub platform: String,
    pub eligible_users_count: u32,
    pub time_remaining: Option<i64>, // Seconds remaining for timed giveaways
}

/// Information about an eligible user for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EligibilityInfo {
    pub username: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub user_level: UserLevel,
    pub entries: u32,
    pub entry_time: Option<DateTime<Utc>>,
    pub fraud_score: f32,
    pub manual_override: bool,
}

// Default implementation
impl Default for GiveawaySystem {
    fn default() -> Self {
        Self::new()
    }
}