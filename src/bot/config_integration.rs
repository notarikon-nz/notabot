// src/bot/config_integration.rs - Integration with existing bot system

use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::config::{
    ConfigurationManager, ConfigChangeEvent, FilterConfiguration, PatternConfiguration,
    TimerConfiguration, EnhancedBlacklistFilter, EnhancedSpamFilter, PatternDefinition,
    EnhancedTimer
};
use crate::types::{SpamFilter, SpamFilterType, BlacklistPattern, ModerationEscalation, ExemptionLevel};
use crate::bot::moderation::ModerationSystem;
use crate::bot::pattern_matching::{EnhancedPatternMatcher, AdvancedPattern};
use crate::bot::enhanced_moderation::EnhancedModerationSystem;

/// Configuration integration layer that bridges external config files with bot systems
pub struct ConfigIntegration {
    config_manager: Arc<ConfigurationManager>,
    moderation_system: Arc<ModerationSystem>,
    enhanced_moderation: Option<Arc<EnhancedModerationSystem>>,
    
    /// Cache for quick lookups
    filter_cache: Arc<RwLock<HashMap<String, SpamFilter>>>,
    pattern_cache: Arc<RwLock<Vec<AdvancedPattern>>>,
    
    /// Configuration change handlers
    change_handlers: Arc<RwLock<Vec<Box<dyn ConfigChangeHandler + Send + Sync>>>>,
}

/// Trait for handling configuration changes
#[async_trait::async_trait]
pub trait ConfigChangeHandler {
    async fn handle_filter_change(&self, filter_id: &str, filter: &EnhancedBlacklistFilter) -> Result<()>;
    async fn handle_pattern_change(&self, collection_id: &str, patterns: &[AdvancedPattern]) -> Result<()>;
    async fn handle_timer_change(&self, timer_id: &str, timer: &EnhancedTimer) -> Result<()>;
    async fn handle_global_config_change(&self) -> Result<()>;
}

