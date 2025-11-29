/*============================================================
  Synavera Project: Syn-Syu
  Module: synsyu_core::main
  Etiquette: Synavera Script Etiquette — Rust Profile v1.1
  ------------------------------------------------------------
  Purpose:
    Unified CLI entry for Syn-Syu. Parses the full orchestrator
    interface, delegates orchestration to the Bash driver, and
    performs manifest generation directly for the core command.

  Security / Safety Notes:
    Operates within user privileges. Invokes pacman/vercmp and
    performs HTTPS GET requests only when building manifests.
============================================================*/

mod aur;
mod config;
mod error;
mod future;
mod logger;
mod manifest;
mod package_info;
mod pacman;
mod space;

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use chrono::Utc;
use clap::{ArgAction, Args, Parser, Subcommand};

use aur::AurClient;
use config::SynsyuConfig;
use error::{Result, SynsyuError};
use logger::Logger;
use manifest::{build_manifest, write_manifest, ManifestDocument};
use package_info::VersionInfo;
use pacman::{enumerate_installed_packages, query_repo_versions, InstalledPackage};
use space::{assess_default_paths, ensure_capacity, format_bytes};

#[derive(Parser, Debug)]
#[command(
    name = "Syn-Syu",
    version,
    author = "Synavera Systems",
    about = "Conscious package orchestration"
)]
struct Cli {
    #[command(flatten)]
    globals: GlobalArgs,
    #[command(subcommand)]
    command: Option<CommandSpec>,
    /// Internal flag used by the Bash driver to trigger manifest generation without re-dispatch.
    #[arg(long = "internal-manifest-build", hide = true, action = ArgAction::SetTrue)]
    internal_manifest_build: bool,
}

#[derive(Args, Debug, Clone)]
struct GlobalArgs {
    /// Use alternate configuration file.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Override manifest location.
    #[arg(long, value_name = "PATH")]
    manifest: Option<PathBuf>,
    /// Explicit log file path (manifest generation only).
    #[arg(long, value_name = "PATH")]
    log: Option<PathBuf>,
    /// Force manifest rebuild before command.
    #[arg(long, action = ArgAction::SetTrue)]
    rebuild: bool,
    /// Simulate actions without applying.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Disable AUR operations.
    #[arg(long, action = ArgAction::SetTrue)]
    no_aur: bool,
    /// Disable repo operations.
    #[arg(long, action = ArgAction::SetTrue)]
    no_repo: bool,
    /// Stream logs to stderr.
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,
    /// Suppress non-essential output.
    #[arg(long, short = 'q', action = ArgAction::SetTrue)]
    quiet: bool,
    /// JSON output for check/inspect.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    /// Ask for confirmation in helpers (drop --noconfirm).
    #[arg(long, action = ArgAction::SetTrue)]
    confirm: bool,
    /// Force a specific AUR helper.
    #[arg(long = "helper", value_name = "NAME")]
    helper: Option<String>,
    /// Include only packages matching regex (repeatable).
    #[arg(long = "include", value_name = "REGEX", action = ArgAction::Append)]
    include: Vec<String>,
    /// Exclude packages matching regex (repeatable).
    #[arg(long = "exclude", value_name = "REGEX", action = ArgAction::Append)]
    exclude: Vec<String>,
    /// Override required free space buffer in gigabytes.
    #[arg(long = "min-free-gb", value_name = "GB")]
    min_free_gb: Option<f64>,
    /// Batch size for repo installs (default from config or 10).
    #[arg(long = "batch", value_name = "N")]
    batch_size: Option<usize>,
    /// Override group configuration path.
    #[arg(long = "groups", value_name = "PATH")]
    groups_path: Option<PathBuf>,
    /// Include Flatpak updates in manifest and sync.
    #[arg(long = "with-flatpak", action = ArgAction::SetTrue)]
    with_flatpak: bool,
    /// Skip Flatpak updates (overrides config/manifest).
    #[arg(long = "no-flatpak", action = ArgAction::SetTrue)]
    no_flatpak: bool,
    /// Include firmware updates in manifest and sync.
    #[arg(long = "with-fwupd", action = ArgAction::SetTrue)]
    with_fwupd: bool,
    /// Skip firmware updates (overrides config/manifest).
    #[arg(long = "no-fwupd", action = ArgAction::SetTrue)]
    no_fwupd: bool,
}

