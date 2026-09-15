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
    "sha256": "a2a23c4d5d172a15eb4867ca401e54c36397aa77bc21acdbf2f71bee5764d669"
  },
  "public_api": {
    "canonical_sha256": "6f66210d7992d02f6ed0ee800bd2d137b408004455a7c5e5787a7c2e7e688b3a",
    "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "raw_sha256": "576dffe38b047499effb77bb7d98aceb56c9aae2941bf4013ec2efb25b16755e"
  },
  "schema_version": 1
}
```
<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->
