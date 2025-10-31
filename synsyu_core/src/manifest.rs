/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::manifest
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Construct the Syn-Syu manifest by reconciling installed
    packages against repo and AUR sources.

  Security / Safety Notes:
    Manifest data is written to operator-controlled paths; no
    privileged operations are performed.

  Dependencies:
    serde for JSON serialization.

  Operational Scope:
    Consumed by the Bash orchestrator to decide update flows.

  Revision History:
    2024-11-04 COD  Authored manifest builder.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Deterministic ordering for reproducible manifests
    - Explicit source attribution for each package
    - Rich metadata for audit and observability
============================================================*/

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::error::{Result, SynsyuError};
use crate::logger::Logger;
use crate::package_info::VersionInfo;
use crate::pacman::{compare_versions, InstalledPackage};

/// Wrapper representing the full manifest document.
#[derive(Debug, Serialize)]
pub struct ManifestDocument {
    pub metadata: ManifestMetadata,
    pub packages: BTreeMap<String, ManifestEntry>,
}

/// Metadata block describing manifest context.
#[derive(Debug, Serialize)]
pub struct ManifestMetadata {
    pub generated_at: String,
    pub generated_by: String,
    pub total_packages: usize,
    pub repo_candidates: usize,
    pub aur_candidates: usize,
    pub updates_available: usize,
    pub download_size_total: u64,
    pub build_size_total: u64,
    pub install_size_total: u64,
    pub transient_size_total: u64,
    pub min_free_bytes: u64,
    pub required_space_total: u64,
    pub available_space_bytes: u64,
    pub space_checked_path: String,
}

/// Per-package manifest entry.
#[derive(Debug, Serialize)]
pub struct ManifestEntry {
    pub installed_version: String,
    pub version_repo: Option<String>,
    pub version_aur: Option<String>,
    pub newer_version: String,
    pub source: PackageSource,
    pub update_available: bool,
    pub notes: Option<String>,
    pub download_size_repo: Option<u64>,
    pub installed_size_repo: Option<u64>,
    pub download_size_aur: Option<u64>,
    pub installed_size_aur: Option<u64>,
    pub download_size_selected: Option<u64>,
    pub installed_size_selected: Option<u64>,
    pub install_size_estimate: Option<u64>,
    pub build_size_estimate: Option<u64>,
    pub transient_size_estimate: Option<u64>,
}

/// Source classification for an update candidate.
#[derive(Debug, Serialize, Clone, Copy)]
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
    repo_versions: &HashMap<String, VersionInfo>,
    aur_versions: &HashMap<String, VersionInfo>,
    min_free_bytes: u64,
    logger: &Logger,
) -> Result<ManifestDocument> {
    let mut entries = BTreeMap::new();
    let mut repo_candidates = 0usize;
    let mut aur_candidates = 0usize;
    let mut updates_available = 0usize;
    let mut download_total = 0u64;
    let mut build_total = 0u64;
    let mut install_total = 0u64;
    let mut transient_total = 0u64;

    for package in packages {
        let repo_info = repo_versions.get(&package.name);
        let aur_info = aur_versions.get(&package.name);

        if repo_info.is_some() {
            repo_candidates += 1;
        }
        if aur_info.is_some() {
            aur_candidates += 1;
        }

        let resolved = resolve_package(package, repo_info, aur_info).await?;
        if resolved.update_available {
            updates_available += 1;
            if let Some(size) = resolved.download_size_selected {
                download_total = download_total.saturating_add(size);
            }
            if let Some(size) = resolved.build_size_estimate {
                build_total = build_total.saturating_add(size);
            }
            if let Some(size) = resolved.install_size_estimate {
                install_total = install_total.saturating_add(size);
            }
            if let Some(size) = resolved.transient_size_estimate {
                transient_total = transient_total.saturating_add(size);
            }
        }
        logger.debug(
            "MANIFEST",
            format!(
                "{} → {} via {:?}",
                package.name, resolved.newer_version, resolved.source
            ),
        );

        entries.insert(package.name.clone(), resolved);
    }

    let metadata = ManifestMetadata {
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        generated_by: "synsyu_core".to_string(),
        total_packages: packages.len(),
        repo_candidates,
        aur_candidates,
        updates_available,
        download_size_total: download_total,
        build_size_total: build_total,
        install_size_total: install_total,
        transient_size_total: transient_total,
        min_free_bytes,
        required_space_total: transient_total.saturating_add(min_free_bytes),
        available_space_bytes: 0,
        space_checked_path: String::new(),
    };

    Ok(ManifestDocument {
        metadata,
        packages: entries,
    })
}

