// src/adaptive/parameters.rs
//! Dynamic parameter management for adaptive performance tuning

use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::fmt;
use serde::{Deserialize, Serialize};

/// A tunable parameter value that can be dynamically adjusted
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    Duration(u64), // Duration in milliseconds
}

impl ParameterValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ParameterValue::Integer(v) => Some(*v),
            ParameterValue::Float(v) => Some(*v as i64),
            _ => None,
        }
    }
    
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParameterValue::Float(v) => Some(*v),
            ParameterValue::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }
    
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParameterValue::Boolean(v) => Some(*v),
            _ => None,
        }
    }
    
    pub fn as_string(&self) -> Option<String> {
        match self {
            ParameterValue::String(v) => Some(v.clone()),
            _ => None,
        }
    }
    
    pub fn as_duration_ms(&self) -> Option<u64> {
        match self {
            ParameterValue::Duration(v) => Some(*v),
            ParameterValue::Integer(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }
}

impl fmt::Display for ParameterValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterValue::Integer(v) => write!(f, "{}", v),
            ParameterValue::Float(v) => write!(f, "{:.2}", v),
            ParameterValue::Boolean(v) => write!(f, "{}", v),
            ParameterValue::String(v) => write!(f, "{}", v),
            ParameterValue::Duration(v) => write!(f, "{}ms", v),
        }
    }
}

/// Constraints for parameter values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterConstraints {
    pub min_value: Option<ParameterValue>,
    pub max_value: Option<ParameterValue>,
    pub allowed_values: Option<Vec<ParameterValue>>,
    pub step_size: Option<ParameterValue>,
}

impl ParameterConstraints {
    pub fn validate(&self, value: &ParameterValue) -> Result<()> {
        // Check allowed values first
        if let Some(ref allowed) = self.allowed_values {
            if !allowed.contains(value) {
                return Err(anyhow!("Value {:?} not in allowed values: {:?}", value, allowed));
            }
        }
        
        // Check min/max constraints
        match (value, &self.min_value, &self.max_value) {
            (ParameterValue::Integer(v), Some(ParameterValue::Integer(min)), Some(ParameterValue::Integer(max))) => {
                if v < min || v > max {
                    return Err(anyhow!("Value {} outside range [{}, {}]", v, min, max));
                }
            }
            (ParameterValue::Float(v), Some(ParameterValue::Float(min)), Some(ParameterValue::Float(max))) => {
                if v < min || v > max {
                    return Err(anyhow!("Value {} outside range [{}, {}]", v, min, max));
                }
            }
            (ParameterValue::Duration(v), Some(ParameterValue::Duration(min)), Some(ParameterValue::Duration(max))) => {
                if v < min || v > max {
                    return Err(anyhow!("Value {}ms outside range [{}ms, {}ms]", v, min, max));
                }
            }
            _ => {} // No constraints to check
        }
        
        Ok(())
    }
}

/// Definition of a tunable parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub name: String,
    pub description: String,
    pub category: ParameterCategory,
    pub default_value: ParameterValue,
    pub current_value: ParameterValue,
    pub constraints: ParameterConstraints,
    pub impact_level: ImpactLevel,
    pub tuning_frequency: TuningFrequency,
    pub dependencies: Vec<String>, // Other parameters this depends on
}

/// Categories of parameters for organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterCategory {
    Connection,
    Memory,
    Processing,
    AI,
    Moderation,
    Cache,
    Database,
    Network,
    Custom(String),
}

/// Impact level of parameter changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactLevel {
    Low,    // Changes have minimal impact
    Medium, // Changes have moderate impact
    High,   // Changes have significant impact
    Critical, // Changes can affect system stability
}

/// How frequently a parameter can be tuned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TuningFrequency {
    Continuous,  // Can be adjusted continuously
    Hourly,      // Can be adjusted at most once per hour
    Daily,       // Can be adjusted at most once per day
    Manual,      // Only manual adjustment allowed
}

