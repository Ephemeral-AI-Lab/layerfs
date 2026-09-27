# Issue 266: 1,024-write scaling attribution

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This distinct count-driven diagnostic is declared after the fixed-source 512
receipt and the retained 4,097 FAIL, before adding the selection or running
it. It measures whether the within-run growth already visible at 512 extends
to 1,024. It is not a new 4,097 arm, a replacement for either FAIL, or a
latency admission result. Source product code, generic writer, fixture, cache
policy, worker count, callback deadline, 15-second diagnostic complete-command
limit and independent verifier remain unchanged.

Selection `issue266-separated-1024-v1` uses the same
`separated_writes.py` → release `benchmark_shell` public route as 100, 512
and 4,097: one WorkspaceApi mount, one Exec of
`/fixtures/bin/write-separated data.bin 1024`, one explicit Commit only after
successful Exec, and the generic one-process/one-fd writer. The writer makes
one-byte positional writes at offsets `2*i` for `i=0..1023`. The old head is
the closed, previously verified 8,194-byte master; a new independent writable
byte copy supplies the run. Expected final state has unchanged size/mode,
1,024 separated changed runs, 1,024 replacement bytes and 2,048 file
extents. The independent oracle checks both exact heads and all old/new file
bytes, parent, inventory, mode and length.

Use four cumulative progress and post-reply backing/metadata snapshots
at 256, 512, 768 and 1,024 accepted WRITEs. The diagnostic image sets only
the existing snapshot interval to 256; the production path and reply-wait fix
are unchanged. Capture actual FUSE WRITE callback count, cumulative payload
acquisition and Workspace publication nanoseconds, direct 4 KiB ledger
reads/writes, metadata-page reads, retained/allocated payloads, roots/pages,
routine scans, Commit lower/C1/Store counts, Exec/Commit/whole-command and
verification wall, resource scopes, positive or retained daemon close,
cleanup, source/product/harness and binary/image/master hashes. Derive each
256-write interval by subtraction of checkpoints; do not present one
cumulative counter as a phase count. The optional snapshots themselves make
this a diagnostic, and ordinary cache is uncontrolled, so latency stays
`INELIGIBLE` even if functionally correct.

Take one attempt at one frozen source identity in a fresh output directory
under this worktree's run lock. Retain every FAIL, timeout and missing oracle.
Do not raise any deadline or worker count, alter the writer or prewarm paths.
Interpret growth against the already retained 512 within-run counts and the
specific source loops; no 4,097 callback count is inferred from a timed-out
driver. #265 owns payload/extent/C1 algorithm changes, and #249 owns the
30-second product Exec timer.
