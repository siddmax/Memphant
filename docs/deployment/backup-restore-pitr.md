# Backup, Restore, and PITR Reconciliation

Postgres PITR is authoritative. The object store is reconciled by blob presence after the Postgres restore; it is not rolled back independently.

## Runbook

1. Stop MemPhant writers and workers.
2. Restore Postgres to target time `T1`.
3. Keep blob GC disabled until this runbook reaches the final integrity gate.
4. Enumerate every live row with a `blob_hash` from restored Postgres.
5. Check the customer bucket for each content-addressed object.
6. If the object is present, keep the row live.
7. If the object is present only as a noncurrent version, restore the object version and record `restore_blob_undelete`.
8. If the object is absent and the key has a verified deletion tombstone, record `restore_blob_shredded` and tombstone the row.
9. If the object is absent without a tombstone, record `restore_blob_missing{hash}` and block release.
10. Resume blob GC only after there are zero unexpected missing blobs.
11. Run a recall parity check for restored tenants and a cross-tenant leakage check before reopening writes.

## Retention Floor

Object-store versioning must be enabled, and noncurrent-version retention must be at least the Postgres PITR window plus margin. The bootstrap preflight reports `restore_retention_floor_violation` when this is false because a PITR can otherwise resurrect a row whose blob was permanently deleted.