/// Store for managing all tunable parameters
pub struct ParameterStore {
    parameters: HashMap<String, ParameterDefinition>,
    change_history: Vec<ParameterChange>,
    max_history_size: usize,
}

/// Record of a parameter change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterChange {
    pub timestamp: u64,
    pub parameter_name: String,
    pub old_value: ParameterValue,
    pub new_value: ParameterValue,
    pub reason: String,
    pub triggered_by: String, // "system", "user", "safety", etc.
}

impl ParameterStore {
    pub fn new() -> Self {
        let mut store = Self {
            parameters: HashMap::new(),
            change_history: Vec::new(),
            max_history_size: 1000,
        };
        
        // Initialize with default system parameters
        store.initialize_default_parameters();
        store
    }
    
    pub fn register_parameter(&mut self, definition: ParameterDefinition) -> Result<()> {
        // Validate the definition
        definition.constraints.validate(&definition.default_value)?;
        definition.constraints.validate(&definition.current_value)?;
        
        info!("Registering parameter: {} ({})", definition.name, definition.description);
        self.parameters.insert(definition.name.clone(), definition);
        Ok(())
    }
    
    pub fn set_parameter(&mut self, name: &str, value: ParameterValue) -> Result<ParameterValue> {
        let old_value = {
            let parameter = self.parameters.get_mut(name)
                .ok_or_else(|| anyhow!("Parameter '{}' not found", name))?;

            // Validate the new value
            parameter.constraints.validate(&value)?;

            let old_value = parameter.current_value.clone();
            parameter.current_value = value.clone();

            debug!("Parameter '{}' changed from {} to {}", name, old_value, parameter.current_value);

            old_value
        }; // Mutable borrow ends here

        // Record the change (now self is not mutably borrowed)
        self.record_change(name, old_value.clone(), value, "manual".to_string(), "user".to_string());

        
        Ok(old_value)
    }
    
    pub fn get_parameter(&self, name: &str) -> Option<&ParameterValue> {
        self.parameters.get(name).map(|p| &p.current_value)
    }
    
    pub fn get_parameter_definition(&self, name: &str) -> Option<&ParameterDefinition> {
        self.parameters.get(name)
    }
    
    pub fn get_all_parameters(&self) -> HashMap<String, ParameterValue> {
        self.parameters.iter()
            .map(|(name, def)| (name.clone(), def.current_value.clone()))
            .collect()
    }
    
    pub fn get_parameters_by_category(&self, category: &ParameterCategory) -> Vec<&ParameterDefinition> {
        self.parameters.values()
            .filter(|p| std::mem::discriminant(&p.category) == std::mem::discriminant(category))
            .collect()
    }
    
    pub fn reset_parameter(&mut self, name: &str) -> Result<()> {
        let (old_value, new_value) = {
            let parameter = self.parameters.get_mut(name)
                .ok_or_else(|| anyhow!("Parameter '{}' not found", name))?;

            let old_value = parameter.current_value.clone();
            parameter.current_value = parameter.default_value.clone();
            let new_value = parameter.current_value.clone();
            info!("Parameter '{}' reset to default value: {}", name, parameter.current_value);
            (old_value, new_value)
        }; // Mutable borrow ends here

        self.record_change(name, old_value, new_value, "reset".to_string(), "system".to_string());        
        
        Ok(())
    }
    
    pub fn get_change_history(&self, parameter_name: Option<&str>) -> Vec<&ParameterChange> {
        match parameter_name {
            Some(name) => self.change_history.iter()
                .filter(|change| change.parameter_name == name)
                .collect(),
            None => self.change_history.iter().collect(),
        }
    }
    
