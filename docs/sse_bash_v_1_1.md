<!--
Synavera Script Etiquette — Bash Profile v1.1.1
Derived from SSE v1.1
Author: CMD Draven
Scope: Shell / POSIX-compliant Bash scripting under Synavera standards.
-->

# Synavera Script Etiquette — Bash Profile v1.1.1

## 1. Verbosity vs Brevity  
**Chosen Approach:** Verbose + Defensive  

Bash is an interpretive shell, not a compiler. Ambiguity kills reliability. Every Bash script under Synavera’s umbrella must prefer *explicitness* over clever shorthand. Always quote variables (`"$VAR"`), always define functions clearly, and always check command outcomes. Obfuscating behavior for the sake of brevity is forbidden. One-liners are acceptable only when their purpose is self-evident. Use descriptive variable names (`LOG_PATH`, not `lp`), and declare constants in uppercase to visually distinguish them. Scripts must be written as if they will be run by an auditor, not a hobbyist.

---

## 2. Error Handling and Fault Philosophy  
**Chosen Approach:** Predictable + Exit-Bound  

Silent failure is a sin in Bash. Every command whose failure could impact behavior must be validated immediately using either `set -euo pipefail` or explicit conditionals. Use `trap` for cleanup and error signals (`EXIT`, `ERR`, `INT`) to ensure the system leaves no undefined state. When a command fails, log both the command and its exit code in a timestamped record. Synavera Bash scripts must produce deterministic exits: either zero (success) or a well-defined nonzero code, never half-failed silent states.  
Each error should be human-legible and machine-parsable, e.g.:

```bash
log_error "E101" "Failed to initialize registry at $REG_PATH"
exit 101
```

---

## 3. Organizational Structure  
**Chosen Approach:** Modular + Sourced  

Break large scripts into self-contained modules under a `lib/` or `modules/` directory. Use sourcing (`source ./lib/module.sh`) instead of duplication. Each sourced file must have its own SSE header and must not rely on undeclared globals. Use `readonly` or `declare -r` for constants. Functions must return via exit codes (`return 0` / `return 1`) and output via stdout only when intended as data, not for debugging.  
Keep functions pure wherever possible — they should modify state only when clearly documented in their header comment.

---

## 4. Commentary, Documentation, and Narrative Context  
**Chosen Approach:** Annotated Shell with Operational Rationale  

The first 20 lines of any script belong to context. Each file starts with an **SSE-Bash Header** (see template below), followed by a concise docstring explaining the script’s mission, assumptions, and safety model. Inline comments must describe *intent*, not syntax.  
Security annotations (`# Security:` / `# Safety:`) must appear near any block that alters system state, handles sensitive data, or performs privilege escalation. These annotations are mandatory for any operation touching `/etc`, `/var`, network sockets, or external command execution.

Example:
```bash
# Security: ensures only the root user can run this installer
[[ $EUID -ne 0 ]] && { log_error "E201" "Must run as root"; exit 201; }
```

---

## 5. Aesthetic and Formatting Conventions  
**Chosen Approach:** POSIX-Friendly + Legible Blocks  

Indentation: two spaces, never tabs.  
Line width: 100 characters max.  
Use blank lines to separate logical units, not arbitrary commands.  
Every function starts with `#---` divider comments to improve visibility.  
Long pipelines must be split with trailing backslashes and aligned to show clear logical flow. Avoid unnecessary command substitution or nested backticks; prefer `$(...)` and maintain shell portability.  
Files must end with a newline.  

---

## 6. Ethical Layer  
**Chosen Approach:** User Sovereignty + No Hidden State  

A Bash script is the system’s front line — treat it as sacred ground.  
No Synavera Bash script shall ever:  
- Write outside user-owned directories without explicit confirmation.  
- Send or receive network traffic without clearly announcing its intent.  
- Mask or redirect stderr in ways that conceal behavior.  
- Modify configuration files without backup or user prompt.  
Every action must be reversible unless explicitly documented as destructive. The user is the root authority; scripts are only custodians.

---

## 7. Auditability and Temporal Trace  
**Chosen Approach:** Append-Only Logs + Timestamp Integrity  

Every operation of consequence must be logged in an append-only file (default: `/var/log/synavera.log` or `$HOME/.local/share/synavera.log`).  
Logs use the format:

```
YYYY-MM-DDTHH:MM:SSZ [LEVEL] [CODE] message
```

Example:
```
2025-10-27T22:34:01Z [INFO] [INIT] Daemon startup complete
```

Each session generates a SHA-256 hash of the cumulative log for audit chaining:

```bash
sha256sum "$LOG_PATH" > "$LOG_PATH.hash"
```

No deletion or truncation of logs is permitted; rotate only by archiving.

---

## 8. SSE-Bash Standard Template  

```
#============================================================
# Synavera Project: [PROJECT / MODULE NAME]
# Module: [relative/path/to/script.sh]
# Etiquette: Synavera Script Etiquette — Bash Profile v1.1.1
#------------------------------------------------------------
# Purpose:
#   [Briefly describe what this script does.]
#
# Security / Safety Notes:
#   [Identify any sensitive operations, privilege usage, or
#    irreversible actions. If none, state “N/A”.]
#
# Dependencies:
#   [List required binaries or sourced modules.]
#
# Operational Scope:
#   [Explain how this script fits within the system.]
#
# Revision History:
#   [ISO date, author initials, and change summary.]
#   2025-10-27 CMD  Created initial Bash profile template.
#------------------------------------------------------------
# SSE Principles Observed:
#   - set -euo pipefail for predictable behavior
#   - Explicit error codes and logged exits
#   - User confirmation before any write/destruction
#   - Narrative comments for auditability
#============================================================
```

---

## 9. Tooling and Validation  

Scripts must pass **shellcheck** and **bashate** before integration.  
A commit is only valid when `shellcheck --severity=style` returns zero issues.  
Where deviations are required for clarity, document them inline with justification (`# shellcheck disable=SC2086 # intentional variable splitting`).

---

## 10. Conformance  

Any Bash script not conforming to this etiquette cannot be labeled as “Synavera-compliant.”  
SSE-Bash inherits all universal tenets from SSE v1.1.  
If ambiguity arises between this profile and the universal etiquette, the universal etiquette supersedes.
