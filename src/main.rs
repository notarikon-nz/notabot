// This should be your Cargo.toml file:
/*
[package]
name = "notabot"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
tokio-tungstenite = "0.20"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
log = "0.4"
env_logger = "0.10"
anyhow = "1.0"
async-trait = "0.1"
url = "2.4"
chrono = { version = "0.4", features = ["serde"] }
futures-util = "0.3"
dotenv = "0.15"
*/

mod types;
mod platforms;
mod bot;

use anyhow::{Context, Result};
use log::{debug, error, info};
use tokio::time::{sleep, Duration};

use bot::ChatBot;
use platforms::twitch::{TwitchConnection, TwitchConfig};
use types::SpamFilterType;

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

    // Register some basic commands
    bot.add_command("hello".to_string(), "Hello there, $(user)! 👋".to_string(), false, 5).await;
    bot.add_command("uptime".to_string(), "Bot has been running in $(channel) for a while!".to_string(), false, 30).await;
    bot.add_command("modonly".to_string(), "This command is for moderators only, $(displayname)!".to_string(), true, 0).await;
    
    // Timer management commands
    bot.add_command("timers".to_string(), "Active timers: social (10m), subscribe (15m), uptime_reminder (30m)".to_string(), true, 10).await;

    // Configure spam protection
    bot.add_spam_filter(SpamFilterType::ExcessiveCaps { max_percentage: 70 }).await?;
    bot.add_spam_filter(SpamFilterType::LinkBlocking { 
        allow_mods: true, 
        whitelist: vec!["discord.gg".to_string(), "twitter.com".to_string()] 
    }).await?;
    bot.add_spam_filter(SpamFilterType::RepeatedMessages { max_repeats: 3, window_seconds: 300 }).await?;
    bot.add_spam_filter(SpamFilterType::MessageLength { max_length: 500 }).await?;
    bot.add_spam_filter(SpamFilterType::RateLimit { max_messages: 5, window_seconds: 30 }).await?;
    bot.add_spam_filter(SpamFilterType::SymbolSpam { max_percentage: 50 }).await?;

    info!("Configured spam protection with {} filters", 6);

    // Register periodic timers
    bot.add_timer("social".to_string(), "Follow us on Twitter @YourHandle and join our Discord! 🐦".to_string(), 600).await?; // Every 10 minutes
    bot.add_timer("subscribe".to_string(), "Don't forget to subscribe if you're enjoying the stream! 🔔".to_string(), 900).await?; // Every 15 minutes
    bot.add_timer("uptime_reminder".to_string(), "Stream has been live for a while! Thanks for watching! ❤️".to_string(), 1800).await?; // Every 30 minutes

    // Start the bot
    if let Err(e) = bot.start().await {
        error!("Failed to start bot: {}", e);
        return Err(e);
    }

    // Run health checks periodically
    let health_check_interval = Duration::from_secs(60);
    // let bot_health = bot.health_check();
    tokio::spawn(async move {
        loop {
            sleep(health_check_interval).await;
            let status = bot.health_check().await;
            debug!("Health check: {:?}", status);
        }
    });

    // Keep the main thread alive
    loop {
        sleep(Duration::from_secs(30)).await;
        info!("Bot is running...");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bot_creation() {
        let bot = ChatBot::new();
        // Basic smoke test
        assert!(true);
    }

    #[test]
    fn test_version_info() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}