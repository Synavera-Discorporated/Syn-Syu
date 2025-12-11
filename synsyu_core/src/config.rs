/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::config
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Load Syn-Syu configuration from operator-defined sources,
    providing typed accessors and sensible defaults.

  Security / Safety Notes:
    Configuration paths are user-owned. The loader refuses to
    parse world-writable overrides to mitigate privilege risks.

  Dependencies:
    dirs for platform-specific config lookup, serde for parsing.

  Operational Scope:
    Consumed by manifest orchestration to tune network behavior,
    manifest location, and helper preferences.

  Revision History:
    2024-11-04 COD  Authored configuration subsystem.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Explicit defaults with documented precedence
    - Validation of untrusted configuration sources
    - Deterministic error reporting with context
============================================================*/

use std::fs;
use std::path::{Path, PathBuf};

use dirs::config_dir;
use serde::{Deserialize, Serialize};

use crate::error::{Result, SynsyuError};

/// Top-level configuration for Syn-Syu-Core.
#[derive(Debug, Deserialize, Clone)]
pub struct SynsyuConfig {
    #[serde(default)]
    pub aur: AurConfig,
    #[serde(default)]
    pub core: CoreConfig,
    #[serde(default)]
    pub helpers: HelperConfig,
    #[serde(default)]
    pub space: SpaceConfig,
    #[serde(default)]
    pub applications: ApplicationsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub snapshots: SnapshotsConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub clean: CleanConfig,
}

impl SynsyuConfig {
    /// Load configuration, applying defaults and overriding with file contents if present.
    pub fn load_from_optional_path(path: Option<&Path>) -> Result<Self> {
        let mut config = SynsyuConfig::default();
        if let Some(path) = path {
            if path.exists() {
                ensure_secure_permissions(path)?;
                let contents = fs::read_to_string(path).map_err(|err| {
                    SynsyuError::Filesystem(format!(
                        "Failed to read configuration {}: {err}",
                        path.display()
                    ))
                })?;
                let external: SynsyuConfig = toml::from_str(&contents).map_err(|err| {
                    SynsyuError::Config(format!(
                        "Failed to parse configuration {}: {err}",
                        path.display()
                    ))
                })?;
                config.merge(external);
            } else {
                return Err(SynsyuError::Config(format!(
                    "Configuration file {} does not exist",
                    path.display()
                )));
            }
        } else if let Some(default_path) = default_config_path() {
            if default_path.exists() {
                ensure_secure_permissions(&default_path)?;
                let contents = fs::read_to_string(&default_path).map_err(|err| {
                    SynsyuError::Filesystem(format!(
                        "Failed to read configuration {}: {err}",
                        default_path.display()
                    ))
                })?;
                let external: SynsyuConfig = toml::from_str(&contents).map_err(|err| {
                    SynsyuError::Config(format!(
                        "Failed to parse configuration {}: {err}",
                        default_path.display()
                    ))
                })?;
                config.merge(external);
            }
        }
        Ok(config)
    }

    fn merge(&mut self, other: SynsyuConfig) {
        self.aur = other.aur;
        self.core = other.core;
        self.helpers = other.helpers;
        self.space = other.space;
        self.applications = other.applications;
        self.logging = other.logging;
        self.snapshots = other.snapshots;
        self.safety = other.safety;
        self.clean = other.clean;
    }

    /// Manifest path resolved from configuration.
    pub fn manifest_path(&self) -> PathBuf {
        PathBuf::from(&self.core.manifest_path)
    }

    /// Optional log directory defined by operator.
    pub fn log_dir(&self) -> PathBuf {
        self.core
            .log_directory
            .as_ref()
            .map(|p| PathBuf::from(p.as_str()))
            .unwrap_or_else(default_log_dir)
    }

    /// Preferred helper priority order.
    #[allow(dead_code)]
    pub fn helper_priority(&self) -> &[String] {
        &self.helpers.priority
    }

    /// Minimum free bytes required before operations.
    pub fn min_free_bytes(&self) -> u64 {
        self.space.min_free_bytes()
    }

    /// Policy for handling low space relative to the configured buffer.
    pub fn space_policy(&self) -> SpacePolicy {
        self.space.policy
    }

    /// Whether flatpak application metadata should be collected by default.
    pub fn flatpak_enabled(&self) -> bool {
        self.applications.flatpak
    }

    /// Whether fwupd firmware metadata should be collected by default.
    pub fn fwupd_enabled(&self) -> bool {
        self.applications.fwupd
    }