#[derive(Subcommand, Debug, Clone)]
enum CommandSpec {
    /// Update all packages per manifest.
    Sync,
    /// Rebuild manifest via synsyu_core.
    Core(CoreArgs),
    /// Update only AUR packages.
    Aur,
    /// Update only repo packages.
    Repo,
    /// Update specific packages.
    Update {
        #[arg(value_name = "PKG")]
        packages: Vec<String>,
    },
    /// Update package group defined in groups.toml.
    Group {
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Show manifest detail for package.
    Inspect {
        #[arg(value_name = "PKG")]
        package: String,
    },
    /// Summarize manifest contents.
    Check,
    /// Prune caches and remove orphans.
    Clean,
    /// List recent Syn-Syu log files.
    Log,
    /// Export package lists for replication.
    Export(ExportArgs),
    /// Apply Flatpak application updates.
    Flatpak,
    /// Apply firmware updates via fwupdmgr.
    Fwupd,
    /// Apply both Flatpak and firmware updates.
    Apps,
    /// Display version information.
    Version,
}

#[derive(Args, Debug, Clone, Default)]
struct CoreArgs {
    /// Limit manifest to specific packages.
    #[arg(long = "package", value_name = "PKG", action = ArgAction::Append)]
    packages: Vec<String>,
}

#[derive(Args, Debug, Clone)]
struct ExportArgs {
    #[arg(long = "format", value_name = "FMT", value_parser = ["json", "plain"])]
    format: Option<String>,
    #[arg(long = "output", short = 'o', value_name = "PATH")]
    output: Option<PathBuf>,
    #[arg(long = "repo-only", action = ArgAction::SetTrue)]
    repo_only: bool,
    #[arg(long = "aur-only", action = ArgAction::SetTrue)]
    aur_only: bool,
    #[arg(long = "json", action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long = "plain", action = ArgAction::SetTrue)]
    plain: bool,
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
    let cli = Cli::parse_from(normalize_args(env::args_os()));

    if cli.internal_manifest_build {
        let core_args = match cli.command.clone() {
            Some(CommandSpec::Core(args)) => args,
            _ => CoreArgs::default(),
        };
        return run_manifest_builder(&cli.globals, &core_args).await;
    }

    let mut command = cli.command.unwrap_or(CommandSpec::Sync);
    // Apply implicit command transforms.
    match command {
        CommandSpec::Aur => {
            // AUR-only implies repo disabled.
            command = CommandSpec::Aur;
        }
        CommandSpec::Repo => {
            // Repo-only implies AUR disabled.
            command = CommandSpec::Repo;
        }
        _ => {}
    }

