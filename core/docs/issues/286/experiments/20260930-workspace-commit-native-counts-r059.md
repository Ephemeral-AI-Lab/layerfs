# #286 r059: native8192 local C5 Capacity and charged-cause diagnostic

**8192-count FAIL**, complete12,746,044,625 ns <60 s; actual canonical Commit was known Committed, but local Reconcile returned KnownCommitLocalFailure / Capacity with installed_revision absent.10240 NOT_RUN after failure. Product cleanup UNKNOWN; archived retained Linux state/Store and checked external container/volume removal PASS. Source9effbb614, exact release test artifact and prepared327,680-byte independent byte-copy fixture in the [compact receipt](20260930-workspace-commit-native-counts-r059-receipts.json). Default8 MiB memory and64 MiB disk quota were unchanged.

A separate **count-driven cause diagnostic**, not a replacement gate arm, reused the same binary and master with the existing LFS_CAPACITY_DIAGNOSTIC observer. It produced the same known-canonical/local-C5 Capacity disposition, preserving the gate FAIL. Counter output identifies the overlap precisely:

```text
LFS_C5_CHARGE v=1 phase=prepared dirty_rows=1 deletion_keys=24576 deletion_key_bytes=499712 rows_charge=448 deletion_charge=3145728 updates_charge=3646138 budget_before=1043154 budget_after_prepare=4189330 budget_after_updates=7835468
LFS_C5_COMPACTION v=1 status=aborted branch=unreached initial_updates=24681 final_updates=24681 estimate=0 remaining=NA touched_logicals=0 all_logicals=0 scan_calls=0 scanned_locators=0 pressure_pack_reads=0 source_pack_reads=0 partial=0 paired=0 physical_pack_attempts=0 physical_pack_created=0 sources_selected=0 budget_before=7861115 budget_after=7861115
LFS_C5_FAILURE_CONTEXT v=1 known=true installed_revision=NA class=capacity budget_used=1042758 allocated=3620864 reserved=843776 quota=67108864 active_revision=8193 pages=882 pins=0
```

Immutable rows retained24,576 deletion keys/499,712 key bytes under3,145,728 B source-list charge. The already-admitted patch was charged3,646,138 B, bringing Budget to7,835,468 B. Pruning added105 keys (24,681 total); before ordered transfer Budget was7,861,115 B. The ordered tuple scratch required24,681*48=1,184,688 B, so simultaneous admission needed9,045,803 B > fixed8,388,608 B. The retained source deletion list was redundant once its keys entered the patch. This is a source-backed local completion ownership defect, not a codec, storage-size or deadline regression. The diagnostic wall is not a second speed sample and cannot replace the gate row.

The fix moves each deletion key into the already-pre-admitted patch, frees the emptied immutable source list and its3,145,728 B charge before ordered/index scratch admission, and preserves the row's small original metadata facts. No Budget/physical quota/worker/deadline/format change and no uncharged allocation. Atomic publication, G1/G2 matching, known outcome and held-view/physical failure custody remain. Architecture is updated in the same commit.

Next: native counts once at changed product source, then only affected Family4 reconciliation proofs and an affected Family3 representative. Family2 is InProcess Store/history with no Workspace reconciliation dependency and remains reused, as do F1 and stopping/read-only routes. No broad earlier-family sweep. Original FAIL and diagnostic files stay append-only. Production change is minimal and counted from exact snapshots in the fix commit; no compression change.
