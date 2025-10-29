#!/usr/bin/env bash
#============================================================
# Synavera Project: Syn-Syu
# Module: tools/uninstall_syn-syu.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Interactive uninstaller to remove Syn-Syu and Syn-Syu-Core
#   binaries, supporting library modules, and optional user
#   data (logs/config/manifests).
#
# Security / Safety Notes:
#   - Prompts before deleting any files.
#   - Uses sudo only when required to remove privileged paths.
#   - Dry-run mode available to preview actions.
#
# Dependencies:
#   bash, rm, install, sha256sum, command -v
#
# Operational Scope:
#   Safe cleanup of Syn-Syu artifacts regardless of install
#   prefix (detects typical locations and PATH-resolved files).
#
# Revision History:
#   2025-10-28 COD  Created uninstaller wizard.
#------------------------------------------------------------
# SSE Principles Observed:
#   - set -euo pipefail; explicit confirmations
#   - Timestamped log with SHA-256 integrity hash
#   - No destructive actions without operator consent
#============================================================

set -euo pipefail
IFS=$'\n\t'

SESSION_STAMP="$(date -u +"%Y-%m-%d_%H-%M-%S")"
readonly SESSION_STAMP

LOG_DIR="${HOME}/.local/share/syn-syu/install"
LOG_PATH="${LOG_DIR}/uninstall_${SESSION_STAMP}.log"
LOG_VERBOSE=1
DRY_RUN=0
SUDO_CMD="sudo"

#--- log_init
log_init() {
  if ! mkdir -p "$LOG_DIR" 2>/dev/null; then
    LOG_DIR="/tmp/syn-syu/install"
    mkdir -p "$LOG_DIR" 2>/dev/null || LOG_DIR="/tmp"
    LOG_PATH="${LOG_DIR}/uninstall_${SESSION_STAMP}.log"
  fi
  : >"$LOG_PATH"
}

#--- log_event
log_event() {
  if [ $# -lt 3 ]; then
    return 1
  fi
  local level="$1" code="$2" message="$3"
  local timestamp
  timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  printf '%s [%s] [%s] %s\n' "$timestamp" "$level" "$code" "$message" >>"$LOG_PATH"
  if [ "$LOG_VERBOSE" = "1" ] || [ "$level" = "ERROR" ] || [ "$level" = "WARN" ]; then
    printf '%s [%s] %s\n' "$timestamp" "$level" "$message"
  fi
}

log_info() { log_event "INFO" "$1" "$2"; }
log_warn() { log_event "WARN" "$1" "$2"; }
log_error() { log_event "ERROR" "$1" "$2"; }

log_init
trap '[ -s "$LOG_PATH" ] && sha256sum "$LOG_PATH" >"${LOG_PATH}.hash"' EXIT

#--- usage
usage() {
  cat <<'EOF'
Syn-Syu Uninstaller
Usage: uninstall_syn-syu.sh [--dry-run] [--no-sudo]

Options:
  --dry-run   Preview all actions without deleting anything
  --no-sudo   Do not use sudo; skip files that require it
EOF
}

#--- parse_args
parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --dry-run) DRY_RUN=1; shift ;;
      --no-sudo) SUDO_CMD=""; shift ;;
      -h|--help) usage; exit 0 ;;
      *) log_error "ARGS" "Unknown option $1"; usage; exit 1 ;;
    esac
  done
}

#--- prompt_yes_no
prompt_yes_no() {
  local question="$1" default="${2:-N}"
  local reply options="[y/N]"
  if [ "$default" = "Y" ] || [ "$default" = "y" ]; then
    options="[Y/n]"
  fi
  while true; do
    printf '%s %s ' "$question" "$options"
    read -r reply
    reply="${reply:-$default}"
    case "$reply" in
      [Yy]) return 0 ;;
      [Nn]) return 1 ;;
      *) printf 'Please answer y or n.\n' ;;
    esac
  done
}

#--- can_write
can_write() {
  local path="$1"
  [ -w "$path" ] || [ -w "$(dirname "$path")" ]
}

#--- rm_path
rm_path() {
  local target="$1"
  if [ ! -e "$target" ]; then
    return 0
  fi
  if [ "$DRY_RUN" = "1" ]; then
    log_info "DRY" "rm -rf $target"
    return 0
  fi
  if can_write "$target" || [ -z "$SUDO_CMD" ]; then
    log_info "RM" "Removing $target"
    rm -rf -- "$target"
  else
    log_info "RM" "Removing (sudo) $target"
    "$SUDO_CMD" rm -rf -- "$target"
  fi
}

