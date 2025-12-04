use std::collections::HashMap;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::Stdio;

use chrono::Utc;
use clap::{ArgAction, Args};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::{SpacePolicy, SynsyuConfig};
use crate::error::{Result, SynsyuError};
use crate::pacman;
use crate::space;

#[derive(Debug, Args, Clone)]
pub struct PlanCommand {
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Override manifest input path.
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,
    /// Override plan output path.
    #[arg(long, value_name = "PATH")]
    pub plan: Option<PathBuf>,
    /// Emit JSON to stdout.
    #[arg(long, action = ArgAction::SetTrue)]
    pub json: bool,
    /// Exit non-zero if any errors are recorded.
    #[arg(long, action = ArgAction::SetTrue)]
    pub strict: bool,
    /// Disable network calls (no AUR).
    #[arg(long, action = ArgAction::SetTrue)]
    pub offline: bool,
    /// Skip repository (pacman) checks.
    #[arg(long = "no-repo", action = ArgAction::SetTrue)]
    pub no_repo: bool,
    /// Skip AUR checks.
    #[arg(long = "no-aur", action = ArgAction::SetTrue)]
    pub no_aur: bool,
    /// Include flatpak updates.
    #[arg(long = "with-flatpak", action = ArgAction::SetTrue)]
    pub with_flatpak: bool,
    /// Include firmware updates (from manifest).
    #[arg(long = "with-fwupd", action = ArgAction::SetTrue)]
    pub with_fwupd: bool,
}

#[derive(Debug)]
pub struct PlanOutput {
    pub plan_json: serde_json::Value,
    pub blocked: bool,
}

#[derive(Debug, Deserialize)]
struct ManifestPackages {
    packages: HashMap<String, InstalledPkgRecord>,
    #[serde(default)]
    applications: ManifestApplications,
}

#[derive(Debug, Deserialize, Default)]
struct ManifestApplications {
    #[serde(default)]
    fwupd: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct InstalledPkgRecord {
    installed_version: String,
    source: Option<String>,
    #[serde(default)]
    package_hash: Option<String>,
    #[serde(default)]
    validated_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FwupdUpdates {
    #[serde(rename = "Devices", default)]
    devices: Vec<FwupdUpdateDevice>,
    #[serde(default)]
    devices_lower: Vec<FwupdUpdateDevice>,
}

#[derive(Debug, Deserialize)]
struct FwupdUpdateDevice {
    #[serde(rename = "DeviceId")]
    device_id: Option<String>,
    #[serde(rename = "Id")]
    id: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    installed: Option<String>,
    #[serde(rename = "Releases")]
    releases: Option<Vec<FwupdUpdateRelease>>,
    #[serde(rename = "releases")]
    releases_lower: Option<Vec<FwupdUpdateRelease>>,
}

#[derive(Debug, Deserialize)]
struct FwupdUpdateRelease {
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "Summary")]
    summary: Option<String>,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "Checksum")]
    checksum: Option<String>,
    #[serde(rename = "Checksums")]
    checksums: Option<Vec<String>>,
    #[serde(rename = "checksums")]
    checksums_lower: Option<Vec<String>>,
    #[serde(rename = "TrustFlags")]
    trust_flags: Option<Vec<String>>,
    #[serde(rename = "trust-flags")]
    trust_flags_lower: Option<Vec<String>>,
    #[serde(rename = "Signed")]
    signed: Option<bool>,
}

