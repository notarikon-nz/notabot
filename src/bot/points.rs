use anyhow::Result;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

use crate::types::ChatMessage;

/// User points and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPoints {
    pub user_id: String,
    pub platform: String,
    pub username: String,
    pub display_name: Option<String>,
    pub points: i64,
    pub total_earned: i64,
    pub total_spent: i64,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub first_seen: chrono::DateTime<chrono::Utc>,
    pub minutes_watched: u64,
    pub messages_sent: u64,
    pub commands_used: u64,
    pub is_subscriber: bool,
    pub is_moderator: bool,
    pub multiplier: f64, // Points multiplier (VIP, subscriber bonus, etc.)
}

impl UserPoints {
    pub fn new(platform: String, username: String, display_name: Option<String>) -> Self {
        let now = chrono::Utc::now();
        let user_id = format!("{}:{}", platform, username);
        
        Self {
            user_id,
            platform,
            username,
            display_name,
            points: 0,
            total_earned: 0,
            total_spent: 0,
            last_activity: now,
            first_seen: now,
            minutes_watched: 0,
            messages_sent: 0,
            commands_used: 0,
            is_subscriber: false,
            is_moderator: false,
            multiplier: 1.0,
        }
    }

    /// Add points with multiplier
    pub fn add_points(&mut self, amount: i64, reason: &str) {
        let adjusted_amount = (amount as f64 * self.multiplier) as i64;
        self.points += adjusted_amount;
        self.total_earned += adjusted_amount;
        self.last_activity = chrono::Utc::now();
        
        debug!("Added {} points to {} (reason: {}, multiplier: {})", 
               adjusted_amount, self.username, reason, self.multiplier);
    }

    /// Spend points (returns true if successful)
    pub fn spend_points(&mut self, amount: i64, reason: &str) -> bool {
        if self.points >= amount {
            self.points -= amount;
            self.total_spent += amount;
            self.last_activity = chrono::Utc::now();
            
            debug!("Spent {} points for {} (reason: {})", amount, self.username, reason);
            true
        } else {
            debug!("Insufficient points for {}: has {}, needs {}", 
                   self.username, self.points, amount);
            false
        }
    }

    /// Update user status from chat message
    pub fn update_from_message(&mut self, message: &ChatMessage) {
        self.last_activity = message.timestamp;
        self.messages_sent += 1;
        self.is_subscriber = message.is_subscriber;
        self.is_moderator = message.is_mod;
        
        if let Some(ref display) = message.display_name {
            self.display_name = Some(display.clone());
        }

        // Update multiplier based on status
        self.update_multiplier();
    }

    /// Update points multiplier based on user status
    fn update_multiplier(&mut self) {
        self.multiplier = 1.0;
        
        if self.is_subscriber {
            self.multiplier += 0.5; // 50% bonus for subscribers
        }
        
        if self.is_moderator {
            self.multiplier += 1.0; // 100% bonus for moderators
        }
        
        // VIP multiplier could be added here based on badges
        if self.total_earned > 100000 {
            self.multiplier += 0.25; // 25% bonus for long-term users
        }
    }

    /// Get user rank based on total points earned
    pub fn get_rank(&self) -> String {
        match self.total_earned {
            0..=999 => "Newcomer".to_string(),
            1000..=4999 => "Regular".to_string(),
            5000..=19999 => "Veteran".to_string(),
            20000..=49999 => "Champion".to_string(),
            50000..=99999 => "Legend".to_string(),
            _ => "Mythic".to_string(),
        }
    }
}

/// Points earning reasons and amounts
#[derive(Debug, Clone)]
pub struct PointsConfig {
    pub watching_interval_minutes: u64,
    pub points_per_interval: i64,
    pub points_per_message: i64,
    pub points_per_command: i64,
    pub daily_bonus: i64,
    pub first_time_bonus: i64,
    pub max_points_per_hour: i64,
}

impl Default for PointsConfig {
    fn default() -> Self {
        Self {
            watching_interval_minutes: 5,   // Points every 5 minutes of watching
            points_per_interval: 10,        // 10 points per interval
            points_per_message: 1,          // 1 point per message
            points_per_command: 2,          // 2 points per command
            daily_bonus: 100,              // 100 points for first activity of day
            first_time_bonus: 500,         // 500 points for new users
            max_points_per_hour: 200,      // Rate limiting
        }
    }
}

