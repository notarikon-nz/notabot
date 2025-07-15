use anyhow::Result;
use log::{warn};
use std::sync::Arc;

use crate::bot::points::PointsSystem;
use crate::types::ChatMessage;

pub struct PointsCommands {
    points_system: Arc<PointsSystem>,
}

impl PointsCommands {
    pub fn new(points_system: Arc<PointsSystem>) -> Self {
        Self { points_system }
    }

    /// Process points-related commands
    pub async fn process_command(
        &self,
        command: &str,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool> {
        match command {
            "points" | "balance" => {
                self.handle_points_command(args, message, response_sender).await?;
                Ok(true)
            }
            "leaderboard" | "top" => {
                self.handle_leaderboard_command(args, message, response_sender).await?;
                Ok(true)
            }
            "rank" => {
                self.handle_rank_command(message, response_sender).await?;
                Ok(true)
            }
            "give" | "transfer" => {
                self.handle_transfer_command(args, message, response_sender).await?;
                Ok(true)
            }
            "addpoints" => {
                self.handle_admin_add_points(args, message, response_sender).await?;
                Ok(true)
            }
            "pointstats" => {
                self.handle_points_stats(message, response_sender).await?;
                Ok(true)
            }
            _ => Ok(false), // Command not handled by points system
        }
    }

    async fn handle_points_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let (platform, username) = if args.is_empty() {
            // Show user's own points
            (message.platform.as_str(), message.username.as_str())
        } else {
            // Show another user's points
            (message.platform.as_str(), args[0])
        };

        if let Some(user_points) = self.points_system.get_user_points(platform, username).await {
            let display_name = user_points.display_name.as_deref().unwrap_or(username);
            let rank = user_points.get_rank();
            
            let response = if username == message.username {
                format!(
                    "💰 {}, you have {} points! Rank: {} | Total earned: {} | Messages: {} ⭐",
                    display_name, 
                    user_points.points,
                    rank,
                    user_points.total_earned,
                    user_points.messages_sent
                )
            } else {
                format!(
                    "💰 {} has {} points! Rank: {} | Total earned: {} ⭐",
                    display_name,
                    user_points.points,
                    rank,
                    user_points.total_earned
                )
            };

            self.send_response(response, message, response_sender).await?;
        } else {
            let response = format!(
                "❌ User {} not found. Users earn points by chatting and watching!",
                username
            );
            self.send_response(response, message, response_sender).await?;
        }

        Ok(())
    }

    async fn handle_leaderboard_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let limit = if let Some(arg) = args.get(0) {
            arg.parse::<usize>().unwrap_or(5).min(10) // Max 10 users
        } else {
            5
        };

        let leaderboard = self.points_system.get_leaderboard(limit).await;
        