impl PlanCommand {
    pub async fn execute(
        &self,
        config: &SynsyuConfig,
        manifest_path: PathBuf,
        plan_path: PathBuf,
    ) -> Result<PlanOutput> {
        let manifest_path = fs::canonicalize(&manifest_path).unwrap_or(manifest_path);
        let plan_path = if let Ok(p) = fs::canonicalize(&plan_path) {
            p
        } else {
            plan_path
        };

        let manifest_file = File::open(&manifest_path).map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to open manifest {}: {err}",
                manifest_path.display()
            ))
        })?;
        let manifest: ManifestPackages = serde_json::from_reader(manifest_file).map_err(|err| {
            SynsyuError::Serialization(format!(
                "Failed to parse manifest {}: {err}",
                manifest_path.display()
            ))
        })?;

        let mut errors: Vec<String> = Vec::new();
        let mut sources: Vec<String> = Vec::new();

        let mut pacman_updates = Vec::new();
        let mut aur_updates = Vec::new();
        let mut flatpak_updates = Vec::new();
        let mut fwupd_updates = Vec::new();
        let mut space_blocked = false;

        if !self.no_repo {
            sources.push("pacman".to_string());
            let (updates, errs) = collect_pacman_updates(&manifest).await;
            pacman_updates = updates;
            errors.extend(errs);
        }

        if !self.no_aur && !self.offline {
            sources.push("aur".to_string());
            let (updates, errs) = collect_aur_updates(&manifest).await;
            aur_updates = updates;
            errors.extend(errs);
        }

        if self.with_flatpak {
            sources.push("flatpak".to_string());
            let (updates, errs) = collect_flatpak_updates().await;
            flatpak_updates = updates;
            errors.extend(errs);
        }

        if self.with_fwupd {
            sources.push("fwupd".to_string());
            let (updates, errs) = collect_fwupd_updates().await;
            fwupd_updates = updates;
            errors.extend(errs);
        }

        // Disk buffer check
        let min_free = config.min_free_bytes();
        let policy = config.space_policy();
        let mut space_meta = serde_json::Map::new();
        space_meta.insert("policy".to_string(), policy.to_string().into());
        space_meta.insert("min_free_bytes".to_string(), min_free.into());
        match space::assess_default_paths() {
            Ok(report) => {
                space_meta.insert(
                    "available_bytes".to_string(),
                    report.available_bytes.into(),
                );
                space_meta.insert(
                    "checked_path".to_string(),
                    report.checked_path.display().to_string().into(),
                );
                if min_free > 0 && report.available_bytes < min_free {
                    let msg = format!(
                        "disk: available {} below configured buffer {} on {}",
                        report.available_bytes,
                        min_free,
                        report.checked_path.display()
                    );
                    space_meta.insert("status".to_string(), "low".into());
                    match policy {
                        SpacePolicy::Warn => {
                            space_meta.insert("warning".to_string(), msg.clone().into());
                        }
                        SpacePolicy::Enforce => {
                            errors.push(msg.clone());
                            space_blocked = true;
                        }
                    }
                } else {
                    space_meta.insert("status".to_string(), "ok".into());
                }
            }
            Err(err) => {
                space_meta.insert("status".to_string(), "unknown".into());
                errors.push(format!("disk: unable to assess free space ({err})"));
            }
        }

        let generated_at = Utc::now().to_rfc3339();

        let plan_json = json!({
            "metadata": {
                "generated_at": generated_at,
                "generated_by": "synsyu_core plan",
                "manifest_path": manifest_path.display().to_string(),
                "plan_path": plan_path.display().to_string(),
                "sources": sources,
                "space": space_meta,
                "errors": errors,
            },
            "pacman_updates": pacman_updates,
            "aur_updates": aur_updates,
            "flatpak_updates": flatpak_updates,
            "fwupd_updates": fwupd_updates,
            "counts": {
                "pacman": pacman_updates.len(),
                "aur": aur_updates.len(),
                "flatpak": flatpak_updates.len(),
                "fwupd": fwupd_updates.len(),
            }
        });

        if let Some(parent) = plan_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                SynsyuError::Filesystem(format!(
                    "Failed to create plan directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
        let mut file = tokio::fs::File::create(&plan_path).await.map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to create plan file {}: {err}",
                plan_path.display()
            ))
        })?;
        let json_pretty =
            serde_json::to_string_pretty(&plan_json).unwrap_or_else(|_| "{}".to_string());
        file.write_all(json_pretty.as_bytes()).await.map_err(|err| {
            SynsyuError::Filesystem(format!(
                "Failed to write plan {}: {err}",
                plan_path.display()
            ))
        })?;

        Ok(PlanOutput { plan_json, blocked: space_blocked })
    }
}

