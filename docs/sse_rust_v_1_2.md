<!--
Title: Synavera Script Etiquette — Rust Profile
Version: 1.2
Status: Stable
SSE Profile: Rust Profile v1.2; Markdown & Documentation v1.2; Universal SSE v1.2
Audience: Synavera developers, maintainers, security auditors, and tooling authors working with Rust
Scope: Systems and application development in Rust under Synavera standards.
Last Reviewed: 2025-12-09T00:00:00Z
Security / Safety: Non-compliant Rust code may cause safety-contract violations, undefined behaviour through unsafe misuse, or reliability issues in Synavera systems.
Migration Note: This Rust profile has been elevated to v1.2 by aligning document structure and metadata with SSE-Markdown v1.2 and Universal SSE v1.2; Rust code authored against v1.1 remains valid but should be re-audited against this version.
Linked Artifacts:
  - Universal SSE: v1.2
  - SSE-Markdown & Documentation Profile: v1.2
  - SSE-JSON: v1.2
  - SSE-YAML: v1.2
  - Language Profiles: SSE-{{Bash,C Core,C Secure,C++,Integrated Control,Java,Kotlin,Python,Rust}} v1.2
-->

# Synavera Script Etiquette — Rust Profile v1.2.1

## 1. Verbosity vs Brevity
**Chosen Approach:** Explicit + Intent‑Driven

Rust rewards precision. Names, modules, and traits must express purpose without resorting to clever shorthand. Prefer clarity to “magic.” Hidden invariants are documented; surprising lifetimes or trait bounds are explained. Obscure one‑liners that compress meaning are discouraged. Silence is fragility; clarity is security.


## 2. Error Handling and Fault Philosophy
**Chosen Approach:** Result‑first; panic‑last; auditable at boundaries

Errors are values. Functions return `Result` where failure is possible and meaningful. Panics are reserved for unrecoverable programmer errors and are never used for control flow. Error paths are deliberate, logged at appropriate boundaries, and mapped to deterministic process exits for binaries. Libraries surface rich error types with context; binaries convert them to stable, operator‑legible forms. No operation of consequence may fail without a timestamped record and actionable context.


## 3. Organizational Structure
**Chosen Approach:** Modular crates + contract‑bound interfaces

Design around crates with clear responsibilities. Public APIs are minimal and stable; internal modules are private by default. Traits define capability boundaries; implementations must be swappable without leaking invariants. Avoid global state; inject dependencies explicitly. Feature flags are narrow and orthogonal, with safety semantics documented. Crates declare MSRV and supported targets.


## 4. Commentary, Documentation, and Narrative Context
**Chosen Approach:** Storytelling with Security/Safety Annotations

Every public item carries a doc comment describing purpose, contracts, side effects, and failure modes. Files begin with an SSE header and a concise rationale. Sections that cross trust boundaries or manipulate resources include `Security:` / `Safety:` notes explaining assumptions, permission scope, and consequences of failure. Documentation favors intent and hazard mapping over restating signatures.


## 5. Aesthetic and Formatting Conventions
**Chosen Approach:** Tool‑enforced consistency + Synavera discipline

Formatting is enforced by `rustfmt` (stable channel). Linting via `clippy` runs with a strict configuration; deviations are explicitly allowed with justification at the smallest scope. Line width targets 100 characters. Imports are grouped by crate, then alphabetized. Modules are small and cohesive; whitespace separates ideas, not merely syntax. Files are UTF‑8 and end with a newline.


## 6. Ethical Layer
**Chosen Approach:** User Agency + Explicit Consent

Software belongs to its operator. No Synavera Rust binary or library shall emit telemetry, alter external state, or escalate privileges without clear disclosure and operator control. Destructive actions require explicit confirmation or safe non‑interactive flags. Persistent state changes are reversible where feasible, or documented as irreversible with rationale. Secrets are never logged; redaction is mandatory.


## 7. Auditability and Temporal Trace
**Chosen Approach:** Immutable, reconstructible history

Operational events are logged with RFC‑3339 UTC timestamps and stable fields. Logs are structured and append‑only, with rotation by archival. Where integrity matters, segments are chained with SHA‑256 digests. Configuration changes, migrations, and capability flips are recorded with actor, rationale, and before/after state when privacy permits.


## 8. Execution Strata Guarantees
**Applies universally; select the strictest fitting the module.**

