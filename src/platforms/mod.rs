use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::types::ChatMessage;

pub mod twitch;

/// Trait defining the interface all platform connections must implement
#[async_trait]
pub trait PlatformConnection: Send + Sync {
    /// Connect to the platform and start receiving messages
    async fn connect(&mut self) -> Result<()>;
    
    /// Send a message to the specified channel
    async fn send_message(&self, channel: &str, message: &str) -> Result<()>;
    
    /// Get the platform identifier (e.g., "twitch", "youtube")
    fn platform_name(&self) -> &str;
    
    /// Check if the connection is healthy
    async fn is_connected(&self) -> bool;
    
    /// Get a receiver for incoming messages
    fn get_message_receiver(&self) -> Option<broadcast::Receiver<ChatMessage>>;
    
    /// Get list of channels this connection is active in
    fn get_channels(&self) -> Vec<String>;
    
    /// Gracefully disconnect
    async fn disconnect(&mut self) -> Result<()>;
}
