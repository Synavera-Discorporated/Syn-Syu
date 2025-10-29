#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/logging.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Provide structured logging utilities for Syn-Syu
#   orchestrator, including append-only log files and hash
#   generation for audit chaining.
#
# Security / Safety Notes:
#   Writes logs under user-owned directories only.
#   No privileged operations are executed.
#
# Dependencies:
#   sha256sum (coreutils), date, mkdir.
#
# Operational Scope:
#   Sourced by Syn-Syu to emit runtime telemetry with RFC-3339
#   timestamps and to maintain log integrity hashes.
#
# Revision History:
#   2025-10-28 COD  Established logging primitives.
#------------------------------------------------------------
# SSE Principles Observed:
#   - set -euo pipefail compliance from parent script
#   - Append-only logging with UTC timestamps
#   - Deterministic log hashing via SHA-256
#============================================================

#--- log_init
log_init() {
  readonly LOG_VERBOSE="${LOG_VERBOSE:-0}"
  readonly LOG_TIME_ORIGIN="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  local _target_dir="${LOG_DIR:-$HOME/.local/share/syn-syu}"
  if ! mkdir -p "$_target_dir" 2>/dev/null; then
    _target_dir="/tmp/syn-syu/logs"
    mkdir -p "$_target_dir" 2>/dev/null || _target_dir="/tmp"
  fi
  readonly LOG_DIR="$_target_dir"
  readonly LOG_PATH="${LOG_PATH:-$LOG_DIR/${SESSION_STAMP}.log}"
  touch "$LOG_PATH"
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

#--- log_info
log_info() {
  log_event "INFO" "$1" "$2"
}

#--- log_warn
log_warn() {
  log_event "WARN" "$1" "$2"
}

#--- log_error
log_error() {
  log_event "ERROR" "$1" "$2"
}

#--- log_debug
log_debug() {
  if [ "$LOG_VERBOSE" = "1" ]; then
    log_event "DEBUG" "$1" "$2"
  fi
}

#--- log_finalize
log_finalize() {
  if [ -n "${LOG_PATH:-}" ] && [ -s "$LOG_PATH" ]; then
    local hash_path
    hash_path="${LOG_PATH}.hash"
    sha256sum "$LOG_PATH" >"$hash_path"
  fi
}
