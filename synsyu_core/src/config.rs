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
use serde::Deserialize;

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
}

impl Default for SynsyuConfig {
    fn default() -> Self {
        Self {
            aur: AurConfig::default(),
            core: CoreConfig::default(),
            helpers: HelperConfig::default(),
            space: SpaceConfig::default(),
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
        "/tmp/syn-syu_manifest.json".to_string()
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
}

impl SpaceConfig {
    fn default_min_free_gb() -> f64 {
        2.0
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
        }
    }
}

/// Preferred helper prioritization.
#[derive(Debug, Deserialize, Clone)]
pub struct HelperConfig {
    #[serde(default = "HelperConfig::default_priority")]
    #[allow(dead_code)]
    pub priority: Vec<String>,
}

impl HelperConfig {
    fn default_priority() -> Vec<String> {
        vec![
            "paru".into(),
            "yay".into(),
            "trizen".into(),
            "pikaur".into(),
            "aura".into(),
            "pamac".into(),
        ]
    }
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            priority: Self::default_priority(),
        }
    }
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
