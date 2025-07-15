//! # Extensible Chat Bot Framework
//! 
//! A high-performance, memory-efficient chat bot framework written in Rust,
//! designed for real-time streaming platforms with advanced moderation capabilities.
//! 
//! ## Features
//! 
//! - **Multi-Platform Support**: Extensible architecture supporting Twitch, YouTube, and more
//! - **Real-time Command Processing**: Instant response with variable substitution
//! - **Advanced Spam Protection**: Intelligent filters with configurable actions
//! - **Timer System**: Automated periodic messages with targeting
//! - **Web Dashboard**: Real-time analytics and management interface
//! - **High Performance**: Rust's performance with minimal memory footprint
//! 
//! ## Quick Start
//! 
//! ```rust,no_run
//! use notabot::prelude::*;
//! 
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut bot = ChatBot::new();
//!     
//!     // Configure platform
//!     let config = TwitchConfig::from_env()?;
//!     bot.add_connection(Box::new(TwitchConnection::new(config))).await;
//!     
//!     // Add commands and filters
//!     bot.add_command("hello", "Hello $(user)!", false, 5).await;
//!     bot.add_spam_filter(SpamFilterType::ExcessiveCaps { max_percentage: 70 }).await?;
//!     
//!     // Start the bot
//!     bot.start().await?;
//!     
//!     // Start web dashboard
//!     bot.start_web_dashboard(3000).await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod types;
pub mod platforms;
pub mod bot;

#[cfg(feature = "web")]
pub mod web;

// Re-export commonly used items
pub mod prelude {
    pub use crate::bot::ChatBot;
    pub use crate::platforms::{
        PlatformConnection, 
        twitch::{TwitchConnection, TwitchConfig},
        youtube::{YouTubeConnection, YouTubeConfig},
    };
    pub use crate::types::{
        ChatMessage, BotCommand, BotTimer, SpamFilterType, SpamFilter, 
        ModerationAction, UserMessageHistory
    };
    #[cfg(feature = "web")]
    pub use crate::web::{WebDashboard, DashboardState};
    pub use anyhow::Result;
}

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");