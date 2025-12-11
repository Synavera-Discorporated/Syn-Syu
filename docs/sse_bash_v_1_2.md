<!--
Title: Synavera Script Etiquette — Bash Profile
Version: 1.2
Status: Stable
SSE Profile: Bash Variant v1.2; Markdown & Documentation v1.2; Universal SSE v1.2
Audience: Synavera developers, maintainers, security auditors, and tooling authors working with Bash and POSIX shell
Scope: Shell / POSIX-compliant Bash scripting under Synavera standards
Last Reviewed: 2025-12-06T05:50:00Z
Security / Safety: Non-compliant Bash scripts may cause data loss, privilege escalation, or unreliable automation in Synavera environments
Migration Note: This Bash profile has been elevated to v1.2 by aligning document structure and metadata with SSE-Markdown v1.2; Bash scripts authored against v1.1 remain valid but should be re-audited against this version
Linked Artifacts:
  - Universal SSE: v1.2
  - SSE-Markdown & Documentation Profile: v1.2
  - SSE-JSON: v1.2
  - SSE-YAML: v1.2
-->

# Synavera Script Etiquette — Bash Profile v1.2

## 1. Verbosity vs Brevity  
**Chosen Approach:** Verbose + Defensive  

Bash is an interpretive shell, not a compiler. Ambiguity kills reliability, and terse one-liners become unauditable very quickly.  
Under Synavera, Bash scripts:

- must prefer **readable, explicit pipelines** over “clever” command chains,
- must spell out edge cases (empty input, missing files, permission errors),
- and must assume that the reader may not be a Bash expert.

Comments should explain **why** a choice is made, not merely restate what the code already expresses.  
Inline comments such as:

```bash
# Using -r to avoid backslash escapes, and -p for explicit prompt
read -r -p "Enter target hostname: " TARGET_HOST
```

are accepted and encouraged, especially near security- or data-sensitive actions.

One-liners are acceptable only when their purpose is self-evident, safe, and resides in local tooling or personal aliases, not in shared production scripts.

---

## 2. Error Handling and Exit Discipline  
**Chosen Approach:** Fail Fast + Loud, Never Silently  

Bash’s default error handling is permissive and dangerous. Under Synavera:

- `set -euo pipefail` (or the moral equivalent) is **mandatory** at the top of every non-trivial script,
- unset variables must never be relied upon (`set -u`),
- and pipelines must not discard failures (`pipefail`).

Every script must define a consistent error-exit path, for example:

```bash
die() {
    local exit_code="$1"
    shift
    printf 'ERROR [%s]: %s
' "$exit_code" "$*" >&2
    exit "$exit_code"
}
```

Scripts should never “just continue” after a failure in a critical command.  
If a failure can be safely ignored, the code must:

- explicitly **capture and evaluate** the status, and
- clearly document why ignoring it is safe.

For example:

```bash
if ! rm -f "$TEMP_FILE"; then
    printf 'WARN: Could not remove temp file %s; continuing.
' "$TEMP_FILE" >&2
fi
```

Silent failure is a sin in Bash. Every command whose failure could harm integrity, confidentiality, or availability must either:

- halt the script explicitly with a log message and a nonzero code, or
- be carefully fenced with explicit justification and monitoring.

---

## 3. Organizational Structure  
**Chosen Approach:** Modular + Sourced  

Break large scripts into self-contained modules under a `lib/` or `scripts/` directory, each responsible for a single coherent concern.  
Prefer:

- small, composable functions with clear contracts,
- separate files for unrelated domains (e.g. `fs.sh`, `net.sh`, `ui.sh`),
- and a single, clearly marked `main()` or entrypoint.

Example:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/fs.sh
. "$SCRIPT_DIR/../lib/fs.sh"
# shellcheck source=../lib/net.sh
. "$SCRIPT_DIR/../lib/net.sh"

main() {
    ensure_dependencies
    sync_registry
}

main "$@"
```

Organizational rules:

- Sourced files must not have side effects beyond function and constant definitions.
- Globals must be minimised; where used, they must be clearly named and documented.
- Function names should be descriptive and namespaced when appropriate (e.g. `fs_list_backups`).

---

## 4. Naming Conventions and Readability  
**Chosen Approach:** Intent-Revealing, Lowercase with Underscores  

Under SSE-Bash:

- functions and variables are generally `lower_snake_case`,
- constants and environment-like flags are `UPPER_SNAKE_CASE`,
- and acronyms are treated as words (e.g. `db_conn_string`, not `DBConnStr`).

Examples:

```bash
readonly DEFAULT_TIMEOUT_SECONDS=30
backup_root="/var/backups/synavera"

