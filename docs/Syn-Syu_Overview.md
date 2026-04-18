# Syn-Syu Overview

Syn-Syu is Synavera's conscious orchestrator for Arch Linux systems. It couples a
Rust backend (`synsyu_core`) with a Bash front-end (`syn-syu`) to coordinate
updates across official repositories and the Arch User Repository.

## Components

- **synsyu_core** – Rust binary that enumerates installed packages and emits a
  structured JSON manifest at `~/.config/syn-syu/manifest.json` (configurable)
  describing the current state: what is installed, which source it came from,
  direct AUR RPC source classification, and bounded network state such as
  ranked pacman mirror candidates.
- **syn-syu** – Bash CLI that parses the manifest, builds update plans, selects
  helpers, applies source-aware bounded acquisition policy, and executes updates
  per user intent. Pacman repos, AUR RPC, and AUR helper acquisition use
  different recovery strategies because their transports and failure modes are
  different. Logging follows the Synavera Script Etiquette and is kept under
  `~/.local/share/syn-syu/` by default.
- **syn-syu plan** – Builds an update plan from fresh sources (pacman, AUR,
  Flatpak, fwupd when enabled), writes it to `~/.config/syn-syu/plan.json`, and
  prints a concise summary (with optional strict/JSON modes).

## Manifest Schema

`synsyu_core` writes a JSON document with the following shape. This manifest is
the user-owned source of truth for the desired system state and is persisted in
the Syn-Syu config directory (not under `/tmp`).

```json
{
  "metadata": {
    "generated_at": "2024-11-04T18:41:00Z",
    "generated_by": "synsyu_core",
    "total_packages": 243,
    "pacman_packages": 156,
    "aur_packages": 87,
    "local_packages": 0,
    "unknown_packages": 0
  },
  "packages": {
    "bash": {
      "installed_version": "5.2.32-1",
      "repository": "core",
      "source": "PACMAN",
      "installed_size": 20545536,
      "install_date": "2024-11-01T12:00:00Z",
      "validated_by": "Signature"
    }
  },
  "network": {
    "mirrors": {
      "enabled": true,
      "status": "ready",
      "source": "mirrorlist",
      "source_path": "/etc/pacman.d/mirrorlist",
      "max_failovers": 2,
      "candidate_count": 6,
      "usable_count": 3,
      "candidates": [
        {
          "rank": 1,
          "server": "https://mirror.example/archlinux/$repo/os/$arch",
          "status": "ready",
          "outcome": "ready",
          "freshness": "fresh",
          "usable": true,
          "latency_ms": 94
        }
      ]
    }
  }
}
```

The Bash orchestrator consumes the manifest as the authoritative record of what
is installed; update planning and disk checks live elsewhere.

When Flatpak or firmware updates are requested, the manifest also includes an
`applications` block capturing the chosen sources (`flatpak` / `fwupd`), whether
they were enabled during manifest generation, and any discovered updates.

When mirror probing is enabled, `synsyu_core` also records `network.mirrors`.
The Bash layer treats that state as advisory input for repo acquisition only.
It does not rewrite `/etc/pacman.d/mirrorlist`; for each attempt it creates a
temporary pacman config that points the existing mirrorlist include at a
one-line temporary mirrorlist. Pacman still performs repository, package,
signature, dependency, and transaction validation.

Operationally: when repo downloads fail because a mirror is unreachable, slow,
or stale, Syn-Syu can try a bounded number of alternate mirrors. It does not
retry integrity or trust failures. It does not modify your system mirrorlist
permanently.

For AUR operations, Syn-Syu does not pretend that every failure is a mirror
problem. Direct AUR RPC calls in `synsyu_core` use bounded retry with the
configured fixed delay for transient HTTP or network failures. AUR helper
execution in Bash may retry clearly transient helper fetch/clone/download
failures. PKGBUILD, checksum, signature, dependency, conflict, and build
failures stop immediately.

## CLI Sketch

