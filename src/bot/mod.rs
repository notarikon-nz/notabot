use anyhow::Result;
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
// use tokio::time::{sleep, Duration};

use crate::platforms::PlatformConnection;
use crate::types::{ChatMessage, SpamFilterType};

pub mod commands;
pub mod timers;
pub mod moderation;

use commands::CommandSystem;
use timers::TimerSystem;
use moderation::ModerationSystem;

/// Core bot engine that manages connections and all bot systems
pub struct ChatBot {
    connections: Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>,
    command_system: CommandSystem,
    timer_system: TimerSystem,
    moderation_system: ModerationSystem,
}

impl ChatBot {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            command_system: CommandSystem::new(),
            timer_system: TimerSystem::new(),
            moderation_system: ModerationSystem::new(),
        }
    }

    /// Set the command prefix (default is "!")
    pub fn set_command_prefix(&mut self, prefix: String) {
        self.command_system.set_command_prefix(prefix);
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

    /// Remove a timer
    pub async fn remove_timer(&self, name: &str) -> Result<()> {
        self.timer_system.remove_timer(name).await
    }

    /// Get statistics for all timers
    pub async fn get_timer_stats(&self) -> HashMap<String, (bool, u64, Option<chrono::DateTime<chrono::Utc>>)> {
        self.timer_system.get_timer_stats().await
    }

    /// Add a spam filter to the bot
    pub async fn add_spam_filter(&self, filter_type: SpamFilterType) -> Result<()> {
        self.moderation_system.add_spam_filter(filter_type).await
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
        self.moderation_system.add_spam_filter_advanced(
            filter_type, timeout_duration, warning_message, mod_exempt, subscriber_exempt
        ).await
    }

    /// Enable or disable all spam filters
    pub async fn set_spam_protection_enabled(&self, enabled: bool) {
        self.moderation_system.set_spam_protection_enabled(enabled).await;
    }

    /// Clear message history for all users (useful for cleanup)
    pub async fn clear_message_history(&self) {
        self.moderation_system.clear_message_history().await;
    }

    /// Start the bot and all connections
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting chat bot...");

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

        // Start the timer system
        self.timer_system.start_timer_system(Arc::clone(&self.connections)).await?;

        info!("Chat bot started successfully");
        Ok(())
    }

    /// Process incoming messages and handle commands/moderation
    async fn start_message_processor(&mut self, receivers: Vec<broadcast::Receiver<ChatMessage>>) -> Result<()> {
        let command_system = &self.command_system;
        let moderation_system = &self.moderation_system;
        let connections = Arc::clone(&self.connections);
        
        // Create a response channel
        let (response_tx, mut response_rx) = tokio::sync::mpsc::channel::<(String, String, String)>(100);
        
        // Response handler that can access connections through Arc
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

        // Process messages from all receivers
        for mut receiver in receivers {
            let response_tx = response_tx.clone();
            
            // We need to share the systems across async tasks
            // This requires some careful handling due to Rust's ownership rules
            // For now, we'll use a simpler approach and refactor later if needed
            
            tokio::spawn(async move {
                loop {
                    match receiver.recv().await {
                        Ok(message) => {
                            // For now, we'll handle this in a simplified way
                            // In the next iteration, we'll improve the architecture
                            info!("Received message from {}: {}", message.username, message.content);
                            
                            // TODO: Integrate with command and moderation systems
                            // This will require some architectural changes to share systems safely
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

    /// Gracefully shutdown all connections
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down chat bot...");
        
        // Stop timer system
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