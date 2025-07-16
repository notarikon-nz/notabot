use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use tokio::fs;

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
                    custom_message: Some("🤖 Crypto spam detected. Appeal with !appeal if this was a mistake.".to_string()),
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
                    custom_message: Some("🤖 Social manipulation detected. Please engage naturally.".to_string()),
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
                    custom_message: Some("🚨 Impersonation attempt detected. This is a serious violation.".to_string()),
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
                    custom_message: Some("🤖 Unauthorized link detected. Please ask before sharing links.".to_string()),
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
                    custom_message: Some("🤖 Excessive repetition detected. Please use normal text.".to_string()),
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
                    custom_message: Some("🤖 Please reduce the use of capital letters.".to_string()),
                    silent_mode: false,
                },
                SpamFilterConfig {
                    name: "symbol_spam".to_string(),
                    enabled: true,
                    filter_type: "SymbolSpam".to_string(),
                    parameters: serde_json::json!({"max_percentage": 50}),
                    timeout_seconds: 300,
                    exemption_level: "Regular".to_string(),
                    custom_message: Some("🤖 Please reduce symbol usage for better readability.".to_string()),
                    silent_mode: true,
                },
                SpamFilterConfig {
                    name: "rate_limiting".to_string(),
                    enabled: true,
                    filter_type: "RateLimit".to_string(),
                    parameters: serde_json::json!({"max_messages": 4, "window_seconds": 15}),
                    timeout_seconds: 300,
                    exemption_level: "Subscriber".to_string(),
                    custom_message: Some("🤖 Please slow down your messages.".to_string()),
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

