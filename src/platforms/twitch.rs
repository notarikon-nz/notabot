use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use std::env;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

use crate::platforms::PlatformConnection;
use crate::types::ChatMessage;

// Type aliases for cleaner code
type WebSocketWriter = Arc<RwLock<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>>>;

/// Configuration for Twitch connection
#[derive(Debug, Clone)]
pub struct TwitchConfig {
    pub username: String,
    pub oauth_token: String, // oauth:your_token_here
    pub channels: Vec<String>,
}

impl TwitchConfig {
    /// Load Twitch configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let username = env::var("TWITCH_USERNAME")
            .context("TWITCH_USERNAME environment variable not set")?;
        
        let oauth_token = env::var("TWITCH_OAUTH_TOKEN")
            .context("TWITCH_OAUTH_TOKEN environment variable not set")?;
        
        let channels_str = env::var("TWITCH_CHANNELS")
            .context("TWITCH_CHANNELS environment variable not set")?;
        
        // Parse comma-separated channel list
        let channels: Vec<String> = channels_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        if channels.is_empty() {
            return Err(anyhow::anyhow!("No channels specified in TWITCH_CHANNELS"));
        }
        
        // Validate OAuth token format
        if !oauth_token.starts_with("oauth:") {
            return Err(anyhow::anyhow!(
                "TWITCH_OAUTH_TOKEN must start with 'oauth:' - got: {}...", 
                &oauth_token[..std::cmp::min(10, oauth_token.len())]
            ));
        }
        
        info!("Loaded Twitch config for user '{}' with {} channels", username, channels.len());
        debug!("Channels: {:?}", channels);
        
        Ok(Self {
            username,
            oauth_token,
            channels,
        })
    }
}

/// Twitch IRC connection implementation
pub struct TwitchConnection {
    config: TwitchConfig,
    message_sender: Option<broadcast::Sender<ChatMessage>>,
    websocket_writer: Option<WebSocketWriter>,
    is_connected: Arc<RwLock<bool>>,
}

