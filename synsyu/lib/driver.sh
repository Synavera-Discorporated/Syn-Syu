#!/usr/bin/env bash
#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/driver.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1
#------------------------------------------------------------
# Purpose:
#   Execute Syn-Syu orchestration flows after CLI parsing has
#   been handled by synsyu_core (Rust). This script mirrors
#   the original runtime wiring while accepting configuration
#   exclusively through environment variables and positional
#   command arguments.
#
# Security / Safety Notes:
#   - Performs no CLI parsing; expects sanitized inputs from
#     synsyu_core.
#   - Uses sudo only for pacman/cache operations as defined in
#     the existing module functions.
#------------------------------------------------------------

set -euo pipefail
IFS=$'\n\t'

SESSION_STAMP="$(date -u +"%Y-%m-%d_%H-%M-%S")"
readonly SESSION_STAMP

SCRIPT_PATH="$(realpath "$0")"
SCRIPT_DIR="$(dirname "$SCRIPT_PATH")"
readonly SCRIPT_DIR

LIB_DIR=""
LIB_SEARCH_PATHS=("$SCRIPT_DIR")
if [ -n "${SYNSYU_LIBDIR:-}" ]; then
  LIB_SEARCH_PATHS+=("$SYNSYU_LIBDIR")
fi
if [ -n "${SYN_SYU_LIBDIR:-}" ]; then
  LIB_SEARCH_PATHS+=("$SYN_SYU_LIBDIR")
fi
LIB_SEARCH_PATHS+=(
  "/usr/local/share/syn-syu/lib"
  "/usr/lib/syn-syu/lib"
  "$HOME/.local/share/syn-syu/lib"
  "/usr/local/share/synsyu/lib"
  "/usr/lib/synsyu/lib"
  "$HOME/.local/share/synsyu/lib"
)
for candidate in "${LIB_SEARCH_PATHS[@]}"; do
  if [ -f "$candidate/logging.sh" ]; then
    LIB_DIR="$candidate"
    break
  fi
done

if [ -z "$LIB_DIR" ]; then
  printf 'Syn-Syu driver cannot locate its library modules. Set SYN_SYU_LIBDIR (or legacy SYNSYU_LIBDIR) to the directory containing logging.sh.\n' >&2
  exit 120
fi

# shellcheck source=/dev/null
. "$LIB_DIR/logging.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/helpers.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/manifest.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/common.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/config.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/cli.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/disk.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/apps.sh"
# shellcheck source=/dev/null
. "$LIB_DIR/commands.sh"

REQUIRED_MODULES=("logging.sh" "helpers.sh" "manifest.sh" "common.sh" "config.sh" "cli.sh" "disk.sh" "apps.sh" "commands.sh")
for _mod in "${REQUIRED_MODULES[@]}"; do
  if [ ! -f "$LIB_DIR/$_mod" ]; then
    printf 'Syn-Syu driver missing module %s in %s. Please reinstall syn-syu to restore required libraries.\n' "$_mod" "$LIB_DIR" >&2
    exit 121
  fi
done

readonly SYN_CORE_BIN="${SYN_CORE_BIN:-$(command -v synsyu_core 2>/dev/null || echo /usr/bin/synsyu_core)}"
readonly DEFAULT_CONFIG_PATH="$HOME/.config/syn-syu/config.toml"
readonly DEFAULT_GROUPS_PATH="$HOME/.config/syn-syu/groups.toml"
readonly DEFAULT_MANIFEST="/tmp/syn-syu_manifest.json"

CONFIG_PATH="${SYNSYU_CONFIG_PATH:-$DEFAULT_CONFIG_PATH}"
GROUPS_PATH="${SYNSYU_GROUPS_PATH:-$DEFAULT_GROUPS_PATH}"
SYN_MANIFEST_PATH="${SYNSYU_MANIFEST_PATH:-$DEFAULT_MANIFEST}"
LOG_VERBOSE="${SYNSYU_VERBOSE:-0}"
LOG_LEVEL="info"
LOG_RETENTION_DAYS=0
LOG_RETENTION_SIZE_MB=0
REBUILD_MANIFEST="${SYNSYU_REBUILD:-0}"
DRY_RUN="${SYNSYU_DRY_RUN:-0}"
NO_AUR="${SYNSYU_NO_AUR:-0}"
NO_REPO="${SYNSYU_NO_REPO:-0}"
NO_CONFIRM=1
if [ "${SYNSYU_CONFIRM:-0}" = "1" ]; then
  NO_CONFIRM=0
