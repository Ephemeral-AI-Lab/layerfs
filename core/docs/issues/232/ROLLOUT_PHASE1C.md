# #232 Phase 1C: complexity screen before the 56-case rollout

> **Status:** Current planning checklist; no release candidate exists.
> Phase 1A's ioctl proof is complete; Phase 1B's retained diagnostic locates
> `service.finish` growth at batch drain without a justified product change.
> No Phase 1C diagnostic or optimization has run.

Tracking: [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).
Run this small screen after [Phase 1B](../241/ROLLOUT_PHASE1.md#phase-1a-exit-and-phase-1b-work)
and before the [Phase 2 56-case rollout](ROLLOUT_PHASE2.md). Its purpose is to
decide whether a remaining complexity term is material on the supported SDK
Exec/FUSE route. Commit this diagnostic contract before implementation or
collection. It does not promise constant time for every file edit.
The [unified implementation plan](UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md)
selects one mounted range ioctl for all 56 registered #232 edits; ordinary
POSIX editors remain a separate product workflow.

## What Phase 1C can decide

Let `N` be file bytes, `M` untouched suffix bytes, `R` edits in one Workspace,
`P` live overlay pieces, `E` canonical extents, `k` replacement bytes and `S`
Store state. Phase 1A removed suffix movement `O(M)` and repeated-write
`O(C^2)` piece rebuilding **for a cooperating program that issues the ioctl**.
The remaining mechanisms have different owners:

| Mechanism | Current source-derived cost | Phase 1C decision |
| --- | --- | --- |
| Repeated Workspace splices | Each splice rebuilds `O(P)` pieces, so `R` edits can total `O(R^2)` if `P` grows with `R`; Commit lowers `O(P)` final pieces. | Count piece visits/pages at 1, 32 and 128 edits. Change the piece index only if this dominates an intended multi-edit workload. |
| Chunked C1 local edit | Replacement and boundary work plus affected extent-tree paths, approximately `O(k + A log E)` for `A` affected extents; whole-route storage cost remains separate. | Count actual CDC input bytes, touched extents and new CAS objects across file sizes. Investigate only an observed file-size-proportional scan. |
| WholeFile and cutoff transition | Reconstructing the canonical whole object can cost `O(N)` below the configured cutoff or at a representation transition. The default cutoff is 128 KiB; supported policy can raise it to 1 MiB. | Prove correctness at cutoff−1, cutoff and cutoff+1. Retain this bounded cost unless an actual budget or memory failure justifies a representation change. |
| Unmodified editor that moves a suffix | The editor itself issues `O(M)` POSIX reads/writes. | Record this as a separate workflow; the ioctl cannot transparently change the editor's algorithm. All 56 #232 cases use an opt-in cooperating tool. |
| C2 `service.finish` | Dependence on `S` is not established by four elapsed times. | Use Phase 1B's cause attribution and decision; do not call it a full-Store scan without counts. |

The v0.1.6 G2 source is an algorithm reference, not a latency arm for this
route: it used a direct specialized SDK edit without shell or FUSE. Its first
equal-length overwrite used a compact descriptor; repeated/structural edits
used persistent treap split/merge with expected local path work, while Commit
still walked final pieces. v0.1.7 already has local C1 extent-tree editing, so
do not port C1 again. v0.1.6's cached-inode length change could itself
reconcile the suffix through EOF; do not transplant that behavior. The
[source comparison](edit-complexity-v016-v017.md) records the exact paths.

## Minimal count diagnostics

Freeze a distinct diagnostic manifest, source/image/tool hashes, deterministic
offsets, cache label, output paths and expected counters **before each first
attempt**. Do not repeat a frozen #241 performance arm for a better wall.

1. **Repeated edits:** one fixed Chunked fixture and one public
   `WorkspaceApi::exec` command containing 1, 32 or 128 checked ioctl edits,
   followed by one public Commit. Hold **total accepted replacement bytes**
   fixed at 4 KiB and spread edits by a frozen offset rule. Count `P` after
   each edit, cumulative piece visits, metadata pages written, Commit lowering
   visits, ioctl callbacks and accepted bytes. Require both superlinear visit
   growth **and a material LFT1 wall share** before selecting a replacement.
   If justified, compare the smallest localized update compatible with the
   existing disk-backed metadata index against a length-indexed tree; a
   prospective target is `O(k + log(P+1))` update work per edit. Commit may
   still traverse `O(P)` final pieces. Do not port an in-memory treap or add a
   tree merely because the current loop is linear.
2. **Chunked locality:** use one bounded 4 KiB range edit at declared offsets
   in independently copied 1, 100 and capped-500 MiB pristine fixtures.
   Record CDC bytes, affected extents/tree pages and newly stored objects.
   Require zero suffix-proportional FUSE I/O and explain any count that grows
   with untouched file length before changing C1, CDC, CAS or delta encoding.
3. **Cutoff edges:** exercise below/at/above the frozen construction cutoff
   through the same public SDK route; verify exact bytes, root, mode/mtime,
   old Commit and reopened Branch. Record constructed bytes and peak sampled
   RSS without claiming `O(1)` for WholeFile.

These are **functional and count-driven diagnostics**, not replacements for
the #232 performance rows. Make one attempt per declared count and identity;
keep the complete command within 15 s and retain every failure. Use locked
release builds, one construction worker, fresh independent writable master
copies and append-only outputs. Product Project/Branch setup, Sandbox Create,
Workspace Mount, Exec, Commit, Status, Unmount and Sandbox Delete use public
`layerfs-sdk`; the tool issues mounted-file ioctls for mutations and may use
POSIX calls only for observation. This is a
separately registered Exec/FUSE workflow, not a direct SDK range-edit claim. No
benchmark-only private LayerFS method or product visibility change is allowed.
Any operation wall/CPU/RSS reported for diagnosis comes only from
`layerfs-telemetry` LFT1, with its producer and sample-window limits. Keep
cache-ineligible timing labelled `INELIGIBLE`; do not use it as a speed PASS.

## Conditional change map and exit

Reuse `core/benchmark/fs-bench-pro/workload/src/splice.rs`, the existing SDK
driver under `core/crates/layerfs-api/sdk/examples/`, and focused tests under
`core/benchmark/fs-bench-pro/tests/`. If counts justify a piece-index change,
its product owners are `core/crates/layerfs-workspace/src/overlay/pieces.rs`,
`core/crates/layerfs-workspace/src/backing/metadata_index.rs`,
`core/crates/layerfs-workspace/src/filesystem/write.rs` and
`core/crates/layerfs-workspace/src/commit/lower.rs`.
If C1 locality fails, trace `core/crates/layerfs-content/src/file/edit/` before
editing it. Keep the Linux ioctl ABI in `layerfs-fuse/src/range_ioctl.rs`
and platform-neutral edit semantics in Workspace. Update the affected
architecture document in the same commit as any algorithm change; do not
patch or fork third-party code.

Phase 1C exits with a count-backed disposition for each row above: **fixed
and verified**, **bounded and accepted**, or **open with its measured reason**.
A justified product fix needs focused correctness checks and a newly frozen
public SDK route proof. No unchanged performance arm is repeated. The
[Phase 2 rollout](ROLLOUT_PHASE2.md) then qualifies all 56 registered cases;
Phase 1C counts and correctness do not substitute for its eligible latency,
independent verifier or cleanup gates.

## Commit checkpoints for Phase 1C

Make each checkpoint a separate commit on an isolated worktree based on the
integrated #241 v4 source and completed Phase 1B decision. Record exact
first-parent production LOC before/after/delta in every commit, including a
zero delta for docs/evidence-only commits. A changed algorithm and its
architecture document land in the same commit.

- [ ] **C0 — Freeze the count contract.** Commit the 1/32/128 offsets and
  payloads, fixed total replacement bytes, expected ioctl/SDK calls, C1
  locality and cutoff-edge cases, cache label, output paths and acceptance
  questions before implementing or attempting a diagnostic.
- [ ] **C1 — Add diagnostic tool support only.** Extend the existing mounted
  splice tool and public SDK driver to run the declared multi-edit command;
  add focused route/receipt checks. Do not expose Workspace internals or add
  a direct SDK edit method. Preserve the one-Exec, one-Commit boundary.
- [ ] **C2 — Retain one count result per declared case.** Collect piece
  visits/pages, CDC bytes, touched extent paths, object counts, LFT1 wall and
  resource coverage, verifier and cleanup. Classify each suspected scaling
  term with source and count evidence; retain `FAIL`, `INELIGIBLE` and
  `NOT_RUN` outcomes. Do not substitute these diagnostics for #232 rows.
- [ ] **C3 — Conditional product algorithm commit.** If C2 shows a material
  avoidable cost, change only its owning Workspace or C1 path. Preserve
  stamped ioctl authorization, canonical roots, old Commit readback and
  one-worker construction. Add a focused regression check and update the
  matching architecture document. A bounded, accepted term needs no code.
- [ ] **C4 — New-source proof if C3 changed product.** Freeze a new identity,
  run the affected public SDK Exec→ioctl→Commit proof once, verify
  independently and confirm unmount/Sandbox Delete. Report count reduction,
  raw LFT1, cache eligibility and any target miss. If C3 made no product
  change, do not repeat unchanged performance arms.