    pub fn can_tune_parameter(&self, name: &str) -> Result<bool> {
        let parameter = self.parameters.get(name)
            .ok_or_else(|| anyhow!("Parameter '{}' not found", name))?;
        
        match parameter.tuning_frequency {
            TuningFrequency::Manual => Ok(false),
            TuningFrequency::Continuous => Ok(true),
            TuningFrequency::Hourly => {
                // Check if last change was more than an hour ago
                let one_hour_ago = chrono::Utc::now().timestamp() as u64 - 3600;
                let last_change = self.change_history.iter()
                    .filter(|c| c.parameter_name == name)
                    .last();
                
                match last_change {
                    Some(change) => Ok(change.timestamp < one_hour_ago),
                    None => Ok(true),
                }
            }
            TuningFrequency::Daily => {
                // Check if last change was more than a day ago
                let one_day_ago = chrono::Utc::now().timestamp() as u64 - 86400;
                let last_change = self.change_history.iter()
                    .filter(|c| c.parameter_name == name)
                    .last();
                
                match last_change {
                    Some(change) => Ok(change.timestamp < one_day_ago),
                    None => Ok(true),
                }
            }
        }
    }
    
    fn record_change(&mut self, name: &str, old_value: ParameterValue, new_value: ParameterValue, reason: String, triggered_by: String) {
        let change = ParameterChange {
            timestamp: chrono::Utc::now().timestamp() as u64,
            parameter_name: name.to_string(),
            old_value,
            new_value,
            reason,
            triggered_by,
        };
        
        self.change_history.push(change);
        
        // Trim history if it gets too large
        if self.change_history.len() > self.max_history_size {
            self.change_history.remove(0);
        }
    }
    
    fn initialize_default_parameters(&mut self) {
        // Connection parameters
        let connection_params = vec![
            ParameterDefinition {
                name: "connection_pool_max_size".to_string(),
                description: "Maximum number of connections per platform".to_string(),
                category: ParameterCategory::Connection,
                default_value: ParameterValue::Integer(3),
                current_value: ParameterValue::Integer(3),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(1)),
                    max_value: Some(ParameterValue::Integer(10)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(1)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Hourly,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "connection_timeout_ms".to_string(),
                description: "Connection timeout in milliseconds".to_string(),
                category: ParameterCategory::Connection,
                default_value: ParameterValue::Duration(30000),
                current_value: ParameterValue::Duration(30000),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Duration(5000)),
                    max_value: Some(ParameterValue::Duration(120000)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Duration(5000)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "connection_retry_attempts".to_string(),
                description: "Number of connection retry attempts".to_string(),
                category: ParameterCategory::Connection,
                default_value: ParameterValue::Integer(3),
                current_value: ParameterValue::Integer(3),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(1)),
                    max_value: Some(ParameterValue::Integer(10)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(1)),
                },
                impact_level: ImpactLevel::Low,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
        ];
        
