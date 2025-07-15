use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use tokio::time::{sleep, Duration};
use std::env;

use notabot::prelude::*;

/// Example usage and main function
#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    dotenv::dotenv().ok();
    
    // Initialize logging from environment or default to info level
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting extensible chat bot framework v{}", env!("CARGO_PKG_VERSION"));

    // Create bot instance
    let mut bot = ChatBot::new();

    // Load Twitch configuration from environment variables
    let twitch_config = TwitchConfig::from_env()
        .context("Failed to load Twitch configuration from environment")?;

    // Add Twitch connection
    let twitch_connection = TwitchConnection::new(twitch_config);
    bot.add_connection(Box::new(twitch_connection)).await;

    // Register basic commands with argument support
    bot.add_command("hello".to_string(), "Hello there, $(user)! 👋".to_string(), false, 5).await;
    bot.add_command("uptime".to_string(), "Bot has been running in $(channel) for a while!".to_string(), false, 30).await;
    bot.add_command("modonly".to_string(), "This command is for moderators only, $(displayname)!".to_string(), true, 0).await;
    
    // Commands with argument support
    bot.add_command("echo".to_string(), "$(user) said: $(args)".to_string(), false, 10).await;
    bot.add_command("greet".to_string(), "Hello $(1)! Welcome to $(channel)!".to_string(), false, 5).await;
    
    // Management commands for moderators
    bot.add_command("timers".to_string(), "Active timers: social (10m), subscribe (15m), uptime_reminder (30m)".to_string(), true, 10).await;
    bot.add_command("stats".to_string(), "Check bot statistics and analytics!".to_string(), true, 30).await;

    // Configure comprehensive spam protection
    info!("Configuring spam protection...");
    
    bot.add_spam_filter(SpamFilterType::ExcessiveCaps { max_percentage: 70 }).await?;
    bot.add_spam_filter(SpamFilterType::LinkBlocking { 
        allow_mods: true, 
        whitelist: vec!["discord.gg".to_string(), "twitter.com".to_string(), "youtube.com".to_string()] 
    }).await?;
    
    bot.add_spam_filter_advanced(
        SpamFilterType::RepeatedMessages { max_repeats: 3, window_seconds: 300 },
        1200, // 20 minute timeout
        Some("Please don't repeat messages".to_string()),
        true,  // mods exempt
        false  // subscribers not exempt
    ).await?;
    
    bot.add_spam_filter(SpamFilterType::MessageLength { max_length: 500 }).await?;
    bot.add_spam_filter(SpamFilterType::RateLimit { max_messages: 5, window_seconds: 30 }).await?;
    bot.add_spam_filter(SpamFilterType::SymbolSpam { max_percentage: 50 }).await?;
    bot.add_spam_filter(SpamFilterType::ExcessiveEmotes { max_count: 10 }).await?;

    info!("Configured spam protection with {} filters", 7);

    // Register periodic timers with different intervals
    info!("Setting up periodic timers...");
    
    bot.add_timer("social".to_string(), 
        "Follow us on Twitter @YourHandle and join our Discord!".to_string(), 
        600 // Every 10 minutes
    ).await?;
    
    bot.add_timer("subscribe".to_string(), 
        "Don't forget to subscribe if you're enjoying the stream! 🔔".to_string(), 
        900 // Every 15 minutes
    ).await?;
    
    bot.add_timer("uptime_reminder".to_string(), 
        "Thanks for watching! Bot uptime: $(count) posts".to_string(), 
        1800 // Every 30 minutes
    ).await?;

    // Advanced timer with specific targeting
    bot.add_timer_advanced(
        "special_announcement".to_string(),
        "Special stream event happening soon! Don't miss it! ⭐".to_string(),
        3600, // Every hour
        vec![], // All channels
        vec!["twitch".to_string()] // Only on Twitch
    ).await?;

    info!("Configured {} timers", 4);

    // Check if web feature is enabled
    #[cfg(feature = "web")]
    info!("Web dashboard feature is ENABLED");
    
    #[cfg(not(feature = "web"))]
    info!("❌ Web dashboard feature is DISABLED");

    // Start the web dashboard FIRST (before bot systems)
    let dashboard_port = env::var("DASHBOARD_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    
    info!("Attempting to start web dashboard on port {}...", dashboard_port);
    
    // Test web dashboard creation
    #[cfg(feature = "web")]
    {
        info!("Testing web dashboard creation...");
        let _test_dashboard = notabot::web::WebDashboard::new();
        info!("Web dashboard test creation successful");
    }
    
    // Start the web dashboard
    if let Err(e) = bot.start_web_dashboard(dashboard_port).await {
        error!("Failed to start web dashboard: {}", e);
        warn!("Continuing without web dashboard");
    } else {
        info!("Web dashboard startup initiated successfully");
    }

    // NOW start the bot systems
    info!("Starting bot core systems...");
    if let Err(e) = bot.start().await {
        error!("Failed to start bot: {}", e);
        return Err(e);
    }

    info!("Bot started successfully! All systems operational.");
    info!("Analytics tracking enabled");
    info!("Spam protection active");
    info!("Timer system running");
    info!("Command system ready");
    info!("Web dashboard should be available at: http://localhost:{}", dashboard_port);

    // Run periodic health checks and stats logging
    let health_check_interval = Duration::from_secs(60);
    let stats_log_interval = Duration::from_secs(300); // Log stats every 5 minutes
    let mut stats_counter = 0;
    
    loop {
        sleep(health_check_interval).await;
        
        // Health check
        let status = bot.health_check().await;
        debug!("Health check: {:?}", status);
        
        // Check if any connections are unhealthy
        let unhealthy_platforms: Vec<_> = status.iter()
            .filter(|(_, &healthy)| !healthy)
            .map(|(platform, _)| platform)
            .collect();
        
        if !unhealthy_platforms.is_empty() {
            error!("Unhealthy platforms detected: {:?}", unhealthy_platforms);
        }
        
        // Log comprehensive stats every 5 minutes
        stats_counter += 1;
        if stats_counter >= 5 {
            stats_counter = 0;
            
            match bot.get_bot_stats().await {
                Ok(stats) => {
                    info!("Bot Statistics: {}", serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "Failed to serialize".to_string()));
                }
                Err(e) => {
                    error!("Failed to get bot stats: {}", e);
                }
            }
            
            // Log timer stats
            let timer_stats = bot.get_timer_stats().await;
            info!("⏰ Timer Status: {} active timers", timer_stats.len());
            for (name, (enabled, count, last_triggered)) in timer_stats {
                let status = if enabled { "+" } else { "-" };
                let last = last_triggered
                    .map(|t| format!("{} ago", chrono::Utc::now().signed_duration_since(t).num_minutes()))
                    .unwrap_or_else(|| "never".to_string());
                info!("  {} {} - {} executions, last: {}", status, name, count, last);
            }
        }
        
        // Simple keep-alive message
        if stats_counter == 0 {
            info!("Bot running smoothly...");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bot_creation() {
        let bot = ChatBot::new();
        
        // Test basic bot functionality
        assert!(bot.command_exists("nonexistent").await == false);
        
        // Test command registration
        bot.add_command("test".to_string(), "Test response".to_string(), false, 0).await;
        assert!(bot.command_exists("test").await == true);
    }

    #[tokio::test]
    async fn test_command_system() {
        let bot = ChatBot::new();
        
        // Add test commands
        bot.add_command("hello".to_string(), "Hello $(user)!".to_string(), false, 5).await;
        bot.add_command("mod".to_string(), "Mod only".to_string(), true, 0).await;
        
        let commands = bot.command_system.get_all_commands().await;
        assert!(commands.contains(&"hello".to_string()));
        assert!(commands.contains(&"mod".to_string()));
    }

    #[tokio::test]
    async fn test_timer_system() {
        let bot = ChatBot::new();
        
        // Test timer creation
        let result = bot.add_timer("test_timer".to_string(), "Test message".to_string(), 60).await;
        assert!(result.is_ok());
        
        // Test invalid timer (too short interval)
        let result = bot.add_timer("invalid".to_string(), "Test".to_string(), 10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_spam_filters() {
        let bot = ChatBot::new();
        
        // Test adding spam filters
        let result = bot.add_spam_filter(SpamFilterType::ExcessiveCaps { max_percentage: 70 }).await;
        assert!(result.is_ok());
        
        let result = bot.add_spam_filter(SpamFilterType::MessageLength { max_length: 500 }).await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "web")]
    #[tokio::test]
    async fn test_web_dashboard_creation() {
        let _dashboard = notabot::web::WebDashboard::new();
        
        // Basic smoke test for web dashboard
        assert!(true);
    }

    #[test]
    fn test_version_info() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}