# Admission of ambiguous credential-key metadata

Status: closed 2026-10-23 as expected conservative admission, not a confirmed detector defect. Historical observation: 2026-10-20 while dogfooding SymForge v11.9.1 for AAP S14-S16.

`get_file_content` refused the newly authored `specs/020-repository-knowledge-index/contracts/embed-refresh-v1.json` with rule `secret.context-assignment` at line 14. That historical contract path has since moved to `docs/contracts/embed-refresh-v1.json`. The author identified the field name `token` as innocuous contract metadata, not a credential. No refused content was retrieved through a fallback.

The author renamed that known schema field to `change_version_type` from the original authored patch. A subsequent SymForge read admitted the document. The rejected spelling's value was not reproduced in chat.

The original investigation request was to examine context-only matching on schema descriptions with a minimal non-secret fixture and positive credential controls. A fix must preserve credential withholding; do not blanket-allow fields named `token`.

2026-10-22 dogfood observation: `get_file_context` also withheld `scripts/validate-lifecycle-oracle-traceability.cjs` under `secret.context-assignment` (one match). The user explicitly authorized a redacted Terminal Commander inspection to unblock CI work; sensitive-keyword lines and adjacent context were suppressed before output. No secret value was reproduced, and the detector policy was not changed.

## Current-source disposition — 2026-10-23

A disposable probe used the current detector functions byte-identical to `aadad9de`, with the real regex dependency. It checked six synthetic cases: identifier-like type metadata under a credential key was withheld; a descriptive type field, an explicit placeholder and a short natural-language label were admitted; runtime-generated credential-shaped controls remained withheld. No credential values were printed and no refused file was retrieved.

An arbitrary opaque string under a credential key cannot safely be distinguished from a real credential solely from that assignment. Keep the fail-closed policy. Use descriptive field names for type metadata, or explicit placeholders when documenting credential examples. Do not allowlist all credential keys or arbitrary identifiers.

Terminal Commander receipt: `job_01a16803e327769184ad94b8df5c6cf3`, observed exit 0; compilation and all six assertions passed; temporary files/processes cleaned up. This establishes current detector behavior, not an integration test of the original unretained input or withheld CJS content. Reopen only with a safely retained, nonambiguous reproduction that contradicts the intended policy.