        // Memory parameters
        let memory_params = vec![
            ParameterDefinition {
                name: "message_cache_size".to_string(),
                description: "Maximum number of messages to cache".to_string(),
                category: ParameterCategory::Memory,
                default_value: ParameterValue::Integer(1000),
                current_value: ParameterValue::Integer(1000),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(100)),
                    max_value: Some(ParameterValue::Integer(10000)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(100)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "gc_threshold_percent".to_string(),
                description: "Memory usage threshold to trigger garbage collection".to_string(),
                category: ParameterCategory::Memory,
                default_value: ParameterValue::Float(80.0),
                current_value: ParameterValue::Float(80.0),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Float(50.0)),
                    max_value: Some(ParameterValue::Float(95.0)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Float(5.0)),
                },
                impact_level: ImpactLevel::High,
                tuning_frequency: TuningFrequency::Hourly,
                dependencies: vec![],
            },
        ];
        
        // Processing parameters
        let processing_params = vec![
            ParameterDefinition {
                name: "message_processing_batch_size".to_string(),
                description: "Number of messages to process in a batch".to_string(),
                category: ParameterCategory::Processing,
                default_value: ParameterValue::Integer(10),
                current_value: ParameterValue::Integer(10),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(1)),
                    max_value: Some(ParameterValue::Integer(100)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(5)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "processing_queue_max_size".to_string(),
                description: "Maximum size of the message processing queue".to_string(),
                category: ParameterCategory::Processing,
                default_value: ParameterValue::Integer(1000),
                current_value: ParameterValue::Integer(1000),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(100)),
                    max_value: Some(ParameterValue::Integer(10000)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(100)),
                },
                impact_level: ImpactLevel::High,
                tuning_frequency: TuningFrequency::Hourly,
                dependencies: vec!["message_processing_batch_size".to_string()],
            },
            ParameterDefinition {
                name: "worker_thread_count".to_string(),
                description: "Number of worker threads for message processing".to_string(),
                category: ParameterCategory::Processing,
                default_value: ParameterValue::Integer(4),
                current_value: ParameterValue::Integer(4),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(1)),
                    max_value: Some(ParameterValue::Integer(16)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(1)),
                },
                impact_level: ImpactLevel::High,
                tuning_frequency: TuningFrequency::Hourly,
                dependencies: vec![],
            },
        ];
        
        // AI/Moderation parameters
        let ai_params = vec![
            ParameterDefinition {
                name: "ai_confidence_threshold".to_string(),
                description: "Minimum confidence level for AI moderation actions".to_string(),
                category: ParameterCategory::AI,
                default_value: ParameterValue::Float(0.8),
                current_value: ParameterValue::Float(0.8),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Float(0.1)),
                    max_value: Some(ParameterValue::Float(1.0)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Float(0.05)),
                },
                impact_level: ImpactLevel::Critical,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "pattern_matching_timeout_ms".to_string(),
                description: "Timeout for pattern matching operations".to_string(),
                category: ParameterCategory::AI,
                default_value: ParameterValue::Duration(500),
                current_value: ParameterValue::Duration(500),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Duration(100)),
                    max_value: Some(ParameterValue::Duration(5000)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Duration(100)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "learning_rate".to_string(),
                description: "Learning rate for AI model adaptation".to_string(),
                category: ParameterCategory::AI,
                default_value: ParameterValue::Float(0.001),
                current_value: ParameterValue::Float(0.001),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Float(0.0001)),
                    max_value: Some(ParameterValue::Float(0.1)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Float(0.0001)),
                },
                impact_level: ImpactLevel::High,
                tuning_frequency: TuningFrequency::Daily,
                dependencies: vec![],
            },
        ];
        
        // Cache parameters
        let cache_params = vec![
            ParameterDefinition {
                name: "response_cache_ttl_seconds".to_string(),
                description: "Time-to-live for cached responses".to_string(),
                category: ParameterCategory::Cache,
                default_value: ParameterValue::Duration(300000), // 5 minutes in ms
                current_value: ParameterValue::Duration(300000),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Duration(60000)), // 1 minute
                    max_value: Some(ParameterValue::Duration(3600000)), // 1 hour
                    allowed_values: None,
                    step_size: Some(ParameterValue::Duration(60000)),
                },
                impact_level: ImpactLevel::Low,
                tuning_frequency: TuningFrequency::Continuous,
                dependencies: vec![],
            },
            ParameterDefinition {
                name: "cache_max_entries".to_string(),
                description: "Maximum number of entries in the cache".to_string(),
                category: ParameterCategory::Cache,
                default_value: ParameterValue::Integer(1000),
                current_value: ParameterValue::Integer(1000),
                constraints: ParameterConstraints {
                    min_value: Some(ParameterValue::Integer(100)),
                    max_value: Some(ParameterValue::Integer(10000)),
                    allowed_values: None,
                    step_size: Some(ParameterValue::Integer(100)),
                },
                impact_level: ImpactLevel::Medium,
                tuning_frequency: TuningFrequency::Hourly,
                dependencies: vec![],
            },
        ];
        
        // Register all parameters
        for param in connection_params.into_iter()
            .chain(memory_params.into_iter())
            .chain(processing_params.into_iter())
            .chain(ai_params.into_iter())
            .chain(cache_params.into_iter()) {
            if let Err(e) = self.register_parameter(param) {
                error!("Failed to register default parameter: {}", e);
            }
        }
        
        info!("Initialized {} default parameters", self.parameters.len());
    }
}