fi
QUIET="${SYNSYU_QUIET:-0}"
JSON_OUTPUT="${SYNSYU_JSON:-0}"
declare -a INCLUDE_PATTERNS=()
declare -a EXCLUDE_PATTERNS=()
AUR_HELPER="${SYNSYU_HELPER:-}"
BATCH_SIZE="${SYNSYU_BATCH_SIZE:-10}"
SNAPSHOTS_ENABLED=0
SNAPSHOT_PRE_CMD=""
SNAPSHOT_POST_CMD=""
SNAPSHOT_REQUIRE_SUCCESS=0
MIN_FREE_SPACE_BYTES=$((2 * 1024 * 1024 * 1024))
MIN_FREE_SPACE_OVERRIDE_BYTES="${SYNSYU_MIN_FREE_BYTES:-}"
DISK_CHECK=1
DISK_MARGIN_MB="${SYNSYU_DISK_MARGIN_MB:-0}"
SPACE_CHECK_PATH="/"
CLEAN_KEEP_VERSIONS=2
CLEAN_REMOVE_ORPHANS=0
CLEAN_CHECK_PACNEW=1
APPLICATIONS_FLATPAK=0
APPLICATIONS_FWUPD=0
APPLICATIONS_FLATPAK_CLI=""
APPLICATIONS_FWUPD_CLI=""
declare -a FAILED_UPDATES=()
COMMAND="${1:-sync}"
COMMAND_ARGS=("${@:2}")
HELPER_PRIORITY=()

trap 'handle_exit $?' EXIT
trap 'handle_interrupt' INT

#--- hydrate_arrays_from_env
hydrate_arrays_from_env() {
  local raw_include raw_exclude
  raw_include="${SYNSYU_INCLUDE:-}"
  raw_exclude="${SYNSYU_EXCLUDE:-}"
  if [ -n "$raw_include" ]; then
    # Preserve regex spacing by splitting on newlines only.
    while IFS= read -r line; do
      [ -z "$line" ] && continue
      INCLUDE_PATTERNS+=("$line")
    done <<<"$raw_include"
  fi
  if [ -n "$raw_exclude" ]; then
    while IFS= read -r line; do
      [ -z "$line" ] && continue
      EXCLUDE_PATTERNS+=("$line")
    done <<<"$raw_exclude"
  fi
}

#--- apply_cli_overrides
apply_cli_overrides() {
  if [ -n "${SYNSYU_GROUPS_PATH:-}" ]; then
    GROUPS_PATH="$SYNSYU_GROUPS_PATH"
  fi
  if [ -n "${SYNSYU_BATCH_SIZE:-}" ]; then
    BATCH_SIZE="$SYNSYU_BATCH_SIZE"
  fi
  if [ -n "${SYNSYU_MIN_FREE_GB:-}" ]; then
    MIN_FREE_SPACE_BYTES="$(convert_gb_to_bytes "$SYNSYU_MIN_FREE_GB")"
    MIN_FREE_SPACE_OVERRIDE_BYTES="$MIN_FREE_SPACE_BYTES"
  elif [ -n "$MIN_FREE_SPACE_OVERRIDE_BYTES" ]; then
    MIN_FREE_SPACE_BYTES="$MIN_FREE_SPACE_OVERRIDE_BYTES"
  fi
  if [ "${SYNSYU_WITH_FLATPAK:-}" = "1" ]; then
    APPLICATIONS_FLATPAK=1
    APPLICATIONS_FLATPAK_CLI=1
  elif [ "${SYNSYU_WITH_FLATPAK:-}" = "0" ]; then
    APPLICATIONS_FLATPAK=0
    APPLICATIONS_FLATPAK_CLI=0
  fi
  if [ "${SYNSYU_WITH_FWUPD:-}" = "1" ]; then
    APPLICATIONS_FWUPD=1
    APPLICATIONS_FWUPD_CLI=1
  elif [ "${SYNSYU_WITH_FWUPD:-}" = "0" ]; then
    APPLICATIONS_FWUPD=0
    APPLICATIONS_FWUPD_CLI=0
  fi
}

#--- apply_application_flags
# Align application toggles with CLI overrides or manifest state.
apply_application_flags() {
  # If CLI explicitly set the flags, keep them; otherwise hydrate from manifest.
  if [ -n "${APPLICATIONS_FLATPAK_CLI:-}" ] || [ -n "${APPLICATIONS_FWUPD_CLI:-}" ]; then
    return 0
  fi
  manifest_apply_application_flags
}

#--- main
main() {
  hydrate_arrays_from_env
  ensure_prerequisites
  apply_cli_overrides
  load_config
  log_init

  if [ "$NO_AUR" = "1" ] && [ "$NO_REPO" = "1" ]; then
    log_error "E103" "Cannot disable both repo and AUR operations"
    exit 103
  fi

  detect_helpers
  apply_application_flags
  dispatch_command
}

main "$@"
