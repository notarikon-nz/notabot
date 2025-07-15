use serde::{Deserialize, Serialize};
// use std::collections::HashMap;

/// Core message types that flow through the bot system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub platform: String,
    pub channel: String,
    pub username: String,
    pub display_name: Option<String>,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub user_badges: Vec<String>,
    pub is_mod: bool,
    pub is_subscriber: bool,
}

#[derive(Debug, Clone)]
pub struct BotCommand {
    pub trigger: String,
    pub response: String,
    pub mod_only: bool,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct BotTimer {
    pub name: String,
    pub message: String,
    pub interval_seconds: u64,
    pub channels: Vec<String>, // Channels to post in (empty = all channels)
    pub platforms: Vec<String>, // Platforms to post on (empty = all platforms)
    pub enabled: bool,
    pub last_triggered: Option<chrono::DateTime<chrono::Utc>>,
    pub trigger_count: u64,
}

#[derive(Debug, Clone)]
pub enum SpamFilterType {
    /// Excessive caps lock (percentage threshold)
    ExcessiveCaps { max_percentage: u8 },
    /// Link blocking with whitelist
    LinkBlocking { allow_mods: bool, whitelist: Vec<String> },
    /// Repeated messages (same message within time window)
    RepeatedMessages { max_repeats: u8, window_seconds: u64 },
    /// Message length limits
    MessageLength { max_length: usize },
    /// Excessive emotes
    ExcessiveEmotes { max_count: u8 },
    /// Symbol spam (non-alphanumeric characters)
    SymbolSpam { max_percentage: u8 },
    /// Fast posting (rate limiting)
    RateLimit { max_messages: u8, window_seconds: u64 },
}

#[derive(Debug, Clone)]
pub struct SpamFilter {
    pub filter_type: SpamFilterType,
    pub enabled: bool,
    pub timeout_duration: u64, // seconds, 0 = just delete message
    pub warning_message: Option<String>,
    pub mod_exempt: bool, // whether mods are exempt from this filter
    pub subscriber_exempt: bool, // whether subscribers are exempt
}

#[derive(Debug, Clone)]
pub enum ModerationAction {
    DeleteMessage,
    TimeoutUser { duration_seconds: u64 },
    WarnUser { message: String },
    LogOnly,
}

#[derive(Debug)]
pub struct UserMessageHistory {
    pub messages: Vec<(chrono::DateTime<chrono::Utc>, String)>,
    pub last_warning: Option<chrono::DateTime<chrono::Utc>>,
    pub violation_count: u64,
}