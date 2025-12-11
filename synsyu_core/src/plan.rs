use std::path::PathBuf;
use std::process::Stdio;

use chrono::Utc;
use clap::{ArgAction, Args};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::config::SynsyuConfig;
use crate::error::{Result, SynsyuError};
use crate::fwupd::collect_fwupd_updates_for_plan;

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

impl PlanCommand {
    pub async fn execute(
        &self,
        config: &SynsyuConfig,
        plan_path: PathBuf,
    ) -> Result<PlanOutput> {
        let mut errors: Vec<String> = Vec::new();
        let mut sources: Vec<String> = Vec::new();

        let mut pacman_updates = Vec::new();
        let mut aur_updates = Vec::new();
        let mut flatpak_updates = Vec::new();
        let mut fwupd_updates = Vec::new();

        if !self.no_repo {
            sources.push("pacman".to_string());
            let (updates, errs) = collect_pacman_updates().await;
            pacman_updates = updates;
            errors.extend(errs);
        }

        if !self.no_aur && !self.offline {
            sources.push("aur".to_string());
            let helper = resolve_aur_helper(config);
            let (updates, errs) = collect_aur_updates(helper.as_deref()).await;
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

        let generated_at = Utc::now().to_rfc3339();

        let plan_json = json!({
            "metadata": {
                "generated_at": generated_at,
                "generated_by": "synsyu_core plan",
                "plan_path": plan_path.display().to_string(),
                "sources": sources,
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
            tokio::fs::create_dir_all(parent).await.map_err(|err| {
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
        file.write_all(json_pretty.as_bytes())
            .await
            .map_err(|err| {
                SynsyuError::Filesystem(format!(
                    "Failed to write plan {}: {err}",
                    plan_path.display()
                ))
            })?;

        Ok(PlanOutput {
            plan_json,
            blocked: false,
        })
    }
}

async fn collect_pacman_updates() -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();

    let output = Command::new("pacman")
        .arg("-Qu")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        errors.push("pacman: failed to spawn".to_string());
        return (updates, errors);
    };
    if !output.status.success() {
        errors.push(format!(
            "pacman: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
        return (updates, errors);
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[2] == "->" {
            let name = parts[0].to_string();
            let installed = parts[1].to_string();
            let available = parts[3].to_string();
            updates.push(json!({
                "name": name,
                "installed": installed,
                "available": available,
                "source": "pacman"
            }));
        }
    }

    (updates, errors)
}

async fn collect_aur_updates(helper: Option<&str>) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();

    let Some(helper) = helper else {
        errors.push("AUR: no helper available".to_string());
        return (updates, errors);
    };

    let output = Command::new(helper)
        .args(["-Qua"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        errors.push("AUR: failed to spawn helper".to_string());
        return (updates, errors);
    };
    if !output.status.success() {
        errors.push(format!(
            "AUR: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
        return (updates, errors);
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[2] == "->" {
            let name = parts[0].to_string();
            let installed = parts[1].to_string();
            let available = parts[3].to_string();
            updates.push(json!({
                "name": name,
                "installed": installed,
                "available": available,
                "source": "aur"
            }));
        }
    }

    (updates, errors)
}

async fn collect_flatpak_updates() -> (Vec<serde_json::Value>, Vec<String>) {
    let mut updates = Vec::new();
    let mut errors = Vec::new();
    let output = Command::new("flatpak")
        .args([
            "remote-ls",
            "--updates",
            "--columns=application,branch,origin,version",
        ])
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
            let available = if parts.len() >= 4 {
                parts[3].to_string()
            } else {
                String::new()
            };
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
    let (raw_updates, errs) = collect_fwupd_updates_for_plan().await;
    let updates = raw_updates
        .into_iter()
        .map(|u| {
            json!({
                "device": u.device,
                "name": u.name,
                "installed": u.installed,
                "available": u.available,
                "summary": u.summary,
                "available_hash": u.available_hash,
                "trust": u.trust,
                "source": "fwupd"
            })
        })
        .collect();
    (updates, errs)
}

fn resolve_aur_helper(config: &SynsyuConfig) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(default) = config.helpers.default.clone() {
        candidates.push(default);
    }
    candidates.extend(config.helpers.priority.clone());
    for helper in candidates {
        if let Ok(output) = std::process::Command::new(&helper)
            .arg("--version")
            .output()
        {
            if output.status.success() {
                return Some(helper);
            }
        }
    }
    None
}