/// Transaction record for points history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointsTransaction {
    pub user_id: String,
    pub transaction_type: TransactionType,
    pub amount: i64,
    pub reason: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub balance_after: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    Earned,
    Spent,
    Bonus,
    Admin,
}

pub struct PointsSystem {
    users: Arc<RwLock<HashMap<String, UserPoints>>>,
    config: PointsConfig,
    transactions: Arc<RwLock<Vec<PointsTransaction>>>,
    watching_tracker: Arc<RwLock<HashMap<String, Instant>>>,
    hourly_earnings: Arc<RwLock<HashMap<String, (Instant, i64)>>>,
}

impl PointsSystem {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            config: PointsConfig::default(),
            transactions: Arc::new(RwLock::new(Vec::new())),
            watching_tracker: Arc::new(RwLock::new(HashMap::new())),
            hourly_earnings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_config(config: PointsConfig) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            config,
            transactions: Arc::new(RwLock::new(Vec::new())),
            watching_tracker: Arc::new(RwLock::new(HashMap::new())),
            hourly_earnings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the points system with periodic tasks
    pub async fn start(&self) -> Result<()> {
        info!("Starting user points system...");

        // Start watching time tracker
        self.start_watching_tracker().await;

        // Start daily bonus reset
        self.start_daily_reset().await;

        info!("User points system started");
        Ok(())
    }

    /// Process a chat message for points
    pub async fn process_message(&self, message: &ChatMessage) -> Result<()> {
        let user_id = format!("{}:{}", message.platform, message.username);
        
        // Check if user exists and if it's a new user
        let (is_new_user, needs_daily_bonus) = {
            let users = self.users.read().await;
            let is_new = !users.contains_key(&user_id);
            let needs_daily = if let Some(user) = users.get(&user_id) {
                let today = chrono::Utc::now().date_naive();
                user.last_activity.date_naive() < today
            } else {
                false
            };
            (is_new, needs_daily)
        };
        
        // Create new user if needed
        if is_new_user {
            let mut new_user = UserPoints::new(
                message.platform.clone(),
                message.username.clone(),
                message.display_name.clone()
            );
            new_user.add_points(self.config.first_time_bonus, "Welcome bonus");
            
            // Insert new user
            {
                let mut users = self.users.write().await;
                users.insert(user_id.clone(), new_user);
            }
            
            // Record welcome bonus transaction
            let transaction = PointsTransaction {
                user_id: user_id.clone(),
                transaction_type: TransactionType::Bonus,
                amount: self.config.first_time_bonus,
                reason: "Welcome bonus".to_string(),
                timestamp: chrono::Utc::now(),
                balance_after: self.config.first_time_bonus,
            };
            self.add_transaction(transaction).await;
            
            info!("New user {}: Welcome bonus {} points", message.username, self.config.first_time_bonus);
        }

        // Handle daily bonus first (without holding the user lock during transaction)
        if needs_daily_bonus {
            let mut users = self.users.write().await;
            if let Some(user) = users.get_mut(&user_id) {
                user.add_points(self.config.daily_bonus, "Daily bonus");
                let balance_after = user.points;
                drop(users); // Release lock before adding transaction
                
                let transaction = PointsTransaction {
                    user_id: user_id.clone(),
                    transaction_type: TransactionType::Bonus,
                    amount: self.config.daily_bonus,
                    reason: "Daily bonus".to_string(),
                    timestamp: chrono::Utc::now(),
                    balance_after,
                };
                self.add_transaction(transaction).await;
                info!("Daily bonus {} points awarded to {}", self.config.daily_bonus, message.username);
            }
        }

        // Check rate limiting
        if !self.check_hourly_limit(&user_id).await {
            debug!("User {} hit hourly points limit", message.username);
            return Ok(());
        }

        // Update user info and award message points
        {
            let mut users = self.users.write().await;
            if let Some(user) = users.get_mut(&user_id) {
                // Update user info from message
                user.update_from_message(message);
                
                // Award points for message
                if self.config.points_per_message > 0 {
                    user.add_points(self.config.points_per_message, "Chat message");
                    let balance_after = user.points;
                    self.update_hourly_earnings(&user_id, self.config.points_per_message).await;
                    
                    // Release lock before adding transaction
                    drop(users);
                    
                    let transaction = PointsTransaction {
                        user_id: user_id.clone(),
                        transaction_type: TransactionType::Earned,
                        amount: self.config.points_per_message,
                        reason: "Chat message".to_string(),
                        timestamp: chrono::Utc::now(),
                        balance_after,
                    };
                    self.add_transaction(transaction).await;
                }
            }
        }

        // Update watching tracker
        self.update_watching_time(&user_id).await;

        Ok(())
    }

