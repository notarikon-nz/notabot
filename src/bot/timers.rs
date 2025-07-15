use anyhow::Result;
use log::{debug, error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration; //{Duration, sleep};

use crate::platforms::PlatformConnection;
use crate::types::BotTimer;

pub struct TimerSystem {
    pub timers: Arc<RwLock<HashMap<String, BotTimer>>>,
    timer_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TimerSystem {
    pub fn new() -> Self {
        Self {
            timers: Arc::new(RwLock::new(HashMap::new())),
            timer_handle: None,
        }
    }

    /// Add a new timer that posts messages at regular intervals
    pub async fn add_timer(&self, name: String, message: String, interval_seconds: u64) -> Result<()> {
        self.add_timer_advanced(name, message, interval_seconds, Vec::new(), Vec::new()).await
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
        if interval_seconds < 30 {
            return Err(anyhow::anyhow!("Timer interval must be at least 30 seconds to prevent spam"));
        }

        let timer = BotTimer {
            name: name.clone(),
            message,
            interval_seconds,
            channels,
            platforms,
            enabled: true,
            last_triggered: None,
            trigger_count: 0,
        };

        self.timers.write().await.insert(name.clone(), timer);
        info!("Registered timer '{}' with interval {}s", name, interval_seconds);
        Ok(())
    }

    /// Enable or disable a specific timer
    pub async fn set_timer_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut timers_guard = self.timers.write().await;
        if let Some(timer) = timers_guard.get_mut(name) {
            timer.enabled = enabled;
            info!("Timer '{}' {}", name, if enabled { "enabled" } else { "disabled" });
            Ok(())
        } else {
            Err(anyhow::anyhow!("Timer '{}' not found", name))
        }
    }

    /// Remove a timer
    pub async fn remove_timer(&self, name: &str) -> Result<()> {
        let mut timers_guard = self.timers.write().await;
        if timers_guard.remove(name).is_some() {
            info!("Removed timer '{}'", name);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Timer '{}' not found", name))
        }
    }

    /// Get statistics for all timers
    pub async fn get_timer_stats(&self) -> HashMap<String, (bool, u64, Option<chrono::DateTime<chrono::Utc>>)> {
        let timers_guard = self.timers.read().await;
        let mut stats = HashMap::new();
        
        for (name, timer) in timers_guard.iter() {
            stats.insert(
                name.clone(), 
                (timer.enabled, timer.trigger_count, timer.last_triggered)
            );
        }
        
        stats
    }

    /// Start the timer system that processes periodic messages
    pub async fn start_timer_system(
        &mut self, 
        connections: Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>
    ) -> Result<()> {
        let timers = Arc::clone(&self.timers);
        
        let handle = tokio::spawn(async move {
            info!("Timer system started");
            let mut check_interval = tokio::time::interval(Duration::from_secs(10)); // Check every 10 seconds
            
            loop {
                check_interval.tick().await;
                
                let now = chrono::Utc::now();
                let mut timers_to_trigger = Vec::new();
                
                // Check which timers need to be triggered
                {
                    let mut timers_guard = timers.write().await;
                    for (name, timer) in timers_guard.iter_mut() {
                        if !timer.enabled {
                            continue;
                        }
                        
                        let should_trigger = match timer.last_triggered {
                            Some(last) => {
                                let elapsed = now.signed_duration_since(last);
                                elapsed.num_seconds() >= timer.interval_seconds as i64
                            }
                            None => true, // First time running
                        };
                        
                        if should_trigger {
                            timer.last_triggered = Some(now);
                            timer.trigger_count += 1;
                            timers_to_trigger.push(timer.clone());
                            debug!("Timer '{}' triggered (count: {})", name, timer.trigger_count);
                        }
                    }
                }
                
                // Send timer messages
                for timer in timers_to_trigger {
                    if let Err(e) = Self::execute_timer(&timer, &connections).await {
                        error!("Failed to execute timer '{}': {}", timer.name, e);
                    }
                }
            }
        });
        
        self.timer_handle = Some(handle);
        info!("Timer system initialized");
        Ok(())
    }

    /// Execute a timer by sending its message to appropriate channels
    async fn execute_timer(
        timer: &BotTimer,
        connections: &Arc<RwLock<HashMap<String, Box<dyn PlatformConnection>>>>
    ) -> Result<()> {
        let connections_guard = connections.read().await;
        
        for (platform_name, connection) in connections_guard.iter() {
            // Check if this timer should post on this platform
            if !timer.platforms.is_empty() && !timer.platforms.contains(platform_name) {
                continue;
            }
            
            // Get channels for this platform
            let channels_to_post = if timer.channels.is_empty() {
                // Post to all channels this connection is active in
                connection.get_channels()
            } else {
                // Use specific channels defined for this timer
                timer.channels.clone()
            };
            
            for channel in channels_to_post {
                // Process message with variable substitution
                let processed_message = timer.message
                    .replace("$(timer)", &timer.name)
                    .replace("$(count)", &timer.trigger_count.to_string())
                    .replace("$(platform)", platform_name)
                    .replace("$(channel)", &channel);
                
                if let Err(e) = connection.send_message(&channel, &processed_message).await {
                    error!("Failed to send timer message to {}#{}: {}", platform_name, channel, e);
                } else {
                    info!("Timer '{}' posted to {}#{}: {}", timer.name, platform_name, channel, processed_message);
                }
            }
        }
        
        Ok(())
    }

    /// Stop the timer system
    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
            info!("Timer system stopped");
        }
    }
}