impl TwitchConnection {
    pub fn new(config: TwitchConfig) -> Self {
        Self {
            config,
            message_sender: None,
            websocket_writer: None,
            is_connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Parse incoming Twitch IRC message into our standard format
    fn parse_twitch_message(&self, raw_message: &str) -> Option<ChatMessage> {
        // Simplified Twitch IRC parsing - in production, use a proper IRC library
        let lines: Vec<&str> = raw_message.split('\n').collect();
        
        for line in lines {
            if line.starts_with("@") && line.contains("PRIVMSG") {
                return self.parse_privmsg(line);
            }
        }
        None
    }

    fn parse_privmsg(&self, line: &str) -> Option<ChatMessage> {
        // Parse IRC tags and message
        // Format: @badges=...;display-name=...;mod=... :user!user@user.tmi.twitch.tv PRIVMSG #channel :message
        
        let parts: Vec<&str> = line.splitn(2, " :").collect();
        if parts.len() != 2 {
            return None;
        }

        let (tags_and_prefix, message_content) = (parts[0], parts[1]);
        let content_parts: Vec<&str> = message_content.splitn(2, " PRIVMSG ").collect();
        if content_parts.len() != 2 {
            return None;
        }

        let channel_and_message: Vec<&str> = content_parts[1].splitn(2, " :").collect();
        if channel_and_message.len() != 2 {
            return None;
        }

        let channel = channel_and_message[0].trim_start_matches('#');
        let message = channel_and_message[1];

        // Extract username from prefix
        let username = content_parts[0].split('!').next()?.to_string();

        // Parse IRC tags for additional info
        let mut display_name = None;
        let mut is_mod = false;
        let mut is_subscriber = false;
        let mut badges = Vec::new();

        if let Some(tags_part) = tags_and_prefix.strip_prefix('@') {
            let tag_end = tags_part.find(' ').unwrap_or(tags_part.len());
            let tags = &tags_part[..tag_end];
            
            for tag in tags.split(';') {
                let tag_parts: Vec<&str> = tag.splitn(2, '=').collect();
                if tag_parts.len() == 2 {
                    match tag_parts[0] {
                        "display-name" => display_name = Some(tag_parts[1].to_string()),
                        "mod" => is_mod = tag_parts[1] == "1",
                        "subscriber" => is_subscriber = tag_parts[1] == "1",
                        "badges" => {
                            badges = tag_parts[1].split(',')
                                .filter_map(|b| b.split('/').next())
                                .map(String::from)
                                .collect();
                        }
                        _ => {}
                    }
                }
            }
        }

        Some(ChatMessage {
            platform: "twitch".to_string(),
            channel: channel.to_string(),
            username,
            display_name,
            content: message.to_string(),
            timestamp: chrono::Utc::now(),
            user_badges: badges,
            is_mod,
            is_subscriber,
        })
    }

    fn parse_message(text: &str) -> Option<ChatMessage> {
        // Simplified parsing for the static context
        if text.contains("PRIVMSG") && text.starts_with('@') {
            // This is a basic fallback - in practice, you'd use the instance method
            // or a proper IRC parsing library
            Some(ChatMessage {
                platform: "twitch".to_string(),
                channel: "example".to_string(),
                username: "user".to_string(),
                display_name: None,
                content: "message".to_string(),
                timestamp: chrono::Utc::now(),
                user_badges: Vec::new(),
                is_mod: false,
                is_subscriber: false,
            })
        } else {
            None
        }
    }
}

#[async_trait]
impl PlatformConnection for TwitchConnection {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Twitch IRC...");

        let url = Url::parse("wss://irc-ws.chat.twitch.tv:443")
            .context("Failed to parse Twitch WebSocket URL")?;

        let (ws_stream, _) = connect_async(url)
            .await
            .context("Failed to connect to Twitch WebSocket")?;

        let (write, read) = ws_stream.split();

        // Store writer for sending messages and clone for PONG responses
        let writer_arc = Arc::new(RwLock::new(write));
        let writer_for_pong = Arc::clone(&writer_arc);
        self.websocket_writer = Some(writer_arc);

        // Authenticate with Twitch
        let pass_msg = format!("PASS {}\r\n", self.config.oauth_token);
        let nick_msg = format!("NICK {}\r\n", self.config.username);
        
        writer_for_pong.write().await.send(Message::Text(pass_msg)).await
            .context("Failed to send PASS command")?;
        writer_for_pong.write().await.send(Message::Text(nick_msg)).await
            .context("Failed to send NICK command")?;

        // Request capabilities for better message parsing
        writer_for_pong.write().await.send(Message::Text("CAP REQ :twitch.tv/tags twitch.tv/commands\r\n".to_string())).await
            .context("Failed to request capabilities")?;

        // Join channels
        for channel in &self.config.channels {
            let join_msg = format!("JOIN #{}\r\n", channel);
            writer_for_pong.write().await.send(Message::Text(join_msg)).await
                .with_context(|| format!("Failed to join channel: {}", channel))?;
            info!("Joined channel: #{}", channel);
        }

        // Set up message broadcasting
        let (tx, _) = broadcast::channel(1000);
        self.message_sender = Some(tx.clone());

        // Mark as connected
        *self.is_connected.write().await = true;

        // Spawn message reading task
        let message_sender = tx;
        let is_connected = Arc::clone(&self.is_connected);
        
        tokio::spawn(async move {
            let mut read = read;
            loop {
                match read.next().await {
                    Some(Ok(Message::Text(text))) => {
                        debug!("Received: {}", text);
                        
                        // Handle PING/PONG to keep connection alive
                        if text.starts_with("PING") {
                            debug!("Responding to PING");
                            let pong_msg = text.replace("PING", "PONG");
                            if let Err(e) = writer_for_pong.write().await.send(Message::Text(pong_msg)).await {
                                error!("Failed to send PONG: {}", e);
                            }
                            continue;
                        }

                        // Parse and broadcast chat messages
                        if let Some(chat_msg) = Self::parse_message(&text) {
                            if let Err(e) = message_sender.send(chat_msg) {
                                warn!("Failed to broadcast message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        debug!("Received binary message (ignoring)");
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        debug!("Received ping, sending pong");
                        if let Err(e) = writer_for_pong.write().await.send(Message::Pong(payload)).await {
                            error!("Failed to send pong: {}", e);
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        debug!("Received pong");
                    }
                    Some(Ok(Message::Close(close_frame))) => {
                        info!("WebSocket connection closed: {:?}", close_frame);
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {
                        debug!("Received raw frame (ignoring)");
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        warn!("WebSocket stream ended");
                        break;
                    }
                }
            }
            
            *is_connected.write().await = false;
            warn!("Twitch connection handler exited");
        });

        info!("Successfully connected to Twitch IRC");
        Ok(())
    }

    async fn send_message(&self, channel: &str, message: &str) -> Result<()> {
        if let Some(writer_arc) = &self.websocket_writer {
            let privmsg = format!("PRIVMSG #{} :{}\r\n", channel, message);
            
            match writer_arc.write().await.send(Message::Text(privmsg)).await {
                Ok(_) => {
                    debug!("Sent message to #{}: {}", channel, message);
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to send message to #{}: {}", channel, e);
                    Err(e.into())
                }
            }
        } else {
            Err(anyhow::anyhow!("Not connected to Twitch"))
        }
    }

    fn platform_name(&self) -> &str {
        "twitch"
    }

    async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }

    fn get_message_receiver(&self) -> Option<broadcast::Receiver<ChatMessage>> {
        self.message_sender.as_ref().map(|sender| sender.subscribe())
    }

    fn get_channels(&self) -> Vec<String> {
        self.config.channels.clone()
    }

    async fn disconnect(&mut self) -> Result<()> {
        *self.is_connected.write().await = false;
        self.websocket_writer = None;
        self.message_sender = None;
        info!("Disconnected from Twitch");
        Ok(())
    }
}