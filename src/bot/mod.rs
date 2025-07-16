use anyhow::Result;
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::platforms::PlatformConnection;
use crate::types::{ChatMessage, SpamFilterType, ExemptionLevel, ModerationEscalation, ModerationAction};

pub mod achievements;
pub mod achievement_commands;
pub mod analytics;
pub mod commands;
pub mod enhanced_moderation;
pub mod filter_commands;
pub mod filter_import_export;
pub mod moderation;
pub mod pattern_matching;
pub mod points;
pub mod points_commands;
pub mod realtime_analytics;
pub mod smart_escalation;
pub mod timers;
pub mod timer_commands;

use commands::CommandSystem;
use timers::TimerSystem;
use timer_commands::TimerCommands;
use moderation::ModerationSystem;
use analytics::{AnalyticsSystem, AnalyticsEvent};
use points::PointsSystem;
use points_commands::PointsCommands;
use achievements::AchievementSystem;
use achievement_commands::AchievementCommands;
use filter_commands::FilterCommands;
use enhanced_moderation::EnhancedModerationSystem;

/// Core bot engine that manages connections and all bot systems
pub struct ChatBot {
    connections: Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>,
    command_system: Arc<CommandSystem>,
    timer_system: Arc<TimerSystem>,
    timer_commands: Arc<TimerCommands>,
    moderation_system: Arc<ModerationSystem>,
    analytics_system: Arc<RwLock<AnalyticsSystem>>,
    points_system: Arc<PointsSystem>,
    points_commands: Arc<PointsCommands>,
    achievement_system: Arc<AchievementSystem>,
    achievement_commands: Arc<AchievementCommands>,
    filter_commands: Arc<FilterCommands>,
}

