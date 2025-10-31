# Syn-Syu Overview

Syn-Syu is Synavera's conscious orchestrator for Arch Linux systems. It couples a
Rust backend (`synsyu_core`) with a Bash front-end (`syn-syu`) to coordinate
updates across official repositories and the Arch User Repository.

## Components

- **synsyu_core** – Rust binary that enumerates installed packages, queries repo
  metadata via `pacman`, and consults the AUR RPC API. It emits a structured
  JSON manifest at `/tmp/syn-syu_manifest.json` (configurable) describing the
  freshest source for every package.
- **syn-syu** – Bash CLI that parses the manifest, selects helpers, and executes
  updates per user intent. Logging follows the Synavera Script Etiquette and is
  kept under `~/.local/share/syn-syu/` by default.

## Manifest Schema

`synsyu_core` writes a JSON document with the following shape:

```json
{
  "metadata": {
    "generated_at": "2024-11-04T18:41:00Z",
    "generated_by": "synsyu_core",
    "total_packages": 243,
    "repo_candidates": 156,
    "aur_candidates": 87,
    "updates_available": 12
  },
  "packages": {
    "bash": {
      "installed_version": "5.2.32-1",
      "version_repo": "5.2.32-1",
      "version_aur": null,
      "newer_version": "5.2.32-1",
      "source": "PACMAN",
      "update_available": false,
      "notes": null,
      "download_size_repo": 5619712,
      "installed_size_repo": 20545536,
      "download_size_selected": null,
      "installed_size_selected": null
    }
  }
}
```

The Bash orchestrator relies on the `source` and `update_available` fields to
route packages through `pacman` or the preferred AUR helper.

## CLI Sketch

| Command | Purpose |
| --- | --- |
| `syn-syu core` | Regenerate the manifest using `synsyu_core`. |
| `syn-syu sync` | Update all packages with available upgrades. |
| `syn-syu aur` | Apply only AUR updates (repo upgrades skipped). |
| `syn-syu repo` | Apply only repo updates. |
| `syn-syu update brave-bin` | Update selected packages. |
| `syn-syu group development` | Update packages in a named group from `groups.toml`. |
| `syn-syu inspect brave-bin` | Show manifest detail for a package. |
| `syn-syu check` | Print manifest summary without applying changes. |
| `syn-syu clean` | Prune caches/orphans according to policy. |
| `syn-syu export` | Export repo/AUR package lists for replication. |

Use `syn-syu --help` for the full flag list.

### Power-user Flags

- `--json` – machine-readable output for `check` and `inspect`.
- `--quiet`/`-q` – suppress non-essential output; logs still written.
- `--confirm`/`--noconfirm` – toggle interactive confirmations passed to helpers
  and pacman (default is non-interactive).
- `--helper <name>` – force a specific AUR helper instead of auto-detection.
- `--include <regex>` / `--exclude <regex>` – filter packages by name during
  `sync` (both flags repeatable; evaluated as Bash regex).
- `--batch <N>` – repo package batch size; defaults to `core.batch_size` from
  config or `10`.

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
- **Enhanced clean** – `syn-syu clean` now leverages `paccache` to retain the
  most recent `keep_versions` package versions, optionally removes orphaned
  dependencies, and trims stale installer logs.
- **Export packages** – `syn-syu export [--json|--plain]` dumps the explicitly
  installed repo/AUR packages, making it easy to replicate an environment or
  commit your package set to version control.

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
```

The `[space]` section defines `min_free_gb`, a buffer that must remain free on
disk before updates proceed. The orchestrator also honours `disk_extra_margin_mb`
for additional breathing room.

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

## Roadmap Hooks

The codebase includes hooks for future enhancements:

- Placeholder traits (`future.rs`) for multi-core vercmp and plugin systems.
- `manifest::ManifestEntry` stubs for changelog notes.
- Bash scaffolding for helper prioritisation and dry-run flows.

These stubs mark integration points for Syn-Syu v3 without impacting current
stability.
