#!/usr/bin/env bash
#============================================================
# Synavera Project: Syn-Syu
# Module: tools/install_syn-syu.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Interactive installer wizard that builds and deploys
#   synsyu_core (Rust) and syn-syu (Bash), copies required
#   library modules, and prepares configuration files.
#
# Security / Safety Notes:
#   - Invokes sudo when installing into privileged prefixes.
#   - Offers an "It's my system" advanced mode for operators
#     who prefer to manage dependencies manually.
#
# Dependencies:
#   bash, install, mkdir, cargo, rustc, jq, python3, pacman (for
#   optional dependency installation).
#
# Operational Scope:
#   Execute from the repository root to perform guided or
#   advanced installation of the Syn-Syu toolchain.
#
# Revision History:
#   2025-10-28 COD  Created initial installer wizard.
#------------------------------------------------------------
# SSE Principles Observed:
#   - set -euo pipefail for predictable behaviour
#   - Timestamped logging with SHA-256 digest
#   - Explicit operator prompts for privileged actions
#============================================================

set -euo pipefail
IFS=$'\n\t'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly PROJECT_ROOT

SESSION_STAMP="$(date -u +"%Y-%m-%d_%H-%M-%S")"
readonly SESSION_STAMP

LOG_DIR="${HOME}/.local/share/syn-syu/install"
LOG_PATH="${LOG_DIR}/installer_${SESSION_STAMP}.log"
LOG_VERBOSE=1
BUILD_SOURCE_DESC="unknown"

# install verification
MISSING_INSTALL_ITEMS=()

# CLI flags
NON_INTERACTIVE=0
MODE_CLI=""
OVERWRITE_POLICY=""
CLI_NO_SUDO=0

# runtime configuration defaults
INSTALL_MODE=""
INSTALL_DEPS=1
CREATE_CONFIG=0
CREATE_GROUPS=0
SUDO_CMD="sudo"
MANIFEST_OFFLINE=0
HELPER_CANDIDATES=(paru yay pikaur trizen aura pacaur pamac aurman pakku)
HELPER_PRIORITY=(paru yay pikaur trizen)
DETECTED_HELPERS=()

#--- detect_helpers
detect_helpers() {
  DETECTED_HELPERS=()
  local candidate
  for candidate in "${HELPER_CANDIDATES[@]}"; do
    if command -v "$candidate" >/dev/null 2>&1; then
      DETECTED_HELPERS+=("$candidate")
    fi
  done
  if [ "${#DETECTED_HELPERS[@]}" -gt 0 ]; then
    log_info "HELPER" "Detected AUR helpers: ${DETECTED_HELPERS[*]}"
  else
    log_info "HELPER" "No AUR helpers detected."
  fi
}

#--- log_init
log_init() {
  if ! mkdir -p "$LOG_DIR" 2>/dev/null; then
    LOG_DIR="/tmp/syn-syu/install"
    mkdir -p "$LOG_DIR" 2>/dev/null || LOG_DIR="/tmp"
    LOG_PATH="${LOG_DIR}/installer_${SESSION_STAMP}.log"
  fi
  : >"$LOG_PATH"
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

log_info() {
  log_event "INFO" "$1" "$2"
}

log_warn() {
  log_event "WARN" "$1" "$2"
}

log_error() {
  log_event "ERROR" "$1" "$2"
}

log_init

#--- log_finalize
log_finalize() {
  if [ -s "$LOG_PATH" ]; then
    sha256sum "$LOG_PATH" >"${LOG_PATH}.hash"
  fi
}

trap 'log_finalize' EXIT

#--- usage
usage() {
  cat <<'EOF'
Syn-Syu Installer Wizard
Usage: install_syn-syu.sh [options]

Options:
  --mode <guided|advanced>    Choose installer mode without prompt
  --policy <overwrite|backup|skip>
                              Overwrite policy for existing files
  --no-sudo                   Do not use sudo for privileged ops
  --non-interactive, --yes    Suppress interactive prompts where possible
  -h, --help                  Show this help
EOF
}

#--- parse_args
parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --mode)
        MODE_CLI="${2:-}"; shift 2 ;;
      --policy)
        OVERWRITE_POLICY="${2:-}"; shift 2 ;;
      --no-sudo)
        CLI_NO_SUDO=1; shift ;;
      --non-interactive|--yes)
        NON_INTERACTIVE=1; shift ;;
      -h|--help)
        usage; exit 0 ;;
      *)
        log_error "ARGS" "Unknown option $1"; usage; exit 1 ;;
    esac
  done
}

