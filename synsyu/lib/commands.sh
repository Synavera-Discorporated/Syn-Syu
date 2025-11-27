#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/commands.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Command dispatchers and orchestration flows for Syn-Syu.
#
# Security / Safety Notes:
#   Invokes package managers and helpers; expects caller to
#   run as an unprivileged user with sudo for pacman actions.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Single-responsibility command functions
#   - Consistent logging and failure accounting
#------------------------------------------------------------

#--- matches_filters
matches_filters() {
  local name="$1"
  local matched=0 p
  if [ "${#INCLUDE_PATTERNS[@]}" -gt 0 ]; then
    matched=1
    for p in "${INCLUDE_PATTERNS[@]}"; do
      if [[ "$name" =~ $p ]]; then matched=0; break; fi
    done
    if [ $matched -ne 0 ]; then
      return 1
    fi
  fi
  for p in "${EXCLUDE_PATTERNS[@]}"; do
    if [[ "$name" =~ $p ]]; then
      return 1
    fi
  done
  return 0
}

#--- run_repo_batch
run_repo_batch() {
  local -a pkgs=("$@")
  [ "${#pkgs[@]}" -gt 0 ] || return 0
  local -a args=(-S)
  if [ "$NO_CONFIRM" = "1" ]; then
    args+=(--noconfirm)
  fi
  # Security: invokes pacman with sudo; limited to user-requested repo packages.
  sudo pacman "${args[@]}" "${pkgs[@]}"
}

#--- run_snapshot
run_snapshot() {
  local phase="$1"
  local cmd=""
  [ "$SNAPSHOTS_ENABLED" = "1" ] || return 0
  case "$phase" in
    pre) cmd="$SNAPSHOT_PRE_CMD" ;;
    post) cmd="$SNAPSHOT_POST_CMD" ;;
    *) return 0 ;;
  esac
  [ -n "$cmd" ] || return 0
  if [ "$DRY_RUN" = "1" ] && [ "$phase" = "pre" ]; then
    log_info "SNAPSHOT" "Dry-run mode: skipping snapshot command ($cmd)"
    return 0
  fi
  log_info "SNAPSHOT" "Executing $phase snapshot command"
  if ! bash -c "$cmd"; then
    log_error "SNAPSHOT" "Snapshot command for phase $phase failed"
    if [ "$SNAPSHOT_REQUIRE_SUCCESS" = "1" ]; then
      log_finalize
      exit 420
    fi
  fi
}

#--- dispatch_command
dispatch_command() {
  case "$COMMAND" in
    core)
      cmd_core
      ;;
    sync)
      cmd_sync
      ;;
    aur)
      NO_REPO=1
      cmd_sync
      ;;
    repo)
      NO_AUR=1
      cmd_sync
      ;;
    update)
      cmd_update "${COMMAND_ARGS[@]}"
      ;;
    group)
      cmd_group "${COMMAND_ARGS[@]}"
      ;;
    inspect)
      cmd_inspect "${COMMAND_ARGS[@]}"
      ;;
    check)
      cmd_check
      ;;
    clean)
      cmd_clean
      ;;
    log)
      cmd_log
      ;;
    export)
      cmd_export "${COMMAND_ARGS[@]}"
      ;;
    flatpak)
      cmd_flatpak
      ;;
    fwupd)
      cmd_fwupd
      ;;
    apps)
      cmd_apps
      ;;
    help)
      cmd_help
      ;;
    version)
      cmd_version
      ;;
    *)
      log_error "E102" "Unknown command $COMMAND"
      cmd_help
      exit 102
      ;;
  esac
}

#--- cmd_core
cmd_core() {
  if [ "$DRY_RUN" = "1" ]; then
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
      exit 301
    fi
    local args=("--manifest" "$SYN_MANIFEST_PATH" "--dry-run")
    if [ -n "$CONFIG_PATH" ] && [ -f "$CONFIG_PATH" ]; then
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
    "$core_bin" "${args[@]}" || exit $?
  else
    manifest_rebuild
  fi
}

