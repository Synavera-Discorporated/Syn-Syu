<!--
Title: Synavera Script Etiquette — JSON Profile
Version: 1.2
Status: Stable
SSE Profile: JSON Profile v1.2; Markdown & Documentation v1.2; Universal SSE v1.2
Audience: Synavera developers, maintainers, security auditors, and tooling authors working with JSON
Scope: Static JSON for configuration, contracts, schemas, fixtures, and policy under Synavera standards.
Last Reviewed: 2025-12-09T00:00:00Z
Security / Safety: Non-compliant JSON may cause schema drift, misconfiguration, policy violations, or unsafe defaults in Synavera systems.
Migration Note: This JSON profile has been elevated to v1.2 by aligning document structure and metadata with SSE-Markdown v1.2 and Universal SSE v1.2; JSON authored against v1.1 remains valid but should be re-audited against this version.
Linked Artifacts:
  - Universal SSE: v1.2
  - SSE-Markdown & Documentation Profile: v1.2
  - SSE-YAML: v1.2
  - Language Profiles: SSE-{Bash,C Core,C Secure,C++,Integrated Control,Java,Kotlin,Python,Rust} v1.2
-->

# Synavera Script Etiquette — JSON Profile v1.2

JSON under Synavera is not “just a blob of keys and values.”
Every document is an interface, a contract, and often a policy surface.
This profile exists to make those surfaces explicit, auditable, and user-respectful.

JSON here is treated as a disciplined substrate:
small, predictable, boring in the best way — but always wrapped in context that makes its effects and assumptions visible.

---

## 1. Verbosity vs Brevity  
**Chosen Approach:** Human-legible keys, machine-friendly structure  

Key names must reveal intent, not implementation trivia.  
Abbreviations are only acceptable when they are genuinely ubiquitous (`id`, `url`) or clearly defined in adjacent documentation.

JSON objects must map to real concepts: domain entities, configuration scopes, or well-defined data structures.  
“Grab-bag” keys such as `misc`, `other`, or `data` are considered a smell and must be refactored into clearer groupings.

Values must be explicit in type and meaning.  
SSE-JSON forbids abusing `"0"`, `""`, or `null` as undocumented sentinels for special states (e.g. “unset”, “inherit”, “disabled”).  
If such sentinel behaviour is required, it must be formally documented in the contract and consistently applied.

Silence is fragility; clarity is security.

---

## 2. Error Handling and Fault Philosophy  
**Chosen Approach:** Loud failures, deterministic diagnostics  

JSON is either valid or it is not.  
SSE-JSON rejects “best effort” parsing and silent coercion.

When a JSON document fails to parse, fails schema validation, or violates domain rules, the consumer must:

- Reject the document.
- Emit a deterministic error code (for example `J001_SCHEMA_VIOLATION`).
- Provide a human-readable explanation.
- Include machine-parsable context (the failing JSON path, expected vs actual type/value).

Consumers must not silently coerce types (e.g. `"42"` → `42`, `"true"` → `true`) unless this behaviour is explicitly defined in the contract and the coercion is observable via logs or metrics.

Producers must treat validation failures as defects.  
“Accepting whatever the other side sends” is not resilience; it is an accumulation of undefined behaviour.

---

## 3. Organisational Structure  
**Chosen Approach:** Namespaced, discoverable, and environment-aware  

JSON artifacts live in directories that reflect their purpose:

- `config/` for application configuration.
- `schemas/` for JSON Schemas, OpenAPI fragments, and contract descriptors.
- `fixtures/` or `samples/` for test and demonstration data.
- `contracts/` for externally visible payload shapes and policy documents.

Environment-specific configuration is expressed as separate documents  
(for example `config/app.dev.json`, `config/app.prod.json`) rather than embedding environment flags deep inside one monolithic file.

Cross-cutting constants (for example locale lists, feature flag definitions, permissible enum values) are centralised into dedicated JSON documents instead of being copy-pasted across configs and payloads.

When JSON structures are refactored, migrations must be recorded:

- In human-readable form (changelog, ADR, or equivalent).
- Where feasible, as machine scripts to map old shapes to new ones.

---

## 4. Commentary, Documentation, and Narrative Context  
**Chosen Approach:** No inline comments; narrative lives beside the file  

JSON itself does not support comments and SSE-JSON does not rely on “JSON with comments” variants in production.  
Any file that requires comments to be understood is instead paired with narrative documentation.

For stable and central JSON documents (for example `config/app.base.json`), a sibling markdown file is required:

- `config/app.base.json.md` explains:
  - Purpose and scope.
  - Security and privacy considerations.
  - Ownership and operational responsibility.
  - How this document is loaded, merged, and overridden.
  - Pointers to schemas and consuming components.

Where practical, schemas (JSON Schema, OpenAPI) are used as executable documentation and are themselves documented:

- High-level intent in markdown.
- Precise structural rules in schema.

If a document’s meaning cannot be reconstructed by combining:

- The JSON itself.
- Its schema (if any).
- Its markdown narrative.

then the document is considered non-compliant with SSE-JSON.

---

## 5. Aesthetic and Formatting Conventions  
**Chosen Approach:** Stable, predictable formatting  

Formatting is deterministic across the repository:

- Indentation is 2 spaces.
- Keys are quoted strings as required by the JSON specification.
- No trailing commas.
- Files are UTF-8, with a single newline at EOF.

Key ordering inside objects is stable and intentional.  
Where no domain-specific order is required, the following pattern is encouraged:

1. Structural and meta keys first (for example `_meta`, `id`, `type`, `version`).
2. Core domain fields.
3. Optional or auxiliary fields.

