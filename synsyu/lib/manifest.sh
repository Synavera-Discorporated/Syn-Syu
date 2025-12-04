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

#--- manifest_resolved_path
manifest_resolved_path() {
  local resolved
  resolved="$(expand_path_simple "$SYN_MANIFEST_PATH")"
  SYN_MANIFEST_PATH_RESOLVED="$resolved"
  printf '%s\n' "$resolved"
}

#--- manifest_require
manifest_require() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ "${REBUILD_MANIFEST:-0}" = "1" ] || [ ! -f "$manifest_path" ]; then
    log_info "MANIFEST" "Rebuilding manifest via synsyu_core"
    manifest_rebuild || return 1
  fi
  manifest_apply_application_flags
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

  local manifest_path
  manifest_path="$(manifest_resolved_path)"

  local args=("--manifest" "$manifest_path")
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
  if [ "${OFFLINE:-0}" = "1" ]; then
    args+=("--offline")
  fi

  if ! "$core_bin" "${args[@]}"; then
    log_error "E304" "synsyu_core invocation failed"
    return 1
  fi

  manifest_update_applications_section
}

#--- manifest_update_applications_section
manifest_update_applications_section() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 0
  fi

  local apps_json
  apps_json="$(manifest_collect_applications_json)" || apps_json=""
  if [ -z "$apps_json" ]; then
    log_warn "MANIFEST" "Unable to collect application metadata; manifest unchanged"
    return 0
  fi

  local tmp_file
  tmp_file="$(mktemp "${TMPDIR:-/tmp}/synsyu_apps_XXXXXX")" || return 1
  if ! jq --argjson apps "$apps_json" '
    .applications = $apps
    | .metadata.apps_flatpak = ($apps.flatpak.enabled // false)
    | .metadata.apps_fwupd = ($apps.fwupd.enabled // false)
    | .metadata.application_state = {
        flatpak: ($apps.flatpak.installed_count // 0),
        fwupd: ($apps.fwupd.device_count // 0)
      }
  ' "$manifest_path" >"$tmp_file"; then
    rm -f "$tmp_file"
    log_warn "MANIFEST" "Failed to append application data to manifest"
    return 0
  fi
  mv "$tmp_file" "$manifest_path"

  local flatpak_enabled fwupd_enabled flatpak_count fwupd_count
  flatpak_enabled="$(printf '%s' "$apps_json" | jq -r '.flatpak.enabled // false' 2>/dev/null || echo "false")"
  fwupd_enabled="$(printf '%s' "$apps_json" | jq -r '.fwupd.enabled // false' 2>/dev/null || echo "false")"
  flatpak_count="$(printf '%s' "$apps_json" | jq -r '.flatpak.installed_count // 0' 2>/dev/null || echo "0")"
  fwupd_count="$(printf '%s' "$apps_json" | jq -r '.fwupd.device_count // 0' 2>/dev/null || echo "0")"
  log_info "MANIFEST" "Application state recorded: flatpak=${flatpak_enabled:-false} (installed ${flatpak_count:-0}), fwupd=${fwupd_enabled:-false} (devices ${fwupd_count:-0})"
}

#--- manifest_collect_applications_json
manifest_collect_applications_json() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  local flatpak_json fwupd_json
  flatpak_json="$(manifest_collect_flatpak_updates)" || flatpak_json=""

  # Preserve fwupd data from core; do not re-run fwupdmgr here.
  if [ "${APPLICATIONS_FWUPD:-0}" = "1" ]; then
    fwupd_json="$(jq -r '.applications.fwupd // empty' "$manifest_path" 2>/dev/null || true)"
    if [ -z "$fwupd_json" ]; then
      fwupd_json='{"enabled":true,"device_count":0,"devices":[]}'
    fi
  else
    fwupd_json='{"enabled":false,"device_count":0,"devices":[]}'
  fi

  if [ -z "$flatpak_json" ]; then
    flatpak_json='{"enabled":false,"available":false,"update_count":0,"updates":[]}'
  fi

  jq -n --argjson flatpak "$flatpak_json" --argjson fwupd "$fwupd_json" '{flatpak: $flatpak, fwupd: $fwupd}'
}

#--- manifest_collect_flatpak_updates
manifest_collect_flatpak_updates() {
  if [ "${APPLICATIONS_FLATPAK:-0}" != "1" ]; then
    jq -n '{enabled: false, installed_count: 0, installed: [], note: "flatpak disabled"}'
    return 0
  fi
  if ! command -v flatpak >/dev/null 2>&1; then
    log_warn "FLATPAK" "Flatpak requested for manifest but binary not present"
    jq -n '{enabled: true, installed_count: 0, installed: [], note: "flatpak not installed"}'
    return 0
  fi

  local output parsed
  output="$(flatpak list --columns=application,version,branch,origin 2>/dev/null || true)"
  parsed="$(printf '%s\n' "$output" | python3 - <<'PY' 2>/dev/null || true
import json
import sys

lines = [ln.strip() for ln in sys.stdin.read().splitlines() if ln.strip()]
apps = []
for line in lines:
    parts = line.split()
    if len(parts) >= 3:
        app = parts[0]
        version = parts[1] if len(parts) >= 4 else ""
        branch = parts[-2] if len(parts) >= 3 else ""
        origin = parts[-1] if len(parts) >= 2 else ""
        apps.append({"application": app, "version": version, "branch": branch, "origin": origin})

print(json.dumps({
    "enabled": True,
    "installed": apps,
    "installed_count": len(apps)
}))
PY
)"
  if [ -z "$parsed" ]; then
    parsed='{"enabled": true, "installed": [], "installed_count": 0}'
  fi
  printf '%s' "$parsed"
}

#--- manifest_apply_application_flags
manifest_apply_application_flags() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 0
  fi

  local flatpak_enabled fwupd_enabled
  flatpak_enabled="$(jq -r '.applications.flatpak.enabled // empty' "$manifest_path" 2>/dev/null || true)"
  fwupd_enabled="$(jq -r '.applications.fwupd.enabled // empty' "$manifest_path" 2>/dev/null || true)"

  if [ -z "$APPLICATIONS_FLATPAK_CLI" ] && [ -n "$flatpak_enabled" ]; then
    if [ "$flatpak_enabled" = "true" ]; then
      APPLICATIONS_FLATPAK=1
    else
      APPLICATIONS_FLATPAK=0
    fi
  fi
  if [ -z "$APPLICATIONS_FWUPD_CLI" ] && [ -n "$fwupd_enabled" ]; then
    if [ "$fwupd_enabled" = "true" ]; then
      APPLICATIONS_FWUPD=1
    else
      APPLICATIONS_FWUPD=0
    fi
  fi
}

