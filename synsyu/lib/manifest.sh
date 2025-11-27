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

  manifest_update_applications_section
}

#--- manifest_update_applications_section
manifest_update_applications_section() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
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
    | .metadata.application_updates = {
        flatpak: ($apps.flatpak.update_count // 0),
        fwupd: ($apps.fwupd.update_count // 0)
      }
  ' "$SYN_MANIFEST_PATH" >"$tmp_file"; then
    rm -f "$tmp_file"
    log_warn "MANIFEST" "Failed to append application data to manifest"
    return 0
  fi
  mv "$tmp_file" "$SYN_MANIFEST_PATH"

  local flatpak_enabled fwupd_enabled flatpak_count fwupd_count
  flatpak_enabled="$(printf '%s' "$apps_json" | jq -r '.flatpak.enabled // false' 2>/dev/null || echo "false")"
  fwupd_enabled="$(printf '%s' "$apps_json" | jq -r '.fwupd.enabled // false' 2>/dev/null || echo "false")"
  flatpak_count="$(printf '%s' "$apps_json" | jq -r '.flatpak.update_count // 0' 2>/dev/null || echo "0")"
  fwupd_count="$(printf '%s' "$apps_json" | jq -r '.fwupd.update_count // 0' 2>/dev/null || echo "0")"
  log_info "MANIFEST" "Application intent recorded: flatpak=${flatpak_enabled:-false} (updates ${flatpak_count:-0}), fwupd=${fwupd_enabled:-false} (updates ${fwupd_count:-0})"
}

#--- manifest_collect_applications_json
manifest_collect_applications_json() {
  local flatpak_json fwupd_json
  flatpak_json="$(manifest_collect_flatpak_updates)" || flatpak_json=""
  fwupd_json="$(manifest_collect_fwupd_updates)" || fwupd_json=""

  if [ -z "$flatpak_json" ]; then
    flatpak_json='{"enabled":false,"available":false,"update_count":0,"updates":[]}'
  fi
  if [ -z "$fwupd_json" ]; then
    fwupd_json='{"enabled":false,"available":false,"update_count":0,"updates":[]}'
  fi

  jq -n --argjson flatpak "$flatpak_json" --argjson fwupd "$fwupd_json" '{flatpak: $flatpak, fwupd: $fwupd}'
}

#--- manifest_collect_flatpak_updates
manifest_collect_flatpak_updates() {
  if [ "${APPLICATIONS_FLATPAK:-0}" != "1" ]; then
    jq -n '{enabled: false, available: false, update_count: 0, updates: [], note: "flatpak disabled"}'
    return 0
  fi
  if ! command -v flatpak >/dev/null 2>&1; then
    log_warn "FLATPAK" "Flatpak requested for manifest but binary not present"
    jq -n '{enabled: true, available: false, update_count: 0, updates: [], note: "flatpak not installed"}'
    return 0
  fi

  local output parsed
  output="$(flatpak remote-ls --updates --columns=application,branch,origin 2>/dev/null || true)"
  parsed="$(printf '%s\n' "$output" | python3 - <<'PY' 2>/dev/null || true
import json
import sys

lines = [ln.strip() for ln in sys.stdin.read().splitlines() if ln.strip()]
updates = []
for line in lines:
    lower = line.lower()
    if line.startswith("application") and "branch" in lower and "origin" in lower:
        continue
    parts = line.split()
    if len(parts) >= 3:
        updates.append({"application": parts[0], "branch": parts[1], "origin": parts[2]})
    else:
        updates.append({"raw": line})

print(json.dumps({
    "enabled": True,
    "available": True,
    "updates": updates,
    "update_count": len(updates)
}))
PY
)"
  if [ -z "$parsed" ]; then
    parsed='{"enabled": true, "available": true, "updates": [], "update_count": 0}'
  fi
  printf '%s' "$parsed"
}