    /// Process a command usage for points
    pub async fn process_command(&self, message: &ChatMessage, command: &str) -> Result<()> {
        let user_id = format!("{}:{}", message.platform, message.username);
        
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(&user_id) {
            user.commands_used += 1;
            
            if self.config.points_per_command > 0 && self.check_hourly_limit(&user_id).await {
                user.add_points(self.config.points_per_command, &format!("Command: {}", command));
                
                let transaction = PointsTransaction {
                    user_id: user.user_id.clone(),
                    transaction_type: TransactionType::Earned,
                    amount: self.config.points_per_command,
                    reason: format!("Command: {}", command),
                    timestamp: chrono::Utc::now(),
                    balance_after: user.points,
                };
                
                self.update_hourly_earnings(&user_id, self.config.points_per_command).await;
                
                drop(users);
                self.add_transaction(transaction).await;
            }
        }

        Ok(())
    }

    /// Get user points
    pub async fn get_user_points(&self, platform: &str, username: &str) -> Option<UserPoints> {
        let user_id = format!("{}:{}", platform, username);
        self.users.read().await.get(&user_id).cloned()
    }

    /// Add points to user (admin function)
    pub async fn add_points(&self, platform: &str, username: &str, amount: i64, reason: &str) -> Result<bool> {
        let user_id = format!("{}:{}", platform, username);
        let mut users = self.users.write().await;
        
        if let Some(user) = users.get_mut(&user_id) {
            user.add_points(amount, reason);
            
            let transaction = PointsTransaction {
                user_id: user.user_id.clone(),
                transaction_type: TransactionType::Admin,
                amount,
                reason: reason.to_string(),
                timestamp: chrono::Utc::now(),
                balance_after: user.points,
            };
            
            info!("Admin added {} points to {}: {}", amount, username, reason);
            drop(users);
            self.add_transaction(transaction).await;
            Ok(true)
        } else {
            warn!("Attempted to add points to non-existent user: {}", username);
            Ok(false)
        }
    }

