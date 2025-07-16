use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use tokio::fs;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use regex::Regex;

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
    pub channels: Vec<String>,
    pub platforms: Vec<String>,
    pub enabled: bool,
    pub last_triggered: Option<chrono::DateTime<chrono::Utc>>,
    pub trigger_count: u64,
}

/// Enhanced spam filter types with NightBot parity
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
    /// NEW: Blacklist filter with patterns (NightBot parity)
    Blacklist {
        patterns: Vec<BlacklistPattern>,
        case_sensitive: bool,
        whole_words_only: bool,
    },
}

/// Blacklist pattern types supporting literal, wildcard, and regex
#[derive(Debug, Clone)]
pub enum BlacklistPattern {
    /// Literal text match: "badword"
    Literal(String),
    /// Wildcard pattern: "bad*", "*word", "*bad*"
    Wildcard(String),
    /// Regex pattern: ~/pattern/flags
    Regex { pattern: String, compiled: Option<Regex> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    pub version: String,
    pub description: String,
    pub blacklist_filters: Vec<BlacklistFilterConfig>,
    pub spam_filters: Vec<SpamFilterConfig>,
    pub advanced_patterns: Vec<AdvancedPatternConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistFilterConfig {
    pub name: String,
    pub enabled: bool,
    pub patterns: Vec<String>,
    pub timeout_seconds: Option<u64>,
    pub exemption_level: String, // "None", "Subscriber", "Regular", "Moderator", "Owner"
    pub case_sensitive: Option<bool>,
    pub whole_words_only: Option<bool>,
    pub custom_message: Option<String>,
    pub silent_mode: Option<bool>,
    pub tags: Vec<String>, // For categorization: ["crypto", "spam", "urls", etc.]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpamFilterConfig {
    pub name: String,
    pub enabled: bool,
    pub filter_type: String, // "ExcessiveCaps", "SymbolSpam", "RateLimit", etc.
    pub parameters: serde_json::Value, // Flexible parameters for different filter types
    pub timeout_seconds: u64,
    pub exemption_level: String,
    pub custom_message: Option<String>,
    pub silent_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedPatternConfig {
    pub name: String,
    pub enabled: bool,
    pub pattern_type: String, // "FuzzyMatch", "Leetspeak", "ZalgoText", etc.
    pub parameters: serde_json::Value,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            description: "NotaBot AI-Enhanced Filter Configuration".to_string(),
            blacklist_filters: Vec::new(),
            spam_filters: Vec::new(),
            advanced_patterns: Vec::new(),
        }
    }
}

impl BlacklistPattern {
    /// Create a new regex pattern from NightBot-style syntax
    pub fn from_regex_string(input: &str) -> Result<Self, String> {
        if !input.starts_with("~/") {
            return Err("Regex pattern must start with '~/'".to_string());
        }
        
        let content = &input[2..]; // Remove ~/
        
        // Find the last / to separate pattern from flags
        if let Some(last_slash) = content.rfind('/') {
            let pattern = &content[..last_slash];
            let flags = &content[last_slash + 1..];
            
            // Build regex with flags
            let mut regex_builder = regex::RegexBuilder::new(pattern);
            
            for flag in flags.chars() {
                match flag {
                    'i' => { regex_builder.case_insensitive(true); }
                    'm' => { regex_builder.multi_line(true); }
                    's' => { regex_builder.dot_matches_new_line(true); }
                    'x' => { regex_builder.ignore_whitespace(true); }
                    _ => return Err(format!("Unknown regex flag: {}", flag)),
                }
            }
            
            match regex_builder.build() {
                Ok(compiled) => Ok(BlacklistPattern::Regex {
                    pattern: input.to_string(),
                    compiled: Some(compiled),
                }),
                Err(e) => Err(format!("Invalid regex pattern: {}", e)),
            }
        } else {
            Err("Regex pattern must end with '/'".to_string())
        }
    }
    
    /// Check if this pattern matches the given text
    pub fn matches(&self, text: &str, case_sensitive: bool, whole_words_only: bool) -> bool {
        match self {
            BlacklistPattern::Literal(pattern) => {
                let text_to_check = if case_sensitive { text } else { &text.to_lowercase() };
                let pattern_to_check = if case_sensitive { pattern } else { &pattern.to_lowercase() };
                
                if whole_words_only {
                    // Check for whole word boundaries
                    Self::is_whole_word_match(text_to_check, pattern_to_check)
                } else {
                    text_to_check.contains(pattern_to_check)
                }
            }
            BlacklistPattern::Wildcard(pattern) => {
                let text_to_check = if case_sensitive { text.to_string() } else { text.to_lowercase() };
                let pattern_to_check = if case_sensitive { pattern.clone() } else { pattern.to_lowercase() };
                
                if whole_words_only {
                    Self::wildcard_whole_word_match(&text_to_check, &pattern_to_check)
                } else {
                    Self::wildcard_match(&text_to_check, &pattern_to_check)
                }
            }
            BlacklistPattern::Regex { compiled, .. } => {
                if let Some(regex) = compiled {
                    regex.is_match(text)
                } else {
                    false
                }
            }
        }
    }
    
