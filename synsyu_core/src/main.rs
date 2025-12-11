/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::main
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1.1
  ------------------------------------------------------------
  Purpose:
    Entry point for Syn-Syu Core. Enumerates installed packages
    and emits a structured manifest capturing the current system
    state for the Syn-Syu orchestrator.

  Security / Safety Notes:
    Operates within user privileges. Executes pacman only.

  Dependencies:
    clap for CLI parsing, chrono for timestamps.

  Operational Scope:
    Invoked by the Syn-Syu Bash layer via `syn-syu core` or when
    operators require standalone manifest regeneration.

  Revision History:
    2025-10-28 COD  Authored Syn-Syu Core runtime.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Result-first error handling with deterministic exits
    - Structured logging following Synavera cadence
    - Configurable execution via CLI and config file
============================================================*/

mod build_info;
mod config;
mod error;
mod flatpak;
mod future;
mod fwupd;
mod log_api;
mod logger;
mod manifest;
mod package_info;
mod pacman;
mod plan;
mod space;
mod updates;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Utc;
use clap::{ArgAction, Parser, Subcommand};
use std::io::IsTerminal;
use std::io::{self, Write};

use build_info::BUILD_INFO;
use config::SynsyuConfig;
use error::Result;
use flatpak::collect_flatpak;
use fwupd::collect_fwupd;
use log_api::{log_emit, log_hash, log_init, log_prune};
use logger::Logger;
use manifest::{build_manifest, write_manifest, ManifestDocument};
use pacman::{
    enumerate_installed_packages, query_aur_helper_versions, query_repo_versions, InstalledPackage,
};
use plan::PlanCommand;
use updates::{collect_updates, UpdatesFilter};

/// Top-level CLI entrypoint.
#[derive(Debug, Parser)]
#[command(
    name = "Syn-Syu-Core",
    version,
    author = "synavera_discorporated",
    about = "Conscious manifest builder for Syn-Syu",
    subcommand_required = false,
    arg_required_else_help = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[command(flatten)]
    core: CoreArgs,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Build an update plan from the manifest.
    Plan(PlanCommand),
    /// Show merged configuration (JSON or human-readable).
    Config(ConfigCommand),
    /// Check disk space against manifest requirements.
    Space(SpaceCommand),
    /// List applicable updates with filtering.
    Updates(UpdatesCommand),
    /// Logging helper commands.
    Logs(LogsCommand),
}

/// Core manifest-building arguments (also used as default when no subcommand is given).
#[derive(Debug, Parser, Clone)]
struct CoreArgs {
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Override manifest output path.
    #[arg(long, value_name = "PATH")]
    manifest: Option<PathBuf>,
    /// Explicit log file path.
    #[arg(long, value_name = "PATH")]
    log: Option<PathBuf>,
    /// Limit manifest to specific packages.
    #[arg(long = "package", value_name = "PKG", action = ArgAction::Append)]
    packages: Vec<String>,
    /// Do not write manifest; emit summary only.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Enable verbose logging to stderr.
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    /// Disable network access (skip AUR origin detection).
    #[arg(long, action = ArgAction::SetTrue)]
    offline: bool,
    /// Include firmware state via fwupdmgr in the manifest.
    #[arg(long = "with-fwupd", action = ArgAction::SetTrue)]
    with_fwupd: bool,
    /// Include Flatpak application state in the manifest.
    #[arg(long = "with-flatpak", action = ArgAction::SetTrue)]
    with_flatpak: bool,
}

