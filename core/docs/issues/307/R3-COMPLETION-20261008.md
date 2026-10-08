# R3 completion: native mutation and kernel coherence

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

R3 of the cluster-two rollout is implemented and functionally verified on real
kernel mounts. A mounted Workspace now serves ordinary mutation to any process
with mount visibility and permission, through the existing Workspace plan and
one publishing Overlay job per request. No second mutation engine, no kernel
notification and no writeback cache were introduced. R5, R6 and timing
qualification are not started; R4 keeps its earlier component state; the root
reference tree is intact.

Architecture: [native mutation and kernel coherence](../../architecture/77-native-mutation-coherence.md).
Assignment: [R3 handoff](HANDOFF-R3-MUTATION-COHERENCE-20261008.md).
Evidence, including every failed attempt and unrun case:
[R3 checks](checks/r3-mutation-20261008/00-results.md).

## What is wired

| Handoff step | Delivered |
| --- | --- |
| 1 Unexecuted R2 paths | Kernel FORGET observed on a live connection with its exact decrement (26 units for 26 inodes over 27 names, owners back to baseline); one normal unmount staged to end `Retained` at stage `Revoke`, with later Unmount and Attach returning the identical custody |
| 2 Writable OPEN and handle custody | Writable descriptors are engine open handles; a handle-addressed mutation resolves its descriptor and an independent processing source in one job; CREATE acquires its descriptor in the publishing transaction |
| 3 WRITE and SETATTR | WRITE always at the kernel's offset with a full-count reply; truncate, shrink, regrow and `O_TRUNC`; chmod and explicit modification time |
| 4 Namespace mutation | CREATE, MKNOD (regular), MKDIR, SYMLINK, LINK, UNLINK, RMDIR, RENAME with replacement and no-replace; entry replies acquire their kernel lookup reference in the publishing transaction; cross-parent directory moves prove ancestry from the connection's retained parent index |
| 5 Removed references and aliases | Open-unlinked content and mutation; removed working directory and `O_PATH` with exact attributes from lookup custody alone; a removed file stays openable under a live kernel reference; hard-link aliases share one inode and one page cache |
| 6 Shared mappings | A store arrives as a WRITE with the page-cache flag, is clipped to the current size, never changes size and never produces a SETATTR |
| 7 Proofs and documentation | Rows below; architecture 77 and updates to 30, 75, 76, the index, `core/AGENTS.md` and the rollout ledger |

The mount answers `EROFS` for nothing. What stays refused is listed in the
architecture record; FALLOCATE and COPY_FILE_RANGE moved from `EROFS` to
`ENOSYS`, and FIFO and socket nodes from `EROFS` to `EPERM`.

Control records, Bridge and the SDK are unchanged. The SDK full-topology
example was updated because its R2 step asserted that mutation was refused.

## Proof rows

Linux, pinned image, kernel 6.12.76-linuxkit, Disposable/WAL/OFF Store,
natural caches, one run per attempt. `LAYERFS_CONSTRUCTION_WORKERS=1`.

| Row | Result | Where |
| --- | --- | --- |
| FP-10 write and `O_APPEND` | PASS | native_mutation; cached-read request count in native_coherence |
| FP-11 create, unlink, replacement, negative lookup | PASS | native_mutation |
| FP-12 lookup racing directory mutation | PASS at attempt 2 after a product correction | native_coherence |
| FP-13 truncate, shrink, regrow, `O_TRUNC` | PASS | native_mutation |
| FP-14 hard-link aliases | PASS | native_mutation, native_coherence |
| FP-15 shared mappings | **PARTIAL** | native_coherence. Executed: `msync`, `munmap` without it, map–close–store–exit, the flag count, no SETATTR, store beside an append handle, store past the end then extension. Not observed: the writeback request's header identity. Commit inclusion is R5 |
| FP-16 reply ordering | **PARTIAL** | native_coherence. Executed: size never decreases under concurrent append and read. **NOT_RUN**: a GETATTR held across a WRITE, which needs a holding fixture inside the daemon; no hook was added |
| FP-17 mutation half | PASS | native_mutation |
| FP-18 refusals stay refused | PASS | native_coherence: one GETXATTR, one SETXATTR and one FALLOCATE over 633 WRITEs |
| FP-28 open-unlinked and rotation | PASS | native_coherence, native_mutation |
| FP-29 removed cwd and `O_PATH` | **PARTIAL** at attempt 3 after a product correction | native_coherence. Executed: exact attributes with link count 0 through lookup custody alone after engine maintenance settled, kernel-forced. Not staged: a processing read spanning the release of its handle |
| FP-20 mutation probe | PASS | native_coherence: six Busy probes against live mutation |
| Unregistered external process | PASS | Nonroot shell scripts in every mutation case; uid 501 through the real Sandbox in the full topology |
| Full topology with mutation | PASS at attempt 2 | SDK example with the actual daemon binary |

Final suites, one run per binary: Linux 69 of 71 exit 0; host 66 of 71 exit 0.

## Every failed attempt