async fn collect_pacman_updates(
    manifest: &ManifestPackages,
) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();

    for (name, pkg) in manifest.packages.iter() {
        if !matches_source(pkg.source.as_deref(), "PACMAN") {
            continue;
        }
        let installed = pkg.installed_version.trim();
        if installed.is_empty() {
            continue;
        }

        let output = Command::new("pacman")
            .arg("-Si")
            .arg(name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        let Ok(output) = output else {
            errors.push(format!("pacman: failed to spawn for {}", name));
            continue;
        };
        if !output.status.success() {
            errors.push(format!(
                "pacman: failed to query {} ({})",
                name,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut available = String::new();
        let mut available_hash = String::new();
        let mut available_trust = String::new();
        for line in stdout.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "Version" => available = val.to_string(),
                    "SHA-256 Sum" => available_hash = truncate_hash(val),
                    "Validated By" => available_trust = val.to_string(),
                    _ => {}
                }
            }
        }
        if available.is_empty() {
            errors.push(format!("pacman: missing remote version for {}", name));
            continue;
        }

        match pacman::compare_versions(installed, &available).await {
            Ok(ordering) if ordering.is_lt() => {
                updates.push(json!({
                    "name": name,
                    "installed": installed,
                    "available": available,
                    "installed_hash": truncate_hash(pkg.package_hash.clone().unwrap_or_default().as_str()),
                    "available_hash": available_hash,
                    "installed_trust": pkg.validated_by.clone().unwrap_or_default(),
                    "available_trust": available_trust,
                    "source": "pacman"
                }));
            }
            Ok(_) => {}
            Err(err) => errors.push(format!("vercmp failed for {name}: {err}")),
        }
    }

    (updates, errors)
}

#[derive(Debug, Deserialize)]
struct AurRpcResponse {
    #[serde(rename = "type")]
    resp_type: Option<String>,
    #[serde(rename = "resultcount")]
    result_count: Option<u64>,
    results: Option<Vec<AurPkg>>,
}

#[derive(Debug, Deserialize)]
struct AurPkg {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
}

async fn collect_aur_updates(
    manifest: &ManifestPackages,
) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();

    let mut aur_pkgs: Vec<(String, String)> = manifest
        .packages
        .iter()
        .filter_map(|(name, pkg)| {
            if matches_source(pkg.source.as_deref(), "AUR") {
                Some((name.clone(), pkg.installed_version.clone()))
            } else {
                None
            }
        })
        .collect();

    if aur_pkgs.is_empty() {
        return (updates, errors);
    }

    const CHUNK: usize = 100;
    let client = Client::new();

    while !aur_pkgs.is_empty() {
        let chunk: Vec<(String, String)> = aur_pkgs.drain(..aur_pkgs.len().min(CHUNK)).collect();
        let mut url = String::from("https://aur.archlinux.org/rpc/?v=5&type=info");
        for (name, _) in &chunk {
            url.push_str("&arg[]=");
            url.push_str(urlencoding::encode(name).as_ref());
        }

        let resp = client.get(&url).send().await;
        let Ok(resp) = resp else {
            errors.push("AUR: request failed".to_string());
            continue;
        };
        if !resp.status().is_success() {
            errors.push(format!("AUR: HTTP {}", resp.status()));
            continue;
        }
        let parsed: AurRpcResponse = match resp.json().await {
            Ok(p) => p,
            Err(err) => {
                errors.push(format!("AUR: parse failed {err}"));
                continue;
            }
        };
        let results = parsed.results.unwrap_or_default();
        let map: HashMap<String, String> = results
            .into_iter()
            .filter_map(|pkg| pkg.name.zip(pkg.version))
            .collect();

        for (name, installed) in chunk {
            if let Some(available) = map.get(&name) {
                match pacman::compare_versions(&installed, available).await {
                    Ok(ordering) if ordering.is_lt() => updates.push(json!({
                        "name": name,
                        "installed": installed,
                        "available": available,
                        "source": "aur"
                    })),
                    Ok(_) => {}
                    Err(err) => errors.push(format!("AUR vercmp failed for {name}: {err}")),
                }
            }
        }
    }

    (updates, errors)
}

