use anyhow::Result;
use log::{debug, error, info, warn};
use tokio::time::{sleep, Duration, Instant};
use std::env;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use notabot::prelude::*;
use notabot::config::ConfigurationManager;
use notabot::bot::config_integration::{ConfigIntegration, ConfigCommands};
use notabot::bot::connection_pool::{ConnectionPool, PoolConfig};
use notabot::bot::shutdown::{GracefulShutdown, ShutdownIntegration, ShutdownConfig};

// adaptive tuning system
use notabot::adaptive::{AdaptivePerformanceSystem, AdaptiveConfig};
use std::sync::atomic::{AtomicBool, Ordering};

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
    // ADAPTIVE PERFORMANCE TUNING SYSTEM INITIALIZATION
    // =================================================================

    info!("Initializing Adaptive Performance Tuning System...");

    // Create adaptive configuration from environment or defaults
    let adaptive_config = AdaptiveConfig {
        enabled: env::var("ADAPTIVE_TUNING_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true),
        tuning_interval_seconds: env::var("ADAPTIVE_TUNING_INTERVAL")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30),
        metrics_retention_hours: env::var("ADAPTIVE_METRICS_RETENTION")
            .unwrap_or_else(|_| "24".to_string())
            .parse()
            .unwrap_or(24),
        safety_checks_enabled: env::var("ADAPTIVE_SAFETY_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true),
        max_parameter_changes_per_hour: env::var("ADAPTIVE_MAX_CHANGES_PER_HOUR")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10),
        rollback_threshold_seconds: env::var("ADAPTIVE_ROLLBACK_THRESHOLD")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .unwrap_or(300),
        learning_mode: env::var("ADAPTIVE_LEARNING_MODE")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false),
        strategies: notabot::adaptive::StrategyConfig {
            latency_tuning: notabot::adaptive::LatencyTuningConfig {
                target_latency_ms: env::var("ADAPTIVE_TARGET_LATENCY")
                    .unwrap_or_else(|_| "100.0".to_string())
                    .parse()
                    .unwrap_or(100.0),
                aggressive_threshold_ms: env::var("ADAPTIVE_AGGRESSIVE_LATENCY_THRESHOLD")
                    .unwrap_or_else(|_| "500.0".to_string())
                    .parse()
                    .unwrap_or(500.0),
                timeout_adjustment_factor: 1.2,
            },
            memory_tuning: notabot::adaptive::MemoryTuningConfig {
                target_memory_percent: env::var("ADAPTIVE_TARGET_MEMORY")
                    .unwrap_or_else(|_| "70.0".to_string())
                    .parse()
                    .unwrap_or(70.0),
                critical_threshold_percent: env::var("ADAPTIVE_CRITICAL_MEMORY")
                    .unwrap_or_else(|_| "90.0".to_string())
                    .parse()
                    .unwrap_or(90.0),
                cache_reduction_factor: 0.8,
            },
            error_rate_tuning: notabot::adaptive::ErrorRateTuningConfig {
                target_error_rate_percent: env::var("ADAPTIVE_TARGET_ERROR_RATE")
                    .unwrap_or_else(|_| "1.0".to_string())
                    .parse()
                    .unwrap_or(1.0),
                critical_error_rate_percent: env::var("ADAPTIVE_CRITICAL_ERROR_RATE")
                    .unwrap_or_else(|_| "5.0".to_string())
                    .parse()
                    .unwrap_or(5.0),
                retry_increase_factor: 1.5,
            },
        },
    };

    // Initialize the adaptive performance system
    let adaptive_system = Arc::new(AdaptivePerformanceSystem::new(adaptive_config.clone())?);

    // Start the adaptive system
    if let Err(e) = adaptive_system.start(adaptive_config.clone()).await {
        error!("Failed to start adaptive performance tuning system: {}", e);
        return Err(e);
    }

    info!("Adaptive Performance Tuning System started successfully");

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
        bot_guard.add_command("hello".to_string(), "Hello $(user)! Welcome to our stream!".to_string(), false, 5).await;
        bot_guard.add_command("uptime".to_string(), "AI moderation system running with connection pooling and graceful shutdown!".to_string(), false, 30).await;
        
        // Add shutdown command for administrators
        bot_guard.add_command("shutdown".to_string(), "Initiating graceful shutdown... (admin only)".to_string(), true, 300).await;
        
        // Add pool statistics command
        bot_guard.add_command("poolstats".to_string(), "Connection pool statistics (mod only)".to_string(), true, 30).await;
        
        // Configuration commands
        bot_guard.add_command("reloadconfig".to_string(), "Configuration management (mod only)".to_string(), true, 60).await;
        bot_guard.add_command("configstatus".to_string(), "Configuration status (mod only)".to_string(), true, 30).await;
    
        // Adaptive system control commands
        bot_guard.add_command("adaptivestatus".to_string(), "Show adaptive performance status (mod only)".to_string(), true, 30).await;
        bot_guard.add_command("adaptivemetrics".to_string(), "Show current performance metrics (mod only)".to_string(), true, 30).await;
        bot_guard.add_command("adaptivetune".to_string(), "Trigger manual tuning cycle (mod only)".to_string(), true, 300).await;
        bot_guard.add_command("adaptiveparams".to_string(), "Show current parameter values (mod only)".to_string(), true, 60).await;
        bot_guard.add_command("adaptivehealth".to_string(), "Show system health status (mod only)".to_string(), true, 60).await;
        bot_guard.add_command("adaptivereset".to_string(), "Reset parameter to default (admin only)".to_string(), true, 600).await;
        bot_guard.add_command("adaptivesafety".to_string(), "Show safety manager status (mod only)".to_string(), true, 60).await;
        bot_guard.add_command("adaptiverollback".to_string(), "Manual parameter rollback (admin only)".to_string(), true, 600).await;

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
    // ENHANCED METRICS COLLECTION WITH SYSTEM INTEGRATION
    // =================================================================

    // Set up comprehensive metrics collection that integrates with all your systems
    let adaptive_metrics_collector = adaptive_system.get_metrics_collector().await?;

    // Start collection from your connection pool
    let pool_metrics_collector = adaptive_metrics_collector.clone();
    let pool_for_metrics = connection_pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = notabot::adaptive::connection_pool_integration::collect_connection_pool_metrics(
                &pool_for_metrics,
                &pool_metrics_collector,
            ).await {
                error!("Failed to collect connection pool metrics: {}", e);
            }
        }
    });

    // Start collection from your moderation system
    let moderation_metrics_collector = adaptive_metrics_collector.clone();
    let moderation_for_metrics = enhanced_moderation.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = notabot::adaptive::moderation_integration::collect_moderation_metrics(
                &moderation_for_metrics.get_base_moderation_system(),
                &moderation_metrics_collector,
            ).await {
                error!("Failed to collect moderation metrics: {}", e);
            }
        }
    });

    // Start collection from your configuration system
    let config_metrics_collector = adaptive_metrics_collector.clone();
    let config_for_metrics = config_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = notabot::adaptive::config_integration::collect_configuration_metrics(
                &config_for_metrics,
                &config_metrics_collector,
            ).await {
                error!("Failed to collect configuration metrics: {}", e);
            }
        }
    });

    // =================================================================
    // ENHANCED MESSAGE PROCESSING WITH ADAPTIVE METRICS
    // =================================================================

    let adaptive_message_processor = adaptive_system.clone();
    let message_processor_shutdown = shutdown_manager.clone();
    let message_processor_config_commands = config_commands.clone();
    let message_processor_enhanced_mod = enhanced_moderation.clone();
    let message_processor_pool = connection_pool.clone();

    tokio::spawn(async move {
        info!("Starting adaptive-aware message processor...");
        let mut message_count = 0;
        let mut processing_times = Vec::new();
        let mut last_metric_update = Instant::now();
        
        loop {
            // Check if shutdown is requested
            if message_processor_shutdown.is_shutdown_requested().await {
                info!("Message processor stopping due to shutdown request");
                break;
            }
            
            // Acquire operation permit
            if let Some(_permit) = message_processor_shutdown.acquire_operation_permit().await {
                let start_time = std::time::Instant::now();
                
                // Get current adaptive parameters for dynamic behavior
                let current_params = adaptive_message_processor.get_current_parameters().await.unwrap_or_default();
                
                // Use adaptive batch size if available
                let batch_size = current_params.get("message_processing_batch_size")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10) as usize;
                
                // Use adaptive response delay
                let response_delay = current_params.get("response_delay_ms")
                    .and_then(|v| v.as_duration_ms())
                    .unwrap_or(100);
                
                // Process messages in adaptive batches
                for _ in 0..batch_size {
                    // Simulate processing one message with adaptive delay
                    tokio::time::sleep(Duration::from_millis(response_delay / batch_size as u64)).await;
                    message_count += 1;
                    
                    // Simulate different processing complexities
                    let complexity_delay = match message_count % 10 {
                        0 => 50, // Complex AI processing
                        1..=2 => 20, // Moderation checks
                        _ => 5, // Simple commands
                    };
                    tokio::time::sleep(Duration::from_millis(complexity_delay)).await;
                }
                
                let processing_time = start_time.elapsed();
                processing_times.push(processing_time.as_millis() as f64);
                
                // Record comprehensive metrics for adaptive system
                if let Err(e) = adaptive_message_processor.record_metric("message_processing_latency", processing_time.as_millis() as f64).await {
                    error!("Failed to record processing latency: {}", e);
                }
                
                if let Err(e) = adaptive_message_processor.record_metric("messages_processed_per_batch", batch_size as f64).await {
                    error!("Failed to record batch size metric: {}", e);
                }
                
                // Update system metrics periodically
                if last_metric_update.elapsed() >= Duration::from_secs(30) {
                    let avg_processing_time = processing_times.iter().sum::<f64>() / processing_times.len() as f64;
                    let throughput = (batch_size as f64 * 1000.0) / avg_processing_time; // messages per second
                    
                    // Record aggregated metrics
                    if let Err(e) = adaptive_message_processor.record_metric("message_throughput", throughput).await {
                        error!("Failed to record throughput: {}", e);
                    }
                    
                    if let Err(e) = adaptive_message_processor.record_metric("processing_efficiency", 1000.0 / avg_processing_time).await {
                        error!("Failed to record efficiency: {}", e);
                    }
                    
                    // Simulate memory usage tracking
                    let memory_usage = 40.0 + (message_count % 1000) as f64 / 20.0; // Simulate 40-90% usage
                    if let Err(e) = adaptive_message_processor.record_metric("memory_usage", memory_usage).await {
                        error!("Failed to record memory usage: {}", e);
                    }
                    
                    // Simulate error rate tracking
                    let error_rate = if message_count % 100 == 0 { 2.0 } else { 0.5 }; // Periodic errors
                    if let Err(e) = adaptive_message_processor.record_metric("error_rate", error_rate).await {
                        error!("Failed to record error rate: {}", e);
                    }
                    
                    // Clear old processing times to keep memory usage bounded
                    if processing_times.len() > 100 {
                        processing_times.drain(0..50);
                    }
                    
                    last_metric_update = Instant::now();
                    
                    debug!("Processed {} messages, avg latency: {:.2}ms, throughput: {:.2} msg/s, memory: {:.1}%", 
                        message_count, avg_processing_time, throughput, memory_usage);
                }
                
                // Adaptive sleep based on current system load and performance
                let base_sleep = Duration::from_millis(100);
                let adaptive_sleep = if processing_time > Duration::from_millis(500) {
                    base_sleep / 2 // Process faster if we're slow
                } else if processing_time < Duration::from_millis(50) {
                    base_sleep * 2 // Process slower if we're fast (save resources)
                } else {
                    base_sleep
                };
                
                sleep(adaptive_sleep).await;
            } else {
                debug!("Skipping message processing due to shutdown");
                break;
            }
        }
        
        info!("Adaptive message processor stopped gracefully after processing {} messages", message_count);
    });

    // =================================================================
    // ADAPTIVE PERFORMANCE MONITORING WITH SYSTEM INTEGRATION
    // =================================================================

    // Enhanced monitoring that integrates with all your systems
    let adaptive_monitor = adaptive_system.clone();
    let monitor_shutdown = shutdown_manager.clone();
    let monitor_connection_pool = connection_pool.clone();
    let monitor_enhanced_moderation = enhanced_moderation.clone();
    let monitor_config_manager = config_manager.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        let mut cycle_count = 0;
        
        info!("Starting comprehensive adaptive performance monitor...");
        
        loop {
            interval.tick().await;
            
            if monitor_shutdown.is_shutdown_requested().await {
                info!("Adaptive monitor received shutdown signal");
                break;
            }
            
            cycle_count += 1;
            
            // Get comprehensive performance metrics from adaptive system
            match adaptive_monitor.get_performance_metrics().await {
                Ok(metrics) => {
                    // Log key performance indicators
                    info!("Adaptive Metrics (cycle {}): Latency {:.1}ms, Memory {:.1}%, Errors {:.2}%, Health {:.1}%",
                        cycle_count,
                        metrics.average_latency_ms,
                        metrics.memory_usage_percent,
                        metrics.error_rate_percent,
                        metrics.system_health_score * 100.0);
                    
                    // Get connection pool utilization from your existing system
                    let pool_stats = monitor_connection_pool.get_stats().await;
                        for (platform, stats) in pool_stats {
                            let utilization = if stats.total_connections > 0 {
                                stats.active_connections as f64 / stats.total_connections as f64
                            } else {
                                0.0
                            };
                            
                            debug!("Adaptive Pool {}: {:.1}% utilization, {:.1}ms avg response, {} failures",
                                platform, utilization * 100.0, stats.average_response_time_ms, stats.failed_connections);
                        }
                    
                    
                    // Check for performance issues and trigger manual tuning if needed
                    if metrics.system_health_score < 0.7 {
                        warn!("Low system health detected ({:.1}%), triggering manual tuning cycle", 
                            metrics.system_health_score * 100.0);
                        
                        match adaptive_monitor.trigger_tuning_cycle().await {
                            Ok(result) => {
                                info!("🎯 Auto-tuning completed: {} parameters adjusted, {:.2}% improvement",
                                    result.changes.len(), result.performance_improvement * 100.0);
                                
                                // Log specific changes made
                                for change in &result.changes {
                                    info!("🔧 Adaptive change: {} -> {} (reason: {})",
                                        change.parameter_name, change.new_value, change.reason);
                                }
                            }
                            Err(e) => {
                                error!("Auto-tuning failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get adaptive performance metrics: {}", e);
                }
            }
            
            // Every 5 cycles, get detailed system statistics
            if cycle_count % 5 == 0 {
                match adaptive_monitor.get_statistics().await {
                    Ok(stats) => {
                        info!("📊 Adaptive System: {} tuning cycles, {} active parameters, {:.2}% optimization level",
                            stats.total_tuning_cycles,
                            stats.active_parameters,
                            stats.current_optimization_level * 100.0);
                    }
                    Err(e) => {
                        error!("Failed to get adaptive statistics: {}", e);
                    }
                }
                
                // Get health status with safety information
                match adaptive_monitor.get_health_status().await {
                    Ok(health) => {
                        if health.overall_health < 0.8 {
                            warn!("🏥 Adaptive system health is below optimal: {:.1}% (safety warnings: {:?})",
                                health.overall_health * 100.0, health.safety_status.warnings);
                        }
                        
                        info!("🛡️ Safety Status: Circuit Breaker {:?}, {} recent changes, {} rollbacks",
                            health.safety_status.circuit_breaker_state,
                            health.safety_status.recent_changes,
                            health.safety_status.rollbacks_in_last_hour);
                    }
                    Err(e) => {
                        error!("Failed to get adaptive health status: {}", e);
                    }
                }
            }
        }
        
        info!("Comprehensive adaptive performance monitor stopped");
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
            info!("Dashboard: http://localhost:{}", dashboard_port);
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

    info!("NotaBot System Started Successfully!");
    info!("Features active:");
    info!("  - Hot-reload configuration management");
    info!("  - Connection pooling with health monitoring");
    info!("  - Graceful shutdown with component coordination");
    info!("  - AI-powered moderation with real-time learning");
    info!("  - Adaptive Performance Tuning (5 strategies active)");
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
            info!("NotaBot running strong - Connection pooling + Graceful shutdown enabled!");
        }
    }

    // =================================================================
    // ENHANCED GRACEFUL SHUTDOWN WITH ADAPTIVE SYSTEM
    // =================================================================

    info!("Starting graceful shutdown sequence...");

    // Stop adaptive system first to prevent new parameter changes during shutdown
    if let Err(e) = adaptive_system.stop().await {
        error!("Failed to stop adaptive system cleanly: {}", e);
    } else {
        info!("🤖 Adaptive performance tuning system stopped");
    }

    // Export final adaptive state before shutdown
    match adaptive_system.export_state().await {
        Ok(state) => {
            let final_state_file = "data/adaptive_final_state.json";
            if let Ok(state_json) = serde_json::to_string_pretty(&state) {
                if let Err(e) = tokio::fs::write(final_state_file, state_json).await {
                    error!("Failed to save final adaptive state: {}", e);
                } else {
                    info!("📊 Final adaptive state saved to {}", final_state_file);
                }
            }
        }
        Err(e) => {
            error!("Failed to export final adaptive state: {}", e);
        }
    }

        
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

    info!("NotaBot shutdown complete. Goodbye!");
    Ok(())
}

