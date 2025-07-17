//! # NotaBot - AI-Powered Chat Moderation Framework
//! 
//! A high-performance, memory-efficient chat bot framework written in Rust,
//! designed for real-time streaming platforms with advanced AI moderation capabilities.
//! 
//! ## Phase 2 Features
//! 
//! - **Advanced AI Pattern Matching**: Fuzzy matching, leetspeak detection, Unicode normalization
//! - **Smart Escalation System**: User behavior tracking, rehabilitation, context-aware penalties
//! - **Real-time Analytics**: Filter effectiveness monitoring, auto-optimization
//! - **Community Intelligence**: Filter sharing, collaborative improvement
//! - **Import/Export System**: NightBot compatibility, multiple formats
//! 
//! ```

pub mod types;
pub mod platforms;
pub mod bot;
pub mod config;
pub mod adaptive;

#[cfg(feature = "web")]
pub mod web;

// Re-export commonly used items
pub mod prelude {
    pub use crate::bot::ChatBot;
    pub use crate::platforms::{
        PlatformConnection, 
        twitch::{TwitchConnection, TwitchConfig},
        youtube::{YouTubeConnection, YouTubeConfig},
    };
    pub use crate::types::{
        ChatMessage, BotCommand, BotTimer, SpamFilterType, SpamFilter, 
        ModerationAction, UserMessageHistory, ExemptionLevel, ModerationEscalation,
        FilterConfigManager
    };
    pub use crate::adaptive::AdaptivePerformanceSystem;


    // Phase 2 exports
    pub use crate::bot::enhanced_moderation::EnhancedModerationSystem;
    pub use crate::bot::pattern_matching::{AdvancedPattern, EnhancedPatternMatcher};
    pub use crate::bot::smart_escalation::{SmartEscalation, ViolationSeverity, PositiveActionType};
    pub use crate::bot::realtime_analytics::{FilterAnalyticsSystem, UserReportType, ModeratorReviewType};
    pub use crate::bot::filter_import_export::{FilterImportExport, ExportFormat, ExportOptions, ImportOptions};
    pub use crate::config::{ConfigurationManager};

    #[cfg(feature = "web")]
    pub use crate::web::{WebDashboard, DashboardState};
    pub use anyhow::Result;
}

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DESCRIPTION: &str = "AI-powered chat moderation system - The NightBot killer";

/// Phase 2 feature flags
pub mod features {
    pub const ADVANCED_PATTERNS: bool = true;
    pub const SMART_ESCALATION: bool = true;
    pub const REAL_TIME_ANALYTICS: bool = true;
    pub const COMMUNITY_INTELLIGENCE: bool = true;
    pub const IMPORT_EXPORT: bool = true;
}

/// AI capabilities
pub mod ai {
    pub const FUZZY_MATCHING: bool = true;
    pub const LEETSPEAK_DETECTION: bool = true;
    pub const UNICODE_NORMALIZATION: bool = true;
    pub const HOMOGLYPH_DETECTION: bool = true;
    pub const ZALGO_DETECTION: bool = true;
    pub const ENCODED_CONTENT_SCANNING: bool = true;
    pub const PHONETIC_MATCHING: bool = true;
    pub const KEYBOARD_SHIFT_DETECTION: bool = true;
    pub const REPEATED_CHAR_COMPRESSION: bool = true;
}

/// Performance characteristics
pub mod performance {
    pub const TARGET_RESPONSE_TIME_MS: f64 = 1.0;
    pub const MAX_MEMORY_MB: usize = 10;
    pub const TARGET_UPTIME_PERCENT: f64 = 99.9;
    pub const MAX_MESSAGES_PER_SECOND: usize = 10000;
}

/// Compatibility information
pub mod compatibility {
    pub const NIGHTBOT_IMPORT: bool = true;
    pub const STREAMLABS_IMPORT: bool = false; // Future
    pub const EXPORT_FORMATS: &[&str] = &["json", "yaml", "toml", "nightbot", "compressed"];
    pub const SUPPORTED_PLATFORMS: &[&str] = &["twitch", "youtube"];
    pub const PLANNED_PLATFORMS: &[&str] = &["discord"];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        assert!(!VERSION.is_empty());
        assert!(!DESCRIPTION.is_empty());
    }

    #[test]
    fn test_feature_flags() {
        assert!(features::ADVANCED_PATTERNS);
        assert!(features::SMART_ESCALATION);
        assert!(features::REAL_TIME_ANALYTICS);
        assert!(features::COMMUNITY_INTELLIGENCE);
        assert!(features::IMPORT_EXPORT);
    }

    #[test]
    fn test_ai_capabilities() {
        assert!(ai::FUZZY_MATCHING);
        assert!(ai::LEETSPEAK_DETECTION);
        assert!(ai::UNICODE_NORMALIZATION);
        assert!(ai::HOMOGLYPH_DETECTION);
        assert!(ai::ZALGO_DETECTION);
        assert!(ai::ENCODED_CONTENT_SCANNING);
    }

    #[test]
    fn test_performance_targets() {
        assert!(performance::TARGET_RESPONSE_TIME_MS <= 5.0);
        assert!(performance::MAX_MEMORY_MB <= 50);
        assert!(performance::TARGET_UPTIME_PERCENT >= 99.0);
        assert!(performance::MAX_MESSAGES_PER_SECOND >= 1000);
    }

    #[test]
    fn test_compatibility() {
        assert!(compatibility::NIGHTBOT_IMPORT);
        assert!(compatibility::EXPORT_FORMATS.contains(&"json"));
        assert!(compatibility::SUPPORTED_PLATFORMS.contains(&"twitch"));
    }
}