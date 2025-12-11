<!--
Title: Synavera Script Etiquette — YAML Profile
Version: 1.2
Status: Stable
SSE Profile: YAML Profile v1.2; Markdown & Documentation v1.2; Universal SSE v1.2
Audience: Synavera developers, maintainers, security auditors, and tooling authors working with YAML
Scope: Human-edited YAML for configuration, orchestration, manifests, and policy under Synavera standards.
Last Reviewed: 2025-12-09T00:00:00Z
Security / Safety: Non-compliant YAML may introduce configuration drift, unsafe defaults, or misapplied policy in Synavera systems.
Migration Note: This YAML profile has been elevated to v1.2 by aligning document structure and metadata with SSE-Markdown v1.2 and Universal SSE v1.2; YAML authored against v1.1 remains valid but should be re-audited against this version.
Linked Artifacts:
  - Universal SSE: v1.2
  - SSE-Markdown & Documentation Profile: v1.2
  - SSE-JSON: v1.2
  - Language Profiles: SSE-{{Bash,C Core,C Secure,C++,Integrated Control,Java,Kotlin,Python,Rust}} v1.2
-->

# Synavera Script Etiquette — YAML Profile v1.2

YAML under Synavera is for **humans first, machines second**.

Every YAML document is a control surface: it configures behaviour, activates features, and often encodes policy.  
This profile exists to keep those surfaces legible, predictable, and safe — without leaning on YAML’s more magical foot-guns.

Where JSON is the disciplined substrate, YAML is the higher-level control panel:
expressive, annotated, and ergonomic, but constrained enough to be auditable and convertible when needed.

---

## 1. Verbosity vs Brevity  
**Chosen Approach:** Plain, descriptive keys; minimal magic  

YAML keys must reveal intent rather than data plumbing.  
Abbreviations are acceptable only when genuinely ubiquitous (`id`, `url`, `api`, `tls`) or clearly defined in nearby documentation.

Top-level and nested mappings must correspond to real conceptual areas:

- `server:`, `security:`, `logging:`, `features:`, not `misc:` or `other:`.
- Group related options; avoid dumping heterogeneous keys into a single flat map “just because YAML allows it”.

Values must be explicit in both type and semantics:

- Avoid overloading `""`, `0`, `[]`, `{}`, or `null` as undocumented sentinel values.
- Use explicit enums or tagged values where special meaning is required, and document those meanings.

**Magic words caveat:** bare words like `yes`, `no`, `on`, `off`, `null` have special meaning in YAML 1.1.  
SSE-YAML strongly prefers:

- `true` / `false` for booleans.
- `null` or `~` only when you actually mean null.
- Quoting ambiguous tokens (for example `"on"`, `"off"`, `"yes"`, `"no"` when used as labels or strings).

Silence and ambiguity are fragility; clarity is security.

---

## 2. Error Handling and Fault Philosophy  
**Chosen Approach:** Fail loudly, validate aggressively  

YAML that cannot be parsed or validated is rejected, not “partially applied”.

When a YAML document fails to parse, fails schema validation, or violates domain rules, the consumer must:

- Reject the document.
- Emit a deterministic error code (for example `Y001_SCHEMA_VIOLATION`).
- Provide a human-readable message.
- Include machine-parsable context (file, line/column where possible, path to the failing node, expected vs actual value/type).

Consumers must not:

- Silently default missing required keys.
- Silently coerce types (for example `"42"` → integer `42`, `"true"` → boolean `true`) outside of clearly documented rules.
- Ignore unknown keys unless the schema explicitly allows extension/`additionalProperties`.

Producers and maintainers treat validation failures as defects that must be fixed at source, not “warnings we’ll tidy later”.

---

## 3. Organisational Structure  
**Chosen Approach:** Role-based, environment-aware layout  

YAML artifacts live in purpose-specific directories, mirroring the SSE-JSON conventions:

- `config/` — application and service configuration.
- `orchestration/` — deployment manifests, CI/CD pipelines, infra-as-code YAML.
- `schemas/` — YAML-based schemas or references to JSON Schema/OpenAPI.
- `fixtures/` or `samples/` — demo and test data.
- `policy/` — access control, feature policy, routing rules, etc.

Environment-specific files are separated by name or directory, not hidden deep inside one monolithic file:

- `config/app.dev.yaml`, `config/app.prod.yaml`, or
- `config/dev/…`, `config/prod/…`

Cross-cutting constants (such as region lists, supported locales, feature flag catalogues) must be centralised into their own YAML documents, referenced by others instead of copied around.

