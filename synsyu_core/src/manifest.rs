/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::manifest
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Construct the Syn-Syu manifest as a snapshot of the
    user-defined desired system state: what is installed right
    now, with source attribution.

  Security / Safety Notes:
    Manifest data is written to operator-controlled paths with
    private permissions; no privileged operations are performed.

  Dependencies:
    serde for JSON serialization.

  Operational Scope:
    Consumed by the Bash orchestrator as the authoritative
    snapshot of installed state.

  Revision History:
    2024-11-04 COD  Authored manifest builder.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Deterministic ordering for reproducible manifests
    - Explicit source attribution for each package
    - Rich metadata for audit and observability
============================================================*/

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::error::{Result, SynsyuError};
use crate::flatpak::FlatpakState;
use crate::logger::Logger;
use crate::pacman::InstalledPackage;

/// Wrapper representing the full manifest document.
#[derive(Debug, Serialize)]
pub struct ManifestDocument {
    pub metadata: ManifestMetadata,
    pub packages: BTreeMap<String, ManifestEntry>,
    pub packages_by_source: Vec<PackageGroup>,
    pub applications: Applications,
}

/// Metadata block describing manifest context.
#[derive(Debug, Serialize)]
pub struct ManifestMetadata {
    pub generated_at: String,
    pub generated_by: String,
    pub total_packages: usize,
    pub pacman_packages: usize,
    pub aur_packages: usize,
    pub local_packages: usize,
    pub unknown_packages: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps_flatpak: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps_fwupd: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_state: Option<ApplicationStateSummary>,
}

/// Per-package manifest entry.
#[derive(Debug, Serialize)]
pub struct ManifestEntry {
    pub installed_version: String,
    pub repository: Option<String>,
    pub source: PackageSource,
    pub installed_size: Option<u64>,
    pub install_date: Option<String>,
    pub validated_by: Option<String>,
    pub package_hash: Option<String>,
}

/// Group of package names for a particular source.
#[derive(Debug, Serialize)]
pub struct PackageGroup {
    pub source: PackageSource,
    pub count: usize,
    pub packages: Vec<String>,
}

/// Optional application/firmware state.
#[derive(Debug, Serialize, Default, Clone)]
pub struct Applications {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatpak: Option<FlatpakState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fwupd: Option<crate::fwupd::FwupdState>,
}

/// Lightweight summary of application state for manifest metadata.
#[derive(Debug, Serialize, Default, Clone)]
pub struct ApplicationStateSummary {
    pub flatpak: usize,
    pub fwupd: usize,
}

/// Source classification for an update candidate.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum PackageSource {
    Pacman,
    Aur,
    Local,
    Unknown,
}

/// Build a manifest from installed package data.
pub async fn build_manifest(
    packages: &[InstalledPackage],
    logger: &Logger,
) -> Result<ManifestDocument> {
    let mut entries = BTreeMap::new();
    let mut grouped: BTreeMap<PackageSource, Vec<String>> = BTreeMap::new();
    let mut pacman_packages = 0usize;
    let mut aur_packages = 0usize;
    let mut local_packages = 0usize;
    let mut unknown_packages = 0usize;

    for package in packages {
        let resolved = resolve_package(package);
        match resolved.source {
            PackageSource::Pacman => pacman_packages += 1,
            PackageSource::Aur => aur_packages += 1,
            PackageSource::Local => local_packages += 1,
            PackageSource::Unknown => unknown_packages += 1,
        }
        logger.debug(
            "MANIFEST",
            format!(
                "{} @ {} via {:?}",
                package.name, resolved.installed_version, resolved.source
            ),
        );

        entries.insert(package.name.clone(), resolved);
        grouped
            .entry(source_from_repo(package.repository.as_deref()))
            .or_default()
            .push(package.name.clone());
    }

    let mut packages_by_source: Vec<PackageGroup> = grouped
        .into_iter()
        .map(|(src, mut names)| {
            names.sort();
            PackageGroup {
                source: src,
                count: names.len(),
                packages: names,
            }
        })
        .collect();
    packages_by_source.sort_by(|a, b| a.count.cmp(&b.count).then_with(|| a.source.cmp(&b.source)));

    let metadata = ManifestMetadata {
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        generated_by: "synsyu_core".to_string(),
        total_packages: packages.len(),
        pacman_packages,
        aur_packages,
        local_packages,
        unknown_packages,
        apps_flatpak: None,
        apps_fwupd: None,
        application_state: None,
    };

    Ok(ManifestDocument {
        metadata,
        packages: entries,
        packages_by_source,
        applications: Applications::default(),
    })
}

impl ManifestDocument {
    /// Refresh metadata summaries based on collected application state.
    pub fn refresh_application_metadata(&mut self) {
        let flatpak_enabled = self
            .applications
            .flatpak
            .as_ref()
            .map(|f| f.enabled)
            .unwrap_or(false);
        let fwupd_enabled = self
            .applications
            .fwupd
            .as_ref()
            .map(|f| f.enabled)
            .unwrap_or(false);
        let flatpak_count = self
            .applications
            .flatpak
            .as_ref()
            .map(|f| f.installed_count)
            .unwrap_or(0);
        let fwupd_count = self
            .applications
            .fwupd
            .as_ref()
            .map(|f| f.device_count)
            .unwrap_or(0);

        self.metadata.apps_flatpak = Some(flatpak_enabled);
        self.metadata.apps_fwupd = Some(fwupd_enabled);
        self.metadata.application_state = Some(ApplicationStateSummary {
            flatpak: flatpak_count,
            fwupd: fwupd_count,
        });
    }
}

fn source_from_repo(repo: Option<&str>) -> PackageSource {
    match repo {
        Some(name) if name.eq_ignore_ascii_case("aur") => PackageSource::Aur,
        Some(name) if name.eq_ignore_ascii_case("local") => PackageSource::Local,
        Some(_) => PackageSource::Pacman,
        None => PackageSource::Unknown,
    }
}

fn resolve_package(package: &InstalledPackage) -> ManifestEntry {
    let repo = package.repository.clone();
    let source = source_from_repo(repo.as_deref());

    ManifestEntry {
        installed_version: package.version.clone(),
        repository: repo,
        source,
        installed_size: package.installed_size,
        install_date: package.install_date.clone(),
        validated_by: package.validated_by.clone(),
        package_hash: package
            .package_hash
            .as_ref()
            .map(|h| truncate_hash(h.as_str())),
    }
}

fn truncate_hash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 16 {
        trimmed.to_string()
    } else {
        trimmed[..16].to_string()
    }
}

/// Persist the manifest to the given path.
pub fn write_manifest(document: &ManifestDocument, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to create manifest directory {}: {err}",
                parent.display()
            ))
        })?;
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o700);
            fs::set_permissions(parent, perms).map_err(|err| {
                SynsyuError::Filesystem(format!(
                    "Failed to secure manifest directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
    }
    let mut file = File::create(path).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to create manifest file {}: {err}",
            path.display()
        ))
    })?;
    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to secure manifest file {}: {err}",
                path.display()
            ))
        })?;
    }
    serde_json::to_writer_pretty(&mut file, document).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to write manifest {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}
