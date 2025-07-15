// src/types/mod.rs - Enhanced spam filter types for NightBot parity

use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone)]
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