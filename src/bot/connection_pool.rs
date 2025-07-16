// src/bot/connection_pool.rs - Connection pooling for platform connections

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};

use crate::platforms::{PlatformConnection, twitch::TwitchConnection, youtube::YouTubeConnection};
use crate::platforms::{twitch::TwitchConfig, youtube::YouTubeConfig};

/// Configuration for connection pooling
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_connections_per_platform: usize,
    pub min_idle_connections: usize,
    pub max_idle_connections: usize,
    pub connection_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub health_check_interval_seconds: u64,
    pub retry_attempts: u32,
    pub retry_delay_seconds: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_platform: 5,
            min_idle_connections: 1,
            max_idle_connections: 3,
            connection_timeout_seconds: 30,
            idle_timeout_seconds: 300, // 5 minutes
            health_check_interval_seconds: 60,
            retry_attempts: 3,
            retry_delay_seconds: 5,
        }
    }
}

/// Statistics for connection pool monitoring
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub platform: String,
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub failed_connections: usize,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_response_time_ms: f64,
}

/// Wrapper for pooled connections with metadata
struct PooledConnection {
    connection: Box<dyn PlatformConnection>,
    created_at: Instant,
    last_used: Instant,
    use_count: u64,
    is_healthy: bool,
}

impl PooledConnection {
    fn new(connection: Box<dyn PlatformConnection>) -> Self {
        let now = Instant::now();
        Self {
            connection,
            created_at: now,
            last_used: now,
            use_count: 0,
            is_healthy: true,
        }
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }

    fn is_expired(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }

    async fn health_check(&mut self) -> bool {
        self.is_healthy = self.connection.is_connected().await;
        self.is_healthy
    }
}

/// Platform-specific connection pool
struct PlatformPool {
    platform: String,
    config: PoolConfig,
    active_connections: Vec<PooledConnection>,
    idle_connections: Vec<PooledConnection>,
    semaphore: Arc<Semaphore>,
    stats: PoolStats,
}

