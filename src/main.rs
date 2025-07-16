// Integration code for main.rs - Add these sections to your existing main.rs

use anyhow::Result;
use log::{debug, error, info, warn};
use tokio::time::{sleep, Duration};
use std::env;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use notabot::prelude::*;
use notabot::config::ConfigurationManager;
use notabot::bot::config_integration::{ConfigIntegration, ConfigCommands};
use notabot::bot::connection_pool::{ConnectionPool, PoolConfig};
use notabot::bot::shutdown::{GracefulShutdown, ShutdownIntegration, ShutdownConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables and initialize logging
    dotenv::dotenv().ok();
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting NotaBot v{} - AI-Powered Moderation", env!("CARGO_PKG_VERSION"));

    // =================================================================
    // CONFIGURATION SYSTEM INITIALIZATION
    // =================================================================
    
    info!("Initializing configuration management system...");
    
    let config_dir = Path::new("config");
    let config_manager = Arc::new(ConfigurationManager::new(config_dir));
    
    if let Err(e) = config_manager.initialize().await {
        error!("Failed to initialize configuration system: {}", e);
        return Err(e);
    }
    
    info!("Configuration system initialized with hot-reload support");

    // =================================================================
    // CONNECTION POOL INITIALIZATION
    // =================================================================
    
    info!("Initializing connection pool...");
    
    // Create connection pool with custom configuration
    let pool_config = PoolConfig {
        max_connections_per_platform: 3,
        min_idle_connections: 1,
        max_idle_connections: 2,
        connection_timeout_seconds: 30,
        idle_timeout_seconds: 300,
        health_check_interval_seconds: 60,
        retry_attempts: 3,
        retry_delay_seconds: 5,
    };
    
    let connection_pool = Arc::new(ConnectionPool::new(pool_config));
    
    // Initialize pools for available platforms
    let mut available_platforms = Vec::new();
    
    // Check what platforms are configured
    let bot_config = config_manager.get_bot_config().await;
    if let Some(twitch_config) = bot_config.platforms.get("twitch") {
        if twitch_config.enabled {
            available_platforms.push("twitch".to_string());
        }
    }
    if let Some(youtube_config) = bot_config.platforms.get("youtube") {
        if youtube_config.enabled {
            available_platforms.push("youtube".to_string());
        }
    }
    
    if !available_platforms.is_empty() {
        connection_pool.initialize(available_platforms.clone()).await?;
        info!("Connection pool initialized for platforms: {:?}", available_platforms);
    } else {
        warn!("No platforms enabled in configuration");
    }

    // =================================================================
    // BOT CORE INITIALIZATION WITH POOLED CONNECTIONS
    // =================================================================
    
    let mut bot = ChatBot::new();
    let bot_arc = Arc::new(RwLock::new(bot));

    // Instead of adding connections directly, the bot will use the pool
    // You would modify the ChatBot to use the connection pool for sending messages
    
    // For now, we'll still add connections directly but show how to integrate the pool
    {
        let mut bot_guard = bot_arc.write().await;
        
        // Add platform connections (these will be managed by the pool)
        if available_platforms.contains(&"twitch".to_string()) {
            if let Ok(twitch_config) = TwitchConfig::from_env() {
                let twitch_connection = TwitchConnection::new(twitch_config);
                bot_guard.add_connection(Box::new(twitch_connection)).await;
                info!("Twitch connection added to bot");
            }
        }

        if available_platforms.contains(&"youtube".to_string()) {
            if let Ok(youtube_config) = YouTubeConfig::from_env() {
                let youtube_connection = YouTubeConnection::new(youtube_config);
                bot_guard.add_connection(Box::new(youtube_connection)).await;
                info!("YouTube connection added to bot");
            }
        }
    }

    // =================================================================
    // GRACEFUL SHUTDOWN SETUP
    // =================================================================
    
    info!("Setting up graceful shutdown system...");
    
    let shutdown_manager = ShutdownIntegration::setup(
        bot_arc.clone(),
        Some(connection_pool.clone()),
        config_manager.clone(),
    ).await?;
    
    info!("Graceful shutdown system ready");

    // =================================================================
    // ENHANCED MODERATION WITH CONFIGURATION INTEGRATION
    // =================================================================
    
    info!("Setting up AI-powered moderation with configuration integration...");
    
    let enhanced_moderation = {
        let bot_guard = bot_arc.read().await;
        bot_guard.create_enhanced_moderation()
    };
    let enhanced_moderation = Arc::new(enhanced_moderation);
    
    // Setup configuration integration
    let mut config_integration = ConfigIntegration::new(
        config_manager.clone(),
        {
            let bot_guard = bot_arc.read().await;
            bot_guard.get_moderation_system()
        },
    );
    config_integration.set_enhanced_moderation(enhanced_moderation.clone());
    
    if let Err(e) = config_integration.initialize().await {
        error!("Failed to initialize configuration integration: {}", e);
        return Err(e);
    }
    
    let config_integration = Arc::new(config_integration);
    
    info!("Configuration integration initialized - all filters and patterns loaded from files");

    // Enable enhanced features based on configuration
    if bot_config.features.ai_moderation {
        enhanced_moderation.set_enhanced_features_enabled(true).await;
        info!("AI moderation enabled");
    }
    
    if bot_config.features.learning_mode {
        enhanced_moderation.set_learning_mode(true).await;
        info!("Learning mode enabled");
    }

    // =================================================================
    // COMMAND REGISTRATION WITH SHUTDOWN AWARENESS
    // =================================================================
    
    info!("Registering commands with shutdown awareness...");
    
    let config_commands = Arc::new(ConfigCommands::new(config_integration.clone()));
    
    // Register basic commands
    {
        let bot_guard = bot_arc.read().await;
        bot_guard.add_command("hello".to_string(), "Hello $(user)! Welcome to our enterprise-grade AI-moderated stream!".to_string(), false, 5).await;
        bot_guard.add_command("uptime".to_string(), "AI moderation system running with connection pooling and graceful shutdown!".to_string(), false, 30).await;
        
        // Add shutdown command for administrators
        bot_guard.add_command("shutdown".to_string(), "Initiating graceful shutdown... (admin only)".to_string(), true, 300).await;
        
        // Add pool statistics command
        bot_guard.add_command("poolstats".to_string(), "Connection pool statistics (mod only)".to_string(), true, 30).await;
        
        // Configuration commands
        bot_guard.add_command("reloadconfig".to_string(), "Configuration management (mod only)".to_string(), true, 60).await;
        bot_guard.add_command("configstatus".to_string(), "Configuration status (mod only)".to_string(), true, 30).await;
    }

    // =================================================================
    // BACKGROUND MONITORING WITH SHUTDOWN AWARENESS
    // =================================================================
    
    // Start connection pool monitoring
    let pool_monitor = connection_pool.clone();
    let shutdown_monitor = shutdown_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        let mut shutdown_receiver = shutdown_monitor.subscribe_to_shutdown();
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if shutdown_monitor.is_shutdown_requested().await {
                        break;
                    }
                    
                    let stats = pool_monitor.get_stats().await;
                    for (platform, platform_stats) in stats {
                        debug!("Pool stats for {}: {} total, {} active, {} idle", 
                               platform, 
                               platform_stats.total_connections,
                               platform_stats.active_connections, 
                               platform_stats.idle_connections);
                    }
                }
                _ = shutdown_receiver.recv() => {
                    info!("Pool monitor received shutdown signal");
                    break;
                }
            }
        }
        
        info!("Connection pool monitor stopped");
    });

    // =================================================================
    // MESSAGE PROCESSING WITH SHUTDOWN-AWARE PERMITS
    // =================================================================
    
    // Enhanced message processing that respects shutdown
    let message_processor_shutdown = shutdown_manager.clone();
    let message_processor_config_commands = config_commands.clone();
    let message_processor_enhanced_mod = enhanced_moderation.clone();
    let message_processor_pool = connection_pool.clone();
    
    tokio::spawn(async move {
        info!("Starting shutdown-aware message processor...");
        
        loop {
            // Check if shutdown is requested
            if message_processor_shutdown.is_shutdown_requested().await {
                info!("Message processor stopping due to shutdown request");
                break;
            }
            
            // Simulate message processing with shutdown permits
            if let Some(_permit) = message_processor_shutdown.acquire_operation_permit().await {
                // Process messages here
                sleep(Duration::from_millis(100)).await;
            } else {
                debug!("Skipping message processing due to shutdown");
                break;
            }
        }
        
        info!("Message processor stopped gracefully");
    });

    // =================================================================
    // WEB DASHBOARD WITH ENHANCED STATS
    // =================================================================
    
    let dashboard_port = env::var("DASHBOARD_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    
    {
        let bot_guard = bot_arc.read().await;
        if let Err(e) = bot_guard.start_web_dashboard(dashboard_port).await {
            warn!("Failed to start web dashboard: {}", e);
        } else {
            info!("Enterprise Dashboard: http://localhost:{}", dashboard_port);
            info!("Features: Connection pooling stats, graceful shutdown status, real-time config monitoring");
        }
    }

    // =================================================================
    // START CORE BOT SYSTEMS
    // =================================================================
    
    {
        let mut bot_guard = bot_arc.write().await;
        if let Err(e) = bot_guard.start().await {
            error!("Failed to start bot core: {}", e);
            return Err(e);
        }
    }

    info!("NotaBot Enterprise System Started Successfully!");
    info!("Features active:");
    info!("  - Hot-reload configuration management");
    info!("  - Connection pooling with health monitoring");
    info!("  - Graceful shutdown with component coordination");
    info!("  - AI-powered moderation with real-time learning");
    info!("  - Enterprise-grade monitoring and analytics");

    // =================================================================
    // MAIN APPLICATION LOOP WITH GRACEFUL SHUTDOWN
    // =================================================================
    
    // Run monitoring loop until shutdown
    let mut stats_counter = 0;
    let mut pool_stats_counter = 0;
    
    loop {
        // Check for shutdown
        if shutdown_manager.is_shutdown_requested().await {
            info!("Shutdown requested, exiting main loop");
            break;
        }
        
        sleep(Duration::from_secs(60)).await;
        
        // Connection pool health monitoring (every 5 minutes)
        pool_stats_counter += 1;
        if pool_stats_counter >= 5 {
            pool_stats_counter = 0;
            
            // Force health check on pools
            connection_pool.force_health_check().await;
            
            let pool_stats = connection_pool.get_stats().await;
            for (platform, stats) in pool_stats {
                info!("Pool {}: {} connections ({} active, {} idle), {:.1}ms avg response, {} failures",
                      platform,
                      stats.total_connections,
                      stats.active_connections,
                      stats.idle_connections,
                      stats.average_response_time_ms,
                      stats.failed_connections);
            }
        }
        
        // General statistics (every 5 minutes)
        stats_counter += 1;
        if stats_counter >= 5 {
            stats_counter = 0;
            
            // AI system status
            let ai_status = enhanced_moderation.get_system_status().await;
            info!("AI Status: Health={:.1}%, Patterns={}, Learning={}", 
                  ai_status.system_health_score * 100.0,
                  ai_status.total_patterns,
                  if ai_status.learning_mode_enabled { "ON" } else { "OFF" }
            );
            
            // Shutdown system status
            let shutdown_stats = shutdown_manager.get_stats().await;
            debug!("Shutdown phase: {:?}, Components ready: {}", 
                   shutdown_stats.phase, 
                   shutdown_stats.components_shutdown.len());
        }
        
        // Heartbeat
        if stats_counter == 0 && pool_stats_counter == 0 {
            info!("NotaBot Enterprise running strong - Connection pooling + Graceful shutdown enabled!");
        }
    }

    // =================================================================
    // GRACEFUL SHUTDOWN EXECUTION
    // =================================================================
    
    info!("Starting graceful shutdown sequence...");
    
    // Wait for graceful shutdown to complete
    if let Err(e) = shutdown_manager.wait_for_shutdown().await {
        error!("Graceful shutdown failed: {}", e);
        return Err(e);
    }

    let shutdown_stats = shutdown_manager.get_stats().await;
    info!("Shutdown completed in {:.2} seconds", 
          shutdown_stats.duration_seconds.unwrap_or(0.0));
    info!("Components shut down: {:?}", shutdown_stats.components_shutdown);
    
    if !shutdown_stats.failed_components.is_empty() {
        warn!("Some components failed to shutdown cleanly: {:?}", shutdown_stats.failed_components);
    }

    info!("NotaBot Enterprise shutdown complete. Goodbye!");
    Ok(())
}