/// Configuration inspection subcommand.
#[derive(Debug, Parser, Clone)]
struct ConfigCommand {
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Emit JSON output.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

/// Disk space assessment subcommand.
#[derive(Debug, Parser, Clone)]
struct SpaceCommand {
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Override manifest path.
    #[arg(long, value_name = "PATH")]
    manifest: Option<PathBuf>,
    /// Specific packages to check (uses manifest metrics if present).
    #[arg(long = "package", value_name = "PKG", action = ArgAction::Append)]
    packages: Vec<String>,
    /// Override minimum free bytes buffer.
    #[arg(long = "min-free-bytes", value_name = "BYTES")]
    min_free_bytes: Option<u64>,
    /// Extra disk margin in megabytes (adds to min-free).
    #[arg(long = "disk-margin-mb", value_name = "MB")]
    disk_margin_mb: Option<u64>,
    /// Target path to assess (falls back to defaults when omitted).
    #[arg(long = "path", value_name = "PATH")]
    path: Option<PathBuf>,
    /// Emit JSON output.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

/// Update listing subcommand.
#[derive(Debug, Parser, Clone)]
struct UpdatesCommand {
    /// Override manifest path.
    #[arg(long, value_name = "PATH")]
    manifest: Option<PathBuf>,
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Include packages matching regex (repeatable).
    #[arg(long = "include", value_name = "REGEX", action = ArgAction::Append)]
    include: Vec<String>,
    /// Exclude packages matching regex (repeatable).
    #[arg(long = "exclude", value_name = "REGEX", action = ArgAction::Append)]
    exclude: Vec<String>,
    /// Skip repo packages.
    #[arg(long = "no-repo", action = ArgAction::SetTrue)]
    no_repo: bool,
    /// Skip AUR packages.
    #[arg(long = "no-aur", action = ArgAction::SetTrue)]
    no_aur: bool,
    /// Limit to specific packages.
    #[arg(long = "package", value_name = "PKG", action = ArgAction::Append)]
    packages: Vec<String>,
    /// Emit JSON output.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

/// Logging helper subcommand.
#[derive(Debug, Parser, Clone)]
struct LogsCommand {
    /// Override configuration file path.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Emit a log entry: LEVEL CODE MESSAGE.
    #[arg(long = "emit", num_args = 3, value_names = ["LEVEL", "CODE", "MESSAGE"])]
    emit: Option<Vec<String>>,
    /// Initialize logging and return path/level info.
    #[arg(long = "init", action = ArgAction::SetTrue)]
    init: bool,
    /// Compute hash for a log file.
    #[arg(long = "hash", value_name = "PATH")]
    hash: Option<PathBuf>,
    /// Prune logs per retention policy.
    #[arg(long = "prune", action = ArgAction::SetTrue)]
    prune: bool,
    /// Explicit log path for emit.
    #[arg(long = "path", value_name = "PATH")]
    path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("[Syn-Syu-Core] {}", err);
            err.exit_code()
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    if let Some(cmd) = &cli.command {
        return match cmd {
            Commands::Plan(plan_cmd) => run_plan(plan_cmd).await,
            Commands::Config(cfg_cmd) => run_config(cfg_cmd),
            Commands::Space(space_cmd) => run_space(space_cmd).await,
            Commands::Updates(up_cmd) => run_updates(up_cmd),
            Commands::Logs(log_cmd) => run_logs(log_cmd),
        };
    }

    // Default to core mode if no subcommand provided.
    run_core(&cli.core).await
}

async fn run_plan(cmd: &PlanCommand) -> Result<ExitCode> {
    let config = SynsyuConfig::load_from_optional_path(cmd.config.as_deref())?;
    let plan_path = cmd.plan.clone().unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("syn-syu/plan.json")
    });
    let output = cmd.execute(&config, plan_path.clone()).await?;

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output.plan_json).unwrap_or_else(|_| "{}".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }

