use anyhow::Result;
use log::{debug, error, info, warn};
use tokio::time::{sleep, Duration};
use std::env;
use std::path::Path;
use std::sync::Arc;

use notabot::prelude::*;
use notabot::config::ConfigurationManager;
use notabot::bot::config_integration::{ConfigIntegration, ConfigCommands};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables and initialize logging
    dotenv::dotenv().ok();
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting NotaBot v{} - AI-Powered Moderation System with Hot-Reload Config!", env!("CARGO_PKG_VERSION"));

    // =================================================================
    // CONFIGURATION SYSTEM INITIALIZATION
    // =================================================================
    
    info!("Initializing configuration management system...");
    
    // Create configuration manager with hot-reload support
    let config_dir = Path::new("config");
    let config_manager = Arc::new(ConfigurationManager::new(config_dir));
    
    // Initialize configuration system (creates default files if needed)
    if let Err(e) = config_manager.initialize().await {
        error!("Failed to initialize configuration system: {}", e);
        return Err(e);
    }
    
    info!("Configuration system initialized with hot-reload support");

    // =================================================================
    // BOT CORE INITIALIZATION
    // =================================================================
    
    // Create enhanced bot instance
    let mut bot = ChatBot::new();

    // Load bot configuration and setup platforms
    let bot_config = config_manager.get_bot_config().await;
    
    // Add platform connections based on configuration
    if let Some(twitch_config) = bot_config.platforms.get("twitch") {
        if twitch_config.enabled {
            if let Ok(twitch_connection_config) = TwitchConfig::from_env() {
                let twitch_connection = TwitchConnection::new(twitch_connection_config);
                bot.add_connection(Box::new(twitch_connection)).await;
                info!("Twitch connection configured");
            } else {
                warn!("Twitch enabled in config but environment variables missing");
            }
        }
    }

    if let Some(youtube_config) = bot_config.platforms.get("youtube") {
        if youtube_config.enabled {
            if let Ok(youtube_connection_config) = YouTubeConfig::from_env() {
                let youtube_connection = YouTubeConnection::new(youtube_connection_config);
                bot.add_connection(Box::new(youtube_connection)).await;
                info!("YouTube connection configured");
            } else {
                warn!("YouTube enabled in config but environment variables missing");
            }
        }
    }

    // =================================================================
    // ENHANCED MODERATION WITH CONFIGURATION INTEGRATION
    // =================================================================
    
    info!("Setting up AI-powered moderation with configuration integration...");
    
    // Create enhanced moderation system
    let enhanced_moderation = bot.create_enhanced_moderation();
    
    // Wrap in Arc before using
    let enhanced_moderation = Arc::new(enhanced_moderation);

    // Setup configuration integration
    let mut config_integration = ConfigIntegration::new(
        config_manager.clone(),
        bot.get_moderation_system(),
    );
    config_integration.set_enhanced_moderation(enhanced_moderation.clone());
    
    // Initialize configuration integration (loads and applies all configs)
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

    if bot_config.features.auto_optimization {
        enhanced_moderation.set_auto_optimization_enabled(true).await;
        info!("Auto-optimization enabled");
    }

    // =================================================================
    // CONFIGURATION-BASED COMMANDS
    // =================================================================
    
    info!("Registering configuration-aware commands...");
    
    // Create configuration commands handler
    let config_commands = Arc::new(ConfigCommands::new(config_integration.clone()));
    
    // Basic commands
    bot.add_command("hello".to_string(), "Hello $(user)! Welcome to our AI-moderated stream!".to_string(), false, 5).await;
    bot.add_command("uptime".to_string(), "AI moderation system running smoothly!".to_string(), false, 30).await;
    
    // AI and moderation commands
    bot.add_command("ai".to_string(), 
        "This stream uses NotaBot's next-gen AI moderation! Real-time learning, hot-reload configs, high uptime!".to_string(), 
        false, 30).await;
    
    // Configuration management commands (moderator only)
    bot.add_command("reloadconfig".to_string(), 
        "Reloading configuration... (Use: !reloadconfig [filters|patterns|timers|all])".to_string(), 
        true, 60).await;
    
    bot.add_command("configstatus".to_string(), 
        "Configuration status and statistics".to_string(), 
        true, 30).await;
    
    bot.add_command("validateconfig".to_string(), 
        "Validating all configuration files".to_string(), 
        true, 60).await;
    
    bot.add_command("exportconfig".to_string(), 
        "Export configuration (Use: !exportconfig [json|yaml|nightbot])".to_string(), 
        true, 120).await;
    
    bot.add_command("backupconfig".to_string(), 
        "Create configuration backup".to_string(), 
        true, 300).await;
    
    // Enhanced filter management commands
    bot.add_command("filters".to_string(), 
        "Filter Management: !filters <list|enable|disable|stats> [filter_id] | Configs auto-reload from files!".to_string(), 
        true, 10).await;
    
    bot.add_command("patterns".to_string(), 
        "AI Pattern Status: Fuzzy matching, leetspeak, unicode, homoglyphs, zalgo detection + more! All configurable!".to_string(), 
        true, 30).await;
    
    bot.add_command("ailearning".to_string(), 
        "AI Learning: Real-time pattern adaptation, false positive reduction, community feedback integration".to_string(), 
        true, 30).await;
    
    // User-facing commands
    bot.add_command("appeal".to_string(), 
        "Appeal a moderation action: !appeal <reason>. Our AI learns from feedback to improve accuracy!".to_string(), 
        false, 300).await;
    
    bot.add_command("modhelp".to_string(), 
        "Moderation Help: This chat uses AI-powered moderation. Appeals are welcome and help train the system!".to_string(), 
        false, 60).await;
    
    // Points and engagement commands
    bot.add_command("points".to_string(), "AI-tracked points: !points [user] - Earned through positive participation".to_string(), false, 5).await;
    bot.add_command("achievements".to_string(), "AI-powered achievements: !achievements [user] - Unlock through good behavior".to_string(), false, 10).await;
    bot.add_command("leaderboard".to_string(), "Community leaders: !leaderboard [number] - Top contributors by AI metrics".to_string(), false, 30).await;
    
    // Giveaway commands
    bot.add_command("gstart".to_string(), "Start giveaway: !gstart <active|keyword|number> [options] (mod only)".to_string(), true, 30).await;
    bot.add_command("gend".to_string(), "End current giveaway and select winner (mod only)".to_string(), true, 10).await;
    bot.add_command("gstatus".to_string(), "Show current giveaway status and participant count".to_string(), false, 30).await;
    
    info!("Commands registered with configuration integration");

    // =================================================================
    // CONFIGURATION CHANGE MONITORING
    // =================================================================
    
    // Setup configuration change monitoring
    let config_integration_monitor = config_integration.clone();
    
    tokio::spawn(async move {
        let mut receiver = config_integration_monitor.get_config_manager().subscribe_to_changes();
        
        while let Ok(event) = receiver.recv().await {
            match event {
                notabot::config::ConfigChangeEvent::FiltersUpdated { file } => {
                    info!("Filters updated in {}, automatically applied!", file);
                }
                notabot::config::ConfigChangeEvent::PatternsUpdated { file } => {
                    info!("Patterns updated in {}, AI enhanced!", file);
                }
                notabot::config::ConfigChangeEvent::TimersUpdated { file } => {
                    info!("Timers updated in {}, schedule refreshed!", file);
                }
                notabot::config::ConfigChangeEvent::ValidationError { file, error } => {
                    error!("Configuration error in {}: {}", file, error);
                }
                notabot::config::ConfigChangeEvent::ReloadComplete { files_updated } => {
                    info!("Hot-reload complete for: {:?}", files_updated);
                }
                _ => {}
            }
        }
    });

    // =================================================================
    // CUSTOM COMMAND HANDLING
    // =================================================================
    
    // Clone references for the message processing loop
    let config_commands_handler = config_commands.clone();
    let enhanced_moderation_handler = enhanced_moderation.clone();
    
    // Get the bot's message receiver and response sender
    let (message_sender, mut message_receiver) = tokio::sync::mpsc::channel(1000);
    let (response_sender, mut response_receiver) = tokio::sync::mpsc::channel(1000);
    
    // Set up message processing task
    tokio::spawn(async move {
        while let Some(message) = message_receiver.recv().await {
            // Handle config commands first
            if let Some(response) = handle_config_commands(&message, &config_commands_handler, &enhanced_moderation_handler).await {
                if let Err(e) = response_sender.send((
                    message.platform.clone(),
                    message.channel.clone(),
                    response
                )).await {
                    error!("Failed to send config command response: {}", e);
                }
                continue; // Skip regular command processing
            }
            
            // Continue with regular command processing through the bot's command system
            // The bot will handle this through its existing CommandSystem
        }
    });

    // =================================================================
    // WEB DASHBOARD WITH CONFIGURATION MANAGEMENT
    // =================================================================
    
    // Start web dashboard
    let dashboard_port = env::var("DASHBOARD_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    
    if let Err(e) = bot.start_web_dashboard(dashboard_port).await {
        warn!("Failed to start web dashboard: {}", e);
    } else {
        info!("AI Analytics Dashboard: http://localhost:{}", dashboard_port);
        info!("Real-time configuration monitoring available");
    }

    // =================================================================
    // START BOT CORE SYSTEMS
    // =================================================================
    
    // Start core bot systems
    if let Err(e) = bot.start().await {
        error!("Failed to start bot core: {}", e);
        return Err(e);
    }

    info!("NotaBot AI System Started Successfully!");
    info!("Configuration: Hot-reload enabled from ./config/");
    info!("Dashboard: http://localhost:{}", dashboard_port);
    info!("AI Features: ACTIVE with real-time learning");
    info!("Spam Protection: ENHANCED with configurable patterns");
    info!("Hot-Reload: Edit config files without restarting!");
    info!("Analytics: Real-time effectiveness monitoring");

    // =================================================================
    // CONFIGURATION-AWARE MONITORING LOOP
    // =================================================================
    
    let mut stats_counter = 0;
    let mut optimization_counter = 0;
    let mut config_validation_counter = 0;
    
    loop {
        sleep(Duration::from_secs(60)).await;
        
        // Health monitoring
        let health = bot.health_check().await;
        let unhealthy: Vec<_> = health.iter().filter(|(_, &h)| !h).collect();
        if !unhealthy.is_empty() {
            error!("Unhealthy platforms: {:?}", unhealthy);
        }
        
        // Configuration statistics (every 5 minutes)
        stats_counter += 1;
        if stats_counter >= 5 {
            stats_counter = 0;
            
            // Get configuration statistics
            let config_stats = config_integration.get_config_stats().await;
            info!("Config Stats: {} filters ({} enabled), {} patterns ({} enabled), {} timers ({} enabled)",
                  config_stats.total_blacklist_filters,
                  config_stats.enabled_blacklist_filters,
                  config_stats.total_pattern_collections,
                  config_stats.enabled_pattern_collections,
                  config_stats.total_timers,
                  config_stats.enabled_timers
            );
            
            // Get AI system status
            let ai_status = enhanced_moderation.get_system_status().await;
            info!("AI Status: Health={:.1}%, Patterns={}, Alerts={}, Learning={}", 
                ai_status.system_health_score * 100.0,
                ai_status.total_patterns,
                ai_status.active_alerts,
                if ai_status.learning_mode_enabled { "ON" } else { "OFF" }
            );
            
            // Get effectiveness report
            if let Ok(report) = enhanced_moderation.get_effectiveness_report().await {
                info!("AI Performance: Accuracy={:.1}%, Satisfaction={:.1}%, AvgResponse={:.1}ms",
                      report.overall_accuracy * 100.0,
                      report.user_satisfaction * 100.0,
                      report.performance_metrics.average_response_time
                );
                
                // Log important recommendations
                for rec in &report.recommendations {
                    if matches!(rec.priority, notabot::bot::realtime_analytics::RecommendationPriority::High | 
                                           notabot::bot::realtime_analytics::RecommendationPriority::Critical) {
                        warn!("AI Recommendation ({:?}): {}", rec.priority, rec.title);
                    }
                }
            }
        }
        
        // Configuration validation (every 15 minutes)
        config_validation_counter += 1;
        if config_validation_counter >= 15 {
            config_validation_counter = 0;
            
            // Validate configurations
            match config_integration.validate_configurations().await {
                Ok(report) => {
                    if !report.errors.is_empty() {
                        error!("Configuration validation errors: {:?}", report.errors);
                    } else {
                        debug!("All configurations valid");
                    }
                    
                    if !report.warnings.is_empty() {
                        warn!("Configuration warnings: {:?}", report.warnings);
                    }
                }
                Err(e) => {
                    error!("Failed to validate configurations: {}", e);
                }
            }
        }
        
        // AI auto-optimization (every 30 minutes)
        optimization_counter += 1;
        if optimization_counter >= 30 {
            optimization_counter = 0;
            
            // Run auto-optimization if enabled
            if bot_config.features.auto_optimization {
                match enhanced_moderation.auto_optimize_filters().await {
                    Ok(result) => {
                        if result.optimizations_applied > 0 {
                            info!("AI Auto-Optimization: {} improvements applied, {:.1}% performance gain",
                                  result.optimizations_applied, result.performance_improvement);
                        }
                    }
                    Err(e) => {
                        debug!("Auto-optimization skipped: {}", e);
                    }
                }
            }
        }
        
        // Heartbeat with configuration info (every 10 minutes)
        if stats_counter == 0 && optimization_counter % 10 == 0 {
            info!("NotaBot AI running strong with hot-reload configuration management!");
            info!("Edit ./config/*.yaml files to update filters, patterns, and timers without restart!");
        }
    }
}