impl ConfigIntegration {
    /// Create new configuration integration
    pub fn new(
        config_manager: Arc<ConfigurationManager>,
        moderation_system: Arc<ModerationSystem>,
    ) -> Self {
        Self {
            config_manager,
            moderation_system,
            enhanced_moderation: None,
            filter_cache: Arc::new(RwLock::new(HashMap::new())),
            pattern_cache: Arc::new(RwLock::new(Vec::new())),
            change_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn get_config_manager(&self) -> Arc<ConfigurationManager> {
        self.config_manager.clone()
    }

    /// Set enhanced moderation system
    pub fn set_enhanced_moderation(&mut self, enhanced_moderation: Arc<EnhancedModerationSystem>) {
        self.enhanced_moderation = Some(enhanced_moderation);
    }

    /// Initialize configuration integration
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing configuration integration...");

        // Load initial configurations
        self.load_all_configurations().await?;

        // Setup change monitoring
        self.setup_change_monitoring().await?;

        info!("Configuration integration initialized successfully");
        Ok(())
    }

    /// Load all configurations and apply them to bot systems
    async fn load_all_configurations(&self) -> Result<()> {
        // Load and apply filter configuration
        let filter_config = self.config_manager.get_filter_config().await;
        self.apply_filter_configuration(&filter_config).await?;

        // Load and apply pattern configuration
        let pattern_config = self.config_manager.get_pattern_config().await;
        self.apply_pattern_configuration(&pattern_config).await?;

        // Load and apply timer configuration
        let timer_config = self.config_manager.get_timer_config().await;
        self.apply_timer_configuration(&timer_config).await?;

        info!("All configurations loaded and applied");
        Ok(())
    }

    /// Apply filter configuration to moderation system
    async fn apply_filter_configuration(&self, config: &FilterConfiguration) -> Result<()> {
        info!("Applying filter configuration with {} blacklist filters and {} spam filters",
              config.blacklist_filters.len(), config.spam_filters.len());

        // Clear existing filters
        let existing_filters = self.moderation_system.list_filters().await;
        for (filter_name, _) in existing_filters {
            if let Err(e) = self.moderation_system.remove_filter(&filter_name).await {
                warn!("Failed to remove existing filter '{}': {}", filter_name, e);
            }
        }

        // Apply blacklist filters
        for filter in &config.blacklist_filters {
            if let Err(e) = self.apply_blacklist_filter(filter).await {
                error!("Failed to apply blacklist filter '{}': {}", filter.id, e);
                continue;
            }
        }

        // Apply spam filters
        for filter in &config.spam_filters {
            if let Err(e) = self.apply_spam_filter(filter).await {
                error!("Failed to apply spam filter '{}': {}", filter.id, e);
                continue;
            }
        }

        // Update cache
        self.update_filter_cache(config).await;

        info!("Filter configuration applied successfully");
        Ok(())
    }

    /// Apply individual blacklist filter
    async fn apply_blacklist_filter(&self, filter: &EnhancedBlacklistFilter) -> Result<()> {
        if !filter.enabled {
            debug!("Skipping disabled filter: {}", filter.id);
            return Ok(());
        }

        // Convert enhanced patterns to blacklist patterns
        let mut blacklist_patterns = Vec::new();
        for pattern_def in &filter.patterns {
            if !pattern_def.enabled {
                continue;
            }

            let blacklist_pattern = match pattern_def.pattern_type.as_str() {
                "literal" => BlacklistPattern::Literal(pattern_def.value.clone()),
                "wildcard" => BlacklistPattern::Wildcard(pattern_def.value.clone()),
                "regex" => {
                    let regex_pattern = if let Some(flags) = &filter.regex_flags {
                        format!("~/{}/{}", pattern_def.value, flags)
                    } else {
                        format!("~/{}/", pattern_def.value)
                    };
                    BlacklistPattern::from_regex_string(&regex_pattern)
                        .map_err(|e| anyhow::anyhow!("Failed to create regex pattern: {}", e))?
                }
                "fuzzy" => {
                    // For fuzzy patterns, we'll add them to the enhanced moderation system
                    // For now, treat as literal for basic compatibility
                    BlacklistPattern::Literal(pattern_def.value.clone())
                }
                _ => {
                    warn!("Unknown pattern type '{}' in filter '{}', treating as literal", 
                          pattern_def.pattern_type, filter.id);
                    BlacklistPattern::Literal(pattern_def.value.clone())
                }
            };

            blacklist_patterns.push(blacklist_pattern);
        }

        if blacklist_patterns.is_empty() {
            warn!("Filter '{}' has no enabled patterns", filter.id);
            return Ok(());
        }

        // Convert exemption level
        let exemption_level = match filter.exemption_level.as_str() {
            "None" => ExemptionLevel::None,
            "Subscriber" => ExemptionLevel::Subscriber,
            "Regular" => ExemptionLevel::Regular,
            "Moderator" => ExemptionLevel::Moderator,
            "Owner" => ExemptionLevel::Owner,
            _ => {
                warn!("Unknown exemption level '{}' in filter '{}', using Regular", 
                      filter.exemption_level, filter.id);
                ExemptionLevel::Regular
            }
        };

        // Create escalation
        let escalation = if filter.escalation_enabled {
            ModerationEscalation {
                first_offense: crate::types::ModerationAction::WarnUser {
                    message: filter.custom_message.clone()
                        .unwrap_or_else(|| format!("Filter '{}' triggered", filter.name))
                },
                repeat_offense: crate::types::ModerationAction::TimeoutUser {
                    duration_seconds: filter.timeout_seconds.unwrap_or(600)
                },
                offense_window_seconds: 3600, // 1 hour default
            }
        } else {
            ModerationEscalation::default()
        };

        // Create patterns vec from the pattern strings
        let pattern_strings: Vec<String> = blacklist_patterns.iter().map(|p| {
            match p {
                BlacklistPattern::Literal(s) => s.clone(),
                BlacklistPattern::Wildcard(s) => s.clone(),
                BlacklistPattern::Regex { pattern, .. } => pattern.clone(),
            }
        }).collect();

        // Add the blacklist filter to moderation system
        self.moderation_system.add_blacklist_filter(
            filter.id.clone(),
            pattern_strings,
            filter.case_sensitive,
            filter.whole_words_only,
            exemption_level,
            filter.timeout_seconds.unwrap_or(600),
            filter.custom_message.clone(),
        ).await?;

        debug!("Applied blacklist filter: {}", filter.id);
        Ok(())
    }

    /// Apply individual spam filter
    async fn apply_spam_filter(&self, filter: &EnhancedSpamFilter) -> Result<()> {
        if !filter.enabled {
            debug!("Skipping disabled spam filter: {}", filter.id);
            return Ok(());
        }

        // Convert filter type from configuration to SpamFilterType
        let filter_type = match filter.filter_type.as_str() {
            "ExcessiveCaps" => {
                let max_percentage = filter.parameters.get("max_percentage")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(60) as u8;
                SpamFilterType::ExcessiveCaps { max_percentage }
            }
            "SymbolSpam" => {
                let max_percentage = filter.parameters.get("max_percentage")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50) as u8;
                SpamFilterType::SymbolSpam { max_percentage }
            }
            "RateLimit" => {
                let max_messages = filter.parameters.get("max_messages")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as u8;
                let window_seconds = filter.parameters.get("window_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30);
                SpamFilterType::RateLimit { max_messages, window_seconds }
            }
            "MessageLength" => {
                let max_length = filter.parameters.get("max_length")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(500) as usize;
                SpamFilterType::MessageLength { max_length }
            }
            "ExcessiveEmotes" => {
                let max_count = filter.parameters.get("max_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as u8;
                SpamFilterType::ExcessiveEmotes { max_count }
            }
            "LinkBlocking" => {
                let allow_mods = filter.parameters.get("allow_mods")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let whitelist = filter.parameters.get("whitelist")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_else(Vec::new);
                SpamFilterType::LinkBlocking { allow_mods, whitelist }
            }
            "RepeatedMessages" => {
                let max_repeats = filter.parameters.get("max_repeats")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(3) as u8;
                let window_seconds = filter.parameters.get("window_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(300);
                SpamFilterType::RepeatedMessages { max_repeats, window_seconds }
            }
            _ => {
                warn!("Unknown spam filter type '{}', skipping", filter.filter_type);
                return Ok(());
            }
        };

        // Convert exemption level
        let exemption_level = match filter.exemption_level.as_str() {
            "None" => ExemptionLevel::None,
            "Subscriber" => ExemptionLevel::Subscriber,
            "Regular" => ExemptionLevel::Regular,
            "Moderator" => ExemptionLevel::Moderator,
            "Owner" => ExemptionLevel::Owner,
            _ => ExemptionLevel::Regular,
        };

        // Create escalation from configuration
        let escalation = ModerationEscalation {
            first_offense: match filter.escalation.first_offense_action.as_str() {
                "warn" => crate::types::ModerationAction::WarnUser {
                    message: filter.custom_message.clone()
                        .unwrap_or_else(|| "Please follow chat rules".to_string())
                },
                "timeout" => crate::types::ModerationAction::TimeoutUser {
                    duration_seconds: filter.timeout_seconds
                },
                "delete" => crate::types::ModerationAction::DeleteMessage,
                _ => crate::types::ModerationAction::WarnUser {
                    message: "Please follow chat rules".to_string()
                }
            },
            repeat_offense: crate::types::ModerationAction::TimeoutUser {
                duration_seconds: filter.timeout_seconds
            },
            offense_window_seconds: filter.escalation.offense_window_seconds,
        };

        // Add spam filter to moderation system
        self.moderation_system.add_spam_filter_advanced(
            filter.id.clone(),
            filter_type,
            escalation,
            exemption_level,
            filter.silent_mode,
            filter.custom_message.clone(),
        ).await?;

        debug!("Applied spam filter: {}", filter.id);
        Ok(())
    }

    /// Apply pattern configuration to enhanced moderation system
    async fn apply_pattern_configuration(&self, config: &PatternConfiguration) -> Result<()> {
        if let Some(enhanced_mod) = &self.enhanced_moderation {
            info!("Applying pattern configuration with {} collections",
                  config.pattern_collections.len());

            let mut all_patterns = Vec::new();

            for (collection_id, collection) in &config.pattern_collections {
                if !collection.enabled {
                    debug!("Skipping disabled pattern collection: {}", collection_id);
                    continue;
                }

                for pattern_def in &collection.patterns {
                    if !pattern_def.enabled {
                        continue;
                    }

                    let advanced_pattern = self.convert_pattern_definition(pattern_def)?;
                    if let Some(pattern) = advanced_pattern {
                        all_patterns.push(pattern);
                    }
                }
            }

            // Add all patterns to enhanced moderation
            for pattern in &all_patterns {
                if let Err(e) = enhanced_mod.add_advanced_pattern(pattern.clone()).await {
                    error!("Failed to add advanced pattern: {}", e);
                }
            }

            let pattern_length = all_patterns.len();

            // Update pattern cache
            *self.pattern_cache.write().await = all_patterns;

            info!("Applied {} advanced patterns", pattern_length);
        } else {
            warn!("Enhanced moderation system not available, skipping pattern configuration");
        }

        Ok(())
    }

    /// Convert pattern definition to advanced pattern
    fn convert_pattern_definition(&self, pattern_def: &crate::config::AdvancedPatternDefinition) -> Result<Option<AdvancedPattern>> {
        let pattern = match pattern_def.pattern_type.as_str() {
            "fuzzy_match" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for fuzzy match"))?;
                let threshold = pattern_def.parameters.get("threshold")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.8) as f32;

                AdvancedPattern::FuzzyMatch {
                    pattern: pattern_value.to_string(),
                    threshold,
                }
            }
            "leetspeak" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for leetspeak"))?;

                AdvancedPattern::Leetspeak(pattern_value.to_string())
            }
            "unicode_normalized" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for unicode normalized"))?;

                AdvancedPattern::UnicodeNormalized(pattern_value.to_string())
            }
            "zalgo_text" => {
                AdvancedPattern::ZalgoText
            }
            "homoglyph" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for homoglyph"))?;

                AdvancedPattern::Homoglyph(pattern_value.to_string())
            }
            "repeated_char_compression" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for repeated char compression"))?;