#--- detect_targets
detect_targets() {
  BIN_SYNSYU="$(command -v syn-syu 2>/dev/null || command -v synsyu 2>/dev/null || true)"
  BIN_CORE="$(command -v synsyu_core 2>/dev/null || true)"
  CARGO_CORE="$HOME/.cargo/bin/synsyu_core"

  LIB_DIRS=(
    "/usr/local/share/syn-syu"
    "/usr/lib/syn-syu"
    "/usr/local/share/synsyu"
    "/usr/lib/synsyu"
    "/usr/local/bin/lib"
  )

  USER_CONFIG_DIR="$HOME/.config/syn-syu"
  USER_DATA_DIR="$HOME/.local/share/syn-syu"
  MANIFEST_FILE="/tmp/syn-syu_manifest.json"
  LEGACY_CONFIG_DIR="$HOME/.config/synsyu"
  LEGACY_DATA_DIR="$HOME/.local/share/synsyu"
  LEGACY_MANIFEST="/tmp/synsyu_manifest.json"
}

#--- summarize
summarize() {
  printf '\nDetected components:\n'
  printf '  syn-syu      : %s\n' "${BIN_SYNSYU:-<not found>}"
  printf '  synsyu_core  : %s\n' "${BIN_CORE:-<not found>}"
  if [ -x "$CARGO_CORE" ]; then
    printf '  (cargo) core : %s\n' "$CARGO_CORE"
  fi
  local found_lib=0
  local d
  for d in "${LIB_DIRS[@]}"; do
    if [ -d "$d" ]; then
      printf '  lib dir      : %s\n' "$d"
      found_lib=1
    fi
  done
  if [ "$found_lib" -eq 0 ]; then
    printf '  lib dir      : <none>\n'
  fi
  printf '  user config  : %s\n' "$USER_CONFIG_DIR"
  printf '  user data    : %s\n' "$USER_DATA_DIR"
  printf '  manifest     : %s\n' "$MANIFEST_FILE"
  if [ -d "$LEGACY_CONFIG_DIR" ] || [ -d "$LEGACY_DATA_DIR" ] || [ -f "$LEGACY_MANIFEST" ]; then
    printf '  legacy config: %s\n' "$LEGACY_CONFIG_DIR"
    printf '  legacy data  : %s\n' "$LEGACY_DATA_DIR"
    printf '  legacy manifest: %s\n' "$LEGACY_MANIFEST"
  fi
}

#--- uninstall_core
uninstall_core() {
  if [ -n "${BIN_CORE:-}" ] && [ -x "$BIN_CORE" ]; then
    if prompt_yes_no "Remove synsyu_core at $BIN_CORE?" "Y"; then
      rm_path "$BIN_CORE"
    fi
  fi
  if [ -x "$CARGO_CORE" ] && prompt_yes_no "Remove cargo-installed synsyu_core ($CARGO_CORE)?" "N"; then
    rm_path "$CARGO_CORE"
  fi
}

#--- uninstall_syn_syu
uninstall_syn_syu() {
  if [ -n "${BIN_SYNSYU:-}" ] && [ -x "$BIN_SYNSYU" ]; then
    if prompt_yes_no "Remove syn-syu at $BIN_SYNSYU?" "Y"; then
      rm_path "$BIN_SYNSYU"
    fi
  fi
}

#--- uninstall_libs
uninstall_libs() {
  local d
  for d in "${LIB_DIRS[@]}"; do
    if [ -d "$d" ] && prompt_yes_no "Remove library directory $d?" "Y"; then
      rm_path "$d"
    fi
  done
}

#--- uninstall_user_data
uninstall_user_data() {
  if [ -d "$USER_CONFIG_DIR" ] && prompt_yes_no "Remove user config $USER_CONFIG_DIR?" "N"; then
    rm_path "$USER_CONFIG_DIR"
  fi
  if [ -d "$USER_DATA_DIR" ] && prompt_yes_no "Remove logs/data $USER_DATA_DIR?" "N"; then
    rm_path "$USER_DATA_DIR"
  fi
  if [ -f "$MANIFEST_FILE" ] && prompt_yes_no "Remove manifest $MANIFEST_FILE?" "Y"; then
    rm_path "$MANIFEST_FILE"
  fi
  if [ -d "$LEGACY_CONFIG_DIR" ] && prompt_yes_no "Remove legacy config $LEGACY_CONFIG_DIR?" "N"; then
    rm_path "$LEGACY_CONFIG_DIR"
  fi
  if [ -d "$LEGACY_DATA_DIR" ] && prompt_yes_no "Remove legacy data $LEGACY_DATA_DIR?" "N"; then
    rm_path "$LEGACY_DATA_DIR"
  fi
  if [ -f "$LEGACY_MANIFEST" ] && prompt_yes_no "Remove legacy manifest $LEGACY_MANIFEST?" "Y"; then
    rm_path "$LEGACY_MANIFEST"
  fi
}

#--- main
main() {
  parse_args "$@"
  log_info "INIT" "Syn-Syu uninstaller started (dry-run=${DRY_RUN})."
  detect_targets
  summarize
  if ! prompt_yes_no "Proceed with removal of the above components?" "N"; then
    log_warn "ABORT" "Operator cancelled uninstall."
    printf 'No changes made.\n'
    exit 0
  fi
  uninstall_syn_syu
  uninstall_core
  uninstall_libs
  uninstall_user_data
  log_info "DONE" "Uninstall completed."
  printf '\nUninstall complete. Log: %s\n' "$LOG_PATH"
}

main "$@"