/// Parameter suggestion for tuning adjustments
#[derive(Debug, Clone)]
pub struct ParameterSuggestion {
    pub parameter_name: String,
    pub current_value: ParameterValue,
    pub suggested_value: ParameterValue,
    pub confidence: f64,
    pub reason: String,
    pub expected_improvement: f64,
}

/// Parameter tuning strategy interface
pub trait ParameterTuningStrategy {
    fn suggest_adjustments(&self, metrics: &crate::adaptive::PerformanceMetrics, parameters: &ParameterStore) -> Vec<ParameterSuggestion>;
    fn get_strategy_name(&self) -> &str;
    fn get_priority(&self) -> u8; // 0-255, higher is more priority
}

/// Utility functions for parameter operations
pub struct ParameterUtils;

impl ParameterUtils {
    /// Calculate the relative change between two parameter values
    pub fn calculate_change_percentage(old_value: &ParameterValue, new_value: &ParameterValue) -> Option<f64> {
        match (old_value, new_value) {
            (ParameterValue::Integer(old), ParameterValue::Integer(new)) => {
                if *old == 0 { return None; }
                Some(((*new - *old) as f64 / *old as f64) * 100.0)
            }
            (ParameterValue::Float(old), ParameterValue::Float(new)) => {
                if *old == 0.0 { return None; }
                Some(((new - old) / old) * 100.0)
            }
            (ParameterValue::Duration(old), ParameterValue::Duration(new)) => {
                if *old == 0 { return None; }
                Some(((*new as f64 - *old as f64) / *old as f64) * 100.0)
            }
            _ => None,
        }
    }
    
    /// Interpolate between two parameter values
    pub fn interpolate(from: &ParameterValue, to: &ParameterValue, factor: f64) -> Option<ParameterValue> {
        let factor = factor.clamp(0.0, 1.0);
        
        match (from, to) {
            (ParameterValue::Integer(from), ParameterValue::Integer(to)) => {
                let diff = *to - *from;
                let new_value = *from + ((diff as f64 * factor) as i64);
                Some(ParameterValue::Integer(new_value))
            }
            (ParameterValue::Float(from), ParameterValue::Float(to)) => {
                let new_value = from + ((to - from) * factor);
                Some(ParameterValue::Float(new_value))
            }
            (ParameterValue::Duration(from), ParameterValue::Duration(to)) => {
                let diff = *to as f64 - *from as f64;
                let new_value = *from as f64 + (diff * factor);
                Some(ParameterValue::Duration(new_value as u64))
            }
            _ => None,
        }
    }
    
    /// Apply constraints to a parameter value
    pub fn apply_constraints(value: ParameterValue, constraints: &ParameterConstraints) -> ParameterValue {
        // Apply min/max constraints
        let constrained = match (&value, &constraints.min_value, &constraints.max_value) {
            (ParameterValue::Integer(v), Some(ParameterValue::Integer(min)), Some(ParameterValue::Integer(max))) => {
                ParameterValue::Integer((*v).clamp(*min, *max))
            }
            (ParameterValue::Float(v), Some(ParameterValue::Float(min)), Some(ParameterValue::Float(max))) => {
                ParameterValue::Float(v.clamp(*min, *max))
            }
            (ParameterValue::Duration(v), Some(ParameterValue::Duration(min)), Some(ParameterValue::Duration(max))) => {
                ParameterValue::Duration((*v).clamp(*min, *max))
            }
            _ => value,
        };
        
        // Apply step size constraints
        if let Some(step) = &constraints.step_size {
            Self::round_to_step(constrained, step)
        } else {
            constrained
        }
    }
    