#--- cmd_sync
cmd_sync() {
  FAILED_UPDATES=()
  manifest_require
  log_info "SYNC" "Commencing orchestrated upgrade"
  run_snapshot "pre"
  check_disk_space
  local helper
  helper="$(select_helper || true)"
  if [ -n "$AUR_HELPER" ]; then
    helper="$AUR_HELPER"
  fi
  if [ -z "$helper" ] && [ "$NO_AUR" = "0" ]; then
    log_warn "HELPER" "No AUR helper detected; AUR updates disabled"
    NO_AUR=1
  fi

  local total processed failed
  total=0
  processed=0
  failed=0
  declare -a repo_batch=()
  while IFS='|' read -r pkg source target; do
    [ -z "$pkg" ] && continue
    if ! matches_filters "$pkg"; then
      continue
    fi
    total=$((total + 1))
    if [ "$DRY_RUN" = "1" ]; then
      [ "$QUIET" = "1" ] || printf -- '-> [%d] %s via %s -> %s (dry-run)\n' "$total" "$pkg" "$source" "$target"
      continue
    fi
    case "$source" in
      PACMAN|Pacman|PACMAN)
        if ! ensure_package_disk_space "$pkg"; then
          failed=$((failed + 1))
          record_failed_update "$pkg" "disk check failed (see logs)"
          continue
        fi
        repo_batch+=("$pkg")
        if [ "${#repo_batch[@]}" -ge "$BATCH_SIZE" ]; then
          if ! run_repo_batch "${repo_batch[@]}"; then
            local status=$?
            log_warn "UPDATE" "Failed repo batch: ${repo_batch[*]}"
            for pkg in "${repo_batch[@]}"; do
              record_failed_update "$pkg" "pacman batch failed (exit $status)"
            done
            failed=$((failed + ${#repo_batch[@]}))
          else
            processed=$((processed + ${#repo_batch[@]}))
          fi
          repo_batch=()
        fi
        ;;
      *)
        if ! execute_update "$pkg" "$source" "$target" "$helper"; then
          log_warn "UPDATE" "Failed to update $pkg"
          failed=$((failed + 1))
        else
          processed=$((processed + 1))
        fi
        ;;
    esac
  done < <(manifest_updates_stream || true)

  if [ "${#repo_batch[@]}" -gt 0 ] && [ "$DRY_RUN" = "0" ]; then
    if ! run_repo_batch "${repo_batch[@]}"; then
      local status=$?
      log_warn "UPDATE" "Failed repo batch: ${repo_batch[*]}"
      for pkg in "${repo_batch[@]}"; do
        record_failed_update "$pkg" "pacman batch failed (exit $status)"
      done
      failed=$((failed + ${#repo_batch[@]}))
    else
      processed=$((processed + ${#repo_batch[@]}))
    fi
  fi

  if [ "$APPLICATIONS_FLATPAK" = "1" ]; then
    if ! run_flatpak_updates; then
      failed=$((failed + 1))
    fi
  fi
  if [ "$APPLICATIONS_FWUPD" = "1" ]; then
    if ! run_fwupd_updates; then
      failed=$((failed + 1))
    fi
  fi

  log_info "SUMMARY" "Updates processed=$processed failed=$failed"
  [ "$QUIET" = "1" ] || printf -- '-> System integrity sweep complete.\n'
  [ "$QUIET" = "1" ] || printf -- '-> Processed: %s (failed %s)\n' "$processed" "$failed"
  if [ "$DRY_RUN" = "1" ]; then
    [ "$QUIET" = "1" ] || printf -- '-> Dry-run completed; no changes applied.\n'
  fi
  [ "$QUIET" = "1" ] || printf -- '-> Log stored at: %s\n' "$LOG_PATH"
  if [ "$DRY_RUN" = "0" ]; then
    check_pacnew
    run_snapshot "post"
  fi
  print_failed_update_summary
}

#--- cmd_flatpak
cmd_flatpak() {
  FAILED_UPDATES=()
  log_info "FLATPAK" "Flatpak update command triggered"
  run_flatpak_updates || true
  print_failed_update_summary
}

#--- cmd_fwupd
cmd_fwupd() {
  FAILED_UPDATES=()
  log_info "FWUPD" "Firmware update command triggered"
  run_fwupd_updates || true
  print_failed_update_summary
}

#--- cmd_apps
cmd_apps() {
  FAILED_UPDATES=()
  run_flatpak_updates || true
  run_fwupd_updates || true
  print_failed_update_summary
}

#--- execute_update
execute_update() {
  local pkg="$1" source="$2" target="$3" helper="$4"
  case "$source" in
    PACMAN|Pacman|PACMAN)
      if [ "$NO_REPO" = "1" ]; then
        log_info "SKIP" "Repo updates disabled; skipping $pkg"
        return 0
      fi
      if ! ensure_package_disk_space "$pkg"; then
        record_failed_update "$pkg" "disk check failed (see logs)"
        return 1
      fi
      # Security: requires sudo to install repo package updates.
      local -a args=(-S)
      [ "$NO_CONFIRM" = "1" ] && args+=(--noconfirm)
      if ! sudo pacman "${args[@]}" "$pkg"; then
        local status=$?
        record_failed_update "$pkg" "pacman exited $status"
        return 1
      fi
      ;;
    AUR|Aur)
      if [ "$NO_AUR" = "1" ]; then
        log_info "SKIP" "AUR updates disabled; skipping $pkg"
        return 0
      fi
      if [ -z "$helper" ]; then
        log_error "E401" "No helper available for $pkg"
        record_failed_update "$pkg" "no helper available"
        return 1
      fi
      if ! ensure_package_disk_space "$pkg"; then
        record_failed_update "$pkg" "disk check failed (see logs)"
        return 1
      fi
      # Security: helper executes as invoking user; it will escalate internally if needed.
      local -a hargs=(-S)
      [ "$NO_CONFIRM" = "1" ] && hargs+=(--noconfirm)
      if ! "$helper" "${hargs[@]}" "$pkg"; then
        local status=$?
        record_failed_update "$pkg" "$helper exited $status"
        return 1
      fi
      ;;
    LOCAL|Local)
      log_info "SKIP" "Package $pkg managed locally"
      return 0
      ;;
    *)
      log_warn "SKIP" "Unknown source $source for $pkg"
      return 0
      ;;
  esac
}

#--- cmd_update
cmd_update() {
  FAILED_UPDATES=()
  if [ $# -eq 0 ]; then
    log_error "E201" "update requires at least one package"
    exit 201
  fi
  REBUILD_MANIFEST=1
  manifest_require
  declare -A requested=()
  local pkg
  for pkg in "$@"; do
    requested["$pkg"]=1
    log_info "SELECT" "Targeting $pkg"
  done
  local helper
  helper="$(select_helper || true)"
  if [ -n "$AUR_HELPER" ]; then
    helper="$AUR_HELPER"
  fi
  local processed=0 failed=0
  while IFS='|' read -r pkg source target flag; do
    [ -z "$pkg" ] && continue
    if ! matches_filters "$pkg"; then
      continue
    fi
    if [ -z "${requested[$pkg]:-}" ]; then
      continue
    fi
    if [ "$flag" != "true" ]; then
      log_info "SKIP" "No update available for $pkg"
      continue
    fi
    if [ "$DRY_RUN" = "1" ]; then
      [ "$QUIET" = "1" ] || printf -- '-> %s via %s -> %s (dry-run)\n' "$pkg" "$source" "$target"
      continue
    fi
    if ! execute_update "$pkg" "$source" "$target" "$helper"; then
      failed=$((failed + 1))
    else
      processed=$((processed + 1))
    fi
  done < <(manifest_packages_stream || true)
  log_info "SUMMARY" "Updates processed=$processed failed=$failed"
  print_failed_update_summary
}

#--- cmd_group
cmd_group() {
  local group="$1"
  if [ -z "$group" ]; then
    log_error "E202" "group command requires a group name"
    exit 202
  fi
  if [ ! -f "$GROUPS_PATH" ]; then
    log_error "E203" "Group configuration missing at $GROUPS_PATH"
    exit 203
  fi
  local packages
  packages="$(GROUPS_PATH="$GROUPS_PATH" GROUP_NAME="$group" python3 - <<'PY'
import json
import os
import sys
import tomllib

groups_path = os.environ.get("GROUPS_PATH")
target = os.environ.get("GROUP_NAME")

try:
    with open(groups_path, "rb") as handle:
        data = tomllib.load(handle)
except FileNotFoundError:
    sys.exit(1)

group = data.get(target)
if not group:
    sys.exit(2)

print(" ".join(group))
PY
  )"
  case $? in
    0)
      ;;
    1)
      log_error "E204" "Failed to read $GROUPS_PATH"
      exit 204
      ;;
    2)
      log_error "E205" "Unknown group $group"
      exit 205
      ;;
  esac
  if [ -z "$packages" ]; then
    log_warn "GROUP" "Group $group has no packages"
    return 0
  fi
  cmd_update $packages
}

#--- cmd_inspect
cmd_inspect() {
  local pkg="$1"
  if [ -z "$pkg" ]; then
    log_error "E301" "inspect requires a package name"
    exit 301
  fi
  manifest_require
  log_info "INSPECT" "Inspecting $pkg"
  local output
  if [ "$JSON_OUTPUT" = "1" ]; then
    output="$(jq -c --arg pkg "$pkg" '.packages[$pkg] // {}' "$SYN_MANIFEST_PATH" 2>/dev/null || true)"
  else
    output="$(manifest_inspect "$pkg" || true)"
  fi
  if [ -z "$output" ]; then
    printf 'No manifest data for %s\n' "$pkg"
  else
    printf '%s\n' "$output"
  fi
}

#--- cmd_check
cmd_check() {
  manifest_require
  if [ "$JSON_OUTPUT" = "1" ]; then
    jq -c '{metadata: .metadata, updates: (.packages | to_entries | map(select(.value.update_available==true) | {key, value}))}' "$SYN_MANIFEST_PATH" || true
  else
    printf -- '-> Manifest summary\n'
    manifest_summary || log_warn "MANIFEST" "Unable to summarize manifest"

    printf '\n-> Package update details\n'
    local package_updates
    package_updates="$(manifest_update_details || true)"
    if [ -n "$package_updates" ]; then
      while IFS=$'\t' read -r name source installed newer; do
        [ -z "$name" ] && continue
        printf ' - %s [%s]: %s -> %s\n' "$name" "$source" "${installed:-?}" "${newer:-?}"
      done <<<"$package_updates"
    else
      printf ' - None\n'
    fi

    printf '\n-> Application updates\n'
    local app_updates
    app_updates="$(manifest_application_update_details || true)"
    if [ -n "$app_updates" ]; then
      while IFS=$'\t' read -r kind detail; do
        [ -z "$kind" ] && continue
        printf ' - %s: %s\n' "$kind" "$detail"
      done <<<"$app_updates"
    else
      printf ' - None\n'
    fi
  fi
}

#--- cmd_export
cmd_export() {
  local format="json"
  local output=""
  local include_repo=1
  local include_aur=1
  while [ $# -gt 0 ]; do
    case "$1" in
      --format)
        format="$2"
        shift 2
        ;;
      --output|-o)
        output="$2"
        shift 2
        ;;
      --repo-only)
        include_aur=0
        shift
        ;;
      --aur-only)
        include_repo=0
        shift
        ;;
      --json)
        format="json"
        shift
        ;;
      --plain)
        format="plain"
        shift
        ;;
      *)
        log_error "EXPORT" "Unknown option $1"
        return 1
        ;;
    esac
  done

  if [ "$include_repo" -eq 0 ] && [ "$include_aur" -eq 0 ]; then
    include_repo=1
    include_aur=1
  fi

  local repo_list="" aur_list=""
  if [ "$include_repo" -eq 1 ]; then
    repo_list="$(pacman -Qqen 2>/dev/null || true)"
  fi
  if [ "$include_aur" -eq 1 ]; then
    aur_list="$(pacman -Qqem 2>/dev/null || true)"
  fi

  local data=""
  case "$format" in
    json|JSON)
      local host="${HOSTNAME:-$(uname -n)}"
      data="$(jq -n --arg repo "$repo_list" --arg aur "$aur_list" --arg host "$host" '{
        generated_at: (now | strftime("%Y-%m-%dT%H:%M:%SZ")),
        host: $host,
        repo: ($repo | split("\n") | map(select(length>0))),
        aur: ($aur | split("\n") | map(select(length>0)))
      }')"
      ;;
    plain|text)
      local timestamp
      timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
      data+=$'# Syn-Syu package export\n'
      data+="# Generated at: $timestamp\n"
      if [ "$include_repo" -eq 1 ]; then
        data+=$'\n[repo]\n'
        if [ -n "$repo_list" ]; then
          data+="$repo_list\n"
        fi
      fi
      if [ "$include_aur" -eq 1 ]; then
        data+=$'\n[aur]\n'
        if [ -n "$aur_list" ]; then
          data+="$aur_list\n"
        fi
      fi
      ;;
    *)
      log_error "EXPORT" "Unknown format: $format"
      return 1
      ;;
  esac

  if [ -n "$output" ]; then
    printf '%s' "$data" >"$output"
    log_info "EXPORT" "Wrote export to $output"
  else
    printf '%s' "$data"
    if [ "$format" = "json" ] || [ "$format" = "JSON" ]; then
      printf '\n'
    fi
  fi
}