    /// Check if pattern matches as whole word
    fn is_whole_word_match(text: &str, pattern: &str) -> bool {
        let word_chars: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).collect();
        word_chars.iter().any(|word| *word == pattern)
    }
    
    /// Match wildcard pattern against text
    fn wildcard_match(text: &str, pattern: &str) -> bool {
        // Convert wildcard pattern to regex
        let regex_pattern = pattern
            .replace("*", ".*")
            .replace("?", ".");
        
        if let Ok(regex) = Regex::new(&format!("^{}$", regex_pattern)) {
            regex.is_match(text)
        } else {
            false
        }
    }
    
    /// Match wildcard pattern against whole words
    fn wildcard_whole_word_match(text: &str, pattern: &str) -> bool {
        let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).collect();
        words.iter().any(|word| Self::wildcard_match(word, pattern))
    }
}

/// Enhanced spam filter with escalation support
#[derive(Debug, Clone)]
pub struct SpamFilter {
    pub filter_type: SpamFilterType,
    pub enabled: bool,
    pub escalation: ModerationEscalation,
    pub exemption_level: ExemptionLevel,
    pub silent_mode: bool,
    pub custom_message: Option<String>,
    pub name: String, // For management commands
}

/// Moderation escalation system (NightBot parity)
#[derive(Debug, Clone)]
pub struct ModerationEscalation {
    pub first_offense: ModerationAction,
    pub repeat_offense: ModerationAction,
    pub offense_window_seconds: u64, // Time window for tracking repeat offenses
}

impl Default for ModerationEscalation {
    fn default() -> Self {
        Self {
            first_offense: ModerationAction::WarnUser { 
                message: "Please follow chat rules (first warning)".to_string() 
            },
            repeat_offense: ModerationAction::TimeoutUser { duration_seconds: 600 },
            offense_window_seconds: 3600, // 1 hour window
        }
    }
}

/// User exemption levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExemptionLevel {
    None,        // No exemptions
    Subscriber,  // Subscribers exempt
    Regular,     // Regulars exempt (custom role)
    Moderator,   // Moderators exempt
    Owner,       // Channel owner exempt
}

impl ExemptionLevel {
    /// Check if user is exempt based on their status
    pub fn is_exempt(&self, message: &ChatMessage, user_points: Option<&crate::bot::points::UserPoints>) -> bool {
        match self {
            ExemptionLevel::None => false,
            ExemptionLevel::Subscriber => message.is_subscriber || message.is_mod,
            ExemptionLevel::Regular => {
                // Check if user is a "regular" (can be customized)
                if let Some(points) = user_points {
                    points.total_earned >= 10000 || message.is_subscriber || message.is_mod
                } else {
                    message.is_subscriber || message.is_mod
                }
            }
            ExemptionLevel::Moderator => message.is_mod,
            ExemptionLevel::Owner => {
                // Channel owner detection (platform-specific)
                message.user_badges.contains(&"broadcaster".to_string()) || 
                message.user_badges.contains(&"owner".to_string())
            }
        }
    }
}

/// Enhanced moderation actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModerationAction {
    DeleteMessage,
    TimeoutUser { duration_seconds: u64 },
    WarnUser { message: String },
    LogOnly,
}

/// User violation history for escalation tracking
#[derive(Debug, Clone)]
pub struct UserViolationHistory {
    pub user_id: String,
    pub violations: Vec<ViolationRecord>,
    pub total_violations: u64,
    pub last_violation: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct ViolationRecord {
    pub filter_name: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action_taken: ModerationAction,
    pub message_content: String, // For review purposes
}

impl UserViolationHistory {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            violations: Vec::new(),
            total_violations: 0,
            last_violation: None,
        }
    }
    
    /// Check if this is a repeat offense within the window
    pub fn is_repeat_offense(&self, filter_name: &str, window_seconds: u64) -> bool {
        let cutoff_time = chrono::Utc::now() - chrono::Duration::seconds(window_seconds as i64);
        
        self.violations.iter().any(|v| {
            v.filter_name == filter_name && v.timestamp > cutoff_time
        })
    }
    
    /// Add a new violation record
    pub fn add_violation(&mut self, violation: ViolationRecord) {
        self.violations.push(violation.clone());
        self.total_violations += 1;
        self.last_violation = Some(violation.timestamp);
        
        // Keep only recent violations to prevent memory bloat
        let cutoff_time = chrono::Utc::now() - chrono::Duration::days(7);
        self.violations.retain(|v| v.timestamp > cutoff_time);
    }
}

