use std::collections::HashSet;
use std::path::PathBuf;

use regex::Regex;
use serde::Serialize;

use crate::error::{Result, SynsyuError};

#[derive(Debug, Serialize, Clone)]
pub struct UpdateEntry {
    pub name: String,
    pub source: String,
    pub installed: String,
    pub available: String,
}

pub struct UpdatesFilter {
    pub manifest: PathBuf,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub allow_repo: bool,
    pub allow_aur: bool,
    pub packages: Vec<String>,
}

pub fn collect_updates(filter: UpdatesFilter) -> Result<Vec<UpdateEntry>> {
    let file = std::fs::File::open(&filter.manifest).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to open manifest {}: {err}",
            filter.manifest.display()
        ))
    })?;
    let manifest: serde_json::Value = serde_json::from_reader(file).map_err(|err| {
        SynsyuError::Serialization(format!(
            "Failed to parse manifest {}: {err}",
            filter.manifest.display()
        ))
    })?;

    let include_res: Vec<Regex> = filter
        .include
        .iter()
        .filter_map(|pat| Regex::new(pat).ok())
        .collect();
    let exclude_res: Vec<Regex> = filter
        .exclude
        .iter()
        .filter_map(|pat| Regex::new(pat).ok())
        .collect();
    let packages_set: Option<HashSet<String>> = if filter.packages.is_empty() {
        None
    } else {
        Some(filter.packages.iter().cloned().collect())
    };

    let mut updates = Vec::new();
    if let Some(packages) = manifest.get("packages").and_then(|p| p.as_object()) {
        'outer: for (name, entry) in packages {
            if let Some(set) = &packages_set {
                if !set.contains(name) {
                    continue;
                }
            }

            let available_flag = entry
                .get("update_available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !available_flag {
                continue;
            }

            let source = entry
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            if source.eq_ignore_ascii_case("PACMAN") && !filter.allow_repo {
                continue;
            }
            if source.eq_ignore_ascii_case("AUR") && !filter.allow_aur {
                continue;
            }

            if !include_res.is_empty() {
                let mut matched = false;
                for re in &include_res {
                    if re.is_match(name) {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    continue;
                }
            }
            for re in &exclude_res {
                if re.is_match(name) {
                    continue 'outer;
                }
            }

            let installed = entry
                .get("installed_version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let available = entry
                .get("newer_version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            updates.push(UpdateEntry {
                name: name.to_string(),
                source,
                installed,
                available,
            });
        }
    }

    Ok(updates)
}
