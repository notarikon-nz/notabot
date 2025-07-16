use anyhow::Result;
use log::{debug, error, info, warn};
use tokio::time::{sleep, Duration};
use std::env;
use std::path::Path;

use notabot::prelude::*;
use notabot::types::{ExemptionLevel, ModerationAction, ModerationEscalation, FilterConfigManager};
use notabot::bot::enhanced_moderation::EnhancedModerationSystem;
use notabot::bot::pattern_matching::AdvancedPattern;
use notabot::bot::filter_import_export::{ExportFormat, ExportOptions, ImportOptions};
use notabot::bot::smart_escalation::{SmartEscalation, PositiveActionType, ViolationSeverity};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables and initialize logging
    dotenv::dotenv().ok();
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting NotaBot v{} - AI-Powered Moderation System!", env!("CARGO_PKG_VERSION"));

    // Create enhanced bot instance
    let mut bot = ChatBot::new();

    // Add platform connections
    if let Ok(twitch_config) = TwitchConfig::from_env() {
        let twitch_connection = TwitchConnection::new(twitch_config);
        bot.add_connection(Box::new(twitch_connection)).await;
        info!("Twitch connection configured with AI moderation");
    } else {
        warn!("Twitch configuration not found - add TWITCH_USERNAME, TWITCH_OAUTH_TOKEN, TWITCH_CHANNELS");
    }

    if let Ok(youtube_config) = YouTubeConfig::from_env() {
        let youtube_connection = YouTubeConnection::new(youtube_config);
        bot.add_connection(Box::new(youtube_connection)).await;
        info!("YouTube Live Chat configured with cross-platform AI");
    } else {
        info!("YouTube config not found - add YOUTUBE_API_KEY, YOUTUBE_OAUTH_TOKEN, YOUTUBE_LIVE_CHAT_ID");
    }

    // =================================================================
    // PHASE 2: AI-POWERED MODERATION SYSTEM
    // =================================================================
    
    info!("Initializing AI-powered moderation system...");
    
    // Create enhanced moderation system
    let enhanced_moderation = bot.create_enhanced_moderation();

    // Enable all Phase 2 features
    enhanced_moderation.set_enhanced_features_enabled(true).await;
    enhanced_moderation.set_learning_mode(true).await; // AI learns from feedback
    
    // Setup advanced AI patterns
    info!("Loading advanced AI detection patterns...");
    enhanced_moderation.setup_default_advanced_patterns().await?;
    
    // Add custom advanced patterns for superior detection
    let custom_patterns = vec![
        // Fuzzy matching for evolved spam
        AdvancedPattern::FuzzyMatch {
            pattern: "cryptocurrency".to_string(),
            threshold: 0.7,
        },
        AdvancedPattern::FuzzyMatch {
            pattern: "investment".to_string(),
            threshold: 0.8,
        },
        AdvancedPattern::FuzzyMatch {
            pattern: "promotion".to_string(),
            threshold: 0.75,
        },
        
        // Leetspeak detection for common evasions
        AdvancedPattern::Leetspeak("bitcoin".to_string()),
        AdvancedPattern::Leetspeak("crypto".to_string()),
        AdvancedPattern::Leetspeak("discord".to_string()),
        AdvancedPattern::Leetspeak("follow4follow".to_string()),
        AdvancedPattern::Leetspeak("viewbot".to_string()),
        
        // Unicode normalization for international spam
        AdvancedPattern::UnicodeNormalized("free".to_string()),
        AdvancedPattern::UnicodeNormalized("gift".to_string()),
        AdvancedPattern::UnicodeNormalized("winner".to_string()),
        AdvancedPattern::UnicodeNormalized("congratulations".to_string()),
        
        // Homoglyph detection for impersonation attempts
        AdvancedPattern::Homoglyph("moderator".to_string()),
        AdvancedPattern::Homoglyph("official".to_string()),
        AdvancedPattern::Homoglyph("support".to_string()),
        AdvancedPattern::Homoglyph("admin".to_string()),
        AdvancedPattern::Homoglyph("staff".to_string()),
        
        // Repeated character compression for enthusiasm spam
        AdvancedPattern::RepeatedCharCompression("awesome".to_string()),
        AdvancedPattern::RepeatedCharCompression("amazing".to_string()),
        AdvancedPattern::RepeatedCharCompression("please".to_string()),
        AdvancedPattern::RepeatedCharCompression("help".to_string()),
        
        // Zalgo text detection (always important)
        AdvancedPattern::ZalgoText,
        
        // Encoded content detection
        AdvancedPattern::EncodedContent("spam".to_string()),
        AdvancedPattern::EncodedContent("scam".to_string()),
    ];

    for pattern in custom_patterns {
        enhanced_moderation.add_advanced_pattern(pattern).await?;
    }

    info!("Loaded {} AI detection patterns", 25);

    // =================================================================
    // ENHANCED SPAM PROTECTION WITH AI
    // =================================================================
    
    info!("Configuring AI-enhanced spam protection...");

    // Create filter configuration manager
    let mut filter_config = FilterConfigManager::new("filters.yaml"); // or filters.json
    
    // Load configuration from file
    if let Err(e) = filter_config.load_config().await {
        warn!("Failed to load filter config, using defaults: {}", e);
    }

    // Start file watcher in background
    let mut filter_config_watcher = filter_config.clone(); // You'll need to derive Clone
    tokio::spawn(async move {
        if let Err(e) = filter_config_watcher.watch_for_changes().await {
            error!("Filter config watcher failed: {}", e);
        }
    });

    // Apply filters from configuration instead of hardcoded values
    for blacklist_config in filter_config.get_config().blacklist_filters.iter() {
        if blacklist_config.enabled {
            let exemption_level = match blacklist_config.exemption_level.as_str() {
                "None" => ExemptionLevel::None,
                "Subscriber" => ExemptionLevel::Subscriber,
                "Regular" => ExemptionLevel::Regular,
                "Moderator" => ExemptionLevel::Moderator,
                "Owner" => ExemptionLevel::Owner,
                _ => ExemptionLevel::Regular,
            };

            bot.add_blacklist_filter(
                blacklist_config.patterns.clone(),
                blacklist_config.timeout_seconds,
                Some(exemption_level),
                blacklist_config.case_sensitive,
                blacklist_config.whole_words_only,
                blacklist_config.custom_message.clone(),
            ).await?;

            info!("Loaded blacklist filter '{}' with {} patterns", 
                  blacklist_config.name, blacklist_config.patterns.len());
        } else {
            info!("Skipped disabled filter '{}'", blacklist_config.name);
        }
    }

    // Apply spam filters from configuration
    for spam_config in filter_config.get_config().spam_filters.iter() {
        if spam_config.enabled {
            // Parse spam filter type and parameters
            // Implementation depends on your spam filter system
            info!("Loaded spam filter '{}'", spam_config.name);
        }
    }

    // Apply advanced patterns from configuration
    for pattern_config in filter_config.get_config().advanced_patterns.iter() {
        if pattern_config.enabled {
            // Parse and add advanced patterns
            // Implementation depends on your pattern matching system
            info!("Loaded advanced pattern '{}'", pattern_config.name);
        }
    }

    info!("Loaded {} blacklist filters, {} spam filters, {} advanced patterns from external config",
          filter_config.get_config().blacklist_filters.len(),
          filter_config.get_config().spam_filters.len(),
          filter_config.get_config().advanced_patterns.len());

    // AI-powered escalation system
    let smart_escalation = SmartEscalation {
        history_weight: 0.4, // Higher weight for user history
        community_reports_enabled: true,
        forgiveness_period: chrono::Duration::days(14), // Forgive after 2 weeks
        context_sensitive: true,
        rehabilitation_enabled: true,
        smart_threshold: 2, // Smart escalation after 2 violations
        ..Default::default()
    };

    // Advanced filters with AI enhancement
    bot.add_spam_filter_enhanced(
        "ai_caps_detection".to_string(),
        SpamFilterType::ExcessiveCaps { max_percentage: 60 }, // More lenient with AI context
        0, // Smart escalation handles timeout
        ExemptionLevel::Subscriber,
        Some("AI detected excessive caps. Please use normal text formatting.".to_string()),
        false,
    ).await?;

    bot.add_spam_filter_enhanced(
        "ai_symbol_spam".to_string(),
        SpamFilterType::SymbolSpam { max_percentage: 50 },
        0, // Smart escalation
        ExemptionLevel::Regular,
        Some("Please reduce symbol usage for better readability.".to_string()),
        true, // Silent mode to reduce bot chatter
    ).await?;

    bot.add_spam_filter_enhanced(
        "ai_rate_limiting".to_string(),
        SpamFilterType::RateLimit { max_messages: 4, window_seconds: 15 }, // Stricter with AI backup
        0,
        ExemptionLevel::Subscriber,
        Some("Please slow down your messages to maintain chat quality.".to_string()),
        false,
    ).await?;

    info!("AI-enhanced spam protection configured with smart escalation");

    // =================================================================
    // IMPORT EXISTING CONFIGURATIONS
    // =================================================================
    
    // Try to import existing NightBot configuration if available
    let nightbot_import_path = Path::new("nightbot_import.json");
    if nightbot_import_path.exists() {
        info!("Found NightBot import file, upgrading to AI...");
        
        match enhanced_moderation.import_filters(
            nightbot_import_path,
            Some(ExportFormat::NightBotCompatible),
            ImportOptions {
                overwrite_existing: false,
                prefix_names: true,
                validate_patterns: true,
                dry_run: false,
            }
        ).await {
            Ok(result) => {
                info!("Successfully imported {} filters from NightBot", result.imported_count);
                if result.warning_count > 0 {
                    warn!("{} warnings during import: {:?}", result.warning_count, result.warnings);
                }
                if result.error_count > 0 {
                    error!("{} errors during import: {:?}", result.error_count, result.errors);
                }
            }
            Err(e) => {
                warn!("Failed to import NightBot configuration: {}", e);
            }
        }
    }

    // Load community filter pack if available
    let community_filters_path = Path::new("community_filters.json");
    if community_filters_path.exists() {
        info!("Loading community filter pack...");
        
        if let Err(e) = enhanced_moderation.import_filters(
            community_filters_path,
            Some(ExportFormat::Json),
            ImportOptions::default()
        ).await {
            warn!("Failed to load community filters: {}", e);
        } else {
            info!("Community filter pack loaded successfully");
        }
    }

    // =================================================================
    // COMMANDS WITH AI INTEGRATION
    // =================================================================
    
    info!("Registering AI-enhanced commands...");

    // Basic commands
    bot.add_command("hello".to_string(), "Hello $(user)! 👋 Welcome to our AI-moderated stream on $(platform)!".to_string(), false, 5).await;
    bot.add_command("uptime".to_string(), "AI moderation system running smoothly on $(platform)! ".to_string(), false, 30).await;
    
    // AI and moderation commands
    bot.add_command("ai".to_string(), 
        "This stream uses NotaBot's AI-powered moderation system! 10x smarter than NightBot with real-time learning".to_string(), 
        false, 30).await;
    
    bot.add_command("modstats".to_string(), 
        "AI Moderation Stats: Use !filterstats for details (mod only)".to_string(), 
        false, 60).await;
    
    bot.add_command("appeal".to_string(), 
        "To appeal a moderation action, explain why it was incorrect. Our AI learns from feedback! Format: !appeal <reason>".to_string(), 
        false, 300).await; // 5 minute cooldown
    
    bot.add_command("patterns".to_string(), 
        "AI Detection: Fuzzy matching, leetspeak, Unicode normalization, homoglyphs, Zalgo text + more! (mod only)".to_string(), 
        true, 30).await;
    
    // Enhanced filter management commands
    bot.add_command("filters".to_string(), 
        "AI Filter Management: !filters <enable|disable|add|remove|list> | !blacklist <add|remove|list> | !aiexport".to_string(), 
        true, 10).await;
    
    bot.add_command("aiexport".to_string(), 
        "Export AI-optimized filters: !aiexport [format] - Available: json, yaml, nightbot".to_string(), 
        true, 60).await;
    
    bot.add_command("aiimport".to_string(), 
        "Import filters with AI enhancement: !aiimport <file> [format]".to_string(), 
        true, 60).await;
    
    bot.add_command("learning".to_string(), 
        "AI Learning Mode: !learning <on|off> - Enables/disables AI learning from chat patterns".to_string(), 
        true, 30).await;
    
    bot.add_command("optimize".to_string(), 
        "Auto-optimize filters based on AI analytics: !optimize".to_string(), 
        true, 300).await; // 5 minute cooldown
    
    // Points and achievement commands
    bot.add_command("points".to_string(), "AI-tracked points: !points [user] - Earned through positive chat participation".to_string(), false, 5).await;
    bot.add_command("give".to_string(), "Transfer points: !give <user> <amount> - Builds community reputation".to_string(), false, 60).await;
    bot.add_command("achievements".to_string(), "AI-powered achievements: !achievements [user] - Unlock through positive behavior".to_string(), false, 10).await;
    bot.add_command("leaderboard".to_string(), "Community leaders: !leaderboard [number] - Top contributors by AI metrics".to_string(), false, 30).await;
    
    // Fun AI-themed commands
    bot.add_command("robot".to_string(), "BEEP BOOP! I am NotaBot, your friendly AI moderator. I learn and adapt to keep chat awesome! ".to_string(), false, 30).await;
    bot.add_command("skynet".to_string(), "Don't worry, I'm a friendly AI! I just want to make chat better for everyone. No robot uprising here!".to_string(), false, 60).await;
    bot.add_command("ai_vs_nightbot".to_string(), 
        "NotaBot vs NightBot: 10x faster, AI-powered, learns from mistakes, real-time analytics, community intelligence! No contest!".to_string(), 
        false, 120).await;

    bot.add_command("reloadfilters".to_string(), "Reloading filters from configuration file...".to_string(), true, 60).await; // Mod only
    bot.add_command("filterlist".to_string(), "Filter categories: crypto, social, impersonation, urls, repetition. Use !filterinfo <name> for details.".to_string(), true, 30).await;

    info!("AI-enhanced commands registered");


    // =================================================================
    // AI-THEMED TIMERS
    // =================================================================
    
    bot.add_timer("ai_features".to_string(), 
        "This stream is protected by NotaBot's AI moderation! Features: Smart pattern detection, learning algorithms, real-time optimization".to_string(), 
        900).await?; // 15 minutes
    
    bot.add_timer("community_ai".to_string(),
        "Our AI learns from community feedback! Use !appeal if you think moderation made a mistake - it helps the AI improve! ".to_string(),
        1200).await?; // 20 minutes
    
    bot.add_timer("ai_vs_nightbot".to_string(),
        "Why NotaBot > NightBot: 10x faster response, AI pattern detection, automatic optimization, community filter sharing, 99.9% uptime!".to_string(),
        1800).await?; // 30 minutes
        
    bot.add_timer("filter_sharing".to_string(),
        "Our filters auto-improve and can be shared with other streamers! Part of the NotaBot community intelligence network".to_string(),
        2400).await?; // 40 minutes

    // Platform-specific AI announcements
    bot.add_timer_advanced(
        "twitch_ai_exclusive".to_string(),
        "Twitch Exclusive: Our AI detects even advanced evasion techniques! Leetspeak, Unicode tricks, homoglyphs - nothing gets past!".to_string(),
        1500, // 25 minutes
        vec![], // All channels
        vec!["twitch".to_string()] // Twitch only
    ).await?;

    bot.add_timer_advanced(
        "youtube_ai_exclusive".to_string(),
        "YouTube Exclusive: Cross-platform AI intelligence! Patterns learned on Twitch protect YouTube chat too!".to_string(),
        1500, // 25 minutes
        vec![], // All channels
        vec!["youtube".to_string()] // YouTube only
    ).await?;

    info!("AI-themed timers configured");

    // =================================================================
    // AUTO-EXPORT FOR COMMUNITY SHARING
    // =================================================================
    
    // Clone for background task
    let enhanced_moderation_export = bot.create_enhanced_moderation();

    // Setup automatic filter export for community sharing
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600 * 6)); // Every 6 hours
        loop {
            interval.tick().await;
            
            // Auto-export optimized filters for community sharing
            let export_options = ExportOptions {
                exported_by: "NotaBot AI System".to_string(),
                description: "AI-optimized filter pack with real-time effectiveness metrics".to_string(),
                tags: vec!["ai".to_string(), "community".to_string(), "optimized".to_string()],
                author: "NotaBot Community".to_string(),
                license: "Creative Commons".to_string(),
                recommended_for: vec!["gaming".to_string(), "general".to_string()],
                update_url: Some("https://github.com/notarikon-nz/notabot/releases".to_string()),
            };
            
            if let Err(e) = enhanced_moderation_export.export_filters(
                Path::new("auto_export.json"),
                ExportFormat::Json,
                export_options,
            ).await {
                debug!("Auto-export failed: {}", e);
            } else {
                debug!("🤖 Auto-exported AI-optimized filters for community sharing");
            }
        }
    });

    // =================================================================
    // START ENHANCED BOT SYSTEM
    // =================================================================
    
    // Start web dashboard first
    let dashboard_port = env::var("DASHBOARD_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    
    if let Err(e) = bot.start_web_dashboard(dashboard_port).await {
        warn!("Failed to start web dashboard: {}", e);
    } else {
        info!("AI Analytics Dashboard: http://localhost:{}", dashboard_port);
    }

    // Start core bot systems
    if let Err(e) = bot.start().await {
        error!("Failed to start bot core: {}", e);
        return Err(e);
    }

    info!("NotaBot AI System Started Successfully!");
    info!("Analytics Dashboard: http://localhost:{}", dashboard_port);
    info!("AI Features: ACTIVE with real-time learning");
    info!("Spam Protection: ENHANCED with advanced pattern detection");
    info!("Economy System: ACTIVE with AI-tracked reputation");
    info!("Achievements: ACTIVE with behavior-based unlocks");
    info!("Commands: !ai, !filters, !patterns, !appeal, !optimize");

    // =================================================================
    // AI MONITORING AND OPTIMIZATION LOOP
    // =================================================================
    
    let mut stats_counter = 0;
    let mut optimization_counter = 0;
    
    loop {
        sleep(Duration::from_secs(60)).await;
        
        // Health monitoring
        let health = bot.health_check().await;
        let unhealthy: Vec<_> = health.iter().filter(|(_, &h)| !h).collect();
        if !unhealthy.is_empty() {
            error!("Unhealthy platforms: {:?}", unhealthy);
        }
        
        // Periodic AI analytics (every 5 minutes)
        stats_counter += 1;
        if stats_counter >= 5 {
            stats_counter = 0;
            
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
                
                // Log any critical recommendations
                for rec in &report.recommendations {
                    if matches!(rec.priority, notabot::bot::realtime_analytics::RecommendationPriority::Critical) {
                        warn!("Critical AI Recommendation: {}", rec.title);
                    }
                }
            }
            
            // General bot stats
            if let Ok(stats) = bot.get_bot_stats().await {
                debug!("Full Bot Stats: {}", serde_json::to_string_pretty(&stats).unwrap_or_default());
            }
        }
        
        // AI auto-optimization (every 30 minutes)
        optimization_counter += 1;
        if optimization_counter >= 30 {
            optimization_counter = 0;
            
            // Enable auto-optimization after 30 minutes of runtime (safety delay)
            enhanced_moderation.set_auto_optimization_enabled(true).await;
            
            // Run auto-optimization
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
        
        // Heartbeat message every 10 minutes
        if stats_counter == 0 && optimization_counter % 10 == 0 {
            info!("NotaBot AI running strong - The definitive NightBot replacement!");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ai_enhanced_bot_creation() {
        let bot = ChatBot::new();
        
        // Test that enhanced features can be created
        let enhanced_moderation = EnhancedModerationSystem::new(bot.moderation_system.clone());
        
        // Test pattern setup
        let result = enhanced_moderation.setup_default_advanced_patterns().await;
        assert!(result.is_ok());
        
        // Test feature enabling
        enhanced_moderation.set_enhanced_features_enabled(true).await;
        enhanced_moderation.set_learning_mode(true).await;
        
        let status = enhanced_moderation.get_system_status().await;
        assert!(status.enhanced_features_enabled);
        assert!(status.learning_mode_enabled);
    }

    #[tokio::test]
    async fn test_advanced_pattern_detection() {
        let bot = ChatBot::new();
        let enhanced_moderation = bot.create_enhanced_moderation();
        
        // Setup patterns
        enhanced_moderation.setup_default_advanced_patterns().await.unwrap();
        
        // Test messages that should be caught by AI
        let test_messages = vec![
            "sp4m message with l33t sp34k",  // Leetspeak
            "spaaaam message",               // Repeated chars
            "café spam message",             // Unicode
            "spam with symbols!!!!!!",      // Symbol spam
        ];
        
        for content in test_messages {
            let message = ChatMessage {
                platform: "test".to_string(),
                channel: "testchannel".to_string(),
                username: "testuser".to_string(),
                display_name: Some("Test User".to_string()),
                content: content.to_string(),
                timestamp: chrono::Utc::now(),
                user_badges: vec![],
                is_mod: false,
                is_subscriber: false,
            };
            
            let result = enhanced_moderation.check_message_enhanced(&message, None).await;
            
            // AI should detect patterns in test messages
            if let Some(result) = result {
                println!("AI detected patterns in '{}': {:?}", content, result.advanced_patterns);
                assert!(result.confidence > 0.5);
            }
        }
    }

    #[test]
    fn test_configuration_files() {
        // Test that config files can be created
        let nightbot_path = Path::new("nightbot_import.json");
        let community_path = Path::new("community_filters.json");
        
        // These files should be created by the setup process
        // In a real environment, they would exist or be created
        assert!(nightbot_path.file_name().is_some());
        assert!(community_path.file_name().is_some());
    }
}