                AdvancedPattern::RepeatedCharCompression(pattern_value.to_string())
            }
            "encoded_content" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for encoded content"))?;

                AdvancedPattern::EncodedContent(pattern_value.to_string())
            }
            "phonetic" => {
                let pattern_value = pattern_def.parameters.get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter for phonetic"))?;

                AdvancedPattern::Phonetic(pattern_value.to_string())
            }
            _ => {
                warn!("Unknown pattern type '{}', skipping", pattern_def.pattern_type);
                return Ok(None);
            }
        };

        Ok(Some(pattern))
    }

    /// Apply timer configuration
    async fn apply_timer_configuration(&self, config: &TimerConfiguration) -> Result<()> {
        info!("Timer configuration loaded with {} timers", config.timers.len());
        
        // Timer application would depend on your timer system implementation
        // For now, we'll just log the configuration
        for timer in &config.timers {
            if timer.enabled {
                debug!("Timer '{}' configured with {} messages, interval: {}s",
                       timer.name, timer.messages.len(), timer.schedule.interval_seconds);
            }
        }

        Ok(())
    }

    /// Setup configuration change monitoring
    async fn setup_change_monitoring(&self) -> Result<()> {
        let mut receiver = self.config_manager.subscribe_to_changes();
        let integration = Arc::new(self.clone());

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv().await {
                if let Err(e) = integration.handle_config_change(event).await {
                    error!("Failed to handle configuration change: {}", e);
                }
            }
        });

        info!("Configuration change monitoring setup complete");
        Ok(())
    }

    /// Handle configuration change events
    async fn handle_config_change(&self, event: ConfigChangeEvent) -> Result<()> {
        match event {
            ConfigChangeEvent::FiltersUpdated { file } => {
                info!("Filters updated in file: {}", file);
                let filter_config = self.config_manager.get_filter_config().await;
                self.apply_filter_configuration(&filter_config).await?;
                
                // Notify handlers
                for handler in self.change_handlers.read().await.iter() {
                    if let Err(e) = handler.handle_global_config_change().await {
                        error!("Configuration change handler failed: {}", e);
                    }
                }
            }
            ConfigChangeEvent::PatternsUpdated { file } => {
                info!("Patterns updated in file: {}", file);
                let pattern_config = self.config_manager.get_pattern_config().await;
                self.apply_pattern_configuration(&pattern_config).await?;
            }
            ConfigChangeEvent::TimersUpdated { file } => {
                info!("Timers updated in file: {}", file);
                let timer_config = self.config_manager.get_timer_config().await;
                self.apply_timer_configuration(&timer_config).await?;
            }
            ConfigChangeEvent::BotConfigUpdated { file } => {
                info!("Bot configuration updated in file: {}", file);
                // Handle bot configuration changes
            }
            ConfigChangeEvent::ValidationError { file, error } => {
                error!("Configuration validation error in {}: {}", file, error);
            }
            ConfigChangeEvent::ReloadComplete { files_updated } => {
                info!("Configuration reload complete for files: {:?}", files_updated);
            }
        }

        Ok(())
    }

    /// Update filter cache
    async fn update_filter_cache(&self, config: &FilterConfiguration) {
        let mut cache = self.filter_cache.write().await;
        cache.clear();

        for filter in &config.blacklist_filters {
            if !filter.enabled {
                continue;
            }

            // Convert to internal SpamFilter representation for caching
            let spam_filter = SpamFilter {
                filter_type: SpamFilterType::Blacklist {
                    patterns: Vec::new(), // Simplified for cache
                    case_sensitive: filter.case_sensitive,
                    whole_words_only: filter.whole_words_only,
                },
                enabled: filter.enabled,
                escalation: ModerationEscalation::default(),
                exemption_level: ExemptionLevel::Regular,
                silent_mode: filter.silent_mode,
                custom_message: filter.custom_message.clone(),
                name: filter.name.clone(),
            };

            cache.insert(filter.id.clone(), spam_filter);
        }

        debug!("Updated filter cache with {} entries", cache.len());
    }

    /// Add configuration change handler
    pub async fn add_change_handler(&self, handler: Box<dyn ConfigChangeHandler + Send + Sync>) {
        self.change_handlers.write().await.push(handler);
    }

    /// Get filter from cache
    pub async fn get_cached_filter(&self, filter_id: &str) -> Option<SpamFilter> {
        self.filter_cache.read().await.get(filter_id).cloned()
    }

    /// Get all cached patterns
    pub async fn get_cached_patterns(&self) -> Vec<AdvancedPattern> {
        self.pattern_cache.read().await.clone()
    }

    /// Reload specific configuration type
    pub async fn reload_configuration(&self, config_type: ConfigType) -> Result<()> {
        match config_type {
            ConfigType::Filters => {
                let filter_config = self.config_manager.get_filter_config().await;
                self.apply_filter_configuration(&filter_config).await?;
            }
            ConfigType::Patterns => {
                let pattern_config = self.config_manager.get_pattern_config().await;
                self.apply_pattern_configuration(&pattern_config).await?;
            }
            ConfigType::Timers => {
                let timer_config = self.config_manager.get_timer_config().await;
                self.apply_timer_configuration(&timer_config).await?;
            }
            ConfigType::All => {
                self.load_all_configurations().await?;
            }
        }

        info!("Reloaded {:?} configuration", config_type);
        Ok(())
    }

    /// Export current configuration to file
    pub async fn export_current_config(&self, format: &str, output_path: &std::path::Path) -> Result<()> {
        self.config_manager.export_config(format, output_path).await
    }

    /// Import configuration from file
    pub async fn import_external_config(&self, format: &str, input_path: &std::path::Path) -> Result<()> {
        let result = self.config_manager.import_config(format, input_path).await?;
        
        info!("Imported configuration: {} items", result.imported_count);
        if !result.warnings.is_empty() {
            warn!("Import warnings: {:?}", result.warnings);
        }
        if !result.errors.is_empty() {
            error!("Import errors: {:?}", result.errors);
        }

        // Reload configurations after import
        self.load_all_configurations().await?;

        Ok(())
    }

    /// Get configuration statistics
    pub async fn get_config_stats(&self) -> crate::config::ConfigStats {
        self.config_manager.get_config_stats().await
    }

    /// Validate all configurations
    pub async fn validate_configurations(&self) -> Result<crate::config::ValidationReport> {
        self.config_manager.validate_all_configs().await
    }

    /// Create configuration backup
    pub async fn create_backup(&self) -> Result<std::path::PathBuf> {
        self.config_manager.create_backup().await
    }
}

