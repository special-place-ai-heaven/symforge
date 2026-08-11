# Feature 020 V11 Detached Refreeze Attestation

This detached attestation binds the refreeze manifest to the exact baseline, design, context, and public API identities below. It is not an approval or a signature. Implementation remains gated on a separately stored, externally signed approval record in the required namespace.

<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON START -->
```json
{
  "amendment_set_id": "ac10f03ed9152724bc20fefdaa7ce4e274c661b9b459f4b43d3ed178c6656859",
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
    "sha256": "1b5718bf3d3a71f0f15e0258094e402b92d7feb62ac17034f28e245a6c030288"
  },
  "public_api": {
    "canonical_sha256": "d8ea37cfb7d72f6bcbf01447ca8c9fe6d35174c58a07061b1b6e86ee965e1a48",
    "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "raw_sha256": "c9594c8a33d6916ad0455feefa8404c93a0f541c857a15af9ac5bf8b1bed9b3e"
  },
  "schema_version": 1
}
```
<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->
