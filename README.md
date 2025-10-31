# Syn-Syu

Syn-Syu is Synavera's conscious successor to `pacman -Syu`. A Bash orchestrator
(`syn-syu`) works together with a Rust backend (`synsyu_core`) to build a
manifest of safe package upgrades, apply repo and AUR updates selectively, and
produce detailed logs for review.

## Features

- Coordinates pacman and AUR helpers with explicit include/exclude filters.
- Generates a JSON manifest via `synsyu_core` for dry runs, reporting, and disk
  safety checks.
- Configurable logging level with time and size-based log retention policies.
- Validates download, build, and install footprint before updates, applying a
  configurable free-space buffer.
- Supports guided or advanced install workflows through optional tooling.
- Logs every action with timestamped entries to simplify auditing.
- Provides commands for sync, targeted updates, group operations, cleaning, and
  inspection.

See `docs/Syn-Syu_Overview.md` for a deeper walkthrough of the architecture and
CLI behaviour.

## Install

### Arch package (recommended)

The repository ships with a ready-to-build PKGBUILD.

```bash
git clone git@github.com:CmdDraven/Syn-Syu.git
cd Syn-Syu
makepkg -sif
```

This installs:

- `/usr/bin/syn-syu` (Bash orchestrator)
- `/usr/bin/synsyu_core` (Rust backend)
- `/usr/lib/syn-syu/*.sh` (helper libraries)
- `/usr/share/doc/syn-syu/` (documentation and examples)

### Manual install

If you prefer to install outside of pacman, use the helper script:

```bash
./tools/install_syn-syu.sh
```

The wizard can perform a guided install, build `synsyu_core` in release mode,
and copy binaries and libraries into a prefix you choose. An accompanying
`tools/uninstall_syn-syu.sh` script safely removes the same assets.

## Configuration

Syn-Syu looks for configuration at `~/.config/syn-syu/config.toml`. An example
file is provided at `examples/config.toml`. Key options include:

- `core.manifest_path` – output path for the generated manifest.
- `space.min_free_gb` – reserved buffer that must remain free after updates.
- `logging.level` – choose from `debug`, `info`, `warn`, `error`, or `none`.
- `logging.retention_days` / `logging.retention_megabytes` – prune old logs by
  age or aggregate size.
- `logging.directory` – explicit log location (falls back to `core.log_directory`
  for compatibility).
- `helpers.priority` – ordered list of AUR helpers to try.
- `snapshots.*` – optional pre/post commands for snapshot integrations.
- `safety.disk_check` / `safety.disk_extra_margin_mb` – enable disk guards and
  define additional safety margin before installs proceed.

CLI flags such as `--config`, `--include`, `--exclude`, `--dry-run`,
`--no-aur`, `--no-repo`, and `--min-free-gb` override configuration on demand.

## Usage

Common entry points:

```bash
syn-syu            # Sync repo metadata, rebuild manifest, prompt for updates
syn-syu --dry-run  # Preview updates without making changes
syn-syu update <pkg>...
syn-syu group <name>
syn-syu clean
```

Use `syn-syu --help` for the full command set, including manifest inspection,
logging, and AUR-only or repo-only operations.

## Development

The Rust backend lives in `synsyu_core/` and is vendored directly into this
repository. Useful commands:

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo test
cargo build --release
```

Bash code follows the Synavera Script Etiquette guidelines (see `docs/`).
Contributions should include appropriate logging and remain shellchecked where
possible.
