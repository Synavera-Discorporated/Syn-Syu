#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/manifest.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Manage Syn-Syu manifest lifecycle and provide JSON parsing
#   helpers for downstream commands.
#
# Security / Safety Notes:
#   Relies on jq for read-only JSON operations. Manifest writes
#   are delegated to synsyu_core.
#
# Dependencies:
#   jq, synsyu_core binary.
#
# Operational Scope:
#   Sourced by Syn-Syu to ensure manifests exist, rebuild them
#   when requested, and provide structured accessors.
#
# Revision History:
#   2024-11-04 COD  Added manifest utilities.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Explicit error messages with dedicated codes
#   - Defensive checks around external command availability
#============================================================

#--- manifest_require
manifest_require() {
  if [ "${REBUILD_MANIFEST:-0}" = "1" ] || [ ! -f "$SYN_MANIFEST_PATH" ]; then
    log_info "MANIFEST" "Rebuilding manifest via synsyu_core"
    manifest_rebuild || return 1
  fi
}

#--- manifest_rebuild
manifest_rebuild() {
  local core_bin="$SYN_CORE_BIN"
  if [ ! -x "$core_bin" ]; then
    local discovered
    discovered="$(command -v synsyu_core 2>/dev/null || true)"
    if [ -n "$discovered" ]; then
      core_bin="$discovered"
    fi
  fi
  if [ ! -x "$core_bin" ]; then
    log_error "E301" "synsyu_core binary not found at $SYN_CORE_BIN"
    return 1
  fi

  local args=("--manifest" "$SYN_MANIFEST_PATH")
  if [ -n "${CONFIG_PATH:-}" ] && [ -f "$CONFIG_PATH" ]; then
    args+=("--config" "$CONFIG_PATH")
  fi
  if [ "$NO_AUR" = "1" ]; then
    args+=("--no-aur")
  fi
  if [ "$NO_REPO" = "1" ]; then
    args+=("--no-repo")
  fi
  if [ "$LOG_VERBOSE" = "1" ]; then
    args+=("--verbose")
  fi
  if [ "${MIN_FREE_SPACE_BYTES:-0}" -gt 0 ]; then
    args+=("--min-free-gb" "$(bytes_to_gb_string "$MIN_FREE_SPACE_BYTES")")
  fi

  if ! "$core_bin" "${args[@]}"; then
    log_error "E304" "synsyu_core invocation failed"
    return 1
  fi
}

#--- manifest_updates_stream
manifest_updates_stream() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    log_error "E302" "Manifest $SYN_MANIFEST_PATH missing"
    return 1
  fi
  jq -r '.packages | to_entries[] | select(.value.update_available == true)
    | "\(.key)|\(.value.source)|\(.value.newer_version)"' "$SYN_MANIFEST_PATH"
}

#--- manifest_packages_stream
manifest_packages_stream() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    return 1
  fi
  jq -r '.packages | to_entries[] | "\(.key)|\(.value.source)|\(.value.newer_version)|\(.value.update_available)"' "$SYN_MANIFEST_PATH"
}

#--- manifest_package_requirements
manifest_package_requirements() {
  local package="$1"
  if [ -z "$package" ]; then
    return 1
  fi
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    return 1
  fi
  jq -r --arg pkg "$package" '
    (.packages[$pkg] // empty)
    | "\((.download_size_selected // 0))|\((.build_size_estimate // 0))|\((.install_size_estimate // .installed_size_selected // 0))|\((.transient_size_estimate // 0))"
  ' "$SYN_MANIFEST_PATH"
}

#--- manifest_inspect
manifest_inspect() {
  local package="$1"
  if [ -z "$package" ]; then
    log_error "E303" "manifest_inspect requires package name"
    return 1
  fi
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    log_error "E302" "Manifest $SYN_MANIFEST_PATH missing"
    return 1
  fi
  jq -r --arg pkg "$package" '
    .packages[$pkg] // empty | to_entries[] | "\(.key): \(.value)"' "$SYN_MANIFEST_PATH"
}

#--- manifest_summary
manifest_summary() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    return 1
  fi
  local summary
  summary="$(jq -r '
    [
      .metadata.total_packages,
      .metadata.repo_candidates,
      .metadata.aur_candidates,
      .metadata.updates_available,
      (.metadata.download_size_total // 0),
      (.metadata.build_size_total // 0),
      (.metadata.install_size_total // 0),
      (.metadata.transient_size_total // 0),
      (.metadata.min_free_bytes // 0),
      (.metadata.required_space_total // 0),
      (.metadata.available_space_bytes // 0),
      (.metadata.space_checked_path // "")
    ] | @tsv
  ' "$SYN_MANIFEST_PATH" 2>/dev/null || true)"
  if [ -z "$summary" ]; then
    return 1
  fi
  local pkgs repo aur updates dl build inst trans buf req avail path
  IFS=$'\t' read -r pkgs repo aur updates dl build inst trans buf req avail path <<<"$summary"
  printf 'Packages: %s\n' "$pkgs"
  printf 'Repo candidates: %s\n' "$repo"
  printf 'AUR candidates: %s\n' "$aur"
  printf 'Updates available: %s\n' "$updates"
  printf 'Download size (bytes): %s (~%s GB)\n' "$dl" "$(bytes_to_gb_string "$dl")"
  printf 'Build size (bytes): %s (~%s GB)\n' "$build" "$(bytes_to_gb_string "$build")"
  printf 'Install size (bytes): %s (~%s GB)\n' "$inst" "$(bytes_to_gb_string "$inst")"
  printf 'Transient size (bytes): %s (~%s GB)\n' "$trans" "$(bytes_to_gb_string "$trans")"
  printf 'Buffer (bytes): %s (~%s GB)\n' "$buf" "$(bytes_to_gb_string "$buf")"
  printf 'Required (bytes): %s (~%s GB)\n' "$req" "$(bytes_to_gb_string "$req")"
  printf 'Available (bytes): %s (~%s GB)\n' "$avail" "$(bytes_to_gb_string "$avail")"
  printf 'Checked path: %s\n' "$path"
}