#--- manifest_updates_stream
manifest_updates_stream() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    log_error "E302" "Manifest $SYN_MANIFEST_PATH missing"
    return 1
  fi
  jq -r '.packages | to_entries[] | select(.value.update_available == true)
    | "\(.key)|\(.value.source)|\(.value.newer_version)"' "$manifest_path"
}

#--- manifest_packages_stream
manifest_packages_stream() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 1
  fi
  jq -r '.packages | to_entries[] | "\(.key)|\(.value.source)|\(.value.newer_version)|\(.value.update_available)"' "$manifest_path"
}

#--- manifest_update_details
manifest_update_details() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 1
  fi
  jq -r '
    .packages
    | to_entries
    | map(select(.value.update_available==true)
      | [
          .key,
          (.value.source // "unknown"),
          (.value.installed_version // "?"),
          (.value.newer_version // "?")
        ] | @tsv)
    | .[]
  ' "$manifest_path"
}

#--- manifest_application_update_details
manifest_application_update_details() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 1
  fi
  jq -r '
    [
      (if .applications.flatpak.enabled==true then
         (.applications.flatpak.updates // [])
         | map("flatpak\t" + ((.application // .raw // "?")
           + (if (.branch // "") != "" then " [" + .branch + "]" else "" end)
           + (if (.origin // "") != "" then " {" + .origin + "}" else "" end)))
       else [] end),
      (if .applications.fwupd.enabled==true then
         (.applications.fwupd.updates // [])
         | map("fwupd\t" + ((.device // .raw // "?")
           + (if (.version // "") != "" then " -> " + .version else "" end)
           + (if (.title // "") != "" then " (" + .title + ")" else "" end)))
       else [] end)
    ]
    | flatten
    | .[]
  ' "$manifest_path"
}

#--- manifest_package_requirements
manifest_package_requirements() {
  local package="$1"
  if [ -z "$package" ]; then
    return 1
  fi
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 1
  fi
  jq -r --arg pkg "$package" '
    (.packages[$pkg] // empty)
    | "\((.download_size_selected // 0))|\((.build_size_estimate // 0))|\((.install_size_estimate // .installed_size_selected // 0))|\((.transient_size_estimate // 0))"
  ' "$manifest_path"
}

#--- manifest_inspect
manifest_inspect() {
  local package="$1"
  if [ -z "$package" ]; then
    log_error "E303" "manifest_inspect requires package name"
    return 1
  fi
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    log_error "E302" "Manifest $manifest_path missing"
    return 1
  fi
  jq -r --arg pkg "$package" '
    .packages[$pkg] // empty | to_entries[] | "\(.key): \(.value)"' "$manifest_path"
}

#--- manifest_summary
manifest_summary() {
  local manifest_path
  manifest_path="$(manifest_resolved_path)"
  if [ ! -f "$manifest_path" ]; then
    return 1
  fi
  local summary
  summary="$(jq -r '
    [
      .metadata.generated_at,
      .metadata.total_packages,
      .metadata.pacman_packages,
      .metadata.aur_packages,
      .metadata.local_packages,
      .metadata.unknown_packages,
      (.applications.flatpak.enabled // false),
      (.applications.flatpak.installed_count // 0),
      (.applications.fwupd.enabled // false),
      (.applications.fwupd.device_count // 0)
    ] | @tsv
  ' "$manifest_path" 2>/dev/null || true)"
  if [ -z "$summary" ]; then
    return 1
  fi
  local generated pkgs pac aur local_pkgs unknown flatpak_enabled flatpak_inst fwupd_enabled fwupd_devices
  IFS=$'\t' read -r generated pkgs pac aur local_pkgs unknown flatpak_enabled flatpak_inst fwupd_enabled fwupd_devices <<<"$summary"
  if [ "${FULL_PATH:-0}" = "1" ] || [ "$SYN_MANIFEST_PATH" = "$manifest_path" ]; then
    printf 'Manifest path: %s\n' "$manifest_path"
  else
    printf 'Manifest path: %s (resolved: %s)\n' "$SYN_MANIFEST_PATH" "$manifest_path"
  fi
  printf 'Generated at: %s\n' "$generated"
  printf 'Packages: %s (pacman: %s, aur: %s, local: %s, unknown: %s)\n' "$pkgs" "$pac" "$aur" "$local_pkgs" "$unknown"
  printf 'Flatpak recorded: %s (installed: %s)\n' "$flatpak_enabled" "$flatpak_inst"
  printf 'FWUPD recorded: %s (devices: %s)\n' "$fwupd_enabled" "$fwupd_devices"
}
