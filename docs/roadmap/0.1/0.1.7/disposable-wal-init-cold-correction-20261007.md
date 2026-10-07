# Correct the observed 100k Init source-reader interference

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Continuation of the owner's full Disposable WAL matrix request and the
[frozen original plan](disposable-wal-init-history-20261007.md). At `5ba8362cb`,
Init100/1000/10000 complete and pass their functional/cold/cleanup checks.
Init100000 v1 is INELIGIBLE with zero product samples: the native helper found
1626 resident pages after invalidation. A read-only `lsof` snapshot then found
Spotlight `mdworker_shared` holding30 descriptors on15 files in that fixture.
Original receipts13–16 and diagnostic17 are retained under
[the campaign checks](../../../../core/docs/issues/307/checks/disposable-wal-matrix-20261007/).
No failed or fast product sample is replaced.

## Prospective corrected selection

Register only `phase7-sqlite-disposable-init-100000-owner-wal-v2` as a new
selection. Its source preparation root is the owned worktree-local
`benchmark-results/fs-bench-pro/disposable-wal-prepared.noindex` directory.
Make an independent byte-stream copy of the existing closed prepared fixture;
verify every copied file against the original manifest while copying with a
128KiB window, preserve all metadata, and record source/destination manifest
hashes. Retain the original fixture. No global Spotlight setting or process is
changed; directory naming is an interference-avoidance measure, not a cold claim.

The new sample keeps exactly100000 files/500000000B, the same manifest, seed,
native helper, product/driver binaries, four Init constructors, WAL/OFF profile,
30s complete command and19s separate proof, and all original oracles. The
in-run zero-resident-page attestation remains mandatory. One new invocation,
no sleep/readiness/preconditioning/retry loop. Retain any further refusal.
The three passing Init cases are not repeated. Then run the three still-unrun
history cases in their registered10/3/1 order.

Deepest files: add one distinct row with `prepared_root` and `supersedes` to the
existing WAL matrix registry; let `serverless_init.py` select that explicit
prepared-source root; extend its registry test to preserve v1 while checking
identical workload/limits. No production source, native executable, canonical
format, durability or numerical limit changes. Report the placement change and
different harness commit; comparisons remain historical and unpaired.
