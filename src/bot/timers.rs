use anyhow::{Result, Context};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::fs;
use tokio::sync::RwLock;
use tokio::time::Duration;

use crate::platforms::PlatformConnection;
use crate::types::BotTimer;

// Include the timer configuration structs from the same module
use crate::types::{
    TimerConfig, GlobalTimerSettings, TimerDefinition, TimerVariables, 
    VariableDefinition, TimerAnalytics, TimerRules
};

pub struct TimerSystem {
    pub timers: Arc<RwLock<HashMap<String, BotTimer>>>,
    config_path: PathBuf,
    timer_config: Arc<RwLock<TimerConfig>>,
    custom_variables: Arc<RwLock<HashMap<String, String>>>,
    shutdown_signal: Arc<AtomicBool>,
}

impl TimerSystem {
    pub fn new() -> Self {
        Self {
            timers: Arc::new(RwLock::new(HashMap::new())),
            config_path: PathBuf::from("timers.yaml"),
            timer_config: Arc::new(RwLock::new(TimerConfig::default())),
            custom_variables: Arc::new(RwLock::new(HashMap::new())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create timer system with custom config path
    pub fn with_config_path<P: AsRef<Path>>(config_path: P) -> Self {
        Self {
            timers: Arc::new(RwLock::new(HashMap::new())),
            config_path: config_path.as_ref().to_path_buf(),
            timer_config: Arc::new(RwLock::new(TimerConfig::default())),
            custom_variables: Arc::new(RwLock::new(HashMap::new())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Load timer configuration from YAML file
    pub async fn load_config(&self) -> Result<()> {
        if !self.config_path.exists() {
            warn!("Timer config file not found, creating default: {}", self.config_path.display());
            self.create_default_config().await?;
        }

        let content = fs::read_to_string(&self.config_path).await
            .with_context(|| format!("Failed to read timer config: {}", self.config_path.display()))?;

        let config: TimerConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse timer config: {}", self.config_path.display()))?;

        // Validate configuration
        self.validate_config(&config)?;

        // Update configuration
        *self.timer_config.write().await = config.clone();

        // Load timers from configuration
        self.load_timers_from_config(config).await?;

        info!("Loaded {} timers from configuration", self.timers.read().await.len());
        Ok(())
    }

    /// Create default timer configuration file
    async fn create_default_config(&self) -> Result<()> {
        let default_config = self.create_comprehensive_default_config();
        let yaml_content = serde_yaml::to_string(&default_config)
            .context("Failed to serialize default timer config")?;

        fs::write(&self.config_path, yaml_content).await
            .with_context(|| format!("Failed to write default config to: {}", self.config_path.display()))?;

        info!("Created default timer configuration at: {}", self.config_path.display());
        Ok(())
    }

    /// Create a comprehensive default configuration
    fn create_comprehensive_default_config(&self) -> TimerConfig {
        TimerConfig {
            version: "1.0".to_string(),
            description: "NotaBot AI-Enhanced Timer Configuration".to_string(),
            global_settings: GlobalTimerSettings {
                minimum_interval_seconds: 30,
                auto_reload: true,
                variable_substitution: true,
                platform_targeting: true,
            },
            timers: vec![
                TimerDefinition {
                    name: "ai_features".to_string(),
                    enabled: true,
                    message: "This stream is protected by NotaBot's AI moderation! Features: Smart pattern detection, learning algorithms, real-time optimization".to_string(),
                    interval_seconds: 900, // 15 minutes
                    channels: vec![],
                    platforms: vec![],
                    description: Some("Showcase AI moderation features".to_string()),
                    tags: Some(vec!["ai".to_string(), "features".to_string(), "promotion".to_string()]),
                    variables: None,
                },
                TimerDefinition {
                    name: "community_ai".to_string(),
                    enabled: true,
                    message: "Our AI learns from community feedback! Use !appeal if you think moderation made a mistake - it helps the AI improve!".to_string(),
                    interval_seconds: 1200, // 20 minutes
                    channels: vec![],
                    platforms: vec![],
                    description: Some("Explain AI learning capabilities".to_string()),
                    tags: Some(vec!["ai".to_string(), "community".to_string(), "education".to_string()]),
                    variables: None,
                },
                TimerDefinition {
                    name: "ai_vs_nightbot".to_string(),
                    enabled: true,
                    message: "Why NotaBot > NightBot: 10x faster response, AI pattern detection, automatic optimization, community filter sharing, 99.9% uptime!".to_string(),
                    interval_seconds: 1800, // 30 minutes
                    channels: vec![],
                    platforms: vec![],
                    description: Some("Compare NotaBot advantages".to_string()),
                    tags: Some(vec!["comparison".to_string(), "nightbot".to_string(), "superiority".to_string()]),
                    variables: None,
                },
                TimerDefinition {
                    name: "twitch_ai_exclusive".to_string(),
                    enabled: true,
                    message: "Twitch Exclusive: Our AI detects even advanced evasion techniques! Leetspeak, Unicode tricks, homoglyphs - nothing gets past!".to_string(),
                    interval_seconds: 1500, // 25 minutes
                    channels: vec![],
                    platforms: vec!["twitch".to_string()],
                    description: Some("Twitch-specific AI features".to_string()),
                    tags: Some(vec!["twitch".to_string(), "ai".to_string(), "exclusive".to_string()]),
                    variables: None,
                },
                TimerDefinition {
                    name: "youtube_ai_exclusive".to_string(),
                    enabled: true,
                    message: "YouTube Exclusive: Cross-platform AI intelligence! Patterns learned on Twitch protect YouTube chat too!".to_string(),
                    interval_seconds: 1500, // 25 minutes
                    channels: vec![],
                    platforms: vec!["youtube".to_string()],
                    description: Some("YouTube-specific AI features".to_string()),
                    tags: Some(vec!["youtube".to_string(), "ai".to_string(), "cross-platform".to_string()]),
                    variables: None,
                },
                TimerDefinition {
                    name: "points_economy".to_string(),
                    enabled: true,
                    message: "Earn points by chatting and being positive! Check your balance with !points - spend them on rewards! Use !leaderboard to see top contributors".to_string(),
                    interval_seconds: 1800, // 30 minutes
                    channels: vec![],
                    platforms: vec![],
                    description: Some("Explain points system".to_string()),
                    tags: Some(vec!["points".to_string(), "economy".to_string(), "engagement".to_string()]),
                    variables: None,
                },
            ],
            categories: {
                let mut categories = HashMap::new();
                categories.insert("core".to_string(), vec!["ai_features".to_string(), "community_ai".to_string(), "ai_vs_nightbot".to_string()]);
                categories.insert("platform_specific".to_string(), vec!["twitch_ai_exclusive".to_string(), "youtube_ai_exclusive".to_string()]);
                categories.insert("engagement".to_string(), vec!["points_economy".to_string()]);
                categories
            },
            variables: TimerVariables {
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
                custom: vec![
                    VariableDefinition {
                        name: "$(discord)".to_string(),
                        description: "Discord server invite".to_string(),
                        example: Some("discord.gg/yourserver".to_string()),
                        default: Some("discord.gg/yourserver".to_string()),
                    },
                    VariableDefinition {
                        name: "$(twitter)".to_string(),
                        description: "Twitter handle".to_string(),
                        example: Some("@yourhandle".to_string()),
                        default: Some("@yourhandle".to_string()),
                    },
                ],
            },
            analytics: TimerAnalytics {
                track_effectiveness: true,
                track_engagement: true,
                track_click_through: false,
            },
            rules: TimerRules {
                max_timers_per_channel: 20,
                max_message_length: 500,
                min_interval_seconds: 30,
                max_interval_seconds: 86400,
            },
        }
    }

    /// Validate timer configuration
    fn validate_config(&self, config: &TimerConfig) -> Result<()> {
        for timer in &config.timers {
            if timer.interval_seconds < config.global_settings.minimum_interval_seconds {
                return Err(anyhow::anyhow!(
                    "Timer '{}' interval {}s is below minimum {}s",
                    timer.name,
                    timer.interval_seconds,
                    config.global_settings.minimum_interval_seconds
                ));
            }

            if timer.interval_seconds < config.rules.min_interval_seconds {
                return Err(anyhow::anyhow!(
                    "Timer '{}' interval {}s violates rules (min: {}s)",
                    timer.name,
                    timer.interval_seconds,
                    config.rules.min_interval_seconds
                ));
            }

            if timer.interval_seconds > config.rules.max_interval_seconds {
                return Err(anyhow::anyhow!(
                    "Timer '{}' interval {}s violates rules (max: {}s)",
                    timer.name,
                    timer.interval_seconds,
                    config.rules.max_interval_seconds
                ));
            }

            if timer.message.len() > config.rules.max_message_length {
                return Err(anyhow::anyhow!(
                    "Timer '{}' message is too long: {} > {} characters",
                    timer.name,
                    timer.message.len(),
                    config.rules.max_message_length
                ));
            }
        }

        info!("Timer configuration validation passed");
        Ok(())
    }

    /// Load timers from configuration into runtime timers
    async fn load_timers_from_config(&self, config: TimerConfig) -> Result<()> {
        let mut timers = self.timers.write().await;
        timers.clear();

        for timer_def in config.timers {
            if !timer_def.enabled {
                debug!("Skipping disabled timer: {}", timer_def.name);
                continue;
            }

            let bot_timer = BotTimer {
                name: timer_def.name.clone(),
                message: timer_def.message,
                interval_seconds: timer_def.interval_seconds,
                channels: timer_def.channels,
                platforms: timer_def.platforms,
                enabled: timer_def.enabled,
                last_triggered: None,
                trigger_count: 0,
            };

            timers.insert(timer_def.name.clone(), bot_timer);
            debug!("Loaded timer: {} ({}s interval)", timer_def.name, timer_def.interval_seconds);
        }

        info!("Loaded {} enabled timers from configuration", timers.len());
        Ok(())
    }

    /// Start file watcher for auto-reload
    async fn start_config_watcher(&self) {
        if !self.timer_config.read().await.global_settings.auto_reload {
            debug!("Timer config auto-reload is disabled");
            return;
        }

        let config_path = self.config_path.clone();
        let timer_system_handle = self.timers.clone();
        let timer_config_handle = self.timer_config.clone();
        let custom_variables_handle = self.custom_variables.clone();
        let shutdown_signal = Arc::clone(&self.shutdown_signal);

        tokio::spawn(async move {
            let mut last_modified = std::fs::metadata(&config_path)
                .and_then(|m| m.modified())
                .unwrap_or_else(|_| std::time::SystemTime::now());

            info!("Timer config file watcher started for: {}", config_path.display());

            loop {
                // Check for shutdown signal
                if shutdown_signal.load(Ordering::Relaxed) {
                    info!("Config watcher received shutdown signal");
                    break;
                }

                tokio::time::sleep(Duration::from_secs(30)).await; // Check every 30 seconds

                if let Ok(metadata) = std::fs::metadata(&config_path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified > last_modified {
                            info!("Timer config file changed, reloading...");
                            
                            // Create a temporary TimerSystem to load the new config
                            let temp_system = TimerSystem {
                                timers: timer_system_handle.clone(),
                                config_path: config_path.clone(),
                                timer_config: timer_config_handle.clone(),
                                custom_variables: custom_variables_handle.clone(),
                                shutdown_signal: shutdown_signal.clone(),
                            };

                            match temp_system.load_config().await {
                                Ok(_) => {
                                    info!("Timer configuration reloaded successfully");
                                }
                                Err(e) => {
                                    error!("Failed to reload timer config: {}", e);
                                }
                            }

                            last_modified = modified;
                        }
                    }
                }
            }
        });
    }

    /// Add a new timer that posts messages at regular intervals
    pub async fn add_timer(&self, name: String, message: String, interval_seconds: u64) -> Result<()> {
        self.add_timer_advanced(name, message, interval_seconds, Vec::new(), Vec::new()).await
    }

    /// Add a timer with specific channels and platforms
    pub async fn add_timer_advanced(
        &self, 
        name: String, 
        message: String, 
        interval_seconds: u64,
        channels: Vec<String>,
        platforms: Vec<String>
    ) -> Result<()> {
        let min_interval = self.timer_config.read().await.global_settings.minimum_interval_seconds;
        if interval_seconds < min_interval {
            return Err(anyhow::anyhow!("Timer interval must be at least {} seconds to prevent spam", min_interval));
        }

        let timer = BotTimer {
            name: name.clone(),
            message,
            interval_seconds,
            channels,
            platforms,
            enabled: true,
            last_triggered: None,
            trigger_count: 0,
        };

        self.timers.write().await.insert(name.clone(), timer);
        info!("Registered timer '{}' with interval {}s", name, interval_seconds);
        Ok(())
    }

    /// Enable or disable a specific timer
    pub async fn set_timer_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut timers_guard = self.timers.write().await;
        if let Some(timer) = timers_guard.get_mut(name) {
            timer.enabled = enabled;
            info!("Timer '{}' {}", name, if enabled { "enabled" } else { "disabled" });
            Ok(())
        } else {
            Err(anyhow::anyhow!("Timer '{}' not found", name))
        }
    }

    /// Remove a timer
    pub async fn remove_timer(&self, name: &str) -> Result<()> {
        let mut timers_guard = self.timers.write().await;
        if timers_guard.remove(name).is_some() {
            info!("Removed timer '{}'", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Timer '{}' not found", name))
        }
    }

    /// Get statistics for all timers
    pub async fn get_timer_stats(&self) -> HashMap<String, (bool, u64, Option<chrono::DateTime<chrono::Utc>>)> {
        let timers_guard = self.timers.read().await;
        let mut stats = HashMap::new();
        
        for (name, timer) in timers_guard.iter() {
            stats.insert(
                name.clone(), 
                (timer.enabled, timer.trigger_count, timer.last_triggered)
            );
        }
        
        stats
    }

    /// Get timer configuration categories
    pub async fn get_timer_categories(&self) -> HashMap<String, Vec<String>> {
        self.timer_config.read().await.categories.clone()
    }

    /// Enable/disable timers by category
    pub async fn set_category_enabled(&self, category: &str, enabled: bool) -> Result<usize> {
        let categories = self.timer_config.read().await.categories.clone();
        
        if let Some(timer_names) = categories.get(category) {
            let mut count = 0;
            for timer_name in timer_names {
                if self.set_timer_enabled(timer_name, enabled).await.is_ok() {
                    count += 1;
                }
            }
            info!("{} {} timers in category '{}'", 
                  if enabled { "Enabled" } else { "Disabled" }, count, category);
            Ok(count)
        } else {
            Err(anyhow::anyhow!("Category '{}' not found", category))
        }
    }

    /// Start the timer system that processes periodic messages
    pub async fn start_timer_system(
        &self, 
        connections: Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>
    ) -> Result<()> {
        // Load configuration first
        self.load_config().await?;

        // Start config file watcher
        self.start_config_watcher().await;

        let timers = Arc::clone(&self.timers);
        let timer_config = Arc::clone(&self.timer_config);
        let custom_variables = Arc::clone(&self.custom_variables);
        let shutdown_signal = Arc::clone(&self.shutdown_signal);
        
        let handle = tokio::spawn(async move {
            info!("Timer system started with configuration-based timers");
            let mut check_interval = tokio::time::interval(Duration::from_secs(10)); // Check every 10 seconds
            
            loop {
                // Check for shutdown signal
                if shutdown_signal.load(Ordering::Relaxed) {
                    info!("Timer system received shutdown signal");
                    break;
                }
                
                check_interval.tick().await;
                
                let now = chrono::Utc::now();
                let mut timers_to_trigger = Vec::new();
                
                // Check which timers need to be triggered
                {
                    let mut timers_guard = timers.write().await;
                    for (name, timer) in timers_guard.iter_mut() {
                        if !timer.enabled {
                            continue;
                        }
                        
                        let should_trigger = match timer.last_triggered {
                            Some(last) => {
                                let elapsed = now.signed_duration_since(last);
                                elapsed.num_seconds() >= timer.interval_seconds as i64
                            }
                            None => true, // First time running
                        };
                        
                        if should_trigger {
                            timer.last_triggered = Some(now);
                            timer.trigger_count += 1;
                            timers_to_trigger.push(timer.clone());
                            debug!("Timer '{}' triggered (count: {})", name, timer.trigger_count);
                        }
                    }
                }
                
                // Send timer messages
                for timer in timers_to_trigger {
                    if let Err(e) = Self::execute_timer_with_variables(
                        &timer, 
                        &connections, 
                        &timer_config,
                        &custom_variables
                    ).await {
                        error!("Failed to execute timer '{}': {}", timer.name, e);
                    }
                }
            }
        });
        
        // Store the handle in a way that doesn't require mutable access
        // We'll use a different approach - store the handle ID for potential cancellation
        let handle_id = handle.id();
        info!("Timer system initialized with file-based configuration (handle ID: {:?})", handle_id);
        
        // Let the handle run in the background
        tokio::spawn(async move {
            if let Err(e) = handle.await {
                error!("Timer system task failed: {}", e);
            }
        });
        
        Ok(())
    }

    /// Execute a timer by sending its message to appropriate channels (with variable substitution)
    async fn execute_timer_with_variables(
        timer: &BotTimer,
        connections: &Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>,
        timer_config: &Arc<RwLock<TimerConfig>>,
        custom_variables: &Arc<RwLock<HashMap<String, String>>>,
    ) -> Result<()> {
        let connections_guard = connections.read().await;
        let config = timer_config.read().await;
        let custom_vars = custom_variables.read().await;
        
        for (platform_name, connection) in connections_guard.iter() {
            // Check if this timer should post on this platform
            if !timer.platforms.is_empty() && !timer.platforms.contains(platform_name) {
                continue;
            }
            
            // Get channels for this platform
            let channels_to_post = if timer.channels.is_empty() {
                // Post to all channels this connection is active in
                connection.get_channels()
            } else {
                // Use specific channels defined for this timer
                timer.channels.clone()
            };
            
            for channel in channels_to_post {
                // Process message with variable substitution
                let mut processed_message = timer.message.clone();
                
                if config.global_settings.variable_substitution {
                    // Built-in variable substitution
                    processed_message = processed_message
                        .replace("$(timer)", &timer.name)
                        .replace("$(count)", &timer.trigger_count.to_string())
                        .replace("$(platform)", platform_name)
                        .replace("$(channel)", &channel);
                    
                    // Custom variable substitution
                    for (var_name, var_value) in custom_vars.iter() {
                        processed_message = processed_message.replace(var_name, var_value);
                    }
                    
                    // Environment variable substitution (for dynamic values)
                    if let Ok(discord_url) = std::env::var("DISCORD_URL") {
                        processed_message = processed_message.replace("$(discord)", &discord_url);
                    }
                    if let Ok(twitter_handle) = std::env::var("TWITTER_HANDLE") {
                        processed_message = processed_message.replace("$(twitter)", &twitter_handle);
                    }
                }
                
                if let Err(e) = connection.send_message(&channel, &processed_message).await {
                    error!("Failed to send timer message to {}#{}: {}", platform_name, channel, e);
                } else {
                    info!("Timer '{}' posted to {}#{}: {}", timer.name, platform_name, channel, processed_message);
                }
            }
        }
        
        Ok(())
    }

    /// Get timer analytics (if enabled)
    pub async fn get_timer_analytics(&self) -> HashMap<String, serde_json::Value> {
        let config = self.timer_config.read().await;
        let timers = self.timers.read().await;
        
        if !config.analytics.track_effectiveness {
            return HashMap::new();
        }

        let mut analytics = HashMap::new();
        
        // Basic analytics
        analytics.insert("total_timers".to_string(), serde_json::Value::Number(timers.len().into()));
        analytics.insert("enabled_timers".to_string(), serde_json::Value::Number(
            timers.values().filter(|t| t.enabled).count().into()
        ));
        
        let total_triggers: u64 = timers.values().map(|t| t.trigger_count).sum();
        analytics.insert("total_triggers".to_string(), serde_json::Value::Number(total_triggers.into()));
        
        // Per-timer analytics
        let timer_stats: HashMap<String, serde_json::Value> = timers.iter()
            .map(|(name, timer)| {
                let stats = serde_json::json!({
                    "enabled": timer.enabled,
                    "interval_seconds": timer.interval_seconds,
                    "trigger_count": timer.trigger_count,
                    "last_triggered": timer.last_triggered,
                    "platforms": timer.platforms,
                    "channels": timer.channels
                });
                (name.clone(), stats)
            })
            .collect();
        
        analytics.insert("timer_details".to_string(), serde_json::Value::Object(
            timer_stats.into_iter().collect()
        ));
        
        analytics
    }

    /// Export current timer configuration
    pub async fn export_config(&self, output_path: &Path) -> Result<()> {
        let config = self.timer_config.read().await.clone();
        let yaml_content = serde_yaml::to_string(&config)
            .context("Failed to serialize timer config")?;
        
        fs::write(output_path, yaml_content).await
            .with_context(|| format!("Failed to write timer config to: {}", output_path.display()))?;
        
        info!("Exported timer configuration to: {}", output_path.display());
        Ok(())
    }

    /// Reload configuration from file
    pub async fn reload_config(&self) -> Result<()> {
        info!("Manually reloading timer configuration...");
        self.load_config().await
    }

    /// Set a custom variable for timer message substitution
    pub async fn set_custom_variable(&self, name: String, value: String) {
        let mut custom_vars = self.custom_variables.write().await;
        custom_vars.insert(name.clone(), value.clone());
        info!("Set custom timer variable '{}' = '{}'", name, value);
    }

    /// Get a custom variable value
    pub async fn get_custom_variable(&self, name: &str) -> Option<String> {
        let custom_vars = self.custom_variables.read().await;
        custom_vars.get(name).cloned()
    }

    /// Remove a custom variable
    pub async fn remove_custom_variable(&self, name: &str) -> bool {
        let mut custom_vars = self.custom_variables.write().await;
        let removed = custom_vars.remove(name).is_some();
        if removed {
            info!("Removed custom timer variable '{}'", name);
        }
        removed
    }

    /// List all custom variables
    pub async fn list_custom_variables(&self) -> Vec<(String, String)> {
        let custom_vars = self.custom_variables.read().await;
        custom_vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Signal the timer system to shutdown gracefully
    pub async fn shutdown(&self) {
        info!("Signaling timer system shutdown...");
        self.shutdown_signal.store(true, Ordering::Relaxed);
        
        // Give the timer system a moment to shut down gracefully
        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("Timer system shutdown signal sent");
    }
}