#--- prompt_menu
prompt_menu() {
  local prompt="$1"
  shift
  local options=("$@")
  local selection=""
  while true; do
    printf '\n%s\n' "$prompt"
    local idx=1
    for entry in "${options[@]}"; do
      printf '  [%d] %s\n' "$idx" "$entry"
      idx=$((idx + 1))
    done
    printf '> '
    read -r selection
    if [ -n "$selection" ] && [ "$selection" -ge 1 ] && [ "$selection" -le "${#options[@]}" ] 2>/dev/null; then
      printf '%s' "$selection"
      return 0
    fi
    log_warn "PROMPT" "Invalid selection: $selection"
  done
}

#--- prompt_default
prompt_default() {
  local question="$1" default="$2"
  local reply
  printf '%s [%s]: ' "$question" "$default"
  read -r reply
  if [ -z "$reply" ]; then
    printf '%s' "$default"
  else
    printf '%s' "$reply"
  fi
}

#--- prompt_yes_no
prompt_yes_no() {
  local question="$1" default="${2:-Y}"
  local reply
  local options="[Y/n]"
  if [ "$default" = "N" ] || [ "$default" = "n" ]; then
    options="[y/N]"
  fi
  while true; do
    printf '%s %s ' "$question" "$options"
    read -r reply
    reply="${reply:-$default}"
    case "$reply" in
      [Yy]) return 0 ;;
      [Nn]) return 1 ;;
      *) printf 'Please answer y or n.\n' ;;
    esac
  done
}

