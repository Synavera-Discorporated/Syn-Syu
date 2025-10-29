<!--
Synavera Script Etiquette — Python Profile v1.1
Derived from SSE v1.1
Author: CMD Draven
Scope: Python across three strata — daemon, service, and research — under Synavera standards.
-->

# Synavera Script Etiquette — Python Profile v1.1

## 1. Verbosity vs Brevity
**Chosen Approach:** Explicit + Intent‑Driven

Python reads like English; make it speak clearly. Names must reveal purpose, modules must announce their remit, and behavior must never hide behind magic. Dynamic features (metaprogramming, monkey‑patching, implicit imports) are discouraged unless documented with risk and rationale. Prefer readability and traceability to cleverness. Silence is fragility; clarity is security.


## 2. Error Handling and Fault Philosophy
**Chosen Approach:** Exceptions are first‑class; silence is forbidden

Exception paths must be deliberate, logged, and auditable. Catch only what you can act upon. Bare `except:` is disallowed. Errors that reach the process boundary must be rendered as deterministic exit codes for CLIs and as structured error responses for services. Recovery logic is explicit and commented with intent. No operation of consequence may fail without a timestamped record and context sufficient for post‑mortem.


## 3. Organizational Structure
**Chosen Approach:** Modular + Interface‑Bound

Code is organized into replaceable units with explicit boundaries: domains, adapters, and interfaces. Cross‑module behavior does not leak. Public interfaces are documented in‐module. Globals are avoided; configuration is injected. Modules and packages may be unplugged and tested in isolation. Each package declares its minimal supported Python version and runtime constraints.


## 4. Commentary, Documentation, and Narrative Context
**Chosen Approach:** Storytelling with Security/Safety Annotations

Every file begins with an SSE header and a short narrative of purpose, risks, and constraints. Comments explain *why*, not *what*. Security‑relevant sections carry `Security:` / `Safety:` notes describing threat assumptions, permission boundaries, and failure consequences. Docstrings describe contracts and side effects rather than rephrasing signatures.


## 5. Aesthetic and Formatting Conventions
**Chosen Approach:** Tool‑enforced consistency + Synavera discipline

Code is formatted with `black` (88–100 columns), imports sorted with `isort`, and linted with `ruff`. Deviations are rare and documented inline with justification and the specific rule ID. Lines group ideas; whitespace separates concepts. Type hints are mandatory at public boundaries and strongly encouraged internally. Files end with a newline and maintain UTF‑8 encoding.


## 6. Ethical Layer
**Chosen Approach:** User Agency + Explicit Consent

Software belongs to its operator. No Synavera Python component shall transmit telemetry, alter external state, or escalate privileges without clear disclosure and operator control. Destructive actions require confirmations or explicit non‑interactive flags. Persistent changes are reversible where feasible or documented as irreversible with justification. Secrets never enter source control; redaction is mandatory in logs and errors.


## 7. Auditability and Temporal Trace
**Chosen Approach:** Immutable, reconstructible history

Operational events are logged append‑only with RFC‑3339 UTC timestamps and stable fields. Logs are structured (key‑value or JSON). Where integrity matters, log segments are chained with SHA‑256 digests and archived rather than truncated. Configuration changes and migrations are recorded with actor, rationale, and before/after state when privacy permits.


## 8. Execution Strata Guarantees
**Applies universally; choose the strictest that fits the context.**

Daemon (system/ops): predictable exits; idempotent actions; no background mutation without logs; explicit signal handling and graceful shutdown.  
Service (application/modules): typed public APIs; stable contracts; dependency isolation; structured observability; migration discipline; backward‑compatible configs.  
Research (utility/analysis): reproducible environments; deterministic seeds; captured metadata of runs; provenance of datasets and code versions; clear separation of exploratory vs productionized modules.


## 9. Configuration, Dependencies, and Environments
**Chosen Approach:** Reproducible and least‑privilege by default

Dependency sets are pinned and reproducible (e.g., `pyproject.toml` with a lock file). Environments are isolated (virtualenv/Poetry). Configuration follows a clear precedence (env → config file → CLI flags), is validated at startup, and rejects unknown keys. Secrets are provided through secure channels (environment, keyring) and never embedded.


## 10. Type System and Contracts
**Chosen Approach:** Typed boundaries + Checked behavior

Public APIs and IO boundaries are annotated with types and validated at runtime where safety matters. Prefer dataclasses or typed models for payloads. Contracts include invariants and failure modes in docstrings. Backward‑compatibility is deliberate, versioned, and documented.


## 11. Testing, Quality Gates, and CI
**Chosen Approach:** Prove it works; prove it stays working

Automated tests cover critical paths, error behavior, and security controls. Unit tests are fast and deterministic; integration tests are isolated and hermetic where possible. Coverage targets are enforced, with exemptions justified. Static analysis (ruff, mypy), security scanning (bandit), and dependency audit run in CI. Artifacts (wheels, images) are signed where appropriate.


## 12. Logging and Observability
**Chosen Approach:** Structured, stable, and respectful of privacy

Logs are machine‑parsable, consistent across modules, and include correlation fields when distributed. Sensitive content is redacted at the source. Metrics and traces, when used, are opt‑in and documented. No debug logging in production by default; verbosity is operator‑controlled.


## 13. Data Handling and Privacy
**Chosen Approach:** Minimalism + Clear lineage

Store only what is required. Data transformations record lineage where feasible. Temporary files and caches are scoped, cleaned, and never contain secrets unencrypted. Exported data includes schema/version markers.


## 14. Packaging and Distribution
**Chosen Approach:** Transparent builds + Verifiable artifacts

Projects use a single canonical build system (PEP 517/518). Builds are reproducible and documented. Versioning follows semantic intent. Release notes enumerate risks and migrations. Wheels and images identify their build provenance.


## 15. SSE‑Python Standard Template

```
#============================================================
# Synavera Project: [PROJECT / MODULE NAME]
# Module: [relative/path/to/file.py]
# Etiquette: Synavera Script Etiquette — Python Profile v1.1
#------------------------------------------------------------
# Purpose:
#   [Describe the module’s mission in one or two sentences.]
#
# Security / Safety Notes:
#   [Threat assumptions, permission boundaries, irreversible
#    actions, secret handling. If none, state “N/A”.]
#
# Dependencies:
#   [Critical libraries/services; runtime constraints.]
#
# Operational Scope:
#   [How this module fits; inputs/outputs; side effects.]
#
# Revision History:
#   [ISO date, author initials, change summary.]
#   2025-10-27 CMD  Created initial Python profile template.
#------------------------------------------------------------
# SSE Principles Observed:
#   - Explicit exception handling; no silent failure
#   - Structured, append‑only logging with UTC timestamps
#   - Typed public interfaces; validated configs
#   - Reproducible environments; pinned dependencies
#============================================================
```


## 16. Tooling and Validation
**Chosen Approach:** Automated guardians

Formatting with `black`; imports via `isort`; lint via `ruff`. Type checking with `mypy` (strict on public APIs). Security scanning with `bandit` and dependency audit. Any deviation from rules is in‑code justified and traceable to a ticket or rationale.


## 17. Conformance
**Chosen Approach:** Universal precedence

SSE‑Python inherits the universal SSE v1.1. When conflicts arise, the universal etiquette supersedes. A module may not be labeled “Synavera‑compliant” unless it meets this profile’s mandates appropriate to its execution stratum.
