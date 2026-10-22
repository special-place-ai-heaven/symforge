# Follow-up: secret admission false positive on a schema field

Status: open; observed 2026-10-20 while dogfooding SymForge v11.9.1 for AAP S14-S16.

`get_file_content` refused the newly authored `specs/020-repository-knowledge-index/contracts/embed-refresh-v1.json` with rule `secret.context-assignment` at line 14. The author identified the field name `token` as innocuous contract metadata, not a credential. No refused content was retrieved through a fallback.

The author renamed that known schema field to `change_version_type` from the original authored patch. A subsequent SymForge read admitted the document. The rejected spelling's value was not reproduced in chat.

Investigate context-only matching on schema descriptions and add a minimal non-secret contract fixture alongside positive controls for real credentials. A fix must preserve credential withholding; do not blanket-allow fields named `token`.