    /// Snapshot of merged configuration suitable for reporting.
    pub fn to_report(&self) -> ConfigReport {
        ConfigReport {
            manifest_path: self.manifest_path(),
            log_directory: self.log_dir(),
            helper_priority: self.helpers.priority.clone(),
            helper_default: self.helpers.default.clone(),
            space_min_free_bytes: self.space.min_free_bytes(),
            space_policy: self.space.policy.to_string(),
            batch_size: self.core.batch_size,
            applications_flatpak: self.applications.flatpak,
            applications_fwupd: self.applications.fwupd,
            log_level: self.logging.level.clone(),
            log_retention_days: self.logging.retention_days,
            log_retention_megabytes: self.logging.retention_megabytes,
            snapshots_enabled: self.snapshots.enabled,
            snapshot_pre_command: self.snapshots.pre_command.clone(),
            snapshot_post_command: self.snapshots.post_command.clone(),
            snapshot_require_success: self.snapshots.require_success,
            safety_disk_check: self.safety.disk_check,
            safety_disk_margin_mb: self.safety.disk_extra_margin_mb,
            clean_keep_versions: self.clean.keep_versions,
            clean_remove_orphans: self.clean.remove_orphans,
            clean_check_pacnew: self.clean.check_pacnew,
        }
    }
}

impl Default for SynsyuConfig {
    fn default() -> Self {
        Self {
            aur: AurConfig::default(),
            core: CoreConfig::default(),
            helpers: HelperConfig::default(),
            space: SpaceConfig::default(),
            applications: ApplicationsConfig::default(),
            logging: LoggingConfig::default(),
            snapshots: SnapshotsConfig::default(),
            safety: SafetyConfig::default(),
            clean: CleanConfig::default(),
        }
    }
}

/// Configuration options for AUR interactions.
#[derive(Debug, Deserialize, Clone)]
pub struct AurConfig {
    #[serde(default = "AurConfig::default_base_url")]
    pub base_url: String,
    #[serde(default = "AurConfig::default_max_args")]
    pub max_args: usize,
    #[serde(default = "AurConfig::default_max_retries")]
    pub max_retries: usize,
    #[serde(default = "AurConfig::default_timeout_seconds")]
    pub timeout: u64,
}

impl AurConfig {
    fn default_base_url() -> String {
        "https://aur.archlinux.org/rpc/".to_string()
    }
    fn default_max_args() -> usize {
        50
    }
    fn default_max_retries() -> usize {
        3
    }
    fn default_timeout_seconds() -> u64 {
        10
    }
}

impl Default for AurConfig {
    fn default() -> Self {
        Self {
            base_url: Self::default_base_url(),
            max_args: Self::default_max_args(),
            max_retries: Self::default_max_retries(),
            timeout: Self::default_timeout_seconds(),
        }
    }
}

/// Configuration for core runtime.
#[derive(Debug, Deserialize, Clone)]
pub struct CoreConfig {
    #[serde(default = "CoreConfig::default_manifest_path")]
    pub manifest_path: String,
    #[serde(default)]
    pub log_directory: Option<String>,
    #[serde(default = "CoreConfig::default_batch_size")]
    #[allow(dead_code)]
    pub batch_size: usize,
}

impl CoreConfig {
    fn default_manifest_path() -> String {
        let base = config_dir().unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".config")
        });
        base.join("syn-syu")
            .join("manifest.json")
            .to_string_lossy()
            .into_owned()
    }

    fn default_batch_size() -> usize {
        10
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            manifest_path: Self::default_manifest_path(),
            log_directory: None,
            batch_size: Self::default_batch_size(),
        }
    }
}

/// Disk space requirements.
#[derive(Debug, Deserialize, Clone)]
pub struct SpaceConfig {
    #[serde(default = "SpaceConfig::default_min_free_gb")]
    pub min_free_gb: f64,
    #[serde(default = "SpaceConfig::default_policy")]
    pub policy: SpacePolicy,
}

impl SpaceConfig {
    fn default_min_free_gb() -> f64 {
        2.0
    }

    fn default_policy() -> SpacePolicy {
        SpacePolicy::Warn
    }

    pub fn min_free_bytes(&self) -> u64 {
        if self.min_free_gb <= 0.0 {
            0
        } else {
            (self.min_free_gb * 1024.0_f64 * 1024.0_f64 * 1024.0_f64).round() as u64
        }
    }
}