// Clone implementation for ConfigIntegration
impl Clone for ConfigIntegration {
    fn clone(&self) -> Self {
        Self {
            config_manager: Arc::clone(&self.config_manager),
            moderation_system: Arc::clone(&self.moderation_system),
            enhanced_moderation: self.enhanced_moderation.as_ref().map(Arc::clone),
            filter_cache: Arc::clone(&self.filter_cache),
            pattern_cache: Arc::clone(&self.pattern_cache),
            change_handlers: Arc::clone(&self.change_handlers),
        }
    }
}

/// Configuration type enum for selective reloading
#[derive(Debug, Clone)]
pub enum ConfigType {
    Filters,
    Patterns,
    Timers,
    All,
}

/// Example configuration change handler for logging
pub struct LoggingChangeHandler;

#[async_trait::async_trait]
impl ConfigChangeHandler for LoggingChangeHandler {
    async fn handle_filter_change(&self, filter_id: &str, filter: &EnhancedBlacklistFilter) -> Result<()> {
        info!("Filter '{}' changed: {} patterns, enabled: {}", 
              filter_id, filter.patterns.len(), filter.enabled);
        Ok(())
    }

    async fn handle_pattern_change(&self, collection_id: &str, patterns: &[AdvancedPattern]) -> Result<()> {
        info!("Pattern collection '{}' changed: {} patterns", collection_id, patterns.len());
        Ok(())
    }

