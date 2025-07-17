// src/adaptive/metrics.rs
//! Comprehensive metrics collection for adaptive performance tuning

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::interval;
use serde::{Deserialize, Serialize};

/// Comprehensive performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub system_health_score: f64,
    pub total_metrics_collected: usize,
    
    // Latency metrics
    pub average_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
    
    // Memory metrics
    pub memory_usage_percent: f64,
    pub heap_size_mb: f64,
    pub gc_frequency_hz: f64,
    
    // Error metrics
    pub error_rate_percent: f64,
    pub total_errors: u64,
    pub errors_per_minute: f64,
    
    // Throughput metrics
    pub messages_per_second: f64,
    pub commands_per_second: f64,
    pub api_calls_per_second: f64,
    
    // Connection metrics
    pub active_connections: u32,
    pub connection_pool_utilization: f64,
    pub connection_failures: u32,
    
    // AI/Moderation metrics
    pub ai_processing_time_ms: f64,
    pub moderation_queue_length: u32,
    pub pattern_match_rate: f64,
    
    // Custom metrics
    pub custom_metrics: HashMap<String, f64>,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            uptime_seconds: 0,
            system_health_score: 1.0,
            total_metrics_collected: 0,
            average_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
            memory_usage_percent: 0.0,
            heap_size_mb: 0.0,
            gc_frequency_hz: 0.0,
            error_rate_percent: 0.0,
            total_errors: 0,
            errors_per_minute: 0.0,
            messages_per_second: 0.0,
            commands_per_second: 0.0,
            api_calls_per_second: 0.0,
            active_connections: 0,
            connection_pool_utilization: 0.0,
            connection_failures: 0,
            ai_processing_time_ms: 0.0,
            moderation_queue_length: 0,
            pattern_match_rate: 0.0,
            custom_metrics: HashMap::new(),
        }
    }
}

/// Individual metric data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp: u64,
    pub value: f64,
    pub metric_type: MetricType,
}

/// Types of metrics we collect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    Latency,
    Memory,
    Error,
    Throughput,
    Connection,
    AI,
    Custom(String),
}

/// Time-series data for a specific metric
#[derive(Debug, Clone)]
pub struct MetricTimeSeries {
    pub name: String,
    pub data_points: VecDeque<MetricDataPoint>,
    pub max_size: usize,
}

impl MetricTimeSeries {
    pub fn new(name: String, max_size: usize) -> Self {
        Self {
            name,
            data_points: VecDeque::new(),
            max_size,
        }
    }
    
    pub fn add_point(&mut self, value: f64, metric_type: MetricType) {
        let point = MetricDataPoint {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            value,
            metric_type,
        };
        
        self.data_points.push_back(point);
        
        if self.data_points.len() > self.max_size {
            self.data_points.pop_front();
        }
    }
    
    pub fn get_recent_average(&self, window_seconds: u64) -> f64 {
        let cutoff = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - window_seconds;
        
        let recent_points: Vec<_> = self.data_points
            .iter()
            .filter(|p| p.timestamp >= cutoff)
            .collect();
        
        if recent_points.is_empty() {
            0.0
        } else {
            recent_points.iter().map(|p| p.value).sum::<f64>() / recent_points.len() as f64
        }
    }
    
    pub fn get_percentile(&self, percentile: f64) -> f64 {
        if self.data_points.is_empty() {
            return 0.0;
        }
        
        let mut values: Vec<f64> = self.data_points.iter().map(|p| p.value).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let index = ((values.len() - 1) as f64 * percentile / 100.0).floor() as usize;
        values[index.min(values.len() - 1)]
    }
    
    pub fn get_max(&self) -> f64 {
        self.data_points.iter().map(|p| p.value).fold(0.0, f64::max)
    }
}

/// Metrics collector that gathers and stores performance data
pub struct MetricsCollector {
    metrics: Arc<RwLock<HashMap<String, MetricTimeSeries>>>,
    current_metrics: Arc<RwLock<PerformanceMetrics>>,
    retention_hours: u64,
    collection_interval: Duration,
    start_time: Instant,
    running: Arc<RwLock<bool>>,
}

impl MetricsCollector {
    pub fn new(retention_hours: u64) -> Result<Self> {
        let max_points = (retention_hours * 60 * 60) / 30; // 30-second intervals
        
        Ok(Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            current_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            retention_hours,
            collection_interval: Duration::from_secs(30),
            start_time: Instant::now(),
            running: Arc::new(RwLock::new(false)),
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                return Ok(());
            }
            *running = true;
        }
        