/// User message history for moderation
#[derive(Debug)]
pub struct UserMessageHistory {
    pub messages: Vec<(chrono::DateTime<chrono::Utc>, String)>,
    pub last_warning: Option<chrono::DateTime<chrono::Utc>>,
    pub violation_count: u64,
    pub violation_history: UserViolationHistory,
}

impl UserMessageHistory {
    pub fn new(user_id: String) -> Self {
        Self {
            messages: Vec::new(),
            last_warning: None,
            violation_count: 0,
            violation_history: UserViolationHistory::new(user_id),
        }
    }
}

#[derive(Clone)]
pub struct FilterConfigManager {
    config_path: std::path::PathBuf,
    current_config: FilterConfig,
}

impl FilterConfigManager {
    pub fn new<P: AsRef<Path>>(config_path: P) -> Self {
        Self {
            config_path: config_path.as_ref().to_path_buf(),
            current_config: FilterConfig::default(),
        }
    }

    /// Load filter configuration from file
    pub async fn load_config(&mut self) -> Result<()> {
        if !self.config_path.exists() {
            // Create default config file if it doesn't exist
            self.create_default_config().await?;
        }

        let content = fs::read_to_string(&self.config_path).await?;
        
        // Try JSON first, then YAML as fallback
        self.current_config = if self.config_path.extension()
            .and_then(|ext| ext.to_str()) == Some("json") {
            serde_json::from_str(&content)?
        } else {
            serde_yaml::from_str(&content)?
        };

        log::info!("Loaded filter config from {}", self.config_path.display());
        Ok(())
    }

    /// Save current configuration to file
    pub async fn save_config(&self) -> Result<()> {
        let content = if self.config_path.extension()
            .and_then(|ext| ext.to_str()) == Some("json") {
            serde_json::to_string_pretty(&self.current_config)?
        } else {
            serde_yaml::to_string(&self.current_config)?
        };

        fs::write(&self.config_path, content).await?;
        log::info!("Saved filter config to {}", self.config_path.display());
        Ok(())
    }