impl ChatBot {
    pub fn new() -> Self {
        let points_system = Arc::new(PointsSystem::new());
        let points_commands = Arc::new(PointsCommands::new(Arc::clone(&points_system)));
        let achievement_system = Arc::new(AchievementSystem::new());
        let achievement_commands = Arc::new(AchievementCommands::new(Arc::clone(&achievement_system)));
        let moderation_system = Arc::new(ModerationSystem::new());
        let filter_commands = Arc::new(FilterCommands::new(Arc::clone(&moderation_system)));
        let timer_system = Arc::new(TimerSystem::new());
        let timer_commands = Arc::new(TimerCommands::new(Arc::clone(&timer_system)));
        
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            command_system: Arc::new(CommandSystem::new()),
            timer_system,
            timer_commands,
            moderation_system,
            analytics_system: Arc::new(RwLock::new(AnalyticsSystem::new())),
            points_system,
            points_commands,
            achievement_system,
            achievement_commands,
            filter_commands,
        }
    }

    pub fn create_enhanced_moderation(&self) -> EnhancedModerationSystem {
        EnhancedModerationSystem::new(self.moderation_system.clone())
    }

    /// Set the command prefix (default is "!")
    pub async fn set_command_prefix(&self, prefix: String) {
        self.command_system.set_command_prefix(prefix).await;
    }

    /// Add a platform connection to the bot
    pub async fn add_connection(&mut self, connection: Box<dyn PlatformConnection>) {
        let platform_name = connection.platform_name().to_string();
        info!("Added {} connection", platform_name);
        self.connections.write().await.insert(platform_name, connection);
    }

    /// Register a new command
    pub async fn add_command(&self, trigger: String, response: String, mod_only: bool, cooldown_seconds: u64) {
        self.command_system.add_command(trigger, response, mod_only, cooldown_seconds).await;
    }

    // =================================================================
    // TIMER SYSTEM API - Updated to work with external YAML config
    // =================================================================

    /// Add a new timer that posts messages at regular intervals
    pub async fn add_timer(&self, name: String, message: String, interval_seconds: u64) -> Result<()> {
        self.timer_system.add_timer(name, message, interval_seconds).await
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
        self.timer_system.add_timer_advanced(name, message, interval_seconds, channels, platforms).await
    }

    /// Enable or disable a specific timer
    pub async fn set_timer_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        self.timer_system.set_timer_enabled(name, enabled).await
    }

    /// Get statistics for all timers
    pub async fn get_timer_stats(&self) -> HashMap<String, (bool, u64, Option<chrono::DateTime<chrono::Utc>>)> {
        self.timer_system.get_timer_stats().await
    }

    /// Remove a timer
    pub async fn remove_timer(&self, name: &str) -> Result<()> {
        self.timer_system.remove_timer(name).await
    }

    /// Set a custom variable for timer message substitution
    pub async fn set_timer_variable(&self, name: String, value: String) -> Result<()> {
        self.timer_system.set_custom_variable(name, value).await;
        Ok(())
    }

    /// Get a custom timer variable value
    pub async fn get_timer_variable(&self, name: &str) -> Option<String> {
        self.timer_system.get_custom_variable(name).await
    }

    /// Remove a custom timer variable
    pub async fn remove_timer_variable(&self, name: &str) -> Result<bool> {
        Ok(self.timer_system.remove_custom_variable(name).await)
    }

    /// List all custom timer variables
    pub async fn list_timer_variables(&self) -> Result<Vec<(String, String)>> {
        Ok(self.timer_system.list_custom_variables().await)
    }

    /// Reload timer configuration from file
    pub async fn reload_timer_config(&self) -> Result<()> {
        self.timer_system.reload_config().await
    }

    /// Get timer configuration categories
    pub async fn get_timer_categories(&self) -> HashMap<String, Vec<String>> {
        self.timer_system.get_timer_categories().await
    }

    /// Enable/disable timers by category
    pub async fn set_timer_category_enabled(&self, category: &str, enabled: bool) -> Result<usize> {
        self.timer_system.set_category_enabled(category, enabled).await
    }

    /// Get timer analytics (if enabled)
    pub async fn get_timer_analytics(&self) -> HashMap<String, serde_json::Value> {
        self.timer_system.get_timer_analytics().await
    }

    // =================================================================
    // SPAM FILTERING AND MODERATION SYSTEM
    // =================================================================

    /// Add a spam filter to the bot (legacy method for backward compatibility)
    pub async fn add_spam_filter(&self, filter_type: SpamFilterType) -> Result<()> {
        self.moderation_system.add_spam_filter(filter_type).await
    }

    /// Add a spam filter with custom configuration (legacy method)
    pub async fn add_spam_filter_advanced(
        &self,
        filter_type: SpamFilterType,
        timeout_duration: u64,
        warning_message: Option<String>,
        mod_exempt: bool,
        subscriber_exempt: bool,
    ) -> Result<()> {
        // Convert old parameters to new escalation system
        let escalation = ModerationEscalation {
            first_offense: ModerationAction::WarnUser { 
                message: warning_message.clone().unwrap_or_else(|| "Please follow chat rules (first warning)".to_string())
            },
            repeat_offense: ModerationAction::TimeoutUser { duration_seconds: timeout_duration },
            offense_window_seconds: 3600,
        };

        let exemption_level = if mod_exempt && subscriber_exempt {
            ExemptionLevel::Subscriber
        } else if mod_exempt {
            ExemptionLevel::Moderator
        } else {
            ExemptionLevel::None
        };

        let filter_name = format!("{}_{}", 
            Self::generate_filter_name(&filter_type), 
            chrono::Utc::now().timestamp()
        );

        self.moderation_system.add_spam_filter_advanced(
            filter_name,
            filter_type,
            escalation,
            exemption_level,
            false, // silent_mode
            warning_message,
        ).await
    }

    /// Add a spam filter with enhanced configuration (NEW)
    pub async fn add_spam_filter_enhanced(
        &self,
        name: String,
        filter_type: SpamFilterType,
        timeout_seconds: u64,
        exemption_level: ExemptionLevel,
        custom_message: Option<String>,
        silent_mode: bool,
    ) -> Result<()> {
        let escalation = ModerationEscalation {
            first_offense: ModerationAction::WarnUser { 
                message: custom_message.clone().unwrap_or_else(|| "Please follow chat rules (first warning)".to_string())
            },
            repeat_offense: ModerationAction::TimeoutUser { duration_seconds: timeout_seconds },
            offense_window_seconds: 3600,
        };

        self.moderation_system.add_spam_filter_advanced(
            name,
            filter_type,
            escalation,
            exemption_level,
            silent_mode,
            custom_message,
        ).await
    }

    /// Add blacklist filter (NightBot parity)
    pub async fn add_blacklist_filter(
        &self,
        patterns: Vec<String>,
        timeout_seconds: Option<u64>,
        exemption_level: Option<ExemptionLevel>,
        case_sensitive: Option<bool>,
        whole_words_only: Option<bool>,
        custom_message: Option<String>,
    ) -> Result<()> {
        let filter_name = format!("blacklist_{}", chrono::Utc::now().timestamp());
        
        self.moderation_system.add_blacklist_filter(
            filter_name,
            patterns,
            case_sensitive.unwrap_or(false),
            whole_words_only.unwrap_or(false),
            exemption_level.unwrap_or(ExemptionLevel::Moderator),
            timeout_seconds.unwrap_or(600),
            custom_message,
        ).await
    }

    /// Enable or disable all spam filters
    pub async fn set_spam_protection_enabled(&self, enabled: bool) {
        self.moderation_system.set_spam_protection_enabled(enabled).await;
    }

    /// Enable/disable specific filter
    pub async fn set_filter_enabled(&self, filter_name: &str, enabled: bool) -> Result<()> {
        self.moderation_system.set_filter_enabled(filter_name, enabled).await
    }

    /// Remove filter
    pub async fn remove_filter(&self, filter_name: &str) -> Result<()> {
        self.moderation_system.remove_filter(filter_name).await
    }

    /// List all filters
    pub async fn list_filters(&self) -> Vec<(String, bool)> {
        self.moderation_system.list_filters().await
    }

    /// Get filter statistics
    pub async fn get_filter_stats(&self) -> HashMap<String, serde_json::Value> {
        self.moderation_system.get_filter_stats().await
    }

    /// Clear message history for all users (useful for cleanup)
    pub async fn clear_message_history(&self) {
        self.moderation_system.clear_message_history().await;
    }

    // =================================================================
    // WEB DASHBOARD
    // =================================================================

    /// Start the web dashboard on the specified port
    #[cfg(feature = "web")]
    pub async fn start_web_dashboard(&self, port: u16) -> Result<()> {
        info!("Starting web dashboard on port {}...", port);
        
        // Import web modules locally to avoid module resolution issues
        use crate::web::{WebDashboard};
        
        // Create dashboard
        let dashboard = WebDashboard::new();
        let dashboard_state = dashboard.get_state();
        
        info!("Setting up dashboard data updates...");
        
        // Start periodic data updates for the dashboard
        let analytics_system = Arc::clone(&self.analytics_system);
        let connections = Arc::clone(&self.connections);
        let state_for_updates = dashboard_state.clone();
        
        tokio::spawn(async move {
            info!("Dashboard data updater started");
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                
                // Update analytics data
                let analytics = analytics_system.read().await.get_analytics().await;
                state_for_updates.update_analytics(analytics).await;
                
                // Update health data
                let mut health = HashMap::new();
                {
                    let connections_guard = connections.read().await;
                    for (platform_name, connection) in connections_guard.iter() {
                        let is_healthy = connection.is_connected().await;
                        health.insert(platform_name.clone(), is_healthy);
                    }
                }
                state_for_updates.update_health(health).await;
            }
        });
        
        info!("Starting web server...");
        
        // Start the web server in a separate task
        tokio::spawn(async move {
            if let Err(e) = dashboard.start_server(port).await {
                error!("Web dashboard error: {}", e);
            }
        });
        
        info!("Web dashboard started on port {}", port);
        Ok(())
    }

    /// Start the web dashboard (no-op when web feature is disabled)
    #[cfg(not(feature = "web"))]
    pub async fn start_web_dashboard(&self, _port: u16) -> Result<()> {
        warn!("Web dashboard is disabled. Enable with --features web");
        Ok(())
    }

    // =================================================================
    // ACHIEVEMENTS AND POINTS SYSTEM
    // =================================================================

    /// Get achievement statistics
    pub async fn get_achievement_stats(&self) -> HashMap<String, serde_json::Value> {
        self.achievement_system.get_statistics().await
    }

    /// Get user achievements
    pub async fn get_user_achievements(&self, user_id: &str) -> Option<achievements::UserAchievements> {
        self.achievement_system.get_user_achievements(user_id).await
    }

    /// Get achievement leaderboard
    pub async fn get_achievement_leaderboard(&self, limit: usize) -> Vec<(String, i64, usize)> {
        self.achievement_system.get_achievement_leaderboard(limit).await
    }

    /// Get user points
    pub async fn get_user_points(&self, platform: &str, username: &str) -> Option<points::UserPoints> {
        self.points_system.get_user_points(platform, username).await
    }

    /// Add points to user (admin function)
    pub async fn add_user_points(&self, platform: &str, username: &str, amount: i64, reason: &str) -> Result<bool> {
        self.points_system.add_points(platform, username, amount, reason).await
    }

    /// Get points leaderboard
    pub async fn get_points_leaderboard(&self, limit: usize) -> Vec<points::UserPoints> {
        self.points_system.get_leaderboard(limit).await
    }

    /// Get points system statistics
    pub async fn get_points_stats(&self) -> HashMap<String, serde_json::Value> {
        self.points_system.get_statistics().await
    }

    /// Get analytics data
    pub async fn get_analytics(&self) -> HashMap<String, serde_json::Value> {
        self.analytics_system.read().await.get_analytics().await
    }

    // =================================================================
    // BOT LIFECYCLE
    // =================================================================

    /// Start the bot and all connections
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting chat bot...");

        // Start analytics processor first
        {
            let mut analytics_guard = self.analytics_system.write().await;
            analytics_guard.start_analytics_processor().await;
        }

        // Start points system
        self.points_system.start().await?;

        // Initialize achievement system
        self.achievement_system.initialize_default_achievements().await;

        // Collect message receivers
        let mut receivers = Vec::new();
        
        // Connect to all platforms
        {
            let mut connections_guard = self.connections.write().await;
            for (platform_name, connection) in connections_guard.iter_mut() {
                if let Err(e) = connection.connect().await {
                    error!("Failed to connect to {}: {}", platform_name, e);
                    continue;
                }
                
                // Get message receiver after successful connection
                if let Some(receiver) = connection.get_message_receiver() {
                    receivers.push(receiver);
                    info!("Set up message receiver for {}", platform_name);
                }
            }
        }

        // Start message processing with the collected receivers
        self.start_message_processor(receivers).await?;

        // Start the timer system with external YAML configuration
        let timer_system_clone = Arc::clone(&self.timer_system);
        let connections_clone = Arc::clone(&self.connections);
        
        tokio::spawn(async move {
            // We need to get a mutable reference to start the timer system
            // Since we're using Arc, we need to handle this carefully
            match timer_system_clone.start_timer_system(connections_clone).await {
                Ok(_) => {
                    info!("Timer system started successfully");
                }
                Err(e) => {
                    error!("Failed to start timer system: {}", e);
                }
            }
        });

        info!("Chat bot started successfully");
        
        // Return immediately - don't block here
        Ok(())
    }

    /// Process incoming messages with enhanced moderation
    async fn start_message_processor(&self, receivers: Vec<broadcast::Receiver<ChatMessage>>) -> Result<()> {
        let command_system = Arc::clone(&self.command_system);
        let moderation_system = Arc::clone(&self.moderation_system);
        let analytics_system = Arc::clone(&self.analytics_system);
        let connections = Arc::clone(&self.connections);
        
        // Create response channel for sending bot responses
        let (response_tx, mut response_rx) = tokio::sync::mpsc::channel::<(String, String, String)>(100);
        
        // Get analytics sender
        let analytics_sender = {
            let analytics_guard = analytics_system.read().await;
            analytics_guard.get_sender()
        };

        // Response handler that sends messages back to platforms
        {
            let connections = Arc::clone(&connections);
            tokio::spawn(async move {
                while let Some((platform, channel, message)) = response_rx.recv().await {
                    let connections_guard = connections.read().await;
                    if let Some(connection) = connections_guard.get(&platform) {
                        if let Err(e) = connection.send_message(&channel, &message).await {
                            error!("Failed to send response to {}#{}: {}", platform, channel, e);
                        } else {
                            info!("Sent response to {}#{}: {}", platform, channel, message);
                        }
                    } else {
                        warn!("No connection found for platform: {}", platform);
                    }
                }
            });
        }

        // Create analytics command channel
        let (analytics_command_tx, mut analytics_command_rx) = tokio::sync::mpsc::channel::<(String, String, String)>(100);
        
        // Analytics command processor
        {
            let analytics_sender = Arc::clone(&analytics_sender);
            tokio::spawn(async move {
                while let Some((command, user, channel)) = analytics_command_rx.recv().await {
                    if let Err(e) = analytics_sender.send(AnalyticsEvent::CommandExecuted {
                        command,
                        user,
                        channel,
                    }).await {
                        error!("Failed to send analytics command event: {}", e);
                    }
                }
            });
        }

        // Process messages from all platform receivers
        for mut receiver in receivers {
            let response_tx = response_tx.clone();
            let analytics_command_tx = analytics_command_tx.clone();
            let command_system = Arc::clone(&command_system);
            let moderation_system = Arc::clone(&moderation_system);
            let analytics_sender = Arc::clone(&analytics_sender);
            let points_system = Arc::clone(&self.points_system);
            let points_commands = Arc::clone(&self.points_commands);
            let achievement_system = Arc::clone(&self.achievement_system);
            let achievement_commands = Arc::clone(&self.achievement_commands);
            let filter_commands = Arc::clone(&self.filter_commands);
            let timer_commands = Arc::clone(&self.timer_commands); // Add timer commands
            
            tokio::spawn(async move {
                loop {
                    match receiver.recv().await {
                        Ok(message) => {
                            info!("Processing message from {}: {}", message.username, message.content);
                            
                            // Record message in analytics
                            if let Err(e) = analytics_sender.send(AnalyticsEvent::MessageReceived(message.clone())).await {
                                error!("Failed to send analytics message event: {}", e);
                            }
                            
                            // Process message for points (always, even if spam)
                            if let Err(e) = points_system.process_message(&message).await {
                                error!("Failed to process points for message: {}", e);
                            }
                            
                            // Check for achievement unlocks after processing points
                            if let Some(user_points) = points_system.get_user_points(&message.platform, &message.username).await {
                                let unlocked_achievements = achievement_system.check_achievements(&user_points).await;
                                
                                for achievement in unlocked_achievements {
                                    // Award achievement bonus points
                                    if let Err(e) = points_system.add_points(&message.platform, &message.username, 
                                                                           achievement.reward_points, &format!("Achievement: {}", achievement.name)).await {
                                        error!("Failed to award achievement points: {}", e);
                                    }
                                    
                                    // Announce the achievement
                                    if let Err(e) = achievement_commands.announce_achievement(&achievement, &message.username, &message, &response_tx).await {
                                        error!("Failed to announce achievement: {}", e);
                                    }
                                }
                            }
                            
                            // Update user message history for moderation
                            moderation_system.update_user_history(&message).await;
                            
                            // Check spam filters first (ENHANCED with user points context)
                            let user_points = points_system.get_user_points(&message.platform, &message.username).await;
                            if let Some(action) = moderation_system.check_spam_filters(&message, user_points.as_ref()).await {
                                warn!("Message flagged by spam filter: {} from {}", message.content, message.username);
                                
                                // Record spam in analytics
                                if let Err(e) = analytics_sender.send(AnalyticsEvent::SpamBlocked(message.clone())).await {
                                    error!("Failed to send analytics spam event: {}", e);
                                }
                                
                                // Handle moderation action
                                if let Err(e) = moderation::ModerationSystem::handle_moderation_action(
                                    action, &message, &response_tx
                                ).await {
                                    error!("Failed to handle moderation action: {}", e);
                                }
                                continue; // Don't process commands for flagged messages
                            }
                            
                            // Check for commands
                            let prefix = command_system.command_prefix.read().await.clone();
                            if message.content.starts_with(&prefix) {
                                let content_without_prefix = &message.content[prefix.len()..];
                                let parts: Vec<&str> = content_without_prefix.split_whitespace().collect();
                                
                                if !parts.is_empty() {
                                    let command_name = parts[0].to_lowercase();
                                    let args: Vec<&str> = parts[1..].to_vec();
                                    
                                    // Try timer commands first (NEW)
                                    match timer_commands.process_command(&command_name, &args, &message, &response_tx).await {
                                        Ok(true) => {
                                            // Timer command was handled
                                            continue;
                                        }
                                        Ok(false) => {
                                            // Not a timer command, try filter commands
                                        }
                                        Err(e) => {
                                            error!("Error processing timer command: {}", e);
                                        }
                                    }
                                    
                                    // Try filter commands
                                    match filter_commands.process_command(&command_name, &args, &message, &response_tx).await {
                                        Ok(true) => {
                                            // Filter command was handled
                                            continue;
                                        }
                                        Ok(false) => {
                                            // Not a filter command, try achievement commands
                                        }
                                        Err(e) => {
                                            error!("Error processing filter command: {}", e);
                                        }
                                    }
                                    
                                    // Try achievement commands
                                    match achievement_commands.process_command(&command_name, &args, &message, &response_tx).await {
                                        Ok(true) => {
                                            // Achievement command was handled
                                            continue;
                                        }
                                        Ok(false) => {
                                            // Not an achievement command, try points commands
                                        }
                                        Err(e) => {
                                            error!("Error processing achievement command: {}", e);
                                        }
                                    }
                                    
                                    // Try points commands
                                    match points_commands.process_command(&command_name, &args, &message, &response_tx).await {
                                        Ok(true) => {
                                            // Points command was handled
                                            if let Err(e) = points_system.process_command(&message, &command_name).await {
                                                error!("Failed to process command points: {}", e);
                                            }
                                            continue;
                                        }
                                        Ok(false) => {
                                            // Not a points command, continue to regular commands
                                        }
                                        Err(e) => {
                                            error!("Error processing points command: {}", e);
                                        }
                                    }
                                }
                            }
                            
                            // Process regular commands
                            if let Err(e) = command_system.process_message(
                                message.clone(), 
                                &response_tx,
                                Some(&analytics_command_tx)
                            ).await {
                                error!("Failed to process command: {}", e);
                            } else {
                                // Award points for command usage
                                if message.content.starts_with(&prefix) {
                                    let content_without_prefix = &message.content[prefix.len()..];
                                    let parts: Vec<&str> = content_without_prefix.split_whitespace().collect();
                                    
                                    if !parts.is_empty() {
                                        let command_name = parts[0].to_lowercase();
                                        if let Err(e) = points_system.process_command(&message, &command_name).await {
                                            error!("Failed to process command points: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Message receiver lagged by {} messages", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Message receiver closed");
                            break;
                        }
                    }
                }
            });
        }
        
        Ok(())
    }

    // =================================================================
    // UTILITY METHODS
    // =================================================================

    /// Health check for all connections
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let mut status = HashMap::new();
        let connections_guard = self.connections.read().await;
        
        for (platform_name, connection) in connections_guard.iter() {
            let is_healthy = connection.is_connected().await;
            status.insert(platform_name.clone(), is_healthy);
            
            if !is_healthy {
                warn!("{} connection is unhealthy", platform_name);
            }
        }
        
        status
    }

    /// Get detailed bot statistics
    pub async fn get_bot_stats(&self) -> Result<serde_json::Value> {
        let mut stats = serde_json::Map::new();
        
        // Get analytics
        let analytics = self.get_analytics().await;
        stats.insert("analytics".to_string(), serde_json::Value::Object(
            analytics.into_iter().collect()
        ));
        
        // Get timer stats
        let timer_stats = self.get_timer_stats().await;
        let timer_json: serde_json::Value = serde_json::to_value(timer_stats)
            .unwrap_or_else(|_| serde_json::Value::Null);
        stats.insert("timers".to_string(), timer_json);
        
        // Get timer analytics
        let timer_analytics = self.get_timer_analytics().await;
        stats.insert("timer_analytics".to_string(), serde_json::Value::Object(
            timer_analytics.into_iter().collect()
        ));
        
        // Get connection health
        let health = self.health_check().await;
        stats.insert("connections".to_string(), serde_json::to_value(health)?);
        
        // Get command count
        let commands = self.command_system.get_all_commands().await;
        stats.insert("total_commands".to_string(), serde_json::Value::Number(commands.len().into()));
        
        // Get filter stats
        let filter_stats = self.get_filter_stats().await;
        stats.insert("filters".to_string(), serde_json::Value::Object(
            filter_stats.into_iter().collect()
        ));
        
        Ok(serde_json::Value::Object(stats))
    }

    /// Get user information
    pub async fn get_user_info(&self, platform: &str, username: &str) -> Option<serde_json::Value> {
        let analytics_guard = self.analytics_system.read().await;
        if let Some(user_stats) = analytics_guard.get_user_stats(platform, username).await {
            serde_json::to_value(user_stats).ok()
        } else {
            None
        }
    }

    /// Add a command with argument support
    pub async fn add_command_with_args(&self, trigger: String, response: String, mod_only: bool, cooldown_seconds: u64, help_text: Option<String>) {
        // For now, we'll store help text in the response with a special marker
        let enhanced_response = if let Some(help) = help_text {
            format!("{}|HELP:{}", response, help)
        } else {
            response
        };
        
        self.add_command(trigger, enhanced_response, mod_only, cooldown_seconds).await;
    }

    /// Remove a command
    pub async fn remove_command(&self, command_name: &str) -> bool {
        self.command_system.remove_command(command_name).await
    }

    /// Check if a command exists
    pub async fn command_exists(&self, command_name: &str) -> bool {
        self.command_system.command_exists(command_name).await
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

    /// Gracefully shutdown all connections
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down chat bot...");
        
        // Stop timer system gracefully
        self.timer_system.shutdown().await;
        
        // Disconnect all platforms
        let mut connections_guard = self.connections.write().await;
        for (platform_name, connection) in connections_guard.iter_mut() {
            if let Err(e) = connection.disconnect().await {
                error!("Error disconnecting from {}: {}", platform_name, e);
            }
        }
        
        info!("Chat bot shutdown complete");
        Ok(())
    }
}