        info!("Starting metrics collection (retention: {} hours)", self.retention_hours);
        
        // Start collection loop
        self.start_collection_loop().await?;
        
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        info!("Metrics collection stopped");
        Ok(())
    }
    
    pub async fn record_latency(&self, operation: &str, latency_ms: f64) -> Result<()> {
        let metric_name = format!("latency_{}", operation);
        self.record_metric(&metric_name, latency_ms, MetricType::Latency).await
    }
    
    pub async fn record_memory_usage(&self, usage_percent: f64) -> Result<()> {
        self.record_metric("memory_usage", usage_percent, MetricType::Memory).await
    }
    
    pub async fn record_error(&self, error_type: &str) -> Result<()> {
        let metric_name = format!("error_{}", error_type);
        self.record_metric(&metric_name, 1.0, MetricType::Error).await
    }
    
    pub async fn record_throughput(&self, operation: &str, rate: f64) -> Result<()> {
        let metric_name = format!("throughput_{}", operation);
        self.record_metric(&metric_name, rate, MetricType::Throughput).await
    }
    
    pub async fn record_custom_metric(&self, name: &str, value: f64) -> Result<()> {
        self.record_metric(name, value, MetricType::Custom(name.to_string())).await
    }
    
    async fn record_metric(&self, name: &str, value: f64, metric_type: MetricType) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        
        if !metrics.contains_key(name) {
            let max_points = (self.retention_hours * 60 * 60) / 30;
            metrics.insert(name.to_string(), MetricTimeSeries::new(name.to_string(), max_points as usize));
        }
        
        if let Some(series) = metrics.get_mut(name) {
            series.add_point(value, metric_type);
        }
        
        Ok(())
    }
    
    pub async fn get_current_metrics(&self) -> Result<PerformanceMetrics> {
        let metrics = self.current_metrics.read().await;
        Ok(metrics.clone())
    }
    
    pub async fn get_metric_history(&self, name: &str) -> Result<Vec<MetricDataPoint>> {
        let metrics = self.metrics.read().await;
        
        if let Some(series) = metrics.get(name) {
            Ok(series.data_points.iter().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }
    
    pub async fn get_all_metric_names(&self) -> Result<Vec<String>> {
        let metrics = self.metrics.read().await;
        Ok(metrics.keys().cloned().collect())
    }
    
    pub async fn calculate_system_health(&self) -> Result<f64> {
        let metrics = self.metrics.read().await;
        
        let mut health_factors = Vec::new();
        
        // Latency health (lower is better)
        if let Some(latency_series) = metrics.get("latency_message_processing") {
            let avg_latency = latency_series.get_recent_average(300); // 5 minutes
            let latency_health = if avg_latency < 100.0 { 1.0 }
                                else if avg_latency < 500.0 { 0.8 }
                                else if avg_latency < 1000.0 { 0.6 }
                                else { 0.3 };
            health_factors.push(latency_health);
        }
        
        // Memory health (lower is better)
        if let Some(memory_series) = metrics.get("memory_usage") {
            let avg_memory = memory_series.get_recent_average(300);
            let memory_health = if avg_memory < 70.0 { 1.0 }
                               else if avg_memory < 85.0 { 0.8 }
                               else if avg_memory < 95.0 { 0.5 }
                               else { 0.2 };
            health_factors.push(memory_health);
        }
        
        // Error rate health (lower is better)
        if let Some(error_series) = metrics.get("error_rate") {
            let error_rate = error_series.get_recent_average(300);
            let error_health = if error_rate < 1.0 { 1.0 }
                              else if error_rate < 5.0 { 0.7 }
                              else if error_rate < 10.0 { 0.4 }
                              else { 0.1 };
            health_factors.push(error_health);
        }
        
        // Connection health
        if let Some(connection_series) = metrics.get("connection_failures") {
            let failure_rate = connection_series.get_recent_average(300);
            let connection_health = if failure_rate < 1.0 { 1.0 }
                                   else if failure_rate < 5.0 { 0.8 }
                                   else { 0.5 };
            health_factors.push(connection_health);
        }
        
        // Calculate overall health
        if health_factors.is_empty() {
            Ok(1.0) // Default to healthy if no metrics available
        } else {
            Ok(health_factors.iter().sum::<f64>() / health_factors.len() as f64)
        }
    }
    
    async fn start_collection_loop(&self) -> Result<()> {
        let running = self.running.clone();
        let metrics = self.metrics.clone();
        let current_metrics = self.current_metrics.clone();
        let start_time = self.start_time;
        let collection_interval = self.collection_interval;
        
        tokio::spawn(async move {
            let mut interval = interval(collection_interval);
            
            info!("Metrics collection loop started");
            
            loop {
                interval.tick().await;
                
                if !*running.read().await {
                    break;
                }
                
                // Collect system metrics
                let uptime = start_time.elapsed().as_secs();
                
                // Simulate system metric collection
                // In a real implementation, these would come from actual system monitoring
                let memory_usage = Self::get_memory_usage().await;
                let cpu_usage = Self::get_cpu_usage().await;
                
                // Calculate derived metrics
                let system_health = Self::calculate_health_from_metrics(&metrics).await;
                
                // Update current metrics
                {
                    let mut current = current_metrics.write().await;
                    current.timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    current.uptime_seconds = uptime;
                    current.system_health_score = system_health;
                    current.memory_usage_percent = memory_usage;
                    
                    // Update metrics from time series data
                    let metrics_read = metrics.read().await;
                    
                    if let Some(latency_series) = metrics_read.get("latency_message_processing") {
                        current.average_latency_ms = latency_series.get_recent_average(300);
                        current.p95_latency_ms = latency_series.get_percentile(95.0);
                        current.p99_latency_ms = latency_series.get_percentile(99.0);
                        current.max_latency_ms = latency_series.get_max();
                    }
                    
                    if let Some(error_series) = metrics_read.get("error_rate") {
                        current.error_rate_percent = error_series.get_recent_average(300);
                    }
                    
                    if let Some(throughput_series) = metrics_read.get("throughput_messages") {
                        current.messages_per_second = throughput_series.get_recent_average(60);
                    }
                    
                    current.total_metrics_collected = metrics_read.len();
                }
                
                // Record system metrics
                {
                    let mut metrics_write = metrics.write().await;
                    
                    // Record memory usage
                    if let Some(memory_series) = metrics_write.get_mut("memory_usage") {
                        memory_series.add_point(memory_usage, MetricType::Memory);
                    }
                    
                    // Record system health
                    if let Some(health_series) = metrics_write.get_mut("system_health") {
                        health_series.add_point(system_health, MetricType::Custom("health".to_string()));
                    }
                }
                
                debug!("Metrics collection cycle completed - Health: {:.2}, Memory: {:.1}%", 
                       system_health, memory_usage);
            }
            
            info!("Metrics collection loop stopped");
        });
        
        Ok(())
    }
    
    async fn get_memory_usage() -> f64 {
        // Platform-specific memory usage collection
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = tokio::fs::read_to_string("/proc/meminfo").await {
                let mut total_kb = 0;
                let mut available_kb = 0;
                
                for line in contents.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(value) = line.split_whitespace().nth(1) {
                            total_kb = value.parse::<u64>().unwrap_or(0);
                        }
                    } else if line.starts_with("MemAvailable:") {
                        if let Some(value) = line.split_whitespace().nth(1) {
                            available_kb = value.parse::<u64>().unwrap_or(0);
                        }
                    }
                }
                
                if total_kb > 0 {
                    return ((total_kb - available_kb) as f64 / total_kb as f64) * 100.0;
                }
            }
        }
        
        // Fallback simulation for other platforms
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(30.0..80.0) // Simulate 30-80% memory usage
    }
    
    async fn get_cpu_usage() -> f64 {
        // Platform-specific CPU usage collection
        #[cfg(target_os = "linux")]
        {
            // This would require more complex implementation to read /proc/stat
            // For now, simulate
        }
        
        // Fallback simulation
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(10.0..60.0) // Simulate 10-60% CPU usage
    }
    
    async fn calculate_health_from_metrics(metrics: &Arc<RwLock<HashMap<String, MetricTimeSeries>>>) -> f64 {
        let metrics_read = metrics.read().await;
        let mut health_factors = Vec::new();
        
        // Latency health
        if let Some(latency_series) = metrics_read.get("latency_message_processing") {
            let avg_latency = latency_series.get_recent_average(300);
            let health = if avg_latency < 100.0 { 1.0 }
                        else if avg_latency < 500.0 { 0.8 }
                        else if avg_latency < 1000.0 { 0.6 }
                        else { 0.3 };
            health_factors.push(health);
        }
        
        // Memory health
        if let Some(memory_series) = metrics_read.get("memory_usage") {
            let avg_memory = memory_series.get_recent_average(300);
            let health = if avg_memory < 70.0 { 1.0 }
                        else if avg_memory < 85.0 { 0.8 }
                        else if avg_memory < 95.0 { 0.5 }
                        else { 0.2 };
            health_factors.push(health);
        }
        
        if health_factors.is_empty() {
            1.0
        } else {
            health_factors.iter().sum::<f64>() / health_factors.len() as f64
        }
    }
}