    /// Create a comprehensive default configuration file
    async fn create_default_config(&mut self) -> Result<()> {
        // Remove emoji usage as requested
        self.current_config = FilterConfig {
            version: "1.0".to_string(),
            description: "NotaBot AI-Enhanced Filter Configuration - Edit this file to update filters without rebuilding!".to_string(),
            blacklist_filters: vec![
                BlacklistFilterConfig {
                    name: "crypto_spam".to_string(),
                    enabled: true,
                    patterns: vec![
                        "*free money*".to_string(),
                        "*easy money*".to_string(),
                        "*crypto investment*".to_string(),
                        "*bitcoin profit*".to_string(),
                        "*guaranteed return*".to_string(),
                        "*100% profit*".to_string(),
                        "~/(?i)(free|easy)\\s*(money|crypto|bitcoin|eth)/".to_string(),
                        "~/(?i)(guaranteed|100%)\\s*(profit|return|roi)/".to_string(),
                        "~/(?i)(invest|trade)\\s*(now|today|immediately)/".to_string(),
                    ],
                    timeout_seconds: Some(1800), // 30 minutes
                    exemption_level: "Regular".to_string(),
                    case_sensitive: Some(false),
                    whole_words_only: Some(false),
                    custom_message: Some("Crypto spam detected. Appeal with !appeal if this was a mistake.".to_string()),
                    silent_mode: Some(false),
                    tags: vec!["crypto".to_string(), "financial".to_string(), "spam".to_string()],
                },
                BlacklistFilterConfig {
                    name: "social_manipulation".to_string(),
                    enabled: true,
                    patterns: vec![
                        "*follow for follow*".to_string(),
                        "*f4f*".to_string(),
                        "*sub4sub*".to_string(),
                        "*like4like*".to_string(),
                        "*check out my channel*".to_string(),
                        "*visit my stream*".to_string(),
                        "~/(?i)(follow|sub)\\s*(for|4)\\s*(follow|sub)/".to_string(),
                        "~/(?i)(view|like)\\s*(for|4)\\s*(view|like)/".to_string(),
                        "~/(?i)check\\s*(out|my)\\s*(channel|stream)/".to_string(),
                    ],
                    timeout_seconds: Some(600), // 10 minutes
                    exemption_level: "Subscriber".to_string(),
                    case_sensitive: Some(false),
                    whole_words_only: Some(false),
                    custom_message: Some("Social manipulation detected. Please engage naturally.".to_string()),
                    silent_mode: Some(false),
                    tags: vec!["social".to_string(), "manipulation".to_string()],
                },
                BlacklistFilterConfig {
                    name: "impersonation".to_string(),
                    enabled: true,
                    patterns: vec![
                        "*official support*".to_string(),
                        "*admin team*".to_string(),
                        "*moderator here*".to_string(),
                        "*staff member*".to_string(),
                        "*twitch support*".to_string(),
                        "*youtube support*".to_string(),
                        "staff*".to_string(),
                        "admin*".to_string(),
                        "official*".to_string(),
                        "*support*".to_string(),
                    ],
                    timeout_seconds: Some(3600), // 1 hour
                    exemption_level: "Moderator".to_string(),
                    case_sensitive: Some(false),
                    whole_words_only: Some(false),
                    custom_message: Some("Impersonation attempt detected. This is a serious violation.".to_string()),
                    silent_mode: Some(false),
                    tags: vec!["impersonation".to_string(), "security".to_string()],
                },
                BlacklistFilterConfig {
                    name: "urls_and_links".to_string(),
                    enabled: true,
                    patterns: vec![
                        "*discord.gg/*".to_string(),
                        "*bit.ly/*".to_string(),
                        "*tinyurl.com/*".to_string(),
                        "*shortened.link/*".to_string(),
                        "~/(?i)https?:\\/\\/[^\\s]+/".to_string(),
                    ],
                    timeout_seconds: Some(300), // 5 minutes
                    exemption_level: "Regular".to_string(),
                    case_sensitive: Some(false),
                    whole_words_only: Some(false),
                    custom_message: Some("Unauthorized link detected. Please ask before sharing links.".to_string()),
                    silent_mode: Some(true),
                    tags: vec!["urls".to_string(), "links".to_string()],
                },
                BlacklistFilterConfig {
                    name: "excessive_repetition".to_string(),
                    enabled: true,
                    patterns: vec![
                        "*!!!!!*".to_string(),
                        "*?????*".to_string(),
                        "*.....*".to_string(),
                        "*-----*".to_string(),
                        "*=====*".to_string(),
                        "*hahaha*".to_string(),
                        "*hehehe*".to_string(),
                        "*lololo*".to_string(),
                        "*woooo*".to_string(),
                        "~/!{3,}/".to_string(),
                        "~/\\?{3,}/".to_string(),
                        "~/\\.{3,}/".to_string(),
                        "~/a{5,}/".to_string(),
                        "~/e{5,}/".to_string(),
                        "~/o{5,}/".to_string(),
                    ],
                    timeout_seconds: Some(180), // 3 minutes
                    exemption_level: "Subscriber".to_string(),
                    case_sensitive: Some(false),
                    whole_words_only: Some(false),
                    custom_message: Some("Excessive repetition detected. Please use normal text.".to_string()),
                    silent_mode: Some(true),
                    tags: vec!["repetition".to_string(), "spam".to_string()],
                },
            ],
            spam_filters: vec![
                SpamFilterConfig {
                    name: "excessive_caps".to_string(),
                    enabled: true,
                    filter_type: "ExcessiveCaps".to_string(),
                    parameters: serde_json::json!({"max_percentage": 60}),
                    timeout_seconds: 300,
                    exemption_level: "Subscriber".to_string(),
                    custom_message: Some("Please reduce the use of capital letters.".to_string()),
                    silent_mode: false,
                },
                SpamFilterConfig {
                    name: "symbol_spam".to_string(),
                    enabled: true,
                    filter_type: "SymbolSpam".to_string(),
                    parameters: serde_json::json!({"max_percentage": 50}),
                    timeout_seconds: 300,
                    exemption_level: "Regular".to_string(),
                    custom_message: Some("Please reduce symbol usage for better readability.".to_string()),
                    silent_mode: true,
                },
                SpamFilterConfig {
                    name: "rate_limiting".to_string(),
                    enabled: true,
                    filter_type: "RateLimit".to_string(),
                    parameters: serde_json::json!({"max_messages": 4, "window_seconds": 15}),
                    timeout_seconds: 300,
                    exemption_level: "Subscriber".to_string(),
                    custom_message: Some("Please slow down your messages.".to_string()),
                    silent_mode: false,
                },
            ],
            advanced_patterns: vec![
                AdvancedPatternConfig {
                    name: "crypto_fuzzy".to_string(),
                    enabled: true,
                    pattern_type: "FuzzyMatch".to_string(),
                    parameters: serde_json::json!({"pattern": "cryptocurrency", "threshold": 0.7}),
                },
                AdvancedPatternConfig {
                    name: "spam_leetspeak".to_string(),
                    enabled: true,
                    pattern_type: "Leetspeak".to_string(),
                    parameters: serde_json::json!({"pattern": "spam"}),
                },
                AdvancedPatternConfig {
                    name: "zalgo_detection".to_string(),
                    enabled: true,
                    pattern_type: "ZalgoText".to_string(),
                    parameters: serde_json::json!({}),
                },
            ],
        };

        self.save_config().await?;
        log::info!("Created default filter configuration at {}", self.config_path.display());
        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> &FilterConfig {
        &self.current_config
    }

    /// Update a blacklist filter by name
    pub async fn update_blacklist_filter(&mut self, name: &str, filter: BlacklistFilterConfig) -> Result<()> {
        if let Some(existing) = self.current_config.blacklist_filters.iter_mut().find(|f| f.name == name) {
            *existing = filter;
        } else {
            self.current_config.blacklist_filters.push(filter);
        }
        self.save_config().await
    }

    /// Remove a blacklist filter by name
    pub async fn remove_blacklist_filter(&mut self, name: &str) -> Result<bool> {
        let initial_len = self.current_config.blacklist_filters.len();
        self.current_config.blacklist_filters.retain(|f| f.name != name);
        let removed = self.current_config.blacklist_filters.len() != initial_len;
        if removed {
            self.save_config().await?;
        }
        Ok(removed)
    }

    /// Enable/disable a filter by name
    pub async fn toggle_filter(&mut self, name: &str, enabled: bool) -> Result<bool> {
        let mut found = false;
        
        for filter in &mut self.current_config.blacklist_filters {
            if filter.name == name {
                filter.enabled = enabled;
                found = true;
                break;
            }
        }
        
        if !found {
            for filter in &mut self.current_config.spam_filters {
                if filter.name == name {
                    filter.enabled = enabled;
                    found = true;
                    break;
                }
            }
        }
        
        if found {
            self.save_config().await?;
        }
        Ok(found)
    }

    /// Watch for file changes and reload automatically
    pub async fn watch_for_changes(&mut self) -> Result<()> {
        use tokio::time::{sleep, Duration};
        
        let mut last_modified = std::fs::metadata(&self.config_path)?.modified()?;
        
        loop {
            sleep(Duration::from_secs(5)).await; // Check every 5 seconds
            
            if let Ok(metadata) = std::fs::metadata(&self.config_path) {
                if let Ok(modified) = metadata.modified() {
                    if modified > last_modified {
                        log::info!("Filter config file changed, reloading...");
                        if let Err(e) = self.load_config().await {
                            log::error!("Failed to reload config: {}", e);
                        } else {
                            log::info!("Filter config reloaded successfully");
                        }
                        last_modified = modified;
                    }
                }
            }
        }
    }
}

/// Main timer configuration structure loaded from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerConfig {
    pub version: String,
    pub description: String,
    pub global_settings: GlobalTimerSettings,
    pub timers: Vec<TimerDefinition>,
    pub categories: HashMap<String, Vec<String>>,
    pub variables: TimerVariables,
    pub analytics: TimerAnalytics,
    pub rules: TimerRules,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            description: "NotaBot Timer Configuration".to_string(),
            global_settings: GlobalTimerSettings::default(),
            timers: Vec::new(),
            categories: HashMap::new(),
            variables: TimerVariables::default(),
            analytics: TimerAnalytics::default(),
            rules: TimerRules::default(),
        }
    }
}