    let counts = output.plan_json.get("counts").cloned().unwrap_or_default();
    let pac = counts.get("pacman").and_then(|v| v.as_u64()).unwrap_or(0);
    let aur = counts.get("aur").and_then(|v| v.as_u64()).unwrap_or(0);
    let flat = counts.get("flatpak").and_then(|v| v.as_u64()).unwrap_or(0);
    let fw = counts.get("fwupd").and_then(|v| v.as_u64()).unwrap_or(0);
    let total = pac + aur + flat + fw;
    let meta = output
        .plan_json
        .get("metadata")
        .cloned()
        .unwrap_or_default();
    let generated = meta
        .get("generated_at")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let plan_path_val = plan_path.display().to_string();
    let sources = meta
        .get("sources")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let errors = meta
        .get("errors")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(space) = meta.get("space") {
        if let Some(warning) = space.get("warning").and_then(|v| v.as_str()) {
            eprintln!("Warning: {warning}");
        }
    }
    let error_count = errors.len();

    println!("Plan created at {}", generated);
    let sources_display: Vec<String> = sources
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    println!("Sources: {}", sources_display.join(", "));
    println!("Repo updates: {}", pac);
    println!("AUR updates: {}", aur);
    println!("Flatpak updates: {}", flat);
    println!("fwupd: {}", fw);
    println!();
    println!("Detailed JSON written to: {}", plan_path_val);
    if error_count > 0 {
        println!("Errors: {}", error_count);
    }

    if total > 0 && io::stdout().is_terminal() {
        println!();
        print!("Show update summary now? [y/N]: ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_ok() {
            let resp = line.trim().to_lowercase();
            if resp == "y" || resp == "yes" {
                println!("Pacman: {pac}");
                println!("AUR   : {aur}");
                println!("Flatpak: {flat}");
                println!("fwupd : {fw}");
            }
        }
    }

    if cmd.strict && error_count > 0 {
        return Ok(ExitCode::from(1));
    }

    if output.blocked {
        return Ok(ExitCode::from(1));
    }

    Ok(ExitCode::SUCCESS)
}

