use serde::Deserialize;

use crate::error::{Result, SynsyuError};
use crate::logger::Logger;

#[derive(Debug, Deserialize)]
struct FwupdJson {
    #[serde(default)]
    Devices: Vec<FwupdDeviceRaw>,
    #[serde(default)]
    devices: Vec<FwupdDeviceRaw>,
}

#[derive(Debug, Deserialize)]
struct FwupdDeviceRaw {
    #[serde(rename = "Id")]
    id: Option<String>,
    #[serde(rename = "DeviceId")]
    device_id: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "VersionBootloader")]
    version_bootloader: Option<String>,
    #[serde(rename = "Summary")]
    summary: Option<String>,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "Releases")]
    releases: Option<Vec<FwupdReleaseRaw>>,
    #[serde(rename = "releases")]
    releases_lower: Option<Vec<FwupdReleaseRaw>>,
    #[serde(rename = "Checksums")]
    checksums: Option<Vec<String>>,
    #[serde(rename = "Checksum")]
    checksum: Option<String>,
    #[serde(rename = "checksums")]
    checksums_lower: Option<Vec<String>>,
    #[serde(rename = "TrustFlags")]
    trust_flags: Option<Vec<String>>,
    #[serde(rename = "trust-flags")]
    trust_flags_lower: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct FwupdReleaseRaw {
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

#[derive(Debug, serde::Serialize, Clone)]
pub struct FwupdRelease {
    pub version: String,
    pub summary: String,
    pub checksum: String,
    pub trust: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct FwupdDevice {
    pub device: String,
    pub name: String,
    pub installed: String,
    pub summary: String,
    pub checksum: String,
    pub trust: String,
    pub releases: Vec<FwupdRelease>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct FwupdState {
    pub enabled: bool,
    pub device_count: usize,
    pub devices: Vec<FwupdDevice>,
    pub update_count: usize,
    pub updates: Vec<FwupdUpdate>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct FwupdUpdate {
    pub device: String,
    pub name: String,
    pub installed: String,
    pub available: String,
    pub summary: String,
    pub available_hash: String,
    pub trust: String,
}

pub async fn collect_fwupd(logger: &Logger, include_updates: bool) -> Result<Option<FwupdState>> {
    let output = tokio::process::Command::new("fwupdmgr")
        .arg("get-devices")
        .arg("--json")
        .output()
        .await;

    let Ok(output) = output else {
        logger.warn("FWUPD", "fwupdmgr not found; skipping firmware capture.");
        return Ok(None);
    };

    if !output.status.success() {
        logger.warn(
            "FWUPD",
            format!(
                "fwupdmgr get-devices failed (status {:?}); skipping firmware capture.",
                output.status.code()
            ),
        );
        return Ok(None);
    }

    let parsed: FwupdJson = serde_json::from_slice(&output.stdout).map_err(|err| {
        SynsyuError::Serialization(format!("Failed to parse fwupd JSON output: {err}"))
    })?;
    let devices_raw = if !parsed.Devices.is_empty() {
        parsed.Devices
    } else {
        parsed.devices
    };

    let mut devices = Vec::new();
    for raw in devices_raw {
        let device_id = raw
            .id
            .or(raw.device_id)
            .unwrap_or_else(|| "unknown".to_string());
        let name = raw.name.clone().unwrap_or_else(|| device_id.clone());
        let installed = raw
            .version
            .or(raw.version_bootloader)
            .unwrap_or_else(String::new);
        let summary = raw.summary.or(raw.description).unwrap_or_else(String::new);

        let checksum = truncate_hash(select_checksum(
            raw.checksum,
            raw.checksums,
            raw.checksums_lower,
        ));
        let trust = join_trust(raw.trust_flags, raw.trust_flags_lower).unwrap_or_else(String::new);

        // Manifest should reflect current firmware state; exclude pending release data.
        let releases = Vec::new();

        devices.push(FwupdDevice {
            device: device_id,
            name,
            installed,
            summary,
            checksum,
            trust,
            releases,
        });
    }

    let mut updates = Vec::new();
    if include_updates {
        match collect_fwupd_updates().await {
            Ok(list) => updates = list,
            Err(err) => logger.warn("FWUPD", format!("fwupdmgr get-updates failed: {err}")),
        }
    }

    let state = FwupdState {
        enabled: true,
        device_count: devices.len(),
        devices,
        update_count: updates.len(),
        updates,
    };
    logger.info(
        "FWUPD",
        format!(
            "Recorded fwupd state: devices={} (releases across devices={})",
            state.device_count,
            state
                .devices
                .iter()
                .map(|d| d.releases.len())
                .sum::<usize>()
        ),
    );
    Ok(Some(state))
}

pub async fn collect_fwupd_updates_for_plan() -> (Vec<FwupdUpdate>, Vec<String>) {
    match collect_fwupd_updates().await {
        Ok(list) => (list, Vec::new()),
        Err(err) => (Vec::new(), vec![err]),
    }
}

async fn collect_fwupd_updates() -> std::result::Result<Vec<FwupdUpdate>, String> {
    let output = tokio::process::Command::new("fwupdmgr")
        .args(["get-updates", "--json"])
        .output()
        .await
        .map_err(|_| "failed to spawn fwupdmgr".to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let parsed: FwupdUpdates =
        serde_json::from_slice(&output.stdout).map_err(|err| format!("parse failed {err}"))?;

    let devices = if !parsed.devices.is_empty() {
        parsed.devices
    } else {
        parsed.devices_lower
    };

    let mut updates = Vec::new();
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
            if available.is_empty() || (!installed.is_empty() && installed == available) {
                continue;
            }
            let summary = rel.summary.or(rel.description).unwrap_or_default();
            let checksum = truncate_hash(select_checksum(
                rel.checksum,
                rel.checksums,
                rel.checksums_lower,
            ));
            let trust = join_trust(rel.trust_flags, rel.trust_flags_lower)
                .or_else(|| {
                    rel.signed.map(|s| {
                        if s {
                            "signed".to_string()
                        } else {
                            "unsigned".to_string()
                        }
                    })
                })
                .unwrap_or_default();

            updates.push(FwupdUpdate {
                device: dev_id.clone(),
                name: name.clone(),
                installed: installed.clone(),
                available,
                summary,
                available_hash: checksum,
                trust,
            });
        }
    }

    Ok(updates)
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

fn truncate_hash(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 16 {
        trimmed.to_string()
    } else {
        trimmed[..16].to_string()
    }
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