    if let CommandSpec::Version = command {
        println!("Syn-Syu orchestrator {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    run_driver(&cli.globals, &command).await
}

async fn run_manifest_builder(globals: &GlobalArgs, core_args: &CoreArgs) -> Result<ExitCode> {
    if globals.no_aur && globals.no_repo {
        return Err(SynsyuError::Config(
            "Cannot disable both repo and AUR resolution".into(),
        ));
    }

    let config_path = globals.config.as_deref();
    let config = SynsyuConfig::load_from_optional_path(config_path)?;
    let min_free_bytes = globals
        .min_free_gb
        .map(gb_to_bytes)
        .unwrap_or_else(|| config.min_free_bytes());

    let manifest_path = globals
        .manifest
        .clone()
        .unwrap_or_else(|| config.manifest_path());

    let session_stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let log_path = globals
        .log
        .clone()
        .or_else(|| Some(config.log_dir().join(format!("core_{session_stamp}.log"))));
    let logger = Logger::new(log_path.clone(), globals.verbose)?;
    logger.info("INIT", "Syn-Syu Core awakening.");

    let mut installed = enumerate_installed_packages().await?;
    logger.info(
        "PACKAGES",
        format!("Detected {} installed packages", installed.len()),
    );

    let selected = filter_packages(&mut installed, &core_args.packages, &logger)?;
    if selected.is_empty() {
        logger.warn(
            "EMPTY",
            "No packages selected for manifest generation; exiting",
        );
        logger.finalize()?;
        return Ok(ExitCode::SUCCESS);
    }

    let repo_versions: HashMap<String, VersionInfo> = if globals.no_repo {
        HashMap::new()
    } else {
        let repo_candidates: Vec<String> = selected
            .iter()
            .filter(|pkg| {
                pkg.repository
                    .as_deref()
                    .map(|r| r != "local")
                    .unwrap_or(false)
            })
            .map(|pkg| pkg.name.clone())
            .collect();
        if repo_candidates.is_empty() {
            HashMap::new()
        } else {
            query_repo_versions(&repo_candidates).await?
        }
    };

    let aur_versions: HashMap<String, VersionInfo> = if globals.no_aur {
        HashMap::new()
    } else {
        let aur_candidates: Vec<String> = selected
            .iter()
            .filter(|pkg| repo_versions.get(&pkg.name).is_none())
            .map(|pkg| pkg.name.clone())
            .collect();
        if aur_candidates.is_empty() {
            HashMap::new()
        } else {
            let aur_client = AurClient::new(&config.aur)?;
            aur_client.fetch_versions(&aur_candidates).await?
        }
    };

    logger.info(
        "SOURCES",
        format!(
            "Repo candidates={} AUR candidates={}",
            repo_versions.len(),
            aur_versions.len()
        ),
    );

    let mut document = build_manifest(
        &selected,
        &repo_versions,
        &aur_versions,
        min_free_bytes,
        &logger,
    )
    .await?;

    let required_total = document.metadata.required_space_total;
    let download_total = document.metadata.download_size_total;
    let build_total = document.metadata.build_size_total;
    let install_total = document.metadata.install_size_total;

    let space_report = assess_default_paths()?;
    document.metadata.available_space_bytes = space_report.available_bytes;
    document.metadata.space_checked_path = space_report.checked_path.display().to_string();

    if document.metadata.transient_size_total > 0 {
        match ensure_capacity(
            &space_report,
            required_total,
            download_total,
            build_total,
            install_total,
            min_free_bytes,
        ) {
            Ok(_) => {
                logger.info(
                    "DISK",
                    format!(
                        "Space OK: need {} (download {} + build {} + install {} + buffer {}), have {} on {}",
                        format_bytes(required_total),
                        format_bytes(download_total),
                        format_bytes(build_total),
                        format_bytes(install_total),
                        format_bytes(min_free_bytes),
                        format_bytes(space_report.available_bytes),
                        space_report.checked_path.display()
                    ),
                );
            }
            Err(message) => {
                if globals.dry_run {
                    logger.warn("DISK", &message);
                } else {
                    logger.error("DISK", &message);
                    logger.finalize()?;
                    return Err(SynsyuError::Runtime(message));
                }
            }
        }
    } else {
        logger.info(
            "DISK",
            format!(
                "No updates selected; available {} on {}",
                format_bytes(space_report.available_bytes),
                space_report.checked_path.display()
            ),
        );
    }

    if globals.dry_run {
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
            "packages={} updates={}",
            document.metadata.total_packages, document.metadata.updates_available
        ),
    );
    logger.info("COMPLETE", "Consciousness synchronised.");
    logger.finalize()?;

    Ok(ExitCode::SUCCESS)
}

fn gb_to_bytes(value: f64) -> u64 {
    if value <= 0.0 {
        0
    } else {
        (value * 1024.0_f64 * 1024.0_f64 * 1024.0_f64).round() as u64
    }
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
        "→ Manifest dry-run. Packages={} Updates={} (Repo candidates={} AUR candidates={})",
        document.metadata.total_packages,
        document.metadata.updates_available,
        document.metadata.repo_candidates,
        document.metadata.aur_candidates
    );
}

async fn run_driver(globals: &GlobalArgs, command: &CommandSpec) -> Result<ExitCode> {
    let driver = resolve_driver_script()?;
    let mut cmd = Command::new(&driver);
    let args = driver_args(command);
    cmd.args(&args);
    cmd.envs(driver_env(globals, command)?);

    // Point the driver back to this binary for manifest rebuilds.
    if let Ok(exe) = env::current_exe() {
        cmd.env("SYN_CORE_BIN", exe);
    }

    let status = cmd.status().map_err(|err| {
        SynsyuError::Runtime(format!("Failed to launch driver {driver:?}: {err}"))
    })?;
    if let Some(code) = status.code() {
        return Ok(ExitCode::from(code as u8));
    }
    Err(SynsyuError::Runtime(
        "Driver terminated by signal; no exit code available".into(),
    ))
}

fn driver_args(command: &CommandSpec) -> Vec<String> {
    match command {
        CommandSpec::Sync => vec!["sync".into()],
        CommandSpec::Core(_) => vec!["core".into()],
        CommandSpec::Aur => vec!["aur".into()],
        CommandSpec::Repo => vec!["repo".into()],
        CommandSpec::Update { packages } => {
            let mut args = vec!["update".into()];
            args.extend(packages.clone());
            args
        }
        CommandSpec::Group { name } => vec!["group".into(), name.clone()],
        CommandSpec::Inspect { package } => vec!["inspect".into(), package.clone()],
        CommandSpec::Check => vec!["check".into()],
        CommandSpec::Clean => vec!["clean".into()],
        CommandSpec::Log => vec!["log".into()],
        CommandSpec::Export(opts) => export_args(opts),
        CommandSpec::Flatpak => vec!["flatpak".into()],
        CommandSpec::Fwupd => vec!["fwupd".into()],
        CommandSpec::Apps => vec!["apps".into()],
        CommandSpec::Version => vec!["version".into()],
    }
}

fn export_args(opts: &ExportArgs) -> Vec<String> {
    let mut args = vec!["export".into()];
    if let Some(fmt) = &opts.format {
        args.push("--format".into());
        args.push(fmt.clone());
    }
    if let Some(path) = &opts.output {
        args.push("--output".into());
        args.push(path.display().to_string());
    }
    if opts.repo_only {
        args.push("--repo-only".into());
    }
    if opts.aur_only {
        args.push("--aur-only".into());
    }
    if opts.json {
        args.push("--json".into());
    }
    if opts.plain {
        args.push("--plain".into());
    }
    args
}

fn driver_env(globals: &GlobalArgs, command: &CommandSpec) -> Result<Vec<(String, String)>> {
    let mut envs = Vec::new();
    if let Some(config) = &globals.config {
        envs.push(("SYNSYU_CONFIG_PATH".into(), config.display().to_string()));
    }
    if let Some(manifest) = &globals.manifest {
        envs.push((
            "SYNSYU_MANIFEST_PATH".into(),
            manifest.display().to_string(),
        ));
    }
    if let Some(groups) = &globals.groups_path {
        envs.push(("SYNSYU_GROUPS_PATH".into(), groups.display().to_string()));
    }
    envs.push(("SYNSYU_REBUILD".into(), bool_flag(globals.rebuild)));
    envs.push(("SYNSYU_DRY_RUN".into(), bool_flag(globals.dry_run)));
    envs.push((
        "SYNSYU_NO_AUR".into(),
        bool_flag(globals.no_aur || matches!(command, CommandSpec::Repo)),
    ));
    envs.push((
        "SYNSYU_NO_REPO".into(),
        bool_flag(globals.no_repo || matches!(command, CommandSpec::Aur)),
    ));
    envs.push(("SYNSYU_VERBOSE".into(), bool_flag(globals.verbose)));
    envs.push(("SYNSYU_QUIET".into(), bool_flag(globals.quiet)));
    envs.push(("SYNSYU_JSON".into(), bool_flag(globals.json)));
    envs.push(("SYNSYU_CONFIRM".into(), bool_flag(globals.confirm)));

    if let Some(helper) = &globals.helper {
        envs.push(("SYNSYU_HELPER".into(), helper.clone()));
    }

    if !globals.include.is_empty() {
        envs.push(("SYNSYU_INCLUDE".into(), globals.include.join("\n")));
    }
    if !globals.exclude.is_empty() {
        envs.push(("SYNSYU_EXCLUDE".into(), globals.exclude.join("\n")));
    }

    if let Some(batch) = globals.batch_size {
        envs.push(("SYNSYU_BATCH_SIZE".into(), batch.to_string()));
    }

    if let Some(min_free) = globals.min_free_gb {
        envs.push(("SYNSYU_MIN_FREE_GB".into(), min_free.to_string()));
        envs.push((
            "SYNSYU_MIN_FREE_BYTES".into(),
            gb_to_bytes(min_free).to_string(),
        ));
    }

    if globals.with_flatpak {
        envs.push(("SYNSYU_WITH_FLATPAK".into(), "1".into()));
    } else if globals.no_flatpak {
        envs.push(("SYNSYU_WITH_FLATPAK".into(), "0".into()));
    }

    if globals.with_fwupd {
        envs.push(("SYNSYU_WITH_FWUPD".into(), "1".into()));
    } else if globals.no_fwupd {
        envs.push(("SYNSYU_WITH_FWUPD".into(), "0".into()));
    }

    Ok(envs)
}

fn bool_flag(value: bool) -> String {
    if value { "1" } else { "0" }.to_string()
}

fn resolve_driver_script() -> Result<PathBuf> {
    if let Ok(path) = env::var("SYN_SYU_DRIVER") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("driver.sh"));
            candidates.push(dir.join("../lib/driver.sh"));
            candidates.push(dir.join("../synsyu/lib/driver.sh"));
            candidates.push(dir.join("../../synsyu/lib/driver.sh"));
        }
    }

    candidates.extend(
        [
            "/usr/local/share/syn-syu/lib/driver.sh",
            "/usr/share/syn-syu/lib/driver.sh",
            "/usr/lib/syn-syu/lib/driver.sh",
            "/usr/lib/syn-syu/driver.sh",
            "/usr/share/syn-syu/driver.sh",
            "/usr/local/share/synsyu/lib/driver.sh",
            "/usr/lib/synsyu/lib/driver.sh",
        ]
        .iter()
        .map(PathBuf::from),
    );

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(SynsyuError::Runtime(
        "Unable to locate syn-syu driver (driver.sh)".into(),
    ))
}

