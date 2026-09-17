# Feature 020 V11 Detached Refreeze Attestation

This detached attestation binds the refreeze manifest to the exact baseline, design, context, and public API identities below. It is not an approval or a signature. Implementation remains gated on a separately stored, externally signed approval record in the required namespace.

<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON START -->
```json
{
  "amendment_set_id": "6d4e16060e3362a5a00f88c1ae116ac7cc996fce5a27945caab6173fbb95a77d",
  "baseline": {
    "commit": "1521abb0197dac16e046a2b0b20a66a70c3a909b",
    "tree": "c26043df97571dd079681291d2621a4e06438d8d"
  },
  "context": {
    "path": "CONTEXT.md",
    "sha256": "ea7fca771e080b20ae38c0fd15db97fafe111d536e59c0eff31c062e6762fb26"
  },
  "design": {
    "path": "docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md",
    "sha256": "9b0a1b79b20bc70197a438409e8484e74319888ee5ded5bba39452b6b301bf5b"
  },
  "external_approval": {
    "purpose": "implementation_start",
    "required": true,
    "signature_namespace": "symforge-feature-020-refreeze-v11"
  },
  "kind": "symforge-feature-020-refreeze-attestation",
  "manifest": {
    "path": "specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md",
    "sha256": "80c076ef486edd9d7cca2e25656765811c8a597306aca7984c7cd2f95f3311ab"
  },
  "public_api": {
    "canonical_sha256": "e97d5131d24794e789e324a49026dec718bb9f93500023cbfeccb3af2dd4ae9b",
    "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "raw_sha256": "a3d2fb4fabe932e988a195ec9c8736876a891f09f01ed580e4eee4e42793b0a2"
  },
  "schema_version": 1
}
```
<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->
