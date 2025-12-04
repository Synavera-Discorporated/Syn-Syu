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
mod future;
mod logger;
mod fwupd;
mod manifest;
mod package_info;
mod pacman;
mod space;
mod plan;

use std::collections::{HashSet};
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Utc;
use clap::{ArgAction, Parser, Subcommand};
use std::io::{self, Write};
use std::io::IsTerminal;

use build_info::BUILD_INFO;
use config::SynsyuConfig;
use error::Result;
use logger::Logger;
use manifest::{build_manifest, write_manifest, ManifestDocument};
use pacman::{enumerate_installed_packages, InstalledPackage};
use plan::PlanCommand;
use fwupd::collect_fwupd;

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
    let manifest_path = cmd
        .manifest
        .clone()
        .unwrap_or_else(|| config.manifest_path());
    let output = cmd.execute(&config, manifest_path, plan_path.clone()).await?;

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output.plan_json)
                .unwrap_or_else(|_| "{}".to_string())
        );
        return Ok(ExitCode::SUCCESS);
    }

    let counts = output.plan_json.get("counts").cloned().unwrap_or_default();
    let pac = counts.get("pacman").and_then(|v| v.as_u64()).unwrap_or(0);
    let aur = counts.get("aur").and_then(|v| v.as_u64()).unwrap_or(0);
    let flat = counts.get("flatpak").and_then(|v| v.as_u64()).unwrap_or(0);
    let fw = counts.get("fwupd").and_then(|v| v.as_u64()).unwrap_or(0);
    let total = pac + aur + flat + fw;
    let meta = output.plan_json.get("metadata").cloned().unwrap_or_default();
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

    if args.with_fwupd {
        if let Ok(Some(fwupd)) = collect_fwupd(&logger).await {
            document.applications.fwupd = Some(fwupd);
        } else {
            logger.warn("FWUPD", "Firmware state unavailable; proceeding without fwupd data.");
        }
    }

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

async fn classify_aur_packages(
    packages: &mut [InstalledPackage],
    offline: bool,
    logger: &Logger,
) {
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