perform_incremental_backup() {
    local source_dir="$1"
    local target_dir="$2"
    # ...
}
```

Naming principles:

- Prefer long, descriptive names over short or cryptic ones.
- Make error codes and log keys searchable (e.g. `E101_FS_INIT_FAILURE`).
- Avoid reuse of short positional variables beyond small, local scopes.

Scripts must be formatted for **diffability**:

- one logical step per line,
- avoid trailing spaces,
- and keep line length human-friendly, wrapping with `\` only when necessary and carefully aligned.

---

## 5. Input, Output, and Logging  
**Chosen Approach:** Explicit, Structured, and Safe  

Input handling:

- Always validate arguments and environment variables before use.
- Use `getopts` or a well-documented parser function for CLI options.
- Reject ambiguous or conflicting options with a clear error.

Example skeleton:

```bash
usage() {
    cat <<'EOF'
Usage: synavera-tool [options]

Options:
  -h            Show this help and exit
  -c <config>   Path to configuration file (required)
EOF
}

parse_args() {
    local config_path=""
    while getopts ":hc:" opt; do
        case "$opt" in
            h)
                usage
                exit 0
                ;;
            c)
                config_path="$OPTARG"
                ;;
            \?)
                printf 'ERROR: Invalid option: -%s
' "$OPTARG" >&2
                usage >&2
                exit 2
                ;;
            :)
                printf 'ERROR: Option -%s requires an argument.
' "$OPTARG" >&2
                usage >&2
                exit 2
                ;;
        esac
    done
    # ...
}
```

Output and logging:

- Use `stdout` for normal output and `stderr` for diagnostics.
- Use a small, consistent logging API:

```bash
log_info()  { printf 'INFO: %s
' "$*" >&2; }
log_warn()  { printf 'WARN: %s
' "$*" >&2; }
log_error() { printf 'ERROR: %s
' "$*" >&2; }
```

Logs should include context where practical (e.g. host, script name, correlation ID) and must avoid leaking secrets (tokens, passwords, keys).

---

## 6. Security and Secrets Handling  
**Chosen Approach:** Least Privilege + Zero Trust in Input  

Bash is especially prone to injection, accidental globbing, and unsafe handling of secrets.  
Under SSE-Bash:

- Never `eval` untrusted input.
- Always quote variable expansions, especially when they may contain spaces or glob characters.
- Treat environment variables and CLI arguments as hostile until validated.

Examples:

```bash
# Dangerous:
cp $SOURCE $DEST  # may expand or split unexpectedly

# Safer:
cp -- "$SOURCE" "$DEST"
```

Secrets:

- must never be echoed or logged in plaintext,
- must not be persisted in world-readable files,
- and should be passed via secure channels (environment, protected files, secret managers) with minimal lifetime.

Where elevated privileges are required:

- use `sudo` sparingly and explicitly,
- log the intent before privilege escalation,
- and drop privileges as soon as the privileged operation is complete.

---

## 7. Portability and Environment Assumptions  
**Chosen Approach:** POSIX-leaning, Bash-Specific Where Justified  

Scripts should:

- prefer POSIX constructs for portability,
- only rely on Bash-specific features when they materially improve clarity, safety, or performance,
- and clearly document any non-portable behaviours or dependencies (e.g. GNU coreutils features).

At the top of each script, document:

- the minimum supported Bash version,
- any required external tools (with versions if relevant),
- and any OS-specific assumptions.

Example:

```bash
# Requires: bash >= 5.0, rsync, jq
# Tested on: Ubuntu 22.04, Debian 12
```

Where platform-specific branches exist, keep them:

- as small and isolated as possible,
- guarded by explicit checks (e.g. `uname`, `lsb_release`, or feature detection),
- and well-commented.

---

## 8. Testing, Idempotency, and Observability  
**Chosen Approach:** Repeatable Runs, Testable Units  

Under Synavera, Bash scripts must be written as though they will be:

- executed repeatedly,
- run in CI/CD pipelines,
- and inspected by security auditors.

Requirements:

- Design scripts to be **idempotent** where possible (safe to re-run without corrupting state).
- Expose key behaviours behind functions that can be exercised via unit-style tests (e.g. `bats`).
- Exit codes must be documented and stable; consumers should be able to rely on them.

Fast checks:

- provide a `--dry-run` or `--check` mode for scripts that make changes,
- emit structured logs (JSON or clearly parseable text) for automation when appropriate,
- and surface metrics-friendly messages where they feed into monitoring.

---

## 9. Tooling and Validation  

Scripts must pass **shellcheck** and **bashate** before integration.  
A commit is only valid when `shellcheck --severity=style` returns zero issues.  
Where deviations are required for clarity, document them inline with precise justification (e.g. `# shellcheck disable=SC2086 # intentional variable splitting`).

---

## 10. Conformance  

Any Bash script not conforming to this etiquette cannot be labeled as “Synavera-compliant.”  
SSE-Bash inherits all universal tenets from SSE v1.2.  
If ambiguity arises between this profile and the universal etiquette, the universal etiquette supersedes.