#--- cmd_clean
cmd_clean() {
  log_info "CLEAN" "Pruning cache and orphans"
  if command -v paccache >/dev/null 2>&1; then
    if ! sudo paccache -rk "$CLEAN_KEEP_VERSIONS"; then
      log_warn "CLEAN" "paccache failed; falling back to pacman -Sc"
      # Security: limited to cache pruning via sudo pacman -Sc, no package installs/removals.
      sudo pacman -Sc --noconfirm || log_warn "CLEAN" "Failed to prune pacman cache"
    fi
  else
    log_warn "CLEAN" "paccache not available; using pacman -Sc"
    sudo pacman -Sc --noconfirm || log_warn "CLEAN" "Failed to prune pacman cache"
  fi

  if [ "$CLEAN_REMOVE_ORPHANS" = "1" ]; then
    local orphan_file="/tmp/syn-syu_orphans.txt"
    sudo pacman -Qtdq >"$orphan_file" 2>/dev/null || true
    if [ -s "$orphan_file" ]; then
      mapfile -t _syn_syu_orphans <"$orphan_file"
      if [ "${#_syn_syu_orphans[@]}" -gt 0 ] && sudo pacman -Rns --noconfirm "${_syn_syu_orphans[@]}"; then
        log_info "CLEAN" "Removed orphaned packages"
      else
        log_warn "CLEAN" "Failed to remove orphaned packages"
      fi
      unset _syn_syu_orphans
    else
      log_info "CLEAN" "No orphaned packages detected"
    fi
    rm -f "$orphan_file"
    rm -f /tmp/synsyu_orphans.txt
  fi

  if [ -d "$LOG_DIR" ]; then
    find "$LOG_DIR" -maxdepth 1 -type f -name 'installer_*.log*' -mtime +30 -delete 2>/dev/null || true
  fi
}