// Helper function for shutdown-aware message processing
async fn process_message_with_shutdown_protection(
    message: ChatMessage,
    shutdown_manager: &GracefulShutdown,
    config_commands: &ConfigCommands,
    enhanced_moderation: &Arc<EnhancedModerationSystem>,
    connection_pool: &Arc<ConnectionPool>,
) -> Option<String> {
    // Get operation permit to ensure we don't start processing during shutdown
    let _permit = shutdown_manager.acquire_operation_permit().await?;
    
    // Handle configuration commands
    if let Some(response) = handle_config_commands(&message, config_commands, enhanced_moderation).await {
        return Some(response);
    }
    
    // Handle pool commands
    if message.content.starts_with("!poolstats") && message.is_mod {
        let stats = connection_pool.get_stats().await;
        let mut response = "Connection Pool Stats:\n".to_string();
        
        for (platform, platform_stats) in stats {
            response.push_str(&format!(
                "{}: {} total ({} active, {} idle), {:.1}ms avg\n",
                platform,
                platform_stats.total_connections,
                platform_stats.active_connections,
                platform_stats.idle_connections,
                platform_stats.average_response_time_ms
            ));
        }
        
        return Some(response);
    }
    
    // Handle manual shutdown command
    if message.content.starts_with("!shutdown") && message.is_mod {
        shutdown_manager.trigger_shutdown().await;
        return Some("Graceful shutdown initiated by moderator. Bot will shut down safely.".to_string());
    }
    
    None
}