fn normalize_args(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    if args.len() <= 2 {
        return args;
    }

    let bin = args.remove(0);
    let first = args.get(0).cloned();
    let subcommands: HashSet<&str> = HashSet::from([
        "sync", "core", "aur", "repo", "update", "group", "inspect", "check", "clean", "log",
        "export", "flatpak", "fwupd", "apps", "version", "help",
    ]);

    let Some(command_os) = first else {
        let mut restored = Vec::new();
        restored.push(bin);
        restored.extend(args);
        return restored;
    };
    let command_string = command_os.to_string_lossy();
    if !subcommands.contains(command_string.as_ref()) {
        let mut restored = Vec::new();
        restored.push(bin);
        restored.push(command_os);
        restored.extend(args.into_iter().skip(1));
        return restored;
    }

    let mut trailing_globals: Vec<OsString> = Vec::new();
    let mut remainder: Vec<OsString> = Vec::new();
    let mut iter = args.into_iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match classify_global(&arg) {
            GlobalKind::Flag => trailing_globals.push(arg),
            GlobalKind::Value => {
                trailing_globals.push(arg.clone());
                if let Some(next) = iter.next() {
                    trailing_globals.push(next);
                }
            }
            GlobalKind::InlineValue => trailing_globals.push(arg),
            GlobalKind::NotGlobal => remainder.push(arg),
        }
    }

    let mut normalized = Vec::new();
    normalized.push(bin);
    normalized.extend(trailing_globals);
    normalized.push(command_os);
    normalized.extend(remainder);
    normalized
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalKind {
    Flag,
    Value,
    InlineValue,
    NotGlobal,
}

fn classify_global(arg: &OsString) -> GlobalKind {
    let Some(raw) = arg.to_str() else {
        return GlobalKind::NotGlobal;
    };
    if raw.starts_with("--config=")
        || raw.starts_with("--manifest=")
        || raw.starts_with("--log=")
        || raw.starts_with("--helper=")
        || raw.starts_with("--include=")
        || raw.starts_with("--exclude=")
        || raw.starts_with("--min-free-gb=")
        || raw.starts_with("--batch=")
        || raw.starts_with("--groups=")
    {
        return GlobalKind::InlineValue;
    }

    match raw {
        "--config" | "--manifest" | "--log" | "--helper" | "--include" | "--exclude"
        | "--min-free-gb" | "--batch" | "--groups" => GlobalKind::Value,
        "--rebuild" | "--dry-run" | "--no-aur" | "--no-repo" | "--verbose" | "--quiet" | "-q"
        | "--json" | "--confirm" | "--no-flatpak" | "--with-flatpak" | "--with-fwupd"
        | "--no-fwupd" | "--version" | "-V" | "--help" | "-h" => GlobalKind::Flag,
        _ => GlobalKind::NotGlobal,
    }
}
