# #232 repeated-128 liveness diagnostic

**Frozen before execution.** This is a distinct, one-attempt diagnostic of the
retained Phase 1C `repeated-128` failure, not a replacement sample or a cold
latency gate. The original nine receipts and 56 single-edit campaign remain
unchanged. Source parent is `363cc6b26d87e4a6eb9d64ee269b45060bf85ca8`.

Use the same sealed 1 MiB prepared master and 4,096-byte replacement recipe,
cloned to a fresh writable Store/history pair. Use the public SDK
Exec→mounted `splice-batch` command, with 128 checked 32-byte EDIT ioctls at
the frozen Phase 1C offsets. Do not change the native bridge's 5 s progress
boundary, SDK 30 s deadline, callback 10 s budget, one construction worker,
cache policy or verification. The command and complete run each get one
attempt. Retain errors and cleanup outcomes even if Exec is unknown.

The new workload diagnostic flag counts cumulative elapsed time for the 128
`run_with` calls, separately for pre/post STATE ioctls, `fstat`, bounded POSIX
reads, EDIT ioctls and residual command work. It writes one summary into the
daemon log after the last checked edit. These local interval sums are diagnostic
attribution, not latency admission. The existing caller, Service and daemon
LFT1 Exec/ReadFile records supply the authoritative root and cross-process
window. Existing `LFS_PIECE_COUNT`/`LFS_PIECE_PAGES` lines provide exact splice
visit and rebuilt-page counts. No per-phase count is silently converted into
a wall-time claim.

Record source/tree, product/compilation/harness and workload hashes, host
binary and image hashes, exact prepared-master hashes and clone method, case
fields, cache label, LFT1 output, counters, SDK outcome, independent verifier
when Commit exists, Unmount and Delete. Complete command limit is 15 s and
independent verifier limit is 10 s. A failed/unknown Exec is FAIL, regardless
of any eventual daemon completion. Linux FUSE backing remains INELIGIBLE for
cold latency without invalidation and full residency checks.
