// src/bot/moderation.rs - Enhanced moderation system with NightBot parity

use anyhow::Result;
use log::{error, info, warn, debug};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{
    ChatMessage, SpamFilter, SpamFilterType, ModerationAction, ModerationEscalation,
    UserMessageHistory, BlacklistPattern, ExemptionLevel, ViolationRecord
};
use crate::bot::points::UserPoints;

pub struct ModerationSystem {
    pub spam_filters: Arc<RwLock<HashMap<String, SpamFilter>>>,
    pub user_message_history: Arc<RwLock<HashMap<String, UserMessageHistory>>>,
    pub global_enabled: Arc<RwLock<bool>>,
}

impl ModerationSystem {
    pub fn new() -> Self {
        Self {
            spam_filters: Arc::new(RwLock::new(HashMap::new())),
            user_message_history: Arc::new(RwLock::new(HashMap::new())),
            global_enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Add a spam filter with default configuration
    pub async fn add_spam_filter(&self, filter_type: SpamFilterType) -> Result<()> {
        let filter_name = Self::generate_filter_name(&filter_type);
        let filter = SpamFilter {
            filter_type: filter_type.clone(),
            enabled: true,
            escalation: ModerationEscalation::default(),
            exemption_level: ExemptionLevel::Moderator,
            silent_mode: false,
            custom_message: None,
            name: filter_name.clone(),
        };

        self.spam_filters.write().await.insert(filter_name.clone(), filter);
        info!("Added spam filter '{}': {:?}", filter_name, filter_type);
        Ok(())
    }

    /// Add a spam filter with custom configuration (enhanced version)
    pub async fn add_spam_filter_advanced(
        &self,
        name: String,
        filter_type: SpamFilterType,
        escalation: ModerationEscalation,
        exemption_level: ExemptionLevel,
        silent_mode: bool,
        custom_message: Option<String>,
    ) -> Result<()> {
        let filter = SpamFilter {
            filter_type: filter_type.clone(),
            enabled: true,
            escalation,
            exemption_level,
            silent_mode,
            custom_message,
            name: name.clone(),
        };

        self.spam_filters.write().await.insert(name.clone(), filter);
        info!("Added advanced spam filter '{}': {:?}", name, filter_type);
        Ok(())
    }

    /// Add blacklist filter with patterns (NightBot parity)
    pub async fn add_blacklist_filter(
        &self,
        name: String,
        patterns: Vec<String>,
        case_sensitive: bool,
        whole_words_only: bool,
        exemption_level: ExemptionLevel,
        timeout_seconds: u64,
        custom_message: Option<String>,
    ) -> Result<()> {
        let mut blacklist_patterns = Vec::new();
        
        for pattern_str in patterns {
            let pattern = if pattern_str.starts_with("~/") && pattern_str.ends_with('/') || pattern_str.matches('/').count() >= 2 {
                // Regex pattern
                match BlacklistPattern::from_regex_string(&pattern_str) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("Invalid regex pattern '{}': {}", pattern_str, e);
                        continue;
                    }
                }
            } else if pattern_str.contains('*') {
                // Wildcard pattern
                BlacklistPattern::Wildcard(pattern_str)
            } else {
                // Literal pattern
                BlacklistPattern::Literal(pattern_str)
            };
            
            blacklist_patterns.push(pattern);
        }

        let escalation = ModerationEscalation {
            first_offense: ModerationAction::WarnUser { 
                message: custom_message.clone().unwrap_or_else(|| "Please watch your language (first warning)".to_string())
            },
            repeat_offense: ModerationAction::TimeoutUser { duration_seconds: timeout_seconds },
            offense_window_seconds: 3600, // 1 hour
        };

        let filter_type = SpamFilterType::Blacklist {
            patterns: blacklist_patterns,
            case_sensitive,
            whole_words_only,
        };

        self.add_spam_filter_advanced(
            name,
            filter_type,
            escalation,
            exemption_level,
            false, // Don't use silent mode by default for blacklist
            custom_message,
        ).await
    }

