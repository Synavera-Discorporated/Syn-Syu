#============================================================
# Synavera Project: Syn-Syu
# Module: synsyu/lib/cli.sh
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   Parse Syn-Syu command-line arguments into global state.
#
# Security / Safety Notes:
#   Performs argument parsing only; no privileged operations.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Clear flag handling with early validation
#   - Separation of parsing from execution
#------------------------------------------------------------

#--- parse_cli
parse_cli() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --config)
        CONFIG_PATH="$2"
        shift 2
        ;;
      --manifest)
        SYN_MANIFEST_PATH="$2"
        shift 2
        ;;
      --rebuild)
        REBUILD_MANIFEST=1
        shift
        ;;
      --plan)
        PLAN_PATH="$2"
        shift 2
        ;;
      --edit-plan|--edit)
        EDIT_PLAN=1
        shift
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      --no-aur)
        NO_AUR=1
        shift
        ;;
      --no-repo)
        NO_REPO=1
        shift
        ;;
      --verbose)
        LOG_VERBOSE=1
        shift
        ;;
      --full-path)
        FULL_PATH=1
        shift
        ;;
      --offline)
        OFFLINE=1
        shift
        ;;
      --quiet|-q)
        QUIET=1
        shift
        ;;
      --json)
        JSON_OUTPUT=1
        shift
        ;;
      --confirm)
        NO_CONFIRM=0
        shift
        ;;
      --noconfirm)
        NO_CONFIRM=1
        shift
        ;;
      --helper)
        AUR_HELPER="$2"
        shift 2
        ;;
      --strict)
        STRICT_MODE=1
        shift
        ;;
      --include)
        INCLUDE_PATTERNS+=("$2")
        shift 2
        ;;
      --exclude)
        EXCLUDE_PATTERNS+=("$2")
        shift 2
        ;;
      --batch)
        BATCH_SIZE="$2"
        shift 2
        ;;
      --groups)
        GROUPS_PATH="$2"
        shift 2
        ;;
      --min-free-gb)
        if [ -z "${2:-}" ]; then
          printf 'Option --min-free-gb requires a value.\n' >&2
          exit 101
        fi
        local converted
        converted="$(convert_gb_to_bytes "$2")"
        if ! [[ "$converted" =~ ^[0-9]+$ ]]; then
          printf 'Invalid value for --min-free-gb: %s\n' "$2" >&2
          exit 101
        fi
        MIN_FREE_SPACE_BYTES="$converted"
        MIN_FREE_SPACE_OVERRIDE_BYTES="$converted"
        shift 2
        ;;
      --with-flatpak)
        APPLICATIONS_FLATPAK=1
        APPLICATIONS_FLATPAK_CLI=1
        shift
        ;;
      --no-flatpak)
        APPLICATIONS_FLATPAK=0
        APPLICATIONS_FLATPAK_CLI=0
        shift
        ;;
      --with-fwupd)
        APPLICATIONS_FWUPD=1
        APPLICATIONS_FWUPD_CLI=1
        shift
        ;;
      --no-fwupd)
        APPLICATIONS_FWUPD=0
        APPLICATIONS_FWUPD_CLI=0
        shift
        ;;
      --help|-h)
        COMMAND="help"
        COMMAND_ARGS=()
        shift
        ;;
      --version)
        COMMAND="version"
        COMMAND_ARGS=()
        shift
        ;;
      --)
        shift
        COMMAND_ARGS+=("$@")
        break
        ;;
      -*)
        printf 'Unknown flag %s\n' "$1" >&2
        exit 101
        ;;
      *)
        if [ -z "$COMMAND" ]; then
          COMMAND="$1"
        else
          COMMAND_ARGS+=("$1")
        fi
        shift
        ;;
    esac
  done

  if [ -z "$COMMAND" ]; then
    COMMAND="sync"
  fi
}

#--- parse_post_command_flags
# Allow common flags to appear after the command name.
parse_post_command_flags() {
  local -a rest=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --with-flatpak)
        APPLICATIONS_FLATPAK=1
        APPLICATIONS_FLATPAK_CLI=1
        ;;
      --no-flatpak)
        APPLICATIONS_FLATPAK=0
        APPLICATIONS_FLATPAK_CLI=0
        ;;
      --with-fwupd)
        APPLICATIONS_FWUPD=1
        APPLICATIONS_FWUPD_CLI=1
        ;;
      --no-fwupd)
        APPLICATIONS_FWUPD=0
        APPLICATIONS_FWUPD_CLI=0
        ;;
      --offline)
        OFFLINE=1
        ;;
      --verbose)
        LOG_VERBOSE=1
        ;;
      --full-path)
        FULL_PATH=1
        ;;
      --config)
        CONFIG_PATH="$2"
        shift
        ;;
      --manifest)
        SYN_MANIFEST_PATH="$2"
        shift
        ;;
      --plan)
        PLAN_PATH="$2"
        shift
        ;;
      --groups)
        GROUPS_PATH="$2"
        shift
        ;;
      *)
        rest+=("$1")
        ;;
    esac
    shift
  done
  COMMAND_ARGS=("${rest[@]}")
}