Systems/daemon: predictable exits; idempotent operations; signal handling and graceful shutdown; resource caps and backpressure; no background mutation without logs.  
Service/application: stable public interfaces; typed configurations; dependency isolation; structured observability; backward‑compatible migrations.  
Research/utility: reproducible environments; deterministic seeds; captured run metadata; provenance of datasets, binaries, and configuration.


## 9. Configuration, Dependencies, and Environments
**Chosen Approach:** Reproducible and least‑privilege by default

Dependencies are explicit in `Cargo.toml` with semver‑aware version ranges pinned by lockfiles for releases. Build features default to the safest capability set. Configuration follows a clear precedence (env → config file → CLI flags), is validated at startup, and rejects unknown keys. Secrets arrive via secure channels and never ship in source or binary defaults.


## 10. Type System, Contracts, and Unsafe Code
**Chosen Approach:** Prove invariants; fence the sharp edges

Public APIs and FFI boundaries declare precise types and lifetimes. Invariants are documented and, where feasible, encoded in the type system. `unsafe` is allowed only with minimal scope, comprehensive justification, and tests that exercise the safety contract. Interior mutability and global singletons are used sparingly and documented with reasoning.


## 11. Concurrency and Asynchrony
**Chosen Approach:** Deterministic by construction

Choose sync vs async deliberately; do not mix models casually. Executors and runtimes are selected and documented per crate. Shared state is minimized; ownership and borrowing are preferred over locks. Where concurrency hazards exist, include `Safety:` notes and tests using tools such as `loom` or similar to stress scheduling assumptions.


## 12. Testing, Quality Gates, and CI
**Chosen Approach:** Prove it works; prove it stays working

Unit tests are fast and deterministic; integration tests are isolated and hermetic where possible. Property‑based and fuzz testing guard against edge conditions; regression tests pin behavior. Static analysis (`clippy`), documentation tests, and doctests run in CI. Security and supply chain checks (e.g., advisory audits, license allow‑lists) are enforced. Coverage targets are set and exemptions are justified.


## 13. Logging and Observability
**Chosen Approach:** Structured, stable, and respectful of privacy

Logs are machine‑parsable and consistent across crates, including correlation identifiers when distributed. Sensitive data is redacted at the source. Metrics and tracing are opt‑in and documented. Production binaries avoid debug logging by default; verbosity is operator‑controlled.


## 14. Data Handling and Privacy
**Chosen Approach:** Minimalism + Clear lineage

Only necessary data is stored. Serialization includes schema/version markers. Temporary files and caches have bounded scope and lifetime. Sensitive material at rest is encrypted or excluded; zeroization is considered where appropriate.


## 15. Packaging, Distribution, and Supply Chain
**Chosen Approach:** Transparent builds + Verifiable artifacts

Builds are reproducible on stable toolchains and documented. Release artifacts identify their build provenance. Semantic versioning encodes intent. Changelogs enumerate risks and migrations. Dependencies are audited; license and security policies are enforced during CI.


## 16. SSE‑Rust Standard Template

```
/*============================================================
  Synavera Project: [PROJECT / CRATE NAME]
  Module: [crate::path::to::module]
  Etiquette: Synavera Script Etiquette — Rust Profile v1.2.1
  ------------------------------------------------------------
  Purpose:
    [Describe the module’s mission succinctly.]
  
  Security / Safety Notes:
    [Threat assumptions, permission boundaries, unsafe blocks,
     irreversible actions, secret handling. If none, state “N/A”.]
  
  Dependencies:
    [Critical crates/features; toolchain constraints; MSRV.]
  
  Operational Scope:
    [How this module fits; inputs/outputs; side effects.]
  
  Revision History:
    [ISO date, author initials, change summary.]
    2025-10-27 CMD  Created initial Rust profile template.
  ------------------------------------------------------------
  SSE Principles Observed:
    - Result‑first error handling; panic‑last
    - Structured, append‑only logging with UTC timestamps
    - Minimal public surface; contract‑bound traits
    - Explicit feature flags; documented safety invariants
============================================================*/
```


## 17. Tooling and Validation
**Chosen Approach:** Automated guardians

Formatting with `rustfmt`; linting with `clippy` at a strict level. Documentation builds must pass without warnings. Security checks and dependency audits run in CI. Any lint allowance or safety exception is local, justified in‑code, and traceable to rationale or ticket.


## 18. Conformance
**Chosen Approach:** Universal precedence

SSE‑Rust inherits the universal SSE v1.2. Where conflicts arise, the universal etiquette supersedes. A crate or module may not be labeled “Synavera‑compliant” unless it meets this profile’s mandates appropriate to its execution stratum.
