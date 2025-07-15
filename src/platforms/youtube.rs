use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{sleep, Duration};

use crate::platforms::PlatformConnection;
use crate::types::ChatMessage;

/// YouTube API response structures
#[derive(Debug, Deserialize)]
struct YouTubeResponse<T> {
    items: Vec<T>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "pollingIntervalMillis")]
    polling_interval_millis: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LiveChatMessage {
    id: String,
    snippet: LiveChatMessageSnippet,
    #[serde(rename = "authorDetails")]
    author_details: AuthorDetails,
}

#[derive(Debug, Deserialize)]
struct LiveChatMessageSnippet {
    #[serde(rename = "displayMessage")]
    display_message: String,
    #[serde(rename = "publishedAt")]
    published_at: String,
    #[serde(rename = "liveChatId")]
    live_chat_id: String,
}

#[derive(Debug, Deserialize)]
struct AuthorDetails {
    #[serde(rename = "channelId")]
    channel_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "isChatModerator")]
    is_chat_moderator: bool,
    #[serde(rename = "isChatOwner")]
    is_chat_owner: bool,
    #[serde(rename = "isChatSponsor")]
    is_chat_sponsor: bool,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    snippet: SendMessageSnippet,
}

#[derive(Debug, Serialize)]
struct SendMessageSnippet {
    #[serde(rename = "liveChatId")]
    live_chat_id: String,
    #[serde(rename = "textMessageDetails")]
    text_message_details: TextMessageDetails,
    #[serde(rename = "type")]
    message_type: String,
}

#[derive(Debug, Serialize)]
struct TextMessageDetails {
    #[serde(rename = "messageText")]
    message_text: String,
}

/// Configuration for YouTube Live Chat connection
#[derive(Debug, Clone)]
pub struct YouTubeConfig {
    pub api_key: String,          // For read-only operations
    pub oauth_token: String,      // For sending messages
    pub live_chat_id: String,
    pub video_id: Option<String>,
    pub polling_interval_ms: u64,
}

impl YouTubeConfig {
    /// Load YouTube configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("YOUTUBE_API_KEY")
            .context("YOUTUBE_API_KEY environment variable not set")?;
        
        let oauth_token = env::var("YOUTUBE_OAUTH_TOKEN")
            .context("YOUTUBE_OAUTH_TOKEN environment variable not set")?;
        
        let live_chat_id = env::var("YOUTUBE_LIVE_CHAT_ID")
            .context("YOUTUBE_LIVE_CHAT_ID environment variable not set")?;
        
        let video_id = env::var("YOUTUBE_VIDEO_ID").ok();
        
        let polling_interval_ms = env::var("YOUTUBE_POLLING_INTERVAL")
            .unwrap_or_else(|_| "5000".to_string())
            .parse::<u64>()
            .unwrap_or(5000);
        
        info!("Loaded YouTube config for live chat: {}", live_chat_id);
        if let Some(ref vid_id) = video_id {
            info!("Monitoring video: {}", vid_id);
        }
        
        Ok(Self {
            api_key,
            oauth_token,
            live_chat_id,
            video_id,
            polling_interval_ms,
        })
    }
    
    /// Auto-discover live chat ID from video ID
    pub async fn from_video_id(api_key: String, oauth_token: String, video_id: String) -> Result<Self> {
        let client = reqwest::Client::new();
        let url = format!(
            "https://www.googleapis.com/youtube/v3/videos?part=liveStreamingDetails&id={}&key={}",
            video_id, api_key
        );
        
        let response: YouTubeResponse<serde_json::Value> = client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        if let Some(video) = response.items.first() {
            if let Some(live_streaming) = video.get("liveStreamingDetails") {
                if let Some(chat_id) = live_streaming.get("activeLiveChatId") {
                    let live_chat_id = chat_id.as_str()
                        .context("Invalid live chat ID format")?
                        .to_string();
                    
                    info!("Auto-discovered live chat ID: {}", live_chat_id);
                    
                    return Ok(Self {
                        api_key,
                        oauth_token,
                        live_chat_id,
                        video_id: Some(video_id),
                        polling_interval_ms: 5000,
                    });
                }
            }
        }
        
        Err(anyhow::anyhow!("Could not find active live chat for video: {}", video_id))
    }
}

