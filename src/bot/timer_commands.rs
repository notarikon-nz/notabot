use anyhow::Result;
use log::{warn, info};
use std::sync::Arc;

use crate::types::ChatMessage;
use super::timers::TimerSystem;

pub struct TimerCommands {
    timer_system: Arc<TimerSystem>,
}

impl TimerCommands {
    pub fn new(timer_system: Arc<TimerSystem>) -> Self {
        Self { timer_system }
    }

    /// Process timer-related commands
    pub async fn process_command(
        &self,
        command: &str,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool> {
        // Only moderators can manage timers
        if !message.is_mod {
            return Ok(false);
        }

        match command {
            "timers" => {
                self.handle_timers_command(args, message, response_sender).await?;
                Ok(true)
            }
            "reloadtimers" => {
                self.handle_reload_timers_command(message, response_sender).await?;
                Ok(true)
            }
            "timerstats" => {
                self.handle_timer_stats_command(message, response_sender).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Handle !timers command with subcommands
    async fn handle_timers_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.is_empty() {
            let response = "Timer Commands: !timers <list|enable|disable|reload|categories> [name/category] | !timerstats | !reloadtimers".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        match args[0].to_lowercase().as_str() {
            "list" => {
                self.handle_list_timers_command(message, response_sender).await?;
            }
            "enable" => {
                if args.len() < 2 {
                    let response = "Usage: !timers enable <timer_name>".to_string();
                    self.send_response(response, message, response_sender).await?;
                    return Ok(());
                }
                
                let timer_name = args[1];
                match self.timer_system.set_timer_enabled(timer_name, true).await {
                    Ok(_) => {
                        let response = format!("Timer '{}' enabled", timer_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                    Err(_) => {
                        let response = format!("Timer '{}' not found", timer_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "disable" => {
                if args.len() < 2 {
                    let response = "Usage: !timers disable <timer_name>".to_string();
                    self.send_response(response, message, response_sender).await?;
                    return Ok(());
                }
                
                let timer_name = args[1];
                match self.timer_system.set_timer_enabled(timer_name, false).await {
                    Ok(_) => {
                        let response = format!("Timer '{}' disabled", timer_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                    Err(_) => {
                        let response = format!("Timer '{}' not found", timer_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "reload" => {
                match self.timer_system.reload_config().await {
                    Ok(_) => {
                        let response = "Timer configuration reloaded from timers.yaml".to_string();
                        self.send_response(response, message, response_sender).await?;
                    }
                    Err(e) => {
                        let response = format!("Failed to reload timers: {}", e);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "categories" => {
                self.handle_categories_command(args, message, response_sender).await?;
            }
            _ => {
                let response = "Unknown subcommand. Use: list, enable, disable, reload, categories".to_string();
                self.send_response(response, message, response_sender).await?;
            }
        }

        Ok(())
    }

    /// Handle !timers list command
    async fn handle_list_timers_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let stats = self.timer_system.get_timer_stats().await;
        
        if stats.is_empty() {
            let response = "No timers configured. Check timers.yaml file.".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let mut response = format!("Timers ({}): ", stats.len());
        let mut timer_list = Vec::new();
        
        for (name, (enabled, trigger_count, _)) in stats.iter() {
            let status = if *enabled { "+" } else { "-" };
            timer_list.push(format!("{} {} ({})", status, name, trigger_count));
        }
        
        // Split into multiple messages if too long
        let timer_text = timer_list.join(" | ");
        if timer_text.len() + response.len() > 450 { // Leave room for prefix
            response.push_str(&format!("{} timers total. Use !timerstats for details.", stats.len()));
        } else {
            response.push_str(&timer_text);
        }

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    /// Handle timer categories
    async fn handle_categories_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let categories = self.timer_system.get_timer_categories().await;
        
        if args.len() == 1 {
            // List all categories
            let category_names: Vec<String> = categories.keys().cloned().collect();
            let response = format!("Timer Categories: {} | Use !timers categories <name> enable/disable", 
                                 category_names.join(", "));
            self.send_response(response, message, response_sender).await?;
        } else if args.len() >= 3 {
            // Enable/disable category
            let category = args[1];
            let action = args[2].to_lowercase();
            
            match action.as_str() {
                "enable" | "on" => {
                    match self.timer_system.set_category_enabled(category, true).await {
                        Ok(count) => {
                            let response = format!("Enabled {} timers in category '{}'", count, category);
                            self.send_response(response, message, response_sender).await?;
                        }
                        Err(e) => {
                            let response = format!("{}", e);
                            self.send_response(response, message, response_sender).await?;
                        }
                    }
                }
                "disable" | "off" => {
                    match self.timer_system.set_category_enabled(category, false).await {
                        Ok(count) => {
                            let response = format!("Disabled {} timers in category '{}'", count, category);
                            self.send_response(response, message, response_sender).await?;
                        }
                        Err(e) => {
                            let response = format!("{}", e);
                            self.send_response(response, message, response_sender).await?;
                        }
                    }
                }
                _ => {
                    let response = "Usage: !timers categories <category> <enable|disable>".to_string();
                    self.send_response(response, message, response_sender).await?;
                }
            }
        } else {
            let response = "Usage: !timers categories [category enable/disable]".to_string();
            self.send_response(response, message, response_sender).await?;
        }
        
        Ok(())
    }

    /// Handle !reloadtimers command
    async fn handle_reload_timers_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        info!("Manual timer reload requested by {}", message.username);
        
        match self.timer_system.reload_config().await {
            Ok(_) => {
                let stats = self.timer_system.get_timer_stats().await;
                let enabled_count = stats.values().filter(|(enabled, _, _)| *enabled).count();
                let response = format!("Timers reloaded from timers.yaml! {} enabled, {} total", 
                                     enabled_count, stats.len());
                self.send_response(response, message, response_sender).await?;
                info!("Timer configuration successfully reloaded by {}", message.username);
            }
            Err(e) => {
                let response = format!("Failed to reload timer config: {}", e);
                self.send_response(response, message, response_sender).await?;
            }
        }
        
        Ok(())
    }

    /// Handle !timerstats command
    async fn handle_timer_stats_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let stats = self.timer_system.get_timer_stats().await;
        let analytics = self.timer_system.get_timer_analytics().await;
        
        let total_timers = stats.len();
        let enabled_timers = stats.values().filter(|(enabled, _, _)| *enabled).count();
        let total_triggers = stats.values().map(|(_, triggers, _)| triggers).sum::<u64>();
        
        // Find most active timer
        let most_active = stats.iter()
            .max_by_key(|(_, (_, triggers, _))| triggers)
            .map(|(name, (_, triggers, _))| format!("{} ({})", name, triggers))
            .unwrap_or_else(|| "none".to_string());
        
        let response = format!(
            "TIMER STATS: {} total | {} enabled | {} triggers total | Most active: {} | Config: timers.yaml",
            total_timers, enabled_timers, total_triggers, most_active
        );

        self.send_response(response, message, response_sender).await?;
        
        // Send detailed analytics if available
        if let Some(timer_details) = analytics.get("timer_details") {
            if let Some(details_obj) = timer_details.as_object() {
                let mut detail_parts = Vec::new();
                
                for (name, data) in details_obj.iter().take(3) { // Show top 3
                    if let Some(trigger_count) = data.get("trigger_count").and_then(|v| v.as_u64()) {
                        if let Some(enabled) = data.get("enabled").and_then(|v| v.as_bool()) {
                            let status = if enabled { "+" } else { "-" };
                            detail_parts.push(format!("{} {} ({})", status, name, trigger_count));
                        }
                    }
                }
                
                if !detail_parts.is_empty() {
                    let detail_response = format!("Top Timers: {}", detail_parts.join(" | "));
                    self.send_response(detail_response, message, response_sender).await?;
                }
            }
        }

        Ok(())
    }

    /// Send response message
    async fn send_response(
        &self,
        response: String,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if let Err(e) = response_sender.send((
            message.platform.clone(),
            message.channel.clone(),
            response
        )).await {
            warn!("Failed to send timer command response: {}", e);
        }
        Ok(())
    }
}

// Helper functions for timer management
impl TimerCommands {
    /// Get timer configuration summary
    pub async fn get_timer_summary(&self) -> String {
        let stats = self.timer_system.get_timer_stats().await;
        let analytics = self.timer_system.get_timer_analytics().await;
        
        let total = stats.len();
        let enabled = stats.values().filter(|(enabled, _, _)| *enabled).count();
        let triggers = stats.values().map(|(_, triggers, _)| triggers).sum::<u64>();
        
        format!("Timers: {}/{} enabled, {} triggers total", enabled, total, triggers)
    }

    /// Check if timer system is healthy
    pub async fn check_timer_health(&self) -> bool {
        let stats = self.timer_system.get_timer_stats().await;
        
        // Consider healthy if we have at least one enabled timer
        stats.values().any(|(enabled, _, _)| *enabled)
    }

    /// Get timer categories for management
    pub async fn get_available_categories(&self) -> Vec<String> {
        let categories = self.timer_system.get_timer_categories().await;
        categories.keys().cloned().collect()
    }

    /// Quick enable/disable timer
    pub async fn toggle_timer(
        &self,
        timer_name: &str,
        enabled: bool,
        requester: &str,
    ) -> Result<String> {
        match self.timer_system.set_timer_enabled(timer_name, enabled).await {
            Ok(_) => {
                let action = if enabled { "enabled" } else { "disabled" };
                info!("Timer '{}' {} by {}", timer_name, action, requester);
                Ok(format!("Timer '{}' {}", timer_name, action))
            }
            Err(e) => {
                Err(anyhow::anyhow!("Failed to toggle timer '{}': {}", timer_name, e))
            }
        }
    }

    /// Emergency disable all timers
    pub async fn emergency_disable_all_timers(&self, requester: &str) -> Result<usize> {
        let stats = self.timer_system.get_timer_stats().await;
        let mut disabled_count = 0;
        
        for (timer_name, (enabled, _, _)) in stats {
            if enabled {
                if self.timer_system.set_timer_enabled(&timer_name, false).await.is_ok() {
                    disabled_count += 1;
                }
            }
        }
        
        warn!("Emergency: {} timers disabled by {}", disabled_count, requester);
        Ok(disabled_count)
    }

    /// Get next timer to fire
    pub async fn get_next_timer_info(&self) -> Option<(String, u64)> {
        let stats = self.timer_system.get_timer_stats().await;
        let timers = self.timer_system.timers.read().await;
        
        let mut next_timer = None;
        let mut shortest_wait = u64::MAX;
        
        for (name, timer) in timers.iter() {
            if !timer.enabled {
                continue;
            }
            
            let time_since_last = if let Some(last_triggered) = timer.last_triggered {
                let elapsed = chrono::Utc::now().signed_duration_since(last_triggered);
                elapsed.num_seconds().max(0) as u64
            } else {
                timer.interval_seconds // Will fire immediately
            };
            
            let time_until_next = if time_since_last >= timer.interval_seconds {
                0 // Ready to fire
            } else {
                timer.interval_seconds - time_since_last
            };
            
            if time_until_next < shortest_wait {
                shortest_wait = time_until_next;
                next_timer = Some(name.clone());
            }
        }
        
        next_timer.map(|name| (name, shortest_wait))
    }
}