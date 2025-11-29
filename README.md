# Syn-Syu

Syn-Syu is Synavera's conscious successor to `pacman -Syu`. A single Rust CLI
(`synsyu_core`, exposed as the `syn-syu` entrypoint) drives the workflow: it
parses the entire command surface, builds manifests, then hands orchestration
off to the packaged Bash driver for installs. One binary owns all behaviour; the
shell wrapper is only a compatibility shim.

## Features

- Coordinates pacman and AUR helpers with explicit include/exclude filters.
- Generates a JSON manifest via `synsyu_core` for dry runs, reporting, and disk
  safety checks.
- Configurable logging level with time and size-based log retention policies.
- Validates download, build, and install footprint before updates, applying a
  configurable free-space buffer.
- Optional application updates for Flatpak and firmware (fwupd) with dedicated
  commands or opt-in flags that now also seed the manifest.
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
git clone https://github.com/Synavera-Discorporated/Syn-Syu.git
cd Syn-Syu
makepkg -sif
```

This installs:

- `/usr/bin/syn-syu` (wrapper that execs `synsyu_core`)
- `/usr/bin/synsyu_core` (Rust CLI + manifest builder)
- `/usr/lib/syn-syu/driver.sh` (Bash driver invoked by the CLI)
- `/usr/lib/syn-syu/*.sh` (helper libraries used by the driver)
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
- `aur.max_parallel_requests` / `aur.max_kib_per_sec` – control how many AUR
  RPC calls run concurrently and optionally throttle each request in KiB/s.
- `applications.flatpak` / `applications.fwupd` – defaults for including
  application/firmware updates in both manifest generation and `sync` (also
  exposed as commands and `--with-*` flags).
- `snapshots.*` – optional pre/post commands for snapshot integrations.
- `safety.disk_check` / `safety.disk_extra_margin_mb` – enable disk guards and
  define additional safety margin before installs proceed.

CLI flags such as `--config`, `--include`, `--exclude`, `--dry-run`,
`--no-aur`, `--no-repo`, and `--min-free-gb` override configuration on demand.

## Usage

The CLI accepts all commands via `syn-syu` (or `synsyu` symlink). Global flags
can be placed before or after the subcommand.

```bash
syn-syu sync --dry-run                    # Preview updates without applying
syn-syu update <pkg>...                   # Update specific packages
syn-syu group <name>                      # Update packages defined in groups.toml
syn-syu aur --include 'vscodium'          # AUR-only with include filter
syn-syu repo --batch 5                    # Repo-only with custom batch size
syn-syu check --json                      # Summarize manifest in JSON
syn-syu inspect <pkg>                     # Show manifest detail for a package
syn-syu core --with-flatpak --with-fwupd  # Rebuild manifest, recording app intent
syn-syu flatpak                           # Apply Flatpak updates
syn-syu fwupd                             # Apply firmware updates
syn-syu clean                             # Prune caches/orphans
```

Use `syn-syu --help` for the full command set and `syn-syu <command> --help` for
command-specific flags. The driver binary is auto-discovered from packaged
paths; set `SYN_SYU_DRIVER` to override if needed.

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

## License

Apache License 2.0. Arch packages use the common license from the licenses package. See your system’s common licenses directory or the official text: https://www.apache.org/licenses/LICENSE-2.0