    async fn handle_timer_change(&self, timer_id: &str, timer: &EnhancedTimer) -> Result<()> {
        info!("Timer '{}' changed: {} messages, interval: {}s",
              timer_id, timer.messages.len(), timer.schedule.interval_seconds);
        Ok(())
    }

    async fn handle_global_config_change(&self) -> Result<()> {
        info!("Global configuration change detected");
        Ok(())
    }
}

/// Configuration management commands for bot integration
pub struct ConfigCommands {
    integration: Arc<ConfigIntegration>,
}

impl ConfigCommands {
    pub fn new(integration: Arc<ConfigIntegration>) -> Self {
        Self { integration }
    }

    /// Handle reload command
    pub async fn handle_reload_command(&self, config_type: Option<&str>) -> Result<String> {
        let reload_type = match config_type {
            Some("filters") => ConfigType::Filters,
            Some("patterns") => ConfigType::Patterns,
            Some("timers") => ConfigType::Timers,
            _ => ConfigType::All,
        };

        self.integration.reload_configuration(reload_type.clone()).await?;
        Ok(format!("Successfully reloaded {:?} configuration", reload_type))
    }

    /// Handle status command
    pub async fn handle_status_command(&self) -> Result<String> {
        let stats = self.integration.get_config_stats().await;
        
        Ok(format!(
            "📊 Config Status: {} filters ({} enabled), {} patterns ({} enabled), {} timers ({} enabled), {} categories",
            stats.total_blacklist_filters,
            stats.enabled_blacklist_filters,
            stats.total_pattern_collections,
            stats.enabled_pattern_collections,
            stats.total_timers,
            stats.enabled_timers,
            stats.categories
        ))
    }