/// Global settings that apply to all timers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalTimerSettings {
    pub minimum_interval_seconds: u64,
    pub auto_reload: bool,
    pub variable_substitution: bool,
    pub platform_targeting: bool,
}

impl Default for GlobalTimerSettings {
    fn default() -> Self {
        Self {
            minimum_interval_seconds: 30,
            auto_reload: true,
            variable_substitution: true,
            platform_targeting: true,
        }
    }
}

/// Individual timer definition from configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDefinition {
    pub name: String,
    pub enabled: bool,
    pub message: String,
    pub interval_seconds: u64,
    pub channels: Vec<String>,
    pub platforms: Vec<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub variables: Option<Vec<String>>,
}

/// Variable definitions for timer messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerVariables {
    pub builtin: Vec<VariableDefinition>,
    pub custom: Vec<VariableDefinition>,
}

impl Default for TimerVariables {
    fn default() -> Self {
        Self {
            builtin: vec![
                VariableDefinition {
                    name: "$(timer)".to_string(),
                    description: "Name of the current timer".to_string(),
                    example: Some("ai_features".to_string()),
                    default: None,
                },
                VariableDefinition {
                    name: "$(count)".to_string(),
                    description: "Number of times this timer has triggered".to_string(),
                    example: Some("42".to_string()),
                    default: None,
                },
                VariableDefinition {
                    name: "$(platform)".to_string(),
                    description: "Current platform name".to_string(),
                    example: Some("twitch".to_string()),
                    default: None,
                },
                VariableDefinition {
                    name: "$(channel)".to_string(),
                    description: "Current channel name".to_string(),
                    example: Some("awesome_streamer".to_string()),
                    default: None,
                },
            ],
            custom: Vec::new(),
        }
    }
}

