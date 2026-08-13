# Feature 020 V11 Detached Refreeze Attestation

This detached attestation binds the refreeze manifest to the exact baseline, design, context, and public API identities below. It is not an approval or a signature. Implementation remains gated on a separately stored, externally signed approval record in the required namespace.

<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON START -->
```json
{
  "amendment_set_id": "4e44bfef7dbf4aa4b7c67641c6e2bfb7323261036e0e67d68270ff0362b7c0db",
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
    "sha256": "49f334f690743cb18c8899a39a67595025f90e2693a8fd704c7cb493ebd83215"
  },
  "public_api": {
    "canonical_sha256": "c45f3cd3f77e5690ad1dcd2e5fc7e39e30d52df38fa564d7b663e1c95823a7da",
    "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "raw_sha256": "5e5b47b110b27f57f5cb83506130131be772047b03a0f4cacc3412e60718f5a9"
  },
  "schema_version": 1
}
```
<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->