    /// Spend points for user
    pub async fn spend_points(&self, platform: &str, username: &str, amount: i64, reason: &str) -> Result<bool> {
        let user_id = format!("{}:{}", platform, username);
        let mut users = self.users.write().await;
        
        if let Some(user) = users.get_mut(&user_id) {
            if user.spend_points(amount, reason) {
                let transaction = PointsTransaction {
                    user_id: user.user_id.clone(),
                    transaction_type: TransactionType::Spent,
                    amount,
                    reason: reason.to_string(),
                    timestamp: chrono::Utc::now(),
                    balance_after: user.points,
                };
                
                drop(users);
                self.add_transaction(transaction).await;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Get leaderboard
    pub async fn get_leaderboard(&self, limit: usize) -> Vec<UserPoints> {
        let users = self.users.read().await;
        let mut sorted_users: Vec<UserPoints> = users.values().cloned().collect();
        
        sorted_users.sort_by(|a, b| b.points.cmp(&a.points));
        sorted_users.truncate(limit);
        sorted_users
    }

    /// Get user statistics
    pub async fn get_statistics(&self) -> HashMap<String, serde_json::Value> {
        let users = self.users.read().await;
        let transactions = self.transactions.read().await;
        
        let total_users = users.len();
        let total_points_circulating = users.values().map(|u| u.points).sum::<i64>();
        let total_points_earned = users.values().map(|u| u.total_earned).sum::<i64>();
        let total_points_spent = users.values().map(|u| u.total_spent).sum::<i64>();
        let active_users_24h = users.values()
            .filter(|u| u.last_activity > chrono::Utc::now() - chrono::Duration::hours(24))
            .count();

        let mut stats = HashMap::new();
        stats.insert("total_users".to_string(), serde_json::Value::Number(total_users.into()));
        stats.insert("total_points_circulating".to_string(), serde_json::Value::Number(total_points_circulating.into()));
        stats.insert("total_points_earned".to_string(), serde_json::Value::Number(total_points_earned.into()));
        stats.insert("total_points_spent".to_string(), serde_json::Value::Number(total_points_spent.into()));
        stats.insert("active_users_24h".to_string(), serde_json::Value::Number(active_users_24h.into()));
        stats.insert("total_transactions".to_string(), serde_json::Value::Number(transactions.len().into()));

        stats
    }

    /// Transfer points between users
    pub async fn transfer_points(&self, from_platform: &str, from_user: &str, 
                                to_platform: &str, to_user: &str, amount: i64) -> Result<bool> {
        let from_id = format!("{}:{}", from_platform, from_user);
        let to_id = format!("{}:{}", to_platform, to_user);
        
        let mut users = self.users.write().await;
        
        // Check if both users exist and from_user has enough points
        let can_transfer = if let (Some(from), Some(_to)) = (users.get(&from_id), users.get(&to_id)) {
            from.points >= amount
        } else {
            false
        };
        
        if can_transfer {
            // Perform transfer
            let mut transactions = Vec::new();
            
            if let Some(from) = users.get_mut(&from_id) {
                from.spend_points(amount, &format!("Transfer to {}", to_user));
                transactions.push(PointsTransaction {
                    user_id: from.user_id.clone(),
                    transaction_type: TransactionType::Spent,
                    amount,
                    reason: format!("Transfer to {}", to_user),
                    timestamp: chrono::Utc::now(),
                    balance_after: from.points,
                });
            }
            
            if let Some(to) = users.get_mut(&to_id) {
                to.add_points(amount, &format!("Transfer from {}", from_user));
                transactions.push(PointsTransaction {
                    user_id: to.user_id.clone(),
                    transaction_type: TransactionType::Earned,
                    amount,
                    reason: format!("Transfer from {}", from_user),
                    timestamp: chrono::Utc::now(),
                    balance_after: to.points,
                });
            }
            
            drop(users);
            
            // Record transactions
            for transaction in transactions {
                self.add_transaction(transaction).await;
            }
            
            info!("Transferred {} points from {} to {}", amount, from_user, to_user);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Internal helper methods
    async fn add_transaction(&self, transaction: PointsTransaction) {
        let mut transactions = self.transactions.write().await;
        transactions.push(transaction);
        
        // Keep only last 10000 transactions to prevent memory bloat
        let len = transactions.len();
        if len > 10000 {
            transactions.drain(0..len - 10000);
        }
    }

    async fn check_hourly_limit(&self, user_id: &str) -> bool {
        let mut hourly = self.hourly_earnings.write().await;
        let now = Instant::now();
        
        match hourly.get_mut(user_id) {
            Some((last_reset, earned)) => {
                if now.duration_since(*last_reset) >= Duration::from_secs(3600) {
                    *last_reset = now;
                    *earned = 0;
                }
                *earned < self.config.max_points_per_hour
            }
            None => {
                hourly.insert(user_id.to_string(), (now, 0));
                true
            }
        }
    }

    async fn update_hourly_earnings(&self, user_id: &str, amount: i64) {
        let mut hourly = self.hourly_earnings.write().await;
        if let Some((_, earned)) = hourly.get_mut(user_id) {
            *earned += amount;
        }
    }

    async fn check_daily_bonus(&self, user: &mut UserPoints) {
        let now = chrono::Utc::now();
        let last_activity_date = user.last_activity.date_naive();
        let today = now.date_naive();
        
        if last_activity_date < today {
            user.add_points(self.config.daily_bonus, "Daily bonus");
            info!("Daily bonus {} points awarded to {}", self.config.daily_bonus, user.username);
            
            // Note: Transaction recording is handled in the calling function
            // to avoid borrow checker issues
        }
    }

    async fn update_watching_time(&self, user_id: &str) {
        let mut watching = self.watching_tracker.write().await;
        watching.insert(user_id.to_string(), Instant::now());
    }

    async fn start_watching_tracker(&self) {
        let users = Arc::clone(&self.users);
        let watching = Arc::clone(&self.watching_tracker);
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let watching_guard = watching.read().await;
                let mut users_guard = users.write().await;
                
                for (user_id, last_seen) in watching_guard.iter() {
                    if now.duration_since(*last_seen) <= Duration::from_secs(config.watching_interval_minutes * 60) {
                        if let Some(user) = users_guard.get_mut(user_id) {
                            user.minutes_watched += 1;
                            
                            // Award watching points every interval
                            if user.minutes_watched % config.watching_interval_minutes == 0 {
                                user.add_points(config.points_per_interval, "Watching stream");
                                debug!("Watching bonus {} points for {}", config.points_per_interval, user.username);
                            }
                        }
                    }
                }
            }
        });
    }

    async fn start_daily_reset(&self) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Check every hour
            
            loop {
                interval.tick().await;
                // Daily reset logic can be added here if needed
            }
        });
    }
}