// Helper function for shutdown-aware message processing
async fn process_message_with_shutdown_protection(
    message: ChatMessage,
    shutdown_manager: &GracefulShutdown,
    config_commands: &ConfigCommands,
    enhanced_moderation: &Arc<EnhancedModerationSystem>,
    adaptive_system: &Arc<AdaptivePerformanceSystem>,
    config_manager: &Arc<ConfigurationManager>,
    connection_pool: &Arc<ConnectionPool>,
) -> Option<String> {
    // Get operation permit to ensure we don't start processing during shutdown
    let _permit = shutdown_manager.acquire_operation_permit().await?;
    
    // Handle configuration commands
    if let Some(response) = handle_config_commands(
        &message, 
        config_commands, 
        enhanced_moderation,
        &adaptive_system,     // Add this
        &config_manager,      // Add this  
        &connection_pool      // Add this
    ).await {
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
    enhanced_moderation: &Arc<EnhancedModerationSystem>,
    adaptive_system: &Arc<AdaptivePerformanceSystem>,
    config_manager: &Arc<ConfigurationManager>,
    connection_pool: &Arc<ConnectionPool>    
) -> Option<String> {

    if let Some(response) = handle_adaptive_commands(message, adaptive_system, config_manager, connection_pool).await {
        return Some(response);
    }

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

// Add this function after handle_config_commands
async fn handle_adaptive_commands(
    message: &ChatMessage,
    adaptive_system: &Arc<AdaptivePerformanceSystem>,
    config_manager: &Arc<ConfigurationManager>,
    connection_pool: &Arc<ConnectionPool>,
) -> Option<String> {
    if !message.content.starts_with("!") || !message.is_mod {
        return None;
    }
    
    let parts: Vec<&str> = message.content[1..].split_whitespace().collect();
    let command = parts.first()?;
    let args = &parts[1..];
    
    match *command {
        "adaptivestatus" => {
            match adaptive_system.get_health_status().await {
                Ok(health) => {
                    Some(format!(
                        "🤖 Adaptive Status: Health {:.1}%, Optimization {:.1}%, Safety: {}, {} active parameters | Circuit Breaker: {:?}",
                        health.overall_health * 100.0,
                        health.metrics_health * 100.0,
                        if health.safety_status.is_safe { "✅ OK" } else { "⚠️ WARNING" },
                        health.active_parameters,
                        health.safety_status.circuit_breaker_state
                    ))
                }
                Err(e) => Some(format!("❌ Status error: {}", e)),
            }
        }
        
        "adaptivemetrics" => {
            match adaptive_system.get_performance_metrics().await {
                Ok(metrics) => {
                    Some(format!(
                        "📊 Metrics: Latency {:.1}ms (p95: {:.1}ms), Memory {:.1}%, Errors {:.2}%, Throughput {:.1} msg/s, Pool {:.1}% util",
                        metrics.average_latency_ms,
                        metrics.p95_latency_ms,
                        metrics.memory_usage_percent,
                        metrics.error_rate_percent,
                        metrics.messages_per_second,
                        metrics.connection_pool_utilization * 100.0
                    ))
                }
                Err(e) => Some(format!("❌ Metrics error: {}", e)),
            }
        }
        
        "adaptivetune" => {
            match adaptive_system.trigger_tuning_cycle().await {
                Ok(result) => {
                    if result.changes.is_empty() {
                        Some("✨ Tuning completed: No adjustments needed - system is optimally configured!".to_string())
                    } else {
                        Some(format!(
                            "⚡ Tuning completed: {} parameters adjusted, {:.2}% improvement ({}ms) | Strategy: {}",
                            result.changes.len(),
                            result.performance_improvement * 100.0,
                            result.duration_ms,
                            result.summary.dominant_strategy
                        ))
                    }
                }
                Err(e) => Some(format!("❌ Tuning failed: {}", e)),
            }
        }
        
        "adaptiveparams" => {
            match adaptive_system.get_current_parameters().await {
                Ok(params) => {
                    let mut response = format!("🔧 Active Parameters ({}):\n", params.len());
                    for (name, value) in params.iter().take(5) { // Show first 5
                        response.push_str(&format!("  {} = {}\n", name, value));
                    }
                    if params.len() > 5 {
                        response.push_str(&format!("  ... and {} more. Use web dashboard for full view.", params.len() - 5));
                    }
                    Some(response)
                }
                Err(e) => Some(format!("❌ Parameters error: {}", e)),
            }
        }
        
        "adaptivehealth" => {
            match adaptive_system.get_health_status().await {
                Ok(health) => {
                    let safety_status = &health.safety_status;
                    Some(format!(
                        "🏥 Health: Overall {:.1}%, Metrics {:.1}%, Safety {}, Changes: {}/hr, Last tuning: {}s ago",
                        health.overall_health * 100.0,
                        health.metrics_health * 100.0,
                        if safety_status.is_safe { "✅ SAFE" } else { "⚠️ UNSAFE" },
                        safety_status.recent_changes,
                        (chrono::Utc::now() - health.last_tuning_cycle).num_seconds().abs()
                    ))
                }
                Err(e) => Some(format!("❌ Health check error: {}", e)),
            }
        }
        
        "adaptivesafety" => {
            match adaptive_system.get_health_status().await {
                Ok(health) => {
                    let safety = &health.safety_status;
                    Some(format!(
                        "Safety: {} | CB: {:?} | Score: {:.2} | Rollbacks: {} | Warnings: {}",
                        if safety.is_safe { "SAFE" } else { "UNSAFE" },
                        safety.circuit_breaker_state,
                        safety.safety_score,
                        safety.rollbacks_in_last_hour,
                        safety.warnings.len()
                    ))
                }
                Err(e) => Some(format!("❌ Safety check error: {}", e)),
            }
        }
        
        "adaptivereset" => {
            if !message.is_mod {
                return Some("This command requires administrator privileges.".to_string());
            }
            
            let param_name = args.first().unwrap_or(&"");
            if param_name.is_empty() {
                return Some("Usage: !adaptivereset <parameter_name>".to_string());
            }
            
            Some(format!("Would reset parameter '{}' to default value", param_name))
        }
        
        "adaptiverollback" => {
            if !message.is_mod {
                return Some("This command requires administrator privileges.".to_string());
            }
            
            let param_name = args.first().unwrap_or(&"");
            if param_name.is_empty() {
                return Some("Usage: !adaptiverollback <parameter_name> [reason]".to_string());
            }
            
            let reason = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "Manual admin rollback".to_string()
            };
            
            Some(format!("↩Would rollback parameter '{}' (reason: {})", param_name, reason))
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
        
        assert_eq!(shutdown_manager.get_phase().await, notabot::bot::shutdown::ShutdownPhase::Running);
        
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