use log::{info};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::bot::points::UserPoints;

/// Achievement definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: AchievementCategory,
    pub requirement: AchievementRequirement,
    pub reward_points: i64,
    pub badge_emoji: String,
    pub rarity: AchievementRarity,
    pub hidden: bool, // Hidden until unlocked
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AchievementCategory {
    Engagement,    // Chat and participation
    Loyalty,       // Time-based achievements
    Social,        // Community interaction
    Points,        // Point milestones
    Special,       // Event or rare achievements
    Moderation,    // Mod-specific achievements
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AchievementRequirement {
    MessageCount(u64),           // Send X messages
    WatchTime(u64),             // Watch for X minutes
    PointsEarned(i64),          // Earn X total points
    PointsBalance(i64),         // Have X points at once
    CommandsUsed(u64),          // Use X commands
    DaysActive(u64),            // Be active for X days
    TransferPoints(i64),        // Transfer X points to others
    ReceiveTransfer(i64),       // Receive X points from others
    Streak(u64),                // X consecutive days active
    LeaderboardPosition(u8),    // Reach top X position
    HelpNewcomers(u64),         // Transfer points to newcomers X times
    Custom(String),             // Custom achievement logic
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
pub enum AchievementRarity {
    Common,     // Easy to get
    Uncommon,   // Moderate effort
    Rare,       // Significant effort
    Epic,       // High dedication
    Legendary,  // Exceptional achievement
    Mythic,     // Nearly impossible
}

/// User achievement progress and unlocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAchievements {
    pub user_id: String,
    pub unlocked: HashSet<String>, // Achievement IDs
    pub progress: HashMap<String, u64>, // Achievement ID -> progress value
    pub unlock_timestamps: HashMap<String, chrono::DateTime<chrono::Utc>>,
    pub total_achievement_points: i64,
    pub rarest_achievement: Option<AchievementRarity>,
}

impl UserAchievements {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            unlocked: HashSet::new(),
            progress: HashMap::new(),
            unlock_timestamps: HashMap::new(),
            total_achievement_points: 0,
            rarest_achievement: None,
        }
    }

    pub fn unlock_achievement(&mut self, achievement: &Achievement) -> bool {
        if self.unlocked.insert(achievement.id.clone()) {
            self.unlock_timestamps.insert(achievement.id.clone(), chrono::Utc::now());
            self.total_achievement_points += achievement.reward_points;
            
            // Update rarest achievement
            if self.rarest_achievement.is_none() || achievement.rarity > *self.rarest_achievement.as_ref().unwrap() {
                self.rarest_achievement = Some(achievement.rarity.clone());
            }
            
            true
        } else {
            false
        }
    }

    pub fn has_achievement(&self, achievement_id: &str) -> bool {
        self.unlocked.contains(achievement_id)
    }

    pub fn get_progress(&self, achievement_id: &str) -> u64 {
        self.progress.get(achievement_id).copied().unwrap_or(0)
    }

    pub fn set_progress(&mut self, achievement_id: String, progress: u64) {
        self.progress.insert(achievement_id, progress);
    }
}

pub struct AchievementSystem {
    achievements: Arc<RwLock<HashMap<String, Achievement>>>,
    user_achievements: Arc<RwLock<HashMap<String, UserAchievements>>>,
}