/// Definition of a variable that can be used in timer messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDefinition {
    pub name: String,
    pub description: String,
    pub example: Option<String>,
    pub default: Option<String>,
}

/// Analytics configuration for timers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerAnalytics {
    pub track_effectiveness: bool,
    pub track_engagement: bool,
    pub track_click_through: bool,
}

impl Default for TimerAnalytics {
    fn default() -> Self {
        Self {
            track_effectiveness: true,
            track_engagement: true,
            track_click_through: false,
        }
    }
}

/// Rules and validation for timers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerRules {
    pub max_timers_per_channel: usize,
    pub max_message_length: usize,
    pub min_interval_seconds: u64,
    pub max_interval_seconds: u64,
}

impl Default for TimerRules {
    fn default() -> Self {
        Self {
            max_timers_per_channel: 20,
            max_message_length: 500,
            min_interval_seconds: 30,
            max_interval_seconds: 86400, // 24 hours
        }
    }
}

/// Different types of giveaways supported by the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GiveawayType {
    /// Selects from users who have been active in chat during the specified duration
    ActiveUser { 
        duration_minutes: u32,
        min_messages: Option<u32>, // Minimum messages required
    },
    /// Users enter by typing a specific keyword
    Keyword { 
        keyword: String,
        case_sensitive: bool,
        anti_spam: bool, // Prevent multiple entries from same user
        max_entries_per_user: Option<u32>,
    },
    /// Generate random number, first person to type it wins
    RandomNumber { 
        min: u32,
        max: u32,
        auto_generate: bool, // If true, generates immediately on start
    },
}

/// User privilege levels for giveaway eligibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum UserLevel {
    Viewer,        // Regular viewer
    Subscriber,    // Platform subscriber
    Regular,       // Long-time community member (based on points/activity)
    VIP,          // VIP status on platform
    Moderator,    // Channel moderator
    Owner,        // Channel owner/broadcaster
}

impl UserLevel {
    /// Get numeric priority for user level (higher = more privileged)
    pub fn priority(&self) -> u8 {
        match self {
            UserLevel::Viewer => 0,
            UserLevel::Regular => 1,
            UserLevel::Subscriber => 2,
            UserLevel::VIP => 3,
            UserLevel::Moderator => 4,
            UserLevel::Owner => 5,
        }
    }

    /// Check if this user level meets the minimum requirement
    pub fn meets_requirement(&self, required: &UserLevel) -> bool {
        self.priority() >= required.priority()
    }
}

/// Status of an individual user's eligibility for a giveaway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EligibilityStatus {
    pub eligible: bool,
    pub user_level: UserLevel,
    pub entries: u32,           // Number of entries (for extra luck)
    pub last_activity: DateTime<Utc>,
    pub entry_time: Option<DateTime<Utc>>, // When they became eligible
    pub manual_override: bool,   // Manually toggled by moderator
    pub fraud_score: f32,       // AI-generated fraud risk (0.0 = safe, 1.0 = suspicious)
    pub platform: String,       // Which platform they're on
    pub username: String,
    pub display_name: Option<String>,
}

impl EligibilityStatus {
    pub fn new(username: String, platform: String, user_level: UserLevel) -> Self {
        Self {
            eligible: false,
            user_level,
            entries: 1,
            last_activity: Utc::now(),
            entry_time: None,
            manual_override: false,
            fraud_score: 0.0,
            platform,
            username,
            display_name: None,
        }
    }

    /// Mark user as eligible with timestamp
    pub fn make_eligible(&mut self) {
        if !self.eligible {
            self.eligible = true;
            self.entry_time = Some(Utc::now());
        }
    }

    /// Remove eligibility
    pub fn make_ineligible(&mut self) {
        self.eligible = false;
        self.entry_time = None;
    }

    /// Toggle eligibility manually (moderator action)
    pub fn toggle_eligibility(&mut self) {
        self.manual_override = true;
        if self.eligible {
            self.make_ineligible();
        } else {
            self.make_eligible();
        }
    }

    /// Calculate total weighted entries based on extra luck
    pub fn weighted_entries(&self, subscriber_multiplier: f32, regular_multiplier: f32) -> u32 {
        if !self.eligible {
            return 0;
        }

        let base_entries = self.entries as f32;
        let multiplier = match self.user_level {
            UserLevel::Subscriber => subscriber_multiplier,
            UserLevel::Regular => regular_multiplier,
            _ => 1.0,
        };

        (base_entries * multiplier).round() as u32
    }
}

/// Configuration settings for a giveaway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiveawaySettings {
    pub eligible_user_levels: Vec<UserLevel>,
    pub subscriber_luck_multiplier: f32,   // Extra entries for subscribers
    pub regular_luck_multiplier: f32,      // Extra entries for regulars
    pub min_account_age_days: Option<u32>, // Minimum account age
    pub min_follow_time_days: Option<u32>, // Minimum follow duration
    pub exclude_banned_users: bool,
    pub exclude_timed_out_users: bool,
    pub fraud_detection_enabled: bool,
    pub max_fraud_score: f32,              // Users above this score are excluded
    pub announcement_message: Option<String>,
    pub winner_announcement: Option<String>,
}