impl PlatformPool {
    fn new(platform: String, config: PoolConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_connections_per_platform));
        let stats = PoolStats {
            platform: platform.clone(),
            total_connections: 0,
            active_connections: 0,
            idle_connections: 0,
            failed_connections: 0,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time_ms: 0.0,
        };

        Self {
            platform,
            config,
            active_connections: Vec::new(),
            idle_connections: Vec::new(),
            semaphore,
            stats,
        }
    }

    async fn get_connection(&mut self) -> Result<Box<dyn PlatformConnection>> {
        self.stats.total_requests += 1;
        let start_time = Instant::now();

        // Try to get a permit (respects max connections limit)
        let permit = self.semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to acquire connection permit"))?;

        // Try to reuse an idle connection first
        if let Some(mut pooled_conn) = self.idle_connections.pop() {
            // Health check the idle connection
            if pooled_conn.health_check().await {
                pooled_conn.mark_used();
                
                // Drop the permit before mutable self operations
                drop(permit);
                
                self.update_stats();
                self.stats.successful_requests += 1;
                
                let response_time = start_time.elapsed().as_millis() as f64;
                self.update_response_time(response_time);
                
                debug!("Reused idle {} connection", self.platform);
                return Ok(pooled_conn.connection);
            } else {
                warn!("Idle {} connection failed health check, creating new one", self.platform);
                self.stats.failed_connections += 1;
            }
        }

        // Create a new connection
        match self.create_new_connection().await {
            Ok(connection) => {
                // Drop the permit before mutable self operations
                drop(permit);
                
                self.update_stats();
                self.stats.successful_requests += 1;
                
                let response_time = start_time.elapsed().as_millis() as f64;
                self.update_response_time(response_time);
                
                info!("Created new {} connection", self.platform);
                Ok(connection)
            }
            Err(e) => {
                // Drop the permit before mutable self operations
                drop(permit);
                
                self.stats.failed_requests += 1;
                self.stats.failed_connections += 1;
                Err(e)
            }
        }
    }

    async fn return_connection(&mut self, connection: Box<dyn PlatformConnection>) {
        // Find the connection in active list
        if let Some(pos) = self.active_connections.iter().position(|pc| 
            std::ptr::eq(pc.connection.as_ref(), connection.as_ref())
        ) {
            let mut pooled_conn = self.active_connections.remove(pos);
            
            // Check if we should keep it in idle pool
            if self.idle_connections.len() < self.config.max_idle_connections {
                pooled_conn.mark_used();
                self.idle_connections.push(pooled_conn);
                debug!("Returned {} connection to idle pool", self.platform);
            } else {
                debug!("Discarded {} connection (idle pool full)", self.platform);
            }
            
            self.update_stats();
        }
    }

    async fn create_new_connection(&self) -> Result<Box<dyn PlatformConnection>> {
        let mut attempts = 0;
        let max_attempts = self.config.retry_attempts;

        while attempts < max_attempts {
            attempts += 1;
            
            match self.platform.as_str() {
                "twitch" => {
                    match TwitchConfig::from_env() {
                        Ok(config) => {
                            let mut connection = Box::new(TwitchConnection::new(config));
                            
                            match tokio::time::timeout(
                                Duration::from_secs(self.config.connection_timeout_seconds),
                                connection.connect()
                            ).await {
                                Ok(Ok(())) => {
                                    info!("Successfully created Twitch connection (attempt {})", attempts);
                                    return Ok(connection);
                                }
                                Ok(Err(e)) => {
                                    error!("Twitch connection failed (attempt {}): {}", attempts, e);
                                }
                                Err(_) => {
                                    error!("Twitch connection timed out (attempt {})", attempts);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to load Twitch config: {}", e);
                            return Err(e);
                        }
                    }
                }
                "youtube" => {
                    match YouTubeConfig::from_env() {
                        Ok(config) => {
                            let mut connection = Box::new(YouTubeConnection::new(config));
                            
                            match tokio::time::timeout(
                                Duration::from_secs(self.config.connection_timeout_seconds),
                                connection.connect()
                            ).await {
                                Ok(Ok(())) => {
                                    info!("Successfully created YouTube connection (attempt {})", attempts);
                                    return Ok(connection);
                                }
                                Ok(Err(e)) => {
                                    error!("YouTube connection failed (attempt {}): {}", attempts, e);
                                }
                                Err(_) => {
                                    error!("YouTube connection timed out (attempt {})", attempts);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to load YouTube config: {}", e);
                            return Err(e);
                        }
                    }
                }
                _ => {
                    return Err(anyhow::anyhow!("Unsupported platform: {}", self.platform));
                }
            }

            if attempts < max_attempts {
                tokio::time::sleep(Duration::from_secs(self.config.retry_delay_seconds)).await;
            }
        }

        Err(anyhow::anyhow!("Failed to create {} connection after {} attempts", self.platform, max_attempts))
    }

    async fn cleanup_expired_connections(&mut self) {
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_seconds);
        let initial_count = self.idle_connections.len();
        
        self.idle_connections.retain(|conn| !conn.is_expired(idle_timeout));
        
        let removed_count = initial_count - self.idle_connections.len();
        if removed_count > 0 {
            debug!("Cleaned up {} expired {} connections", removed_count, self.platform);
            self.update_stats();
        }
    }

    async fn health_check_connections(&mut self) {
        // Check idle connections
        let mut healthy_idle = Vec::new();
        for mut conn in self.idle_connections.drain(..) {
            if conn.health_check().await {
                healthy_idle.push(conn);
            } else {
                warn!("Removed unhealthy idle {} connection", self.platform);
                self.stats.failed_connections += 1;
            }
        }
        self.idle_connections = healthy_idle;

        // Check active connections (non-destructive)
        for conn in &mut self.active_connections {
            if !conn.health_check().await {
                warn!("Active {} connection is unhealthy", self.platform);
                self.stats.failed_connections += 1;
            }
        }

        self.update_stats();
    }

    async fn ensure_minimum_connections(&mut self) {
        let total_connections = self.active_connections.len() + self.idle_connections.len();
        
        if total_connections < self.config.min_idle_connections {
            let needed = self.config.min_idle_connections - total_connections;
            
            for _ in 0..needed {
                match self.create_new_connection().await {
                    Ok(connection) => {
                        let pooled_conn = PooledConnection::new(connection);
                        self.idle_connections.push(pooled_conn);
                        debug!("Created minimum idle {} connection", self.platform);
                    }
                    Err(e) => {
                        error!("Failed to create minimum idle {} connection: {}", self.platform, e);
                        break;
                    }
                }
            }
            
            self.update_stats();
        }
    }

    fn update_stats(&mut self) {
        self.stats.total_connections = self.active_connections.len() + self.idle_connections.len();
        self.stats.active_connections = self.active_connections.len();
        self.stats.idle_connections = self.idle_connections.len();
    }

    fn update_response_time(&mut self, response_time_ms: f64) {
        // Simple moving average
        let alpha = 0.1; // Smoothing factor
        if self.stats.average_response_time_ms == 0.0 {
            self.stats.average_response_time_ms = response_time_ms;
        } else {
            self.stats.average_response_time_ms = 
                alpha * response_time_ms + (1.0 - alpha) * self.stats.average_response_time_ms;
        }
    }

    fn get_stats(&self) -> PoolStats {
        self.stats.clone()
    }
}

/// Main connection pool manager
pub struct ConnectionPool {
    pools: Arc<RwLock<HashMap<String, PlatformPool>>>,
    config: PoolConfig,
    is_running: Arc<RwLock<bool>>,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            config,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(PoolConfig::default())
    }

    /// Initialize the connection pool
    pub async fn initialize(&self, platforms: Vec<String>) -> Result<()> {
        let mut pools = self.pools.write().await;
        
        for platform in platforms {
            let pool = PlatformPool::new(platform.clone(), self.config.clone());
            pools.insert(platform.clone(), pool);
            info!("Initialized connection pool for platform: {}", platform);
        }

        *self.is_running.write().await = true;
        
        // Start background maintenance task
        self.start_maintenance_task().await;
        
        info!("Connection pool initialized with {} platforms", pools.len());
        Ok(())
    }

    /// Get a connection from the pool
    pub async fn get_connection(&self, platform: &str) -> Result<Box<dyn PlatformConnection>> {
        let mut pools = self.pools.write().await;
        
        if let Some(pool) = pools.get_mut(platform) {
            pool.get_connection().await
        } else {
            Err(anyhow::anyhow!("No pool found for platform: {}", platform))
        }
    }

    /// Return a connection to the pool
    pub async fn return_connection(&self, platform: &str, connection: Box<dyn PlatformConnection>) {
        let mut pools = self.pools.write().await;
        
        if let Some(pool) = pools.get_mut(platform) {
            pool.return_connection(connection).await;
        } else {
            warn!("Attempted to return connection to unknown platform: {}", platform);
        }
    }

    /// Get pool statistics for all platforms
    pub async fn get_stats(&self) -> HashMap<String, PoolStats> {
        let pools = self.pools.read().await;
        let mut stats = HashMap::new();
        
        for (platform, pool) in pools.iter() {
            stats.insert(platform.clone(), pool.get_stats());
        }
        
        stats
    }

    /// Get statistics for a specific platform
    pub async fn get_platform_stats(&self, platform: &str) -> Option<PoolStats> {
        let pools = self.pools.read().await;
        pools.get(platform).map(|pool| pool.get_stats())
    }

    /// Shutdown the connection pool gracefully
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down connection pool...");
        
        *self.is_running.write().await = false;
        
        let mut pools = self.pools.write().await;
        for (platform, pool) in pools.iter_mut() {
            // Disconnect all connections
            for mut conn in pool.active_connections.drain(..) {
                if let Err(e) = conn.connection.disconnect().await {
                    error!("Failed to disconnect active {} connection: {}", platform, e);
                }
            }
            
            for mut conn in pool.idle_connections.drain(..) {
                if let Err(e) = conn.connection.disconnect().await {
                    error!("Failed to disconnect idle {} connection: {}", platform, e);
                }
            }
            
            info!("Disconnected all {} connections", platform);
        }
        
        pools.clear();
        info!("Connection pool shutdown complete");
        Ok(())
    }

    /// Start background maintenance task
    async fn start_maintenance_task(&self) {
        let pools = Arc::clone(&self.pools);
        let is_running = Arc::clone(&self.is_running);
        let health_check_interval = Duration::from_secs(self.config.health_check_interval_seconds);
        
        tokio::spawn(async move {
            info!("Connection pool maintenance task started");
            
            while *is_running.read().await {
                {
                    let mut pools_guard = pools.write().await;
                    for (platform, pool) in pools_guard.iter_mut() {
                        debug!("Running maintenance for {} pool", platform);
                        
                        // Cleanup expired connections
                        pool.cleanup_expired_connections().await;
                        
                        // Health check all connections
                        pool.health_check_connections().await;
                        
                        // Ensure minimum connections
                        pool.ensure_minimum_connections().await;
                    }
                }
                
                tokio::time::sleep(health_check_interval).await;
            }
            
            info!("Connection pool maintenance task stopped");
        });
    }

    /// Force health check on all pools
    pub async fn force_health_check(&self) {
        let mut pools = self.pools.write().await;
        for (platform, pool) in pools.iter_mut() {
            debug!("Force health check for {} pool", platform);
            pool.health_check_connections().await;
        }
    }

    /// Get total connection count across all platforms
    pub async fn total_connections(&self) -> usize {
        let pools = self.pools.read().await;
        pools.values()
            .map(|pool| pool.active_connections.len() + pool.idle_connections.len())
            .sum()
    }

    /// Check if pool is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_initialization() {
        let pool = ConnectionPool::with_default_config();
        let platforms = vec!["twitch".to_string(), "youtube".to_string()];
        
        // Note: This test requires environment variables to be set
        // In a real test environment, you'd mock the connections
        // let result = pool.initialize(platforms).await;
        // assert!(result.is_ok());
    }

    #[test]
    fn test_pool_config_defaults() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections_per_platform, 5);
        assert_eq!(config.min_idle_connections, 1);
        assert_eq!(config.max_idle_connections, 3);
    }
}