| Command | Purpose |
| --- | --- |
| `syn-syu core` | Regenerate the manifest using `synsyu_core`. |
| `syn-syu sync` | Update all packages with available upgrades. |
| `syn-syu plan` | Build an update plan JSON (no installs). |
| `syn-syu aur` | Apply only AUR updates (repo upgrades skipped). |
| `syn-syu repo` | Apply only repo updates. |
| `syn-syu flatpak` | Apply Flatpak updates (or list when `--dry-run`). |
| `syn-syu fwupd` | Apply firmware updates via fwupdmgr (or list when `--dry-run`). |
| `syn-syu apps` | Run Flatpak and fwupd update flows together. |
| `syn-syu update brave-bin` | Update selected packages. |
| `syn-syu group development` | Update packages in a named group from `groups.toml`. |
| `syn-syu inspect brave-bin` | Show manifest detail for a package. |
| `syn-syu check` | Print manifest summary without applying changes. |
| `syn-syu clean` | Prune caches/orphans according to policy. |
| `syn-syu export` | Export repo/AUR package lists for replication. |
| `syn-syu helpers` | List detected AUR helpers. |
| `syn-syu helper <name>` | Set helper for this session (persist with helpers.sh). |
| `syn-syu self-update` | Clone GitHub and reinstall Syn-Syu via the repo PKGBUILD. |
| `syn-syu mirrors` | Show ranked pacman mirror candidates recorded in the manifest. |
| `syn-syu acquisition` | Show source-aware bounded acquisition policies by channel. |
| `syn-syu config` | Show config path info. |
| `syn-syu groups-edit` | Open groups file in `$EDITOR`. |
| `syn-syu log` | Show log directory/retention info. |
| `syn-syu version` / `syn-syu help` | Version and usage. |

Use `syn-syu --help` for the full flag list.

### Power-user Flags

- `--json` – machine-readable output where supported, including `check`,
  `inspect`, `mirrors`, and `acquisition`.
- `--quiet`/`-q` – suppress non-essential output; logs still written.
- `--verbose`/`-v` – stream logs to stderr.
- `--confirm`/`-c` and `--noconfirm`/`-nc` – toggle interactive confirmations
  passed to helpers and pacman (default is non-interactive).
- `--helper <name>` – force a specific AUR helper instead of auto-detection.
- `--include <regex>` / `--exclude <regex>` – filter packages by name during
  `sync` (both flags repeatable; evaluated as Bash regex).
- `--batch <N>` – repo package batch size; defaults to `core.batch_size` from
  config or `10`.
- `--mirrors` / `--no-mirrors` – enable or disable repo mirror failover for the
  current run without changing config.
- `--with-flatpak`/`-w-fp` and `--with-fwupd`/`-w-fw` – opt into Flatpak and
  firmware updates during manifest generation and `sync` (also available as
  standalone commands).
- `plan` flags: `--json`, `--strict`, `--offline`, `--no-aur`, `--no-repo`,
  `--with-flatpak`, `--with-fwupd`, and `--plan/--manifest` path overrides.

### Safety & Maintenance Additions

- **Snapshots / Rollback hooks** – optional pre/post snapshot commands (Snapper,
  Timeshift, custom). Enable via `[snapshots]` in the config. When enabled,
  Syn-Syu runs the `pre_command` before updates and `post_command` afterwards,
  aborting if the pre-command fails when `require_success = true`.
- **Disk space guard** – when `[safety].disk_check` is true, Syn-Syu sums the
  download sizes from the manifest and aborts the update if available space on
  `/` is below the required total plus the configured margin.
- **pacnew detection** – after successful updates, Syn-Syu scans for
  `.pacnew/.pacsave` files (using `pacdiff` when available) and surfaces them in
  logs/console so you can merge configuration changes promptly.
- **Source-aware acquisition policy** – Syn-Syu treats each source/channel
  separately. Pacman repo downloads use mirror failover. AUR RPC uses bounded
  HTTP/network retry in Rust. AUR helper execution uses bounded retry only for
  transient helper acquisition failures. PKGBUILD upstream source fallback,
  Flatpak retry policy, and fwupd retry policy are explicit future/tool-owned
  channels, not hidden pacman-style behavior.
- **Mirror-aware repo acquisition** – when enabled, `synsyu_core` reads active
  pacman `Server =` entries from the configured mirrorlist or from explicit
  `[mirrors].servers`, probes a small bounded set, ranks usable mirrors by
  freshness first and response latency second, and marks mirrors stale when
  `lastsync` is older than `max_sync_age_hours`. Mirrors with unknown freshness
  remain usable but sort after known-fresh mirrors. The Bash layer retries repo
  batches against at most `max_failovers + 1` usable mirrors and stops when the
  budget is exhausted. It only retries retrieval-style failures such as failed
  downloads, DNS errors, timeouts, and connection resets. Signature, integrity,
  keyring, dependency, lock, disk, and conflict errors are not mirror-retryable
  and remain final. Status output includes a compact outcome code such as
  `ready`, `stale`, `timeout`, `connect_failed`, or `http_error` so a failed
  candidate can be diagnosed without reading the full probe message first.
