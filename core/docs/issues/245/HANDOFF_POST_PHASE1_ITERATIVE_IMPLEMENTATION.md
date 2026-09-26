# #245 iterative implementation handoff prompt

> **Status:** Current planning checklist; no release candidate exists.
>
> Copy the assignment below into the implementation agent's new task. The
> [implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md) and
> [mini benchmark contract](SHELL_BRAINSTORM_MINI_V1.md) are committed in the
> draft research [PR #257](https://github.com/Ephemeral-AI-Lab/layerfs/pull/257).
> Its `f74dbe77da12fa533587be8a578375bce3f19373` product source pin is
> historical: inspect the current source before editing.
>
> **Revision, 2026-09-26 (post-3.2).** Phase 3's slices 3.0-3.2 are landed and
> verified on the lane branch; the next assignment is **slices 3.3 and 3.4
> together**, because 3.3's last constant cannot be lifted safely without 3.4's
> stream. See the lane state below and
> [3.3 and 3.4 land together](#33-and-34-land-together).
>
> **Revision, 2026-09-26 (post-3.3).** Slice **3.3 is landed** (`e5c14c72c`),
> together with the **admission half of 3.4**: the prepared update is admitted by
> the exact bytes it encodes to rather than by 128-row counts, and the whole
> `layerfs-bridge` package carries a 200-name update with no count refusal. Phase
> 2's two open items are closed as well (`e5c14c72c` decides 2.R2 and narrows the
> holds, `b4a306bf2` holds a lowering page read and admits a mounted mutation
> there). The next assignment is **the rest of 3.4**: carry the prepared
> namespace as an ordered, replayable stream, validate it once into a bounded
> service-side spool and feed C1's ordered builder from it. Everything else in
> this checklist is unchanged; the measured limits and the new neighbouring
> bound are in [what the lane head proves](#what-the-lane-head-already-proves-so-it-is-not-re-proved).

**Lane state, 2026-09-26 (updated again).** The implementation lane is
`codex/issue245-post-phase1-implementation`, based on `f74dbe77d`; its head is
`b4a306bf2`. Phase 1 is done (`b298d175d`, `7e6cac877`), phase 2's code is done
(`007d53552`) and its two open items are now closed on this lane: 2.R1's second
arm (`b4a306bf2`) and the 2.R2 scope decision with the narrowing it implies
(`e5c14c72c`), both recorded on #245. Phase 3 is **partly done**:

| Slice | Commit | State |
| ---: | --- | --- |
| 3.0 | `bd907b5ba` | Done. Both private trees relocated into `backing/binary_plus_tree/{extent,keyed}/`, every old module path kept as a reexport, the whole existing suite run **unedited** and identical to the parent (52 passed / 0 failed / 137 ignored). Migration only: +37 production lines of module docs, reexport blocks and rewritten `use` lists. |
| 3.1 | `8460f5afc` | Done. `keyed/delete.rs` adds a path-local keyed delete (leaf removal, one-neighbour merge or split on underflow, root collapse by lifting); `keyed/cursor.rs` adds a persistent ordered key cursor; `keep_name`/`drop_entry`/`remove_name` are point mutations; `Directory::parse` and rename no longer refuse more than 128 names. |
| 3.2 | `8460f5afc`, `0d974fa82` | Done. `State::frontier_bytes` charges the exact prepared-namespace bytes against the host's declared memory budget instead of refusing `dirty > 128 \|\| names > 128` or a total over `METADATA_BYTES` (32 KiB); the live-node table grows by charged chunks instead of refusing at `NODE_LIMIT` (256). |
| 3.2 pin | `861784ab7` | The `#[ignore]`d 1,025-identity case is measured, not predicted: `dirty_inodes=1026` (one per file plus the parent directory), `listed=1022` and `Capacity` at the cookie table, wall 44.73 s. |
| 3.3 | `e5c14c72c` | Done. The frozen namespace is lowered in one ordered pass per record kind: `commit/lower.rs` walks the captured frontier, the namespace records and the inode records with three cursors advanced in serial order (and reads the result records with a fourth in `prepared_inodes`); `commit/directories.rs` merges a directory's entry and removal leaves in name order, with the 128 row constants gone; `commit/save.rs` admits the update by the exact encoded size. The walk holds no metadata writer gate - that is 2.R2 decided. |
| 3.4 | `e5c14c72c` (admission half) | **Half landed.** The contract admits the prepared update by `PreparedChanges::frame_bytes` against the 32 KiB metadata frame instead of 128-row counts, and every decoded count is bounded by the bytes the frame still holds. The **stream is open**: the rows are still resident through Bridge, the server and C1, charged before Commit by `State::frontier_bytes` but not spooled, so a generation is bounded by one frame (~338 five-byte names) rather than by the configured budget. |
| 2.R1, 2.R2 | `b4a306bf2`, `e5c14c72c` | Done. Two frozen Commit phases are held in the kernel with the writer gate free and a mounted mutation admitted there (`FROZEN`, `NAMESPACE`), with the gate-held control arm; the 2.R2 sentence is read as covering every Commit phase. |

`tests/wide_namespace.rs` now passes unedited - the case that pinned
`create 127: Capacity` at `c30fe68a0` reports `names=165 dirty_inodes=166
revision=345`. Phase 3 is **not** closed: it closes only with all five slices,
and 3.4's stream is the outstanding half.

## Owner revision, 2026-09-26: implementation and focused tests only

This revision supersedes every *measured-gate* requirement below for the
remaining phases. The work is an **implementation task**: the deliverable is
product code that is correct by construction and covered by focused tests.
Full-size public benchmark rows are **not** required to close a phase.

- No `#248` 4,097-separated-write row, no `#256` 129/257/1,025 changed-file
  rows, no `#258` mounted move case, no `#249` overlapping-Exec case and no
  `#219` configured-count case has to be run, sampled, registered or frozen
  before its phase is complete. The
  [mini runner](SHELL_BRAINSTORM_MINI_V1.md) stays unimplemented and unused.
- Evidence for a phase is the focused test selection that covers the changed
  code — external `tests/` cases in `core/crates/<package>/tests/` that drive the
  real public API, plus the release build, Clippy, `fmt` and the product-boundary
  guard. Route fixtures that need a prepared Store, a daemon image or a
  privileged mount are optional, not prerequisites.
- Speed, cache, quota and amplification numbers are not admission criteria for
  these phases. Do not sample wall time to decide a phase, and do not add a
  benchmark harness to satisfy this document.
- The product contract itself is unchanged: one public
  `WorkspaceApi::mount` → one opaque `WorkspaceApi::exec` → `/bin/sh -c` →
  ordinary POSIX/FUSE callbacks → optional explicit `WorkspaceApi::commit`, with
  no command classification and no benchmark-specific product shortcut. It is a
  contract on the code, not a workload that must be measured.
- Keep the per-commit production LOC comparison, the release-only test builds,
  the file-size limits and the architecture-document-in-the-same-commit rule.

This revision also supersedes the "Required completion evidence" column of
[the implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md), whose per-phase
rows are public benchmark gates, and its closing list of proof gates. Where the
body below or that spec says a phase "requires" a measured row, read it as "the
implementation must make that row possible"; the row itself is optional and, if
ever taken, still needs its own prospectively frozen contract.

## Assignment to paste

Implement the open post-Phase-1 #245 work iteratively. The owning issues are
[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) (file runs and
Commit), [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256)
(namespace count and streaming), [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258)
(base-resident directory move), [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249)
(multiple Workspaces and count-free concurrent Exec), and
[#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
(`max_workspaces_per_sandbox`). [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245)
owns the integrated result. Treat Phase 1 A–F and #252 as completed
prerequisites; preserve their receipts and exact accepted semantics.

Read `AGENTS.md`, `core/AGENTS.md`, `docs/general/benchmark_rules.md`,
`docs/general/documentation-policy.md`, and
`core/benchmark/fs-bench-pro/AGENTS.md`. Then read
`core/docs/issues/245/POST_PHASE1_IMPLEMENTATION_SPEC.md`,
`JOINT_248_256_TREE_RESEARCH.md`, `RESOURCE_CONSTRAINT_LIFT_PLAN.md`,
`FINAL_DELTA_COMMIT_COMPLEXITY.md`, `LOAD_BEARING_CASE_REVIEW.md`, and
`SHELL_BRAINSTORM_MINI_V1.md`. Read the live issue bodies and source, since
the documents are source-pinned plans, not proof that later code is unchanged.
Use an isolated implementation worktree/branch based on the latest compatible
source, keeping the docs research PR reviewable. Preserve other owners' work.

**Start here.** Phases 1 and 2 are done, and phase 3's slices 3.0-3.3 are
landed and verified on the lane branch named above, with the admission half of
3.4. The current assignment is **the rest of slice 3.4**: carry the prepared
namespace as an ordered, replayable, quota-charged stream with exact declared
totals, validate it once into a bounded spool and feed C1's ordered builder from
that spool without a resident vector proportional to rows, under
[Phase 3 remaining](#phase-3-remaining--ordered-slices). Keep 3.0-3.3's accepted
behaviour, their exact rows and their focused cases passing, do not change what
they prove, and do not re-run their suites for reassurance. Phases 4-6 come
after. Nothing in this revision licenses a benchmark row in place of a phase's
focused tests.

Two warnings from the half that just landed, both measured rather than argued.
The bound 3.4 has to lift is now the **metadata frame**, not a row count: a
200-name update fits one frame and is admitted, a 95-name update of 255-byte
names does not and is refused as `Capacity` before any command is sent, so the
work is to move the rows out of the frame rather than to raise a constant. And
the deep half is **C1's input**, not the wire: `FilesystemInput` takes
`&[DirectoryUpdate]`, `&[InodeUpdate]` and `&[u64]` and looks serials up by
binary search (`update_for`, `value_for`, `new_inodes.contains`), so a
spool-backed source needs those lookups redesigned - which is why the resident
half must be charged, and reported as remaining, until it is.

### Product contract

The only measured mutation surface is public `WorkspaceApi::mount` → one
opaque `WorkspaceApi::exec(command)` → `/bin/sh -c` → ordinary POSIX/FUSE
callbacks → explicit `WorkspaceApi::commit` for mutating cases. The same API
must accept arbitrary shell commands. Do not classify command text, detect
benchmark IDs in product code, call private Workspace/Bridge/C1 mutation
methods in a benchmark driver, introduce a special editor or ioctl route, or
make a benchmark-specific product shortcut. Read-only `find`, `grep`, and
printed-output cases have no Commit.

Implement the smallest coherent change that passes each phase's gate. Reuse
the existing Arena, keyed pages, C1 sorted builder and charged `FileBacking`
where they suffice; do not create a generic B+ engine, packed-page format or
new C1 file builder merely because the research names a possible module.
Preserve atomic private-root publication, G1/G2 custody, exact canonical
identity for previously accepted inputs, one expected-head publication,
unknown-outcome retention and incremental Commit against the immediately
preceding successful head. One Commit/Stage submission at a time **per
Workspace** is the only logical serialization rule; different Workspaces may
progress independently subject to actual Store/resource admission.

### Iteration loop — repeat after every completed phase

1. Name the issue, source commit, exact behavior and the focused test that
   will cover it. Trace all callers and the shared root cause before editing.
   If a benchmark scenario is ever registered later, freeze it before its first
   sample; old receipts remain append-only.
2. Implement one reviewable slice. Update the affected `core/docs/architecture/`
   document **in the same commit** if a boundary, format, algorithm or named
   bound changes. Keep production files under 1,000 physical lines and
   `lib.rs`/`mod.rs` under 200; no third-party patch or new dependency for an
   existing capability.
3. Build **locked release binaries only** (`cargo +1.85.1 ... --release
   --manifest-path core/Cargo.toml --locked`) in this worktree's target,
   including every Rust test executable used as evidence. Never run, compare or
   promote a debug binary. Record the source commit, the test binary and the
   profile; reuse a matching sealed release build instead of rebuilding it.
4. Run the smallest covering checks: the focused locked Core tests for the
   changed code, plus release Clippy, `fmt` and the product-boundary guard and
   its self-tests. A test command must finish in **at most 30 seconds**;
   prebuild the release test binaries separately if compilation obscures test
   wall. Do not raise a timeout, worker count, quota or cache allowance to turn
   a miss into a PASS. Report any broader check that did not fit rather than
   substituting a narrower one silently.
5. **Do not rerun a successful test after a minor unrelated fix.** Keep a
   phase-local list of passed checks and the source/behavior they cover. After
   a fix, rerun the failing check and only passed checks whose covered code or
   assumptions changed; run an exact-identity final check only where a frozen
   admission contract requires it. Do not resample an unchanged benchmark arm
   or rerun a passing suite for reassurance. Record the identity and limits
   of any retained proof rather than calling an older result a new PASS.
6. When the phase gate passes, compute exact first-parent **production LOC
   before → after (signed delta)**, with Core and legacy subtotals, using the
   same `core/tools/production_loc.py` method on parent and staged tree.
   Commit that completed slice, push it, and update its owning GitHub issue
   plus #245 with the commit link, the route or API the change is on, the exact
   checks and their status, the limits and the next phase. A phase closes on
   its focused tests, not on a speed row.
7. Continue directly to the next phase. **Do not ask the owner for
   authorization to continue, commit, update issues or stop.** If an actual
   external blocker prevents a phase, record its evidence in the issue and work
   on any independent slice. If no meaningful work remains possible, provide a
   precise blocked handoff; never claim an unfinished phase done.

### Product implementation phase order

Each phase closes on its implementation plus the focused tests that cover it.
The right-hand column names **what the code must do**, and the tests that prove
it; it is not a benchmark row to register.

| Phase | Implementation focus | Closure: code property and its focused tests |
| ---: | --- | --- |
| 1 | Shared #248/#256 page and payload ownership: charged allocation, indexed payload lookup, work-driven reclamation and exact pinned-root custody. | Routine maintenance work does not grow with earlier acquisitions; quota and failure paths keep every owner. **Done** (`b298d175d`, `7e6cac877`): indexed registry, release-list reclamation, `routine_scans`/`lookup_scans` counts, four external tests in `tests/backing_ownership.rs`. |
| 2 | #248 file extent and frozen Commit path: height transitions, monotone cursors, short writer-gate holds, bounded C1 replay. | The extent tree stays height-uniform at every count and pack shape; a splice's page visits are `O(H + touched)` rather than `O(H per leaf)`; no Commit phase holds the metadata writer gate across a frozen walk or a transfer pull; C1 consumes a replayable stream. **Code implemented** (`5a9a9fc5f`, `0c3e455bd`, `95f8ada8e`, `c3fd065c6`, `74ec83e30`, `11f257ed1`, `e8a97c8c6`): level-preserving rebuild and balanced packing, collapsed-node lifting instead of a refusal, boundary insertions placed in the leaf that carries them (before this, appending to any file of about 125 extents refused), a monotone `O(H + L)` cursor, one walk per Commit phase under a bounded read lease with a `metadata_reads` counter, and a spooled-replay confirmation. Covered by `tests/pieces_sequence.rs` (33 cases, including a fourteen-count boundary sweep), `tests/commit_progress.rs` and `layerfs-server/tests/direct.rs`. **Closure open on 2.R1-2.R2 below** - the code properties above are met, but the writer-gate property has no deterministic check and the scope of "no Commit phase" for the namespace-lowering holds is undecided. The 4,097-separated-run public row is a mini-benchmark-lane item, not a phase-2 closure item, by owner direction. |
| 3 | #256 keyed namespace, live pins, prepared stream and C1 ordering. | Path-local binding/tombstone mutation, no 128-name or 32 KiB admission, ordered cursor traversal that visits each reached leaf once, and one published head. Focused tests: create/rename/delete/recreate across a wide directory, a many-identity generation, and an ordered whole-tree walk. **3.0-3.2 done** (`bd907b5ba`, `8460f5afc`, `0d974fa82`): both private trees relocated into `backing/binary_plus_tree/` with the old paths as reexports and the existing suite unedited; a path-local keyed delete with sibling repair and root collapse plus a persistent ordered key cursor (`KEYED_WALK keys=160 leaves=2 reads=3`, `KEYED_DELETE deleted=160 candidates=8`); `keep_name`/`drop_entry`/`remove_name` are point mutations and no 128-name admission remains in `Directory::parse` or rename; the frontier is charged from the actual counts against the host memory budget and the live-node table grows by charged chunks instead of refusing at 256. One unlink of a 130-name directory reads 40 metadata pages. `tests/wide_namespace.rs` passes unedited (`names=165 dirty_inodes=166 revision=345`). **Remaining: slices 3.3 and 3.4, together.** |
| 4 | #258 stable canonical origin for inherited directory moves. | A base-resident directory moves without a subtree copy-up, descendants resolve through a stable origin, invalid destinations are refused before publication, and old paths disappear. Focused tests: inherited move, move-back, held handles, deep/invalid paths. |
| 5 | #249 multi-Workspace daemon registry and lightweight concurrent Exec; #219 operator Workspace-count policy. | Per-Workspace Commit slot only, no whole-Exec timer, no fixed Exec count, lease cleanup, and a selected Workspace count of 1/2/3 enforced by the daemon registry. Focused tests: registry admission and refusal, two Workspaces, overlapping Exec, count policy. |
| 6 | #245 integration: combined file-plus-namespace generation, two sequential Commits against the immediate predecessor, G2 writes retained during G1 construction, exact old/new heads. | Focused tests over the public API; no benchmark artifact required. |

### Phase 2 remaining — closed on this lane

Both items below were the phase's open questions. **2.R1** is done for the two
frozen walks a Commit owns (`007d53552` for the extent transfer, `b4a306bf2`
for the namespace lowering), and **2.R2** is decided and implemented in
`e5c14c72c`: the sentence covers every Commit phase, so the namespace reads and
the frontier walk take no writer gate. The paragraphs below are kept as the
statement of what was asked; the receipts are on #245.

**2.R1 - a deterministic overlap check for the writer-gate property.** The claim
is "no Commit phase holds the metadata writer gate across a frozen walk or a
transfer pull". Today it holds by construction - `lower_file`,
`FileUpload::descriptor_bytes` and `ReplacementSource::pull` no longer acquire it,
and `tests/commit_progress.rs` shows an ordinary mounted mutation admitted
*between* two transfer frames - but nothing fails if the gate comes back. Build
the check that would fail: hold a page read inside a Commit phase and admit a
mounted mutation while it is held. The mechanism already exists in
`core/crates/layerfs-workspace/tests/commit_staged.rs`: a seccomp `pread64`
user-notification listener (`page_read_listener`, `next_page_read`,
`release_page_read`) that the E/F fixtures use to keep the successor builder
blocked. Reuse that pattern for each frozen phase and report, per phase, that the
read was held, how many were held, and the mutation's outcome. If one phase
cannot be held deterministically, prove the phases separately and say which half
covers which; do not generalize one held read into "the walk holds no gate".

**2.R2 - one scope decision.** `commit/directories.rs` still takes the writer gate
around its namespace reads. Phase 2's sentence says "no Commit phase", while the
namespace walk belongs to #256. Decide and record which reading holds: narrow
those holds here if the sentence covers every Commit phase, or state in the row
that it covers the file phases its title names and let #256 narrow them. Either
answer is acceptable; leaving it unstated is not.

**Decided (`e5c14c72c`): the sentence covers every Commit phase.** The namespace
reads and the frontier walk attend a root that is immutable and pinned for the
submission - the same property the phase-2 extent walks rely on - so they take a
bounded read lease and no writer gate, and each leases a scratch window per step
rather than for the whole walk. What still holds the gate in the Commit path is
what mutates shared arena state: `persist_saved`'s result install and the
reconciliation ordering points. `b4a306bf2` holds that open in the kernel:
`COMMIT_OVERLAP NAMESPACE page=m-page-0000000a-0000000a phase=Some(Preparing)
baseline_charge=4304 charge=69072 scans=166 saves=9 metadata=9
lowering_reads=10 mutation=Ok(())`, against the control arm's gate-held read
with `mutation=Err("Busy")`.

The 4,097-separated-run public row is **not** a phase-2 closure item. By owner
direction it is verified in the mini-benchmark lane
([mini contract](SHELL_BRAINSTORM_MINI_V1.md)), which stays unimplemented during
these phases; the implementation's obligation is only that the shape is
representable, and that is already covered at the extent level by
`tests/pieces_sequence.rs` (4,097 separated runs) and through the mounted route by
`tests/commit_progress.rs` (384 appended writes, a declared smaller count).
Nobody should size a phase-2 test to that row, and nobody should present the
smaller route shape as it.

One thing that is *not* open: a collapsed extent node is repaired by
level-preserving lifting, not by sibling rebalancing. The closure property - a
collapsed child never refuses, and every path keeps one depth - is met and
swept at fourteen boundary counts. Do not replace the lift with rebalancing
unless a measured case requires the tighter shape.

### Phase 3 remaining — ordered slices

These follow phase 2's closure; they are not a substitute for 2.R1-2.R2.

**Where the `binary_plus_tree/` shape lives.** Phase 3 owned it, as slice 3.0,
which has landed (`bd907b5ba`): the two tree algorithms have their proposed
component, by relocation
and before the new keyed behaviour is written, so 3.1 authors
`keyed/delete.rs` and `keyed/cursor.rs` where they belong instead of growing
`metadata_index.rs` again. Phase 2 was the extent-tree phase and shipped its work
in place; phase 3 is the keyed-tree phase, and the keyed half is the one that
needs the room first. The
[joint study](JOINT_248_256_TREE_RESEARCH.md#srp-extraction-plan) and the
[spec](POST_PHASE1_IMPLEMENTATION_SPEC.md) define the destination; this revision
only fixes which phase realizes it.

Three rules keep 3.0 honest. It is a **relocation**: no page format, no algorithm
and no public path changes, every existing test runs unedited, and the move is
reported as migration with net production LOC near zero - never as an
algorithmic improvement. It moves **one responsibility per file**, keeping the
old module paths as reexports until nothing imports them. And it does not become
a reason to touch behaviour: the extent half's ceiling pressure is real
(`metadata_pieces.rs` 964 lines at this head) but the `page_store/` and
`payload/` extractions stay deferred until `ownership.rs` (943) or
`metadata.rs` (897) is the file a change actually needs - that is stage two of
the same relocation, not part of 3.0.

The pin was measured, not guessed. `tests/wide_namespace.rs` creates 200 names in
one directory through the public API and, at `c30fe68a0`, reported
`create 127: Capacity` with `dirty_inodes: 128`, `revision: 127` and 1.9 MiB
accounted. That refusal was `State::frontier_bytes`, not a directory page, so
3.1 and 3.2 started where the refusal actually was - and the same case now
**passes unedited** at `8460f5afc`: `names=165 dirty_inodes=166 revision=345`.
The slices below are what is left.

| Slice | Where | What the code must do | Focused tests that prove it |
| ---: | --- | --- | --- |
| 3.0 **done** (`bd907b5ba`) | `backing/` only | Realize the study's tree component by **relocating** the two existing tree algorithms, with no behaviour change: `binary_plus_tree/mod.rs` (thin), `extent/{mod,format,splice,cursor}.rs` from `metadata_pieces.rs`, `metadata_cursor.rs` and the piece half of `metadata_pages.rs`, and `keyed/{mod,format,update,build}.rs` from `metadata_index.rs`, `metadata_build.rs` and the cell half of `metadata_pages.rs`. Keep the shared 4 KiB header, `PageRef` and `PageKind` in one place the three specializations import, keep the old module paths as reexports so nothing else moves, and keep every `mod.rs` under 200 physical lines. | The whole existing suite, unchanged and unedited: if any case needs editing, the move changed behaviour and is not a relocation. |
| 3.1 **done** (`8460f5afc`) | `binary_plus_tree/keyed/` (the 3.0 destination of `metadata_index.rs`), `overlay/directories.rs` | Add a path-local keyed delete (leaf removal, sibling borrow or merge on underflow, root collapse) and a persistent ordered key cursor that keeps its path across leaves; rewrite `keep_name`/`drop_entry`/`remove_name` as point mutations instead of rebuilding a whole page from a resident `Vec` of at most 128 names; drop `Directory::parse`'s `count > 128` and resident-128 caps while keeping the `u16` count and charging the local delta. | `tests/wide_namespace.rs` (200 names, rename/unlink/recreate) plus counted cases: one name walk visits each reached leaf once, and one rename or unlink rewrites only its path. |
| 3.2 **done** (`8460f5afc`, `0d974fa82`) | `runtime/state.rs` (`frontier_bytes`), `overlay/snapshot.rs`, `commit/reconcile.rs` | Compute the generation frontier charge from the actual dirty/name/row counts and reserve it from the host budget, so a refusal is a real budget refusal rather than `dirty > 128 \|\| names > 128` or a computed prepared size over `METADATA_BYTES` (32 KiB). Keep the prepared-namespace accounting exact so 3.3 can declare it. | A many-identity generation (at least 1,025 created files and names) admitted while the declared budget has room; a declared-budget exhaustion that refuses precisely and publishes nothing. |
| 3.3 **done** (`e5c14c72c`) | `commit/directories.rs`, `commit/lower.rs`, `commit/save.rs`, `bridge/contract/request.rs` | Lowered the frozen namespace with the 3.1 cursors: the frontier, the namespace records and the inode records are three ordered passes advanced in serial order, a directory's final bindings are the ordered merge of its entry and removal leaves, `vector(128 + …)`/`rows.len() == 128`/`changes.len() == 128` are gone, admission is the exact encoded size, and one root and one head per generation are unchanged. | `tests/prepared_namespace.rs` (3 cases): a 200-name Commit's exact rows name by name, the frame boundary at 94/95 long names, and a 200-link pass that reads each reached leaf once (`metadata_reads=21`) - plus `commit_overlap.rs`'s held lowering read. |
| 3.4 **half landed** (`e5c14c72c`), **stream next** | the server's prepared-changes path, C1's filesystem input (the contract half is in) | Carry the prepared namespace as an ordered, replayable, quota-charged stream with exact declared totals - the `SaveFile`/`FileInput` spool seam is the model to reuse, not a new file builder - validate it once into a bounded spool, and feed C1's ordered builder from that spool without a resident vector proportional to rows. Keep `expected_head`, one publication and canonical identity for previously accepted inputs. Landed instead: the contract admits by exact frame bytes and the wire reader is bounded by the remaining frame bytes, so no count refuses a generatable update. | 129/257/1,025 changed-name or changed-file generations through the public API with a full-tree oracle; a resident cost that does not grow with the row count; one canonical filesystem root. The 200-name row already passes through the public API; the resident half is what is missing. |

Phase 3 closes only with all five slices. 3.0-3.3 are in and 3.4 is half in.
**The rest of 3.4 is the next assignment.** The fallback this document named for
3.3 - the prepared namespace still resident **with its size charged rather than
unbounded** - is what landed: the frontier charge is exact (`State::frontier_bytes`),
the admission is exact, and the resident half is reported as open rather than
claimed. Do not call the phase closed until the stream lands. Check `layerfs-content`'s filesystem validation limits
(the 4,096-binding walk) before claiming a wide rebind passes: this lane has
already measured that a lifted workspace cap moves the refusal downstream twice
- the 256-entry live-node table, fixed in `0d974fa82`, and the 1,024-entry
cookie table, which still refuses a complete listing of more than 1,022 names.

<a id="33-and-34-land-together"></a>

#### Why 3.3 and 3.4 landed together, and what of it landed

3.3's last constant was coupled to 3.4's stream. `commit/save.rs` computed the
prepared-namespace size and refused it above
`layerfs_bridge::contract::METADATA_BYTES` (32 KiB) **at Commit** - the same
figure 3.2 removed from the pre-Commit frontier. Deleting that check without an
ordered replayable stream does not lift a bound, it turns a refusal into an
unbounded resident prepared namespace, so the two halves were decided together:
3.3 charges and declares the prepared size exactly, and the admission is the
same exact figure the encoder writes. That is the charged-resident outcome this
document named as the fallback, and it is what `e5c14c72c` landed; the stream
half is still the assignment.

Measured on the lane head, in the order a wide generation meets them:

| Refusal | Where | State |
| --- | --- | --- |
| prepared size over 32 KiB at Commit, as an estimate | `commit/save.rs` (`expected > METADATA_BYTES`) | **closed**: the figure is now `PreparedChanges::frame_bytes`, the exact encoded body, checked in the workspace and again in the contract. The frame is still the bound - `PREPARED_FRAME_FIT names=94 declared=32760` / `PREPARED_FRAME_REFUSAL names=95 declared=33106 error=Capacity` - and lifting it is 3.4's stream |
| `vector(usize::from(directory.count) + 128)`, `rows.len() == 128`, `changes.len() == 128` | `commit/directories.rs` | **closed** by 3.3 |
| 128 directories / 128 inodes / 128 names in one prepared update | `bridge/contract/request.rs`, `adapters/native/protocol/prepared.rs` | **closed**: the contract admits by exact frame bytes, and every decoded count is bounded by the bytes the frame still holds |
| one successor lookup per name in the frozen lowering | `commit/directories.rs`, `commit/lower.rs` (`arena.next` per row) | **closed**: `Arena::key_cursor` passes; `PREPARED_NAME_PASS names=200 inodes=1 metadata_reads=21` for 200 names over one identity |
| the prepared namespace is resident through Bridge, the server and C1 | the server's prepared-changes path, C1's filesystem input | **open. This is the next assignment.** It is charged (`State::frontier_bytes`) rather than unbounded, which is why the frame is the current bound |
| a directory whose removal cells are short holds 128 in one leaf and refuses the next | `keyed::update` (the tombstone page) | measured while landing 3.3, `Io` for the 129th removal of a five-byte name; the keyed tree's documented refusal shape, now stated in `core/docs/architecture/06-limits.md`. Not a 3.3/3.4 refusal |
| a complete listing of more than 1,022 names | `runtime::state::COOKIE_LIMIT` (1,024 entries) | measured (`FRONTIER_LISTING listed=1022 refusal=Some(Capacity)`); same #256 lift list, not 3.3/3.4 |
| 4,096 visited bindings on an alias/cycle walk | `layerfs-content` filesystem validation | not measured in this lane; check before claiming a wide **rebind** passes |

#### The Commit-capable fixture 3.3 built, and how to extend it

`tests/prepared_namespace.rs` is that fixture, and it is the cheapest way to
exercise the preparation from here on. It is an in-process `OperationDelivery`
that answers `GetBranch`, the root's attributes, `ReserveInodes`, `SaveFile`
(draining the frozen sequence and returning a root derived from the length),
`ConstructPortableMetadata`/`UpdatePortableMetadata` echoing the selected fields
and the stated base root, and both `HistoryCommand::Commit` (a
`CommitOutcomeWire::Committed`) and `HistoryCommand::StageChanges` (a
`StageWire` built from the request). It records the prepared update, and after a
Commit it answers the root listing from what it published, page by page with a
continuation, and resolves a published name's attributes - without those two it
cannot list a successor state at all.

Two traps are worth naming. The replies must **echo** the metadata fields and
the base root the caller states: a placeholder value stops the Commit before its
lowering, which is how `commit_overlap.rs`'s fixture failed until it echoed
them. And the fixture must hand out **distinct** serials; a constant
`ReserveInodes` start makes the second creation collide. A delivery that records
the `StageChanges` request is the natural oracle: it sees the exact prepared
rows, which is what "exact rows for a wide directory" means.

`tests/commit_progress.rs` still **cannot** exercise `prepared_directories`: its
delivery refuses the file save, and `prepare_changes` - which calls
`prepared_directories` - runs after the save loop in `commit/save.rs`, so the
namespace lowering never executes there. `tests/support/native_workspace.rs` (the
route fixture behind `commit_staged.rs`) remains optional and unrun.

#### What the lane head already proves, so it is not re-proved

Reuse these instead of rebuilding them: `tests/prepared_namespace.rs` (3 cases,
7.5-11 s) for the preparation, the frame boundary and the name pass;
`tests/wide_namespace.rs` (3 cases, 13.8 s) for the wide directory, the counted
unlink and the spent budget; `tests/keyed_tree.rs` (0.21 s) for the cursor's
`leaves`/`reads` counts and 160 deletions to an empty tree;
`tests/commit_overlap.rs` (3 cases, 0.2 s) for the two held frozen walks;
`tests/backing_ownership.rs`, `tests/pieces_sequence.rs` and
`tests/commit_progress.rs` for phases 1 and 2. The recipe below is the same
container the lane used.

**Verified in this lane** (measured, quotable): the 128-name and 32 KiB frontier
admissions and the 256-entry live-node table are gone (`8460f5afc`,
`0d974fa82`); a 4 MiB budget refuses the 512th identity with `Capacity` and
publishes nothing; a 64 MiB budget admits 1,025 creations; a complete listing of
those stops at 1,022 names on the 1,024-entry cookie table; one unlink of a
130-name directory reads 40 metadata pages; a 160-key tree walks in 3 page reads
and deletes to empty in 8 publications. From `e5c14c72c` and `b4a306bf2`: a
200-name Commit of one generation with 145 later name changes prepares
`names=165 rows=260 inodes=165 frame_bytes=17501 metadata_reads=1464 saves=165
metadata=165`; a 200-link generation over one identity prepares in
`metadata_reads=21`; the frame boundary sits between `names=94 declared=32760`
and `names=95 declared=33106` against the 32,768-byte frame, the wider one
refused as `Capacity` with no command delivered; the parent's 128-row
prepared-update caps are gone and a 200-row update round-trips through the
encoder and decoder; a page read of the lowering is held in the kernel with the
writer gate free (`charge=69072` against a held gate's `659664`) and a mounted
mutation is admitted there.

Still unverified candidates, from source reading only - do **not** treat them as
proven refusals and do not cite them as such: `RootOwner::write_page`/
`write_raw_page` refuse at 128 temporary pages per publication,
`Arena::reserve_ledger_identity` refuses a candidate whose allowance is 64 pages,
and the arena's ledger identity table is clamped at 1,058 entries. No case in
this lane covers a publication that writes more than 128 pages or a candidate
with a 64-page allowance - the keyed-tree case keeps each publication well under
the temporary-page ceiling on purpose.

### Environment recipe for the Linux checks

The private backing requires the Linux direct-I/O profile: an ext4-compatible
filesystem (its magic and 4096-byte blocks) with `TMPDIR` inside that filesystem.
`overlayfs`, or a `TMPDIR` outside the volume, fails with `Unsupported` or
`Denied` for reasons unrelated to the code under test; a source tree extracted
with macOS ownership makes `WorkspaceHost::attach` refuse with `Denied`.

**Two recorded pitfalls of the sync itself.** The worktree is copied in with
`tar | docker exec tar -x`; extract with `-m` (`--touch`) or cargo skips the
files, because tar restores the macOS mtimes and a source file older than the
container's build artifacts is "unchanged". And `cargo` is not on the login
shell's `PATH` in that image: `export PATH=/usr/local/cargo/bin:$PATH` (or call
`/usr/local/cargo/bin/cargo`) in every `docker exec bash -c`. GNU tar inside the
container also prints one "unknown extended header keyword" line per file for
the macOS xattrs, so redirect both tars' stderr.

The recipe used for phases 1, 2 and 3: the `rust:1.85.1-bookworm` container
`layerfs-i245-phase1` holding the synced worktree at `/src` on a docker volume
(`/dev/vda1`, ext2/3), source files owned by root, `CARGO_TARGET_DIR=/src/core/target`
and `TMPDIR=/src/tmp`. Two measured pitfalls: the container is the platform's
own architecture (`uname -m` there is `aarch64`), and running a tree extracted
*outside* that volume with a `TMPDIR` outside it fails the payload acquire with
`Unsupported` even though the code under test is fine - the baseline comparison
for slice 3.0 only worked once `TMPDIR` pointed back inside `/src`. Mounted FUSE, prepared Store masters and the native Python
route fixtures are optional here: the focused cases under `core/crates/*/tests/`
drive `WorkspaceHost` and `Workspace` (and, for the service, `layerfs-server`'s
in-process `Service` with a real Store) through a fake `OperationDelivery`, so a
Commit, a wide namespace or a frozen transfer can be exercised without a daemon
image or a privileged mount.

### Tests, not benchmarks

Verification for these phases is **focused tests in the package's `tests/`
directory** that drive the real public API over real Linux backing, run as
locked release builds. A counter is welcome when it makes a claim checkable —
page visits, records examined, rows emitted, bytes charged — because a count is
reproducible and cheap; it is recorded in the receipt as a count, never as a
speed claim.

- Do **not** build the 13-cell mini runner, register new scenarios, prepare Store
  masters, build daemon images, mount FUSE, or take full-size rows to close a
  phase. Those artifacts remain optional and, if ever taken, need their own
  prospectively frozen contract with the repository's cache and budget rules.
- Do **not** close a phase from a cache-ineligible or unqualified timing row, and
  do not add a benchmark-only path to product code to make one possible.
- Keep the [joint tree study](JOINT_248_256_TREE_RESEARCH.md) and
  [resource plan](RESOURCE_CONSTRAINT_LIFT_PLAN.md) as design input, not as a
  proof obligation; their estimates are not a LOC budget. Reuse the existing
  Arena, keyed pages, extent splice, C1 builder and charged backing.
- Keep the per-commit production LOC comparison, the release-only test builds,
  the 999-line/200-line file limits, and the architecture document update in the
  same commit. Tests, docs and receipts stay outside production LOC.

Send concise progress updates during long work. End each completed turn with
the current product or verification phase, commit, issue update, checks with
exact status, and the next actionable step. Continue autonomously until every
required product phase and verification gate is complete or a real blocker is
documented with no independent work left.