    /// Enable or disable all spam filters
    pub async fn set_spam_protection_enabled(&self, enabled: bool) {
        *self.global_enabled.write().await = enabled;
        info!("Global spam protection {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Enable or disable a specific filter
    pub async fn set_filter_enabled(&self, filter_name: &str, enabled: bool) -> Result<()> {
        let mut filters = self.spam_filters.write().await;
        if let Some(filter) = filters.get_mut(filter_name) {
            filter.enabled = enabled;
            info!("Filter '{}' {}", filter_name, if enabled { "enabled" } else { "disabled" });
            Ok(())
        } else {
            Err(anyhow::anyhow!("Filter '{}' not found", filter_name))
        }
    }

    /// Remove a spam filter
    pub async fn remove_filter(&self, filter_name: &str) -> Result<()> {
        let mut filters = self.spam_filters.write().await;
        if filters.remove(filter_name).is_some() {
            info!("Removed filter '{}'", filter_name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Filter '{}' not found", filter_name))
        }
    }

    /// List all filters
    pub async fn list_filters(&self) -> Vec<(String, bool)> {
        let filters = self.spam_filters.read().await;
        filters.iter()
            .map(|(name, filter)| (name.clone(), filter.enabled))
            .collect()
    }

    /// Clear message history for all users (useful for cleanup)
    pub async fn clear_message_history(&self) {
        self.user_message_history.write().await.clear();
        info!("Cleared all user message history");
    }

    /// Check message against all spam filters with enhanced escalation
    pub async fn check_spam_filters(
        &self, 
        message: &ChatMessage,
        user_points: Option<&UserPoints>
    ) -> Option<ModerationAction> {
        if !*self.global_enabled.read().await {
            return None;
        }

        let filters = self.spam_filters.read().await;
        
        for (filter_name, filter) in filters.iter() {
            if !filter.enabled {
                continue;
            }

            // Check exemptions
            if filter.exemption_level.is_exempt(message, user_points) {
                continue;
            }

            // Check against the specific filter type
            if self.violates_filter(message, &filter.filter_type).await {
                info!("Message from {} flagged by filter '{}': {}", 
                      message.username, filter_name, message.content);
                
                // Determine escalation level
                let user_key = format!("{}:{}", message.platform, message.username);
                let mut history_guard = self.user_message_history.write().await;
                let user_history = history_guard.entry(user_key.clone())
                    .or_insert_with(|| UserMessageHistory::new(user_key));
                
                let is_repeat = user_history.violation_history
                    .is_repeat_offense(filter_name, filter.escalation.offense_window_seconds);
                
                // Choose action based on escalation
                let action = if is_repeat {
                    filter.escalation.repeat_offense.clone()
                } else {
                    filter.escalation.first_offense.clone()
                };
                
                // Record violation
                let violation = ViolationRecord {
                    filter_name: filter_name.clone(),
                    timestamp: chrono::Utc::now(),
                    action_taken: action.clone(),
                    message_content: message.content.clone(),
                };
                user_history.violation_history.add_violation(violation);
                
                // Override message for custom responses
                let final_action = if let Some(ref custom_msg) = filter.custom_message {
                    match action {
                        ModerationAction::WarnUser { .. } => {
                            ModerationAction::WarnUser { message: custom_msg.clone() }
                        }
                        other => other,
                    }
                } else {
                    action
                };
                
                // Handle silent mode
                if filter.silent_mode {
                    match final_action {
                        ModerationAction::WarnUser { .. } => {
                            return Some(ModerationAction::LogOnly);
                        }
                        other => return Some(other),
                    }
                } else {
                    return Some(final_action);
                }
            }
        }

        None
    }

    /// Check if a message violates a specific filter type
    async fn violates_filter(&self, message: &ChatMessage, filter_type: &SpamFilterType) -> bool {
        match filter_type {
            SpamFilterType::ExcessiveCaps { max_percentage } => {
                Self::check_excessive_caps(&message.content, *max_percentage)
            }
            SpamFilterType::LinkBlocking { allow_mods, whitelist } => {
                if *allow_mods && message.is_mod {
                    false
                } else {
                    Self::check_links(&message.content, whitelist)
                }
            }
            SpamFilterType::RepeatedMessages { max_repeats, window_seconds } => {
                self.check_repeated_messages(message, *max_repeats, *window_seconds).await
            }
            SpamFilterType::MessageLength { max_length } => {
                message.content.len() > *max_length
            }
            SpamFilterType::ExcessiveEmotes { max_count } => {
                Self::check_excessive_emotes(&message.content, *max_count)
            }
            SpamFilterType::SymbolSpam { max_percentage } => {
                Self::check_symbol_spam(&message.content, *max_percentage)
            }
            SpamFilterType::RateLimit { max_messages, window_seconds } => {
                self.check_rate_limit(message, *max_messages, *window_seconds).await
            }
            SpamFilterType::Blacklist { patterns, case_sensitive, whole_words_only } => {
                Self::check_blacklist(&message.content, patterns, *case_sensitive, *whole_words_only)
            }
        }
    }

    /// Check blacklist patterns against message content
    fn check_blacklist(
        content: &str, 
        patterns: &[BlacklistPattern], 
        case_sensitive: bool, 
        whole_words_only: bool
    ) -> bool {
        for pattern in patterns {
            if pattern.matches(content, case_sensitive, whole_words_only) {
                debug!("Blacklist match found: pattern matched '{}'", content);
                return true;
            }
        }
        false
    }

    /// Generate a default filter name based on filter type
    fn generate_filter_name(filter_type: &SpamFilterType) -> String {
        match filter_type {
            SpamFilterType::ExcessiveCaps { .. } => "excessive_caps".to_string(),
            SpamFilterType::LinkBlocking { .. } => "link_blocking".to_string(),
            SpamFilterType::RepeatedMessages { .. } => "repeated_messages".to_string(),
            SpamFilterType::MessageLength { .. } => "message_length".to_string(),
            SpamFilterType::ExcessiveEmotes { .. } => "excessive_emotes".to_string(),
            SpamFilterType::SymbolSpam { .. } => "symbol_spam".to_string(),
            SpamFilterType::RateLimit { .. } => "rate_limit".to_string(),
            SpamFilterType::Blacklist { .. } => "blacklist".to_string(),
        }
    }

    // Keep existing check methods...
    fn check_excessive_caps(content: &str, max_percentage: u8) -> bool {
        if content.len() < 10 {
            return false;
        }

        let total_letters = content.chars().filter(|c| c.is_alphabetic()).count();
        if total_letters == 0 {
            return false;
        }

        let caps_count = content.chars().filter(|c| c.is_uppercase()).count();
        let caps_percentage = (caps_count * 100) / total_letters;
        
        caps_percentage > max_percentage as usize
    }

    fn check_links(content: &str, whitelist: &[String]) -> bool {
        let link_patterns = ["http://", "https://", "www.", ".com", ".net", ".org", ".tv"];
        
        if !link_patterns.iter().any(|pattern| content.contains(pattern)) {
            return false;
        }

        for domain in whitelist {
            if content.contains(domain) {
                return false;
            }
        }

        true
    }

    async fn check_repeated_messages(&self, message: &ChatMessage, max_repeats: u8, window_seconds: u64) -> bool {
        let user_key = format!("{}:{}", message.platform, message.username);
        let history = self.user_message_history.read().await;
        
        if let Some(user_hist) = history.get(&user_key) {
            let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(window_seconds as i64);
            let recent_messages: Vec<&String> = user_hist.messages
                .iter()
                .filter(|(timestamp, _)| *timestamp > cutoff_time)
                .map(|(_, content)| content)
                .collect();

            let repeat_count = recent_messages.iter()
                .filter(|&&msg| msg == &message.content)
                .count();

            repeat_count >= max_repeats as usize
        } else {
            false
        }
    }

    fn check_excessive_emotes(content: &str, max_count: u8) -> bool {
        let emote_patterns = [":)", ":(", ":D", ":P", ":o", "Kappa", "PogChamp", "LUL"];
        let emote_count = emote_patterns.iter()
            .map(|pattern| content.matches(pattern).count())
            .sum::<usize>();

        emote_count > max_count as usize
    }

    fn check_symbol_spam(content: &str, max_percentage: u8) -> bool {
        if content.len() < 10 {
            return false;
        }

        let symbol_count = content.chars()
            .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
            .count();
        let symbol_percentage = (symbol_count * 100) / content.len();
        
        symbol_percentage > max_percentage as usize
    }

    async fn check_rate_limit(&self, message: &ChatMessage, max_messages: u8, window_seconds: u64) -> bool {
        let user_key = format!("{}:{}", message.platform, message.username);
        let history = self.user_message_history.read().await;
        
        if let Some(user_hist) = history.get(&user_key) {
            let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(window_seconds as i64);
            let recent_count = user_hist.messages
                .iter()
                .filter(|(timestamp, _)| *timestamp > cutoff_time)
                .count();

            recent_count >= max_messages as usize
        } else {
            false
        }
    }

    /// Update user message history
    pub async fn update_user_history(&self, message: &ChatMessage) {
        let user_key = format!("{}:{}", message.platform, message.username);
        let mut history = self.user_message_history.write().await;
        
        let user_hist = history.entry(user_key.clone()).or_insert_with(|| UserMessageHistory::new(user_key));

        user_hist.messages.push((message.timestamp, message.content.clone()));

        // Clean old messages (keep only last 50 or last hour)
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(1);
        user_hist.messages.retain(|(timestamp, _)| *timestamp > cutoff_time);
        
        if user_hist.messages.len() > 50 {
            user_hist.messages.drain(0..user_hist.messages.len() - 50);
        }
    }

    /// Handle moderation actions with enhanced responses
    pub async fn handle_moderation_action(
        action: ModerationAction,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        match action {
            ModerationAction::DeleteMessage => {
                info!("Would delete message from {} in #{}: {}", 
                      message.username, message.channel, message.content);
            }
            ModerationAction::TimeoutUser { duration_seconds } => {
                info!("Would timeout user {} for {}s in #{}", 
                      message.username, duration_seconds, message.channel);
                
                let timeout_msg = format!("@{} has been timed out for {} seconds", 
                                        message.username, duration_seconds);
                if let Err(e) = response_sender.send((
                    message.platform.clone(),
                    message.channel.clone(),
                    timeout_msg
                )).await {
                    error!("Failed to send timeout notification: {}", e);
                }
            }
            ModerationAction::WarnUser { message: warning } => {
                let warn_msg = format!("@{} {}", message.username, warning);
                if let Err(e) = response_sender.send((
                    message.platform.clone(),
                    message.channel.clone(),
                    warn_msg
                )).await {
                    error!("Failed to send warning: {}", e);
                }
            }
            ModerationAction::LogOnly => {
                info!("Spam detected from {} in #{}: {}", 
                      message.username, message.channel, message.content);
            }
        }

        Ok(())
    }

    /// Get filter statistics
    pub async fn get_filter_stats(&self) -> HashMap<String, serde_json::Value> {
        let filters = self.spam_filters.read().await;
        let history = self.user_message_history.read().await;
        
        let total_filters = filters.len();
        let enabled_filters = filters.values().filter(|f| f.enabled).count();
        let total_violations = history.values()
            .map(|h| h.violation_history.total_violations)
            .sum::<u64>();
        
        let mut stats = HashMap::new();
        stats.insert("total_filters".to_string(), serde_json::Value::Number(total_filters.into()));
        stats.insert("enabled_filters".to_string(), serde_json::Value::Number(enabled_filters.into()));
        stats.insert("total_violations".to_string(), serde_json::Value::Number(total_violations.into()));
        stats.insert("global_enabled".to_string(), serde_json::Value::Bool(*self.global_enabled.read().await));
        
        // Per-filter statistics
        let mut filter_stats = serde_json::Map::new();
        for (name, filter) in filters.iter() {
            let violations = history.values()
                .map(|h| h.violation_history.violations.iter()
                    .filter(|v| v.filter_name == *name)
                    .count() as u64)
                .sum::<u64>();
            
            filter_stats.insert(name.clone(), serde_json::json!({
                "enabled": filter.enabled,
                "violations": violations,
                "silent_mode": filter.silent_mode,
                "exemption_level": format!("{:?}", filter.exemption_level)
            }));
        }
        stats.insert("filter_details".to_string(), serde_json::Value::Object(filter_stats));
        
        stats
    }
}