async fn resolve_package(
    package: &InstalledPackage,
    repo_info: Option<&VersionInfo>,
    aur_info: Option<&VersionInfo>,
) -> Result<ManifestEntry> {
    let mut source = PackageSource::Unknown;
    let mut target_version = package.version.clone();
    let mut update_available = false;
    let mut notes: Option<String> = None;

    let repo_cmp = if let Some(info) = repo_info {
        Some(compare_versions(&package.version, &info.version).await?)
    } else {
        None
    };

    let aur_cmp = if let Some(info) = aur_info {
        Some(compare_versions(&package.version, &info.version).await?)
    } else {
        None
    };

    match (repo_info, repo_cmp, aur_info, aur_cmp) {
        (Some(repo_v), Some(repo_cmp), None, _) => {
            source = PackageSource::Pacman;
            target_version = repo_v.version.clone();
            update_available = repo_cmp == std::cmp::Ordering::Less;
        }
        (None, _, Some(aur_v), Some(aur_cmp)) => {
            source = PackageSource::Aur;
            target_version = aur_v.version.clone();
            update_available = aur_cmp == std::cmp::Ordering::Less;
        }
        (Some(repo_v), Some(repo_cmp), Some(aur_v), Some(aur_cmp)) => {
            let repo_vs_aur = compare_versions(&repo_v.version, &aur_v.version).await?;
            match repo_vs_aur {
                std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => {
                    source = PackageSource::Pacman;
                    target_version = repo_v.version.clone();
                    update_available = repo_cmp == std::cmp::Ordering::Less;
                    if aur_cmp == std::cmp::Ordering::Greater {
                        notes = Some("AUR ahead of repo, but repo chosen per policy".into());
                    }
                }
                std::cmp::Ordering::Less => {
                    source = PackageSource::Aur;
                    target_version = aur_v.version.clone();
                    update_available = aur_cmp == std::cmp::Ordering::Less;
                }
            }
        }
        (None, _, None, _) => {
            source = if package.repository.as_deref() == Some("local") {
                PackageSource::Local
            } else {
                PackageSource::Unknown
            };
        }
        _ => {
            if let Some(repo_v) = repo_info {
                source = PackageSource::Pacman;
                target_version = repo_v.version.clone();
            } else if let Some(aur_v) = aur_info {
                source = PackageSource::Aur;
                target_version = aur_v.version.clone();
            }
        }
    }

    let download_repo = repo_info.and_then(|info| info.download_size);
    let installed_repo = repo_info.and_then(|info| info.installed_size);
    let download_aur = aur_info.and_then(|info| info.download_size);
    let installed_aur = aur_info.and_then(|info| info.installed_size);
    let (download_selected, installed_selected) = match source {
        PackageSource::Pacman => (download_repo, installed_repo),
        PackageSource::Aur => (download_aur, installed_aur),
        PackageSource::Local => (None, None),
        PackageSource::Unknown => (
            download_repo.or(download_aur),
            installed_repo.or(installed_aur),
        ),
    };

    let install_estimate = match (installed_selected, download_selected, source) {
        (Some(value), _, _) => Some(value),
        (None, Some(download), PackageSource::Aur) => Some(download.saturating_mul(2)),
        (None, Some(download), _) => Some(download),
        _ => None,
    };

    let build_estimate = match (source, install_estimate, download_selected) {
        (PackageSource::Pacman, Some(install), _) => {
            let triple = install.saturating_mul(3);
            Some(triple / 2 + triple % 2)
        }
        (PackageSource::Aur, _, Some(download)) => Some(download.saturating_mul(8)),
        _ => None,
    };

    let transient_estimate = {
        let download = download_selected.unwrap_or(0);
        let build = build_estimate.unwrap_or(0);
        let install = install_estimate.unwrap_or(0);
        let total = download.saturating_add(build).saturating_add(install);
        if total == 0 {
            None
        } else {
            Some(total)
        }
    };

    Ok(ManifestEntry {
        installed_version: package.version.clone(),
        version_repo: repo_info.map(|info| info.version.clone()),
        version_aur: aur_info.map(|info| info.version.clone()),
        newer_version: target_version,
        source,
        update_available,
        notes,
        download_size_repo: download_repo,
        installed_size_repo: installed_repo,
        download_size_aur: download_aur,
        installed_size_aur: installed_aur,
        download_size_selected: download_selected,
        installed_size_selected: installed_selected,
        install_size_estimate: install_estimate,
        build_size_estimate: build_estimate,
        transient_size_estimate: transient_estimate,
    })
}

/// Persist the manifest to the given path.
pub fn write_manifest(document: &ManifestDocument, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to create manifest directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let file = File::create(path).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to create manifest file {}: {err}",
            path.display()
        ))
    })?;
    serde_json::to_writer_pretty(file, document).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to write manifest {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}