async fn run_core(args: &CoreArgs) -> Result<ExitCode> {
    let config_path = args.config.as_deref();
    let config = SynsyuConfig::load_from_optional_path(config_path)?;

    let manifest_path = args
        .manifest
        .clone()
        .unwrap_or_else(|| config.manifest_path());

    let session_stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let log_path = args
        .log
        .clone()
        .or_else(|| Some(config.log_dir().join(format!("core_{session_stamp}.log"))));
    let logger = Logger::new(log_path.clone(), args.verbose)?;
    logger.info("INIT", "Syn-Syu Core awakening.");
    let aur_pkg = if BUILD_INFO.aur_pkgver.is_empty() {
        "n/a".to_string()
    } else {
        let mut version = BUILD_INFO.aur_pkgver.to_string();
        if !BUILD_INFO.aur_pkgrel.is_empty() {
            version.push('-');
            version.push_str(BUILD_INFO.aur_pkgrel);
        }
        if !BUILD_INFO.aur_epoch.is_empty() && BUILD_INFO.aur_epoch != "0" {
            format!("{}:{}", BUILD_INFO.aur_epoch, version)
        } else {
            version
        }
    };
    let aur_commit = if BUILD_INFO.aur_commit.is_empty() {
        "n/a"
    } else {
        BUILD_INFO.aur_commit
    };
    let build_time = if BUILD_INFO.build_time.is_empty() {
        "unknown"
    } else {
        BUILD_INFO.build_time
    };
    let features = if BUILD_INFO.features.is_empty() {
        "none"
    } else {
        BUILD_INFO.features
    };
    logger.info(
        "BUILD",
        format!(
            "Syn-Syu {} (source={} git={} aur_pkg={} aur_commit={} rustc={} profile={} target={} built={} features={})",
            BUILD_INFO.version,
            BUILD_INFO.source,
            BUILD_INFO.git_commit,
            aur_pkg,
            aur_commit,
            BUILD_INFO.rustc_version,
            BUILD_INFO.build_profile,
            BUILD_INFO.target,
            build_time,
            features
        ),
    );

    let mut installed = enumerate_installed_packages().await?;
    classify_aur_packages(&mut installed, args.offline, &logger).await;
    logger.info(
        "PACKAGES",
        format!("Detected {} installed packages", installed.len()),
    );

    let enable_flatpak = args.with_flatpak || config.flatpak_enabled();
    let enable_fwupd = args.with_fwupd || config.fwupd_enabled();

    let selected = filter_packages(&mut installed, &args.packages, &logger)?;
    if selected.is_empty() {
        logger.warn(
            "EMPTY",
            "No packages selected for manifest generation; exiting",
        );
        logger.finalize()?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut document = build_manifest(&selected, &logger).await?;

    if enable_flatpak {
        match collect_flatpak(&logger).await {
            Some(flatpak) => {
                document.applications.flatpak = Some(flatpak);
            }
            None => logger.warn(
                "FLATPAK",
                "Flatpak state unavailable; proceeding without flatpak data.",
            ),
        }
    }

    if enable_fwupd {
        match collect_fwupd(&logger, true).await {
            Ok(Some(fwupd)) => {
                document.applications.fwupd = Some(fwupd);
            }
            Ok(None) => logger.warn(
                "FWUPD",
                "Firmware state unavailable; proceeding without fwupd data.",
            ),
            Err(err) => logger.warn("FWUPD", format!("Firmware capture failed: {err}")),
        }
    }

    document.refresh_application_metadata();

    if args.dry_run {
        print_summary(&document);
    } else {
        write_manifest(&document, &manifest_path)?;
        logger.info(
            "MANIFEST",
            format!("Manifest written to {}", manifest_path.display()),
        );
    }

    logger.info(
        "SUMMARY",
        format!(
            "packages={} pacman={} aur={} local={} unknown={}",
            document.metadata.total_packages,
            document.metadata.pacman_packages,
            document.metadata.aur_packages,
            document.metadata.local_packages,
            document.metadata.unknown_packages
        ),
    );
    logger.info("COMPLETE", "Consciousness synchronised.");
    logger.finalize()?;

    Ok(ExitCode::SUCCESS)
}

fn run_config(cmd: &ConfigCommand) -> Result<ExitCode> {
    let config = SynsyuConfig::load_from_optional_path(cmd.config.as_deref())?;
    let report = config.to_report();
    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("Manifest: {}", report.manifest_path.display());
        println!("Log dir : {}", report.log_directory.display());
        println!("Batch   : {}", report.batch_size);
        println!(
            "Helpers : {}",
            if report.helper_priority.is_empty() {
                "<none>".to_string()
            } else {
                report.helper_priority.join(", ")
            }
        );
        println!(
            "Space   : min_free_bytes={} policy={}",
            report.space_min_free_bytes, report.space_policy
        );
        println!(
            "Apps    : flatpak={} fwupd={}",
            report.applications_flatpak, report.applications_fwupd
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn run_space(cmd: &SpaceCommand) -> Result<ExitCode> {
    let config = SynsyuConfig::load_from_optional_path(cmd.config.as_deref())?;
    let manifest_path = cmd
        .manifest
        .clone()
        .unwrap_or_else(|| config.manifest_path());
    let min_free = cmd
        .min_free_bytes
        .unwrap_or_else(|| config.min_free_bytes());
    let disk_margin_bytes = cmd
        .disk_margin_mb
        .unwrap_or(config.safety.disk_extra_margin_mb)
        .saturating_mul(1024 * 1024);
    let margin = min_free.saturating_add(disk_margin_bytes);

    let manifest: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&manifest_path).map_err(|err| {
            crate::error::SynsyuError::Filesystem(format!(
                "Failed to open manifest {}: {err}",
                manifest_path.display()
            ))
        })?)
        .map_err(|err| {
            crate::error::SynsyuError::Serialization(format!(
                "Failed to parse manifest {}: {err}",
                manifest_path.display()
            ))
        })?;

    // Pre-fetch repo sizes for requested pacman packages to avoid relying solely on manifest data.
    let mut repo_pkg_names = Vec::new();
    if let Some(pkgs) = manifest.get("packages").and_then(|p| p.as_object()) {
        for pkg in &cmd.packages {
            if let Some(entry) = pkgs.get(pkg) {
                if entry
                    .get("source")
                    .and_then(|s| s.as_str())
                    .map(|s| s.eq_ignore_ascii_case("PACMAN"))
                    .unwrap_or(false)
                {
                    repo_pkg_names.push(pkg.clone());
                }
            }
        }
    }
    let _repo_sizes = if repo_pkg_names.is_empty() {
        std::collections::HashMap::new()
    } else {
        query_repo_versions(&repo_pkg_names)
            .await
            .unwrap_or_default()
    };

    // Optional AUR helper size lookup.
    let mut aur_helper: Option<String> = None;
    if let Some(default_helper) = config.helpers.default.clone() {
        if std::process::Command::new(&default_helper)
            .arg("--version")
            .output()
            .is_ok()
        {
            aur_helper = Some(default_helper);
        }
    }
    if aur_helper.is_none() {
        for helper in &config.helpers.priority {
            if std::process::Command::new(helper)
                .arg("--version")
                .output()
                .is_ok()
            {
                aur_helper = Some(helper.clone());
                break;
            }
        }
    }
    let aur_pkg_names: Vec<String> =
        if let Some(pkgs) = manifest.get("packages").and_then(|p| p.as_object()) {
            pkgs.iter()
                .filter_map(|(name, entry)| {
                    let source = entry.get("source").and_then(|s| s.as_str()).unwrap_or("");
                    if source.eq_ignore_ascii_case("AUR") {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
    let aur_sizes = if let Some(helper) = aur_helper {
        if aur_pkg_names.is_empty() {
            std::collections::HashMap::new()
        } else {
            query_aur_helper_versions(&helper, &aur_pkg_names)
                .await
                .unwrap_or_default()
        }
    } else {
        std::collections::HashMap::new()
    };

    let report = if let Some(path) = &cmd.path {
        space::assess_path(path)?
    } else {
        space::assess_default_paths()?
    };

    let mut failures = Vec::new();
    let mut details = Vec::new();
    let mut unknowns = Vec::new();

    // Aggregate check using manifest metadata if present.
    if let Some(meta) = manifest.get("metadata") {
        let transient = meta
            .get("transient_size_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let download = meta
            .get("download_size_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let build = meta
            .get("build_size_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let install = meta
            .get("install_size_total")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let required_transient = if transient > 0 {
            transient
        } else {
            download.saturating_add(build).saturating_add(install)
        };
        if required_transient > 0 {
            let required_total = required_transient.saturating_add(margin);
            if report.available_bytes < required_total {
                failures.push(format!(
                    "Insufficient space: need ~{} (download {} + build {} + install {} + buffer {}) on {}; have {}",
                    space::format_bytes(required_total),
                    space::format_bytes(download),
                    space::format_bytes(build),
                    space::format_bytes(install),
                    space::format_bytes(margin),
                    report.checked_path.display(),
                    space::format_bytes(report.available_bytes),
                ));
            } else {
                details.push(format!(
                    "OK: need ~{} (download {} + build {} + install {} + buffer {}) on {}; have {}",
                    space::format_bytes(required_total),
                    space::format_bytes(download),
                    space::format_bytes(build),
                    space::format_bytes(install),
                    space::format_bytes(margin),
                    report.checked_path.display(),
                    space::format_bytes(report.available_bytes),
                ));
            }
        }
    }

    // Pre-fetch repo sizes for requested pacman packages to avoid relying on manifest sizes.
    let mut repo_pkg_names = Vec::new();
    if let Some(pkgs) = manifest.get("packages").and_then(|p| p.as_object()) {
        for pkg in &cmd.packages {
            if let Some(entry) = pkgs.get(pkg) {
                if entry
                    .get("source")
                    .and_then(|s| s.as_str())
                    .map(|s| s.eq_ignore_ascii_case("PACMAN"))
                    .unwrap_or(false)
                {
                    repo_pkg_names.push(pkg.clone());
                }
            }
        }
    }
    let repo_sizes = if repo_pkg_names.is_empty() {
        std::collections::HashMap::new()
    } else {
        query_repo_versions(&repo_pkg_names)
            .await
            .unwrap_or_default()
    };

    // Per-package checks when requested.
    for pkg in &cmd.packages {
        if let Some(entry) = manifest.get("packages").and_then(|p| p.get(pkg)) {
            let source = entry
                .get("source")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let download = entry
                .get("download_size_selected")
                .or_else(|| entry.get("download_size_estimate"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let build = entry
                .get("build_size_estimate")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let install = entry
                .get("install_size_estimate")
                .or_else(|| entry.get("installed_size_selected"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let transient = entry
                .get("transient_size_estimate")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            // Prefer repo data for pacman packages to avoid relying on installed size.
            let (download, install, build) = if source.eq_ignore_ascii_case("PACMAN") {
                if let Some(info) = repo_sizes.get(pkg) {
                    (
                        info.download_size.unwrap_or(download),
                        info.installed_size.unwrap_or(install),
                        0u64,
                    )
                } else {
                    (download, install, build)
                }
            } else if source.eq_ignore_ascii_case("AUR") {
                if let Some(info) = aur_sizes.get(pkg) {
                    (
                        info.download_size.unwrap_or(download),
                        info.installed_size.unwrap_or(install),
                        0u64,
                    )
                } else {
                    (download, install, build)
                }
            } else {
                (download, install, build)
            };

            let required_base = if transient > 0 {
                transient
            } else {
                download.saturating_add(build).saturating_add(install)
            };
            if required_base == 0 {
                details.push(format!(
                    "WARN: no size telemetry for {pkg}; unable to validate disk usage"
                ));
                unknowns.push(pkg.clone());
                continue;
            }
            let required_total = required_base.saturating_add(margin);
            if report.available_bytes < required_total {
                failures.push(format!(
                    "Package {pkg}: need ~{} (download {} + build {} + install {} + buffer {}) on {}; have {}",
                    space::format_bytes(required_total),
                    space::format_bytes(download),
                    space::format_bytes(build),
                    space::format_bytes(install),
                    space::format_bytes(margin),
                    report.checked_path.display(),
                    space::format_bytes(report.available_bytes),
                ));
            } else {
                details.push(format!(
                    "Package {pkg}: OK need ~{} on {}; have {}",
                    space::format_bytes(required_total),
                    report.checked_path.display(),
                    space::format_bytes(report.available_bytes),
                ));
            }
        } else {
            details.push(format!(
                "WARN: {pkg} not found in manifest; skipping disk check"
            ));
        }
    }

    if cmd.json {
        let output = serde_json::json!({
            "checked_path": report.checked_path,
            "available_bytes": report.available_bytes,
            "margin_bytes": margin,
            "failures": failures,
            "unknown": unknowns,
            "details": details,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        for line in &details {
            println!("{line}");
        }
        for line in &failures {
            eprintln!("{line}");
        }
        if !unknowns.is_empty() {
            eprintln!("WARN: size telemetry missing for: {}", unknowns.join(", "));
        }
    }

    if !failures.is_empty() {
        Ok(ExitCode::from(1))
    } else if !unknowns.is_empty() {
        Ok(ExitCode::from(2))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn run_updates(cmd: &UpdatesCommand) -> Result<ExitCode> {
    let config = SynsyuConfig::load_from_optional_path(cmd.config.as_deref())?;
    let manifest_path = cmd
        .manifest
        .clone()
        .unwrap_or_else(|| config.manifest_path());
    let filter = UpdatesFilter {
        manifest: manifest_path,
        include: cmd.include.clone(),
        exclude: cmd.exclude.clone(),
        allow_repo: !cmd.no_repo,
        allow_aur: !cmd.no_aur,
        packages: cmd.packages.clone(),
    };
    let updates = collect_updates(filter)?;
    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&updates).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        for u in updates {
            println!("{}|{}|{}|{}", u.name, u.source, u.installed, u.available);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn run_logs(cmd: &LogsCommand) -> Result<ExitCode> {
    let config = SynsyuConfig::load_from_optional_path(cmd.config.as_deref())?;

    if cmd.init {
        let init = log_init(&config)?;
        println!(
            "{}|{}|{}",
            init.path.display(),
            init.level,
            init.directory.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    if cmd.prune {
        log_prune(&config)?;
    }

    if let Some(path) = &cmd.hash {
        let hash_path = log_hash(path)?;
        println!("{}", hash_path.display());
    }

    if let Some(parts) = &cmd.emit {
        if parts.len() == 3 {
            let level = &parts[0];
            let code = &parts[1];
            let message = &parts[2];
            let log_path = if let Some(p) = &cmd.path {
                p.clone()
            } else {
                let init = log_init(&config)?;
                init.path
            };
            log_emit(&log_path, level, code, message)?;
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn filter_packages(
    installed: &mut Vec<InstalledPackage>,
    requested: &[String],
    logger: &Logger,
) -> Result<Vec<InstalledPackage>> {
    installed.sort_by(|a, b| a.name.cmp(&b.name));

    if requested.is_empty() {
        return Ok(installed.clone());
    }

    let mut requested_set: HashSet<String> = HashSet::new();
    for pkg in requested {
        requested_set.insert(pkg.to_string());
    }

    let mut selected = Vec::new();
    for pkg in installed.iter() {
        if requested_set.contains(&pkg.name) {
            selected.push(pkg.clone());
        }
    }

    let missing: Vec<String> = requested_set
        .into_iter()
        .filter(|name| !selected.iter().any(|pkg| &pkg.name == name))
        .collect();

    if !missing.is_empty() {
        logger.warn(
            "PKG404",
            format!("Requested packages not installed: {}", missing.join(", ")),
        );
    }

    Ok(selected)
}

fn print_summary(document: &ManifestDocument) {
    println!(
        "→ Manifest dry-run. Packages={} (pacman={} aur={} local={} unknown={})",
        document.metadata.total_packages,
        document.metadata.pacman_packages,
        document.metadata.aur_packages,
        document.metadata.local_packages,
        document.metadata.unknown_packages
    );
}

async fn classify_aur_packages(packages: &mut [InstalledPackage], offline: bool, logger: &Logger) {
    let mut candidates = Vec::new();
    for pkg in packages.iter() {
        if pkg
            .repository
            .as_deref()
            .map(|r| r.eq_ignore_ascii_case("local"))
            .unwrap_or(true)
        {
            candidates.push(pkg.name.clone());
        }
    }
    if candidates.is_empty() {
        return;
    }
    if offline {
        logger.info("AUR", "Offline flag set; skipping AUR origin detection.");
        return;
    }
    match pacman::aur_presence(&candidates, offline).await {
        Ok(found) => {
            if found.is_empty() {
                logger.info("AUR", "No AUR matches found for foreign packages.");
                return;
            }
            let mut updated = 0usize;
            for pkg in packages.iter_mut() {
                if pkg
                    .repository
                    .as_deref()
                    .map(|r| r.eq_ignore_ascii_case("local"))
                    .unwrap_or(true)
                    && found.contains(&pkg.name)
                {
                    pkg.repository = Some("aur".to_string());
                    updated += 1;
                }
            }
            logger.info("AUR", format!("Classified {updated} package(s) as AUR."));
        }
        Err(err) => {
            logger.warn("AUR", format!("AUR origin detection skipped: {err}"));
        }
    }
}
