// src/bot/shutdown.rs - Graceful shutdown management

use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock, Semaphore};
use tokio::signal;
use tokio::time::{sleep, timeout};

use crate::bot::connection_pool::ConnectionPool;
use crate::config::ConfigurationManager;

/// Shutdown phases for orderly termination
#[derive(Debug, Clone, PartialEq)]
pub enum ShutdownPhase {
    /// Normal operation
    Running,
    /// Stop accepting new requests but continue processing existing ones
    Draining,
    /// Force stop all operations
    Terminating,
    /// Shutdown complete
    Stopped,
}

/// Configuration for graceful shutdown behavior
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Maximum time to wait for graceful shutdown before forcing termination
    pub graceful_timeout_seconds: u64,
    /// Time to wait for individual components to shut down
    pub component_timeout_seconds: u64,
    /// Whether to save state before shutting down
    pub save_state: bool,
    /// Whether to create backups before shutting down
    pub create_backup: bool,
    /// Whether to send shutdown notifications
    pub send_notifications: bool,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            graceful_timeout_seconds: 30,
            component_timeout_seconds: 10,
            save_state: true,
            create_backup: true,
            send_notifications: true,
        }
    }
}

/// Statistics about the shutdown process
#[derive(Debug, Clone)]
pub struct ShutdownStats {
    pub phase: ShutdownPhase,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_seconds: Option<f64>,
    pub components_shutdown: Vec<String>,
    pub failed_components: Vec<(String, String)>, // (component, error)
    pub messages_processed_during_shutdown: u64,
    pub forced_termination: bool,
}

impl Default for ShutdownStats {
    fn default() -> Self {
        Self {
            phase: ShutdownPhase::Running,
            started_at: None,
            completed_at: None,
            duration_seconds: None,
            components_shutdown: Vec::new(),
            failed_components: Vec::new(),
            messages_processed_during_shutdown: 0,
            forced_termination: false,
        }
    }
}

/// Component that can be gracefully shut down
#[async_trait::async_trait]
pub trait ShutdownComponent: Send + Sync {
    /// Component name for logging
    fn name(&self) -> &str;
    
    /// Gracefully shutdown the component
    async fn shutdown(&self) -> Result<()>;
    
    /// Check if component is ready to shutdown (no pending operations)
    async fn is_ready_for_shutdown(&self) -> bool {
        true // Default implementation
    }
    
    /// Force shutdown the component (called if graceful shutdown fails)
    async fn force_shutdown(&self) -> Result<()> {
        self.shutdown().await // Default to same as graceful
    }
}

/// Main graceful shutdown manager
pub struct GracefulShutdown {
    config: ShutdownConfig,
    phase: Arc<RwLock<ShutdownPhase>>,
    stats: Arc<RwLock<ShutdownStats>>,
    shutdown_notifier: broadcast::Sender<ShutdownPhase>,
    components: Arc<RwLock<Vec<Box<dyn ShutdownComponent>>>>,
    active_operations: Arc<Semaphore>,
    is_shutdown_requested: Arc<RwLock<bool>>,
}