    /// Round a value to the nearest step size
    pub fn round_to_step(value: ParameterValue, step: &ParameterValue) -> ParameterValue {
        match (&value, step) {
            (ParameterValue::Integer(v), ParameterValue::Integer(s)) => {
                let rounded = (*v / *s) * *s;
                ParameterValue::Integer(rounded)
            }
            (ParameterValue::Float(v), ParameterValue::Float(s)) => {
                let rounded = (v / s).round() * s;
                ParameterValue::Float(rounded)
            }
            (ParameterValue::Duration(v), ParameterValue::Duration(s)) => {
                let rounded = (*v / *s) * *s;
                ParameterValue::Duration(rounded)
            }
            _ => value,
        }
    }
    
    /// Validate parameter dependencies
    pub fn validate_dependencies(store: &ParameterStore, parameter_name: &str, new_value: &ParameterValue) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        
        if let Some(param_def) = store.get_parameter_definition(parameter_name) {
            for dependency in &param_def.dependencies {
                if let Some(dep_value) = store.get_parameter(dependency) {
                    // Check for logical conflicts
                    match (parameter_name, dependency.as_str(), new_value, dep_value) {
                        ("processing_queue_max_size", "message_processing_batch_size", 
                         ParameterValue::Integer(queue_size), ParameterValue::Integer(batch_size)) => {
                            if queue_size < batch_size {
                                warnings.push(format!(
                                    "Queue size ({}) should be larger than batch size ({})", 
                                    queue_size, batch_size
                                ));
                            }
                        }
                        _ => {} // Add more dependency checks as needed
                    }
                }
            }
        }
        
        Ok(warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parameter_value_conversions() {
        let int_val = ParameterValue::Integer(42);
        assert_eq!(int_val.as_i64(), Some(42));
        assert_eq!(int_val.as_f64(), Some(42.0));
        
        let float_val = ParameterValue::Float(3.14);
        assert_eq!(float_val.as_f64(), Some(3.14));
        assert_eq!(float_val.as_i64(), Some(3));
        
        let bool_val = ParameterValue::Boolean(true);
        assert_eq!(bool_val.as_bool(), Some(true));
        
        let duration_val = ParameterValue::Duration(5000);
        assert_eq!(duration_val.as_duration_ms(), Some(5000));
    }
    
    #[test]
    fn test_parameter_constraints() {
        let constraints = ParameterConstraints {
            min_value: Some(ParameterValue::Integer(1)),
            max_value: Some(ParameterValue::Integer(10)),
            allowed_values: None,
            step_size: None,
        };
        
        assert!(constraints.validate(&ParameterValue::Integer(5)).is_ok());
        assert!(constraints.validate(&ParameterValue::Integer(0)).is_err());
        assert!(constraints.validate(&ParameterValue::Integer(15)).is_err());
    }
    
    #[test]
    fn test_parameter_store_operations() {
        let mut store = ParameterStore::new();
        
        // Test getting a default parameter
        let connection_timeout = store.get_parameter("connection_timeout_ms");
        assert!(connection_timeout.is_some());
        
        // Test setting a parameter
        let result = store.set_parameter("connection_timeout_ms", ParameterValue::Duration(45000));
        assert!(result.is_ok());
        
        // Test getting the updated value
        let updated_value = store.get_parameter("connection_timeout_ms");
        assert_eq!(updated_value, Some(&ParameterValue::Duration(45000)));
        
        // Test change history
        let history = store.get_change_history(Some("connection_timeout_ms"));
        assert_eq!(history.len(), 1);
    }
    
    #[test]
    fn test_parameter_utils() {
        let old_value = ParameterValue::Integer(100);
        let new_value = ParameterValue::Integer(150);
        
        let change_percent = ParameterUtils::calculate_change_percentage(&old_value, &new_value);
        assert_eq!(change_percent, Some(50.0));
        
        let interpolated = ParameterUtils::interpolate(&old_value, &new_value, 0.5);
        assert_eq!(interpolated, Some(ParameterValue::Integer(125)));
    }
    
    #[test]
    fn test_step_size_rounding() {
        let value = ParameterValue::Float(17.3);
        let step = ParameterValue::Float(5.0);
        
        let rounded = ParameterUtils::round_to_step(value, &step);
        assert_eq!(rounded, ParameterValue::Float(15.0));
    }
}