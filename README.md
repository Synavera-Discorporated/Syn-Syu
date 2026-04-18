# Syn-Syu

Syn-Syu is Synavera's conscious successor to `pacman -Syu`. A Bash orchestrator
(`syn-syu`) works together with a Rust backend (`synsyu_core`) to build a
manifest of the current system state (installed packages with source), build
update plans from fresh sources, apply repo/AUR/app/firmware updates
selectively, and produce detailed logs for review.

Syn-Syu is source-aware and tool-aware. Different channels may use different
bounded recovery strategies; pacman mirror failover is not treated as a generic
solution for AUR helpers, PKGBUILD sources, Flatpak, or fwupd.

## Features

- Coordinates pacman and AUR helpers with explicit include/exclude filters.
- Generates a JSON manifest via `synsyu_core` as the authoritative state, plus
  update plans from fresh sources (pacman, AUR, optional Flatpak/fwupd).
- Captures install metadata (install date, validation root, truncated package
  or firmware hashes) for audit-friendly manifests and plans without leaking
  full fingerprints.
- Configurable logging level with time and size-based log retention policies.
- Validates download, build, and install footprint before updates, applying a
  configurable free-space buffer.
- Applies source-aware bounded acquisition policy: pacman repos use mirror
  failover, AUR RPC/helper paths use transient retry where safe, and other
  channels remain tool-owned unless explicitly implemented.
- Optional application updates for Flatpak and firmware (fwupd) with dedicated
  commands or opt-in flags that now also seed the manifest and plan.
- Supports guided or advanced install workflows through optional tooling.
- Logs every action with timestamped entries to simplify auditing.
- Provides commands for sync, targeted updates, group operations, cleaning, and
  inspection, planning, and helper selection.

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
file is provided at `examples/config.toml`. You can view the merged
configuration with `synsyu_core config [--json]`; log retention pruning is
available through `synsyu_core logs --prune`. Key options include:

- `core.manifest_path` – output path for the generated manifest.
- `space.min_free_gb` – reserved buffer that must remain free after updates.
- `space.mode` – `"warn"` (default) logs a warning if free space is below the buffer; `"enforce"` fails the plan when the buffer is not met.
- `logging.level` – choose from `debug`, `info`, `warn`, `error`, or `none`.
- `logging.retention_days` / `logging.retention_megabytes` – prune old logs by
  age or aggregate size.
- `logging.directory` – explicit log location (falls back to `core.log_directory`
  for compatibility).
- `helpers.priority` – ordered list of AUR helpers to try.
- `aur.max_parallel_requests` / `aur.max_kib_per_sec` – control how many AUR
  RPC calls run concurrently and optionally throttle each request in KiB/s.
- `mirrors.enabled` – enables mirror-aware repo acquisition failover.
- `mirrors.mirrorlist_path` / `mirrors.pacman_conf_path` – inputs used to read
  mirror candidates and build temporary pacman configs without editing system
  files.
- `mirrors.probe`, `mirrors.probe_timeout_seconds`, `mirrors.max_candidates`,
  and `mirrors.max_sync_age_hours` – bound mirror probing and stale-mirror
  filtering.
- `mirrors.cache_ttl_hours` / `mirrors.cache_path` – keep last-known probe
  outcomes so the next rebuild can choose better first candidates before it
  probes again.
- `mirrors.max_failovers` / `mirrors.retry_delay_seconds` – bound repo
  acquisition retries; attempts are limited to `max_failovers + 1`.
- `acquisition.aur_rpc.*` – bounded transient retry for direct AUR RPC calls
  used by `synsyu_core`.
- `acquisition.aur_helper.*` – bounded transient retry around AUR helper
  acquisition failures; build, dependency, signature, checksum, and PKGBUILD
  failures are terminal.
- `applications.flatpak` / `applications.fwupd` – defaults for including
  application/firmware updates in both manifest generation and `sync` (also
  exposed as commands and `--with-*` flags).
- `snapshots.*` – optional pre/post commands for snapshot integrations.
- `safety.disk_check` / `safety.disk_extra_margin_mb` – enable disk guards and
  define additional safety margin before installs proceed.

CLI flags such as `--config`, `--include`, `--exclude`, `--dry-run`,
`--no-aur`, `--no-repo`, `--mirrors`, `--no-mirrors`, and `--min-free-gb`
override configuration on demand.

