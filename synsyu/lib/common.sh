#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/common.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Shared utility functions for conversions and failure
#   tracking used across Syn-Syu command modules.
#
# Security / Safety Notes:
#   Pure data transformations; no external commands beyond
#   python3 for numeric conversions.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Modular utilities for reuse across command modules
#   - Explicit, side-effect-free helpers for predictability
#------------------------------------------------------------

#--- expand_path_simple
expand_path_simple() {
  local raw="${1:-}"
  python3 - "$raw" <<'PY'
import os, sys
path = sys.argv[1] if len(sys.argv) > 1 else ""
print(os.path.abspath(os.path.expanduser(path)))
PY
}

#--- convert_gb_to_bytes
convert_gb_to_bytes() {
  local gb_input="${1:-0}"
  python3 - "$gb_input" <<'PY'
import sys
try:
    value = float(sys.argv[1])
except (ValueError, IndexError):
    print(0)
    raise SystemExit
if value <= 0:
    print(0)
else:
    print(int(round(value * 1024 * 1024 * 1024)))
PY
}

#--- bytes_to_gb_string
bytes_to_gb_string() {
  local bytes_input="${1:-0}"
  python3 - "$bytes_input" <<'PY'
import sys
try:
    value = float(sys.argv[1])
except (ValueError, IndexError):
    print("0")
    raise SystemExit
if value <= 0:
    print("0")
else:
    gb = value / (1024.0 * 1024.0 * 1024.0)
    text = f"{gb:.3f}".rstrip("0").rstrip(".")
    print(text if text else "0")
PY
}

#--- record_failed_update
record_failed_update() {
  local pkg="${1:-unknown}" reason="${2:-unspecified failure}"
  reason="${reason//$'\n'/ }"
  FAILED_UPDATES+=("$pkg|$reason")
}

#--- print_failed_update_summary
print_failed_update_summary() {
  local count=${#FAILED_UPDATES[@]}
  if [ "$count" -eq 0 ]; then
    return 0
  fi
  log_warn "SUMMARY" "Failed updates recorded: $count"
  if [ "$QUIET" != "1" ]; then
    printf -- '-> Failed updates (%s):\n' "$count"
  fi
  local entry pkg reason
  for entry in "${FAILED_UPDATES[@]}"; do
    IFS='|' read -r pkg reason <<<"$entry"
    log_warn "FAIL" "$pkg: $reason"
    if [ "$QUIET" != "1" ]; then
      printf '   - %s: %s\n' "$pkg" "$reason"
    fi
  done
}
