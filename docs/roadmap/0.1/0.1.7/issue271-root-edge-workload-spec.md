# Issue 271: mounted-write root-edge cost selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before any #271 runner change, sample, or product treatment. The
starting combined source is `6bcfa464f74ae9ca3859df31c678985ec69ba098`
(#267 then #268 merged into draft #262). The isolated #266 4,097 attempt stays
`FAIL` at 25.006537 s with no callback count or Commit. Its 512 and 1,024
ownership counts are cause evidence, not a combined-source comparison.

## Public workload and one-shot selections

- Reuse the existing locked-release `benchmark_shell` SDK driver, the generic
  `write-separated` one-process/one-fd writer, and the prepared-master byte-copy
  clone machinery. Every selected run makes one `WorkspaceApi::mount`, one
  `WorkspaceApi::exec` launching the writer, and one explicit
  `WorkspaceApi::commit` after successful Exec. The driver calls no internal
  mutation method. The 8,194-byte old file contains `A`; the writer issues
  exactly `N` one-byte positional writes at offsets `2*i`. Select `N=100` and
  `N=512` on the combined baseline and on each changed final product identity
  only where affected. Do not resample an unchanged identity. The 10 MiB
  append/dispersed/repeated `N=100` sibling workload is checked at final source.
- The 4,097 route uses the same writer and public API. Attempt its #248 gate
  once at the final frozen identity **only when source and lower-count evidence
  make it meaningful**. Keep the ordinary 15 s complete-command budget and the
  previously declared 25 s exception, the separate under-10 s verifier, and
  the existing product Exec timer owned by #249. No workload, deadline, worker,
  cache or snapshot interval is altered to turn a miss into a pass.
- For each case, an independent read-only verifier checks exact old/new heads,
  parent relation, full old/new bytes, changed runs, modes and path inventory.
  Record actual FUSE WRITE callbacks; a claimed `N` with fewer callbacks is
  `FAIL`. Retain Exec, Commit, complete wall, verification, resources, positive
  cleanup, and every FAIL/INELIGIBLE attempt at a fresh output path.

## Cost question and admissibility

The hypothesis is that copied extent branches name every live child and charge
an add on publication and a removal on old-root cleanup. Across the observed
128–1,024 #266 checkpoints, ledger read/write counts rise quadratically; this
does not establish a global time exponent. On the combined source, capture
within-run progress and reconcile each interval's ledger 4 KiB reads/writes,
root height, branch children, child edges added/removed, extent page
visits/writes, payload/ownership I/O, FUSE callbacks, control/Service round
trips, Store work, resource charges, and cleanup. Compare counts first. Raw
wall is diagnostic while ordinary host/container cache is uncontrolled:
latency is `INELIGIBLE`, not a performance PASS or a cross-source speed claim.

Choose the smallest shared-path fix only after the combined count and source
model agree. Preserve per-write atomic publication, old roots and G1/G2 custody,
exact Commit roots, page identity/checksum validation, definite and uncertain
failure handling, quotas and refunds, one construction worker, and generic
shell semantics. A new page format or structural sharing rule needs explicit
native old-root, partial-failure and reclaim proofs. Update the affected
architecture document in the algorithm commit and record exact first-parent
production LOC for every commit. Use one focused covering test command under
30 s; do not rerun passing unaffected checks.