        if leaderboard.is_empty() {
            let response = "📊 No users found! Start chatting to earn points! 💫".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let mut response = format!("🏆 TOP {} POINTS LEADERBOARD 🏆 | ", limit);
        
        for (i, user) in leaderboard.iter().enumerate() {
            let display_name = user.display_name.as_deref().unwrap_or(&user.username);
            let medal = match i {
                0 => "🥇",
                1 => "🥈", 
                2 => "🥉",
                _ => "⭐",
            };
            
            response.push_str(&format!(
                "{}{}. {} ({} pts) ",
                medal,
                i + 1,
                display_name,
                user.points
            ));
        }

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_rank_command(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if let Some(user_points) = self.points_system.get_user_points(&message.platform, &message.username).await {
            let rank = user_points.get_rank();
            let display_name = user_points.display_name.as_deref().unwrap_or(&message.username);
            
            let response = format!(
                "🎖️ {}, your rank is: {} | Points: {} | Watch time: {}h | Multiplier: {}x ⚡",
                display_name,
                rank,
                user_points.points,
                user_points.minutes_watched / 60,
                user_points.multiplier
            );

            self.send_response(response, message, response_sender).await?;
        } else {
            let response = "❌ You haven't earned any points yet! Start chatting to begin! 🚀".to_string();
            self.send_response(response, message, response_sender).await?;
        }

        Ok(())
    }

    async fn handle_transfer_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.len() < 2 {
            let response = "Usage: !give <username> <amount> - Transfer points to another user 💸".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let target_user = args[0];
        let amount = match args[1].parse::<i64>() {
            Ok(amt) if amt > 0 => amt,
            _ => {
                let response = "❌ Please enter a valid positive amount!".to_string();
                self.send_response(response, message, response_sender).await?;
                return Ok(());
            }
        };

        // Minimum transfer amount
        if amount < 10 {
            let response = "❌ Minimum transfer amount is 10 points!".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        // Can't transfer to yourself
        if target_user.to_lowercase() == message.username.to_lowercase() {
            let response = "❌ You can't transfer points to yourself! 🤷".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let success = self.points_system.transfer_points(
            &message.platform, &message.username,
            &message.platform, target_user,
            amount
        ).await?;

        let response = if success {
            format!(
                "✅ {} successfully transferred {} points to {}! 💝",
                message.username, amount, target_user
            )
        } else {
            format!(
                "❌ Transfer failed! You might not have enough points or {} doesn't exist.",
                target_user
            )
        };

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_admin_add_points(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        // Check if user is moderator
        if !message.is_mod {
            let response = "❌ This command is for moderators only!".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        if args.len() < 2 {
            let response = "Usage: !addpoints <username> <amount> [reason] - Add points to user (mod only)".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let target_user = args[0];
        let amount = match args[1].parse::<i64>() {
            Ok(amt) => amt,
            Err(_) => {
                let response = "❌ Please enter a valid amount!".to_string();
                self.send_response(response, message, response_sender).await?;
                return Ok(());
            }
        };

        let reason = if args.len() > 2 {
            args[2..].join(" ")
        } else {
            format!("Admin bonus from {}", message.username)
        };

        let success = self.points_system.add_points(
            &message.platform, target_user, amount, &reason
        ).await?;

        let response = if success {
            format!(
                "✅ Added {} points to {}! Reason: {} 🎁",
                amount, target_user, reason
            )
        } else {
            format!(
                "❌ Failed to add points to {}. User might not exist.",
                target_user
            )
        };

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_points_stats(
        &self,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        // Check if user is moderator
        if !message.is_mod {
            let response = "❌ This command is for moderators only!".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let stats = self.points_system.get_statistics().await;
        
        let total_users = stats.get("total_users").and_then(|v| v.as_u64()).unwrap_or(0);
        let circulating = stats.get("total_points_circulating").and_then(|v| v.as_i64()).unwrap_or(0);
        let earned = stats.get("total_points_earned").and_then(|v| v.as_i64()).unwrap_or(0);
        let spent = stats.get("total_points_spent").and_then(|v| v.as_i64()).unwrap_or(0);
        let active_24h = stats.get("active_users_24h").and_then(|v| v.as_u64()).unwrap_or(0);

        let response = format!(
            "📊 POINTS STATS: {} users | {} points circulating | {} earned | {} spent | {} active (24h) 💰",
            total_users, circulating, earned, spent, active_24h
        );

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

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
            warn!("Failed to send points command response: {}", e);
        }
        Ok(())
    }
}

// Helper functions for common point operations
impl PointsCommands {
    /// Get formatted user points string
    pub async fn get_user_points_string(&self, platform: &str, username: &str) -> String {
        if let Some(user) = self.points_system.get_user_points(platform, username).await {
            format!("{} points", user.points)
        } else {
            "0 points".to_string()
        }
    }

    /// Check if user has enough points for something
    pub async fn user_has_points(&self, platform: &str, username: &str, amount: i64) -> bool {
        if let Some(user) = self.points_system.get_user_points(platform, username).await {
            user.points >= amount
        } else {
            false
        }
    }

    /// Quick spend points with response
    pub async fn spend_user_points(
        &self,
        platform: &str,
        username: &str,
        amount: i64,
        reason: &str,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool> {
        let success = self.points_system.spend_points(platform, username, amount, reason).await?;
        
        if success {
            let response = format!(
                "✅ {} spent {} points on {}! 💸",
                username, amount, reason
            );
            self.send_response(response, message, response_sender).await?;
        } else {
            let response = format!(
                "❌ {} doesn't have enough points! Need: {} 💔",
                username, amount
            );
            self.send_response(response, message, response_sender).await?;
        }
        
        Ok(success)
    }

    /// Award bonus points with announcement
    pub async fn award_bonus_points(
        &self,
        platform: &str,
        username: &str,
        amount: i64,
        reason: &str,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if self.points_system.add_points(platform, username, amount, reason).await? {
            let response = format!(
                "🎉 {} earned {} bonus points for {}! 🌟",
                username, amount, reason
            );
            self.send_response(response, message, response_sender).await?;
        }
        Ok(())
    }
}