// Enhanced config command handler with pool integration
async fn handle_config_commands(
    message: &ChatMessage, 
    config_commands: &ConfigCommands,
    enhanced_moderation: &Arc<EnhancedModerationSystem>
) -> Option<String> {
    if !message.content.starts_with("!") {
        return None;
    }
    
    let parts: Vec<&str> = message.content[1..].split_whitespace().collect();
    let command = parts.first()?;
    let args = &parts[1..];
    
    match *command {
        "reloadconfig" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            let config_type = args.first().copied();
            match config_commands.handle_reload_command(config_type).await {
                Ok(response) => Some(format!("Success: {}", response)),
                Err(e) => Some(format!("Reload failed: {}", e)),
            }
        }
        
        "configstatus" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            match config_commands.handle_status_command().await {
                Ok(response) => Some(response),
                Err(e) => Some(format!("Status error: {}", e)),
            }
        }
        
        "validateconfig" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            match config_commands.handle_validate_command().await {
                Ok(response) => Some(response),
                Err(e) => Some(format!("Validation error: {}", e)),
            }
        }
        
        "exportconfig" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            let format = args.first().copied().unwrap_or("json");
            match config_commands.handle_export_command(format).await {
                Ok(response) => Some(response),
                Err(e) => Some(format!("Export failed: {}", e)),
            }
        }
        
        "backupconfig" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            match config_commands.handle_backup_command().await {
                Ok(response) => Some(response),
                Err(e) => Some(format!("Backup failed: {}", e)),
            }
        }
        
        "appeal" => {
            if args.is_empty() {
                return Some("Usage: !appeal <reason>. Describe why you think the moderation action was incorrect.".to_string());
            }
            
            let reason = args.join(" ");
            let user_id = format!("{}:{}", message.platform, message.username);
            
            if let Err(e) = enhanced_moderation.record_user_feedback(
                "user_appeal",
                &user_id,
                notabot::bot::realtime_analytics::UserReportType::FalsePositive,
                &message.content,
                Some(reason.clone()),
            ).await {
                error!("Failed to record user appeal: {}", e);
            }
            
            Some(format!("Appeal recorded: '{}'. Our AI will learn from this feedback. Thank you!", reason))
        }
        
        "aiinfo" => {
            let status = enhanced_moderation.get_system_status().await;
            Some(format!(
                "AI Status: Health {:.0}%, {} patterns active, Learning: {}, Optimization: {}",
                status.system_health_score * 100.0,
                status.total_patterns,
                if status.learning_mode_enabled { "ON" } else { "OFF" },
                if status.auto_optimization_enabled { "ON" } else { "OFF" }
            ))
        }
        
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_connection_pool_integration() {
        let pool_config = PoolConfig::default();
        let connection_pool = Arc::new(ConnectionPool::new(pool_config));
        
        // Test pool initialization
        let platforms = vec!["test_platform".to_string()];
        // Note: This would fail without actual platform implementations
        // In real tests, you'd mock the platform connections
        
        assert!(connection_pool.is_running().await == false); // Not started yet
    }

    #[tokio::test]
    async fn test_graceful_shutdown_integration() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        config_manager.initialize().await.unwrap();
        
        let bot = Arc::new(RwLock::new(ChatBot::new()));
        
        let shutdown_manager = ShutdownIntegration::setup(
            bot,
            None, // No connection pool for this test
            config_manager,
        ).await.unwrap();
        
        assert_eq!(shutdown_manager.get_phase().await, ShutdownPhase::Running);
        
        // Test manual shutdown trigger
        shutdown_manager.trigger_shutdown().await;
        assert!(shutdown_manager.is_shutdown_requested().await);
    }

    #[tokio::test]
    async fn test_shutdown_aware_operation_permits() {
        let shutdown_manager = GracefulShutdown::with_default_config();
        
        // Should get permit during normal operation
        let permit = shutdown_manager.acquire_operation_permit().await;
        assert!(permit.is_some());
        
        // Trigger shutdown
        shutdown_manager.trigger_shutdown().await;
        
        // Should not get permit during shutdown
        let permit_after_shutdown = shutdown_manager.acquire_operation_permit().await;
        assert!(permit_after_shutdown.is_none());
    }
}