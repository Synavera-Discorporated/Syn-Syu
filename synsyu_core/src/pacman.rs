/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::pacman
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Interface with pacman utilities to enumerate installed
    packages, query repository metadata, and compare versions.

  Security / Safety Notes:
    Executes pacman/vercmp binaries with user privileges only;
    no privilege escalation is attempted.

  Dependencies:
    tokio::process for async command execution.

  Operational Scope:
    Supplies Syn-Syu-Core with local inventory data and version
    comparisons against repo sources.

  Revision History:
    2024-11-04 COD  Crafted pacman integration layer.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Deterministic command invocation with explicit checks
    - Structured parsing with clear failure modes
    - Reusable helpers for external command diagnostics
============================================================*/

use std::collections::{HashMap, HashSet};
use std::io;
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::sleep;

use crate::config::{AcquisitionAurRpcConfig, AurConfig};
use crate::error::{Result, SynsyuError};
use crate::logger::Logger;
use crate::package_info::VersionInfo;
use urlencoding::encode;

/// Represents a package currently installed on the system.
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub repository: Option<String>,
    pub installed_size: Option<u64>,
    pub install_date: Option<String>,
    pub validated_by: Option<String>,
    pub package_hash: Option<String>,
}

/// Enumerate all installed packages via `pacman -Qi`.
pub async fn enumerate_installed_packages() -> Result<Vec<InstalledPackage>> {
    let foreign = detect_foreign_packages().await.unwrap_or_default();
    let output = Command::new("pacman")
        .arg("-Qi")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| map_spawn_error(err, "pacman"))?;

    if !output.status.success() {
        return Err(SynsyuError::CommandFailure {
            command: "pacman -Qi".into(),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| {
        SynsyuError::Serialization(format!("pacman -Qi emitted invalid UTF-8: {err}"))
    })?;

    let mut packages = Vec::new();
    for block in stdout.split("\n\n") {
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        let mut repository: Option<String> = None;
        let mut installed_size: Option<u64> = None;
        let mut install_date: Option<String> = None;
        let mut validated_by: Option<String> = None;
        let mut package_hash: Option<String> = None;

        for line in block.lines() {
            if let Some((raw_key, raw_value)) = line.split_once(':') {
                let key = raw_key.trim();
                let value = raw_value.trim();
                match key {
                    "Name" => name = Some(value.to_string()),
                    "Version" => version = Some(value.to_string()),
                    "Repository" => repository = Some(value.to_string()),
                    "Install Date" => install_date = Some(value.to_string()),
                    "Installed Size" => installed_size = parse_pacman_size(value),
                    "Validated By" => validated_by = Some(value.to_string()),
                    "SHA-256 Sum" => package_hash = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        if let (Some(mut name), Some(version)) = (name, version) {
            if repository.is_none() {
                if foreign.contains(&name) {
                    repository = Some("local".to_string());
                } else {
                    repository = Some("pacman".to_string());
                }
            }
            packages.push(InstalledPackage {
                name: std::mem::take(&mut name),
                version,
                repository,
                installed_size,
                install_date,
                validated_by,
                package_hash,
            });
        }
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

/// Retrieve remote repository versions for the specified packages via `pacman -Si`.
pub async fn query_repo_versions(packages: &[String]) -> Result<HashMap<String, VersionInfo>> {
    let mut versions = HashMap::new();
    if packages.is_empty() {
        return Ok(versions);
    }

    const CHUNK_SIZE: usize = 64;
    for chunk in packages.chunks(CHUNK_SIZE) {
        let output = Command::new("pacman")
            .arg("-Si")
            .args(chunk)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|err| map_spawn_error(err, "pacman"))?;

        if !output.status.success() {
            return Err(SynsyuError::CommandFailure {
                command: format!("pacman -Si {}", chunk.join(" ")),
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            SynsyuError::Serialization(format!("pacman -Si emitted invalid UTF-8: {err}"))
        })?;

        let mut current: Option<String> = None;
        let mut current_version: Option<String> = None;
        let mut download_size: Option<u64> = None;
        let mut installed_size: Option<u64> = None;
        for line in stdout.lines() {
            if let Some((raw_key, raw_value)) = line.split_once(':') {
                let key = raw_key.trim();
                let value = raw_value.trim();
                match key {
                    "Name" => {
                        current = Some(value.to_string());
                        current_version = None;
                        download_size = None;
                        installed_size = None;
                    }
                    "Version" => {
                        current_version = Some(value.to_string());
                    }
                    "Download Size" => {
                        download_size = parse_pacman_size(value);
                    }
                    "Installed Size" => {
                        installed_size = parse_pacman_size(value);
                    }
                    _ => {}
                }
            } else if line.trim().is_empty() {
                if let (Some(name), Some(ver)) = (current.take(), current_version.take()) {
                    versions.insert(name, VersionInfo::new(ver, download_size, installed_size));
                }
                download_size = None;
                installed_size = None;
            }
        }
        if let (Some(name), Some(ver)) = (current.take(), current_version.take()) {
            versions.insert(name, VersionInfo::new(ver, download_size, installed_size));
        }
    }

    Ok(versions)
}

/// Retrieve version and size info for the specified packages via an AUR helper (paru/yay/etc.).
pub async fn query_aur_helper_versions(
    helper: &str,
    packages: &[String],
) -> Result<HashMap<String, VersionInfo>> {
    let mut versions = HashMap::new();
    if packages.is_empty() {
        return Ok(versions);
    }

    const CHUNK_SIZE: usize = 32;
    for chunk in packages.chunks(CHUNK_SIZE) {
        let output = Command::new(helper)
            .arg("-Si")
            .args(chunk)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|err| map_spawn_error(err, helper))?;

        if !output.status.success() {
            return Err(SynsyuError::CommandFailure {
                command: format!("{helper} -Si {}", chunk.join(" ")),
                status: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            SynsyuError::Serialization(format!("{helper} -Si emitted invalid UTF-8: {err}"))
        })?;

        let mut current: Option<String> = None;
        let mut current_version: Option<String> = None;
        let mut download_size: Option<u64> = None;
        let mut installed_size: Option<u64> = None;
        for line in stdout.lines() {
            if let Some((raw_key, raw_value)) = line.split_once(':') {
                let key = raw_key.trim();
                let value = raw_value.trim();
                match key {
                    "Name" => {
                        current = Some(value.to_string());
                        current_version = None;
                        download_size = None;
                        installed_size = None;
                    }
                    "Version" => {
                        current_version = Some(value.to_string());
                    }
                    "Download Size" => {
                        download_size = parse_pacman_size(value);
                    }
                    "Installed Size" => {
                        installed_size = parse_pacman_size(value);
                    }
                    _ => {}
                }
            } else if line.trim().is_empty() {
                if let (Some(name), Some(ver)) = (current.take(), current_version.take()) {
                    versions.insert(name, VersionInfo::new(ver, download_size, installed_size));
                }
                download_size = None;
                installed_size = None;
            }
        }
        if let (Some(name), Some(ver)) = (current.take(), current_version.take()) {
            versions.insert(name, VersionInfo::new(ver, download_size, installed_size));
        }
    }

    Ok(versions)
}

/// Compare two package versions using `vercmp`.
pub async fn compare_versions(local: &str, remote: &str) -> Result<std::cmp::Ordering> {
    let output = Command::new("vercmp")
        .arg(local)
        .arg(remote)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| map_spawn_error(err, "vercmp"))?;

    if !output.status.success() {
        return Err(SynsyuError::CommandFailure {
            command: format!("vercmp {local} {remote}"),
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| {
        SynsyuError::Serialization(format!("vercmp emitted invalid UTF-8: {err}"))
    })?;
    let verdict = stdout.trim();
    let ordering = i32::from_str(verdict).map_err(|err| {
        SynsyuError::Serialization(format!("Failed to parse vercmp output `{verdict}`: {err}"))
    })?;

    Ok(ordering.cmp(&0))
}

async fn detect_foreign_packages() -> Result<HashSet<String>> {
    let output = Command::new("pacman")
        .arg("-Qm")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        return Ok(HashSet::new());
    };
    if !output.status.success() {
        return Ok(HashSet::new());
    }
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    let set = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(|s| s.to_string())
        .collect();
    Ok(set)
}

/// Query AUR to see which package names exist there.
pub async fn aur_presence(
    names: &[String],
    offline: bool,
    aur_config: &AurConfig,
    policy: &AcquisitionAurRpcConfig,
    max_retries: usize,
    logger: &Logger,
) -> Result<HashSet<String>> {
    if offline || names.is_empty() {
        return Ok(HashSet::new());
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(aur_config.timeout.max(1)))
        .build()
        .map_err(|err| SynsyuError::Network(format!("AUR RPC client setup failed: {err}")))?;
    let mut found = HashSet::new();
    let chunk_size = aur_config.max_args.max(1);
    let attempts = if policy.enabled {
        max_retries.saturating_add(1)
    } else {
        1
    };
    for chunk in names.chunks(chunk_size) {
        let mut query = aur_rpc_base_query(&aur_config.base_url);
        for name in chunk {
            query.push_str("&arg[]=");
            query.push_str(encode(name).as_ref());
        }

        let mut last_error = String::new();
        let mut body: Option<AurResponse> = None;
        for attempt in 1..=attempts {
            match query_aur_rpc_once(&client, &query).await {
                Ok(response) => {
                    body = Some(response);
                    if attempt > 1 {
                        logger.info(
                            "AUR_RPC",
                            format!("AUR RPC request succeeded on attempt {attempt}/{attempts}"),
                        );
                    }
                    break;
                }
                Err(AurRpcAttemptError { message, retryable }) => {
                    last_error = message;
                    if !policy.enabled || !retryable || attempt >= attempts {
                        if retryable && attempt >= attempts {
                            logger.warn(
                                "AUR_RPC",
                                format!(
                                    "AUR RPC retry budget exhausted after {attempts} attempt(s): {last_error}"
                                ),
                            );
                        }
                        return Err(SynsyuError::Network(format!(
                            "AUR request failed: {last_error}"
                        )));
                    }
                    logger.warn(
                        "AUR_RPC",
                        format!(
                            "AUR RPC transient failure on attempt {attempt}/{attempts}: {last_error}"
                        ),
                    );
                    if policy.retry_delay_seconds > 0 {
                        sleep(Duration::from_secs(policy.retry_delay_seconds)).await;
                    }
                }
            }
        }

        let Some(body) = body else {
            return Err(SynsyuError::Network(format!(
                "AUR request failed: {last_error}"
            )));
        };
        if body.resp_type.as_deref() != Some("multiinfo") {
            continue;
        }
        if let Some(results) = body.results {
            for entry in results {
                if let Some(name) = entry.name {
                    found.insert(name);
                }
            }
        }
    }
    Ok(found)
}

struct AurRpcAttemptError {
    message: String,
    retryable: bool,
}

async fn query_aur_rpc_once(
    client: &Client,
    query: &str,
) -> std::result::Result<AurResponse, AurRpcAttemptError> {
    let resp = client
        .get(query)
        .send()
        .await
        .map_err(|err| AurRpcAttemptError {
            message: err.to_string(),
            retryable: err.is_timeout() || err.is_connect() || err.is_request(),
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AurRpcAttemptError {
            message: format!("HTTP status {status}"),
            retryable: aur_rpc_status_retryable(status),
        });
    }
    resp.json().await.map_err(|err| AurRpcAttemptError {
        message: format!("response parse failed: {err}"),
        retryable: true,
    })
}

fn aur_rpc_base_query(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        base = "https://aur.archlinux.org/rpc".to_string();
    }
    format!("{base}/?v=5&type=info")
}

fn aur_rpc_status_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
        || status.is_server_error()
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    #[serde(rename = "type")]
    resp_type: Option<String>,
    resultcount: Option<u64>,
    results: Option<Vec<AurEntry>>,
}

#[derive(Debug, Deserialize)]
struct AurEntry {
    #[serde(rename = "Name")]
    name: Option<String>,
}

pub fn parse_pacman_size(value: &str) -> Option<u64> {
    let mut parts = value.trim().split_whitespace();
    let number = parts.next()?.replace(',', "");
    let unit = parts.next().unwrap_or("B");
    let magnitude = number.parse::<f64>().ok()?;
    let multiplier = match unit {
        "B" => 1_f64,
        "KiB" => 1024_f64,
        "MiB" => 1024_f64.powi(2),
        "GiB" => 1024_f64.powi(3),
        "TiB" => 1024_f64.powi(4),
        _ => 1_f64,
    };
    let bytes = magnitude * multiplier;
    if bytes.is_finite() && bytes >= 0.0 {
        Some(bytes.round() as u64)
    } else {
        None
    }
}

fn map_spawn_error(err: io::Error, command: &str) -> SynsyuError {
    if err.kind() == io::ErrorKind::NotFound {
        SynsyuError::CommandMissing {
            command: command.into(),
        }
    } else {
        SynsyuError::Runtime(format!("Failed to spawn {command}: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aur_rpc_status_retry_policy_is_bounded_to_transient_http() {
        assert!(aur_rpc_status_retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(aur_rpc_status_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(!aur_rpc_status_retryable(StatusCode::NOT_FOUND));
        assert!(!aur_rpc_status_retryable(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn aur_rpc_base_query_normalizes_url() {
        assert_eq!(
            aur_rpc_base_query("https://aur.archlinux.org/rpc/"),
            "https://aur.archlinux.org/rpc/?v=5&type=info"
        );
    }
}