/// YouTube Live Chat connection implementation
pub struct YouTubeConnection {
    config: YouTubeConfig,
    message_sender: Option<broadcast::Sender<ChatMessage>>,
    is_connected: Arc<RwLock<bool>>,
    http_client: reqwest::Client,
    next_page_token: Arc<RwLock<Option<String>>>,
}

impl YouTubeConnection {
    pub fn new(config: YouTubeConfig) -> Self {
        Self {
            config,
            message_sender: None,
            is_connected: Arc::new(RwLock::new(false)),
            http_client: reqwest::Client::new(),
            next_page_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Poll YouTube Live Chat API for new messages
    async fn poll_messages(&self) -> Result<Vec<LiveChatMessage>> {
        let page_token = self.next_page_token.read().await.clone();
        
        let mut url = format!(
            "https://www.googleapis.com/youtube/v3/liveChat/messages?liveChatId={}&part=snippet,authorDetails&key={}",
            self.config.live_chat_id, self.config.api_key
        );
        
        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", token));
        }
        
        debug!("Polling YouTube Live Chat: {}", url);
        
        let response = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to poll YouTube Live Chat API")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("YouTube API error {}: {}", status, error_text));
        }
        
        let chat_response: YouTubeResponse<LiveChatMessage> = response
            .json()
            .await
            .context("Failed to parse YouTube Live Chat response")?;
        
        // Update next page token for subsequent requests
        {
            let mut token_guard = self.next_page_token.write().await;
            *token_guard = chat_response.next_page_token;
        }
        
        // Update polling interval if provided
        if let Some(interval) = chat_response.polling_interval_millis {
            debug!("YouTube suggested polling interval: {}ms", interval);
        }
        
        Ok(chat_response.items)
    }

    /// Convert YouTube message to our standard ChatMessage format
    fn convert_message(&self, yt_message: LiveChatMessage) -> ChatMessage {
        let display_name = yt_message.author_details.display_name.clone();
        ChatMessage {
            platform: "youtube".to_string(),
            channel: self.config.live_chat_id.clone(),
            username: display_name.clone(),
            display_name: Some(display_name),
            content: yt_message.snippet.display_message,
            timestamp: chrono::Utc::now(),
            user_badges: self.extract_badges(&yt_message.author_details),
            is_mod: yt_message.author_details.is_chat_moderator || yt_message.author_details.is_chat_owner,
            is_subscriber: yt_message.author_details.is_chat_sponsor,
        }
    }

    /// Extract user badges from YouTube author details
    fn extract_badges(&self, author: &AuthorDetails) -> Vec<String> {
        let mut badges = Vec::new();
        
        if author.is_chat_owner {
            badges.push("owner".to_string());
        }
        if author.is_chat_moderator {
            badges.push("moderator".to_string());
        }
        if author.is_chat_sponsor {
            badges.push("member".to_string());
        }
        
        badges
    }
}

#[async_trait]
impl PlatformConnection for YouTubeConnection {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to YouTube Live Chat...");
        
        // Test API connectivity
        let test_url = format!(
            "https://www.googleapis.com/youtube/v3/liveChat/messages?liveChatId={}&part=snippet&maxResults=1&key={}",
            self.config.live_chat_id, self.config.api_key
        );
        