JSON meant for humans (configs, fixtures, sample payloads) must be pretty-printed.  
Machine-only artifacts (caches, dumps, transient intermediate results) may be minified, but must not live alongside hand-edited configuration.

Formatting is enforced in CI using a single canonical tool (for example `jq`, `prettier`, or equivalent).  
A diff that deviates from canonical formatting is rejected.

---

## 6. Ethical Layer  
**Chosen Approach:** Data as responsibility, not convenience  

JSON documents frequently encode user data, permissions, policy, and trust relationships.

SSE-JSON imposes the following:

- No secrets in committed JSON:
  - API keys, private keys, passwords, access tokens, refresh tokens, encryption keys, or similar material must not live in version-controlled JSON.
  - Use environment variables, secret managers, or encrypted stores.
- No unnecessary raw PII in demo data:
  - Use synthetic or anonymised examples in fixtures and documentation payloads.
- No hidden policy flips:
  - Fields that affect user privacy, consent, or control must be named plainly and documented.
  - Defaults must be chosen conservatively in favour of user agency.
  - Silent behaviour changes that diminish user rights are not acceptable.

Any configuration surface that might materially affect user security or privacy must:

- Be documented in clear language.
- Be auditable (visible in configuration/state dumps).
- Prefer “opt-in” over “opt-out” for invasive or aggregative behaviour.

---

## 7. Auditability and Temporal Trace  
**Chosen Approach:** Versioned, timestamped, reconstructible  

For long-lived JSON documents that influence system behaviour (configuration, policy, contract definitions), SSE-JSON encourages explicit temporal metadata.

Where appropriate:

- Include a `version` field (semver or a documented numeric scheme).
- Include `last_updated_utc` as an RFC-3339 timestamp.
- Record changes in a human-readable changelog or ADR.

For JSON that represents audit events, each event should include:

- `ts_utc` — RFC-3339 timestamp.
- `actor` or anonymised actor identifier.
- `action` — a short verb phrase, stable enough to be used in metrics.
- `context` or `details` — a structured object with relevant, non-sensitive details.

Audit payloads must not carry secrets, credentials, full raw PII, or high-risk sensitive fields.  
Where reference to such data is needed, suitable redaction or hashing is required.

---

## 8. SSE-JSON Standard Template  

SSE-JSON replaces comment-heavy headers with a structured `_meta` block for central documents.  
This block lives at the top level and provides machine-readable context.

```json
{
  "_meta": {
    "etiquette": "Synavera Script Etiquette — JSON Profile v1.2",
    "project": "PROJECT OR MODULE NAME",
    "document": "relative/path/to/file.json",
    "purpose": "Short narrative of what this JSON controls or represents.",
    "owner": "Team or role responsible for this document.",
    "security_notes": [
      "Threat assumptions, permission boundaries, irreversible actions.",
      "If none, use: "N/A"."
    ],
    "dependencies": [
      "Upstream systems or schemas this document relies on.",
      "Downstream consumers that assume its structure."
    ],
    "operational_scope": "Where, when, and how this document is loaded and applied.",
    "revision_history": [
      "2025-11-26 CMD  Created initial JSON profile.",
      "YYYY-MM-DD XX  Short description of the change."
    ],
    "sse_principles_observed": [
      "No silent failures; consumers validate and fail loudly.",
      "No secrets or raw PII in committed JSON.",
      "Stable, documented structure with clear versioning.",
      "Narrative context maintained in adjacent markdown or schema."
    ]
  },

  "config": {
    "...": "Domain-specific configuration lives here."
  }
}
```

Notes:

- `_meta` MUST NOT contain secrets.
- For very high-volume runtime payloads (for example request/response objects, event streams), `_meta` may be omitted to keep payloads small and fast; in such cases, the contract and SSE-alignment are documented in schemas and markdown rather than inside each payload.
- For JSON Schema or OpenAPI files, `_meta` may be included alongside `$schema`, `title`, and other standard fields, provided it does not break downstream tooling. If tools are too strict, move the SSE narrative to an adjacent markdown file referencing the schema’s path.

---

## 9. Tooling and Validation  
**Chosen Approach:** Automated guardians at every boundary  

SSE-JSON artifacts are never committed directly from an editor without automated checks.

For configuration, contracts, and fixtures:

- A strict JSON parser must accept the file (no comments, no trailing commas, correct string quoting).
- A single canonical formatter must produce a stable representation; CI must enforce this formatting.
- Where schemas exist, validation is mandatory; a change that breaks schema must either:
  - Update the schema and document the migration, or
  - Be rejected.

CI and pre-commit hooks should:

- Reject invalid JSON.
- Reject formatting drift.
- Reject fixtures and sample payloads that no longer conform to their declared schemas or OpenAPI components.

Where JSON format is externally dictated by a third-party system and cannot be made fully SSE-compliant, such files must live under clearly named quarantine directories (for example `third_party/`) and be accompanied by a local README explaining the deviation and any mitigation.

---

## 10. Conformance  
**Chosen Approach:** Universal precedence  

SSE-JSON inherits all universal principles from Synavera Script Etiquette v1.1.  
Where an apparent conflict exists between this profile and the universal etiquette, the universal etiquette prevails.

Any JSON artifact that claims to be “Synavera-compliant” must:

- Either embed an `_meta` block as defined above (for configs, policies, schemas, or other central artifacts),  
  or be clearly referenced from higher-level documentation that carries the SSE header and links directly to the artifact.

Opaque, unaudited JSON that hides behaviour, policy, or user-impacting decisions has no place under the Synavera name.

#============================================================
*End of Synavera Script Etiquette — JSON Profile v1.2*
