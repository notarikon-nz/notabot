use anyhow::{Result};
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

    // Add Twitch connection
    if let Ok(twitch_config) = TwitchConfig::from_env() {
        let twitch_connection = TwitchConnection::new(twitch_config);
        bot.add_connection(Box::new(twitch_connection)).await;
        info!("✅ Twitch connection configured");
    } else {
        warn!("⚠️ Twitch configuration not found, skipping Twitch integration");
    }

    // Add YouTube connection (optional)
    if let Ok(youtube_config) = YouTubeConfig::from_env() {
        let youtube_connection = YouTubeConnection::new(youtube_config);
        bot.add_connection(Box::new(youtube_connection)).await;
        info!("✅ YouTube Live Chat connection configured");
    } else {
        info!("ℹ️ YouTube configuration not found, skipping YouTube integration");
        info!("   To enable YouTube: Set YOUTUBE_API_KEY and YOUTUBE_LIVE_CHAT_ID");
    }

    // Register basic commands with argument support (work on all platforms)
    bot.add_command("hello".to_string(), "Hello there, $(user)! 👋 Welcome to $(platform)!".to_string(), false, 5).await;
    bot.add_command("uptime".to_string(), "Bot has been running smoothly on $(platform)!".to_string(), false, 30).await;
    bot.add_command("modonly".to_string(), "This command is for moderators only, $(displayname)!".to_string(), true, 0).await;
    
    // Commands with argument support
    bot.add_command("echo".to_string(), "$(user) said: $(args)".to_string(), false, 10).await;
    bot.add_command("greet".to_string(), "Hello $(1)! Welcome to $(channel) on $(platform)!".to_string(), false, 5).await;
    
    // Platform-specific commands
    bot.add_command("subscribe".to_string(), 
        "Don't forget to subscribe! $(if:youtube)🔔 Ring the bell!$(endif)$(if:twitch)💜 Follow for updates!$(endif)".to_string(), 
        false, 30).await;
    
    bot.add_command("social".to_string(), 
        "📱 Find us: $(if:twitch)Twitch.tv/YourChannel$(endif)$(if:youtube)YouTube.com/YourChannel$(endif)".to_string(), 
        false, 60).await;
    
    // Points system commands (automatically handled by PointsCommands)
    bot.add_command("points".to_string(), "Check your points! Usage: !points [username]".to_string(), false, 5).await;
    bot.add_command("balance".to_string(), "Check your point balance! Same as !points".to_string(), false, 5).await;
    bot.add_command("leaderboard".to_string(), "View the points leaderboard! Usage: !leaderboard [number]".to_string(), false, 30).await;
    bot.add_command("top".to_string(), "View top users by points! Same as !leaderboard".to_string(), false, 30).await;
    bot.add_command("rank".to_string(), "Check your rank and stats!".to_string(), false, 10).await;
    bot.add_command("give".to_string(), "Transfer points to another user! Usage: !give <user> <amount>".to_string(), false, 60).await;
    bot.add_command("transfer".to_string(), "Transfer points to another user! Same as !give".to_string(), false, 60).await;
    
    // Achievement system commands (automatically handled by AchievementCommands)
    bot.add_command("achievements".to_string(), "View your achievements! Usage: !achievements [username]".to_string(), false, 10).await;
    bot.add_command("achieve".to_string(), "View your achievements! Same as !achievements".to_string(), false, 10).await;
    bot.add_command("achievement".to_string(), "View specific achievement details! Usage: !achievement <name>".to_string(), false, 10).await;
    bot.add_command("progress".to_string(), "Check your achievement progress!".to_string(), false, 15).await;
    bot.add_command("achievementleaderboard".to_string(), "View top achievement hunters!".to_string(), false, 30).await;
    bot.add_command("achievetop".to_string(), "View top achievement hunters! Same as !achievementleaderboard".to_string(), false, 30).await;
    
    // Management commands for moderators
    bot.add_command("timers".to_string(), "Active timers: cross-platform, platform-specific messages running".to_string(), true, 10).await;
    bot.add_command("stats".to_string(), "📊 Multi-platform bot analytics at http://localhost:3000".to_string(), true, 30).await;
    bot.add_command("platforms".to_string(), "Connected platforms: Check dashboard for status".to_string(), true, 30).await;
    bot.add_command("addpoints".to_string(), "Add points to user (mod only): !addpoints <user> <amount> [reason]".to_string(), true, 5).await;
    bot.add_command("pointstats".to_string(), "View points system statistics (mod only)".to_string(), true, 30).await;
    bot.add_command("achievementstats".to_string(), "View achievement system statistics (mod only)".to_string(), true, 30).await;
    bot.add_command("transfer".to_string(), "Transfer points to another user! Same as !give".to_string(), false, 60).await;
    
    // Management commands for moderators
    bot.add_command("timers".to_string(), "Active timers: cross-platform, platform-specific messages running".to_string(), true, 10).await;
    bot.add_command("stats".to_string(), "📊 Multi-platform bot analytics at http://localhost:3000".to_string(), true, 30).await;
    bot.add_command("platforms".to_string(), "Connected platforms: Check dashboard for status".to_string(), true, 30).await;
    bot.add_command("addpoints".to_string(), "Add points to user (mod only): !addpoints <user> <amount> [reason]".to_string(), true, 5).await;
    bot.add_command("pointstats".to_string(), "View points system statistics (mod only)".to_string(), true, 30).await;

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

    // Register cross-platform timers
    bot.add_timer("cross_platform_social".to_string(), 
        "Follow us everywhere! We're multi-platform!".to_string(), 
        600 // Every 10 minutes
    ).await?;
    
    bot.add_timer("general_reminder".to_string(), 
        "Thanks for watching! Enjoying the stream? Let us know in chat!".to_string(), 
        900 // Every 15 minutes
    ).await?;
    
    bot.add_timer("engagement".to_string(), 
        "💬 Chat is active on $(platform)! Keep the conversation going!".to_string(), 
        1800 // Every 30 minutes
    ).await?;

    // Platform-specific timers
    bot.add_timer_advanced(
        "twitch_exclusive".to_string(),
        "Twitch exclusive: Type !discord for our community server!".to_string(),
        1200, // Every 20 minutes
        vec![], // All channels
        vec!["twitch".to_string()] // Twitch only
    ).await?;

    bot.add_timer_advanced(
        "youtube_exclusive".to_string(),
        "YouTube exclusive: Hit subscribe and the bell for notifications! 🔔".to_string(),
        1200, // Every 20 minutes
        vec![], // All channels
        vec!["youtube".to_string()] // YouTube only
    ).await?;
    
    bot.add_timer_advanced(
        "special_announcement".to_string(),
        "⭐ Special stream event happening soon! Don't miss it!".to_string(),
        3600, // Every hour
        vec![], // All channels
        vec!["twitch".to_string(), "youtube".to_string()] // Both platforms
    ).await?;

    info!("Configured {} cross-platform and platform-specific timers", 8);

    // Check if web feature is enabled
    #[cfg(feature = "web")]
    info!("Web dashboard feature is ENABLED");
    
    #[cfg(not(feature = "web"))]
    info!("Web dashboard feature is DISABLED");

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
                    info!("📊 Bot Statistics: {}", serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "Failed to serialize".to_string()));
                }
                Err(e) => {
                    error!("Failed to get bot stats: {}", e);
                }
            }
            
            // Log timer stats
            let timer_stats = bot.get_timer_stats().await;
            info!("⏰ Timer Status: {} active timers", timer_stats.len());
            for (name, (enabled, count, last_triggered)) in timer_stats {
                let status = if enabled { "✅" } else { "❌" };
                let last = last_triggered
                    .map(|t| format!("{} ago", chrono::Utc::now().signed_duration_since(t).num_minutes()))
                    .unwrap_or_else(|| "never".to_string());
                info!("  {} {} - {} executions, last: {}", status, name, count, last);
            }
        }
        
        // Simple keep-alive message
        if stats_counter == 0 {
            info!("🚀 Bot running smoothly...");
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