#--- manifest_collect_fwupd_updates
manifest_collect_fwupd_updates() {
  if [ "${APPLICATIONS_FWUPD:-0}" != "1" ]; then
    jq -n '{enabled: false, available: false, update_count: 0, updates: [], note: "fwupd disabled"}'
    return 0
  fi
  if ! command -v fwupdmgr >/dev/null 2>&1; then
    log_warn "FWUPD" "Firmware updates requested for manifest but fwupdmgr not present"
    jq -n '{enabled: true, available: false, update_count: 0, updates: [], note: "fwupdmgr not installed"}'
    return 0
  fi

  local parsed json_output
  json_output="$(fwupdmgr get-updates --json 2>/dev/null || true)"
  if [ -n "$json_output" ]; then
    parsed="$(printf '%s' "$json_output" | python3 - <<'PY' 2>/dev/null || true
import json
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(1)

devices = data.get("Devices") or data.get("devices") or []
updates = []
for dev in devices:
    name = dev.get("Name") or dev.get("DeviceId") or dev.get("Device") or dev.get("Id") or ""
    releases = dev.get("Releases") or dev.get("releases") or []
    for rel in releases:
        updates.append({
            "device": name,
            "version": rel.get("Version") or rel.get("version") or "",
            "title": rel.get("Title") or rel.get("AppstreamId") or rel.get("Description") or ""
        })

print(json.dumps({
    "enabled": True,
    "available": True,
    "updates": updates,
    "update_count": len(updates)
}))
PY
)"
  fi

  if [ -z "$parsed" ]; then
    local text_output
    text_output="$(fwupdmgr get-updates 2>/dev/null || true)"
    parsed="$(printf '%s\n' "$text_output" | python3 - <<'PY' 2>/dev/null || true
import json
import sys

lines = [ln.strip() for ln in sys.stdin.read().splitlines() if ln.strip()]
print(json.dumps({
    "enabled": True,
    "available": True,
    "updates": [{"raw": ln} for ln in lines],
    "update_count": len(lines)
}))
PY
)"
  fi

  if [ -z "$parsed" ]; then
    parsed='{"enabled": true, "available": true, "updates": [], "update_count": 0}'
  fi
  printf '%s' "$parsed"
}

#--- manifest_apply_application_flags
manifest_apply_application_flags() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
    return 0
  fi

  local flatpak_enabled fwupd_enabled
  flatpak_enabled="$(jq -r '.applications.flatpak.enabled // empty' "$SYN_MANIFEST_PATH" 2>/dev/null || true)"
  fwupd_enabled="$(jq -r '.applications.fwupd.enabled // empty' "$SYN_MANIFEST_PATH" 2>/dev/null || true)"

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

#--- manifest_update_details
manifest_update_details() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
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
  ' "$SYN_MANIFEST_PATH"
}

#--- manifest_application_update_details
manifest_application_update_details() {
  if [ ! -f "$SYN_MANIFEST_PATH" ]; then
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
  ' "$SYN_MANIFEST_PATH"
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
      (.metadata.space_checked_path // ""),
      (.applications.flatpak.enabled // false),
      (.applications.flatpak.update_count // 0),
      (.applications.fwupd.enabled // false),
      (.applications.fwupd.update_count // 0)
    ] | @tsv
  ' "$SYN_MANIFEST_PATH" 2>/dev/null || true)"
  if [ -z "$summary" ]; then
    return 1
  fi
  local pkgs repo aur updates dl build inst trans buf req avail path flatpak_enabled flatpak_updates fwupd_enabled fwupd_updates
  IFS=$'\t' read -r pkgs repo aur updates dl build inst trans buf req avail path flatpak_enabled flatpak_updates fwupd_enabled fwupd_updates <<<"$summary"
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
  printf 'Flatpak in manifest: %s (updates: %s)\n' "$flatpak_enabled" "$flatpak_updates"
  printf 'FWUPD in manifest: %s (updates: %s)\n' "$fwupd_enabled" "$fwupd_updates"
}
