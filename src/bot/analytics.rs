use log::info;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::ChatMessage;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserStats {
    pub total_messages: u64,
    pub command_usage: u64,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub is_regular: bool,
    pub activity_score: f64,
    pub first_seen: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandStats {
    pub usage_count: u64,
    pub last_used: chrono::DateTime<chrono::Utc>,
    pub unique_users: Vec<String>, // Changed from HashSet for serialization
    pub cooldown_hits: u64, // How many times users hit cooldown
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelStats {
    pub total_messages: u64,
    pub unique_users: Vec<String>, // Changed from HashSet for serialization
    pub commands_executed: u64,
    pub spam_messages_blocked: u64,
}

pub struct AnalyticsSystem {
    user_stats: Arc<RwLock<HashMap<String, UserStats>>>,
    command_stats: Arc<RwLock<HashMap<String, CommandStats>>>,
    channel_stats: Arc<RwLock<HashMap<String, ChannelStats>>>,
    start_time: chrono::DateTime<chrono::Utc>,
    analytics_receiver: Option<tokio::sync::mpsc::Receiver<AnalyticsEvent>>,
    analytics_sender: Arc<tokio::sync::mpsc::Sender<AnalyticsEvent>>,
}

#[derive(Debug, Clone)]
pub enum AnalyticsEvent {
    MessageReceived(ChatMessage),
    CommandExecuted { command: String, user: String, channel: String },
    SpamBlocked(ChatMessage),
    CooldownHit { command: String, user: String },
}

impl AnalyticsSystem {
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);
        
        Self {
            user_stats: Arc::new(RwLock::new(HashMap::new())),
            command_stats: Arc::new(RwLock::new(HashMap::new())),
            channel_stats: Arc::new(RwLock::new(HashMap::new())),
            start_time: chrono::Utc::now(),
            analytics_receiver: Some(receiver),
            analytics_sender: Arc::new(sender),
        }
    }

    /// Get a sender for analytics events
    pub fn get_sender(&self) -> Arc<tokio::sync::mpsc::Sender<AnalyticsEvent>> {
        Arc::clone(&self.analytics_sender)
    }

    /// Start the analytics processing loop
    pub async fn start_analytics_processor(&mut self) {
        if let Some(mut receiver) = self.analytics_receiver.take() {
            let user_stats = Arc::clone(&self.user_stats);
            let command_stats = Arc::clone(&self.command_stats);
            let channel_stats = Arc::clone(&self.channel_stats);
            let start_time = self.start_time;

            tokio::spawn(async move {
                info!("Analytics processor started");
                
                while let Some(event) = receiver.recv().await {
                    match event {
                        AnalyticsEvent::MessageReceived(message) => {
                            Self::process_message_event(&user_stats, &channel_stats, &message, start_time).await;
                        }
                        AnalyticsEvent::CommandExecuted { command, user, channel } => {
                            Self::process_command_event(&command_stats, &user_stats, &command, &user, &channel).await;
                        }
                        AnalyticsEvent::SpamBlocked(message) => {
                            Self::process_spam_event(&channel_stats, &message).await;
                        }
                        AnalyticsEvent::CooldownHit { command, user: _ } => {
                            Self::process_cooldown_event(&command_stats, &command).await;
                        }
                    }
                }
                
                info!("Analytics processor stopped");
            });
        }
    }

    /// Record a message for analytics (convenience method)
    pub async fn record_message(&self, message: &ChatMessage) {
        let _ = self.analytics_sender.send(AnalyticsEvent::MessageReceived(message.clone())).await;
    }

    /// Record command execution (convenience method)
    pub async fn record_command_usage(&self, command: &str, user: &str, channel: &str) {
        let _ = self.analytics_sender.send(AnalyticsEvent::CommandExecuted {
            command: command.to_string(),
            user: user.to_string(),
            channel: channel.to_string(),
        }).await;
    }

    /// Record spam message blocked (convenience method)
    pub async fn record_spam_blocked(&self, message: &ChatMessage) {
        let _ = self.analytics_sender.send(AnalyticsEvent::SpamBlocked(message.clone())).await;
    }

    /// Record cooldown hit (convenience method)
    pub async fn record_cooldown_hit(&self, command: &str, user: &str) {
        let _ = self.analytics_sender.send(AnalyticsEvent::CooldownHit {
            command: command.to_string(),
            user: user.to_string(),
        }).await;
    }

    // Internal event processors
    async fn process_message_event(
        user_stats: &Arc<RwLock<HashMap<String, UserStats>>>,
        channel_stats: &Arc<RwLock<HashMap<String, ChannelStats>>>,
        message: &ChatMessage,
        start_time: chrono::DateTime<chrono::Utc>,
    ) {
        // Update user stats
        {
            let mut user_stats_guard = user_stats.write().await;
            let user_key = format!("{}:{}", message.platform, message.username);
            
            let stats = user_stats_guard.entry(user_key).or_insert(UserStats {
                total_messages: 0,
                command_usage: 0,
                last_seen: message.timestamp,
                is_regular: false,
                activity_score: 0.0,
                first_seen: message.timestamp,
            });
            
            stats.total_messages += 1;
            stats.last_seen = message.timestamp;
            
            // Update activity score (messages per hour since start)
            let hours_since_start = chrono::Utc::now()
                .signed_duration_since(start_time)
                .num_hours().max(1) as f64;
            stats.activity_score = stats.total_messages as f64 / hours_since_start;
            
            // Mark as regular if they have high activity (configurable threshold)
            stats.is_regular = stats.activity_score > 2.0 && stats.total_messages > 20;
        }

        // Update channel stats
        {
            let mut channel_stats_guard = channel_stats.write().await;
            let channel_key = format!("{}:{}", message.platform, message.channel);
            
            let stats = channel_stats_guard.entry(channel_key).or_insert(ChannelStats {
                total_messages: 0,
                unique_users: Vec::new(), // Changed to Vec
                commands_executed: 0,
                spam_messages_blocked: 0,
            });
            
            stats.total_messages += 1;
            let user_key = format!("{}:{}", message.platform, message.username);
            if !stats.unique_users.contains(&user_key) {
                stats.unique_users.push(user_key);
            }
        }
    }

    async fn process_command_event(
        command_stats: &Arc<RwLock<HashMap<String, CommandStats>>>,
        user_stats: &Arc<RwLock<HashMap<String, UserStats>>>,
        command: &str,
        user: &str,
        channel: &str,
    ) {
        // Update command stats
        {
            let mut command_stats_guard = command_stats.write().await;
            let stats = command_stats_guard.entry(command.to_string()).or_insert(CommandStats {
                usage_count: 0,
                last_used: chrono::Utc::now(),
                unique_users: Vec::new(), // Changed to Vec
                cooldown_hits: 0,
            });
            
            stats.usage_count += 1;
            stats.last_used = chrono::Utc::now();
            if !stats.unique_users.contains(&user.to_string()) {
                stats.unique_users.push(user.to_string());
            }
        }

        // Update user command usage
        {
            let mut user_stats_guard = user_stats.write().await;
            let user_key = format!("{}:{}", "platform", user); // Note: we'd need platform info
            if let Some(stats) = user_stats_guard.get_mut(&user_key) {
                stats.command_usage += 1;
            }
        }

        info!("Recorded command usage: {} by {} in {}", command, user, channel);
    }

    async fn process_spam_event(
        channel_stats: &Arc<RwLock<HashMap<String, ChannelStats>>>,
        message: &ChatMessage,
    ) {
        let mut channel_stats_guard = channel_stats.write().await;
        let channel_key = format!("{}:{}", message.platform, message.channel);
        
        let stats = channel_stats_guard.entry(channel_key).or_insert(ChannelStats {
            total_messages: 0,
            unique_users: Vec::new(), // Changed to Vec
            commands_executed: 0,
            spam_messages_blocked: 0,
        });
        
        stats.spam_messages_blocked += 1;
        info!("Recorded spam blocked from {} in {}", message.username, message.channel);
    }

    async fn process_cooldown_event(
        command_stats: &Arc<RwLock<HashMap<String, CommandStats>>>,
        command: &str,
    ) {
        let mut command_stats_guard = command_stats.write().await;
        if let Some(stats) = command_stats_guard.get_mut(command) {
            stats.cooldown_hits += 1;
        }
    }

    /// Get comprehensive analytics data
    pub async fn get_analytics(&self) -> HashMap<String, serde_json::Value> {
        let mut analytics = HashMap::new();
        
        // Overall stats
        let uptime_hours = chrono::Utc::now()
            .signed_duration_since(self.start_time)
            .num_hours();
        
        analytics.insert("uptime_hours".to_string(), Value::Number(uptime_hours.into()));
        analytics.insert("start_time".to_string(), Value::String(self.start_time.to_rfc3339()));
        
        // User stats summary
        let user_stats = self.user_stats.read().await;
        let total_users = user_stats.len();
        let regular_users = user_stats.values().filter(|s| s.is_regular).count();
        let total_messages = user_stats.values().map(|s| s.total_messages).sum::<u64>();
        
        analytics.insert("total_users".to_string(), Value::Number(total_users.into()));
        analytics.insert("regular_users".to_string(), Value::Number(regular_users.into()));
        analytics.insert("total_messages".to_string(), Value::Number(total_messages.into()));
        
        // Command stats summary
        let command_stats = self.command_stats.read().await;
        let total_commands_used = command_stats.values().map(|s| s.usage_count).sum::<u64>();
        let most_popular_command = command_stats
            .iter()
            .max_by_key(|(_, stats)| stats.usage_count)
            .map(|(cmd, _)| cmd.clone())
            .unwrap_or_else(|| "none".to_string());
        
        analytics.insert("total_commands_used".to_string(), Value::Number(total_commands_used.into()));
        analytics.insert("most_popular_command".to_string(), Value::String(most_popular_command));
        
        // Channel stats summary
        let channel_stats = self.channel_stats.read().await;
        let total_spam_blocked = channel_stats.values().map(|s| s.spam_messages_blocked).sum::<u64>();
        
        analytics.insert("total_spam_blocked".to_string(), Value::Number(total_spam_blocked.into()));
        
        // Top users by activity
        let mut top_users: Vec<_> = user_stats
            .iter()
            .map(|(user, stats)| (user.clone(), stats.activity_score))
            .collect();
        top_users.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_users.truncate(10);
        
        let top_users_json: Vec<Value> = top_users
            .into_iter()
            .map(|(user, score)| {
                let mut obj = Map::new();
                obj.insert("user".to_string(), Value::String(user));
                obj.insert("activity_score".to_string(), Value::Number(serde_json::Number::from_f64(score).unwrap_or_else(|| 0.into())));
                Value::Object(obj)
            })
            .collect();
        
        analytics.insert("top_users".to_string(), Value::Array(top_users_json));
        
        analytics
    }

    /// Get user stats for a specific user
    pub async fn get_user_stats(&self, platform: &str, username: &str) -> Option<UserStats> {
        let user_key = format!("{}:{}", platform, username);
        self.user_stats.read().await.get(&user_key).cloned()
    }

    /// Get command statistics
    pub async fn get_command_stats(&self) -> HashMap<String, CommandStats> {
        self.command_stats.read().await.clone()
    }

    /// Reset analytics (useful for testing or periodic resets)
    pub async fn reset_analytics(&self) {
        self.user_stats.write().await.clear();
        self.command_stats.write().await.clear();
        self.channel_stats.write().await.clear();
        info!("Analytics reset");
    }
}