- **AUR bounded acquisition** – direct AUR RPC calls retry transient HTTP
  failures such as 429, timeout, and 5xx responses within the configured retry
  budget. AUR helper execution retries only clear transport/fetch failures such
  as DNS failure, timeout, TLS/connect failure, or transient Git transport
  errors. PKGBUILD, checksum, signature, dependency, conflict, and build
  failures are terminal and are not retried by Syn-Syu.
- **Application updates** – opt into Flatpak and firmware (fwupd) updates via
  config (`[applications]`) or on-demand commands `syn-syu flatpak` /
  `syn-syu fwupd`, or include them in both the manifest and `sync` with
  `--with-flatpak`/`--with-fwupd`.
- **Enhanced clean** – `syn-syu clean` now leverages `paccache` to retain the
  most recent `keep_versions` package versions, optionally removes orphaned
  dependencies, and trims stale installer logs.
- **Export packages** – `syn-syu export [--json|--plain]` dumps the explicitly
  installed repo/AUR packages, making it easy to replicate an environment or
  commit your package set to version control.
- **Self-update from GitHub** – `syn-syu self-update` clones the upstream GitHub
  repository into a temporary build directory and runs `makepkg -sif` against
  the checked-out `PKGBUILD`. This is intentionally package-manager aware:
  Syn-Syu does not overwrite `/usr/bin` or `/usr/lib/syn-syu` directly. Use
  `--dry-run` to preview, or set `SYN_SYU_SELF_UPDATE_REPO` /
  `SYN_SYU_SELF_UPDATE_REF` for testing a fork, branch, or tag.

## Configuration Files

- `~/.config/syn-syu/config.toml` – Controls core behavior. See
  `examples/config.toml` for defaults.
- `~/.config/syn-syu/groups.toml` – Optional group definitions used by the
  `group` command. Current format expects top-level arrays:

```toml
development = ["rust", "rust-analyzer", "cargo"]
media = ["mpv", "vlc"]
```

Key config sections beyond the basics:

```toml
[aur]
# Limit concurrent AUR RPC calls and optionally throttle each request (KiB/s)
max_parallel_requests = 4
max_kib_per_sec = 0

[applications]
flatpak = false
fwupd = false

[mirrors]
enabled = true
mirrorlist_path = "/etc/pacman.d/mirrorlist"
pacman_conf_path = "/etc/pacman.conf"
probe = true
probe_timeout_seconds = 3
max_candidates = 6
max_failovers = 2
retry_delay_seconds = 2
max_sync_age_hours = 48
cache_ttl_hours = 168
# cache_path = "~/.cache/syn-syu/mirror-probes.json"
# Optional explicit candidates; when non-empty these replace mirrorlist discovery.
# servers = ["https://mirror.example/archlinux/$repo/os/$arch"]

[acquisition.aur_rpc]
enabled = true
max_retries = 2
retry_delay_seconds = 2

[acquisition.aur_helper]
enabled = true
max_retries = 1
retry_delay_seconds = 3

[snapshots]
enabled = false
pre_command = "sudo snapper create --description 'Syn-Syu pre-update'"
post_command = ""
require_success = false

[safety]
disk_check = true
disk_extra_margin_mb = 200

[clean]
keep_versions = 2
remove_orphans = false
check_pacnew = true

[space]
min_free_gb = 100
mode = "warn"
```

The `[applications]` section controls the default inclusion of Flatpak and
firmware updates when building the manifest and during subsequent `sync`
operations; CLI flags `--with-flatpak` and `--with-fwupd` override the defaults.
Flatpak metadata is appended by the Bash layer after `synsyu_core` runs; the
core binary itself only supports `--with-fwupd`.

The `[mirrors]` section controls repo mirror failover. `enabled = true` records
mirror state in the manifest and allows the Bash layer to cycle official repo
package acquisition through usable candidates. `max_candidates` limits how many
servers are considered from the source list. `probe_timeout_seconds` limits
each HTTP probe. `max_sync_age_hours` controls freshness: stale mirrors are
excluded, known-fresh mirrors rank ahead of freshness-unknown mirrors, and
latency is used within those groups. `max_failovers` means additional mirrors
after the first attempt, so the maximum attempts for a repo batch are
`max_failovers + 1`, further capped by the number of usable candidates.
`retry_delay_seconds` inserts a short pause between retryable failures.
`cache_ttl_hours` keeps last-known probe outcomes for a bounded time so the next
manifest rebuild can choose better first candidates before it probes again. The
cache is only an ordering hint; fresh probe results still replace cached results,
and retry bounds do not change.

