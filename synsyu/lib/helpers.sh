#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/helpers.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Detect and rank available Arch User Repository helpers.
#
# Security / Safety Notes:
#   Performs PATH lookups only; no commands are executed here.
#
# Dependencies:
#   command -v (POSIX shell builtin).
#
# Operational Scope:
#   Sourced by Syn-Syu to decide which helper should execute
#   AUR package updates when required.
#
# Revision History:
#   2024-11-04 COD  Added helper detection utilities.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Verbose variable names with explicit state
#   - Modular function design for testability
#============================================================

readonly HELPER_CANDIDATES=(paru yay pikaur trizen aura pacaur pamac aurman pakku)

#--- detect_helpers
detect_helpers() {
  DETECTED_HELPERS=()
  local candidate
  for candidate in "${HELPER_CANDIDATES[@]}"; do
    if command -v "$candidate" >/dev/null 2>&1; then
      DETECTED_HELPERS+=("$candidate")
    fi
  done
}

#--- select_helper
select_helper() {
  local preferred helper
  for preferred in "${HELPER_PRIORITY[@]}"; do
    for helper in "${DETECTED_HELPERS[@]}"; do
      if [ "$preferred" = "$helper" ]; then
        printf '%s\n' "$helper"
        return 0
      fi
    done
  done
  if [ "${#DETECTED_HELPERS[@]}" -gt 0 ]; then
    printf '%s\n' "${DETECTED_HELPERS[0]}"
    return 0
  fi
  return 1
}

#--- update_helper_default
update_helper_default() {
  local helper="$1"
  local cfg="${CONFIG_PATH:-$DEFAULT_CONFIG_PATH}"
  local cfg_dir
  cfg_dir="$(dirname "$cfg")"
  mkdir -p "$cfg_dir"
  if ! command -v python3 >/dev/null 2>&1; then
    printf 'python3 required to update config\n' >&2
    return 1
  fi
  python3 - "$helper" "$cfg" <<'PY' || return 1
import sys, os, tomllib, tomli_w

helper = sys.argv[1]
cfg_path = sys.argv[2]
data = {}
if os.path.isfile(cfg_path):
    with open(cfg_path, "rb") as handle:
        try:
            data = tomllib.load(handle)
        except Exception:
            data = {}
helpers = data.get("helpers") or {}
helpers["default"] = helper
data["helpers"] = helpers
with open(cfg_path, "wb") as handle:
    tomli_w.dump(data, handle)
PY
}
