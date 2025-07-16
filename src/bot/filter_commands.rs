use anyhow::Result;
use log::{warn, info};
use std::sync::Arc;

use crate::bot::moderation::ModerationSystem;
use crate::types::{ChatMessage, SpamFilterType, ExemptionLevel, ModerationEscalation, ModerationAction};

pub struct FilterCommands {
    moderation_system: Arc<ModerationSystem>,
}

impl FilterCommands {
    pub fn new(moderation_system: Arc<ModerationSystem>) -> Self {
        Self { moderation_system }
    }

    /// Process filter-related commands (!filters, !blacklist, etc.)
    pub async fn process_command(
        &self,
        command: &str,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool> {
        // Only moderators can manage filters
        if !message.is_mod {
            return Ok(false);
        }

        match command {
            "filters" => {
                self.handle_filters_command(args, message, response_sender).await?;
                Ok(true)
            }
            "blacklist" => {
                self.handle_blacklist_command(args, message, response_sender).await?;
                Ok(true)
            }
            "filterlist" => {
                self.handle_filter_list_command(message, response_sender).await?;
                Ok(true)
            }
            "filterstats" => {
                self.handle_filter_stats_command(message, response_sender).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Handle !filters command with subcommands
    async fn handle_filters_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.is_empty() {
            let response = "🛡️ Filter Commands: !filters <enable|disable|add|remove|list> | !blacklist <add|remove|list> <pattern> | !filterstats".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        match args[0].to_lowercase().as_str() {
            "enable" => {
                if args.len() < 2 {
                    self.moderation_system.set_spam_protection_enabled(true).await;
                    let response = "✅ Global spam protection enabled".to_string();
                    self.send_response(response, message, response_sender).await?;
                } else {
                    let filter_name = args[1];
                    match self.moderation_system.set_filter_enabled(filter_name, true).await {
                        Ok(_) => {
                            let response = format!("✅ Filter '{}' enabled", filter_name);
                            self.send_response(response, message, response_sender).await?;
                        }
                        Err(_) => {
                            let response = format!("❌ Filter '{}' not found", filter_name);
                            self.send_response(response, message, response_sender).await?;
                        }
                    }
                }
            }
            "disable" => {
                if args.len() < 2 {
                    self.moderation_system.set_spam_protection_enabled(false).await;
                    let response = "⏸️ Global spam protection disabled".to_string();
                    self.send_response(response, message, response_sender).await?;
                } else {
                    let filter_name = args[1];
                    match self.moderation_system.set_filter_enabled(filter_name, false).await {
                        Ok(_) => {
                            let response = format!("⏸️ Filter '{}' disabled", filter_name);
                            self.send_response(response, message, response_sender).await?;
                        }
                        Err(_) => {
                            let response = format!("❌ Filter '{}' not found", filter_name);
                            self.send_response(response, message, response_sender).await?;
                        }
                    }
                }
            }
            "add" => {
                self.handle_add_filter_command(&args[1..], message, response_sender).await?;
            }
            "remove" => {
                if args.len() < 2 {
                    let response = "Usage: !filters remove <filter_name>".to_string();
                    self.send_response(response, message, response_sender).await?;
                    return Ok(());
                }
                
                let filter_name = args[1];
                match self.moderation_system.remove_filter(filter_name).await {
                    Ok(_) => {
                        let response = format!("🗑️ Filter '{}' removed", filter_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                    Err(_) => {
                        let response = format!("❌ Filter '{}' not found", filter_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "list" => {
                self.handle_filter_list_command(message, response_sender).await?;
            }
            _ => {
                let response = "❌ Unknown subcommand. Use: enable, disable, add, remove, list".to_string();
                self.send_response(response, message, response_sender).await?;
            }
        }

        Ok(())
    }

    /// Handle !blacklist command (NightBot style)
    async fn handle_blacklist_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.is_empty() {
            let response = "🚫 Blacklist Commands: !blacklist <add|remove|list> <pattern> | Supports: literal, wild*cards, ~/regex/flags".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        match args[0].to_lowercase().as_str() {
            "add" => {
                if args.len() < 2 {
                    let response = "Usage: !blacklist add <pattern> [timeout_seconds] [exemption_level]".to_string();
                    self.send_response(response, message, response_sender).await?;
                    return Ok(());
                }

                let pattern = args[1].to_string();
                let timeout_seconds = if args.len() > 2 {
                    args[2].parse::<u64>().unwrap_or(600)
                } else {
                    600
                };
                
                let exemption_level = if args.len() > 3 {
                    match args[3].to_lowercase().as_str() {
                        "none" => ExemptionLevel::None,
                        "subscriber" | "sub" => ExemptionLevel::Subscriber,
                        "regular" => ExemptionLevel::Regular,
                        "moderator" | "mod" => ExemptionLevel::Moderator,
                        "owner" => ExemptionLevel::Owner,
                        _ => ExemptionLevel::Moderator,
                    }
                } else {
                    ExemptionLevel::Moderator
                };

                // Generate filter name based on pattern
                let filter_name = format!("blacklist_{}", Self::sanitize_filter_name(&pattern));

                match self.moderation_system.add_blacklist_filter(
                    filter_name.clone(),
                    vec![pattern.clone()],
                    false, // Case insensitive by default
                    false, // Not whole words only by default
                    exemption_level,
                    timeout_seconds,
                    Some(format!("Blacklisted word/phrase detected")),
                ).await {
                    Ok(_) => {
                        let pattern_type = if pattern.starts_with("~/") {
                            "regex"
                        } else if pattern.contains('*') {
                            "wildcard"
                        } else {
                            "literal"
                        };
                        
                        let response = format!(
                            "✅ Added {} blacklist pattern: '{}' | Timeout: {}s | Filter: '{}'", 
                            pattern_type, pattern, timeout_seconds, filter_name
                        );
                        self.send_response(response, message, response_sender).await?;
                        info!("Added blacklist pattern '{}' by {}", pattern, message.username);
                    }
                    Err(e) => {
                        let response = format!("❌ Failed to add blacklist pattern: {}", e);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "remove" => {
                if args.len() < 2 {
                    let response = "Usage: !blacklist remove <filter_name_or_pattern>".to_string();
                    self.send_response(response, message, response_sender).await?;
                    return Ok(());
                }

                let identifier = args[1];
                let filter_name = if identifier.starts_with("blacklist_") {
                    identifier.to_string()
                } else {
                    format!("blacklist_{}", Self::sanitize_filter_name(identifier))
                };

                match self.moderation_system.remove_filter(&filter_name).await {
                    Ok(_) => {
                        let response = format!("🗑️ Removed blacklist filter: '{}'", filter_name);
                        self.send_response(response, message, response_sender).await?;
                        info!("Removed blacklist filter '{}' by {}", filter_name, message.username);
                    }
                    Err(_) => {
                        let response = format!("❌ Blacklist filter '{}' not found", filter_name);
                        self.send_response(response, message, response_sender).await?;
                    }
                }
            }
            "list" => {
                let filters = self.moderation_system.list_filters().await;
                let blacklist_filters: Vec<_> = filters.iter()
                    .filter(|(name, _)| name.starts_with("blacklist_"))
                    .collect();

                if blacklist_filters.is_empty() {
                    let response = "📝 No blacklist filters configured".to_string();
                    self.send_response(response, message, response_sender).await?;
                } else {
                    let mut response = format!("🚫 Blacklist Filters ({}): ", blacklist_filters.len());
                    for (i, (name, enabled)) in blacklist_filters.iter().enumerate() {
                        if i > 0 { response.push_str(" | "); }
                        let status = if *enabled { "✅" } else { "❌" };
                        let display_name = name.strip_prefix("blacklist_").unwrap_or(name);
                        response.push_str(&format!("{} {}", status, display_name));
                    }
                    self.send_response(response, message, response_sender).await?;
                }
            }
            _ => {
                let response = "❌ Unknown subcommand. Use: add, remove, list".to_string();
                self.send_response(response, message, response_sender).await?;
            }
        }

        Ok(())
    }

    /// Handle !filterlist command
    async fn handle_filter_list_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let filters = self.moderation_system.list_filters().await;
        
        if filters.is_empty() {
            let response = "📝 No spam filters configured".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let mut response = format!("🛡️ Spam Filters ({}): ", filters.len());
        for (i, (name, enabled)) in filters.iter().enumerate() {
            if i > 0 { response.push_str(" | "); }
            let status = if *enabled { "✅" } else { "❌" };
            response.push_str(&format!("{} {}", status, name));
        }

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    /// Handle !filterstats command
    async fn handle_filter_stats_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let stats = self.moderation_system.get_filter_stats().await;
        
        let total = stats.get("total_filters").and_then(|v| v.as_u64()).unwrap_or(0);
        let enabled = stats.get("enabled_filters").and_then(|v| v.as_u64()).unwrap_or(0);
        let violations = stats.get("total_violations").and_then(|v| v.as_u64()).unwrap_or(0);
        let global_enabled = stats.get("global_enabled").and_then(|v| v.as_bool()).unwrap_or(false);

        let response = format!(
            "📊 Filter Stats: {} total | {} enabled | {} violations | Global: {} | Use !filterlist for details",
            total, enabled, violations, if global_enabled { "ON" } else { "OFF" }
        );

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    /// Handle adding new filters with advanced configuration
    async fn handle_add_filter_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.is_empty() {
            let response = "Usage: !filters add <type> [options] | Types: caps, links, length, emotes, symbols, rate, repeat".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let filter_type = args[0].to_lowercase();
        let filter_name = format!("{}_{}", filter_type, chrono::Utc::now().timestamp());

        let spam_filter_type = match filter_type.as_str() {
            "caps" => {
                let percentage = if args.len() > 1 {
                    args[1].parse::<u8>().unwrap_or(70)
                } else {
                    70
                };
                SpamFilterType::ExcessiveCaps { max_percentage: percentage }
            }
            "links" => {
                let allow_mods = args.get(1).map_or(true, |s| *s != "false");
                let whitelist = if args.len() > 2 {
                    args[2..].iter().map(|s| s.to_string()).collect()
                } else {
                    vec!["discord.gg".to_string(), "youtube.com".to_string()]
                };
                SpamFilterType::LinkBlocking { allow_mods, whitelist }
            }
            "length" => {
                let max_length = if args.len() > 1 {
                    args[1].parse::<usize>().unwrap_or(500)
                } else {
                    500
                };
                SpamFilterType::MessageLength { max_length }
            }
            "emotes" => {
                let max_count = if args.len() > 1 {
                    args[1].parse::<u8>().unwrap_or(10)
                } else {
                    10
                };
                SpamFilterType::ExcessiveEmotes { max_count }
            }
            "symbols" => {
                let max_percentage = if args.len() > 1 {
                    args[1].parse::<u8>().unwrap_or(50)
                } else {
                    50
                };
                SpamFilterType::SymbolSpam { max_percentage }
            }
            "rate" => {
                let max_messages = if args.len() > 1 {
                    args[1].parse::<u8>().unwrap_or(5)
                } else {
                    5
                };
                let window_seconds = if args.len() > 2 {
                    args[2].parse::<u64>().unwrap_or(30)
                } else {
                    30
                };
                SpamFilterType::RateLimit { max_messages, window_seconds }
            }
            "repeat" => {
                let max_repeats = if args.len() > 1 {
                    args[1].parse::<u8>().unwrap_or(3)
                } else {
                    3
                };
                let window_seconds = if args.len() > 2 {
                    args[2].parse::<u64>().unwrap_or(300)
                } else {
                    300
                };
                SpamFilterType::RepeatedMessages { max_repeats, window_seconds }
            }
            _ => {
                let response = "❌ Unknown filter type. Available: caps, links, length, emotes, symbols, rate, repeat".to_string();
                self.send_response(response, message, response_sender).await?;
                return Ok(());
            }
        };

        let escalation = ModerationEscalation {
            first_offense: ModerationAction::WarnUser { 
                message: "Please follow chat rules (first warning)".to_string() 
            },
            repeat_offense: ModerationAction::TimeoutUser { duration_seconds: 600 },
            offense_window_seconds: 3600,
        };

        match self.moderation_system.add_spam_filter_advanced(
            filter_name.clone(),
            spam_filter_type,
            escalation,
            ExemptionLevel::Moderator,
            false,
            None,
        ).await {
            Ok(_) => {
                let response = format!("✅ Added {} filter: '{}'", filter_type, filter_name);
                self.send_response(response, message, response_sender).await?;
                info!("Added spam filter '{}' by {}", filter_name, message.username);
            }
            Err(e) => {
                let response = format!("❌ Failed to add filter: {}", e);
                self.send_response(response, message, response_sender).await?;
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
            warn!("Failed to send filter command response: {}", e);
        }
        Ok(())
    }

    /// Sanitize pattern for use in filter names
    fn sanitize_filter_name(pattern: &str) -> String {
        pattern
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }
}