This feature is aimed at standard Arch-style repository layouts where official
repos share the configured mirrorlist include and mirror URLs contain `$repo`
and `$arch`. Syn-Syu replaces matching `Include = mirrorlist_path` lines only in
a temporary pacman config for the current attempt. Custom repositories that use
the same include path should be reviewed before enabling mirror failover. This
feature does not guarantee successful downloads whenever internet access exists;
it only gives Syn-Syu a bounded way to move past clearly unreliable mirrors
without hiding serious pacman errors.

The `[acquisition.aur_rpc]` section controls direct AUR RPC retry used by the
Rust backend for source classification and state gathering. `max_retries` means
additional tries after the first request, and `retry_delay_seconds` is a fixed
pause between retryable failures. `[acquisition.aur_rpc].max_retries` has
precedence when set; legacy `[aur].max_retries` is used only when the new
acquisition key is absent. `synsyu_core config` and `syn-syu acquisition` show
the effective value. Retryable conditions are transient HTTP/network failures,
not malformed package metadata or local policy failures.

The `[acquisition.aur_helper]` section controls Bash-layer retry around helper
execution for AUR packages. It is deliberately conservative: Syn-Syu retries
helper transport/fetch failures, but stops immediately for PKGBUILD,
dependency, checksum, signature, conflict, and build failures. PKGBUILD
upstream source fallback is a separate future problem because safe handling
depends on makepkg/helper semantics and upstream source arrays, not pacman
mirrors.

The `[space]` section defines `min_free_gb`, a buffer that must remain free on
disk before updates proceed, and `mode`, which controls behaviour when the
buffer is not met. `mode = "warn"` (default) emits a warning; `mode = "enforce"`
fails the plan when the buffer is below the configured threshold. The
orchestrator also honours `disk_extra_margin_mb` for additional breathing room.

## Logging

Syn-Syu writes append-only logs to `~/.local/share/syn-syu/<timestamp>.log` and
produces companion `.hash` files containing SHA-256 digests for audit chaining.
Log entries follow the pattern `YYYY-MM-DDTHH:MM:SSZ [LEVEL] [CODE] message`.

## Build & Install

Preferred workflow (from the repository root):

```bash
makepkg -sif
```

This builds `synsyu_core`, installs the `syn-syu` orchestrator, and drops
supporting files beneath `/usr/bin`, `/usr/lib/syn-syu/`, and
`/usr/share/syn-syu/`. Copy the example configuration if you want to tweak
defaults:

```bash
mkdir -p ~/.config/syn-syu
cp /usr/share/syn-syu/examples/config.toml ~/.config/syn-syu/config.toml
```

### Installer Wizard

For a guided experience run the installer from the repository root:

```bash
./tools/install_syn-syu.sh
# or non-interactively with defaults
./tools/install_syn-syu.sh --mode guided --policy overwrite --yes
```

Mode options:

- **Guided setup** handles dependency checks, builds the Rust binary, installs
  both executables into `/usr/local/bin`, places the Bash library modules under
  `/usr/local/share/syn-syu`, and copies `config.toml`.
- **It's my system (advanced)** lets you override the install prefix, adjust the
  library directory, opt out of automatic dependency installation, and create
  custom config/group files.

If you install the libraries to a non-standard location, export
`SYN_SYU_LIBDIR=/path/to/lib` (legacy: `SYNSYU_LIBDIR`) so the `syn-syu` CLI can source its modules.

Installer flags:
- `--mode <guided|advanced>` choose installer mode without a prompt
- `--policy <overwrite|backup|skip>` resolve existing files automatically
- `--yes` run non-interactively where safe (defaults policy to `overwrite` if unspecified)
- `--no-sudo` avoid sudo; may skip privileged paths

### Uninstall

Run the uninstaller from the repository root to cleanly remove binaries, library
modules, and optionally user data (config/logs):

```bash
./tools/uninstall_syn-syu.sh       # interactive
./tools/uninstall_syn-syu.sh --dry-run   # preview actions
```

If you installed `synsyu_core` via `cargo install`, remove it with
`cargo uninstall synsyu_core` or allow the uninstaller to delete the file in
`~/.cargo/bin/`.

For a formal description of Syn-Syu’s capabilities and safety guarantees, see
`docs/synspek.yaml` (capabilities) and `docs/synspek_checks.yaml` (test charter).

## Roadmap Hooks

The codebase includes hooks for future enhancements:

- Placeholder traits (`future.rs`) for multi-core vercmp and plugin systems.
- `manifest::ManifestEntry` stubs for changelog notes.
- Bash scaffolding for helper prioritisation and dry-run flows.

These stubs mark integration points for Syn-Syu v3 without impacting current
stability.