async fn collect_flatpak_updates() -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();
    let output = Command::new("flatpak")
        .args(["remote-ls", "--updates", "--columns=application,branch,origin,version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        errors.push("flatpak: failed to spawn".to_string());
        return (updates, errors);
    };
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        errors.push(format!("flatpak: {}", err));
        return (updates, errors);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let app = parts[0].to_string();
            let branch = parts[1].to_string();
            let origin = parts[2].to_string();
            let available = if parts.len() >= 4 { parts[3].to_string() } else { String::new() };
            updates.push(json!({
                "name": app,
                "branch": branch,
                "origin": origin,
                "available": available,
                "source": "flatpak"
            }));
        }
    }
    (updates, errors)
}

async fn collect_fwupd_updates() -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();
    let output = Command::new("fwupdmgr")
        .args(["get-updates", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        errors.push("fwupd: failed to spawn".to_string());
        return (updates, errors);
    };
    if !output.status.success() {
        errors.push(format!(
            "fwupd: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
        return (updates, errors);
    }

    let parsed: FwupdUpdates = match serde_json::from_slice(&output.stdout) {
        Ok(p) => p,
        Err(err) => {
            errors.push(format!("fwupd: parse failed {err}"));
            return (updates, errors);
        }
    };

    let devices = if !parsed.devices.is_empty() {
        parsed.devices
    } else {
        parsed.devices_lower
    };

    for dev in devices {
        let dev_id = dev
            .device_id
            .or(dev.id)
            .unwrap_or_else(|| "unknown".to_string());
        let name = dev.name.clone().unwrap_or_else(|| dev_id.clone());
        let installed = dev.installed.unwrap_or_default();
        let releases = dev.releases.or(dev.releases_lower).unwrap_or_default();
        for rel in releases {
            let available = rel.version.unwrap_or_default();
            if available.is_empty() {
                continue;
            }
            if !installed.is_empty() && installed == available {
                continue;
            }
            let summary = rel
                .summary
                .or(rel.description)
                .unwrap_or_default();
            let checksum = truncate_hash(
                select_checksum(rel.checksum, rel.checksums, rel.checksums_lower).as_str(),
            );
            let trust = join_trust(rel.trust_flags, rel.trust_flags_lower)
                .or_else(|| rel.signed.map(|s| if s { "signed".to_string() } else { "unsigned".to_string() }))
                .unwrap_or_default();

            updates.push(json!({
                "device": dev_id,
                "name": name,
                "installed": installed,
                "available": available,
                "summary": summary,
                "available_hash": checksum,
                "trust": trust,
                "source": "fwupd"
            }));
        }
    }

    (updates, errors)
}

fn matches_source(source: Option<&str>, target: &str) -> bool {
    match source {
        Some(s) => s.eq_ignore_ascii_case(target),
        None => false,
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

fn select_checksum(
    checksum: Option<String>,
    checksums: Option<Vec<String>>,
    checksums_lower: Option<Vec<String>>,
) -> String {
    if let Some(cs) = checksum {
        return cs;
    }
    if let Some(list) = checksums {
        if let Some(first) = list.into_iter().find(|s| !s.is_empty()) {
            return first;
        }
    }
    if let Some(list) = checksums_lower {
        if let Some(first) = list.into_iter().find(|s| !s.is_empty()) {
            return first;
        }
    }
    String::new()
}

fn join_trust(primary: Option<Vec<String>>, alt: Option<Vec<String>>) -> Option<String> {
    let list = primary.or(alt)?;
    let filtered: Vec<String> = list.into_iter().filter(|s| !s.is_empty()).collect();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered.join(","))
    }
}