Refactors of YAML structure require:

- A human-readable note (changelog, ADR, or inline comment near `_meta`).
- Migration guidance when breaking changes occur (scripts or documented manual steps).

---

## 4. Commentary, Documentation, and Narrative Context  
**Chosen Approach:** Use YAML comments, but keep the story in markdown  

YAML supports comments. SSE-YAML embraces them — cautiously.

Use comments to:

- Explain **why** a non-obvious value is set.
- Mark deprecated keys with removal intent.
- Point to related documentation or tickets.

Do not use comments to:

- Document entire schemas inline in every file.
- Hide critical operational knowledge that exists nowhere else.
- Explain secrets or sensitive material in plaintext.

Central or complex YAML files must have a sibling markdown narrative, as in SSE-JSON:

- `config/app.base.yaml` → `config/app.base.yaml.md` includes:
  - Purpose and scope.
  - Security and privacy notes.
  - Ownership and operational responsibility.
  - Merge/override rules (if layered).
  - Pointers to consumers and schemas.

If understanding the file’s semantics requires knowledge that is **only** in a comment and not in markdown or schema, that is a smell. Move the core explanation alongside the file in `.md`.

---

## 5. Aesthetic and Formatting Conventions  
**Chosen Approach:** Deterministic indentation and style  

Formatting is consistent, repository-wide:

- File extension: `.yaml` (not `.yml`) for Synavera-native artifacts.
- Indentation: 2 spaces, no tabs.
- Line endings: UTF-8, newline at EOF.
- Mappings use the standard `key: value` form.
- Sequences are written with `-` and aligned properly:

```yaml
servers:
  - host: api.synavera.example
    port: 443
  - host: api.backup.example
    port: 443
```

Key ordering inside mappings should be stable. Where no domain-specific order exists, this order is encouraged:

1. `_meta` or other structural/meta keys (`id`, `name`, `version`, `enabled`).
2. Core functional configuration.
3. Optional/auxiliary fields (comments may mark them as such).

Avoid overly compact “one-line dictionaries” for non-trivial data. Prefer vertical layout for clarity and diff-friendliness.

A single canonical formatter (for example `prettier` with YAML support or a dedicated linter/formatter) must be used in CI to enforce style. Diffs that do not match canonical formatting are rejected.

---

## 6. YAML-Specific Features: Anchors, Aliases, Tags, and Multi-Documents  
**Chosen Approach:** Use sparingly, with explicit constraints  

YAML offers advanced features that can easily reduce clarity if abused.

Anchors and aliases:

- Are allowed when they **eliminate clear duplication** (for example shared `resource_limits`, common `metadata` blocks).
- Must not be nested excessively or chained into complex graphs.
- Should be accompanied by a brief comment near the anchor explaining its purpose.

Example:

```yaml
defaults: &job_defaults
  retry: 3
  backoff_seconds: 10

jobs:
  - name: sync_users
    <<: *job_defaults
    schedule: "0 * * * *"
  - name: sync_logs
    <<: *job_defaults
    schedule: "*/5 * * * *"
```

Tags (`!CustomType`) and non-standard type systems:

- Are discouraged in Synavera-native YAML unless the consumer is tightly coupled and well-documented.
- Must be clearly documented where used, including their parsing semantics.

Multi-document YAML (`---` separators):

- Are permitted only when the **natural unit of change** is a batch of related objects (for example Kubernetes manifests).
- Must not mix unrelated concerns in a single multi-document file.

For any YAML that must be convertible to JSON (for example config fed into JSON-only services), SSE-YAML forbids:

- Anchors/aliases that the downstream pipeline cannot resolve.
- Custom tags that do not have a clear JSON representation.

Such “JSON-compatible YAML” must be treated as a **JSON superset** and validated via a JSON-conversion step in CI.

---

## 7. Ethical Layer  
**Chosen Approach:** Config as policy, policy as responsibility  

As with JSON, YAML often carries sensitive implications: toggling telemetry, controlling access, defining data retention, etc.

SSE-YAML imposes:

- No secrets in committed YAML:
  - API keys, passwords, tokens, private keys, connection strings with credentials.
  - Use environment variables, secret stores, or encrypted files instead.
- No raw PII in configuration where it is not strictly required.
- No stealthy policy switches:
  - Keys that affect privacy, logging, retention, or external calls must be plainly named and documented.
  - Default behaviour must favour user agency and safety.

