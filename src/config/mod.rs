// src/config/mod.rs - New configuration management module

use anyhow::Result;
use log::{debug, error, info, warn};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{sleep, Duration};

use crate::bot::pattern_matching::AdvancedPattern;
use crate::types::{BlacklistFilterConfig, SpamFilterConfig, AdvancedPatternConfig};

/// Main configuration manager that handles all external configuration files
#[derive(Clone)]
pub struct ConfigurationManager {
    /// Base directory for all configuration files
    config_dir: PathBuf,
    
    /// Cached configurations with hot-reload support
    filter_config: Arc<RwLock<FilterConfiguration>>,
    pattern_config: Arc<RwLock<PatternConfiguration>>,
    timer_config: Arc<RwLock<TimerConfiguration>>,
    bot_config: Arc<RwLock<BotConfiguration>>,
    
    /// File watchers for hot-reloading
    watchers: Arc<RwLock<Vec<RecommendedWatcher>>>,
    
    /// Event broadcaster for configuration changes
    change_notifier: broadcast::Sender<ConfigChangeEvent>,
    
    /// Configuration validation
    validator: Arc<ConfigValidator>,
    
    /// Cache control
    cache_enabled: bool,
    last_reload: Arc<RwLock<std::time::Instant>>,
}

/// Events broadcasted when configuration changes
#[derive(Debug, Clone)]
pub enum ConfigChangeEvent {
    FiltersUpdated { file: String },
    PatternsUpdated { file: String },
    TimersUpdated { file: String },
    BotConfigUpdated { file: String },
    ValidationError { file: String, error: String },
    ReloadComplete { files_updated: Vec<String> },
}

/// Master filter configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfiguration {
    pub version: String,
    pub description: String,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub metadata: ConfigMetadata,
    
    /// Blacklist filters with advanced pattern support
    pub blacklist_filters: Vec<EnhancedBlacklistFilter>,
    
    /// Spam detection filters
    pub spam_filters: Vec<EnhancedSpamFilter>,
    
    /// Global filter settings
    pub global_settings: FilterGlobalSettings,
    
    /// Filter categories for organization
    pub categories: HashMap<String, FilterCategory>,
    
    /// Import/export settings
    pub import_export: ImportExportSettings,
}

/// Enhanced blacklist filter with more configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedBlacklistFilter {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub category: String,
    pub priority: u8, // 1-10, higher = checked first
    
    /// Pattern configuration
    pub patterns: Vec<PatternDefinition>,
    pub case_sensitive: bool,
    pub whole_words_only: bool,
    pub regex_flags: Option<String>, // i, m, s, x flags
    
    /// Action configuration
    pub timeout_seconds: Option<u64>,
    pub escalation_enabled: bool,
    pub custom_message: Option<String>,
    pub silent_mode: bool,
    
    /// User exemptions
    pub exemption_level: String,
    pub exempt_users: Vec<String>, // Specific usernames
    pub exempt_platforms: Vec<String>, // Platform-specific exemptions
    
    /// Scheduling and conditions
    pub active_hours: Option<TimeRange>,
    pub active_days: Option<Vec<String>>, // Mon, Tue, etc.
    pub min_account_age_days: Option<u32>,
    pub min_follow_time_days: Option<u32>,
    
    /// Analytics and performance
    pub track_effectiveness: bool,
    pub auto_disable_threshold: Option<f32>, // Auto-disable if accuracy drops below
    pub tags: Vec<String>,
    
    /// AI enhancement settings
    pub ai_enabled: bool,
    pub confidence_threshold: Option<f32>,
    pub learning_enabled: bool,
}

/// Individual pattern definition with type and parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDefinition {
    pub pattern_type: String, // "literal", "wildcard", "regex", "fuzzy"
    pub value: String,
    pub weight: f32, // 0.0-1.0, how much this pattern contributes to match
    pub description: Option<String>,
    pub enabled: bool,
}

/// Enhanced spam filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedSpamFilter {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub category: String,
    pub priority: u8,
    
    /// Filter type and parameters
    pub filter_type: String,
    pub parameters: serde_json::Value,
    
    /// Action and escalation
    pub timeout_seconds: u64,
    pub escalation: EscalationConfig,
    pub custom_message: Option<String>,
    pub silent_mode: bool,
    
    /// Exemptions and conditions
    pub exemption_level: String,
    pub exempt_users: Vec<String>,
    pub active_conditions: ConditionConfig,
    
    /// Performance settings
    pub max_checks_per_second: Option<u32>,
    pub cache_results: bool,
    pub track_performance: bool,
    
    /// AI integration
    pub ai_enhancement: AIEnhancementConfig,
}

/// AI enhancement configuration for filters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIEnhancementConfig {
    pub enabled: bool,
    pub confidence_boost: f32, // How much AI adds to confidence
    pub pattern_learning: bool,
    pub false_positive_learning: bool,
    pub context_analysis: bool,
    pub user_behavior_analysis: bool,
}

/// Escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    pub enabled: bool,
    pub first_offense_action: String,
    pub repeat_offense_action: String,
    pub offense_window_seconds: u64,
    pub max_escalation_level: u8,
    pub cooling_off_period: u64,
}

/// Condition configuration for when filters are active
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionConfig {
    pub time_ranges: Vec<TimeRange>,
    pub day_of_week: Vec<String>,
    pub platform_specific: HashMap<String, bool>,
    pub channel_specific: HashMap<String, bool>,
    pub user_count_threshold: Option<u32>, // Only active when viewer count above threshold
    pub stream_category_filter: Vec<String>, // Only active for certain game categories
}

/// Time range specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: String, // "14:30" format
    pub end: String,   // "22:00" format
    pub timezone: Option<String>, // "UTC", "PST", etc.
}

/// Global filter settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterGlobalSettings {
    pub max_filters_per_message: u8,
    pub global_timeout_multiplier: f32,
    pub enable_cross_platform_sync: bool,
    pub enable_community_learning: bool,
    pub auto_optimization: bool,
    pub performance_monitoring: bool,
    pub debug_mode: bool,
}

/// Filter category definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCategory {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub priority: u8,
    pub color: Option<String>, // For UI display
    pub icon: Option<String>,
}

/// Import/export settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportExportSettings {
    pub auto_export_enabled: bool,
    pub export_interval_hours: u32,
    pub export_formats: Vec<String>,
    pub community_sharing: bool,
    pub backup_retention_days: u32,
    pub nightbot_compatibility: bool,
}

/// Configuration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_modified_by: String,
    pub version_history: Vec<VersionEntry>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub author: String,
    pub changes: Vec<String>,
}

/// Advanced pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConfiguration {
    pub version: String,
    pub description: String,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    
    /// AI-powered pattern collections
    pub pattern_collections: HashMap<String, PatternCollection>,
    
    /// Global pattern settings
    pub global_settings: PatternGlobalSettings,
    
    /// Machine learning configuration
    pub ml_config: MLConfiguration,
}

/// Collection of related patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternCollection {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub category: String,
    pub priority: u8,
    
    /// Pattern definitions
    pub patterns: Vec<AdvancedPatternDefinition>,
    
    /// Collection-wide settings
    pub confidence_threshold: f32,
    pub learning_enabled: bool,
    pub auto_update: bool,
    
    /// Performance settings
    pub max_processing_time_ms: u32,
    pub cache_results: bool,
    pub parallel_processing: bool,
}

/// Advanced pattern definition with AI capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedPatternDefinition {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub pattern_type: String, // "fuzzy", "leetspeak", "unicode", etc.
    pub parameters: serde_json::Value,
    pub weight: f32,
    pub min_confidence: f32,
    pub learning_rate: f32,
    pub tags: Vec<String>,
}

/// Global pattern settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternGlobalSettings {
    pub parallel_processing: bool,
    pub max_processing_threads: u8,
    pub cache_enabled: bool,
    pub cache_size_mb: u32,
    pub performance_profiling: bool,
    pub auto_optimization: bool,
}

