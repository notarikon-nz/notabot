use anyhow::Result;
use std::sync::Arc;
use crate::types::{ChatMessage, GiveawayType};

use super::giveaways::{GiveawaySystem};

// Example giveaway command handler that would be added to the command processing system
pub struct GiveawayCommands {
    giveaway_system: Arc<GiveawaySystem>,
}

impl GiveawayCommands {
    pub fn new(giveaway_system: Arc<GiveawaySystem>) -> Self {
        Self { giveaway_system }
    }

    /// Process giveaway-related commands
    pub async fn process_command(
        &self,
        command: &str,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        match command {
            "gstart" => {
                if !message.is_mod {
                    self.send_response("Only moderators can start giveaways.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_start_giveaway(args, message, response_sender).await?;
                Ok(true)
            }
            "gend" => {
                if !message.is_mod {
                    self.send_response("Only moderators can end giveaways.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_end_giveaway(message, response_sender).await?;
                Ok(true)
            }
            "gcancel" => {
                if !message.is_mod {
                    self.send_response("Only moderators can cancel giveaways.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_cancel_giveaway(message, response_sender).await?;
                Ok(true)
            }
            "gstatus" => {
                self.handle_giveaway_status(message, response_sender).await?;
                Ok(true)
            }
            "geligible" => {
                if !message.is_mod {
                    self.send_response("Only moderators can manage eligibility.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_toggle_eligibility(args, message, response_sender).await?;
                Ok(true)
            }
            "greset" => {
                if !message.is_mod {
                    self.send_response("Only moderators can reset eligibility.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_reset_eligibility(message, response_sender).await?;
                Ok(true)
            }
            "gstats" => {
                if !message.is_mod {
                    self.send_response("Only moderators can view giveaway statistics.".to_string(), message, response_sender).await?;
                    return Ok(true);
                }
                self.handle_giveaway_stats(message, response_sender).await?;
                Ok(true)
            }
            _ => Ok(false), // Command not handled
        }
    }

    async fn handle_start_giveaway(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if args.is_empty() {
            self.send_response("Usage: !gstart <active|keyword|number> [options]".to_string(), message, response_sender).await?;
            return Ok(());
        }

        let giveaway_type = match args[0].to_lowercase().as_str() {
            "active" => {
                let duration = if args.len() > 1 {
                    args[1].parse::<u32>().unwrap_or(10)
                } else {
                    10
                };
                GiveawayType::ActiveUser {
                    duration_minutes: duration,
                    min_messages: None,
                }
            }
            "keyword" => {
                if args.len() < 2 {
                    self.send_response("Usage: !gstart keyword <word>".to_string(), message, response_sender).await?;
                    return Ok(());
                }
                GiveawayType::Keyword {
                    keyword: args[1].to_string(),
                    case_sensitive: false,
                    anti_spam: true,
                    max_entries_per_user: Some(1),
                }
            }
            "number" => {
                let min = if args.len() > 1 { args[1].parse::<u32>().unwrap_or(1) } else { 1 };
                let max = if args.len() > 2 { args[2].parse::<u32>().unwrap_or(100) } else { 100 };
                GiveawayType::RandomNumber {
                    min,
                    max,
                    auto_generate: true,
                }
            }
            _ => {
                self.send_response("Invalid giveaway type. Use: active, keyword, or number".to_string(), message, response_sender).await?;
                return Ok(());
            }
        };

        match self.giveaway_system.start_giveaway(
            giveaway_type.clone(),
            message.username.clone(),
            message.channel.clone(),
            message.platform.clone(),
            None,
        ).await {
            Ok(giveaway_id) => {
                let response = match giveaway_type {
                    GiveawayType::ActiveUser { duration_minutes, .. } => {
                        format!("Active User Giveaway started! Chat within {} minutes to be eligible. Good luck!", duration_minutes)
                    }
                    GiveawayType::Keyword { keyword, .. } => {
                        format!("Keyword Giveaway started! Type '{}' to enter. One entry per person!", keyword)
                    }
                    GiveawayType::RandomNumber { min, max, .. } => {
                        if let Some(status) = self.giveaway_system.get_giveaway_status().await {
                            if let Some(number) = status.generated_number {
                                format!("Random Number Giveaway! First person to type {} wins! (Range: {}-{})", number, min, max)
                            } else {
                                format!("Random Number Giveaway started! Range: {}-{}", min, max)
                            }
                        } else {
                            format!("Random Number Giveaway started! Range: {}-{}", min, max)
                        }
                    }
                };
                self.send_response(response, message, response_sender).await?;
            }
            Err(e) => {
                self.send_response(format!("Failed to start giveaway: {}", e), message, response_sender).await?;
            }
        }

        Ok(())
    }

    async fn handle_end_giveaway(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.giveaway_system.end_giveaway(false).await {
            Ok(Some(winner)) => {
                let response = format!(
                    "Giveaway ended! Winner: {} from {}! Congratulations! {}",
                    winner.username,
                    winner.platform,
                    winner.channel_url.unwrap_or_else(|| "".to_string())
                );
                self.send_response(response, message, response_sender).await?;
            }
            Ok(None) => {
                self.send_response("Giveaway ended but no winner was selected (no eligible users)".to_string(), message, response_sender).await?;
            }
            Err(e) => {
                self.send_response(format!("Failed to end giveaway: {}", e), message, response_sender).await?;
            }
        }
        Ok(())
    }

    async fn handle_cancel_giveaway(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.giveaway_system.cancel_giveaway(Some("Cancelled by moderator".to_string())).await {
            Ok(_) => {
                self.send_response("Giveaway cancelled by moderator".to_string(), message, response_sender).await?;
            }
            Err(e) => {
                self.send_response(format!("Failed to cancel giveaway: {}", e), message, response_sender).await?;
            }
        }
        Ok(())
    }

    async fn handle_giveaway_status(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.giveaway_system.get_giveaway_status().await {
            Some(status) => {
                let type_description = match status.giveaway_type {
                    GiveawayType::ActiveUser { duration_minutes, .. } => {
                        format!("Active User ({}min)", duration_minutes)
                    }
                    GiveawayType::Keyword { ref keyword, .. } => {
                        format!("Keyword ('{}')", keyword)
                    }
                    GiveawayType::RandomNumber { min, max, .. } => {
                        if let Some(number) = status.generated_number {
                            format!("Random Number ({})", number)
                        } else {
                            format!("Random Number ({}-{})", min, max)
                        }
                    }
                };

                let time_info = if let Some(remaining) = status.time_remaining {
                    if remaining > 0 {
                        format!(" | {}s remaining", remaining)
                    } else {
                        " | Time expired".to_string()
                    }
                } else {
                    "".to_string()
                };

                let response = format!(
                    "Giveaway Status: {} | {} participants | {} total entries{}",
                    type_description, status.participant_count, status.total_entries, time_info
                );
                self.send_response(response, message, response_sender).await?;
            }
            None => {
                self.send_response("No active giveaway. Use !gstart to begin one!".to_string(), message, response_sender).await?;
            }
        }
        Ok(())
    }

    async fn handle_toggle_eligibility(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let username = if args.is_empty() {
            &message.username
        } else {
            args[0]
        };

        match self.giveaway_system.toggle_user_eligibility(&message.platform, username).await {
            Ok(is_eligible) => {
                let status = if is_eligible { "eligible" } else { "ineligible" };
                self.send_response(format!("User {} is now {}", username, status), message, response_sender).await?;
            }
            Err(e) => {
                self.send_response(format!("Failed to toggle eligibility: {}", e), message, response_sender).await?;
            }
        }
        Ok(())
    }

    async fn handle_reset_eligibility(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.giveaway_system.reset_eligibility().await {
            Ok(reset_count) => {
                self.send_response(format!("Reset eligibility for {} users", reset_count), message, response_sender).await?;
            }
            Err(e) => {
                self.send_response(format!("Failed to reset eligibility: {}", e), message, response_sender).await?;
            }
        }
        Ok(())
    }

    async fn handle_giveaway_stats(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stats = self.giveaway_system.get_statistics().await;
        
        let success_rate = if stats.total_giveaways > 0 {
            (stats.successful_giveaways as f64 / stats.total_giveaways as f64) * 100.0
        } else {
            0.0
        };

        let response = format!(
            "Giveaway Stats: {} total | {} successful ({:.1}%) | {} participants total | {:.1} avg participants",
            stats.total_giveaways,
            stats.successful_giveaways,
            success_rate,
            stats.total_participants,
            stats.average_participants_per_giveaway
        );
        
        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn send_response(
        &self,
        response: String,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Err(e) = response_sender.send((
            message.platform.clone(),
            message.channel.clone(),
            response
        )).await {
            log::warn!("Failed to send giveaway command response: {}", e);
        }
        Ok(())
    }
}