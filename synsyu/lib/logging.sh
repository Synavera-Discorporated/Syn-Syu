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

#--- log_level_threshold
_log_level_threshold() {
  local level="${1:-info}"
  case "${level,,}" in
    debug) echo 3 ;;
    info) echo 2 ;;
    warn) echo 1 ;;
    error) echo 0 ;;
    none|off) echo -1 ;;
    *) echo 2 ;;
  esac
}

#--- log_should_write
log_should_write() {
  local level="$1"
  local numeric
  case "$level" in
    ERROR) numeric=0 ;;
    WARN) numeric=1 ;;
    INFO) numeric=2 ;;
    DEBUG) numeric=3 ;;
    *) numeric=2 ;;
  esac
  [ "${LOG_LEVEL_THRESHOLD:-2}" -ge "$numeric" ]
}

#--- log_prune
log_prune() {
  local dir="$1"
  [ -d "$dir" ] || return 0

  local retention_days="${LOG_RETENTION_DAYS:-0}"
  if [[ "$retention_days" =~ ^[0-9]+$ ]] && [ "$retention_days" -gt 0 ]; then
    find "$dir" -maxdepth 1 -type f \( -name '*.log' -o -name '*.log.hash' \) -mtime +"$retention_days" -print0 \
      | xargs -0 -r rm -f --
  fi

  local retention_mb="${LOG_RETENTION_SIZE_MB:-0}"
  if [[ "$retention_mb" =~ ^[0-9]+$ ]] && [ "$retention_mb" -gt 0 ]; then
    LOG_PRUNE_DIR="$dir" LOG_PRUNE_BYTES="$((retention_mb * 1024 * 1024))" python3 - <<'PY'
import os
import sys

directory = os.environ.get("LOG_PRUNE_DIR")
try:
    limit = int(os.environ.get("LOG_PRUNE_BYTES", "0"))
except (TypeError, ValueError):
    limit = 0

if not directory or limit <= 0:
    sys.exit(0)

logs = []
try:
    with os.scandir(directory) as entries:
        for entry in entries:
            if entry.is_file() and entry.name.endswith(".log"):
                try:
                    stat = entry.stat()
                except FileNotFoundError:
                    continue
                logs.append((stat.st_mtime, entry.path, stat.st_size))
except FileNotFoundError:
    sys.exit(0)

if not logs:
    sys.exit(0)

logs.sort(key=lambda item: item[0], reverse=True)
total = sum(item[2] for item in logs)
idx = len(logs) - 1

while total > limit and idx >= 0:
    _, path, size = logs[idx]
    try:
        os.remove(path)
    except FileNotFoundError:
        pass
    hash_path = f"{path}.hash"
    if os.path.exists(hash_path):
        try:
            os.remove(hash_path)
        except FileNotFoundError:
            pass
    total -= size
    idx -= 1
PY
  fi
}

#--- log_init
log_init() {
  declare -gr LOG_VERBOSE="${LOG_VERBOSE:-0}"
  declare -gr LOG_TIME_ORIGIN="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  local _target_dir="${LOG_DIR:-$HOME/.local/share/syn-syu}"
  if ! mkdir -p "$_target_dir" 2>/dev/null; then
    _target_dir="/tmp/syn-syu/logs"
    mkdir -p "$_target_dir" 2>/dev/null || _target_dir="/tmp"
  fi
  declare -gr LOG_DIR="$_target_dir"
  declare -gr LOG_PATH="${LOG_PATH:-$LOG_DIR/${SESSION_STAMP}.log}"
  local configured_level="${LOG_LEVEL:-info}"
  local threshold
  threshold="$(_log_level_threshold "$configured_level")"
  declare -gr LOG_LEVEL_NAME="${configured_level,,}"
  declare -gr LOG_LEVEL_THRESHOLD="$threshold"
  log_prune "$LOG_DIR"
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
  if log_should_write "$level"; then
    printf '%s [%s] [%s] %s\n' "$timestamp" "$level" "$code" "$message" >>"$LOG_PATH"
  fi
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
  log_event "DEBUG" "$1" "$2"
}

#--- log_finalize
log_finalize() {
  if [ -n "${LOG_PATH:-}" ] && [ -s "$LOG_PATH" ]; then
    local hash_path
    hash_path="${LOG_PATH}.hash"
    sha256sum "$LOG_PATH" >"$hash_path"
  fi
}