#--- cmd_log
cmd_log() {
  local dir="${LOG_DIR:-$HOME/.local/share/syn-syu}"
  if [ ! -d "$dir" ]; then
    printf 'No logs found in %s\n' "$dir"
    return 0
  fi
  ls -1t "$dir"/*.log 2>/dev/null | head -n 10
}

#--- cmd_help
cmd_help() {
  cat <<'EOF'
Syn-Syu — Conscious package orchestration

Usage: syn-syu [flags] <command> [args]

Commands:
  sync              Update all packages per manifest
  core              Rebuild manifest via synsyu_core
  aur               Update only AUR packages
  repo              Update only repo packages
  flatpak           Apply Flatpak application updates
  fwupd             Apply firmware updates via fwupdmgr
  apps              Apply both Flatpak and firmware updates
  update <pkgs...>  Update specific packages
  group <name>      Update package group defined in groups.toml
  inspect <pkg>     Show manifest detail for package
  check             Summarize manifest contents
  clean             Prune caches and remove orphans
  log               List recent Syn-Syu log files
  export            Export package lists for replication
  help              Display this help message
  version           Show version information

Flags:
  --config <path>   Use alternate configuration file
  --manifest <path> Override manifest location
  --rebuild         Force manifest rebuild before command
  --dry-run         Simulate actions without applying
  --no-aur          Disable AUR operations
  --no-repo         Disable repo operations
  --verbose         Stream logs to stderr
  --groups <path>   Override group configuration path
  --quiet, -q       Suppress non-essential output
  --json            JSON output for check/inspect
  --confirm         Ask for confirmation in helpers (drop --noconfirm)
  --noconfirm       Force non-interactive operations (default)
  --helper <name>   Force a specific AUR helper
  --include <regex> Include only packages matching regex (repeatable)
  --exclude <regex> Exclude packages matching regex (repeatable)
  --min-free-gb <N> Override required free space buffer in gigabytes
  --batch <N>       Batch size for repo installs (default from config or 10)
  --with-flatpak    Include Flatpak updates in manifest and sync
  --no-flatpak      Skip Flatpak updates (overrides config/manifest)
  --with-fwupd      Include firmware updates in manifest and sync
  --no-fwupd        Skip firmware updates (overrides config/manifest)
EOF
}

#--- cmd_version
cmd_version() {
  printf 'Syn-Syu orchestrator 0.12\n'
}
