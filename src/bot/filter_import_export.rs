use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use log::{info};
use std::path::Path;
use tokio::fs;

use crate::types::{SpamFilter, SpamFilterType, BlacklistPattern, ExemptionLevel, ModerationEscalation, ModerationAction};

/// Exportable filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterExport {
    pub version: String,
    pub exported_at: DateTime<Utc>,
    pub exported_by: String,
    pub bot_version: String,
    pub description: String,
    pub tags: Vec<String>,
    pub filters: Vec<ExportableFilter>,
    pub metadata: ExportMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportableFilter {
    pub name: String,
    pub filter_type: SerializableSpamFilterType,
    pub enabled: bool,
    pub escalation: SerializableModerationEscalation,
    pub exemption_level: String, // Serialized as string for compatibility
    pub silent_mode: bool,
    pub custom_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub effectiveness_stats: Option<EffectivenessStats>,
    pub usage_context: Vec<String>, // Recommended contexts for this filter
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub total_filters: usize,
    pub filter_types: HashMap<String, usize>,
    pub estimated_accuracy: f64,
    pub recommended_for: Vec<String>, // Channel types, languages, etc.
    pub compatibility: Vec<String>,   // Platform compatibility
    pub author: String,
    pub license: String,
    pub update_url: Option<String>,   // URL for filter updates
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivenessStats {
    pub total_triggers: u64,
    pub accuracy: f64,
    pub false_positive_rate: f64,
    pub average_response_time_ms: f64,
    pub user_satisfaction_score: f64,
}

/// Serializable versions of internal types for cross-compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableSpamFilterType {
    ExcessiveCaps { max_percentage: u8 },
    LinkBlocking { allow_mods: bool, whitelist: Vec<String> },
    RepeatedMessages { max_repeats: u8, window_seconds: u64 },
    MessageLength { max_length: usize },
    ExcessiveEmotes { max_count: u8 },
    SymbolSpam { max_percentage: u8 },
    RateLimit { max_messages: u8, window_seconds: u64 },
    Blacklist {
        patterns: Vec<SerializableBlacklistPattern>,
        case_sensitive: bool,
        whole_words_only: bool,
    },
    // Enhanced patterns for Phase 2
    AdvancedPattern {
        pattern_type: String,
        pattern_data: serde_json::Value,
        threshold: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableBlacklistPattern {
    Literal(String),
    Wildcard(String),
    Regex { pattern: String, flags: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableModerationEscalation {
    pub first_offense: SerializableModerationAction,
    pub repeat_offense: SerializableModerationAction,
    pub offense_window_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableModerationAction {
    DeleteMessage,
    TimeoutUser { duration_seconds: u64 },
    WarnUser { message: String },
    LogOnly,
}

/// Filter import/export manager
pub struct FilterImportExport {
    supported_versions: Vec<String>,
    compatibility_matrix: HashMap<String, Vec<String>>,
}

impl FilterImportExport {
    pub fn new() -> Self {
        Self {
            supported_versions: vec!["1.0".to_string(), "1.1".to_string(), "2.0".to_string()],
            compatibility_matrix: Self::build_compatibility_matrix(),
        }
    }

    /// Export filters to various formats
    pub async fn export_filters(
        &self,
        filters: &HashMap<String, SpamFilter>,
        format: ExportFormat,
        output_path: &Path,
        metadata: ExportOptions,
    ) -> Result<()> {
        let export_data = self.prepare_export_data(filters, metadata).await?;
        
        match format {
            ExportFormat::Json => self.export_json(&export_data, output_path).await,
            ExportFormat::Yaml => self.export_yaml(&export_data, output_path).await,
            ExportFormat::Toml => self.export_toml(&export_data, output_path).await,
            ExportFormat::NightBotCompatible => self.export_nightbot_format(&export_data, output_path).await,
            ExportFormat::StreamlabsCompatible => self.export_streamlabs_format(&export_data, output_path).await,
            ExportFormat::CompressedArchive => self.export_compressed(&export_data, output_path).await,
        }
    }

    /// Import filters from various formats
    pub async fn import_filters(
        &self,
        input_path: &Path,
        format: Option<ExportFormat>,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let detected_format = if let Some(fmt) = format {
            fmt
        } else {
            self.detect_format(input_path).await?
        };

        let import_data = match detected_format {
            ExportFormat::Json => self.import_json(input_path).await?,
            ExportFormat::Yaml => self.import_yaml(input_path).await?,
            ExportFormat::Toml => self.import_toml(input_path).await?,
            ExportFormat::NightBotCompatible => self.import_nightbot_format(input_path).await?,
            ExportFormat::StreamlabsCompatible => self.import_streamlabs_format(input_path).await?,
            ExportFormat::CompressedArchive => self.import_compressed(input_path).await?,
        };

        self.process_import(import_data, options).await
    }

    /// Export to NightBot compatible format
    async fn export_nightbot_format(&self, export_data: &FilterExport, output_path: &Path) -> Result<()> {
        let mut nightbot_data = serde_json::Map::new();
        let mut blacklist_patterns = Vec::new();
        
        for filter in &export_data.filters {
            match &filter.filter_type {
                SerializableSpamFilterType::Blacklist { patterns, .. } => {
                    for pattern in patterns {
                        let nightbot_pattern = match pattern {
                            SerializableBlacklistPattern::Literal(text) => text.clone(),
                            SerializableBlacklistPattern::Wildcard(text) => text.clone(),
                            SerializableBlacklistPattern::Regex { pattern, .. } => {
                                format!("~/{}/", pattern)
                            },
                        };
                        blacklist_patterns.push(nightbot_pattern);
                    }
                }
                _ => {
                    // Convert other filter types to equivalent blacklist patterns where possible
                    if let Some(equivalent) = self.convert_to_nightbot_equivalent(filter) {
                        blacklist_patterns.extend(equivalent);
                    }
                }
            }
        }

        // NightBot format structure
        nightbot_data.insert("blacklist".to_string(), serde_json::json!({
            "enabled": true,
            "list": blacklist_patterns,
            "timeout": 600,
            "exempt": "moderator",
            "silent": false,
            "message": "Please watch your language!"
        }));

        nightbot_data.insert("exported_from".to_string(), serde_json::json!("NotaBot"));
        nightbot_data.insert("version".to_string(), serde_json::json!("nightbot_compatible_1.0"));
        nightbot_data.insert("export_date".to_string(), serde_json::json!(export_data.exported_at));

        let json_string = serde_json::to_string_pretty(&nightbot_data)?;
        fs::write(output_path, json_string).await
            .context("Failed to write NightBot compatible export")
    }

    /// Import from NightBot format
    async fn import_nightbot_format(&self, input_path: &Path) -> Result<FilterExport> {
        let content = fs::read_to_string(input_path).await
            .context("Failed to read NightBot import file")?;
        
        let nightbot_data: serde_json::Value = serde_json::from_str(&content)
            .context("Failed to parse NightBot JSON")?;

        let mut filters = Vec::new();

        // Parse blacklist
        if let Some(blacklist) = nightbot_data.get("blacklist") {
            if let Some(patterns) = blacklist.get("list").and_then(|l| l.as_array()) {
                let mut blacklist_patterns = Vec::new();
                
                for pattern_val in patterns {
                    if let Some(pattern_str) = pattern_val.as_str() {
                        let blacklist_pattern = if pattern_str.starts_with("~/") && pattern_str.ends_with("/") {
                            // Regex pattern
                            let regex_content = &pattern_str[2..pattern_str.len()-1];
                            SerializableBlacklistPattern::Regex {
                                pattern: regex_content.to_string(),
                                flags: "i".to_string(), // Default to case insensitive
                            }
                        } else if pattern_str.contains('*') {
                            // Wildcard pattern
                            SerializableBlacklistPattern::Wildcard(pattern_str.to_string())
                        } else {
                            // Literal pattern
                            SerializableBlacklistPattern::Literal(pattern_str.to_string())
                        };
                        
                        blacklist_patterns.push(blacklist_pattern);
                    }
                }

                if !blacklist_patterns.is_empty() {
                    let timeout = blacklist.get("timeout").and_then(|t| t.as_u64()).unwrap_or(600);
                    let silent = blacklist.get("silent").and_then(|s| s.as_bool()).unwrap_or(false);
                    let custom_message = blacklist.get("message").and_then(|m| m.as_str()).map(|s| s.to_string());

                    filters.push(ExportableFilter {
                        name: "imported_blacklist".to_string(),
                        filter_type: SerializableSpamFilterType::Blacklist {
                            patterns: blacklist_patterns,
                            case_sensitive: false,
                            whole_words_only: false,
                        },
                        enabled: blacklist.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                        escalation: SerializableModerationEscalation {
                            first_offense: SerializableModerationAction::WarnUser {
                                message: custom_message.clone().unwrap_or_else(|| "Please follow chat rules".to_string())
                            },
                            repeat_offense: SerializableModerationAction::TimeoutUser { duration_seconds: timeout },
                            offense_window_seconds: 3600,
                        },
                        exemption_level: "Moderator".to_string(),
                        silent_mode: silent,
                        custom_message,
                        created_at: Utc::now(),
                        effectiveness_stats: None,
                        usage_context: vec!["general".to_string()],
                    });
                }
            }
        }

        let total_filters = filters.len();  // Calculate before the move
        Ok(FilterExport {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            exported_by: "NightBot Import".to_string(),
            bot_version: "nightbot_import".to_string(),
            description: "Imported from NightBot configuration".to_string(),
            tags: vec!["imported".to_string(), "nightbot".to_string()],
            filters,
            metadata: ExportMetadata {
                total_filters,
                filter_types: HashMap::new(),
                estimated_accuracy: 0.8, // Conservative estimate
                recommended_for: vec!["general".to_string()],
                compatibility: vec!["twitch".to_string(), "youtube".to_string()],
                author: "Unknown".to_string(),
                license: "Imported".to_string(),
                update_url: None,
            },
        })
    }

    /// Convert other filter types to NightBot equivalent patterns
    fn convert_to_nightbot_equivalent(&self, filter: &ExportableFilter) -> Option<Vec<String>> {
        match &filter.filter_type {
            SerializableSpamFilterType::ExcessiveCaps { max_percentage } => {
                // Create a regex pattern that matches excessive caps
                Some(vec![format!("~/[A-Z]{{{},%}}/", max_percentage * 10 / 100)])
            }
            SerializableSpamFilterType::LinkBlocking { whitelist, .. } => {
                // Create patterns to block common link formats while allowing whitelist
                let mut patterns = vec![
                    "~/https?:\\/\\//".to_string(),
                    "~/www\\./".to_string(),
                    "~/\\.(com|net|org|tv)/".to_string(),
                ];
                
                // Add negative lookahead for whitelisted domains (if regex engine supports it)
                for domain in whitelist {
                    patterns.push(format!("~/(?!.*{}).*\\.(com|net|org)/", regex::escape(domain)));
                }
                
                Some(patterns)
            }
            SerializableSpamFilterType::SymbolSpam { max_percentage } => {
                // Pattern to catch excessive symbols
                Some(vec![format!("~/[^\\w\\s]{{{},}}/", max_percentage / 10)])
            }
            _ => None, // Other types don't have direct NightBot equivalents
        }
    }

    /// Prepare export data from internal filter format
    async fn prepare_export_data(&self, filters: &HashMap<String, SpamFilter>, options: ExportOptions) -> Result<FilterExport> {
        let mut exportable_filters = Vec::new();
        let mut filter_types = HashMap::new();
        
        for (name, filter) in filters {
            let serializable_type = self.convert_to_serializable(&filter.filter_type)?;
            let type_name = format!("{:?}", serializable_type).split('{').next().unwrap_or("Unknown").to_string();
            *filter_types.entry(type_name).or_insert(0) += 1;
            
            exportable_filters.push(ExportableFilter {
                name: name.clone(),
                filter_type: serializable_type,
                enabled: filter.enabled,
                escalation: SerializableModerationEscalation {
                    first_offense: self.convert_action_to_serializable(&filter.escalation.first_offense),
                    repeat_offense: self.convert_action_to_serializable(&filter.escalation.repeat_offense),
                    offense_window_seconds: filter.escalation.offense_window_seconds,
                },
                exemption_level: format!("{:?}", filter.exemption_level),
                silent_mode: filter.silent_mode,
                custom_message: filter.custom_message.clone(),
                created_at: Utc::now(), // Would be stored in actual implementation
                effectiveness_stats: None, // Would be populated from analytics
                usage_context: vec!["general".to_string()], // Default context
            });
        }

        Ok(FilterExport {
            version: "2.0".to_string(),
            exported_at: Utc::now(),
            exported_by: options.exported_by,
            bot_version: env!("CARGO_PKG_VERSION").to_string(),
            description: options.description,
            tags: options.tags,
            filters: exportable_filters,
            metadata: ExportMetadata {
                total_filters: filters.len(),
                filter_types,
                estimated_accuracy: 0.9, // Would be calculated from analytics
                recommended_for: options.recommended_for,
                compatibility: vec!["notabot".to_string(), "twitch".to_string(), "youtube".to_string()],
                author: options.author,
                license: options.license,
                update_url: options.update_url,
            },
        })
    }

    /// Convert internal filter type to serializable format
    fn convert_to_serializable(&self, filter_type: &SpamFilterType) -> Result<SerializableSpamFilterType> {
        let serializable = match filter_type {
            SpamFilterType::ExcessiveCaps { max_percentage } => {
                SerializableSpamFilterType::ExcessiveCaps { max_percentage: *max_percentage }
            }
            SpamFilterType::LinkBlocking { allow_mods, whitelist } => {
                SerializableSpamFilterType::LinkBlocking {
                    allow_mods: *allow_mods,
                    whitelist: whitelist.clone(),
                }
            }
            SpamFilterType::RepeatedMessages { max_repeats, window_seconds } => {
                SerializableSpamFilterType::RepeatedMessages {
                    max_repeats: *max_repeats,
                    window_seconds: *window_seconds,
                }
            }
            SpamFilterType::MessageLength { max_length } => {
                SerializableSpamFilterType::MessageLength { max_length: *max_length }
            }
            SpamFilterType::ExcessiveEmotes { max_count } => {
                SerializableSpamFilterType::ExcessiveEmotes { max_count: *max_count }
            }
            SpamFilterType::SymbolSpam { max_percentage } => {
                SerializableSpamFilterType::SymbolSpam { max_percentage: *max_percentage }
            }
            SpamFilterType::RateLimit { max_messages, window_seconds } => {
                SerializableSpamFilterType::RateLimit {
                    max_messages: *max_messages,
                    window_seconds: *window_seconds,
                }
            }
            SpamFilterType::Blacklist { patterns, case_sensitive, whole_words_only } => {
                let serializable_patterns = patterns.iter()
                    .map(|p| self.convert_pattern_to_serializable(p))
                    .collect::<Result<Vec<_>>>()?;
                
                SerializableSpamFilterType::Blacklist {
                    patterns: serializable_patterns,
                    case_sensitive: *case_sensitive,
                    whole_words_only: *whole_words_only,
                }
            }
        };
        
        Ok(serializable)
    }

    /// Convert blacklist pattern to serializable format
    fn convert_pattern_to_serializable(&self, pattern: &BlacklistPattern) -> Result<SerializableBlacklistPattern> {
        let serializable = match pattern {
            BlacklistPattern::Literal(text) => SerializableBlacklistPattern::Literal(text.clone()),
            BlacklistPattern::Wildcard(text) => SerializableBlacklistPattern::Wildcard(text.clone()),
            BlacklistPattern::Regex { pattern, .. } => {
                // Extract flags from the full pattern string
                let flags = if pattern.contains("/i") { "i" } else { "" };
                let clean_pattern = pattern.trim_start_matches("~/").trim_end_matches("/").trim_end_matches("/i");
                
                SerializableBlacklistPattern::Regex {
                    pattern: clean_pattern.to_string(),
                    flags: flags.to_string(),
                }
            }
        };
        
        Ok(serializable)
    }

    /// Convert moderation action to serializable format
    fn convert_action_to_serializable(&self, action: &ModerationAction) -> SerializableModerationAction {
        match action {
            ModerationAction::DeleteMessage => SerializableModerationAction::DeleteMessage,
            ModerationAction::TimeoutUser { duration_seconds } => {
                SerializableModerationAction::TimeoutUser { duration_seconds: *duration_seconds }
            }
            ModerationAction::WarnUser { message } => {
                SerializableModerationAction::WarnUser { message: message.clone() }
            }
            ModerationAction::LogOnly => SerializableModerationAction::LogOnly,
        }
    }

    /// Export to JSON format
    async fn export_json(&self, export_data: &FilterExport, output_path: &Path) -> Result<()> {
        let json_string = serde_json::to_string_pretty(export_data)
            .context("Failed to serialize to JSON")?;
        
        fs::write(output_path, json_string).await
            .context("Failed to write JSON export file")
    }

    /// Export to YAML format
    async fn export_yaml(&self, export_data: &FilterExport, output_path: &Path) -> Result<()> {
        let yaml_string = serde_yaml::to_string(export_data)
            .context("Failed to serialize to YAML")?;
        
        fs::write(output_path, yaml_string).await
            .context("Failed to write YAML export file")
    }

    /// Export to TOML format
    async fn export_toml(&self, export_data: &FilterExport, output_path: &Path) -> Result<()> {
        let toml_string = toml::to_string_pretty(export_data)
            .context("Failed to serialize to TOML")?;
        
        fs::write(output_path, toml_string).await
            .context("Failed to write TOML export file")
    }

    /// Export to compressed archive with multiple formats
    async fn export_compressed(&self, export_data: &FilterExport, output_path: &Path) -> Result<()> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        
        // Create a tar.gz archive with multiple formats
        let tar_gz = std::fs::File::create(output_path)
            .context("Failed to create compressed archive")?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = tar::Builder::new(enc);
        
        // Add JSON version
        let json_data = serde_json::to_string_pretty(export_data)?;
        let mut header = tar::Header::new_gnu();
        header.set_path("filters.json")?;
        header.set_size(json_data.len() as u64);
        header.set_cksum();
        tar.append(&header, json_data.as_bytes())?;
        
        // Add YAML version
        let yaml_data = serde_yaml::to_string(export_data)?;
        let mut header = tar::Header::new_gnu();
        header.set_path("filters.yaml")?;
        header.set_size(yaml_data.len() as u64);
        header.set_cksum();
        tar.append(&header, yaml_data.as_bytes())?;
        
        // Add README
        let readme_content = self.generate_readme(export_data);
        let mut header = tar::Header::new_gnu();
        header.set_path("README.md")?;
        header.set_size(readme_content.len() as u64);
        header.set_cksum();
        tar.append(&header, readme_content.as_bytes())?;
        
        tar.finish()?;
        info!("Exported filters to compressed archive: {}", output_path.display());
        Ok(())
    }

    /// Import from JSON format
    async fn import_json(&self, input_path: &Path) -> Result<FilterExport> {
        let content = fs::read_to_string(input_path).await
            .context("Failed to read JSON import file")?;
        
        serde_json::from_str(&content)
            .context("Failed to parse JSON import file")
    }

    /// Import from YAML format
    async fn import_yaml(&self, input_path: &Path) -> Result<FilterExport> {
        let content = fs::read_to_string(input_path).await
            .context("Failed to read YAML import file")?;
        
        serde_yaml::from_str(&content)
            .context("Failed to parse YAML import file")
    }

    /// Import from TOML format
    async fn import_toml(&self, input_path: &Path) -> Result<FilterExport> {
        let content = fs::read_to_string(input_path).await
            .context("Failed to read TOML import file")?;
        
        toml::from_str(&content)
            .context("Failed to parse TOML import file")
    }

    /// Import from compressed archive
    async fn import_compressed(&self, input_path: &Path) -> Result<FilterExport> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let file = std::fs::File::open(input_path)
            .context("Failed to open compressed archive")?;
        let dec = GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);
        
        // Look for filters.json first, then filters.yaml
        for entry in archive.entries()? {
            let mut entry = entry?;
            if let Ok(path) = entry.header().path() {
                if path == Path::new("filters.json") {
                    let mut contents = String::new();
                    entry.read_to_string(&mut contents)?;
                    return serde_json::from_str(&contents)
                        .context("Failed to parse JSON from archive");
                } else if path == Path::new("filters.yaml") {
                    let mut contents = String::new();
                    entry.read_to_string(&mut contents)?;
                    return serde_yaml::from_str(&contents)
                        .context("Failed to parse YAML from archive");
                }
            }
        }
        
        Err(anyhow::anyhow!("No valid filter configuration found in archive"))
    }

    /// Detect file format from extension and content
    async fn detect_format(&self, input_path: &Path) -> Result<ExportFormat> {
        let extension = input_path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        
        match extension {
            "json" => Ok(ExportFormat::Json),
            "yaml" | "yml" => Ok(ExportFormat::Yaml),
            "toml" => Ok(ExportFormat::Toml),
            "gz" | "tar" | "tgz" => Ok(ExportFormat::CompressedArchive),
            _ => {
                // Try to detect from content
                let content = fs::read_to_string(input_path).await?;
                if content.trim_start().starts_with('{') {
                    Ok(ExportFormat::Json)
                } else if content.contains("version:") || content.contains("filters:") {
                    Ok(ExportFormat::Yaml)
                } else {
                    Err(anyhow::anyhow!("Could not detect file format"))
                }
            }
        }
    }

    /// Process imported data and convert to internal format
    async fn process_import(&self, import_data: FilterExport, options: ImportOptions) -> Result<ImportResult> {
        let mut imported_filters = HashMap::new();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Version compatibility check
        if !self.is_version_compatible(&import_data.version) {
            warnings.push(format!("Import version {} may not be fully compatible", import_data.version));
        }

        for filter in import_data.filters {
            match self.convert_from_serializable(&filter) {
                Ok(spam_filter) => {
                    let filter_name = if options.prefix_names {
                        format!("imported_{}", filter.name)
                    } else {
                        filter.name.clone()
                    };
                    
                    if imported_filters.contains_key(&filter_name) && !options.overwrite_existing {
                        warnings.push(format!("Skipped duplicate filter: {}", filter_name));
                        continue;
                    }
                    
                    imported_filters.insert(filter_name, spam_filter);
                }
                Err(e) => {
                    errors.push(format!("Failed to import filter '{}': {}", filter.name, e));
                }
            }
        }

        let imported_count = imported_filters.len();  // Calculate before the move
        Ok(ImportResult {
            filters: imported_filters,
            metadata: import_data.metadata,
            imported_count: imported_count,
            error_count: errors.len(),
            warning_count: warnings.len(),
            errors,
            warnings,
            source_info: ImportSourceInfo {
                version: import_data.version,
                exported_by: import_data.exported_by,
                exported_at: import_data.exported_at,
                description: import_data.description,
            },
        })
    }

    /// Check version compatibility
    fn is_version_compatible(&self, version: &str) -> bool {
        self.supported_versions.contains(&version.to_string()) ||
        self.compatibility_matrix.get(version).is_some()
    }

    /// Convert from serializable format to internal format
    fn convert_from_serializable(&self, filter: &ExportableFilter) -> Result<SpamFilter> {
        let filter_type = match &filter.filter_type {
            SerializableSpamFilterType::ExcessiveCaps { max_percentage } => {
                SpamFilterType::ExcessiveCaps { max_percentage: *max_percentage }
            }
            SerializableSpamFilterType::LinkBlocking { allow_mods, whitelist } => {
                SpamFilterType::LinkBlocking {
                    allow_mods: *allow_mods,
                    whitelist: whitelist.clone(),
                }
            }
            SerializableSpamFilterType::Blacklist { patterns, case_sensitive, whole_words_only } => {
                let internal_patterns = patterns.iter()
                    .map(|p| self.convert_pattern_from_serializable(p))
                    .collect::<Result<Vec<_>>>()?;
                
                SpamFilterType::Blacklist {
                    patterns: internal_patterns,
                    case_sensitive: *case_sensitive,
                    whole_words_only: *whole_words_only,
                }
            }
            // Add other conversions as needed
            _ => return Err(anyhow::anyhow!("Unsupported filter type in import")),
        };

        let exemption_level = match filter.exemption_level.as_str() {
            "None" => ExemptionLevel::None,
            "Subscriber" => ExemptionLevel::Subscriber,
            "Regular" => ExemptionLevel::Regular,
            "Moderator" => ExemptionLevel::Moderator,
            "Owner" => ExemptionLevel::Owner,
            _ => ExemptionLevel::Moderator, // Default fallback
        };

        Ok(SpamFilter {
            filter_type,
            enabled: filter.enabled,
            escalation: ModerationEscalation {
                first_offense: self.convert_action_from_serializable(&filter.escalation.first_offense),
                repeat_offense: self.convert_action_from_serializable(&filter.escalation.repeat_offense),
                offense_window_seconds: filter.escalation.offense_window_seconds,
            },
            exemption_level,
            silent_mode: filter.silent_mode,
            custom_message: filter.custom_message.clone(),
            name: filter.name.clone(),
        })
    }

    /// Convert pattern from serializable format
    fn convert_pattern_from_serializable(&self, pattern: &SerializableBlacklistPattern) -> Result<BlacklistPattern> {
        let internal_pattern = match pattern {
            SerializableBlacklistPattern::Literal(text) => BlacklistPattern::Literal(text.clone()),
            SerializableBlacklistPattern::Wildcard(text) => BlacklistPattern::Wildcard(text.clone()),
            SerializableBlacklistPattern::Regex { pattern, flags } => {
                let full_pattern = if flags.is_empty() {
                    format!("~/{}/", pattern)
                } else {
                    format!("~/{}/{}", pattern, flags)
                };
                BlacklistPattern::from_regex_string(&full_pattern)
                    .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?
            }
        };
        
        Ok(internal_pattern)
    }

    /// Convert action from serializable format
    fn convert_action_from_serializable(&self, action: &SerializableModerationAction) -> ModerationAction {
        match action {
            SerializableModerationAction::DeleteMessage => ModerationAction::DeleteMessage,
            SerializableModerationAction::TimeoutUser { duration_seconds } => {
                ModerationAction::TimeoutUser { duration_seconds: *duration_seconds }
            }
            SerializableModerationAction::WarnUser { message } => {
                ModerationAction::WarnUser { message: message.clone() }
            }
            SerializableModerationAction::LogOnly => ModerationAction::LogOnly,
        }
    }

    /// Generate README content for exports
    fn generate_readme(&self, export_data: &FilterExport) -> String {
        format!(
            r#"# NotaBot Filter Export

## Export Information
- **Version**: {}
- **Exported by**: {}
- **Export date**: {}
- **Bot version**: {}
- **Total filters**: {}

## Description
{}

## Tags
{}

## Compatibility
This filter pack is compatible with:
{}

## Usage
1. Import this file using NotaBot's filter import command
2. Review and adjust filters as needed for your community
3. Enable/disable filters based on your moderation needs

## Author
**{}**

## License
{}

---
*Generated by NotaBot - The NightBot Killer*
"#,
            export_data.version,
            export_data.exported_by,
            export_data.exported_at.format("%Y-%m-%d %H:%M:%S UTC"),
            export_data.bot_version,
            export_data.filters.len(),
            export_data.description,
            export_data.tags.join(", "),
            export_data.metadata.compatibility.join(", "),
            export_data.metadata.author,
            export_data.metadata.license
        )
    }

    /// Build compatibility matrix for version migrations
    fn build_compatibility_matrix() -> HashMap<String, Vec<String>> {
        let mut matrix = HashMap::new();
        
        // Version 1.0 can be upgraded to any newer version
        matrix.insert("1.0".to_string(), vec!["1.1".to_string(), "2.0".to_string()]);
        
        // Version 1.1 can be upgraded to 2.0
        matrix.insert("1.1".to_string(), vec!["2.0".to_string()]);
        
        matrix
    }

    /// Import Streamlabs format (placeholder)
    async fn import_streamlabs_format(&self, _input_path: &Path) -> Result<FilterExport> {
        // Placeholder for Streamlabs compatibility
        Err(anyhow::anyhow!("Streamlabs import not yet implemented"))
    }

    /// Export Streamlabs format (placeholder)
    async fn export_streamlabs_format(&self, _export_data: &FilterExport, _output_path: &Path) -> Result<()> {
        // Placeholder for Streamlabs compatibility
        Err(anyhow::anyhow!("Streamlabs export not yet implemented"))
    }
}

// Supporting types and enums
#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Yaml,
    Toml,
    NightBotCompatible,
    StreamlabsCompatible,
    CompressedArchive,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub exported_by: String,
    pub description: String,
    pub tags: Vec<String>,
    pub author: String,
    pub license: String,
    pub recommended_for: Vec<String>,
    pub update_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub overwrite_existing: bool,
    pub prefix_names: bool,
    pub validate_patterns: bool,
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct ImportResult {
    pub filters: HashMap<String, SpamFilter>,
    pub metadata: ExportMetadata,
    pub imported_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub source_info: ImportSourceInfo,
}

#[derive(Debug)]
pub struct ImportSourceInfo {
    pub version: String,
    pub exported_by: String,
    pub exported_at: DateTime<Utc>,
    pub description: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            exported_by: "NotaBot User".to_string(),
            description: "Filter configuration export".to_string(),
            tags: vec!["general".to_string()],
            author: "Anonymous".to_string(),
            license: "Public Domain".to_string(),
            recommended_for: vec!["general".to_string()],
            update_url: None,
        }
    }
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            overwrite_existing: false,
            prefix_names: true,
            validate_patterns: true,
            dry_run: false,
        }
    }
}