impl Default for GiveawaySettings {
    fn default() -> Self {
        Self {
            eligible_user_levels: vec![
                UserLevel::Viewer,
                UserLevel::Subscriber,
                UserLevel::Regular,
                UserLevel::VIP,
            ],
            subscriber_luck_multiplier: 2.0,
            regular_luck_multiplier: 1.5,
            min_account_age_days: None,
            min_follow_time_days: None,
            exclude_banned_users: true,
            exclude_timed_out_users: true,
            fraud_detection_enabled: true,
            max_fraud_score: 0.7,
            announcement_message: Some("Giveaway started! Check eligibility requirements.".to_string()),
            winner_announcement: Some("Congratulations to $(winner) for winning the giveaway!".to_string()),
        }
    }
}

/// Information about a giveaway winner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiveawayWinner {
    pub username: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub user_level: UserLevel,
    pub winning_time: DateTime<Utc>,
    pub total_entries: u32,
    pub winning_entry: Option<String>, // The keyword/number they entered
    pub fraud_score: f32,
    pub response_time_ms: Option<u64>, // For keyword/number giveaways
    pub channel_url: Option<String>,
    pub last_seen: DateTime<Utc>,
}

impl GiveawayWinner {
    pub fn new(
        username: String,
        platform: String,
        user_level: UserLevel,
        total_entries: u32,
    ) -> Self {
        Self {
            username,
            display_name: None,
            platform,
            user_level,
            winning_time: Utc::now(),
            total_entries,
            winning_entry: None,
            fraud_score: 0.0,
            response_time_ms: None,
            channel_url: None,
            last_seen: Utc::now(),
        }
    }

    /// Check if winner is still active (responded recently)
    pub fn is_active(&self, timeout_minutes: u32) -> bool {
        let timeout_duration = chrono::Duration::minutes(timeout_minutes as i64);
        Utc::now().signed_duration_since(self.last_seen) < timeout_duration
    }

    /// Generate channel URL based on platform
    pub fn generate_channel_url(&mut self) {
        self.channel_url = match self.platform.as_str() {
            "twitch" => Some(format!("https://twitch.tv/{}", self.username)),
            "youtube" => {
                // YouTube channel URLs require channel ID, might need different approach
                Some(format!("https://youtube.com/@{}", self.username))
            }
            _ => None,
        };
    }
}

/// Status of a giveaway
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GiveawayStatus {
    Preparing,   // Being set up
    Active,      // Currently running
    Selecting,   // Choosing winner
    Completed,   // Finished with winner
    Cancelled,   // Cancelled before completion
    Failed,      // Failed due to error
}

/// A currently active giveaway - Fixed to avoid borrowing issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveGiveaway {
    #[serde(with = "uuid_serde")]
    pub id: Uuid,
    pub giveaway_type: GiveawayType,
    pub settings: GiveawaySettings,
    pub status: GiveawayStatus,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub eligible_users: HashMap<String, EligibilityStatus>, // Key: platform:username
    pub winner: Option<GiveawayWinner>,
    pub participant_count: u32,
    pub generated_number: Option<u32>, // For random number giveaways
    pub keyword_entries: HashMap<String, DateTime<Utc>>, // Track keyword entry times
    pub creator: String, // Who started the giveaway
    pub channel: String, // Which channel it's running in
    pub platform: String, // Which platform (or "all" for cross-platform)
}

// Custom UUID serialization module
mod uuid_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use uuid::Uuid;

    pub fn serialize<S>(uuid: &Uuid, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&uuid.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Uuid::parse_str(&s).map_err(serde::de::Error::custom)
    }
}

