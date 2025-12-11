use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::SynsyuConfig;
use crate::error::{Result, SynsyuError};

#[derive(Debug, Serialize, Clone)]
pub struct LogInit {
    pub path: PathBuf,
    pub level: String,
    pub directory: PathBuf,
}

pub fn log_init(config: &SynsyuConfig) -> Result<LogInit> {
    let dir = config.log_dir();
    fs::create_dir_all(&dir).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to create log directory {}: {err}",
            dir.display()
        ))
    })?;
    let stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let path = dir.join(format!("{}.log", stamp));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| {
            SynsyuError::Filesystem(format!("Failed to open log {}: {err}", path.display()))
        })?;
    Ok(LogInit {
        path,
        level: config
            .logging
            .level
            .clone()
            .unwrap_or_else(|| "info".to_string()),
        directory: dir,
    })
}

pub fn log_emit(path: &PathBuf, level: &str, code: &str, message: &str) -> Result<()> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let payload = format!("{timestamp} [{}] [{}] {}\n", level, code, message);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            SynsyuError::Filesystem(format!("Failed to open log {}: {err}", path.display()))
        })?
        .write_all(payload.as_bytes())
        .map_err(|err| {
            SynsyuError::Filesystem(format!("Failed to write log {}: {err}", path.display()))
        })?;
    Ok(())
}

pub fn log_hash(path: &PathBuf) -> Result<PathBuf> {
    let data = fs::read(path).map_err(|err| {
        SynsyuError::Filesystem(format!("Failed to read log {}: {err}", path.display()))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let digest = hasher.finalize();
    let mut hash_os = path.as_os_str().to_os_string();
    hash_os.push(".hash");
    let hash_path = PathBuf::from(hash_os);
    let mut file = fs::File::create(&hash_path).map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to create hash {}: {err}",
            hash_path.display()
        ))
    })?;
    writeln!(
        file,
        "{:x}  {}",
        digest,
        path.file_name().unwrap_or_default().to_string_lossy()
    )
    .map_err(|err| {
        SynsyuError::Filesystem(format!(
            "Failed to write hash {}: {err}",
            hash_path.display()
        ))
    })?;
    Ok(hash_path)
}

pub fn log_prune(config: &SynsyuConfig) -> Result<()> {
    let dir = config.log_dir();
    let days = config.logging.retention_days.unwrap_or(0);
    let bytes_limit = config
        .logging
        .retention_megabytes
        .unwrap_or(0)
        .saturating_mul(1024 * 1024);

    if days == 0 && bytes_limit == 0 {
        return Ok(());
    }
    if days > 0 {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(60 * 60 * 24 * days))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file()
                        && meta.modified().unwrap_or(std::time::SystemTime::now()) < cutoff
                        && entry.file_name().to_string_lossy().ends_with(".log")
                    {
                        let _ = fs::remove_file(entry.path());
                        let mut hash_path = entry.path();
                        hash_path.set_extension("log.hash");
                        let _ = fs::remove_file(hash_path);
                    }
                }
            }
        }
    }

    if bytes_limit > 0 {
        let mut logs: Vec<(std::time::SystemTime, PathBuf, u64)> = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() && entry.file_name().to_string_lossy().ends_with(".log") {
                        logs.push((
                            meta.modified().unwrap_or(std::time::SystemTime::now()),
                            entry.path(),
                            meta.len(),
                        ));
                    }
                }
            }
        }
        logs.sort_by_key(|(mtime, _, _)| *mtime);
        let mut total: u64 = logs.iter().map(|(_, _, size)| *size).sum();
        for (_, path, size) in logs {
            if total <= bytes_limit {
                break;
            }
            let _ = fs::remove_file(&path);
            let mut hash_path = path.clone();
            hash_path.set_extension("log.hash");
            let _ = fs::remove_file(hash_path);
            total = total.saturating_sub(size);
        }
    }

    Ok(())
}
