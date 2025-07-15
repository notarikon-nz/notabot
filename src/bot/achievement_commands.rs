use anyhow::Result;
use log::{warn};
use std::sync::Arc;

use crate::bot::achievements::{AchievementSystem, AchievementRarity};
use crate::types::ChatMessage;

pub struct AchievementCommands {
    achievement_system: Arc<AchievementSystem>,
}

impl AchievementCommands {
    pub fn new(achievement_system: Arc<AchievementSystem>) -> Self {
        Self { achievement_system }
    }

    /// Process achievement-related commands
    pub async fn process_command(
        &self,
        command: &str,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<bool> {
        match command {
            "achievements" | "achieve" => {
                self.handle_achievements_command(args, message, response_sender).await?;
                Ok(true)
            }
            "achievement" => {
                self.handle_single_achievement_command(args, message, response_sender).await?;
                Ok(true)
            }
            "progress" => {
                self.handle_progress_command(args, message, response_sender).await?;
                Ok(true)
            }
            "achievementleaderboard" | "achievetop" => {
                self.handle_achievement_leaderboard_command(args, message, response_sender).await?;
                Ok(true)
            }
            "achievementstats" => {
                self.handle_achievement_stats_command(message, response_sender).await?;
                Ok(true)
            }
            _ => Ok(false), // Command not handled by achievement system
        }
    }

    async fn handle_achievements_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let target_user = if args.is_empty() {
            message.username.as_str()
        } else {
            args[0]
        };

        let user_id = format!("{}:{}", message.platform, target_user);
        let achievements = self.achievement_system.get_achievements_for_user(&user_id).await;
        
        if achievements.is_empty() {
            let response = format!(
                "🏆 No achievements available yet! Start chatting and watching to unlock them! ✨"
            );
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let unlocked: Vec<_> = achievements.iter().filter(|(_, unlocked, _)| *unlocked).collect();
        let total_unlocked = unlocked.len();
        let total_available = achievements.len();
        
        if total_unlocked == 0 {
            let response = format!(
                "🏆 {} has 0/{} achievements unlocked! Start participating to earn your first achievement! 🌟",
                target_user, total_available
            );
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        // Show recent achievements (last 3)
        let mut recent_achievements = unlocked.clone();
        recent_achievements.sort_by(|(a, _, _), (b, _, _)| {
            let a_rarity = &a.rarity;
            let b_rarity = &b.rarity;
            b_rarity.cmp(a_rarity) // Sort by rarity (rarest first)
        });
        recent_achievements.truncate(3);

        let achievements_str = recent_achievements.iter()
            .map(|(achievement, _, _)| {
                format!("{} {}", achievement.badge_emoji, achievement.name)
            })
            .collect::<Vec<_>>()
            .join(" | ");

        let response = format!(
            "🏆 {} has {}/{} achievements! Recent: {} ✨ (!achievement <name> for details)",
            target_user, total_unlocked, total_available, achievements_str
        );

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_single_achievement_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        if args.is_empty() {
            let response = "Usage: !achievement <name> - View details about a specific achievement 🏆".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let achievement_name = args.join(" ").to_lowercase();
        let user_id = format!("{}:{}", message.platform, message.username);
        let achievements = self.achievement_system.get_achievements_for_user(&user_id).await;
        
        // Find achievement by name (case insensitive)
        let found_achievement = achievements.iter()
            .find(|(achievement, _, _)| {
                achievement.name.to_lowercase().contains(&achievement_name) ||
                achievement.id.to_lowercase().contains(&achievement_name)
            });

        if let Some((achievement, unlocked, progress)) = found_achievement {
            let rarity_color = match achievement.rarity {
                AchievementRarity::Common => "⚪",
                AchievementRarity::Uncommon => "🟢", 
                AchievementRarity::Rare => "🔵",
                AchievementRarity::Epic => "🟣",
                AchievementRarity::Legendary => "🟠",
                AchievementRarity::Mythic => "🔴",
            };

            let status = if *unlocked {
                "✅ UNLOCKED"
            } else {
                "🔒 Locked"
            };

            let progress_info = if !unlocked {
                // Show progress for locked achievements
                match &achievement.requirement {
                    crate::bot::achievements::AchievementRequirement::MessageCount(target) => {
                        format!(" | Progress: {}/{} messages", progress, target)
                    }
                    crate::bot::achievements::AchievementRequirement::WatchTime(target) => {
                        format!(" | Progress: {}/{} minutes", progress, target)
                    }
                    crate::bot::achievements::AchievementRequirement::PointsEarned(target) => {
                        format!(" | Progress: {}/{} points", progress, target)
                    }
                    crate::bot::achievements::AchievementRequirement::CommandsUsed(target) => {
                        format!(" | Progress: {}/{} commands", progress, target)
                    }
                    _ => " | Progress: Hidden".to_string(),
                }
            } else {
                String::new()
            };

            let response = format!(
                "{} {} {} | {} | Reward: {} pts | {} {}{}",
                achievement.badge_emoji,
                achievement.name,
                status,
                achievement.description,
                achievement.reward_points,
                rarity_color,
                format!("{:?}", achievement.rarity),
                progress_info
            );

            self.send_response(response, message, response_sender).await?;
        } else {
            let response = format!(
                "❌ Achievement '{}' not found! Use !achievements to see all available achievements.",
                achievement_name
            );
            self.send_response(response, message, response_sender).await?;
        }

        Ok(())
    }

    async fn handle_progress_command(
        &self,
        args: &[&str],
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let target_user = if args.is_empty() {
            message.username.as_str()
        } else {
            args[0]
        };

        let user_id = format!("{}:{}", message.platform, target_user);
        let achievements = self.achievement_system.get_achievements_for_user(&user_id).await;
        
        // Show progress on closest achievements (not yet unlocked)
        let mut in_progress: Vec<_> = achievements.iter()
            .filter(|(_, unlocked, progress)| !unlocked && *progress > 0)
            .collect();

        // Sort by progress percentage
        in_progress.sort_by(|(a, _, a_progress), (b, _, b_progress)| {
            let a_percent = calculate_progress_percent(&a.requirement, *a_progress);
            let b_percent = calculate_progress_percent(&b.requirement, *b_progress);
            b_percent.partial_cmp(&a_percent).unwrap_or(std::cmp::Ordering::Equal)
        });

        in_progress.truncate(3); // Show top 3 closest

        if in_progress.is_empty() {
            let response = format!(
                "📈 {} has no achievements in progress! Start chatting to begin earning achievements! 🚀",
                target_user
            );
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let progress_str = in_progress.iter()
            .map(|(achievement, _, progress)| {
                let percent = calculate_progress_percent(&achievement.requirement, *progress);
                format!("{} {} ({}%)", 
                        achievement.badge_emoji, 
                        achievement.name, 
                        percent as u32)
            })
            .collect::<Vec<_>>()
            .join(" | ");

        let response = format!(
            "📈 {} closest achievements: {} 🎯",
            target_user, progress_str
        );

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_achievement_leaderboard_command(
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

        let leaderboard = self.achievement_system.get_achievement_leaderboard(limit).await;
        
        if leaderboard.is_empty() {
            let response = "🏆 No achievements unlocked yet! Be the first to start earning! ⭐".to_string();
            self.send_response(response, message, response_sender).await?;
            return Ok(());
        }

        let mut response = format!("🏆 TOP {} ACHIEVEMENT HUNTERS 🏆 | ", limit);
        
        for (i, (username, points, count)) in leaderboard.iter().enumerate() {
            let medal = match i {
                0 => "🥇",
                1 => "🥈", 
                2 => "🥉",
                _ => "⭐",
            };
            
            response.push_str(&format!(
                "{}{}. {} ({} pts, {} achievements) ",
                medal,
                i + 1,
                username,
                points,
                count
            ));
        }

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }

    async fn handle_achievement_stats_command(
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

        let stats = self.achievement_system.get_statistics().await;
        
        let total_achievements = stats.get("total_achievements").and_then(|v| v.as_u64()).unwrap_or(0);
        let total_users = stats.get("total_users_with_achievements").and_then(|v| v.as_u64()).unwrap_or(0);
        let total_unlocks = stats.get("total_unlocks").and_then(|v| v.as_u64()).unwrap_or(0);

        let response = format!(
            "🏆 ACHIEVEMENT STATS: {} total achievements | {} users participating | {} total unlocks | Avg: {:.1} per user 📊",
            total_achievements, 
            total_users, 
            total_unlocks,
            if total_users > 0 { total_unlocks as f64 / total_users as f64 } else { 0.0 }
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
            warn!("Failed to send achievement command response: {}", e);
        }
        Ok(())
    }

    /// Announce achievement unlock
    pub async fn announce_achievement(
        &self,
        achievement: &crate::bot::achievements::Achievement,
        username: &str,
        message: &ChatMessage,
        response_sender: &tokio::sync::mpsc::Sender<(String, String, String)>,
    ) -> Result<()> {
        let rarity_announcement = match achievement.rarity {
            AchievementRarity::Common => "🎉",
            AchievementRarity::Uncommon => "✨",
            AchievementRarity::Rare => "🌟",
            AchievementRarity::Epic => "💫",
            AchievementRarity::Legendary => "🎆",
            AchievementRarity::Mythic => "👑",
        };

        let response = format!(
            "{} {} UNLOCKED: {} {} | \"{}\" | +{} points! {}",
            rarity_announcement,
            username,
            achievement.badge_emoji,
            achievement.name,
            achievement.description,
            achievement.reward_points,
            rarity_announcement
        );

        self.send_response(response, message, response_sender).await?;
        Ok(())
    }
}

// Helper function to calculate progress percentage
fn calculate_progress_percent(requirement: &crate::bot::achievements::AchievementRequirement, progress: u64) -> f64 {
    match requirement {
        crate::bot::achievements::AchievementRequirement::MessageCount(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        crate::bot::achievements::AchievementRequirement::WatchTime(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        crate::bot::achievements::AchievementRequirement::PointsEarned(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        crate::bot::achievements::AchievementRequirement::PointsBalance(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        crate::bot::achievements::AchievementRequirement::CommandsUsed(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        crate::bot::achievements::AchievementRequirement::DaysActive(target) => {
            (progress as f64 / *target as f64) * 100.0
        }
        _ => 0.0,
    }
}


