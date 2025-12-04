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
}

pub async fn collect_fwupd(logger: &Logger) -> Result<Option<FwupdState>> {
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
        let summary = raw
            .summary
            .or(raw.description)
            .unwrap_or_else(String::new);

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

    let state = FwupdState {
        enabled: true,
        device_count: devices.len(),
        devices,
    };
    logger.info(
        "FWUPD",
        format!(
            "Recorded fwupd state: devices={} (releases across devices={})",
            state.device_count,
            state.devices.iter().map(|d| d.releases.len()).sum::<usize>()
        ),
    );
    Ok(Some(state))
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