| What | Attempt | Cause | Disposition |
| --- | --- | --- | --- |
| native_custody (step 1) | 1, FAILED 1/2 | Test helper submitted a nonwaiting owner job while a read's releases held both lifecycle slots | Test waits for quiescence; attempt 2 PASS |
| native_coherence | 1, FAILED 4/6 | Two product defects: OPEN of a just-replaced file answered `ENOENT`; a removed directory reported link count 2 | Both corrected in the product |
| native_coherence | 2, FAILED 5/6 | Test read `/proc/<pid>/cwd` of another uid without a tracing capability | Observation moved into the resident process; attempt 3 PASS |
| Full topology | 1, FAILED before any effect | I named the image by tag; the Sandbox requires a digest | Attempt 2 PASS |
| Host suite runner | First invocation ran no test | My runner's lock path was relative | Corrected; one execution followed |
| captured_file_edits (host) | FAILED 13/14 in both host runs | Owner-credit counters read immediately after a completion | **Still failing.** Not rerun, not repaired |
| root_qualification (host) | FAILED 0/1 in both host runs | Same | **Still failing.** Not rerun, not repaired |
| edit_backing (host) | PASS in the first host run, FAILED 3/4 in the final one | Same | **Failing at the final identity.** Not rerun, not repaired |

The three host failures are in test files and owner source R3 does not change
(`credits.rs`, `owner.rs`, `queue.rs`) and all three pass on Linux at the same
source. They read `credited_bytes`/`outstanding` right after a completion,
while the one place that decrements both is the drop of a credit the engine
thread may still hold. R2 recorded the same race in `captured_runs`. They are
reported as failures, not as passes.

Not run for unsupplied explicit preconditions, as the handoff directs:
`complete_installed_roots::huge_native_namespace_is_complete_after_install`
(both sides), `shared_processes` (Linux), `host_handoff` (host).

## Decisions that need owner review

1. **A removed file can be opened (reverses an R2 engine rule).** R2 refused a
   native OPEN of an inode whose last name was gone, and an Overlay test pinned
   that. With mutation live, the kernel's path lookup followed by OPEN can
   straddle an unlink or a replacement rename, and the refusal showed an
   atomically replaced name as missing. A native OPEN now succeeds while the
   protecting kernel reference lives; the Overlay test was rewritten to the new
   rule. The alternative is to answer `ESTALE` so the kernel re-resolves the
   path once, which leaves a visible `ESTALE` under repeated replacement. The
   non-native `open_file` rule is unchanged.
2. **A removed directory reports link count 0.** R2 projected a constant 2 for
   every directory. The count is now 0 once the directory has no namespace
   reference.
3. **Rename ancestry on a mount comes from the engine's retained parent
   index**, not a caller-supplied destination path, because a kernel request
   has none. A directory the kernel knows always has a retained parent row; a
   missing one is `Stale`.
4. **Serial reservation is a synchronous bounded Store write on a worker, and
   a contended allocator writer is `EAGAIN`** with nothing reserved. A refill
   reserves 1024 serials, so this is one Store write per 1024 creations per
   daemon. An engine reservation refusal is `ENOSPC` and does not fence the
   mount.
5. **Refusal choices.** FALLOCATE and COPY_FILE_RANGE answer a sticky `ENOSYS`
   (the kernel then copies through ordinary reads and writes); exchange and
   whiteout renames `EINVAL`; FIFO, socket and device nodes `EPERM`; ownership
   naming the configured identity succeeds as a no-op.

Smaller points, stated so they are not surprises:

- `fchmod`, `futimens` and similar on an open-unlinked file answer `ENOENT`:
  the kernel sends no handle with them and the inode has no name. Workspace
  permits attribute changes on a removed inode only through a handle.
- A LINK or SETATTR reply's modification time can be older than a concurrently
  published mapped store. Size is never affected.
- The entry generation is still the mount's owner identity, as in R2.
- Each mutation is at least four owner jobs: source, publish, ticket release,
  source release. No timing claim is made in either direction.
- `operations/mutation.rs` and `request/mutate.rs` are shared drivers outside
  the handoff's listed target layout. A `store_units` counter was added to the
  opcode accounting.

## Dependence on the R2 decisions still under review

R3 relies on R2 decision 1 only as R2 does: a writable handle or a
mutation-acquired lookup reference that the kernel never releases after detach
is retired by the same bounded maintenance. R3 adds no new kind of owner to
that path. If the decision is reversed so that unreleased handles refuse
revocation, then a mount that was written to and detached without RELEASE
would end `Retained`, exactly as a read-only one would.

A request `Retained` by a failed publishing job fences its mount's admission,
and a Workspace that reaches `Retained` at unmount has no path back to service.
Both recoveries are R6. Only the `Revoke` stage has an executed case.

## Production LOC per commit

Counted by the pinned `tools/production_loc.py` (SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`) on git
archive extractions of each commit's first parent and committed tree:
first-party Rust and shipped SQL, excluding comments, blanks, tests, docs,
tools and third-party code.

| Commit | Combined | Core active | Other subtotals |
| --- | --- | --- | --- |
| `35ff3ba2c` handoff record | 179432 -> 179432 (delta 0) | 71141 -> 71141 | unchanged |
| `90b1aef47` two unrun R2 paths (tests and receipts) | 179432 -> 179432 (delta 0) | 71141 -> 71141 | unchanged |
| `fde14d848` native mutation | 179432 -> 180867 (delta +1435) | 71141 -> 72576 | unchanged |
| this commit: corrections, proofs, documentation | 180867 -> 180871 (delta +4) | 72576 -> 72580 | unchanged |

Unchanged subtotals throughout: core excluded predecessors 38878, core excluded
integration 3996, root reference crates 65417. The growth is new product code;
nothing was relocated or retired.

## Not claimed

No cold, timing, storage or resident-memory claim. No Durable execution:
`NOT_RUN — disabled by owner until explicit reauthorization`. No mounted
Commit, forced unmount, multi-Workspace churn or reference-tree retirement.
