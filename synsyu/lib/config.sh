#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/config.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Configuration loading and prerequisite checks.
#
# Security / Safety Notes:
#   Reads user-owned config files; refuses to proceed if jq or
#   python3 are missing.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Defensive prerequisite checks before execution
#   - Explicit parsing with clear defaults and overrides
#------------------------------------------------------------

#--- ensure_prerequisites
ensure_prerequisites() {
  if ! command -v jq >/dev/null 2>&1; then
    printf 'Syn-Syu requires jq in PATH.\n' >&2
    exit 100
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    printf 'Syn-Syu requires python3 for configuration parsing.\n' >&2
    exit 100
  fi
}

#--- load_config
load_config() {
  if [ -z "$CONFIG_PATH" ]; then
    CONFIG_PATH="$DEFAULT_CONFIG_PATH"
  fi

  if [ ! -f "$CONFIG_PATH" ]; then
    HELPER_PRIORITY=($(printf '%s\n' "${HELPER_CANDIDATES[@]}"))
    return
  fi

  local py_output
  if ! py_output="$(CONFIG_PATH="$CONFIG_PATH" python3 - <<'PY'
import json
import os
import sys
import tomllib

config_path = os.environ.get("CONFIG_PATH")
try:
    with open(config_path, "rb") as handle:
        data = tomllib.load(handle)
except FileNotFoundError:
    sys.exit(1)

core = data.get("core", {})
helpers = data.get("helpers", {})
snapshots = data.get("snapshots", {})
safety = data.get("safety", {})
clean = data.get("clean", {})
logging = data.get("logging", {})
space = data.get("space", {})
applications = data.get("applications", {})
helpers_section = data.get("helpers", {})

log_directory = logging.get("directory") or core.get("log_directory", "")

def to_bytes(value):
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    if number <= 0:
        return 0
    return int(round(number * 1024 * 1024 * 1024))

settings = {
    "manifest": core.get("manifest_path", ""),
    "helper_priority": helpers.get("priority", []),
    "helper_default": helpers_section.get("default", ""),
    "log_directory": log_directory,
    "log_level": logging.get("level", ""),
    "log_retention_days": logging.get("retention_days"),
    "log_retention_megabytes": logging.get("retention_megabytes"),
    "batch_size": core.get("batch_size", 10),
    "space_min_free_bytes": to_bytes(space.get("min_free_gb")),
    "space_mode": space.get("mode", "warn"),
    "snapshots_enabled": snapshots.get("enabled", False),
    "snapshot_pre": snapshots.get("pre_command", ""),
    "snapshot_post": snapshots.get("post_command", ""),
    "snapshot_require_success": snapshots.get("require_success", False),
    "safety_disk_check": safety.get("disk_check", False),
    "safety_disk_margin_mb": safety.get("disk_extra_margin_mb", 200),
    "clean_keep_versions": clean.get("keep_versions", 2),
    "clean_remove_orphans": clean.get("remove_orphans", False),
    "clean_check_pacnew": clean.get("check_pacnew", True),
    "apps_flatpak_enabled": applications.get("flatpak", False),
    "apps_fwupd_enabled": applications.get("fwupd", False)
}

print(json.dumps(settings))
PY
)"; then
    py_output=""
  fi

  local manifest_path helper_line helper_default log_dir batch_size log_level retention_days retention_mb
  local apps_flatpak apps_fwupd
  if [ -n "$py_output" ]; then
    manifest_path="$(printf '%s' "$py_output" | jq -r '.manifest // ""')"
    helper_line="$(printf '%s' "$py_output" | jq -r '.helper_priority | join(" ")')"
    helper_default="$(printf '%s' "$py_output" | jq -r '.helper_default // empty')"
    log_dir="$(printf '%s' "$py_output" | jq -r '.log_directory // ""')"
    log_level="$(printf '%s' "$py_output" | jq -r '.log_level // empty')"
    retention_days="$(printf '%s' "$py_output" | jq -r '.log_retention_days // empty')"
    retention_mb="$(printf '%s' "$py_output" | jq -r '.log_retention_megabytes // empty')"
    batch_size="$(printf '%s' "$py_output" | jq -r '.batch_size // 10')"
    apps_flatpak="$(printf '%s' "$py_output" | jq -r '.apps_flatpak_enabled // false')"
    apps_fwupd="$(printf '%s' "$py_output" | jq -r '.apps_fwupd_enabled // false')"
    local min_free_bytes_config
    min_free_bytes_config="$(printf '%s' "$py_output" | jq -r '.space_min_free_bytes // empty')"
    local space_mode
    space_mode="$(printf '%s' "$py_output" | jq -r '.space_mode // "warn"')"
    local snapshots_enabled
    snapshots_enabled="$(printf '%s' "$py_output" | jq -r '.snapshots_enabled')"
    local snapshot_pre snapshot_post snapshot_require
    snapshot_pre="$(printf '%s' "$py_output" | jq -r '.snapshot_pre // ""')"
    snapshot_post="$(printf '%s' "$py_output" | jq -r '.snapshot_post // ""')"
    snapshot_require="$(printf '%s' "$py_output" | jq -r '.snapshot_require_success')"
    local safety_disk_check disk_margin
    safety_disk_check="$(printf '%s' "$py_output" | jq -r '.safety_disk_check')"
    disk_margin="$(printf '%s' "$py_output" | jq -r '.safety_disk_margin_mb // 200')"
    local clean_keep clean_orphans clean_pacnew
    clean_keep="$(printf '%s' "$py_output" | jq -r '.clean_keep_versions // 2')"
    clean_orphans="$(printf '%s' "$py_output" | jq -r '.clean_remove_orphans')"
    clean_pacnew="$(printf '%s' "$py_output" | jq -r '.clean_check_pacnew')"

    if [ -n "$manifest_path" ] && [ "$SYN_MANIFEST_PATH" = "$DEFAULT_MANIFEST" ]; then
      SYN_MANIFEST_PATH="$manifest_path"
    fi

    if [ -n "$helper_line" ]; then
      # shellcheck disable=SC2206
      HELPER_PRIORITY=($helper_line)
    else
      HELPER_PRIORITY=($(printf '%s\n' "${HELPER_CANDIDATES[@]}"))
    fi

    if [ -z "${AUR_HELPER:-}" ] && [ -n "$helper_default" ]; then
      AUR_HELPER="$helper_default"
    fi

    if [ -n "$log_dir" ]; then
      LOG_DIR="$log_dir"
    fi
    if [ -n "$log_level" ]; then
      local normalized
      normalized="${log_level,,}"
      case "$normalized" in
        debug|info|warn|warning|error|none|off)
          case "$normalized" in
            warning) normalized="warn" ;;
            off) normalized="none" ;;
          esac
          LOG_LEVEL="$normalized"
          ;;
        *)
          printf 'Syn-Syu config: invalid logging.level "%s"; defaulting to info.\n' "$log_level" >&2
          LOG_LEVEL="info"
          ;;
      esac
    fi
    if [[ "$retention_days" =~ ^[0-9]+$ ]]; then
      LOG_RETENTION_DAYS="$retention_days"
    fi
    if [[ "$retention_mb" =~ ^[0-9]+$ ]]; then
      LOG_RETENTION_SIZE_MB="$retention_mb"
    fi

    if [[ "$batch_size" =~ ^[0-9]+$ ]]; then
      BATCH_SIZE="$batch_size"
    fi

    if [[ "$min_free_bytes_config" =~ ^[0-9]+$ ]]; then
      if [ "$min_free_bytes_config" -ge 0 ]; then
        MIN_FREE_SPACE_BYTES="$min_free_bytes_config"
      fi
    fi
    SPACE_MODE="$space_mode"

    if [ "$snapshots_enabled" = "true" ]; then
      SNAPSHOTS_ENABLED=1
    fi
    SNAPSHOT_PRE_CMD="$snapshot_pre"
    SNAPSHOT_POST_CMD="$snapshot_post"
    if [ "$snapshot_require" = "true" ]; then
      SNAPSHOT_REQUIRE_SUCCESS=1
    fi

    if [ "$safety_disk_check" = "true" ]; then
      DISK_CHECK=1
    elif [ "$safety_disk_check" = "false" ]; then
      DISK_CHECK=0
    fi
    if [[ "$disk_margin" =~ ^[0-9]+$ ]]; then
      DISK_MARGIN_MB="$disk_margin"
    fi

    if [[ "$clean_keep" =~ ^[0-9]+$ ]]; then
      CLEAN_KEEP_VERSIONS="$clean_keep"
    fi
    if [ "$clean_orphans" = "true" ]; then
      CLEAN_REMOVE_ORPHANS=1
    fi
    if [ "$clean_pacnew" = "false" ]; then
      CLEAN_CHECK_PACNEW=0
    fi

    if [ -z "$APPLICATIONS_FLATPAK_CLI" ] && [ "$apps_flatpak" = "true" ]; then
      APPLICATIONS_FLATPAK=1
    fi
    if [ -z "$APPLICATIONS_FWUPD_CLI" ] && [ "$apps_fwupd" = "true" ]; then
      APPLICATIONS_FWUPD=1
    fi
  fi

  if [ ${#HELPER_PRIORITY[@]} -eq 0 ]; then
    HELPER_PRIORITY=($(printf '%s\n' "${HELPER_CANDIDATES[@]}"))
  fi
  if [ "$BATCH_SIZE" -le 0 ]; then
    BATCH_SIZE=1
  fi
  if [ -n "${MIN_FREE_SPACE_OVERRIDE_BYTES:-}" ]; then
    MIN_FREE_SPACE_BYTES="$MIN_FREE_SPACE_OVERRIDE_BYTES"
  fi
}
