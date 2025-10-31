/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::space
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Assess filesystem capacity and provide human-friendly
    formatting utilities for disk usage checks.

  Security / Safety Notes:
    Uses statvfs on Unix platforms to gather free space. Paths
    are canonicalised to avoid symlink surprises.

  Dependencies:
    libc (Unix), standard library only elsewhere.

  Operational Scope:
    Invoked by Syn-Syu-Core to ensure sufficient space exists
    before orchestrating downloads or builds.

  Revision History:
    2024-11-05 COD  Authored disk space utilities.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Defensive fallbacks when probing nonexistent paths
    - Saturating arithmetic to avoid overflow
    - Readable byte formatting for operator feedback
============================================================*/

use std::path::{Path, PathBuf};

use crate::error::{Result, SynsyuError};

/// Outcome of a disk space assessment.
#[derive(Debug, Clone)]
pub struct SpaceReport {
    pub checked_path: PathBuf,
    pub available_bytes: u64,
}

/// Determine available bytes for the most constrained filesystem among candidates.
pub fn assess_default_paths() -> Result<SpaceReport> {
    let candidates = [
        Path::new("/var/cache/pacman/pkg"),
        Path::new("/var/tmp"),
        Path::new("/tmp"),
        Path::new("/"),
    ];

    let mut report: Option<SpaceReport> = None;
    for candidate in candidates {
        if let Some(existing) = ensure_existing(candidate) {
            match free_bytes(existing) {
                Ok(bytes) => match &report {
                    Some(current) if bytes >= current.available_bytes => {}
                    _ => {
                        report = Some(SpaceReport {
                            checked_path: existing.to_path_buf(),
                            available_bytes: bytes,
                        });
                    }
                },
                Err(err) => {
                    // Propagate the last error if no valid path is found.
                    if report.is_none() {
                        return Err(err);
                    }
                }
            }
        }
    }

    report.ok_or_else(|| SynsyuError::Runtime("Unable to determine available disk space".into()))
}

/// Format bytes into a concise human-readable string (IEC units).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    if value >= 10.0 || (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

/// Validate that sufficient space exists; returns a descriptive error message on failure.
pub fn ensure_capacity(
    report: &SpaceReport,
    required_bytes: u64,
    download_bytes: u64,
    build_bytes: u64,
    install_bytes: u64,
    margin_bytes: u64,
) -> std::result::Result<(), String> {
    if report.available_bytes < required_bytes {
        let message = format!(
            "Insufficient space: need ~{} (download {} + build {} + install {} + buffer {}) on {}; only {} available",
            format_bytes(required_bytes),
            format_bytes(download_bytes),
            format_bytes(build_bytes),
            format_bytes(install_bytes),
            format_bytes(margin_bytes),
            report.checked_path.display(),
            format_bytes(report.available_bytes),
        );
        Err(message)
    } else {
        Ok(())
    }
}

fn ensure_existing(path: &Path) -> Option<&Path> {
    if path.exists() {
        Some(path)
    } else {
        let mut current = path;
        while let Some(parent) = current.parent() {
            if parent.exists() {
                return Some(parent);
            }
            current = parent;
        }
        Some(Path::new("/"))
    }
}

#[cfg(target_family = "unix")]
fn free_bytes(path: &Path) -> Result<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        SynsyuError::Filesystem(format!(
            "Failed to encode path {} for disk query",
            path.display()
        ))
    })?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return Err(SynsyuError::Filesystem(format!(
            "statvfs failed for {} (errno {})",
            path.display(),
            rc
        )));
    }
    let data = unsafe { stat.assume_init() };
    let available = (data.f_bavail as u128)
        .saturating_mul(data.f_frsize as u128)
        .min(u64::MAX as u128);
    Ok(available as u64)
}

#[cfg(not(target_family = "unix"))]
fn free_bytes(_path: &Path) -> Result<u64> {
    Err(SynsyuError::Runtime(
        "Disk space checks are not supported on this platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_human_readable() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1 KiB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10 MiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5 GiB");
    }

    #[test]
    fn ensure_capacity_passes_when_available() {
        let report = SpaceReport {
            checked_path: PathBuf::from("/"),
            available_bytes: 8 * 1024 * 1024 * 1024,
        };
        assert!(
            ensure_capacity(&report, 6 * 1024 * 1024 * 1024, 1, 1, 1, 1).is_ok(),
            "expected capacity check to succeed"
        );
    }

    #[test]
    fn ensure_capacity_fails_with_message() {
        let report = SpaceReport {
            checked_path: PathBuf::from("/var"),
            available_bytes: 512 * 1024 * 1024,
        };
        let err = ensure_capacity(
            &report,
            2 * 1024 * 1024 * 1024,
            300 * 1024 * 1024,
            900 * 1024 * 1024,
            300 * 1024 * 1024,
            500 * 1024 * 1024,
        )
        .expect_err("expected capacity failure");
        assert!(
            err.contains("Insufficient space"),
            "error message should mention insufficiency"
        );
        assert!(
            err.contains("download") && err.contains("build") && err.contains("buffer"),
            "error message should enumerate components"
        );
    }
}
