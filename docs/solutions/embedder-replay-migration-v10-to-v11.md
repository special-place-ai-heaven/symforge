# Embedder replay migration: v10 FileReplayStore to ReplayStore

This is the host procedure for a state directory that still has v10 JSON records under `idempotency/records/`.

## Host procedure

1. Stop every old writer. Confirm those processes cannot start again against this state directory.
2. Build checkpoint evidence from the bytes that record that stop (`LegacyCutoverEvidence::from_checkpoint`). The bytes are scanned and only their digest is stored.
3. Call `ReplayStore::confirm_legacy_cutover` with that evidence.
4. Archive or remove `idempotency/records/` outside this API. SymForge does not modify those bytes.

Until step 3, a records directory blocks every new reservation (`LegacyCutoverRequired`). After step 3, a key with no old record can be reserved. A key whose old record file is still present keeps returning `LegacyStateRequiresReconciliation`.

## Old records

Each old per-key record blocks that key until the host has reconciled it. That includes old Reserved, Completed, and Failed records. Removing or archiving the records directory is what lets those keys be admitted again. There is no automatic replay of an old record into a new execution.

## Key hashes

Old and new key hashes are equal. Both hash the raw key bytes framed by `symforge-idempotency-key-v1` followed by a NUL byte. `legacy_key_hash_matches_replay_key_hash` checks the v10 `IdempotencyKey` hash against that frame and checks that `ReplayStore` blocks the directory named by that hash.

Request hashes keep the old framing through `RequestFingerprint::for_json`.

## Lease column

Opening a store adds nullable `lease_expires_at` and `uncertain_since` columns to an existing `replay-v1.sqlite3` when they are absent. A row with no expiry, including every row written by `reserve`, never expires.
