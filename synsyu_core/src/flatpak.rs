use serde::Serialize;
use tokio::process::Command;

use crate::logger::Logger;

#[derive(Debug, Serialize, Clone, Default)]
pub struct FlatpakState {
    pub enabled: bool,
    pub installed_count: usize,
    pub installed: Vec<FlatpakApp>,
    pub update_count: usize,
    pub updates: Vec<FlatpakUpdate>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FlatpakApp {
    pub application: String,
    pub version: String,
    pub branch: String,
    pub origin: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FlatpakUpdate {
    pub application: String,
    pub branch: String,
    pub origin: String,
    pub available: String,
}

/// Collect installed flatpak applications and pending updates.
pub async fn collect_flatpak(logger: &Logger) -> Option<FlatpakState> {
    let installed = match capture_installed().await {
        Some(list) => list,
        None => {
            logger.warn(
                "FLATPAK",
                "flatpak not available; skipping flatpak collection.",
            );
            return None;
        }
    };

    let updates = match capture_updates().await {
        Some(list) => list,
        None => {
            logger.warn(
                "FLATPAK",
                "flatpak updates unavailable; proceeding without update data.",
            );
            Vec::new()
        }
    };

    let state = FlatpakState {
        enabled: true,
        installed_count: installed.len(),
        installed,
        update_count: updates.len(),
        updates,
    };

    logger.info(
        "FLATPAK",
        format!(
            "Recorded flatpak state: installed={} updates={}",
            state.installed_count, state.update_count
        ),
    );

    Some(state)
}

async fn capture_installed() -> Option<Vec<FlatpakApp>> {
    let output = Command::new("flatpak")
        .args([
            "list",
            "--columns=application,version,branch,origin",
            "--app",
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut apps = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let application = parts.get(0).unwrap_or(&"").trim().to_string();
        if application.is_empty() {
            continue;
        }
        let version = parts.get(1).unwrap_or(&"").trim().to_string();
        let branch = parts.get(2).unwrap_or(&"").trim().to_string();
        let origin = parts.get(3).unwrap_or(&"").trim().to_string();
        apps.push(FlatpakApp {
            application,
            version,
            branch,
            origin,
        });
    }
    Some(apps)
}

async fn capture_updates() -> Option<Vec<FlatpakUpdate>> {
    let output = Command::new("flatpak")
        .args([
            "remote-ls",
            "--updates",
            "--columns=application,branch,origin,version",
            "--app",
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut updates = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let application = parts.get(0).unwrap_or(&"").trim().to_string();
        if application.is_empty() {
            continue;
        }
        let branch = parts.get(1).unwrap_or(&"").trim().to_string();
        let origin = parts.get(2).unwrap_or(&"").trim().to_string();
        let available = parts.get(3).unwrap_or(&"").trim().to_string();
        updates.push(FlatpakUpdate {
            application,
            branch,
            origin,
            available,
        });
    }
    Some(updates)
}