Mirror failover does not replace pacman trust, signature, dependency, or
transaction checks. Syn-Syu only cycles mirrors after retrieval-style failures
such as timeouts or failed downloads. Signature, integrity, keyring, dependency,
lock, disk, and conflict errors stop the mirror loop and are surfaced as final
failures. Known-fresh mirrors are ranked ahead of mirrors whose freshness cannot
be checked; stale mirrors are excluded from failover candidates. Syn-Syu creates
temporary pacman config files for each attempt and does not permanently modify
your system mirrorlist. Mirror status output includes a short outcome code such
as `ready`, `stale`, `timeout`, `connect_failed`, or `http_error`.

Syn-Syu is source-aware and tool-aware. Pacman mirror failover is pacman-specific.
AUR RPC retry covers transient HTTP/network failures in Rust state generation.
AUR helper retry covers clearly transient helper fetch/clone/download failures
in Bash execution. PKGBUILD upstream source fallback, Flatpak retry policy, and
fwupd retry policy are not implemented as Syn-Syu strategies yet; those channels
remain separate future extension points or tool-owned behavior.
For AUR RPC, `[acquisition.aur_rpc].max_retries` wins when set; legacy
`[aur].max_retries` is used only when the new acquisition key is absent.

## Usage

Common entry points (both `syn-syu` and `synsyu` work):

```bash
synsyu            # Sync repo metadata, rebuild manifest, prompt for updates
synsyu --dry-run  # Preview updates without making changes
synsyu plan       # Build an update plan JSON (no installs)
synsyu update <pkg>...
synsyu group <name>
synsyu clean
synsyu aur        # AUR-only updates
synsyu repo       # Repo-only updates
synsyu apps       # Run flatpak + fwupd update flows together
synsyu core --with-flatpak --with-fwupd  # record Flatpak/firmware intent in manifest
synsyu flatpak    # Apply Flatpak updates (or dry-run list)
synsyu fwupd      # Apply firmware updates via fwupdmgr
synsyu sync --with-flatpak --with-fwupd  # include app/firmware updates in one sweep
synsyu helpers    # List detected AUR helpers
synsyu helper <name>  # Set helper for this session (or persist with helpers.sh)
synsyu self-update # Clone GitHub and reinstall Syn-Syu via makepkg
synsyu mirrors    # Show ranked mirror candidates recorded in the manifest
synsyu acquisition # Show source-aware bounded acquisition policy
synsyu config     # Show config path info
synsyu groups-edit  # Open groups.toml in $EDITOR
synsyu log        # Show log directory/retention info
synsyu version    # Show version/build info
synsyu help       # Usage help
```

Common aliases are available for frequent flags: `-v` for `--verbose`, `-c`
for `--confirm`, `-nc` for `--noconfirm`, `-w-fp` for `--with-flatpak`, and
`-w-fw` for `--with-fwupd`.

Use `syn-syu --help` for the full command set, including manifest inspection,
logging, and AUR-only or repo-only operations.

Until Syn-Syu is published through a package repository, `syn-syu self-update`
can update Syn-Syu from GitHub. It clones
`https://github.com/Synavera-Discorporated/Syn-Syu.git` into a temporary build
directory and runs the repository `PKGBUILD` with `makepkg -sif`, preserving
pacman ownership instead of overwriting installed files directly. Use
`syn-syu self-update --dry-run` to preview the operation. Advanced testers can
override the source with `SYN_SYU_SELF_UPDATE_REPO` and
`SYN_SYU_SELF_UPDATE_REF`.

The Rust binary is also available directly:

```bash
synsyu_core --manifest ~/.config/syn-syu/manifest.json --with-fwupd --offline
synsyu_core plan --manifest ~/.config/syn-syu/manifest.json --plan ~/.config/syn-syu/plan.json --json --strict
synsyu_core mirrors --no-probe --json
```

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

For a formal description of Syn-Syu’s capabilities and safety guarantees, see
`docs/synspek.yaml` (capabilities) and `docs/synspek_checks.yaml` (test charter).

## License

Apache License 2.0. Arch packages use the common license from the licenses package. See your system’s common licenses directory or the official text: https://www.apache.org/licenses/LICENSE-2.0