impl Default for SpaceConfig {
    fn default() -> Self {
        Self {
            min_free_gb: Self::default_min_free_gb(),
            policy: Self::default_policy(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SpacePolicy {
    Warn,
    Enforce,
}

impl std::fmt::Display for SpacePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpacePolicy::Warn => write!(f, "warn"),
            SpacePolicy::Enforce => write!(f, "enforce"),
        }
    }
}

/// Preferred helper prioritization.
#[derive(Debug, Deserialize, Clone)]
pub struct HelperConfig {
    #[serde(default = "HelperConfig::default_priority")]
    #[allow(dead_code)]
    pub priority: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

impl HelperConfig {
    fn default_priority() -> Vec<String> {
        vec![
            "paru".into(),
            "yay".into(),
            "pikaur".into(),
            "trizen".into(),
        ]
    }
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            priority: Self::default_priority(),
            default: None,
        }
    }
}

/// Application metadata collection toggles.
#[derive(Debug, Deserialize, Clone)]
pub struct ApplicationsConfig {
    #[serde(default)]
    pub flatpak: bool,
    #[serde(default)]
    pub fwupd: bool,
}

impl Default for ApplicationsConfig {
    fn default() -> Self {
        Self {
            flatpak: false,
            fwupd: false,
        }
    }
}

/// Logging preferences.
#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub retention_days: Option<u64>,
    #[serde(default)]
    pub retention_megabytes: Option<u64>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            directory: None,
            level: Some("info".to_string()),
            retention_days: None,
            retention_megabytes: None,
        }
    }
}

/// Snapshot hooks configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct SnapshotsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pre_command: Option<String>,
    #[serde(default)]
    pub post_command: Option<String>,
    #[serde(default)]
    pub require_success: bool,
}

impl Default for SnapshotsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pre_command: None,
            post_command: None,
            require_success: false,
        }
    }
}

/// Safety tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct SafetyConfig {
    #[serde(default = "SafetyConfig::default_disk_check")]
    pub disk_check: bool,
    #[serde(default)]
    pub disk_extra_margin_mb: u64,
}

impl SafetyConfig {
    fn default_disk_check() -> bool {
        true
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            disk_check: Self::default_disk_check(),
            disk_extra_margin_mb: 0,
        }
    }
}

/// Cache/cleanup preferences.
#[derive(Debug, Deserialize, Clone)]
pub struct CleanConfig {
    #[serde(default = "CleanConfig::default_keep_versions")]
    pub keep_versions: u64,
    #[serde(default)]
    pub remove_orphans: bool,
    #[serde(default = "CleanConfig::default_check_pacnew")]
    pub check_pacnew: bool,
}

impl CleanConfig {
    fn default_keep_versions() -> u64 {
        2
    }
    fn default_check_pacnew() -> bool {
        true
    }
}

impl Default for CleanConfig {
    fn default() -> Self {
        Self {
            keep_versions: Self::default_keep_versions(),
            remove_orphans: false,
            check_pacnew: Self::default_check_pacnew(),
        }
    }
}

/// Serializable configuration summary.
#[derive(Debug, Serialize, Clone)]
pub struct ConfigReport {
    pub manifest_path: PathBuf,
    pub log_directory: PathBuf,
    pub helper_priority: Vec<String>,
    pub helper_default: Option<String>,
    pub space_min_free_bytes: u64,
    pub space_policy: String,
    pub batch_size: usize,
    pub applications_flatpak: bool,
    pub applications_fwupd: bool,
    pub log_level: Option<String>,
    pub log_retention_days: Option<u64>,
    pub log_retention_megabytes: Option<u64>,
    pub snapshots_enabled: bool,
    pub snapshot_pre_command: Option<String>,
    pub snapshot_post_command: Option<String>,
    pub snapshot_require_success: bool,
    pub safety_disk_check: bool,
    pub safety_disk_margin_mb: u64,
    pub clean_keep_versions: u64,
    pub clean_remove_orphans: bool,
    pub clean_check_pacnew: bool,
}

fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("syn-syu").join("config.toml"))
}

fn default_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())))
        .join("syn-syu")
        .join("logs")
}

fn ensure_secure_permissions(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to inspect permissions for {}: {err}",
            path.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o002 != 0 {
            return Err(SynsyuError::Config(format!(
                "Configuration file {} must not be world-writable",
                path.display()
            )));
        }
    }
    let canonical = path.canonicalize().map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to canonicalize configuration path {}: {err}",
            path.display()
        ))
    })?;
    let parent = canonical.parent().ok_or_else(|| {
        SynsyuError::Filesystem(format!(
            "Configuration path {} has no parent directory",
            canonical.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to ensure configuration directory {} exists: {err}",
            parent.display()
        ))
    })?;
    Ok(())
}
