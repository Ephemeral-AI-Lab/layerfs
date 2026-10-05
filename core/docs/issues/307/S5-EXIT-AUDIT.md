# S5 exit audit — payload and streams

> **Status:** COMPLETE for the S5 implementation/exit scope after `f5558fc22`.
> Not release, cache-cold performance, native FUSE or integrated Commit evidence.

The [S5 slice](../303/07-implementation-validation.md#3-slices) and
[handoff](HANDOFF-S5-S6.md) select cells/tails/validity, append/overwrite/inherited
reads, cutoff truncate/regrow, sparse holes and canonical zero-run construction.
The [implemented contract](../../architecture/31-payload-streams.md) links owning
source. S6 and later integrated acceptance retain their separate required exits.

| Exit | Evidence and result |
| --- | --- |
| Fragmentation-bounded mutation, R1 | The independent [65,536-write target](../../../crates/layerfs-overlay/tests/payload_fragmentation.rs) preserves every mutation and compares full-window and unaligned overwrite with an unfragmented file. MacOS and Linux pass. Full overwrite: 45 statements, 2,752 VM macOS, 36 changed rows. Only two unaligned edge cells are read; earlier one-byte fragmentation does not grow a later request |
| Append and atomic current EOF | `Position::End` selects current size inside the real Namespace job and publishes bytes, size/mtime and ticket atomically. Four concurrent writers append 1,000 distinct 23-byte records while a tail reader sees only whole records. Every writer's order and exactly-once final contents pass on both hosts |
| Nonzero shrink/capture/regrow, no resurrection | Reference-model writes/resizes/captured reads/known installs pass; Workspace constructs and installs a real canonical root, then proves later active bytes/cutoffs. Shrink discarding 2 versus 2,048 cells costs the same 16 statements; a 1,500-step staircase stays logarithmic. No discarded-data scan runs in truncate |
| No WRITE base copy-up | First inherited write demands only the 2,094 attribute-fact bytes; later writes demand none. Window data is supplied without acquiring inherited payload |
| Bounded READ gaps | 2,048 alternating inherited gaps demand the same five upstream objects as one unfragmented covering base range; a fully overwritten window demands none. Local read buffers/cells are bounded by one 128 KiB request, independently of file length |
| Holes and canonical P4 | Overlay 1 TiB holes have no gap rows. Public `construct_runs` preserves streamed roots at cutoff/CDC/leaf boundaries and odd input windows; multi-level repeated mapping pages equal an independent ordinary builder oracle. A 1 TiB hole between real bytes processes 32 KiB of zero input, emits 13 mapping pages and reads exact boundary/interior bytes; children precede parents and consumer refusal stops one attempt |
| Tiny/dense/page/copy/custody accounting | 2,000 100-byte files use 184 shared allocated bytes each; dense cells use 1.133 pages each on macOS. Request bind/copy work is actual data plus at most two 4096-byte edges and masks. One-byte append replaces one bounded cell. SQLite page/journal copies are additional; current safe-driver diagnostics do not expose exact journal/dirty-byte or whole-process/pager/OS-cache peaks. These are explicitly unqualified S7 observations. Owner reply credits and the additional Workspace clone/base/sink/native custody are documented; native aggregate output remains S8 |
| EXPLAIN plus complete-operation profiles | Indexed inode/layer/cell/staircase plans and real-owner work at 128/1,024/4,096 unrelated files, identical at every scale, zero fullscan/sort/autoindex/reprepare. The totals below include all source/semantic/publication/reply jobs and the post-observation route seek |
| P3 disposition | `EDIT_DEFERRED_LIMIT=8 MiB−1` still guards resident draft/reference/detached/committed maps. Removing its check would create unbounded resident state. Per the S5 handoff's explicit option, [P3 is carried to backed S10 editing](S5-HOLE-CONTRACT.md), including draft identity exhaustion and sparse replacement wiring. P3 is not claimed resolved; it remains a hard S10 dependency |

| Complete owner operation | Statements | VM macOS / Linux | Changed rows | Bound bytes |
| --- | ---: | ---: | ---: | ---: |
| overwrite 128 KiB | 91 | 3294 / 3226 | 40 | 133240 |
| shrink | 62 | 1069 / 1029 | 10 | 1011 |
| regrow | 59 | 926 / 890 | 8 | 888 |
| read 128 KiB | 73 | 1377 / 1321 | 4 | 1568 |
| append | 61 | 1005 / 968 | 9 | 964 |

These are deterministic correctness/work diagnostics, without a latency/cache
campaign or claimed RSS bound. Source-window and diagnostic route checks are
included rather than silently removed. Raw [S5 checks](checks/s5-payload/) retain
all failed cases and their [diagnoses](checks/s5-payload/failures.md).

## Final covering checks

Rust 1.85.1, locked core manifest/lockfile and repository ARM64 profile. Linux uses
image `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`,
worktree-local target `core/target/cluster2-linux` and Cargo cache
`core/target/cluster2-linux-cargo`. No third-party source or lockfile changes.

Build first with all-target `--no-run`; then each package runs once under an
explicit 120-second wall ceiling. MacOS uses a process-group timeout that kills
and joins the command on expiration. Linux uses `timeout --signal=KILL 120` inside
each owned disposable container. No invocation reaches its ceiling. The final
macOS content package is 42.442 seconds including command overhead; no test is
backgrounded to escape its bound and no unchanged treatment is resampled.

- All 11 active core packages/all targets: PASS, 610 tests, per-package raw
  `core-layerfs-*-final.log` in the checks directory. This is equivalent coverage
  to the active core all-target selection, split to respect the test ceiling.
- Linux ARM64 content/overlay/Workspace/daemon/SDK all targets: PASS, 348 tests;
  SDK's six real global-provider tests execute on macOS and compile only on Linux.
  The existing Linux persistence dead-code warning is retained, not suppressed.
- All-target warning-denying Clippy: PASS (`clippy-boxed.log`); fmt/whitespace:
  PASS. Product boundary: PASS, 522 production Rust/SQL files. Tool tests: 26 OK.
- Affected local documentation links: 224 links and 16 anchors checked, none missing. Source/API,
  canonical compatibility, declared capacities and platform cfgs reviewed.

Raw content-test stdout contains trailing spaces in diagnostic survivor labels;
`git diff --cached --check` reports those preserved bytes in the two raw content
logs. [Raw whitespace receipt](checks/s5-payload/whitespace-raw-receipts.log)
retains that result; source/docs whitespace excluding raw log data passes.
No raw receipt is edited to hide the diagnostic output.

## Remaining ownership

S6 owns automatic removal of stale cells/abandoned shrink steps/unused whiteouts,
independent open-unlinked custody, bounded repeated failed-capture composition,
reader/capture/operation eligibility, retired-generation cleanup and physical
headroom/device-full outcomes. S5 reads can still walk every live generation;
S6 must bound that chain. Symlink creation still uses one cell. Raw cell APIs
are not effective reads. S7 owns detailed journal/pager/residency observations;
S8 owns reply/kernel/lookup/open/mmap/native semantics. P3 and sparse edit/Commit
wiring remain S10, together with the other declared backed-construction gates.

Published fuser 0.18.0's signed-timestamp blocker remains S0/S8/S12. No push,
release, deployment, legacy retirement or integrated qualification occurs here.
Unrelated documentation/#301/research/output and four unrelated containers are
preserved. Exact parent/staged/committed production LOC is recorded with the
completion commit and its retained tracker receipt.

Production LOC: 146209 -> 147386 (delta +1177). Core 80792 -> 81969 (+1177);
reference 65417 -> 65417 (+0). Exact first-parent/final-staged archives of
`core/crates` and `crates`, unchanged `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`;
product src/shipped SQL including excluded source, excluding tests/inline tests,
docs/tools/examples/harnesses/manifests/builds. Receipt:
`core/target/cluster2-307/loc/s5-complete-staged.json`. No relocation, duplication
or legacy retirement is claimed.