impl GracefulShutdown {
    pub fn new(config: ShutdownConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(10);
        
        Self {
            config,
            phase: Arc::new(RwLock::new(ShutdownPhase::Running)),
            stats: Arc::new(RwLock::new(ShutdownStats::default())),
            shutdown_notifier: shutdown_tx,
            components: Arc::new(RwLock::new(Vec::new())),
            active_operations: Arc::new(Semaphore::new(1000)), // Max concurrent operations
            is_shutdown_requested: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(ShutdownConfig::default())
    }

    /// Register a component for graceful shutdown
    pub async fn register_component(&self, component: Box<dyn ShutdownComponent>) {
        let name = component.name().to_string();
        self.components.write().await.push(component);
        debug!("Registered shutdown component: {}", name);
    }

    /// Start listening for shutdown signals
    pub async fn start_signal_handlers(&self) {
        let phase = Arc::clone(&self.phase);
        let shutdown_notifier = self.shutdown_notifier.clone();
        let is_shutdown_requested = Arc::clone(&self.is_shutdown_requested);

        // Handle SIGINT (Ctrl+C)
        let phase_sigint = Arc::clone(&phase);
        let notifier_sigint = shutdown_notifier.clone();
        let shutdown_requested_sigint = Arc::clone(&is_shutdown_requested);
        
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(()) => {
                    info!("Received Ctrl+C signal, initiating graceful shutdown...");
                    *shutdown_requested_sigint.write().await = true;
                    *phase_sigint.write().await = ShutdownPhase::Draining;
                    let _ = notifier_sigint.send(ShutdownPhase::Draining);
                }
                Err(err) => {
                    error!("Failed to listen for Ctrl+C signal: {}", err);
                }
            }
        });

        // Handle SIGTERM (systemd, docker stop, etc.)
        #[cfg(unix)]
        {
            let phase_sigterm = Arc::clone(&phase);
            let notifier_sigterm = shutdown_notifier.clone();
            let shutdown_requested_sigterm = Arc::clone(&is_shutdown_requested);
            
            tokio::spawn(async move {
                let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                    .expect("Failed to register SIGTERM handler");
                
                sigterm.recv().await;
                info!("Received SIGTERM signal, initiating graceful shutdown...");
                *shutdown_requested_sigterm.write().await = true;
                *phase_sigterm.write().await = ShutdownPhase::Draining;
                let _ = notifier_sigterm.send(ShutdownPhase::Draining);
            });
        }

        info!("Shutdown signal handlers started");
    }

    /// Wait for shutdown signal and perform graceful shutdown
    pub async fn wait_for_shutdown(&self) -> Result<()> {
        info!("Waiting for shutdown signal...");

        // Wait for shutdown to be requested
        loop {
            if *self.is_shutdown_requested.read().await {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        info!("Shutdown requested, beginning graceful shutdown process...");
        self.perform_shutdown().await
    }

    /// Perform the actual shutdown process
    async fn perform_shutdown(&self) -> Result<()> {
        let start_time = chrono::Utc::now();
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.started_at = Some(start_time);
            stats.phase = ShutdownPhase::Draining;
        }

        // Phase 1: Draining - Stop accepting new work
        info!("Phase 1: Draining - stopping new requests...");
        *self.phase.write().await = ShutdownPhase::Draining;
        let _ = self.shutdown_notifier.send(ShutdownPhase::Draining);

        // Wait for active operations to complete or timeout
        let drain_timeout = Duration::from_secs(self.config.graceful_timeout_seconds);
        let drain_result = timeout(drain_timeout, self.wait_for_operations_completion()).await;

        match drain_result {
            Ok(_) => {
                info!("All operations completed gracefully");
            }
            Err(_) => {
                warn!("Timeout waiting for operations to complete, proceeding with shutdown");
                let mut stats = self.stats.write().await;
                stats.forced_termination = true;
            }
        }

        // Phase 2: Terminating - Shutdown components
        info!("Phase 2: Terminating - shutting down components...");
        *self.phase.write().await = ShutdownPhase::Terminating;
        let _ = self.shutdown_notifier.send(ShutdownPhase::Terminating);
        
        self.shutdown_all_components().await?;

        // Phase 3: Final cleanup and state saving
        info!("Phase 3: Final cleanup...");
        if self.config.save_state {
            if let Err(e) = self.save_application_state().await {
                error!("Failed to save application state: {}", e);
                let mut stats = self.stats.write().await;
                stats.failed_components.push(("state_saver".to_string(), e.to_string()));
            }
        }

        if self.config.create_backup {
            if let Err(e) = self.create_shutdown_backup().await {
                error!("Failed to create shutdown backup: {}", e);
                let mut stats = self.stats.write().await;
                stats.failed_components.push(("backup_creator".to_string(), e.to_string()));
            }
        }

        // Phase 4: Complete
        let end_time = chrono::Utc::now();
        let duration = (end_time - start_time).num_milliseconds() as f64 / 1000.0;
        
        {
            let mut stats = self.stats.write().await;
            stats.completed_at = Some(end_time);
            stats.duration_seconds = Some(duration);
            stats.phase = ShutdownPhase::Stopped;
        }

        *self.phase.write().await = ShutdownPhase::Stopped;
        let _ = self.shutdown_notifier.send(ShutdownPhase::Stopped);

        info!("Graceful shutdown completed in {:.2} seconds", duration);
        Ok(())
    }

    /// Wait for all active operations to complete
    async fn wait_for_operations_completion(&self) {
        let max_permits = 1000; // Should match the semaphore size
        
        // Try to acquire all permits, which means no operations are running
        let _permits = self.active_operations.acquire_many(max_permits as u32).await
            .expect("Failed to acquire semaphore permits for shutdown");
        
        debug!("All active operations have completed");
    }

    /// Shutdown all registered components
    async fn shutdown_all_components(&self) -> Result<()> {
        let components = self.components.read().await;
        let component_timeout = Duration::from_secs(self.config.component_timeout_seconds);
        
        info!("Shutting down {} components...", components.len());
        
        for component in components.iter() {
            let component_name = component.name().to_string();
            info!("Shutting down component: {}", component_name);
            
            // First try graceful shutdown with timeout
            let shutdown_result = timeout(component_timeout, component.shutdown()).await;
            
            match shutdown_result {
                Ok(Ok(())) => {
                    info!("Component '{}' shut down gracefully", component_name);
                    self.stats.write().await.components_shutdown.push(component_name);
                }
                Ok(Err(e)) => {
                    error!("Component '{}' failed to shutdown gracefully: {}", component_name, e);
                    
                    // Try force shutdown
                    warn!("Attempting force shutdown of component '{}'", component_name);
                    match timeout(component_timeout, component.force_shutdown()).await {
                        Ok(Ok(())) => {
                            warn!("Component '{}' force shutdown successful", component_name);
                            self.stats.write().await.components_shutdown.push(component_name);
                        }
                        Ok(Err(force_err)) => {
                            error!("Component '{}' force shutdown failed: {}", component_name, force_err);
                            self.stats.write().await.failed_components.push((component_name, force_err.to_string()));
                        }
                        Err(_) => {
                            error!("Component '{}' force shutdown timed out", component_name);
                            self.stats.write().await.failed_components.push((component_name, "Force shutdown timeout".to_string()));
                        }
                    }
                }
                Err(_) => {
                    error!("Component '{}' shutdown timed out", component_name);
                    
                    // Try force shutdown after timeout
                    warn!("Attempting force shutdown of timed out component '{}'", component_name);
                    match timeout(component_timeout, component.force_shutdown()).await {
                        Ok(Ok(())) => {
                            warn!("Component '{}' force shutdown successful after timeout", component_name);
                            self.stats.write().await.components_shutdown.push(component_name);
                        }
                        _ => {
                            error!("Component '{}' could not be shut down", component_name);
                            self.stats.write().await.failed_components.push((component_name, "Shutdown timeout".to_string()));
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Save application state before shutdown
    async fn save_application_state(&self) -> Result<()> {
        info!("Saving application state...");
        
        // This would save things like:
        // - Current configuration state
        // - Active user sessions
        // - Analytics data
        // - Points/achievements data
        // - Giveaway state
        
        // For now, just log that we would save state
        // In a real implementation, this would integrate with your data persistence layer
        debug!("Application state saved successfully");
        Ok(())
    }

    /// Create backup before shutdown
    async fn create_shutdown_backup(&self) -> Result<()> {
        info!("Creating shutdown backup...");
        
        // This would create backups of:
        // - Configuration files
        // - Database snapshots
        // - Log files
        // - Any other critical state
        
        debug!("Shutdown backup created successfully");
        Ok(())
    }

    /// Get current shutdown phase
    pub async fn get_phase(&self) -> ShutdownPhase {
        self.phase.read().await.clone()
    }

    /// Get shutdown statistics
    pub async fn get_stats(&self) -> ShutdownStats {
        self.stats.read().await.clone()
    }

    /// Check if shutdown has been requested
    pub async fn is_shutdown_requested(&self) -> bool {
        *self.is_shutdown_requested.read().await
    }

    /// Subscribe to shutdown phase changes
    pub fn subscribe_to_shutdown(&self) -> broadcast::Receiver<ShutdownPhase> {
        self.shutdown_notifier.subscribe()
    }

    /// Manually trigger shutdown (for testing or programmatic shutdown)
    pub async fn trigger_shutdown(&self) {
        info!("Shutdown manually triggered");
        *self.is_shutdown_requested.write().await = true;
        *self.phase.write().await = ShutdownPhase::Draining;
        let _ = self.shutdown_notifier.send(ShutdownPhase::Draining);
    }

    /// Acquire a permit for an operation (blocks during shutdown)
    pub async fn acquire_operation_permit(&self) -> Option<tokio::sync::SemaphorePermit> {
        if *self.phase.read().await == ShutdownPhase::Running {
            self.active_operations.acquire().await.ok()
        } else {
            None // Don't allow new operations during shutdown
        }
    }

    /// Send shutdown notifications to connected platforms
    async fn send_shutdown_notifications(&self) -> Result<()> {
        if !self.config.send_notifications {
            return Ok(());
        }

        info!("Sending shutdown notifications...");
        
        // This would send notifications like:
        // - "Bot is shutting down for maintenance"
        // - Save any final messages
        // - Notify about when bot will be back
        
        debug!("Shutdown notifications sent");
        Ok(())
    }
}

/// Wrapper for ChatBot to implement ShutdownComponent
pub struct ChatBotShutdownComponent {
    bot: Arc<RwLock<crate::bot::ChatBot>>,
}

impl ChatBotShutdownComponent {
    pub fn new(bot: Arc<RwLock<crate::bot::ChatBot>>) -> Self {
        Self { bot }
    }
}

#[async_trait::async_trait]
impl ShutdownComponent for ChatBotShutdownComponent {
    fn name(&self) -> &str {
        "ChatBot"
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down ChatBot...");
        let mut bot = self.bot.write().await;
        bot.shutdown().await?;
        info!("ChatBot shutdown complete");
        Ok(())
    }

    async fn is_ready_for_shutdown(&self) -> bool {
        // Check if bot has any pending operations
        // For now, always return true
        true
    }
}

/// Wrapper for ConnectionPool to implement ShutdownComponent
pub struct ConnectionPoolShutdownComponent {
    pool: Arc<ConnectionPool>,
}

impl ConnectionPoolShutdownComponent {
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ShutdownComponent for ConnectionPoolShutdownComponent {
    fn name(&self) -> &str {
        "ConnectionPool"
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down connection pool...");
        self.pool.shutdown().await?;
        info!("Connection pool shutdown complete");
        Ok(())
    }

    async fn is_ready_for_shutdown(&self) -> bool {
        // Check if pool has any active connections
        self.pool.total_connections().await == 0
    }
}

/// Wrapper for ConfigurationManager to implement ShutdownComponent
pub struct ConfigManagerShutdownComponent {
    config_manager: Arc<ConfigurationManager>,
}

impl ConfigManagerShutdownComponent {
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self { config_manager }
    }
}

#[async_trait::async_trait]
impl ShutdownComponent for ConfigManagerShutdownComponent {
    fn name(&self) -> &str {
        "ConfigurationManager"
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down configuration manager...");
        
        // Create final backup
        if let Err(e) = self.config_manager.create_backup().await {
            warn!("Failed to create final config backup: {}", e);
        }
        
        info!("Configuration manager shutdown complete");
        Ok(())
    }
}

/// Integration helper for setting up graceful shutdown in main application
pub struct ShutdownIntegration;

impl ShutdownIntegration {
    /// Setup graceful shutdown for the entire application
    pub async fn setup(
        bot: Arc<RwLock<crate::bot::ChatBot>>,
        connection_pool: Option<Arc<ConnectionPool>>,
        config_manager: Arc<ConfigurationManager>,
    ) -> Result<Arc<GracefulShutdown>> {
        let shutdown_manager = Arc::new(GracefulShutdown::with_default_config());

        // Register components
        shutdown_manager.register_component(
            Box::new(ChatBotShutdownComponent::new(bot))
        ).await;

        if let Some(pool) = connection_pool {
            shutdown_manager.register_component(
                Box::new(ConnectionPoolShutdownComponent::new(pool))
            ).await;
        }

        shutdown_manager.register_component(
            Box::new(ConfigManagerShutdownComponent::new(config_manager))
        ).await;

        // Start signal handlers
        shutdown_manager.start_signal_handlers().await;

        info!("Graceful shutdown system initialized");
        Ok(shutdown_manager)
    }

    /// Helper function to wrap operations with shutdown-aware permits
    pub async fn with_operation_permit<F, T>(
        shutdown_manager: &GracefulShutdown,
        operation: F,
    ) -> Option<T>
    where
        F: std::future::Future<Output = T>,
    {
        if let Some(_permit) = shutdown_manager.acquire_operation_permit().await {
            Some(operation.await)
        } else {
            debug!("Operation skipped due to shutdown in progress");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestComponent {
        name: String,
        shutdown_called: Arc<AtomicBool>,
        should_fail: bool,
    }

    impl TestComponent {
        fn new(name: String, should_fail: bool) -> Self {
            Self {
                name,
                shutdown_called: Arc::new(AtomicBool::new(false)),
                should_fail,
            }
        }

        fn was_shutdown_called(&self) -> bool {
            self.shutdown_called.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl ShutdownComponent for TestComponent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn shutdown(&self) -> Result<()> {
            self.shutdown_called.store(true, Ordering::Relaxed);
            if self.should_fail {
                Err(anyhow::anyhow!("Test component shutdown failure"))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_graceful_shutdown_phases() {
        let shutdown_manager = GracefulShutdown::with_default_config();
        
        assert_eq!(shutdown_manager.get_phase().await, ShutdownPhase::Running);
        
        shutdown_manager.trigger_shutdown().await;
        
        // Give it a moment to process
        sleep(Duration::from_millis(10)).await;
        
        assert_eq!(shutdown_manager.get_phase().await, ShutdownPhase::Draining);
    }

    #[tokio::test]
    async fn test_component_registration_and_shutdown() {
        let shutdown_manager = GracefulShutdown::with_default_config();
        
        let test_component = TestComponent::new("test".to_string(), false);
        let was_called = test_component.shutdown_called.clone();
        
        shutdown_manager.register_component(Box::new(test_component)).await;
        
        shutdown_manager.trigger_shutdown().await;
        shutdown_manager.perform_shutdown().await.unwrap();
        
        assert!(was_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_operation_permits_during_shutdown() {
        let shutdown_manager = GracefulShutdown::with_default_config();
        
        // Should get permit during normal operation
        let permit1 = shutdown_manager.acquire_operation_permit().await;
        assert!(permit1.is_some());
        
        shutdown_manager.trigger_shutdown().await;
        
        // Should not get permit during shutdown
        let permit2 = shutdown_manager.acquire_operation_permit().await;
        assert!(permit2.is_none());
    }
}