/// Metrics aggregator for different time windows
pub struct MetricsAggregator;

impl MetricsAggregator {
    pub fn aggregate_hourly(data_points: &[MetricDataPoint]) -> Vec<MetricDataPoint> {
        Self::aggregate_by_window(data_points, 3600) // 1 hour windows
    }
    
    pub fn aggregate_daily(data_points: &[MetricDataPoint]) -> Vec<MetricDataPoint> {
        Self::aggregate_by_window(data_points, 86400) // 1 day windows
    }
    
    fn aggregate_by_window(data_points: &[MetricDataPoint], window_seconds: u64) -> Vec<MetricDataPoint> {
        let mut aggregated = Vec::new();
        let mut current_window_start = 0;
        let mut window_points = Vec::new();
        
        for point in data_points {
            let window_start = (point.timestamp / window_seconds) * window_seconds;
            
            if current_window_start == 0 {
                current_window_start = window_start;
            }
            
            if window_start == current_window_start {
                window_points.push(point);
            } else {
                // Process current window
                if !window_points.is_empty() {
                    let avg_value = window_points.iter().map(|p| p.value).sum::<f64>() / window_points.len() as f64;
                    aggregated.push(MetricDataPoint {
                        timestamp: current_window_start,
                        value: avg_value,
                        metric_type: window_points[0].metric_type.clone(),
                    });
                }
                
                // Start new window
                current_window_start = window_start;
                window_points = vec![point];
            }
        }
        
        // Process final window
        if !window_points.is_empty() {
            let avg_value = window_points.iter().map(|p| p.value).sum::<f64>() / window_points.len() as f64;
            aggregated.push(MetricDataPoint {
                timestamp: current_window_start,
                value: avg_value,
                metric_type: window_points[0].metric_type.clone(),
            });
        }
        
        aggregated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new(24).unwrap();
        assert!(!*collector.running.read().await);
    }
    