impl ActiveGiveaway {
    pub fn new(
        giveaway_type: GiveawayType,
        settings: GiveawaySettings,
        creator: String,
        channel: String,
        platform: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            giveaway_type,
            settings,
            status: GiveawayStatus::Preparing,
            start_time: Utc::now(),
            end_time: None,
            eligible_users: HashMap::new(),
            winner: None,
            participant_count: 0,
            generated_number: None,
            keyword_entries: HashMap::new(),
            creator,
            channel,
            platform,
        }
    }

    /// Get user key for internal storage
    fn user_key(platform: &str, username: &str) -> String {
        format!("{}:{}", platform, username.to_lowercase())
    }

    /// Add or update a user's eligibility status
    pub fn update_user_eligibility(
        &mut self,
        username: String,
        platform: String,
        user_level: UserLevel,
        make_eligible: bool,
    ) {
        let key = Self::user_key(&platform, &username);
        
        // Check eligibility first to avoid double-borrowing
        let should_make_eligible = make_eligible && self.is_user_eligible_by_level(&user_level);
        
        let status = self.eligible_users.entry(key).or_insert_with(|| {
            EligibilityStatus::new(username.clone(), platform.clone(), user_level.clone())
        });

        // Update user level (might have changed)
        status.user_level = user_level;
        status.last_activity = Utc::now();

        if should_make_eligible {
            let was_eligible = status.eligible;
            status.make_eligible();
            if !was_eligible && status.eligible {
                self.participant_count += 1;
            }
        }
    }

    /// Check if a user level is eligible for this giveaway
    pub fn is_user_eligible_by_level(&self, user_level: &UserLevel) -> bool {
        self.settings.eligible_user_levels.iter()
            .any(|level| user_level.meets_requirement(level) || level.meets_requirement(user_level))
    }

    /// Get all currently eligible users
    pub fn get_eligible_users(&self) -> Vec<&EligibilityStatus> {
        self.eligible_users.values()
            .filter(|status| status.eligible && status.fraud_score <= self.settings.max_fraud_score)
            .collect()
    }

    /// Get total weighted entries for random selection
    pub fn get_total_weighted_entries(&self) -> u32 {
        self.get_eligible_users()
            .iter()
            .map(|status| status.weighted_entries(
                self.settings.subscriber_luck_multiplier,
                self.settings.regular_luck_multiplier,
            ))
            .sum()
    }

    /// Reset all user eligibility
    pub fn reset_eligibility(&mut self) {
        for status in self.eligible_users.values_mut() {
            status.make_ineligible();
        }
        self.participant_count = 0;
        self.keyword_entries.clear();
    }

    /// Check if giveaway has timed out (for active user type)
    pub fn has_timed_out(&self) -> bool {
        if let GiveawayType::ActiveUser { duration_minutes, .. } = &self.giveaway_type {
            let duration = chrono::Duration::minutes(*duration_minutes as i64);
            Utc::now().signed_duration_since(self.start_time) > duration
        } else {
            false
        }
    }

    /// Mark giveaway as completed with winner
    pub fn complete_with_winner(&mut self, winner: GiveawayWinner) {
        self.winner = Some(winner);
        self.status = GiveawayStatus::Completed;
        self.end_time = Some(Utc::now());
    }

    /// Cancel the giveaway
    pub fn cancel(&mut self, _reason: Option<String>) {
        self.status = GiveawayStatus::Cancelled;
        self.end_time = Some(Utc::now());
        // Could store cancellation reason in metadata if needed
    }
}

/// A completed giveaway for historical tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedGiveaway {
    #[serde(with = "uuid_serde")]
    pub id: Uuid,
    pub giveaway_type: GiveawayType,
    pub settings: GiveawaySettings,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub winner: Option<GiveawayWinner>,
    pub participant_count: u32,
    pub total_entries: u32,
    pub success: bool,
    pub creator: String,
    pub channel: String,
    pub platform: String,
    pub duration_seconds: u64,
}

impl From<ActiveGiveaway> for CompletedGiveaway {
    fn from(active: ActiveGiveaway) -> Self {
        let end_time = active.end_time.unwrap_or_else(Utc::now);
        let duration_seconds = end_time.signed_duration_since(active.start_time)
            .num_seconds()
            .max(0) as u64;

        // Calculate total entries before consuming active
        let total_entries = active.get_eligible_users()
            .iter()
            .map(|status| status.weighted_entries(
                active.settings.subscriber_luck_multiplier,
                active.settings.regular_luck_multiplier,
            ))
            .sum();

        Self {
            id: active.id,
            giveaway_type: active.giveaway_type,
            settings: active.settings,
            start_time: active.start_time,
            end_time,
            winner: active.winner,
            participant_count: active.participant_count,
            total_entries,
            success: matches!(active.status, GiveawayStatus::Completed),
            creator: active.creator,
            channel: active.channel,
            platform: active.platform,
            duration_seconds,
        }
    }
}

/// Error types for giveaway operations
#[derive(Debug, thiserror::Error)]
pub enum GiveawayError {
    #[error("No active giveaway")]
    NoActiveGiveaway,
    
    #[error("Giveaway already active")]
    GiveawayAlreadyActive,
    
    #[error("User not eligible: {reason}")]
    UserNotEligible { reason: String },
    
    #[error("Invalid giveaway configuration: {reason}")]
    InvalidConfiguration { reason: String },
    
    #[error("Winner selection failed: {reason}")]
    WinnerSelectionFailed { reason: String },
    
    #[error("Giveaway timeout")]
    GiveawayTimeout,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },
}

/// Result type for giveaway operations
pub type GiveawayResult<T> = Result<T, GiveawayError>;