        let response = self.http_client
            .get(&test_url)
            .send()
            .await
            .context("Failed to connect to YouTube Live Chat API")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("YouTube API connection failed {}: {}", status, error_text));
        }
        
        info!("Successfully connected to YouTube Live Chat API");
        
        // Set up message broadcasting
        let (tx, _) = broadcast::channel(1000);
        self.message_sender = Some(tx.clone());
        
        // Mark as connected
        *self.is_connected.write().await = true;
        
        // Start message polling loop
        let message_sender = tx;
        let is_connected = Arc::clone(&self.is_connected);
        let config = self.config.clone();
        let http_client = self.http_client.clone();
        let next_page_token = Arc::clone(&self.next_page_token);
        
        tokio::spawn(async move {
            info!("YouTube Live Chat message poller started");
            let mut interval = Duration::from_millis(config.polling_interval_ms);
            
            loop {
                if !*is_connected.read().await {
                    info!("YouTube connection marked as disconnected, stopping poller");
                    break;
                }
                
                // Create a temporary connection for polling
                let temp_connection = YouTubeConnection {
                    config: config.clone(),
                    message_sender: None,
                    is_connected: Arc::clone(&is_connected),
                    http_client: http_client.clone(),
                    next_page_token: Arc::clone(&next_page_token),
                };
                
                match temp_connection.poll_messages().await {
                    Ok(messages) => {
                        debug!("Polled {} new YouTube messages", messages.len());
                        
                        for yt_message in messages {
                            let chat_message = temp_connection.convert_message(yt_message);
                            info!("YouTube message from {}: {}", chat_message.username, chat_message.content);
                            
                            if let Err(e) = message_sender.send(chat_message) {
                                warn!("Failed to broadcast YouTube message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to poll YouTube messages: {}", e);
                        
                        // If we get an auth error, mark as disconnected
                        if e.to_string().contains("403") || e.to_string().contains("401") {
                            error!("YouTube API authentication failed, marking as disconnected");
                            *is_connected.write().await = false;
                            break;
                        }
                        
                        // Exponential backoff on errors
                        interval = std::cmp::min(interval * 2, Duration::from_secs(60));
                        warn!("Backing off polling interval to {:?}", interval);
                    }
                }
                
                sleep(interval).await;
                
                // Reset interval on successful polls
                if interval != Duration::from_millis(config.polling_interval_ms) {
                    interval = Duration::from_millis(config.polling_interval_ms);
                    debug!("Reset polling interval to {}ms", config.polling_interval_ms);
                }
            }
            
            warn!("YouTube Live Chat message poller stopped");
        });
        
        info!("YouTube Live Chat connection established");
        Ok(())
    }

    async fn send_message(&self, _channel: &str, message: &str) -> Result<()> {
        let request = SendMessageRequest {
            snippet: SendMessageSnippet {
                live_chat_id: self.config.live_chat_id.clone(),
                text_message_details: TextMessageDetails {
                    message_text: message.to_string(),
                },
                message_type: "textMessageEvent".to_string(),
            },
        };
        
        let url = format!(
            "https://www.googleapis.com/youtube/v3/liveChat/messages?part=snippet",
        );
        
        let response = self.http_client
            .post(&url)
            .bearer_auth(&self.config.oauth_token)  // Add OAuth token
            .json(&request)
            .send()
            .await
            .context("Failed to send YouTube Live Chat message")?;
        
        if response.status().is_success() {
            debug!("Sent YouTube message: {}", message);
            Ok(())
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(anyhow::anyhow!("Failed to send YouTube message {}: {}", status, error_text))
        }
    }

    fn platform_name(&self) -> &str {
        "youtube"
    }

    async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }

    fn get_message_receiver(&self) -> Option<broadcast::Receiver<ChatMessage>> {
        self.message_sender.as_ref().map(|sender| sender.subscribe())
    }

    fn get_channels(&self) -> Vec<String> {
        vec![self.config.live_chat_id.clone()]
    }

    async fn disconnect(&mut self) -> Result<()> {
        *self.is_connected.write().await = false;
        self.message_sender = None;
        info!("Disconnected from YouTube Live Chat");
        Ok(())
    }
}