/// Machine learning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLConfiguration {
    pub enabled: bool,
    pub training_mode: String, // "online", "batch", "hybrid"
    pub training_data_retention_days: u32,
    pub model_update_frequency: String, // "hourly", "daily", "weekly"
    pub feature_extraction: FeatureExtractionConfig,
    pub model_parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureExtractionConfig {
    pub text_features: bool,
    pub user_behavior_features: bool,
    pub temporal_features: bool,
    pub platform_features: bool,
    pub custom_features: Vec<String>,
}

/// Timer configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerConfiguration {
    pub version: String,
    pub description: String,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    
    /// Timer definitions
    pub timers: Vec<EnhancedTimer>,
    
    /// Global timer settings
    pub global_settings: TimerGlobalSettings,
    
    /// Variable definitions
    pub variables: TimerVariables,
    
    /// Analytics configuration
    pub analytics: TimerAnalytics,
}

/// Enhanced timer with advanced scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedTimer {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub category: String,
    
    /// Message configuration
    pub messages: Vec<TimerMessage>,
    pub message_rotation: String, // "sequential", "random", "weighted"
    
    /// Scheduling configuration
    pub schedule: TimerSchedule,
    
    /// Targeting configuration
    pub targeting: TimerTargeting,
    
    /// Condition configuration
    pub conditions: TimerConditions,
    
    /// Analytics configuration
    pub analytics_enabled: bool,
    pub track_engagement: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerMessage {
    pub content: String,
    pub weight: f32, // For weighted rotation
    pub conditions: Option<MessageConditions>,
    pub variables: Vec<String>, // Variables used in this message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageConditions {
    pub min_viewers: Option<u32>,
    pub max_viewers: Option<u32>,
    pub stream_category: Option<Vec<String>>,
    pub platform_specific: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerSchedule {
    pub interval_seconds: u64,
    pub random_offset_max: Option<u64>, // Random offset ±seconds
    pub time_windows: Vec<TimeRange>,
    pub day_restrictions: Vec<String>,
    pub cooldown_after_message: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerTargeting {
    pub platforms: Vec<String>,
    pub channels: Vec<String>, // Empty = all channels
    pub exclude_channels: Vec<String>,
    pub user_level_filter: Option<String>, // "subscribers_only", etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerConditions {
    pub min_chat_activity: Option<u32>, // Messages per minute
    pub min_viewer_count: Option<u32>,
    pub max_viewer_count: Option<u32>,
    pub stream_uptime_min: Option<u32>, // Minutes
    pub last_timer_cooldown: Option<u64>, // Seconds since last timer
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerGlobalSettings {
    pub max_timers_per_channel: u8,
    pub global_cooldown_seconds: u64,
    pub respect_rate_limits: bool,
    pub batch_processing: bool,
    pub performance_monitoring: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerVariables {
    pub custom_variables: HashMap<String, String>,
    pub dynamic_variables: Vec<DynamicVariable>,
    pub api_variables: Vec<APIVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicVariable {
    pub name: String,
    pub source: String, // "uptime", "viewer_count", "last_follower", etc.
    pub format: Option<String>,
    pub cache_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIVariable {
    pub name: String,
    pub endpoint: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub json_path: String, // JSONPath to extract value
    pub cache_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerAnalytics {
    pub track_effectiveness: bool,
    pub track_click_through: bool,
    pub track_engagement: bool,
    pub retention_days: u32,
}

/// Bot configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfiguration {
    pub version: String,
    pub description: String,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    
    /// Core bot settings
    pub core: CoreBotSettings,
    
    /// Platform configurations
    pub platforms: HashMap<String, PlatformConfig>,
    
    /// Feature flags
    pub features: FeatureFlags,
    
    /// Performance settings
    pub performance: PerformanceSettings,
    
    /// Security settings
    pub security: SecuritySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreBotSettings {
    pub bot_name: String,
    pub global_prefix: String,
    pub response_delay_ms: u64,
    pub max_message_length: usize,
    pub rate_limit_per_minute: u32,
    pub debug_mode: bool,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub enabled: bool,
    pub connection_settings: serde_json::Value,
    pub rate_limits: RateLimitConfig,
    pub features: PlatformFeatures,
    pub webhooks: Vec<WebhookConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub messages_per_second: f32,
    pub burst_limit: u32,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformFeatures {
    pub moderation: bool,
    pub timers: bool,
    pub commands: bool,
    pub points: bool,
    pub giveaways: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub ai_moderation: bool,
    pub advanced_patterns: bool,
    pub smart_escalation: bool,
    pub real_time_analytics: bool,
    pub community_features: bool,
    pub auto_optimization: bool,
    pub learning_mode: bool,
    pub beta_features: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSettings {
    pub max_memory_mb: u32,
    pub max_cpu_percent: u8,
    pub cache_size_mb: u32,
    pub worker_threads: u8,
    pub batch_size: u32,
    pub monitoring_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub encryption_enabled: bool,
    pub api_key_rotation_days: u32,
    pub max_failed_attempts: u8,
    pub ip_whitelist: Vec<String>,
    pub audit_logging: bool,
}

/// Configuration validator
pub struct ConfigValidator {
    schemas: HashMap<String, serde_json::Value>,
}

impl ConfigValidator {
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Validate filter configuration
    pub fn validate_filter_config(&self, config: &FilterConfiguration) -> Result<()> {
        // Validate version format
        if config.version.is_empty() {
            return Err(anyhow::anyhow!("Version cannot be empty"));
        }

        // Validate blacklist filters
        for filter in &config.blacklist_filters {
            self.validate_blacklist_filter(filter)?;
        }

        // Validate spam filters
        for filter in &config.spam_filters {
            self.validate_spam_filter(filter)?;
        }

        Ok(())
    }

    fn validate_blacklist_filter(&self, filter: &EnhancedBlacklistFilter) -> Result<()> {
        if filter.name.is_empty() {
            return Err(anyhow::anyhow!("Filter name cannot be empty"));
        }

        if filter.patterns.is_empty() {
            return Err(anyhow::anyhow!("Filter must have at least one pattern"));
        }

        for pattern in &filter.patterns {
            self.validate_pattern_definition(pattern)?;
        }

        Ok(())
    }

    fn validate_spam_filter(&self, filter: &EnhancedSpamFilter) -> Result<()> {
        if filter.name.is_empty() {
            return Err(anyhow::anyhow!("Filter name cannot be empty"));
        }

        if filter.filter_type.is_empty() {
            return Err(anyhow::anyhow!("Filter type cannot be empty"));
        }

        Ok(())
    }

    fn validate_pattern_definition(&self, pattern: &PatternDefinition) -> Result<()> {
        if pattern.value.is_empty() {
            return Err(anyhow::anyhow!("Pattern value cannot be empty"));
        }

        if !(0.0..=1.0).contains(&pattern.weight) {
            return Err(anyhow::anyhow!("Pattern weight must be between 0.0 and 1.0"));
        }

        // Validate regex patterns
        if pattern.pattern_type == "regex" {
            if let Err(e) = regex::Regex::new(&pattern.value) {
                return Err(anyhow::anyhow!("Invalid regex pattern: {}", e));
            }
        }

        Ok(())
    }

    /// Validate pattern configuration
    pub fn validate_pattern_config(&self, config: &PatternConfiguration) -> Result<()> {
        for collection in config.pattern_collections.values() {
            self.validate_pattern_collection(collection)?;
        }
        Ok(())
    }

    fn validate_pattern_collection(&self, collection: &PatternCollection) -> Result<()> {
        if collection.name.is_empty() {
            return Err(anyhow::anyhow!("Pattern collection name cannot be empty"));
        }

        for pattern in &collection.patterns {
            self.validate_advanced_pattern_definition(pattern)?;
        }

        Ok(())
    }

    fn validate_advanced_pattern_definition(&self, pattern: &AdvancedPatternDefinition) -> Result<()> {
        if pattern.name.is_empty() {
            return Err(anyhow::anyhow!("Pattern name cannot be empty"));
        }

        if !(0.0..=1.0).contains(&pattern.weight) {
            return Err(anyhow::anyhow!("Pattern weight must be between 0.0 and 1.0"));
        }

        if !(0.0..=1.0).contains(&pattern.min_confidence) {
            return Err(anyhow::anyhow!("Min confidence must be between 0.0 and 1.0"));
        }

        Ok(())
    }
}

impl Default for FilterConfiguration {
    fn default() -> Self {
        Self {
            version: "2.0".to_string(),
            description: "NotaBot Enhanced Filter Configuration with Hot-Reload Support".to_string(),
            last_updated: chrono::Utc::now(),
            metadata: ConfigMetadata {
                created_by: "NotaBot System".to_string(),
                created_at: chrono::Utc::now(),
                last_modified_by: "System".to_string(),
                version_history: Vec::new(),
                checksum: None,
            },
            blacklist_filters: Vec::new(),
            spam_filters: Vec::new(),
            global_settings: FilterGlobalSettings {
                max_filters_per_message: 10,
                global_timeout_multiplier: 1.0,
                enable_cross_platform_sync: true,
                enable_community_learning: true,
                auto_optimization: false,
                performance_monitoring: true,
                debug_mode: false,
            },
            categories: HashMap::new(),
            import_export: ImportExportSettings {
                auto_export_enabled: true,
                export_interval_hours: 24,
                export_formats: vec!["json".to_string(), "yaml".to_string()],
                community_sharing: false,
                backup_retention_days: 30,
                nightbot_compatibility: true,
            },
        }
    }
}

impl Default for PatternConfiguration {
    fn default() -> Self {
        Self {
            version: "2.0".to_string(),
            description: "NotaBot Advanced Pattern Configuration with AI Enhancement".to_string(),
            last_updated: chrono::Utc::now(),
            pattern_collections: HashMap::new(),
            global_settings: PatternGlobalSettings {
                parallel_processing: true,
                max_processing_threads: 4,
                cache_enabled: true,
                cache_size_mb: 50,
                performance_profiling: true,
                auto_optimization: true,
            },
            ml_config: MLConfiguration {
                enabled: true,
                training_mode: "online".to_string(),
                training_data_retention_days: 30,
                model_update_frequency: "daily".to_string(),
                feature_extraction: FeatureExtractionConfig {
                    text_features: true,
                    user_behavior_features: true,
                    temporal_features: true,
                    platform_features: true,
                    custom_features: Vec::new(),
                },
                model_parameters: serde_json::json!({}),
            },
        }
    }
}

impl ConfigurationManager {
    /// Create a new configuration manager
    pub fn new<P: AsRef<Path>>(config_dir: P) -> Self {
        let (tx, _) = broadcast::channel(100);
        
        Self {
            config_dir: config_dir.as_ref().to_path_buf(),
            filter_config: Arc::new(RwLock::new(FilterConfiguration::default())),
            pattern_config: Arc::new(RwLock::new(PatternConfiguration::default())),
            timer_config: Arc::new(RwLock::new(TimerConfiguration::default())),
            bot_config: Arc::new(RwLock::new(BotConfiguration::default())),
            watchers: Arc::new(RwLock::new(Vec::new())),
            change_notifier: tx,
            validator: Arc::new(ConfigValidator::new()),
            cache_enabled: true,
            last_reload: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }

    /// Initialize configuration system
    pub async fn initialize(&self) -> Result<()> {
        // Create config directory if it doesn't exist
        if !self.config_dir.exists() {
            fs::create_dir_all(&self.config_dir).await?;
            info!("Created configuration directory: {}", self.config_dir.display());
        }

        // Load all configurations
        self.load_all_configs().await?;

        // Setup file watchers for hot-reloading
        self.setup_file_watchers().await?;

        info!("Configuration manager initialized successfully");
        Ok(())
    }

    /// Load all configuration files
    async fn load_all_configs(&self) -> Result<()> {
        let mut files_loaded = Vec::new();

        // Load filter configuration
        if let Err(e) = self.load_filter_config().await {
            warn!("Failed to load filter config, using defaults: {}", e);
            self.create_default_filter_config().await?;
            files_loaded.push("filters.yaml (created default)".to_string());
        } else {
            files_loaded.push("filters.yaml".to_string());
        }

        // Load pattern configuration
        if let Err(e) = self.load_pattern_config().await {
            warn!("Failed to load pattern config, using defaults: {}", e);
            self.create_default_pattern_config().await?;
            files_loaded.push("patterns.yaml (created default)".to_string());
        } else {
            files_loaded.push("patterns.yaml".to_string());
        }

        // Load timer configuration
        if let Err(e) = self.load_timer_config().await {
            warn!("Failed to load timer config, using defaults: {}", e);
            self.create_default_timer_config().await?;
            files_loaded.push("timers.yaml (created default)".to_string());
        } else {
            files_loaded.push("timers.yaml".to_string());
        }

        // Load bot configuration
        if let Err(e) = self.load_bot_config().await {
            warn!("Failed to load bot config, using defaults: {}", e);
            self.create_default_bot_config().await?;
            files_loaded.push("bot.yaml (created default)".to_string());
        } else {
            files_loaded.push("bot.yaml".to_string());
        }

        // Broadcast reload complete event
        let _ = self.change_notifier.send(ConfigChangeEvent::ReloadComplete { files_updated: files_loaded });

        Ok(())
    }

    /// Load filter configuration from file
    async fn load_filter_config(&self) -> Result<()> {
        let config_path = self.config_dir.join("filters.yaml");
        if !config_path.exists() {
            return Err(anyhow::anyhow!("Filter config file not found"));
        }

        let content = fs::read_to_string(&config_path).await?;
        let config: FilterConfiguration = serde_yaml::from_str(&content)?;

        // Validate configuration
        self.validator.validate_filter_config(&config)?;

        // Update cached configuration
        *self.filter_config.write().await = config;

        debug!("Loaded filter configuration from {}", config_path.display());
        Ok(())
    }

    /// Load pattern configuration from file
    async fn load_pattern_config(&self) -> Result<()> {
        let config_path = self.config_dir.join("patterns.yaml");
        if !config_path.exists() {
            return Err(anyhow::anyhow!("Pattern config file not found"));
        }

        let content = fs::read_to_string(&config_path).await?;
        let config: PatternConfiguration = serde_yaml::from_str(&content)?;

        // Validate configuration
        self.validator.validate_pattern_config(&config)?;

        // Update cached configuration
        *self.pattern_config.write().await = config;

        debug!("Loaded pattern configuration from {}", config_path.display());
        Ok(())
    }

    /// Load timer configuration from file
    async fn load_timer_config(&self) -> Result<()> {
        let config_path = self.config_dir.join("timers.yaml");
        if !config_path.exists() {
            return Err(anyhow::anyhow!("Timer config file not found"));
        }

        let content = fs::read_to_string(&config_path).await?;
        let config: TimerConfiguration = serde_yaml::from_str(&content)?;

        // Update cached configuration
        *self.timer_config.write().await = config;

        debug!("Loaded timer configuration from {}", config_path.display());
        Ok(())
    }

    /// Load bot configuration from file
    async fn load_bot_config(&self) -> Result<()> {
        let config_path = self.config_dir.join("bot.yaml");
        if !config_path.exists() {
            return Err(anyhow::anyhow!("Bot config file not found"));
        }

        let content = fs::read_to_string(&config_path).await?;
        let config: BotConfiguration = serde_yaml::from_str(&content)?;

        // Update cached configuration
        *self.bot_config.write().await = config;

        debug!("Loaded bot configuration from {}", config_path.display());
        Ok(())
    }

    /// Create default filter configuration file
    async fn create_default_filter_config(&self) -> Result<()> {
        let mut config = FilterConfiguration::default();
        
        // Add comprehensive default filters
        config.blacklist_filters = vec![
            EnhancedBlacklistFilter {
                id: "crypto_spam".to_string(),
                name: "Cryptocurrency Spam Detection".to_string(),
                enabled: true,
                description: Some("Detects crypto scams and investment spam".to_string()),
                category: "financial_spam".to_string(),
                priority: 9,
                patterns: vec![
                    PatternDefinition {
                        pattern_type: "wildcard".to_string(),
                        value: "*free money*".to_string(),
                        weight: 1.0,
                        description: Some("Free money promises".to_string()),
                        enabled: true,
                    },
                    PatternDefinition {
                        pattern_type: "regex".to_string(),
                        value: r"(?i)(guaranteed|100%)\s*(profit|return|roi)".to_string(),
                        weight: 1.0,
                        description: Some("Guaranteed profit claims".to_string()),
                        enabled: true,
                    },
                    PatternDefinition {
                        pattern_type: "fuzzy".to_string(),
                        value: "cryptocurrency investment".to_string(),
                        weight: 0.8,
                        description: Some("Crypto investment variations".to_string()),
                        enabled: true,
                    },
                ],
                case_sensitive: false,
                whole_words_only: false,
                regex_flags: Some("i".to_string()),
                timeout_seconds: Some(1800),
                escalation_enabled: true,
                custom_message: Some("🚨 Crypto spam detected. Appeal with !appeal if this was a mistake.".to_string()),
                silent_mode: false,
                exemption_level: "Regular".to_string(),
                exempt_users: Vec::new(),
                exempt_platforms: Vec::new(),
                active_hours: None,
                active_days: None,
                min_account_age_days: None,
                min_follow_time_days: None,
                track_effectiveness: true,
                auto_disable_threshold: Some(0.6),
                tags: vec!["crypto".to_string(), "financial".to_string(), "spam".to_string()],
                ai_enabled: true,
                confidence_threshold: Some(0.8),
                learning_enabled: true,
            },
            EnhancedBlacklistFilter {
                id: "social_manipulation".to_string(),
                name: "Social Media Manipulation".to_string(),
                enabled: true,
                description: Some("Detects follow-for-follow and engagement manipulation".to_string()),
                category: "social_spam".to_string(),
                priority: 8,
                patterns: vec![
                    PatternDefinition {
                        pattern_type: "wildcard".to_string(),
                        value: "*follow for follow*".to_string(),
                        weight: 1.0,
                        description: Some("Follow-for-follow requests".to_string()),
                        enabled: true,
                    },
                    PatternDefinition {
                        pattern_type: "regex".to_string(),
                        value: r"(?i)(follow|sub)\s*(for|4)\s*(follow|sub)".to_string(),
                        weight: 1.0,
                        description: Some("F4F and S4S patterns".to_string()),
                        enabled: true,
                    },
                ],
                case_sensitive: false,
                whole_words_only: false,
                regex_flags: Some("i".to_string()),
                timeout_seconds: Some(600),
                escalation_enabled: true,
                custom_message: Some("Please engage naturally with our community.".to_string()),
                silent_mode: false,
                exemption_level: "Subscriber".to_string(),
                exempt_users: Vec::new(),
                exempt_platforms: Vec::new(),
                active_hours: None,
                active_days: None,
                min_account_age_days: Some(7),
                min_follow_time_days: None,
                track_effectiveness: true,
                auto_disable_threshold: Some(0.7),
                tags: vec!["social".to_string(), "manipulation".to_string()],
                ai_enabled: true,
                confidence_threshold: Some(0.75),
                learning_enabled: true,
            },
        ];

        // Add default spam filters
        config.spam_filters = vec![
            EnhancedSpamFilter {
                id: "excessive_caps".to_string(),
                name: "Excessive Capitals".to_string(),
                enabled: true,
                description: Some("Detects messages with too many capital letters".to_string()),
                category: "text_spam".to_string(),
                priority: 5,
                filter_type: "ExcessiveCaps".to_string(),
                parameters: serde_json::json!({"max_percentage": 60}),
                timeout_seconds: 300,
                escalation: EscalationConfig {
                    enabled: true,
                    first_offense_action: "warn".to_string(),
                    repeat_offense_action: "timeout".to_string(),
                    offense_window_seconds: 3600,
                    max_escalation_level: 3,
                    cooling_off_period: 86400,
                },
                custom_message: Some("Please reduce the use of capital letters.".to_string()),
                silent_mode: false,
                exemption_level: "Subscriber".to_string(),
                exempt_users: Vec::new(),
                active_conditions: ConditionConfig {
                    time_ranges: Vec::new(),
                    day_of_week: Vec::new(),
                    platform_specific: HashMap::new(),
                    channel_specific: HashMap::new(),
                    user_count_threshold: None,
                    stream_category_filter: Vec::new(),
                },
                max_checks_per_second: Some(100),
                cache_results: true,
                track_performance: true,
                ai_enhancement: AIEnhancementConfig {
                    enabled: true,
                    confidence_boost: 0.2,
                    pattern_learning: true,
                    false_positive_learning: true,
                    context_analysis: true,
                    user_behavior_analysis: true,
                },
            },
        ];

        // Add default categories
        config.categories.insert("financial_spam".to_string(), FilterCategory {
            name: "Financial Spam".to_string(),
            description: "Cryptocurrency, investment, and money-related spam".to_string(),
            enabled: true,
            priority: 9,
            color: Some("#ff4444".to_string()),
            icon: Some("💰".to_string()),
        });

        config.categories.insert("social_spam".to_string(), FilterCategory {
            name: "Social Manipulation".to_string(),
            description: "Follow-for-follow and engagement manipulation".to_string(),
            enabled: true,
            priority: 8,
            color: Some("#ff8844".to_string()),
            icon: Some("🔄".to_string()),
        });

        let config_path = self.config_dir.join("filters.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;

        *self.filter_config.write().await = config;
        info!("Created default filter configuration: {}", config_path.display());
        Ok(())
    }

    /// Create default pattern configuration file
    async fn create_default_pattern_config(&self) -> Result<()> {
        let mut config = PatternConfiguration::default();

        // Add default pattern collections
        let mut spam_detection = PatternCollection {
            name: "Spam Detection".to_string(),
            description: "Advanced AI patterns for spam detection".to_string(),
            enabled: true,
            category: "spam".to_string(),
            priority: 9,
            patterns: vec![
                AdvancedPatternDefinition {
                    id: "fuzzy_spam".to_string(),
                    name: "Fuzzy Spam Detection".to_string(),
                    enabled: true,
                    pattern_type: "fuzzy_match".to_string(),
                    parameters: serde_json::json!({
                        "pattern": "spam",
                        "threshold": 0.8
                    }),
                    weight: 1.0,
                    min_confidence: 0.7,
                    learning_rate: 0.1,
                    tags: vec!["spam".to_string(), "fuzzy".to_string()],
                },
                AdvancedPatternDefinition {
                    id: "leetspeak_spam".to_string(),
                    name: "Leetspeak Spam Detection".to_string(),
                    enabled: true,
                    pattern_type: "leetspeak".to_string(),
                    parameters: serde_json::json!({
                        "pattern": "spam"
                    }),
                    weight: 1.0,
                    min_confidence: 0.8,
                    learning_rate: 0.05,
                    tags: vec!["spam".to_string(), "leetspeak".to_string()],
                },
                AdvancedPatternDefinition {
                    id: "zalgo_detection".to_string(),
                    name: "Zalgo Text Detection".to_string(),
                    enabled: true,
                    pattern_type: "zalgo_text".to_string(),
                    parameters: serde_json::json!({}),
                    weight: 1.0,
                    min_confidence: 0.9,
                    learning_rate: 0.01,
                    tags: vec!["zalgo".to_string(), "corruption".to_string()],
                },
            ],
            confidence_threshold: 0.75,
            learning_enabled: true,
            auto_update: true,
            max_processing_time_ms: 100,
            cache_results: true,
            parallel_processing: true,
        };

        config.pattern_collections.insert("spam_detection".to_string(), spam_detection);

        let config_path = self.config_dir.join("patterns.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;

        *self.pattern_config.write().await = config;
        info!("Created default pattern configuration: {}", config_path.display());
        Ok(())
    }

    /// Create default timer configuration file
    async fn create_default_timer_config(&self) -> Result<()> {
        let mut config = TimerConfiguration::default();

        config.timers = vec![
            EnhancedTimer {
                id: "ai_features".to_string(),
                name: "AI Features Announcement".to_string(),
                enabled: true,
                description: Some("Announces AI moderation features".to_string()),
                category: "info".to_string(),
                messages: vec![
                    TimerMessage {
                        content: "🤖 This stream uses NotaBot's AI-powered moderation! 10x smarter than traditional bots with real-time learning.".to_string(),
                        weight: 1.0,
                        conditions: None,
                        variables: vec!["$(platform)".to_string()],
                    },
                    TimerMessage {
                        content: "✨ Our AI detects: fuzzy matching, leetspeak, unicode tricks, and more! Chat quality protected 24/7.".to_string(),
                        weight: 1.0,
                        conditions: None,
                        variables: Vec::new(),
                    },
                ],
                message_rotation: "sequential".to_string(),
                schedule: TimerSchedule {
                    interval_seconds: 1800, // 30 minutes
                    random_offset_max: Some(300), // ±5 minutes
                    time_windows: Vec::new(),
                    day_restrictions: Vec::new(),
                    cooldown_after_message: Some(60),
                },
                targeting: TimerTargeting {
                    platforms: vec!["twitch".to_string(), "youtube".to_string()],
                    channels: Vec::new(), // All channels
                    exclude_channels: Vec::new(),
                    user_level_filter: None,
                },
                conditions: TimerConditions {
                    min_chat_activity: Some(5), // 5 messages per minute
                    min_viewer_count: Some(10),
                    max_viewer_count: None,
                    stream_uptime_min: Some(15), // 15 minutes
                    last_timer_cooldown: Some(600), // 10 minutes since last timer
                },
                analytics_enabled: true,
                track_engagement: true,
            },
        ];

        config.global_settings = TimerGlobalSettings {
            max_timers_per_channel: 10,
            global_cooldown_seconds: 120,
            respect_rate_limits: true,
            batch_processing: false,
            performance_monitoring: true,
        };

        config.variables = TimerVariables {
            custom_variables: HashMap::new(),
            dynamic_variables: vec![
                DynamicVariable {
                    name: "$(uptime)".to_string(),
                    source: "stream_uptime".to_string(),
                    format: Some("human_readable".to_string()),
                    cache_seconds: 60,
                },
                DynamicVariable {
                    name: "$(viewers)".to_string(),
                    source: "viewer_count".to_string(),
                    format: Some("number".to_string()),
                    cache_seconds: 30,
                },
            ],
            api_variables: Vec::new(),
        };

        let config_path = self.config_dir.join("timers.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;

        *self.timer_config.write().await = config;
        info!("Created default timer configuration: {}", config_path.display());
        Ok(())
    }

    /// Create default bot configuration file
    async fn create_default_bot_config(&self) -> Result<()> {
        let mut config = BotConfiguration::default();

        config.core = CoreBotSettings {
            bot_name: "NotaBot".to_string(),
            global_prefix: "!".to_string(),
            response_delay_ms: 100,
            max_message_length: 500,
            rate_limit_per_minute: 20,
            debug_mode: false,
            log_level: "info".to_string(),
        };

        // Platform configurations
        config.platforms.insert("twitch".to_string(), PlatformConfig {
            enabled: true,
            connection_settings: serde_json::json!({
                "oauth_token": "${TWITCH_OAUTH_TOKEN}",
                "username": "${TWITCH_USERNAME}",
                "channels": "${TWITCH_CHANNELS}"
            }),
            rate_limits: RateLimitConfig {
                messages_per_second: 0.5,
                burst_limit: 5,
                cooldown_seconds: 30,
            },
            features: PlatformFeatures {
                moderation: true,
                timers: true,
                commands: true,
                points: true,
                giveaways: true,
            },
            webhooks: Vec::new(),
        });

        config.platforms.insert("youtube".to_string(), PlatformConfig {
            enabled: false, // Disabled by default
            connection_settings: serde_json::json!({
                "api_key": "${YOUTUBE_API_KEY}",
                "oauth_token": "${YOUTUBE_OAUTH_TOKEN}",
                "live_chat_id": "${YOUTUBE_LIVE_CHAT_ID}"
            }),
            rate_limits: RateLimitConfig {
                messages_per_second: 0.3,
                burst_limit: 3,
                cooldown_seconds: 60,
            },
            features: PlatformFeatures {
                moderation: true,
                timers: true,
                commands: true,
                points: false, // Limited on YouTube
                giveaways: true,
            },
            webhooks: Vec::new(),
        });

        config.features = FeatureFlags {
            ai_moderation: true,
            advanced_patterns: true,
            smart_escalation: true,
            real_time_analytics: true,
            community_features: false, // Disabled by default
            auto_optimization: false,  // Disabled by default for safety
            learning_mode: true,
            beta_features: false,
        };

        config.performance = PerformanceSettings {
            max_memory_mb: 256,
            max_cpu_percent: 80,
            cache_size_mb: 64,
            worker_threads: 4,
            batch_size: 100,
            monitoring_enabled: true,
        };

        config.security = SecuritySettings {
            encryption_enabled: true,
            api_key_rotation_days: 90,
            max_failed_attempts: 5,
            ip_whitelist: Vec::new(),
            audit_logging: true,
        };

        let config_path = self.config_dir.join("bot.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;

        *self.bot_config.write().await = config;
        info!("Created default bot configuration: {}", config_path.display());
        Ok(())
    }

    /// Setup file watchers for hot-reloading
    async fn setup_file_watchers(&self) -> Result<()> {
        use notify::{EventKind, RecursiveMode, Watcher};
        
        let config_dir = self.config_dir.clone();
        let change_notifier = self.change_notifier.clone();
        let filter_config = self.filter_config.clone();
        let pattern_config = self.pattern_config.clone();
        let timer_config = self.timer_config.clone();
        let bot_config = self.bot_config.clone();
        let validator = self.validator.clone();
        let last_reload = self.last_reload.clone();

        // Create file watcher
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if let Err(e) = tx.blocking_send(event) {
                    error!("Failed to send file watch event: {}", e);
                }
            }
        })?;

        // Watch the config directory
        watcher.watch(&config_dir, RecursiveMode::NonRecursive)?;

        // Store watcher to prevent it from being dropped
        self.watchers.write().await.push(watcher);

        // Spawn background task to handle file changes
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                // Debounce rapid file changes
                {
                    let now = std::time::Instant::now();
                    let last = *last_reload.read().await;
                    if now.duration_since(last) < Duration::from_millis(500) {
                        continue; // Skip rapid consecutive changes
                    }
                    *last_reload.write().await = now;
                }

                if let EventKind::Modify(_) = event.kind {
                    for path in event.paths {
                        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                            match filename {
                                "filters.yaml" | "filters.yml" => {
                                    info!("Filter configuration file changed, reloading...");
                                    if let Err(e) = Self::reload_filter_config(&path, &filter_config, &validator).await {
                                        error!("Failed to reload filter config: {}", e);
                                        let _ = change_notifier.send(ConfigChangeEvent::ValidationError {
                                            file: filename.to_string(),
                                            error: e.to_string(),
                                        });
                                    } else {
                                        let _ = change_notifier.send(ConfigChangeEvent::FiltersUpdated {
                                            file: filename.to_string(),
                                        });
                                    }
                                }
                                "patterns.yaml" | "patterns.yml" => {
                                    info!("Pattern configuration file changed, reloading...");
                                    if let Err(e) = Self::reload_pattern_config(&path, &pattern_config, &validator).await {
                                        error!("Failed to reload pattern config: {}", e);
                                        let _ = change_notifier.send(ConfigChangeEvent::ValidationError {
                                            file: filename.to_string(),
                                            error: e.to_string(),
                                        });
                                    } else {
                                        let _ = change_notifier.send(ConfigChangeEvent::PatternsUpdated {
                                            file: filename.to_string(),
                                        });
                                    }
                                }
                                "timers.yaml" | "timers.yml" => {
                                    info!("Timer configuration file changed, reloading...");
                                    if let Err(e) = Self::reload_timer_config(&path, &timer_config).await {
                                        error!("Failed to reload timer config: {}", e);
                                        let _ = change_notifier.send(ConfigChangeEvent::ValidationError {
                                            file: filename.to_string(),
                                            error: e.to_string(),
                                        });
                                    } else {
                                        let _ = change_notifier.send(ConfigChangeEvent::TimersUpdated {
                                            file: filename.to_string(),
                                        });
                                    }
                                }
                                "bot.yaml" | "bot.yml" => {
                                    info!("Bot configuration file changed, reloading...");
                                    if let Err(e) = Self::reload_bot_config(&path, &bot_config).await {
                                        error!("Failed to reload bot config: {}", e);
                                        let _ = change_notifier.send(ConfigChangeEvent::ValidationError {
                                            file: filename.to_string(),
                                            error: e.to_string(),
                                        });
                                    } else {
                                        let _ = change_notifier.send(ConfigChangeEvent::BotConfigUpdated {
                                            file: filename.to_string(),
                                        });
                                    }
                                }
                                _ => {
                                    debug!("Ignoring change to non-config file: {}", filename);
                                }
                            }
                        }
                    }
                }
            }
        });

        info!("File watchers setup for hot-reloading");
        Ok(())
    }

    /// Reload filter configuration from file
    async fn reload_filter_config(
        path: &Path,
        filter_config: &Arc<RwLock<FilterConfiguration>>,
        validator: &Arc<ConfigValidator>,
    ) -> Result<()> {
        let content = fs::read_to_string(path).await?;
        let config: FilterConfiguration = serde_yaml::from_str(&content)?;
        
        // Validate before applying
        validator.validate_filter_config(&config)?;
        
        *filter_config.write().await = config;
        debug!("Reloaded filter configuration from {}", path.display());
        Ok(())
    }

    /// Reload pattern configuration from file
    async fn reload_pattern_config(
        path: &Path,
        pattern_config: &Arc<RwLock<PatternConfiguration>>,
        validator: &Arc<ConfigValidator>,
    ) -> Result<()> {
        let content = fs::read_to_string(path).await?;
        let config: PatternConfiguration = serde_yaml::from_str(&content)?;
        
        // Validate before applying
        validator.validate_pattern_config(&config)?;
        
        *pattern_config.write().await = config;
        debug!("Reloaded pattern configuration from {}", path.display());
        Ok(())
    }

    /// Reload timer configuration from file
    async fn reload_timer_config(
        path: &Path,
        timer_config: &Arc<RwLock<TimerConfiguration>>,
    ) -> Result<()> {
        let content = fs::read_to_string(path).await?;
        let config: TimerConfiguration = serde_yaml::from_str(&content)?;
        
        *timer_config.write().await = config;
        debug!("Reloaded timer configuration from {}", path.display());
        Ok(())
    }

    /// Reload bot configuration from file
    async fn reload_bot_config(
        path: &Path,
        bot_config: &Arc<RwLock<BotConfiguration>>,
    ) -> Result<()> {
        let content = fs::read_to_string(path).await?;
        let config: BotConfiguration = serde_yaml::from_str(&content)?;
        
        *bot_config.write().await = config;
        debug!("Reloaded bot configuration from {}", path.display());
        Ok(())
    }

    /// Get current filter configuration
    pub async fn get_filter_config(&self) -> FilterConfiguration {
        self.filter_config.read().await.clone()
    }

    /// Get current pattern configuration
    pub async fn get_pattern_config(&self) -> PatternConfiguration {
        self.pattern_config.read().await.clone()
    }

    /// Get current timer configuration
    pub async fn get_timer_config(&self) -> TimerConfiguration {
        self.timer_config.read().await.clone()
    }

    /// Get current bot configuration
    pub async fn get_bot_config(&self) -> BotConfiguration {
        self.bot_config.read().await.clone()
    }

    /// Subscribe to configuration change events
    pub fn subscribe_to_changes(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.change_notifier.subscribe()
    }

    /// Save filter configuration to file
    pub async fn save_filter_config(&self, config: FilterConfiguration) -> Result<()> {
        // Validate before saving
        self.validator.validate_filter_config(&config)?;
        
        let config_path = self.config_dir.join("filters.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;
        
        *self.filter_config.write().await = config;
        info!("Saved filter configuration to {}", config_path.display());
        Ok(())
    }

    /// Save pattern configuration to file
    pub async fn save_pattern_config(&self, config: PatternConfiguration) -> Result<()> {
        // Validate before saving
        self.validator.validate_pattern_config(&config)?;
        
        let config_path = self.config_dir.join("patterns.yaml");
        let content = serde_yaml::to_string(&config)?;
        fs::write(&config_path, content).await?;
        
        *self.pattern_config.write().await = config;
        info!("Saved pattern configuration to {}", config_path.display());
        Ok(())
    }

    /// Update a specific filter by ID
    pub async fn update_filter(&self, filter_id: &str, updated_filter: EnhancedBlacklistFilter) -> Result<()> {
        let mut config = self.get_filter_config().await;
        
        if let Some(filter) = config.blacklist_filters.iter_mut().find(|f| f.id == filter_id) {
            *filter = updated_filter;
            self.save_filter_config(config).await?;
            info!("Updated filter: {}", filter_id);
        } else {
            return Err(anyhow::anyhow!("Filter not found: {}", filter_id));
        }
        
        Ok(())
    }

    /// Add a new filter
    pub async fn add_filter(&self, filter: EnhancedBlacklistFilter) -> Result<()> {
        let mut config = self.get_filter_config().await;
        
        // Check for duplicate IDs
        if config.blacklist_filters.iter().any(|f| f.id == filter.id) {
            return Err(anyhow::anyhow!("Filter with ID '{}' already exists", filter.id));
        }
        
        let filter_id = filter.id.clone(); // Clone the ID before moving
        config.blacklist_filters.push(filter);
        self.save_filter_config(config).await?;
        info!("Added new filter: {}", filter_id);
        Ok(())
    }

    /// Remove a filter by ID
    pub async fn remove_filter(&self, filter_id: &str) -> Result<()> {
        let mut config = self.get_filter_config().await;
        let initial_len = config.blacklist_filters.len();
        
        config.blacklist_filters.retain(|f| f.id != filter_id);
        
        if config.blacklist_filters.len() == initial_len {
            return Err(anyhow::anyhow!("Filter not found: {}", filter_id));
        }
        
        self.save_filter_config(config).await?;
        info!("Removed filter: {}", filter_id);
        Ok(())
    }

    /// Toggle filter enabled status
    pub async fn toggle_filter(&self, filter_id: &str, enabled: bool) -> Result<()> {
        let mut config = self.get_filter_config().await;
        
        if let Some(filter) = config.blacklist_filters.iter_mut().find(|f| f.id == filter_id) {
            filter.enabled = enabled;
            self.save_filter_config(config).await?;
            info!("Toggled filter '{}' to {}", filter_id, if enabled { "enabled" } else { "disabled" });
        } else {
            return Err(anyhow::anyhow!("Filter not found: {}", filter_id));
        }
        
        Ok(())
    }

    /// Get filters by category
    pub async fn get_filters_by_category(&self, category: &str) -> Vec<EnhancedBlacklistFilter> {
        let config = self.get_filter_config().await;
        config.blacklist_filters.into_iter()
            .filter(|f| f.category == category)
            .collect()
    }

    /// Get enabled filters only
    pub async fn get_enabled_filters(&self) -> Vec<EnhancedBlacklistFilter> {
        let config = self.get_filter_config().await;
        config.blacklist_filters.into_iter()
            .filter(|f| f.enabled)
            .collect()
    }

    /// Export configuration to different formats
    pub async fn export_config(&self, format: &str, output_path: &Path) -> Result<()> {
        match format.to_lowercase().as_str() {
            "json" => {
                let config = self.get_filter_config().await;
                let content = serde_json::to_string_pretty(&config)?;
                fs::write(output_path, content).await?;
            }
            "yaml" | "yml" => {
                let config = self.get_filter_config().await;
                let content = serde_yaml::to_string(&config)?;
                fs::write(output_path, content).await?;
            }
            "nightbot" => {
                // Convert to NightBot format
                let config = self.get_filter_config().await;
                let nightbot_export = self.convert_to_nightbot_format(&config).await?;
                let content = serde_json::to_string_pretty(&nightbot_export)?;
                fs::write(output_path, content).await?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported export format: {}", format));
            }
        }

        info!("Exported configuration to {} in {} format", output_path.display(), format);
        Ok(())
    }

    /// Convert configuration to NightBot format for compatibility
    async fn convert_to_nightbot_format(&self, config: &FilterConfiguration) -> Result<serde_json::Value> {
        let mut nightbot_filters = Vec::new();

        for filter in &config.blacklist_filters {
            if !filter.enabled {
                continue;
            }

            // Convert patterns to NightBot format
            let mut patterns = Vec::new();
            for pattern in &filter.patterns {
                match pattern.pattern_type.as_str() {
                    "literal" => patterns.push(pattern.value.clone()),
                    "wildcard" => patterns.push(pattern.value.clone()),
                    "regex" => patterns.push(format!("~/{}/", pattern.value)),
                    _ => patterns.push(pattern.value.clone()), // Fallback
                }
            }

            let nightbot_filter = serde_json::json!({
                "name": filter.name,
                "patterns": patterns,
                "enabled": filter.enabled,
                "timeout": filter.timeout_seconds.unwrap_or(600),
                "exemptionLevel": filter.exemption_level,
                "customMessage": filter.custom_message,
                "silent": filter.silent_mode
            });

            nightbot_filters.push(nightbot_filter);
        }

        Ok(serde_json::json!({
            "version": "nightbot_compatible",
            "exported_by": "NotaBot",
            "export_date": chrono::Utc::now().to_rfc3339(),
            "filters": nightbot_filters
        }))
    }

    /// Import configuration from external source
    pub async fn import_config(&self, format: &str, input_path: &Path) -> Result<ImportResult> {
        let content = fs::read_to_string(input_path).await?;
        
        match format.to_lowercase().as_str() {
            "json" => {
                let imported_config: FilterConfiguration = serde_json::from_str(&content)?;
                self.validator.validate_filter_config(&imported_config)?;
                self.save_filter_config(imported_config).await?;
            }
            "yaml" | "yml" => {
                let imported_config: FilterConfiguration = serde_yaml::from_str(&content)?;
                self.validator.validate_filter_config(&imported_config)?;
                self.save_filter_config(imported_config).await?;
            }
            "nightbot" => {
                let imported_config = self.convert_from_nightbot_format(&content).await?;
                self.validator.validate_filter_config(&imported_config)?;
                self.save_filter_config(imported_config).await?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported import format: {}", format));
            }
        }

        info!("Imported configuration from {} in {} format", input_path.display(), format);
        Ok(ImportResult {
            imported_count: 1,
            warnings: Vec::new(),
            errors: Vec::new(),
        })
    }

    /// Convert from NightBot format
    async fn convert_from_nightbot_format(&self, content: &str) -> Result<FilterConfiguration> {    
        let nightbot_data: serde_json::Value = serde_json::from_str(content)?;
        let mut config = FilterConfiguration::default();

        if let Some(filters) = nightbot_data.get("filters").and_then(|f| f.as_array()) {
            for (index, filter) in filters.iter().enumerate() {
                if let Some(filter_obj) = filter.as_object() {
                    let name = filter_obj.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(&format!("imported_filter_{}", index))
                        .to_string(); // Convert to owned String

                    let enabled = filter_obj.get("enabled")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(true);

                    let patterns: Vec<PatternDefinition> = filter_obj.get("patterns")
                        .and_then(|p| p.as_array())
                        .unwrap_or(&Vec::new())
                        .iter()
                        .filter_map(|p| p.as_str())
                        .map(|pattern_str| {
                            if pattern_str.starts_with("~/") && pattern_str.ends_with('/') {
                                PatternDefinition {
                                    pattern_type: "regex".to_string(),
                                    value: pattern_str[2..pattern_str.len()-1].to_string(),
                                    weight: 1.0,
                                    description: None,
                                    enabled: true,
                                }
                            } else if pattern_str.contains('*') {
                                PatternDefinition {
                                    pattern_type: "wildcard".to_string(),
                                    value: pattern_str.to_string(),
                                    weight: 1.0,
                                    description: None,
                                    enabled: true,
                                }
                            } else {
                                PatternDefinition {
                                    pattern_type: "literal".to_string(),
                                    value: pattern_str.to_string(),
                                    weight: 1.0,
                                    description: None,
                                    enabled: true,
                                }
                            }
                        })
                        .collect();

                    let enhanced_filter = EnhancedBlacklistFilter {
                        id: format!("imported_{}", index),
                        name: name.to_string(),
                        enabled,
                        description: Some("Imported from NightBot".to_string()),
                        category: "imported".to_string(),
                        priority: 5,
                        patterns,
                        case_sensitive: false,
                        whole_words_only: false,
                        regex_flags: Some("i".to_string()),
                        timeout_seconds: filter_obj.get("timeout")
                            .and_then(|t| t.as_u64()),
                        escalation_enabled: true,
                        custom_message: filter_obj.get("customMessage")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string()),
                        silent_mode: filter_obj.get("silent")
                            .and_then(|s| s.as_bool())
                            .unwrap_or(false),
                        exemption_level: filter_obj.get("exemptionLevel")
                            .and_then(|e| e.as_str())
                            .unwrap_or("Regular")
                            .to_string(),
                        exempt_users: Vec::new(),
                        exempt_platforms: Vec::new(),
                        active_hours: None,
                        active_days: None,
                        min_account_age_days: None,
                        min_follow_time_days: None,
                        track_effectiveness: true,
                        auto_disable_threshold: None,
                        tags: vec!["imported".to_string(), "nightbot".to_string()],
                        ai_enabled: true,
                        confidence_threshold: Some(0.8),
                        learning_enabled: true,
                    };

                    config.blacklist_filters.push(enhanced_filter);
                }
            }
        }

        Ok(config)
    }

    /// Get configuration statistics
    pub async fn get_config_stats(&self) -> ConfigStats {
        let filter_config = self.get_filter_config().await;
        let pattern_config = self.get_pattern_config().await;
        let timer_config = self.get_timer_config().await;

        ConfigStats {
            total_blacklist_filters: filter_config.blacklist_filters.len(),
            enabled_blacklist_filters: filter_config.blacklist_filters.iter().filter(|f| f.enabled).count(),
            total_spam_filters: filter_config.spam_filters.len(),
            enabled_spam_filters: filter_config.spam_filters.iter().filter(|f| f.enabled).count(),
            total_pattern_collections: pattern_config.pattern_collections.len(),
            enabled_pattern_collections: pattern_config.pattern_collections.values().filter(|c| c.enabled).count(),
            total_timers: timer_config.timers.len(),
            enabled_timers: timer_config.timers.iter().filter(|t| t.enabled).count(),
            categories: filter_config.categories.len(),
            last_updated: filter_config.last_updated,
        }
    }

    /// Validate all configurations
    pub async fn validate_all_configs(&self) -> Result<ValidationReport> {
        let mut report = ValidationReport {
            filter_config_valid: true,
            pattern_config_valid: true,
            timer_config_valid: true,
            bot_config_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Validate filter configuration
        if let Err(e) = self.validator.validate_filter_config(&self.get_filter_config().await) {
            report.filter_config_valid = false;
            report.errors.push(format!("Filter config: {}", e));
        }

        // Validate pattern configuration
        if let Err(e) = self.validator.validate_pattern_config(&self.get_pattern_config().await) {
            report.pattern_config_valid = false;
            report.errors.push(format!("Pattern config: {}", e));
        }

        // Additional validations can be added here for timer and bot configs

        Ok(report)
    }

    /// Reset to default configuration
    pub async fn reset_to_defaults(&self) -> Result<()> {
        info!("Resetting all configurations to defaults...");
        
        self.create_default_filter_config().await?;
        self.create_default_pattern_config().await?;
        self.create_default_timer_config().await?;
        self.create_default_bot_config().await?;

        info!("All configurations reset to defaults");
        Ok(())
    }

    /// Create backup of current configuration
    pub async fn create_backup(&self) -> Result<PathBuf> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_dir = self.config_dir.join("backups");
        
        if !backup_dir.exists() {
            fs::create_dir_all(&backup_dir).await?;
        }

        let backup_path = backup_dir.join(format!("config_backup_{}.tar.gz", timestamp));
        
        // Create compressed backup of all config files
        let mut backup_files = Vec::new();
        let mut read_dir = fs::read_dir(&self.config_dir).await?;
        
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
                backup_files.push(path);
            }
        }

        // Use flate2 and tar to create compressed backup
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::fs::File;
        use tar::Builder;

        let backup_file = File::create(&backup_path)?;
        let encoder = GzEncoder::new(backup_file, Compression::default());
        let mut archive = Builder::new(encoder);

        for file_path in backup_files {
            if let Some(file_name) = file_path.file_name() {
                archive.append_path_with_name(&file_path, file_name)?;
            }
        }

        archive.finish()?;
        info!("Created configuration backup: {}", backup_path.display());
        Ok(backup_path)
    }
}

/// Import result structure
#[derive(Debug)]
pub struct ImportResult {
    pub imported_count: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Configuration statistics
#[derive(Debug)]
pub struct ConfigStats {
    pub total_blacklist_filters: usize,
    pub enabled_blacklist_filters: usize,
    pub total_spam_filters: usize,
    pub enabled_spam_filters: usize,
    pub total_pattern_collections: usize,
    pub enabled_pattern_collections: usize,
    pub total_timers: usize,
    pub enabled_timers: usize,
    pub categories: usize,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Validation report
#[derive(Debug)]
pub struct ValidationReport {
    pub filter_config_valid: bool,
    pub pattern_config_valid: bool,
    pub timer_config_valid: bool,
    pub bot_config_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Default for TimerConfiguration {
    fn default() -> Self {
        Self {
            version: "2.0".to_string(),
            description: "NotaBot Enhanced Timer Configuration with Advanced Scheduling".to_string(),
            last_updated: chrono::Utc::now(),
            timers: Vec::new(),
            global_settings: TimerGlobalSettings {
                max_timers_per_channel: 10,
                global_cooldown_seconds: 120,
                respect_rate_limits: true,
                batch_processing: false,
                performance_monitoring: true,
            },
            variables: TimerVariables {
                custom_variables: HashMap::new(),
                dynamic_variables: Vec::new(),
                api_variables: Vec::new(),
            },
            analytics: TimerAnalytics {
                track_effectiveness: true,
                track_click_through: false,
                track_engagement: true,
                retention_days: 30,
            },
        }
    }
}

impl Default for BotConfiguration {
    fn default() -> Self {
        Self {
            version: "2.0".to_string(),
            description: "NotaBot Core Configuration with Platform Integration".to_string(),
            last_updated: chrono::Utc::now(),
            core: CoreBotSettings {
                bot_name: "NotaBot".to_string(),
                global_prefix: "!".to_string(),
                response_delay_ms: 100,
                max_message_length: 500,
                rate_limit_per_minute: 20,
                debug_mode: false,
                log_level: "info".to_string(),
            },
            platforms: HashMap::new(),
            features: FeatureFlags {
                ai_moderation: true,
                advanced_patterns: true,
                smart_escalation: true,
                real_time_analytics: true,
                community_features: false,
                auto_optimization: false,
                learning_mode: true,
                beta_features: false,
            },
            performance: PerformanceSettings {
                max_memory_mb: 256,
                max_cpu_percent: 80,
                cache_size_mb: 64,
                worker_threads: 4,
                batch_size: 100,
                monitoring_enabled: true,
            },
            security: SecuritySettings {
                encryption_enabled: true,
                api_key_rotation_days: 90,
                max_failed_attempts: 5,
                ip_whitelist: Vec::new(),
                audit_logging: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_manager_initialization() {
        let temp_dir = tempdir().unwrap();
        let config_manager = ConfigurationManager::new(temp_dir.path());
        
        let result = config_manager.initialize().await;
        assert!(result.is_ok());
        
        // Check that default files were created
        assert!(temp_dir.path().join("filters.yaml").exists());
        assert!(temp_dir.path().join("patterns.yaml").exists());
        assert!(temp_dir.path().join("timers.yaml").exists());
        assert!(temp_dir.path().join("bot.yaml").exists());
    }

    #[tokio::test]
    async fn test_filter_crud_operations() {
        let temp_dir = tempdir().unwrap();
        let config_manager = ConfigurationManager::new(temp_dir.path());
        config_manager.initialize().await.unwrap();

        // Test adding a filter
        let test_filter = EnhancedBlacklistFilter {
            id: "test_filter".to_string(),
            name: "Test Filter".to_string(),
            enabled: true,
            description: Some("Test description".to_string()),
            category: "test".to_string(),
            priority: 5,
            patterns: vec![PatternDefinition {
                pattern_type: "literal".to_string(),
                value: "test".to_string(),
                weight: 1.0,
                description: None,
                enabled: true,
            }],
            case_sensitive: false,
            whole_words_only: false,
            regex_flags: None,
            timeout_seconds: Some(300),
            escalation_enabled: false,
            custom_message: None,
            silent_mode: false,
            exemption_level: "None".to_string(),
            exempt_users: Vec::new(),
            exempt_platforms: Vec::new(),
            active_hours: None,
            active_days: None,
            min_account_age_days: None,
            min_follow_time_days: None,
            track_effectiveness: true,
            auto_disable_threshold: None,
            tags: vec!["test".to_string()],
            ai_enabled: false,
            confidence_threshold: None,
            learning_enabled: false,
        };

        // Add filter
        let result = config_manager.add_filter(test_filter.clone()).await;
        assert!(result.is_ok());

        // Get filter by category
        let filters = config_manager.get_filters_by_category("test").await;
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].id, "test_filter");

        // Toggle filter
        let result = config_manager.toggle_filter("test_filter", false).await;
        assert!(result.is_ok());

        // Remove filter
        let result = config_manager.remove_filter("test_filter").await;
        assert!(result.is_ok());

        // Verify removal
        let filters = config_manager.get_filters_by_category("test").await;
        assert_eq!(filters.len(), 0);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let temp_dir = tempdir().unwrap();
        let config_manager = ConfigurationManager::new(temp_dir.path());
        config_manager.initialize().await.unwrap();

        let report = config_manager.validate_all_configs().await.unwrap();
        assert!(report.filter_config_valid);
        assert!(report.pattern_config_valid);
        assert!(report.errors.is_empty());
    }

    #[tokio::test]
    async fn test_nightbot_import_export() {
        let temp_dir = tempdir().unwrap();
        let config_manager = ConfigurationManager::new(temp_dir.path());
        config_manager.initialize().await.unwrap();

        // Test export to NightBot format
        let export_path = temp_dir.path().join("export.json");
        let result = config_manager.export_config("nightbot", &export_path).await;
        assert!(result.is_ok());
        assert!(export_path.exists());

        // Test import from NightBot format
        let nightbot_data = serde_json::json!({
            "filters": [
                {
                    "name": "Test Filter",
                    "patterns": ["*test*", "~/regex/i"],
                    "enabled": true,
                    "timeout": 600,
                    "exemptionLevel": "Subscriber"
                }
            ]
        });

        let import_path = temp_dir.path().join("import.json");
        fs::write(&import_path, serde_json::to_string(&nightbot_data).unwrap()).await.unwrap();

        let result = config_manager.import_config("nightbot", &import_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_backup_creation() {
        let temp_dir = tempdir().unwrap();
        let config_manager = ConfigurationManager::new(temp_dir.path());
        config_manager.initialize().await.unwrap();

        let backup_path = config_manager.create_backup().await.unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.extension().unwrap() == "gz");
    }
}
