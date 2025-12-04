#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/plan.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Thin wrapper that delegates plan generation to synsyu_core.
#
# Security / Safety Notes:
#   Read-only operations; relies on synsyu_core manifest.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Deterministic JSON output
#   - Clear operator prompts for additional detail
#============================================================

cmd_plan() {
  manifest_require
  plan_generate_core
}

plan_generate_core() {
  local core_bin="$SYN_CORE_BIN"
  if [ ! -x "$core_bin" ]; then
    core_bin="$(command -v synsyu_core 2>/dev/null || true)"
  fi
  if [ -z "$core_bin" ] || [ ! -x "$core_bin" ]; then
    log_error "PLAN" "synsyu_core not found; cannot build plan."
    return 1
  fi

  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  local plan_path="${PLAN_PATH:-$DEFAULT_PLAN_PATH}"
  local -a args=("plan" "--manifest" "$manifest_path" "--plan" "$plan_path")

  if [ "${NO_REPO:-0}" = "1" ]; then args+=("--no-repo"); fi
  if [ "${NO_AUR:-0}" = "1" ]; then args+=("--no-aur"); fi
  if [ "${APPLICATIONS_FLATPAK:-0}" = "1" ]; then args+=("--with-flatpak"); fi
  if [ "${APPLICATIONS_FWUPD:-0}" = "1" ]; then args+=("--with-fwupd"); fi
  if [ "${OFFLINE:-0}" = "1" ]; then args+=("--offline"); fi
  if [ "${STRICT_MODE:-0}" = "1" ]; then args+=("--strict"); fi
  if [ "${JSON_OUTPUT:-0}" = "1" ]; then args+=("--json"); fi

  log_info "PLAN" "Invoking synsyu_core plan"
  if ! "$core_bin" "${args[@]}"; then
    log_error "PLAN" "synsyu_core plan failed"
    return 1
  fi

  if [ "${EDIT_PLAN:-0}" = "1" ]; then
    plan_edit_file "$plan_path"
  fi
}

#--- plan_edit_file
plan_edit_file() {
  local file="$1"
  local editor="${EDITOR:-}"
  if [ -z "$editor" ]; then
    if command -v nano >/dev/null 2>&1; then
      editor="nano"
    elif command -v vi >/dev/null 2>&1; then
      editor="vi"
    else
      printf 'No editor available to open %s\n' "$file"
      return 1
    fi
  fi
  "$editor" "$file"
}