    #[tokio::test]
    async fn test_metric_recording() {
        let collector = MetricsCollector::new(1).unwrap();
        
        collector.record_latency("test_operation", 150.0).await.unwrap();
        collector.record_memory_usage(65.0).await.unwrap();
        
        let metrics = collector.metrics.read().await;
        assert!(metrics.contains_key("latency_test_operation"));
        assert!(metrics.contains_key("memory_usage"));
    }
    
    #[tokio::test]
    async fn test_time_series_operations() {
        let mut series = MetricTimeSeries::new("test".to_string(), 100);
        
        series.add_point(10.0, MetricType::Latency);
        series.add_point(20.0, MetricType::Latency);
        series.add_point(30.0, MetricType::Latency);
        
        assert_eq!(series.data_points.len(), 3);
        assert_eq!(series.get_recent_average(3600), 20.0);
        assert_eq!(series.get_max(), 30.0);
    }
    
    #[tokio::test]
    async fn test_percentile_calculation() {
        let mut series = MetricTimeSeries::new("test".to_string(), 100);
        
        for i in 1..=100 {
            series.add_point(i as f64, MetricType::Latency);
        }
        
        assert_eq!(series.get_percentile(50.0), 50.0);
        assert_eq!(series.get_percentile(95.0), 95.0);
        assert_eq!(series.get_percentile(99.0), 99.0);
    }
    
    #[tokio::test]
    async fn test_metrics_aggregation() {
        let mut points = Vec::new();
        
        // Add points spanning 2 hours
        for i in 0..7200 {
            points.push(MetricDataPoint {
                timestamp: 1000000 + i,
                value: (i % 100) as f64,
                metric_type: MetricType::Latency,
            });
        }
        
        let hourly = MetricsAggregator::aggregate_hourly(&points);
        assert_eq!(hourly.len(), 3); // 3 hour windows
    }
}