impl AchievementSystem {
    pub fn new() -> Self {
        let system = Self {
            achievements: Arc::new(RwLock::new(HashMap::new())),
            user_achievements: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Initialize with default achievements
        tokio::spawn(async move {
            // This will be called after system is created
        });
        
        system
    }

    /// Initialize the achievement system with default achievements
    pub async fn initialize_default_achievements(&self) {
        let default_achievements = self.create_default_achievements();
        let mut achievements = self.achievements.write().await;
        
        for achievement in default_achievements {
            achievements.insert(achievement.id.clone(), achievement);
        }
        
        info!("Initialized {} default achievements", achievements.len());
    }

    /// Check user progress and unlock achievements
    pub async fn check_achievements(&self, user_points: &UserPoints) -> Vec<Achievement> {
        let mut newly_unlocked = Vec::new();
        
        let achievements = self.achievements.read().await;
        let mut user_achievements = self.user_achievements.write().await;
        
        let user_achievement = user_achievements
            .entry(user_points.user_id.clone())
            .or_insert_with(|| UserAchievements::new(user_points.user_id.clone()));
        
        for achievement in achievements.values() {
            if user_achievement.has_achievement(&achievement.id) {
                continue; // Already unlocked
            }
            
            let (meets_requirement, progress) = self.check_requirement(&achievement.requirement, user_points);
            user_achievement.set_progress(achievement.id.clone(), progress);
            
            if meets_requirement {
                if user_achievement.unlock_achievement(achievement) {
                    newly_unlocked.push(achievement.clone());
                    info!("🏆 {} unlocked achievement: {} (+{} points)", 
                          user_points.username, achievement.name, achievement.reward_points);
                }
            }
        }
        
        newly_unlocked
    }

    /// Check if user meets achievement requirement
    fn check_requirement(&self, requirement: &AchievementRequirement, user: &UserPoints) -> (bool, u64) {
        match requirement {
            AchievementRequirement::MessageCount(target) => {
                (user.messages_sent >= *target, user.messages_sent)
            }
            AchievementRequirement::WatchTime(target) => {
                (user.minutes_watched >= *target, user.minutes_watched)
            }
            AchievementRequirement::PointsEarned(target) => {
                (user.total_earned >= *target, user.total_earned as u64)
            }
            AchievementRequirement::PointsBalance(target) => {
                (user.points >= *target, user.points as u64)
            }
            AchievementRequirement::CommandsUsed(target) => {
                (user.commands_used >= *target, user.commands_used)
            }
            AchievementRequirement::DaysActive(target) => {
                let days_active = chrono::Utc::now()
                    .signed_duration_since(user.first_seen)
                    .num_days() as u64;
                (days_active >= *target, days_active)
            }
            // Additional requirements would need more data tracking
            _ => (false, 0), // Not implemented yet
        }
    }

    /// Get user's achievements
    pub async fn get_user_achievements(&self, user_id: &str) -> Option<UserAchievements> {
        self.user_achievements.read().await.get(user_id).cloned()
    }

    /// Get all achievements with user's unlock status
    pub async fn get_achievements_for_user(&self, user_id: &str) -> Vec<(Achievement, bool, u64)> {
        let achievements = self.achievements.read().await;
        let user_achievements = self.user_achievements.read().await;
        
        let user_data = user_achievements.get(user_id);
        
        achievements.values()
            .filter(|a| !a.hidden || user_data.map_or(false, |u| u.has_achievement(&a.id)))
            .map(|achievement| {
                let unlocked = user_data.map_or(false, |u| u.has_achievement(&achievement.id));
                let progress = user_data.map_or(0, |u| u.get_progress(&achievement.id));
                (achievement.clone(), unlocked, progress)
            })
            .collect()
    }

    /// Get leaderboard by achievement points
    pub async fn get_achievement_leaderboard(&self, limit: usize) -> Vec<(String, i64, usize)> {
        let user_achievements = self.user_achievements.read().await;
        let mut leaderboard: Vec<_> = user_achievements.values()
            .map(|ua| (
                ua.user_id.split(':').nth(1).unwrap_or("Unknown").to_string(),
                ua.total_achievement_points,
                ua.unlocked.len()
            ))
            .collect();
        
        leaderboard.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        leaderboard.truncate(limit);
        leaderboard
    }

    /// Create default achievements
    fn create_default_achievements(&self) -> Vec<Achievement> {
        vec![
            // Engagement Achievements
            Achievement {
                id: "first_message".to_string(),
                name: "First Words".to_string(),
                description: "Send your first chat message".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::MessageCount(1),
                reward_points: 50,
                badge_emoji: "👋".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "chatterbox".to_string(),
                name: "Chatterbox".to_string(),
                description: "Send 100 messages".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::MessageCount(100),
                reward_points: 250,
                badge_emoji: "💬".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "conversationalist".to_string(),
                name: "Conversationalist".to_string(),
                description: "Send 1,000 messages".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::MessageCount(1000),
                reward_points: 1000,
                badge_emoji: "🗣️".to_string(),
                rarity: AchievementRarity::Uncommon,
                hidden: false,
            },
            Achievement {
                id: "chat_legend".to_string(),
                name: "Chat Legend".to_string(),
                description: "Send 10,000 messages".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::MessageCount(10000),
                reward_points: 5000,
                badge_emoji: "👑".to_string(),
                rarity: AchievementRarity::Epic,
                hidden: false,
            },

            // Watch Time Achievements
            Achievement {
                id: "lurker".to_string(),
                name: "Dedicated Lurker".to_string(),
                description: "Watch for 60 minutes".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::WatchTime(60),
                reward_points: 100,
                badge_emoji: "👁️".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "devoted_viewer".to_string(),
                name: "Devoted Viewer".to_string(),
                description: "Watch for 10 hours".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::WatchTime(600),
                reward_points: 500,
                badge_emoji: "📺".to_string(),
                rarity: AchievementRarity::Uncommon,
                hidden: false,
            },
            Achievement {
                id: "marathon_watcher".to_string(),
                name: "Marathon Watcher".to_string(),
                description: "Watch for 100 hours".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::WatchTime(6000),
                reward_points: 2500,
                badge_emoji: "🏃".to_string(),
                rarity: AchievementRarity::Rare,
                hidden: false,
            },

            // Points Achievements
            Achievement {
                id: "point_collector".to_string(),
                name: "Point Collector".to_string(),
                description: "Earn 1,000 total points".to_string(),
                category: AchievementCategory::Points,
                requirement: AchievementRequirement::PointsEarned(1000),
                reward_points: 200,
                badge_emoji: "💰".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "point_hoarder".to_string(),
                name: "Point Hoarder".to_string(),
                description: "Have 5,000 points at once".to_string(),
                category: AchievementCategory::Points,
                requirement: AchievementRequirement::PointsBalance(5000),
                reward_points: 1000,
                badge_emoji: "💎".to_string(),
                rarity: AchievementRarity::Rare,
                hidden: false,
            },
            Achievement {
                id: "millionaire".to_string(),
                name: "Point Millionaire".to_string(),
                description: "Earn 1,000,000 total points".to_string(),
                category: AchievementCategory::Points,
                requirement: AchievementRequirement::PointsEarned(1000000),
                reward_points: 50000,
                badge_emoji: "🏆".to_string(),
                rarity: AchievementRarity::Legendary,
                hidden: false,
            },

            // Command Usage
            Achievement {
                id: "command_curious".to_string(),
                name: "Command Curious".to_string(),
                description: "Use 50 commands".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::CommandsUsed(50),
                reward_points: 150,
                badge_emoji: "⚡".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "power_user".to_string(),
                name: "Power User".to_string(),
                description: "Use 500 commands".to_string(),
                category: AchievementCategory::Engagement,
                requirement: AchievementRequirement::CommandsUsed(500),
                reward_points: 750,
                badge_emoji: "🔧".to_string(),
                rarity: AchievementRarity::Uncommon,
                hidden: false,
            },

            // Loyalty Achievements
            Achievement {
                id: "week_warrior".to_string(),
                name: "Week Warrior".to_string(),
                description: "Be active for 7 days".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::DaysActive(7),
                reward_points: 300,
                badge_emoji: "📅".to_string(),
                rarity: AchievementRarity::Common,
                hidden: false,
            },
            Achievement {
                id: "monthly_regular".to_string(),
                name: "Monthly Regular".to_string(),
                description: "Be active for 30 days".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::DaysActive(30),
                reward_points: 1500,
                badge_emoji: "🗓️".to_string(),
                rarity: AchievementRarity::Uncommon,
                hidden: false,
            },
            Achievement {
                id: "veteran_member".to_string(),
                name: "Veteran Member".to_string(),
                description: "Be active for 365 days".to_string(),
                category: AchievementCategory::Loyalty,
                requirement: AchievementRequirement::DaysActive(365),
                reward_points: 10000,
                badge_emoji: "🎖️".to_string(),
                rarity: AchievementRarity::Epic,
                hidden: false,
            },

            // Hidden/Special Achievements
            Achievement {
                id: "early_bird".to_string(),
                name: "Early Bird".to_string(),
                description: "One of the first 100 users".to_string(),
                category: AchievementCategory::Special,
                requirement: AchievementRequirement::Custom("early_adopter".to_string()),
                reward_points: 2000,
                badge_emoji: "🐦".to_string(),
                rarity: AchievementRarity::Rare,
                hidden: true,
            },
            Achievement {
                id: "bot_whisperer".to_string(),
                name: "Bot Whisperer".to_string(),
                description: "Discover a hidden command".to_string(),
                category: AchievementCategory::Special,
                requirement: AchievementRequirement::Custom("secret_command".to_string()),
                reward_points: 500,
                badge_emoji: "🤖".to_string(),
                rarity: AchievementRarity::Rare,
                hidden: true,
            },
        ]
    }

    /// Get achievement statistics
    pub async fn get_statistics(&self) -> HashMap<String, serde_json::Value> {
        let achievements = self.achievements.read().await;
        let user_achievements = self.user_achievements.read().await;
        
        let total_achievements = achievements.len();
        let total_users_with_achievements = user_achievements.len();
        let total_unlocks = user_achievements.values()
            .map(|ua| ua.unlocked.len())
            .sum::<usize>();
        
        let rarity_counts = achievements.values()
            .fold(HashMap::new(), |mut acc, a| {
                let rarity_str = format!("{:?}", a.rarity);
                *acc.entry(rarity_str).or_insert(0) += 1;
                acc
            });
        
        let mut stats = HashMap::new();
        stats.insert("total_achievements".to_string(), serde_json::Value::Number(total_achievements.into()));
        stats.insert("total_users_with_achievements".to_string(), serde_json::Value::Number(total_users_with_achievements.into()));
        stats.insert("total_unlocks".to_string(), serde_json::Value::Number(total_unlocks.into()));
        stats.insert("rarity_distribution".to_string(), serde_json::to_value(rarity_counts).unwrap_or_default());
        
        stats
    }
}