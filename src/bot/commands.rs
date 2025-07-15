use anyhow::Result;
use log::{debug, error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{BotCommand, ChatMessage};

pub struct CommandSystem {
    pub commands: Arc<RwLock<HashMap<String, BotCommand>>>,
    pub command_cooldowns: Arc<RwLock<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
    pub command_prefix: Arc<RwLock<String>>,
}

impl CommandSystem {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
            command_cooldowns: Arc::new(RwLock::new(HashMap::new())),
            command_prefix: Arc::new(RwLock::new("!".to_string())),
        }
    }

    /// Set the command prefix (default is "!")
    pub async fn set_command_prefix(&self, prefix: String) {
        *self.command_prefix.write().await = prefix.clone();
        info!("Command prefix set to: {}", prefix);
    }

    /// Register a new command
    pub async fn add_command(&self, trigger: String, response: String, mod_only: bool, cooldown_seconds: u64) {
        let command = BotCommand {
            trigger: trigger.clone(),
            response,
            mod_only,
            cooldown_seconds,
        };
        
        self.commands.write().await.insert(trigger.clone(), command);
        info!("Registered command: !{}", trigger);
    }

    /// Process a single message and check for commands
    pub async fn process_message(
        &self,
        message: ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
        analytics_sender: Option<&tokio::sync::mpsc::Sender<(String, String, String)>>, // (command, user, channel)
    ) -> Result<()> {
        // Get current prefix
        let prefix = self.command_prefix.read().await.clone();
        
        // Check if message starts with command prefix
        if !message.content.starts_with(&prefix) {
            return Ok(());
        }

        // Extract command and arguments
        let content_without_prefix = &message.content[prefix.len()..];
        let parts: Vec<&str> = content_without_prefix.split_whitespace().collect();
        
        if parts.is_empty() {
            return Ok(());
        }

        let command_name = parts[0].to_lowercase();
        let args: Vec<&str> = parts[1..].to_vec();
        
        debug!("Processing command '{}' from user '{}' in #{}", 
               command_name, message.username, message.channel);

        // Look up the command
        let commands_guard = self.commands.read().await;
        let command = match commands_guard.get(&command_name) {
            Some(cmd) => cmd.clone(),
            None => {
                debug!("Unknown command: {}", command_name);
                return Ok(());
            }
        };
        drop(commands_guard);

        // Check permissions
        if command.mod_only && !message.is_mod {
            debug!("User '{}' attempted to use mod-only command '{}'", 
                   message.username, command_name);
            return Ok(());
        }

        // Check cooldown
        let cooldown_key = format!("{}:{}:{}", message.platform, message.channel, command_name);
        let mut cooldowns_guard = self.command_cooldowns.write().await;
        
        if let Some(last_used) = cooldowns_guard.get(&cooldown_key) {
            let elapsed = chrono::Utc::now().signed_duration_since(*last_used);
            if elapsed.num_seconds() < command.cooldown_seconds as i64 {
                debug!("Command '{}' is on cooldown for {} more seconds", 
                       command_name, command.cooldown_seconds as i64 - elapsed.num_seconds());
                return Ok(());
            }
        }

        // Update cooldown
        cooldowns_guard.insert(cooldown_key, chrono::Utc::now());
        drop(cooldowns_guard);

        // Execute command
        info!("Executing command '{}' for user '{}' in #{}", 
              command_name, message.username, message.channel);

        // Process response with variable substitution
        let response = Self::process_command_response(&command.response, &message, &args);

        // Send response
        if let Err(e) = response_sender.send((
            message.platform.clone(),
            message.channel.clone(),
            response
        )).await {
            error!("Failed to send command response: {}", e);
        }

        // Record analytics if sender provided
        if let Some(analytics_sender) = analytics_sender {
            if let Err(e) = analytics_sender.send((
                command_name,
                message.username.clone(),
                message.channel.clone()
            )).await {
                error!("Failed to send analytics data: {}", e);
            }
        }

        Ok(())
    }

    /// Process command response with variable substitution and argument support
    fn process_command_response(response: &str, message: &ChatMessage, args: &[&str]) -> String {
        let mut processed = response
            .replace("$(user)", &message.username)
            .replace("$(channel)", &message.channel)
            .replace("$(displayname)", message.display_name.as_deref().unwrap_or(&message.username))
            .replace("$(platform)", &message.platform);

        // Add argument substitution $(1), $(2), etc.
        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("$({})", i + 1);
            processed = processed.replace(&placeholder, arg);
        }

        // Replace $(args) with all arguments joined
        if !args.is_empty() {
            processed = processed.replace("$(args)", &args.join(" "));
        } else {
            processed = processed.replace("$(args)", "");
        }

        processed
    }

    /// Check if a command exists
    pub async fn command_exists(&self, command_name: &str) -> bool {
        self.commands.read().await.contains_key(command_name)
    }

    /// Get all registered commands (for help/listing)
    pub async fn get_all_commands(&self) -> Vec<String> {
        self.commands.read().await.keys().cloned().collect()
    }

    /// Get command info
    pub async fn get_command_info(&self, command_name: &str) -> Option<BotCommand> {
        self.commands.read().await.get(command_name).cloned()
    }

    /// Remove a command
    pub async fn remove_command(&self, command_name: &str) -> bool {
        self.commands.write().await.remove(command_name).is_some()
    }

    /// Check if a command can be executed (cooldown and permissions)
    pub async fn can_execute_command(&self, command: &BotCommand, user: &ChatMessage) -> bool {
        // Check mod-only restriction
        if command.mod_only && !user.is_mod {
            return false;
        }

        // Check cooldown
        let cooldown_key = format!("{}:{}:{}", user.platform, user.channel, command.trigger);
        let cooldowns = self.command_cooldowns.read().await;
        
        if let Some(last_used) = cooldowns.get(&cooldown_key) {
            let elapsed = chrono::Utc::now().signed_duration_since(*last_used);
            if elapsed.num_seconds() < command.cooldown_seconds as i64 {
                return false;
            }
        }

        true
    }
}