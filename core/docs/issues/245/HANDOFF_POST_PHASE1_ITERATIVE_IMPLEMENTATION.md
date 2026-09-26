# #245 iterative implementation handoff prompt

> **Status:** Current planning checklist; no release candidate exists.
>
> Copy the assignment below into the implementation agent's new task. The
> [implementation spec](POST_PHASE1_IMPLEMENTATION_SPEC.md) and
> [mini benchmark contract](SHELL_BRAINSTORM_MINI_V1.md) are committed in the
> draft research [PR #257](https://github.com/Ephemeral-AI-Lab/layerfs/pull/257).
> Its `f74dbe77da12fa533587be8a578375bce3f19373` product source pin is
> historical: inspect the current source before editing.

**Lane state, 2026-09-26.** The implementation lane is
`codex/issue245-post-phase1-implementation`, based on `f74dbe77d`. Phase 1 is
done (`b298d175d`, `7e6cac877`). Phase 2's code is implemented
(`5a9a9fc5f`, `0c3e455bd`, `95f8ada8e`, `c3fd065c6`, `74ec83e30`, `11f257ed1`,
`e8a97c8c6`), and **its closure is still open on the two items in
[Phase 2 remaining](#phase-2-remaining--close-the-open-items)**: a deterministic
overlap check for the writer-gate property, and one scope decision about the
directory-lowering gate holds. The 4,097-separated-run public row is not one of
them: it belongs to the mini-benchmark lane. Phase 3 is open, and its first
measured refusal is pinned at `c30fe68a0`.

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

**Start here.** Phase 1 is done and phase 2's code is implemented on the lane
branch named above. The current assignment is **the remainder of phase 2**:
items 2.R1-2.R2 under
[Phase 2 remaining](#phase-2-remaining--close-the-open-items). They are evidence
and scope items, not a rewrite: do not change phase 2's accepted behaviour, do
not re-run its passing suites for reassurance, and do not re-open a defect it
already fixed. Phase 3's slices under
[Phase 3 remaining](#phase-3-remaining--ordered-slices) come after, and its first
refusal is already pinned by
`core/crates/layerfs-workspace/tests/wide_namespace.rs`. Nothing in this
revision licenses a benchmark row in place of either phase's focused tests.

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
| 3 | #256 keyed namespace, live pins, prepared stream and C1 ordering. | Path-local binding/tombstone mutation, no 128-name or 32 KiB admission, ordered cursor traversal that visits each reached leaf once, and one published head. Focused tests: create/rename/delete/recreate across a wide directory, a many-identity generation, and an ordered whole-tree walk. **In progress** (`c30fe68a0`: `tests/wide_namespace.rs` pins the first refusal and no product source changes). **Remaining: slices 3.0-3.4 below.** |
| 4 | #258 stable canonical origin for inherited directory moves. | A base-resident directory moves without a subtree copy-up, descendants resolve through a stable origin, invalid destinations are refused before publication, and old paths disappear. Focused tests: inherited move, move-back, held handles, deep/invalid paths. |
| 5 | #249 multi-Workspace daemon registry and lightweight concurrent Exec; #219 operator Workspace-count policy. | Per-Workspace Commit slot only, no whole-Exec timer, no fixed Exec count, lease cleanup, and a selected Workspace count of 1/2/3 enforced by the daemon registry. Focused tests: registry admission and refusal, two Workspaces, overlapping Exec, count policy. |
| 6 | #245 integration: combined file-plus-namespace generation, two sequential Commits against the immediate predecessor, G2 writes retained during G1 construction, exact old/new heads. | Focused tests over the public API; no benchmark artifact required. |

### Phase 2 remaining — close the open items

Phase 2's code properties are implemented and covered; two items decide whether
the phase may be called closed. Neither authorizes a workload change, a timeout
increase or a debug binary.

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

**Where the `binary_plus_tree/` shape lives.** Phase 3 owns it, as slice 3.0
below: the two tree algorithms get their proposed component there, by relocation
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

The pin is measured, not guessed. `tests/wide_namespace.rs` creates 200 names in
one directory through the public API and, at `c30fe68a0`, reports
`create 127: Capacity` with `dirty_inodes: 128`, `revision: 127` and 1.9 MiB
accounted. That refusal is `State::frontier_bytes`, not a directory page, so the
slices below start where the refusal actually is. The case is `#[ignore]`d with
the admission as its reason; it must pass unchanged once 3.1 and 3.2 land.

| Slice | Where | What the code must do | Focused tests that prove it |
| ---: | --- | --- | --- |
| 3.0 | `backing/` only | Realize the study's tree component by **relocating** the two existing tree algorithms, with no behaviour change: `binary_plus_tree/mod.rs` (thin), `extent/{mod,format,splice,cursor}.rs` from `metadata_pieces.rs`, `metadata_cursor.rs` and the piece half of `metadata_pages.rs`, and `keyed/{mod,format,update,build}.rs` from `metadata_index.rs`, `metadata_build.rs` and the cell half of `metadata_pages.rs`. Keep the shared 4 KiB header, `PageRef` and `PageKind` in one place the three specializations import, keep the old module paths as reexports so nothing else moves, and keep every `mod.rs` under 200 physical lines. | The whole existing suite, unchanged and unedited: if any case needs editing, the move changed behaviour and is not a relocation. |
| 3.1 | `binary_plus_tree/keyed/` (the 3.0 destination of `metadata_index.rs`), `overlay/directories.rs` | Add a path-local keyed delete (leaf removal, sibling borrow or merge on underflow, root collapse) and a persistent ordered key cursor that keeps its path across leaves; rewrite `keep_name`/`drop_entry`/`remove_name` as point mutations instead of rebuilding a whole page from a resident `Vec` of at most 128 names; drop `Directory::parse`'s `count > 128` and resident-128 caps while keeping the `u16` count and charging the local delta. | `tests/wide_namespace.rs` (200 names, rename/unlink/recreate) plus counted cases: one name walk visits each reached leaf once, and one rename or unlink rewrites only its path. |
| 3.2 | `runtime/state.rs` (`frontier_bytes`), `overlay/snapshot.rs`, `commit/reconcile.rs` | Compute the generation frontier charge from the actual dirty/name/row counts and reserve it from the host budget, so a refusal is a real budget refusal rather than `dirty > 128 \|\| names > 128` or a computed prepared size over `METADATA_BYTES` (32 KiB). Keep the prepared-namespace accounting exact so 3.3 can declare it. | A many-identity generation (at least 1,025 created files and names) admitted while the declared budget has room; a declared-budget exhaustion that refuses precisely and publishes nothing. |
| 3.3 | `commit/directories.rs`, `commit/lower.rs` | Lower the frozen namespace with the 3.1 cursors: one ordered pass per record kind, exact declared totals, no `vector(128 + …)`, no `rows.len() == 128`, no `changes.len() == 128`, and still exactly one filesystem root and one head per generation. | Exact rows for a wide directory; old and new listing oracles; one published head. |
| 3.4 | `bridge/contract/request.rs`, the server's prepared-changes path, C1's filesystem input | Carry the prepared namespace as an ordered, replayable, quota-charged stream with exact declared totals - the `SaveFile`/`FileInput` spool seam is the model to reuse, not a new file builder - validate it once into a bounded spool, and feed C1's ordered builder from that spool without a resident vector proportional to rows. Keep `expected_head`, one publication and canonical identity for previously accepted inputs. | 129/257/1,025 changed-name or changed-file generations through the public API with a full-tree oracle; a resident cost that does not grow with the row count; one canonical filesystem root. |

Phase 3 closes only with all five slices; 3.0 is a prerequisite of 3.1, not an
optional extra. If 3.4 cannot be completed, deliver 3.0-3.3 with the prepared
namespace still resident and report that plainly as the remaining half; do not
call the phase closed. Check `layerfs-content`'s
filesystem validation limits (the 4,096-binding walk) before claiming a wide
rebind passes, since a lifted workspace cap can simply move the refusal
downstream.

Unverified candidates, from source reading only - do **not** treat them as proven
refusals and do not cite them as such: `RootOwner::write_page`/`write_raw_page`
refuse at 128 temporary pages per publication, `Arena::reserve_ledger_identity`
refuses a candidate whose allowance is 64 pages, and the arena's ledger identity
table is clamped at 1,058 entries. No case in this lane covers a publication that
writes more than 128 pages or a candidate with a 64-page allowance.

### Environment recipe for the Linux checks

The private backing requires the Linux direct-I/O profile: an ext4-compatible
filesystem (its magic and 4096-byte blocks) with `TMPDIR` inside that filesystem.
`overlayfs`, or a `TMPDIR` outside the volume, fails with `Unsupported` or
`Denied` for reasons unrelated to the code under test; a source tree extracted
with macOS ownership makes `WorkspaceHost::attach` refuse with `Denied`.

The recipe used for phases 1 and 2: a `rust:1.85.1-bookworm` container holding the
worktree at `/src` on an ext2/3 volume, source files owned by root, and
`TMPDIR=/src/tmp`. Mounted FUSE, prepared Store masters and the native Python
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