#--- expand_path
expand_path() {
  local path="$1"
  if [ -z "$path" ]; then
    printf '%s' "$path"
    return
  fi
  case "$path" in
    ~/*) printf '%s/%s' "$HOME" "${path#~/}" ;;
    ~) printf '%s' "$HOME" ;;
    *) printf '%s' "$path" ;;
  esac
}

#--- ensure_repo_root
ensure_repo_root() {
  if [ ! -d "$PROJECT_ROOT/synsyu" ] || [ ! -f "$PROJECT_ROOT/synsyu/syn-syu" ]; then
    log_error "ROOT" "Installer must be executed from the repository checkout."
    exit 200
  fi
}

#--- detect_build_source
detect_build_source() {
  BUILD_SOURCE_DESC="unknown"
  if command -v git >/dev/null 2>&1; then
    if git -C "$PROJECT_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      local commit dirty_flag
      commit="$(git -C "$PROJECT_ROOT" rev-parse --short=12 HEAD 2>/dev/null || true)"
      dirty_flag="clean"
      if [ -n "$(git -C "$PROJECT_ROOT" status --porcelain 2>/dev/null)" ]; then
        dirty_flag="dirty"
      fi
      if [ -n "$commit" ]; then
        BUILD_SOURCE_DESC="git commit ${commit} (${dirty_flag})"
        log_info "SOURCE" "Building from ${BUILD_SOURCE_DESC}"
        return 0
      fi
    fi
  fi

  if [ -f "$PROJECT_ROOT/PKGBUILD" ]; then
    local pkgver pkgrel
    pkgver="$(sed -n 's/^pkgver=//p' "$PROJECT_ROOT/PKGBUILD" | head -n1)"
    pkgrel="$(sed -n 's/^pkgrel=//p' "$PROJECT_ROOT/PKGBUILD" | head -n1)"
    if [ -n "$pkgver" ]; then
      BUILD_SOURCE_DESC="PKGBUILD pkgver=${pkgver}${pkgrel:+-${pkgrel}} (likely AUR release)"
      log_info "SOURCE" "Building from ${BUILD_SOURCE_DESC}"
      return 0
    fi
  fi

  log_warn "SOURCE" "Could not determine build source (no git metadata or PKGBUILD version found)."
}

#--- require_command
require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    log_error "CMD" "Required command '$cmd' not found in PATH."
    return 1
  fi
}

#--- detect_missing_tools
detect_missing_tools() {
  local -n _missing_ref=$1
  local tools=("cargo" "rustc" "jq" "python3" "install")
  _missing_ref=()
  local tool
  for tool in "${tools[@]}"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      _missing_ref+=("$tool")
    fi
  done
}

#--- choose_mode
choose_mode() {
  log_info "INIT" "Syn-Syu installer wizard started."
  printf '\nSyn-Syu Installation Wizard\n'
  printf '===========================\n'
  printf 'This wizard will build synsyu_core, install binaries and\n'
  printf 'supporting scripts, and optionally create configuration files.\n\n'
  detect_helpers || true
  if [ -n "$MODE_CLI" ]; then
    case "$MODE_CLI" in
      guided|advanced) INSTALL_MODE="$MODE_CLI"; log_info "MODE" "Mode via CLI: $INSTALL_MODE"; return 0 ;;
      *) log_error "MODE" "Invalid --mode: $MODE_CLI"; exit 1 ;;
    esac
  fi

  printf 'Choose your path:\n'
  printf '  1. Guided setup (recommended)\n'
  printf "  2. \"It's my system\" advanced setup\n\n"
  local selection
  while true; do
    printf 'Enter selection [1-2]: '
    read -r selection
    case "$selection" in
      1) INSTALL_MODE="guided"; return 0 ;;
      2) INSTALL_MODE="advanced"; return 0 ;;
      *) printf 'Please choose 1 or 2.\n' ;;
    esac
  done
}

#--- configure_guided
configure_guided() {
  INSTALL_PREFIX="/usr/local"
  BIN_DIR="${INSTALL_PREFIX}/bin"
  LIB_DIR="${INSTALL_PREFIX}/share/syn-syu"
  CONFIG_DIR="${HOME}/.config/syn-syu"
  GROUPS_FILE="${CONFIG_DIR}/groups.toml"
  CREATE_CONFIG=1
  CREATE_GROUPS=0
  SUDO_CMD="sudo"
  INSTALL_DEPS=1
  log_info "MODE" "Guided setup selected."
}

#--- configure_advanced
configure_advanced() {
  log_info "MODE" "Advanced setup selected."
  INSTALL_PREFIX="$(prompt_default 'Install prefix' '/usr/local')"
  INSTALL_PREFIX="$(expand_path "$INSTALL_PREFIX")"
  BIN_DIR="$(prompt_default 'Binary directory' "${INSTALL_PREFIX}/bin")"
  BIN_DIR="$(expand_path "$BIN_DIR")"
  LIB_DIR="$(prompt_default 'Library directory (for .sh modules)' "${INSTALL_PREFIX}/share/syn-syu")"
  LIB_DIR="$(expand_path "$LIB_DIR")"
  CONFIG_DIR="$(prompt_default 'Config directory' "${HOME}/.config/syn-syu")"
  CONFIG_DIR="$(expand_path "$CONFIG_DIR")"
  GROUPS_FILE="$(prompt_default 'Groups file path' "${CONFIG_DIR}/groups.toml")"
  GROUPS_FILE="$(expand_path "$GROUPS_FILE")"
  if prompt_yes_no "Install example config.toml?" "Y"; then
    CREATE_CONFIG=1
  else
    CREATE_CONFIG=0
  fi
  if prompt_yes_no "Create groups.toml scaffold?" "N"; then
    CREATE_GROUPS=1
  else
    CREATE_GROUPS=0
  fi
  if prompt_yes_no "Attempt to install missing dependencies automatically?" "N"; then
    INSTALL_DEPS=1
  else
    INSTALL_DEPS=0
  fi
  if [ "$CLI_NO_SUDO" = "1" ]; then
    log_info "MODE" "--no-sudo flag set; skipping sudo usage in advanced mode."
    SUDO_CMD=""
  else
    if prompt_yes_no "Use sudo for privileged operations?" "Y"; then
      SUDO_CMD="sudo"
    else
      SUDO_CMD=""
    fi
  fi
}

#--- install_missing_dependencies
install_missing_dependencies() {
  local -a missing_cmds=("$@")
  if [ "${#missing_cmds[@]}" -eq 0 ]; then
    return 0
  fi
  if [ -z "${SUDO_CMD:-}" ]; then
    log_warn "DEPS" "Cannot auto-install packages (${missing_cmds[*]}) without sudo; install manually."
    return 1
  fi
  if ! command -v pacman >/dev/null 2>&1; then
    log_warn "DEPS" "Missing commands: ${missing_cmds[*]}. pacman not available; install manually."
    return 1
  fi
  declare -A pkg_map=(
    [cargo]="rust"
    [rustc]="rust"
    [jq]="jq"
    [python3]="python"
  )
  local -a packages=()
  local tool pkg
  for tool in "${missing_cmds[@]}"; do
    pkg="${pkg_map[$tool]}"
    if [ -n "$pkg" ]; then
      packages+=("$pkg")
    fi
  done
  if [ "${#packages[@]}" -eq 0 ]; then
    log_info "DEPS" "No mapped pacman packages to install."
    return 0
  fi
  log_info "DEPS" "Installing packages: ${packages[*]}"
  execute_cmd pacman -S --needed "${packages[@]}"
}

#--- execute_cmd
execute_cmd() {
  local -a cmd=("$@")
  if [ -n "${SUDO_CMD:-}" ]; then
    log_info "EXEC" "Running: $SUDO_CMD ${cmd[*]}"
    "$SUDO_CMD" "${cmd[@]}"
  else
    log_info "EXEC" "Running: ${cmd[*]}"
    "${cmd[@]}"
  fi
}

#--- execute_local
execute_local() {
  log_info "EXEC" "Running: $*"
  "$@"
}

#--- build_synsyu_core
build_synsyu_core() {
  log_info "BUILD" "Building synsyu_core in release mode."
  export SYN_SYU_BUILD_SOURCE="${SYN_SYU_BUILD_SOURCE:-git}"
  if git -C "$PROJECT_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    export SYN_SYU_GIT_COMMIT="${SYN_SYU_GIT_COMMIT:-$(git -C "$PROJECT_ROOT" rev-parse --short=12 HEAD 2>/dev/null || true)}"
  fi
  export SYN_SYU_BUILD_TIME="${SYN_SYU_BUILD_TIME:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}"
  if [ -n "${SYN_SYU_BUILD_SOURCE:-}" ] || [ -n "${SYN_SYU_GIT_COMMIT:-}" ]; then
    log_info "SOURCE" "Build source=${SYN_SYU_BUILD_SOURCE:-unknown} git=${SYN_SYU_GIT_COMMIT:-unknown} built=${SYN_SYU_BUILD_TIME:-unknown}"
  fi
  (cd "$PROJECT_ROOT/synsyu_core" && execute_local cargo build --release --locked)
}

#--- install helpers with overwrite policy

backup_target() {
  local target="$1"
  [ -e "$target" ] || return 0
  local bak="${target}.bak_${SESSION_STAMP}"
  if can_write "$target" || [ -z "${SUDO_CMD:-}" ]; then
    log_info "BACKUP" "Moving $target -> $bak"
    mv -f -- "$target" "$bak"
  else
    log_info "BACKUP" "Moving (sudo) $target -> $bak"
    "$SUDO_CMD" mv -f -- "$target" "$bak"
  fi
}

install_with_policy() {
  # usage: install_with_policy <mode> <src> <dest>
  local mode="$1" src="$2" dest="$3"
  local dest_dir
  dest_dir="$(dirname "$dest")"
  # ensure dest dir exists
  execute_cmd install -d "$dest_dir"
  # apply policy
  if [ -e "$dest" ]; then
    case "$OVERWRITE_POLICY" in
      skip)
        log_warn "SKIP" "Preserving existing $dest"
        return 0
        ;;
      backup)
        backup_target "$dest"
        ;;
      overwrite|"")
        : # default overwrite via install
        ;;
      *)
        log_error "POLICY" "Unknown overwrite policy: $OVERWRITE_POLICY"
        return 1
        ;;
    esac
  fi
  case "$mode" in
    755)
      execute_cmd install -Dm755 "$src" "$dest"
      ;;
    644)
      execute_cmd install -Dm644 "$src" "$dest"
      ;;
    *)
      log_error "MODE" "Unsupported install mode $mode for $dest"
      return 1
      ;;
  esac
}

#--- verify_installation_files
verify_installation_files() {
  MISSING_INSTALL_ITEMS=()
  local expected=(
    "$BIN_DIR/synsyu_core"
    "$BIN_DIR/syn-syu"
    "$LIB_DIR/logging.sh"
    "$LIB_DIR/helpers.sh"
    "$LIB_DIR/manifest.sh"
    "$LIB_DIR/common.sh"
    "$LIB_DIR/config.sh"
    "$LIB_DIR/cli.sh"
    "$LIB_DIR/disk.sh"
    "$LIB_DIR/apps.sh"
    "$LIB_DIR/plan.sh"
    "$LIB_DIR/commands.sh"
  )
  local p
  for p in "${expected[@]}"; do
    if [ ! -e "$p" ]; then
      MISSING_INSTALL_ITEMS+=("$p")
    fi
  done
  if [ "${#MISSING_INSTALL_ITEMS[@]}" -eq 0 ]; then
    return 0
  fi
  return 1
}

#--- deploy_binaries
deploy_binaries() {
  install_with_policy 755 "$PROJECT_ROOT/synsyu_core/target/release/synsyu_core" "$BIN_DIR/synsyu_core"
  install_with_policy 755 "$PROJECT_ROOT/synsyu/syn-syu" "$BIN_DIR/syn-syu"
}

#--- deploy_libraries
deploy_libraries() {
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/logging.sh" "$LIB_DIR/logging.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/helpers.sh" "$LIB_DIR/helpers.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/manifest.sh" "$LIB_DIR/manifest.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/common.sh" "$LIB_DIR/common.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/config.sh" "$LIB_DIR/config.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/cli.sh" "$LIB_DIR/cli.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/disk.sh" "$LIB_DIR/disk.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/apps.sh" "$LIB_DIR/apps.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/plan.sh" "$LIB_DIR/plan.sh"
  install_with_policy 644 "$PROJECT_ROOT/synsyu/lib/commands.sh" "$LIB_DIR/commands.sh"
}

#--- detect existing installation
detect_existing() {
  EXISTING_ITEMS=()
  local check=(
    "$BIN_DIR/synsyu_core"
    "$BIN_DIR/syn-syu"
    "$BIN_DIR/synsyu"
    "$LIB_DIR/logging.sh"
    "$LIB_DIR/helpers.sh"
    "$LIB_DIR/manifest.sh"
    "$LIB_DIR/common.sh"
    "$LIB_DIR/config.sh"
    "$LIB_DIR/cli.sh"
    "$LIB_DIR/disk.sh"
    "$LIB_DIR/apps.sh"
    "$LIB_DIR/plan.sh"
    "$LIB_DIR/commands.sh"
  )
  local p
  for p in "${check[@]}"; do
    if [ -e "$p" ]; then
      EXISTING_ITEMS+=("$p")
    fi
  done
}

choose_overwrite_policy() {
  if [ "${#EXISTING_ITEMS[@]}" -eq 0 ]; then
    log_info "CHECK" "No existing installation artifacts detected."
    OVERWRITE_POLICY="overwrite"
    return 0
  fi
  if [ -n "$OVERWRITE_POLICY" ]; then
    case "$OVERWRITE_POLICY" in
      overwrite|backup|skip)
        log_info "POLICY" "Using CLI policy: $OVERWRITE_POLICY"
        return 0 ;;
      *)
        log_error "POLICY" "Invalid --policy value: $OVERWRITE_POLICY"; exit 1 ;;
    esac
  fi
  printf '\nExisting installation detected at:\n'
  local it
  for it in "${EXISTING_ITEMS[@]}"; do
    printf '  - %s\n' "$it"
  done
  printf '\nChoose how to proceed:\n'
  printf '  [1] Overwrite files\n'
  printf '  [2] Backup then overwrite (adds .bak_%s)\n' "$SESSION_STAMP"
  printf '  [3] Skip existing; install only missing\n'
  printf '  [4] Cancel\n'
  local sel
  while true; do
    printf '> '
    read -r sel
    case "$sel" in
      1) OVERWRITE_POLICY="overwrite"; return 0 ;;
      2) OVERWRITE_POLICY="backup"; return 0 ;;
      3) OVERWRITE_POLICY="skip"; return 0 ;;
      4) log_warn "ABORT" "Operator cancelled install"; exit 0 ;;
      *) printf 'Please choose 1, 2, 3, or 4.\n' ;;
    esac
  done
}

#--- create_config_files
create_config_files() {
  execute_local mkdir -p "$CONFIG_DIR"
  if [ "$CREATE_CONFIG" = "1" ]; then
    if [ -f "$CONFIG_DIR/config.toml" ]; then
      if prompt_yes_no "config.toml exists. Overwrite?" "N"; then
        execute_local cp "$PROJECT_ROOT/examples/config.toml" "$CONFIG_DIR/config.toml"
        log_info "CONF" "config.toml overwritten."
      else
        log_warn "CONF" "config.toml retained."
      fi
    else
      execute_local cp "$PROJECT_ROOT/examples/config.toml" "$CONFIG_DIR/config.toml"
      log_info "CONF" "config.toml installed."
    fi
  fi

  if [ "$CREATE_GROUPS" = "1" ]; then
    execute_local mkdir -p "$(dirname "$GROUPS_FILE")"
    if [ ! -f "$GROUPS_FILE" ]; then
      cat >"$GROUPS_FILE" <<'EOF'
# Syn-Syu groups configuration
# Define groups as TOML arrays:
# development = ["rust", "rust-analyzer", "cargo"]
# media = ["mpv", "vlc"]
EOF
      log_info "CONF" "groups.toml scaffold created at $GROUPS_FILE"
    else
      log_warn "CONF" "groups.toml already exists; not modified."
    fi
  fi
}

#--- summary
print_summary() {
  printf '\nInstallation complete.\n'
  printf '  synsyu_core  -> %s/synsyu_core\n' "$BIN_DIR"
  printf '  syn-syu      -> %s/syn-syu\n' "$BIN_DIR"
  printf '  library dir  -> %s\n' "$LIB_DIR"
  if [ "$CREATE_CONFIG" = "1" ]; then
    printf '  config.toml  -> %s/config.toml\n' "$CONFIG_DIR"
  fi
  if [ "$CREATE_GROUPS" = "1" ]; then
    printf '  groups.toml  -> %s\n' "$GROUPS_FILE"
  fi
  printf '  source       : %s\n' "$BUILD_SOURCE_DESC"
  if [ -n "${AUR_HELPER:-}" ]; then
    printf '  AUR helper   : %s (config default)\n' "$AUR_HELPER"
  elif [ "${#DETECTED_HELPERS[@]}" -gt 0 ]; then
    printf '  AUR helper   : %s (detected)\n' "${DETECTED_HELPERS[0]}"
  else
    printf '  AUR helper   : <none detected>\n'
  fi
  printf '\nRemember to ensure %s is in your PATH.\n' "$BIN_DIR"
  printf 'Run: syn-syu --help\n'
  if [ -z "${AUR_HELPER:-}" ] && [ "${#DETECTED_HELPERS[@]}" -eq 0 ]; then
    printf '%s\n' 'No AUR helper detected.'
    printf '%s\n' 'Syn-Syu can still run, but AUR-related features will be limited.'
    printf '%s\n' ''
    printf '%s\n' 'You can install paru (recommended), yay, or another AUR helper, then run:'
    printf '%s\n' '  syn-syu helpers   # detect and set a default helper'
    printf '%s\n' ''
  fi
  printf 'Log: %s\n' "$LOG_PATH"
}

#--- offer_manifest_build
offer_manifest_build() {
  if [ "$NON_INTERACTIVE" = "1" ]; then
    log_info "MANIFEST" "Non-interactive mode; skipping initial manifest build."
    return 0
  fi

  verify_installation_files
  if [ "${#MISSING_INSTALL_ITEMS[@]}" -gt 0 ]; then
    log_warn "CHECK" "Installation incomplete; missing ${#MISSING_INSTALL_ITEMS[@]} artifact(s)."
    printf '\nDetected missing install artifacts:\n'
    local item
    for item in "${MISSING_INSTALL_ITEMS[@]}"; do
      printf '  - %s\n' "$item"
    done
    printf '\nPlease re-run the installer from a fresh download/clone (or rebuild via makepkg -Sif). If the issue persists, report it with the installer log (%s).\n' "$LOG_PATH"
    return 0
  fi

  local core_bin=""
  if [ -x "$BIN_DIR/synsyu_core" ]; then
    core_bin="$BIN_DIR/synsyu_core"
  else
    core_bin="$(command -v synsyu_core 2>/dev/null || true)"
  fi

  if [ -z "$core_bin" ] || [ ! -x "$core_bin" ]; then
    log_warn "MANIFEST" "synsyu_core not found after install; skipping manifest build prompt."
    return 0
  fi

  if ! prompt_yes_no "Build an initial manifest now with syn-syu core?" "Y"; then
    return 0
  fi

  if prompt_yes_no "Build manifest without network access (offline)?" "N"; then
    MANIFEST_OFFLINE=1
  else
    MANIFEST_OFFLINE=0
  fi

  local manifest_out
  manifest_out="${CONFIG_DIR:-$HOME/.config/syn-syu}/manifest.json"
  execute_local mkdir -p "$(dirname "$manifest_out")"
  log_info "MANIFEST" "Generating manifest at $manifest_out"
  local core_args=(--manifest "$manifest_out")
  if [ "$MANIFEST_OFFLINE" = "1" ]; then
    core_args+=(--offline)
  fi
  if "$core_bin" "${core_args[@]}"; then
    log_info "MANIFEST" "Manifest written to $manifest_out"
  else
    log_warn "MANIFEST" "Manifest build failed; run syn-syu core manually after resolving the error."
  fi
}

#--- main
main() {
  parse_args "$@"
  ensure_repo_root
  detect_build_source
  choose_mode
  if [ "$INSTALL_MODE" = "guided" ]; then
    configure_guided
  else
    configure_advanced
  fi
  if [ "$CLI_NO_SUDO" = "1" ] && [ -n "$SUDO_CMD" ]; then
    SUDO_CMD=""
    log_info "MODE" "--no-sudo flag enforced; privileged operations will run without sudo."
  fi

  local missing_tools=()
  detect_missing_tools missing_tools
  if [ "${#missing_tools[@]}" -gt 0 ]; then
    log_warn "CHECK" "Missing commands detected: ${missing_tools[*]}"
    if [ "$INSTALL_DEPS" = "1" ]; then
      install_missing_dependencies "${missing_tools[@]}"
    else
      printf '\nInstall required tools (%s) before continuing.\n' "${missing_tools[*]}"
      if ! prompt_yes_no "Continue anyway?" "N"; then
        exit 201
      fi
    fi
  else
    log_info "CHECK" "All required commands present."
  fi

  detect_existing
  if [ "$NON_INTERACTIVE" = "1" ] && [ -z "$OVERWRITE_POLICY" ]; then
    OVERWRITE_POLICY="overwrite"
    log_info "POLICY" "Non-interactive mode: defaulting policy to overwrite"
  fi
  choose_overwrite_policy

  build_synsyu_core
  deploy_binaries
  deploy_libraries
  create_config_files
  offer_manifest_build
  print_summary
  log_info "DONE" "Installer completed successfully."
}

main "$@"
