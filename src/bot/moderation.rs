use anyhow::Result;
use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{ChatMessage, SpamFilter, SpamFilterType, ModerationAction, UserMessageHistory};

pub struct ModerationSystem {
    pub spam_filters: Arc<RwLock<Vec<SpamFilter>>>,
    pub user_message_history: Arc<RwLock<HashMap<String, UserMessageHistory>>>,
}

impl ModerationSystem {
    pub fn new() -> Self {
        Self {
            spam_filters: Arc::new(RwLock::new(Vec::new())),
            user_message_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a spam filter with default configuration
    pub async fn add_spam_filter(&self, filter_type: SpamFilterType) -> Result<()> {
        let filter = SpamFilter {
            filter_type: filter_type.clone(),
            enabled: true,
            timeout_duration: 600, // 10 minutes default
            warning_message: Some("Please follow chat rules".to_string()),
            mod_exempt: true,
            subscriber_exempt: false,
        };

        self.spam_filters.write().await.push(filter);
        info!("Added spam filter: {:?}", filter_type);
        Ok(())
    }

    /// Add a spam filter with custom configuration
    pub async fn add_spam_filter_advanced(
        &self,
        filter_type: SpamFilterType,
        timeout_duration: u64,
        warning_message: Option<String>,
        mod_exempt: bool,
        subscriber_exempt: bool,
    ) -> Result<()> {
        let filter = SpamFilter {
            filter_type: filter_type.clone(),
            enabled: true,
            timeout_duration,
            warning_message,
            mod_exempt,
            subscriber_exempt,
        };

        self.spam_filters.write().await.push(filter);
        info!("Added advanced spam filter: {:?}", filter_type);
        Ok(())
    }

    /// Enable or disable all spam filters
    pub async fn set_spam_protection_enabled(&self, enabled: bool) {
        let mut filters = self.spam_filters.write().await;
        for filter in filters.iter_mut() {
            filter.enabled = enabled;
        }
        info!("Spam protection {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Clear message history for all users (useful for cleanup)
    pub async fn clear_message_history(&self) {
        self.user_message_history.write().await.clear();
        info!("Cleared all user message history");
    }

    /// Check message against all spam filters
    pub async fn check_spam_filters(&self, message: &ChatMessage) -> Option<ModerationAction> {
        let filters = self.spam_filters.read().await;
        
        for filter in filters.iter() {
            if !filter.enabled {
                continue;
            }

            // Check exemptions
            if filter.mod_exempt && message.is_mod {
                continue;
            }
            if filter.subscriber_exempt && message.is_subscriber {
                continue;
            }

            // Check against the specific filter type
            if self.violates_filter(message, &filter.filter_type).await {
                info!("Message from {} flagged by filter: {:?}", message.username, filter.filter_type);
                
                return Some(if filter.timeout_duration > 0 {
                    ModerationAction::TimeoutUser { 
                        duration_seconds: filter.timeout_duration 
                    }
                } else {
                    ModerationAction::DeleteMessage
                });
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
        }
    }

    /// Check for excessive caps lock
    fn check_excessive_caps(content: &str, max_percentage: u8) -> bool {
        if content.len() < 10 {
            return false; // Don't flag short messages
        }

        let total_letters = content.chars().filter(|c| c.is_alphabetic()).count();
        if total_letters == 0 {
            return false;
        }

        let caps_count = content.chars().filter(|c| c.is_uppercase()).count();
        let caps_percentage = (caps_count * 100) / total_letters;
        
        caps_percentage > max_percentage as usize
    }

    /// Check for links with whitelist
    fn check_links(content: &str, whitelist: &[String]) -> bool {
        let link_patterns = ["http://", "https://", "www.", ".com", ".net", ".org", ".tv"];
        
        if !link_patterns.iter().any(|pattern| content.contains(pattern)) {
            return false;
        }

        // Check if any whitelisted domain is in the message
        for domain in whitelist {
            if content.contains(domain) {
                return false; // Whitelisted, allow it
            }
        }

        true // Contains link and not whitelisted
    }

    /// Check for repeated messages
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

    /// Check for excessive emotes (basic implementation)
    fn check_excessive_emotes(content: &str, max_count: u8) -> bool {
        // Simple emote detection - looks for common patterns
        let emote_patterns = [":)", ":(", ":D", ":P", ":o", "Kappa", "PogChamp", "LUL"];
        let emote_count = emote_patterns.iter()
            .map(|pattern| content.matches(pattern).count())
            .sum::<usize>();

        emote_count > max_count as usize
    }

    /// Check for symbol spam
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

    /// Check rate limiting
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
        
        let user_hist = history.entry(user_key).or_insert(UserMessageHistory {
            messages: Vec::new(),
            last_warning: None,
            violation_count: 0,
        });

        // Add new message
        user_hist.messages.push((message.timestamp, message.content.clone()));

        // Clean old messages (keep only last 50 or last hour)
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(1);
        user_hist.messages.retain(|(timestamp, _)| *timestamp > cutoff_time);
        
        // Keep only most recent 50 messages per user
        if user_hist.messages.len() > 50 {
            user_hist.messages.drain(0..user_hist.messages.len() - 50);
        }
    }

    /// Handle moderation actions
    pub async fn handle_moderation_action(
        action: ModerationAction,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        match action {
            ModerationAction::DeleteMessage => {
                info!("Would delete message from {} in #{}: {}", 
                      message.username, message.channel, message.content);
                // Note: Actual message deletion would require platform-specific API calls
            }
            ModerationAction::TimeoutUser { duration_seconds } => {
                info!("Would timeout user {} for {}s in #{}", 
                      message.username, duration_seconds, message.channel);
                
                // Send timeout notification to chat
                let timeout_msg = format!("@{} has been timed out for {} seconds for violating chat rules", 
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
}