Where a YAML key could meaningfully reduce user control or increase exposure, the file’s sibling markdown must describe:

- The impact of the setting.
- The rationale behind the default.
- Any legal or compliance implications known at design time.

---

## 8. Auditability and Temporal Trace  
**Chosen Approach:** Versioned config, reconstructible history  

For long-lived YAML configurations or manifests that significantly affect behaviour, SSE-YAML encourages explicit temporal and version metadata.

Where appropriate:

- Include a `version:` field at or near the top of the file (semver or documented scheme).
- Include `last_updated_utc:` as an RFC-3339 timestamp in `_meta` (see below) or near the root.
- Maintain human-readable changelogs for major/breaking changes.

Dependencies between YAML files (for example shared includes, inheritance structures, or layered overrides) must be documented in markdown and/or `_meta.dependencies`.

Where YAML drives critical control surfaces (for example routing, permissions, rollout rules), test harnesses and/or staging validation must be in place to ensure that changes can be exercised and rolled back safely.

---

## 9. SSE-YAML Standard `_meta` Block  

SSE-YAML mirrors SSE-JSON’s `_meta` block as a machine-readable header.  
For Synavera-native YAML, `_meta` SHOULD appear at the top level when the file is central (configs, policies, orchestrations).

```yaml
_meta:
  etiquette: "Synavera Script Etiquette — YAML Profile v1.2"
  project: "PROJECT OR MODULE NAME"
  document: "relative/path/to/file.yaml"
  purpose: >
    Short narrative of what this YAML controls or represents.
    This should be enough for a new engineer to orient themselves.
  owner: "Team or role responsible for this file, not a single individual where possible."
  security_notes:
    - "Threat assumptions, permission boundaries, irreversible actions."
    - "If none, use: 'N/A'."
  dependencies:
    - "Upstream systems or schemas this configuration relies on."
    - "Downstream services that assume its structure or presence."
  operational_scope: >
    Where, when, and how this document is loaded and applied.
  revision_history:
    - "2025-11-26 CMD  Created initial YAML profile."
    - "YYYY-MM-DD XX  Short description of the change."
  sse_principles_observed:
    - "No silent failures; consumers validate and fail loudly."
    - "No secrets or raw PII in committed YAML."
    - "Stable, documented structure with clear versioning."
    - "Narrative context maintained in adjacent markdown or schema."
```

Notes:

- `_meta` must not contain secrets or sensitive tokens.
- For high-volume or extremely terse YAML (for example tiny snippets embedded in code or tests), `_meta` may be omitted, with SSE coverage provided by surrounding documentation and schemas.
- For orchestration systems that interpret every field strictly (for example Kubernetes), ensure that `_meta` does not conflict with their schema; if it does, move `_meta` to a higher-level wrapper or to an adjacent markdown file.

---

## 10. Tooling, Validation, and JSON Interplay  
**Chosen Approach:** Linters, schemas, and JSON-compatibility checks  

SSE-YAML requires that YAML artifacts pass through automated guardians before merge:

- YAML parser validation (no syntax errors, indentation issues, tab usage).
- Style/format enforcement via a single canonical formatter/linter.
- Schema validation where schemas exist (YAML-native or JSON Schema applied post-conversion).

For YAML that is intended to be JSON-compatible (for example converted to JSON for APIs):

- CI must include a YAML→JSON conversion step.
- The resulting JSON must pass the same validation and etiquette constraints defined in SSE-JSON v1.1.
- YAML authors must avoid YAML-only constructs that cannot cleanly round-trip.

Where third-party or legacy YAML cannot be made fully SSE-compliant:

- Place such files in clearly named `third_party/` or `legacy/` directories.
- Add a local README explaining the deviations and any mitigations in place.

---

## 11. Conformance  
**Chosen Approach:** Inherits from SSE and SSE-JSON  

SSE-YAML inherits all universal principles from Synavera Script Etiquette v1.1, and is informed by the constraints and expectations of SSE-JSON v1.1.

Where conflicts appear:

- SSE universal principles take precedence.
- For documents that are both YAML and intended for JSON conversion, the stricter of SSE-YAML and SSE-JSON applies.

A YAML artifact that claims to be “Synavera-compliant” must:

- Use `_meta` as described above **or**
- Be clearly documented and referenced in markdown that carries the SSE header and links to the artifact.

Unstructured, opaque YAML that hides behaviour, policy, or user-impacting decisions has no place under the Synavera name.

#============================================================
*End of Synavera Script Etiquette — YAML Profile v1.2*