// Keep the function signature with Arc:
async fn handle_config_commands(
    message: &ChatMessage, 
    config_commands: &ConfigCommands,
    enhanced_moderation: &EnhancedModerationSystem
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
            
            // Record the appeal for AI learning
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
            
            Some(format!("Appeal recorded: '{}'. Our AI will learn from this feedback to improve accuracy. Thank you!", reason))
        }
        
        "filterstats" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            // Get AI effectiveness report
            match enhanced_moderation.get_effectiveness_report().await {
                Ok(report) => {
                    Some(format!(
                        "Filter Stats: {:.1}% accuracy, {:.1}% satisfaction, {:.1}ms avg response, {} recommendations",
                        report.overall_accuracy * 100.0,
                        report.user_satisfaction * 100.0,
                        report.performance_metrics.average_response_time,
                        report.recommendations.len()
                    ))
                }
                Err(e) => Some(format!("Failed to get filter stats: {}", e))
            }
        }
        
        "aiinfo" => {
            // Get AI system status (available to all users)
            let status = enhanced_moderation.get_system_status().await;
            Some(format!(
                "AI Status: Health {:.0}%, {} patterns active, Learning: {}, Optimization: {}",
                status.system_health_score * 100.0,
                status.total_patterns,
                if status.learning_mode_enabled { "ON" } else { "OFF" },
                if status.auto_optimization_enabled { "ON" } else { "OFF" }
            ))
        }
        
        "togglelearning" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            let enable = args.first().map(|&s| s == "on" || s == "true" || s == "enable");
            match enable {
                Some(true) => {
                    enhanced_moderation.set_learning_mode(true).await;
                    Some("AI learning mode enabled. The system will adapt from user feedback.".to_string())
                }
                Some(false) => {
                    enhanced_moderation.set_learning_mode(false).await;
                    Some("AI learning mode disabled. The system will use static patterns only.".to_string())
                }
                None => {
                    let status = enhanced_moderation.get_system_status().await;
                    Some(format!("AI learning mode is currently: {}", if status.learning_mode_enabled { "ON" } else { "OFF" }))
                }
            }
        }
        
        "optimize" => {
            if !message.is_mod {
                return Some("This command is moderator-only.".to_string());
            }
            
            match enhanced_moderation.auto_optimize_filters().await {
                Ok(result) => {
                    if result.optimizations_applied > 0 {
                        Some(format!(
                            "Optimization complete: {} improvements applied, {:.1}% performance gain",
                            result.optimizations_applied,
                            result.performance_improvement
                        ))
                    } else {
                        Some("No optimizations needed. System is already running efficiently.".to_string())
                    }
                }
                Err(e) => Some(format!("Optimization failed: {}", e))
            }
        }
        
        _ => None, // Not a config command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_configuration_system_integration() {
        // Test that the configuration system can be initialized
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        
        let result = config_manager.initialize().await;
        assert!(result.is_ok());
        
        // Test that configuration files were created
        assert!(temp_dir.path().join("filters.yaml").exists());
        assert!(temp_dir.path().join("patterns.yaml").exists());
        assert!(temp_dir.path().join("timers.yaml").exists());
        assert!(temp_dir.path().join("bot.yaml").exists());
    }

    #[tokio::test]
    async fn test_config_integration_with_moderation() {
        let temp_dir = tempdir().unwrap();
        let config_manager = Arc::new(ConfigurationManager::new(temp_dir.path()));
        let bot = ChatBot::new();
        
        config_manager.initialize().await.unwrap();
        
        let config_integration = ConfigIntegration::new(config_manager, bot.get_moderation_system());
        let result = config_integration.initialize().await;
        assert!(result.is_ok());
    }
}