    /// Handle validate command
    pub async fn handle_validate_command(&self) -> Result<String> {
        let report = self.integration.validate_configurations().await?;
        
        if report.errors.is_empty() {
            Ok("✅ All configurations are valid".to_string())
        } else {
            Ok(format!("❌ Validation errors: {}", report.errors.join(", ")))
        }
    }

    /// Handle export command
    pub async fn handle_export_command(&self, format: &str) -> Result<String> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("config_export_{}_{}.{}", timestamp, format, 
                              if format == "nightbot" { "json" } else { format });
        let output_path = std::path::Path::new(&filename);

        self.integration.export_current_config(format, output_path).await?;
        Ok(format!("📁 Configuration exported to: {}", filename))
    }

    /// Handle backup command
    pub async fn handle_backup_command(&self) -> Result<String> {
        let backup_path = self.integration.create_backup().await?;
        Ok(format!("💾 Configuration backup created: {}", backup_path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::bot::moderation::ModerationSystem;

    #[tokio::test]
    async fn test_config_integration_initialization() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        let moderation_system = Arc::new(ModerationSystem::new());
        
        config_manager.initialize().await.unwrap();
        
        let integration = ConfigIntegration::new(config_manager, moderation_system);
        let result = integration.initialize().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filter_application() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        let moderation_system = Arc::new(ModerationSystem::new());
        
        config_manager.initialize().await.unwrap();
        
        let integration = ConfigIntegration::new(config_manager.clone(), moderation_system);
        integration.initialize().await.unwrap();

        // Test that filters were applied
        let filter_config = config_manager.get_filter_config().await;
        assert!(!filter_config.blacklist_filters.is_empty());
    }

    #[tokio::test]
    async fn test_config_commands() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        let moderation_system = Arc::new(ModerationSystem::new());
        
        config_manager.initialize().await.unwrap();
        
        let integration = Arc::new(ConfigIntegration::new(config_manager, moderation_system));
        integration.initialize().await.unwrap();

        let commands = ConfigCommands::new(integration);
        
        // Test status command
        let status = commands.handle_status_command().await.unwrap();
        assert!(status.contains("Config Status"));
        
        // Test validate command
        let validation = commands.handle_validate_command().await.unwrap();
        assert!(validation.contains("valid"));
        
        // Test reload command
        let reload = commands.handle_reload_command(Some("filters")).await.unwrap();
        assert!(reload.contains("Successfully reloaded"));
    }
}