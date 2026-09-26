# #245 Scalable range-based COW Workspace architecture

> **Status:** Current planning checklist; no release candidate exists.

This is the original #245 design baseline. Its “CURRENT” diagram and 1,024 /
256 / 8 MiB limits describe source **before** packages A–F; they are not the
state at `74ac30e28`. For the implemented private tree, remaining 4,096-run
limit, strict E/F evidence and post-validation algorithm targets, see the
[source-pinned architecture and complexity research](ARCHITECTURE_COMPLEXITY_RESEARCH.md).
The later [resource-constraint lift plan](RESOURCE_CONSTRAINT_LIFT_PLAN.md)
records the current target for Exec lifetime, live-state admission, private
page references and multiple Workspaces per sandbox. This original baseline
does not supersede that plan.

This design keeps one writable LayerFS FUSE mount and the existing `Base` / `Local` / `Zero` range overlay. The public entrypoint remains [`WorkspaceApi::exec(command)`](../../../crates/layerfs-api/sdk/src/workspace.rs), which launches `/bin/sh -c` in the mounted Workspace. It interprets POSIX/FUSE operations, never command text. No edit tool, ioctl, classifier, kernel OverlayFS mount, or whole-file copy-up is part of this route. The immediate evidence is the [ordinary-shell Phase 2 report](../243/evidence/phase2-ordinary-shell-v1/REPORT.md): root lockfile replacement failed with `EIO`; the 4 KiB overwrite and 16 one-byte writes passed functionally, but their latency was cache-ineligible. The rename failure needs its own root-cause repair; changing a piece index cannot fix it.

## Before and after

```text
CURRENT
  any shell command → POSIX → FUSE → Workspace WRITE/SETATTR
                                    │
                                    ├─ load entire file's piece Vec (≤1,024)
                                    ├─ splice and resummarize every piece
                                    ├─ rebuild all fixed two-level index pages
                                    └─ publish active private root

  Commit → pin current root → collect all pieces and ≤256 edits
         → EditFile with ≤8 MiB replacement for an existing file
         → bounded namespace request → Store → Branch head

PROPOSED
  any shell command → POSIX → FUSE → Workspace WRITE/SETATTR
                                    │
                                    ├─ seek byte offset in length-indexed tree
                                    ├─ split/replace affected range
                                    ├─ copy affected page paths; share others
                                    └─ publish active private root

  Commit → pin frozen G1 root ── cursor/stream changed files + names ──┐
           active G2 receives later POSIX calls                       │
           bounded Bridge → server/content builder → candidate root ─┤
           one Branch-head publication ◀───────────────────────────────┘
```

`G1` and `G2` name successive private metadata generations of the **same mount**. They are not directories or copied file trees. Index pages and `Local` payloads live in the Workspace's local `private-backing/<workspace-id>/`; the shell sees the separate FUSE mount. Capturing a root reference does not copy the tree or file bytes. The current [backing layout](../../../crates/layerfs-workspace/src/runtime/host.rs) and [`capture_submission`](../../../crates/layerfs-workspace/src/overlay/snapshot.rs) provide these starting points; later writes and Commit reconciliation must not alter the captured view.

```text
                         /workspace (one FUSE mount)
                         ├─ lookup/list: private bindings and tombstones
                         │               merged with pinned Store base
                         ├─ read: active Local/Zero/Base piece sequence
                         │         Local → private backing payload
                         │         Base  → immutable Store content
                         └─ write: active generation's range tree
                                      │
                       Commit capture ├─ frozen G1 → bounded Store build
                                      └─ live G2   → later shell calls
```

## Current limits and the real bottlenecks

The [`Piece`](../../../crates/layerfs-workspace/src/overlay/pieces.rs) model is already range-based COW: `Base` references unchanged canonical bytes, `Local` references private payload, and `Zero` represents a hole. A piece is a variable-length extent, not a fixed-size chunk. The current [index reader and builder](../../../crates/layerfs-workspace/src/backing/metadata_index.rs) collect every piece into a vector and rebuild all piece pages for a write. [`splice`](../../../crates/layerfs-workspace/src/overlay/pieces.rs) revisits the result, caps it at **1,024 pieces**, and enforces at most **256 changed intervals** and **8 MiB final non-base bytes for an existing file**. The [Bridge request](../../../crates/layerfs-bridge/src/contract/request.rs) independently enforces 256 edits and 8 MiB input. `MAX_FILE` is **4 GiB**. These are separate from command or FUSE-write counts; they constrain the resulting private file representation before Commit and the current Commit request.

The namespace path separately caps a captured frontier at **128 dirty identities and 128 changed names** with a **32 KiB metadata request**; see [`frontier_bytes`](../../../crates/layerfs-workspace/src/runtime/state.rs) and the [Bridge contract](../../../crates/layerfs-bridge/src/contract/request.rs). Its larger-load redesign is required before claiming package-install scale. The first milestone should remove only bounds that the focused cases reach, while choosing formats that can grow without rewriting the architecture.

## Scalable file upper

The proposed shape is a persistent, dynamic-height **B+ tree-style sequence
(rope)**, rather than a binary tree. Each 4 KiB leaf packs many `Base` /
`Local` / `Zero` extents; each internal page holds many child page references
and their **subtree logical byte lengths**. Seeking offset `x` subtracts
preceding child lengths on the way to a leaf. Split or concatenate only the
affected leaves and ancestor paths; rebalance locally and merge adjacent
compatible extents. Multiway pages keep the height and backing-page I/O small
when thousands of pieces exist. Keep height, fanout, arithmetic, and maximum
file length checked; the exact page format and fanout are implementation
decisions to validate, not a current product contract.

This changes the persisted piece-page format: an extent's logical start is derived by its position in the sequence, while its source offset and payload identity remain explicit. Define versioned reading or an explicit migration for retained private roots; never silently interpret an old absolute-key page as a length-indexed page.

Do **not** key every page by absolute file offset. A prepend or insertion changes the logical start of every suffix extent; absolute-offset keys would require rewriting those keys and defeat local COW. Subtree lengths move the suffix logically without touching its pages. With `P` pieces and `K` affected pieces, a local write should visit roughly `O(log P + K)` pieces/index entries and copy only affected page paths, rather than revisit all `P` pieces on every callback. A large contiguous replacement legitimately visits many extents. A cursor supports ordered reads and a single Commit walk without a whole-file `Vec`. Dynamic height and bounded leaf/branch pages remove the fixed 1,024-piece vector; bounded streaming Commit must separately remove the downstream wire limits.

For each accepted FUSE `WRITE` or size mutation: resolve the current inode/root; validate offset, length, holes, and quota; create bounded private payload for the new bytes; build and seal a candidate page path; then compare the expected revision/root and publish the candidate under the existing mutation ordering. Reads after success see it through the same mount. On refusal, do not publish a partial candidate or claim bytes that were not accepted. `CREATE`, `UNLINK`, and `RENAME` remain namespace operations, including tombstones and temp-file replacement. The [current write publication](../../../crates/layerfs-workspace/src/filesystem/write.rs) is the starting point for these ordering and visibility rules.

The tree must preserve payload **custody** across shared roots: copied index pages add references to unchanged child pages and `Local` payloads; replacement drops references only after no active root, frozen Commit, handle, or uncertain outcome needs them. Reclamation must charge retained payload bytes and pages, make progress across partial failures, and never free a payload by looking only at the latest root. Adapt the current [custody](../../../crates/layerfs-workspace/src/backing/payload.rs) and [metadata reclamation](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs) rules rather than starting an unrelated storage path. No durability guarantee or Workspace-backing `fsync` is introduced.

## Incremental, continuously writable Commit

At the capture boundary, pin one immutable upper root (`G1`) and let subsequent calls update a successor (`G2`) that initially sees **all** of G1's changes while sharing its pages. Stream `G1`'s changed identities and ordered pieces with bounded cursors. For an existing file, reuse canonical `Base` ranges and feed its `Local`/`Zero` replacements to a streaming file builder; for a new file, stream complete content. Untouched file roots are reused. A file that has a huge changed region still sends those bytes; bounded memory is not a claim of sublinear I/O. Canonical chunking may read neighboring bytes and persistent growth must be measured, not asserted to equal the nominal edit length.

```text
same mounted Workspace, with one Commit in flight at a time

G1 active: shell/POSIX writes → local COW pages and payloads
Commit A:  order the capture boundary, freeze G1 ──→ build/publish head B1
G2 active: later shell/POSIX writes → new COW root sharing G1's pages
           reads see G1 plus G2 changes while B1 is pending
           after B1 succeeds, reconcile G2 onto B1 without losing later writes
Commit B:  freeze G2 ────────────────────────────→ build/publish head B2
G3 active: later writes may continue while B2 is being built
```

This is local and incremental in two senses: each accepted mutation publishes
only new affected index paths and payload bytes, and each Commit enumerates its
frozen changed identities and ranges rather than copying the entire Workspace.
It does not promise that canonical storage grows by exactly the nominal edit
length. A failed or unknown Commit keeps its frozen generation and dependent
successor until the outcome is resolved; Commit B may not assume B1 succeeded.

The change must cross **all** current request boundaries. [`lower_file`](../../../crates/layerfs-workspace/src/commit/lower.rs) presently collects the full piece vector and edit vector. [`save`](../../../crates/layerfs-workspace/src/commit/save.rs) sends one `EditFile` or `ConstructFile`; the [Bridge contract](../../../crates/layerfs-bridge/src/contract/request.rs) and [server save path](../../../crates/layerfs-server/src/service/save/content.rs) admit bounded edits and materialize replacement buffers. A streamed cursor/framed request must validate ordering, lengths, base identity, acknowledgements, and total resource charges across Workspace, Bridge, server, and the [content builder](../../../crates/layerfs-content/src/file/edit/mod.rs). Raising only one limit moves the failure to the next layer.

Likewise, a larger package frontier needs paged enumeration and bounded
namespace batches through Workspace lowering, Bridge, server, and content
construction. The current [content namespace walk](../../../crates/layerfs-content/src/filesystem/limits.rs)
has its own 4,096-entry validation ceiling to review for that later scale.
Candidate objects may be built in stages, but only **one** final Branch-head
update publishes each Commit. A failed or unknown publication retains its
frozen root and evidence until reconciliation resolves it. `G2` must retain
all changes made after capture and be rebased on B1 without silently losing,
duplicating, or reordering them. Current [Commit reconciliation](../../../crates/layerfs-workspace/src/commit/reconcile.rs)
provides a starting mechanism, but it can return `Busy` if an active write
advances the revision/root before successor installation. Successful
reconciliation under writes that overlap a long Commit, followed by a second
sequential Commit, is **unproven**. “Non-pausing” means Store construction does
not hold the mutation lock; capture/reconcile still need short ordering
points, and individual calls may wait for their own I/O. This is a target
contract, not a current PASS.

## Bounds, speed target, and limits of the claim

Keep explicit bounds on logical file length, actual private backing bytes, retained generations, indexed metadata pages, memory windows, transport frames, and construction workers. Charge work before admitting a mutation or stream stage; report `Capacity` or another precise failure when a real budget is exhausted. Avoid a fixed per-file piece or changed-interval count as the product contract. A workload with more changes may still hit its declared disk, memory, time, or file-size limit.

The first speed target is **structural**: a fixed 4 KiB overwrite in increasingly fragmented files should touch affected pieces and tree paths, not reload/rebuild all `P` pieces; Commit should walk a frozen file once with bounded memory. Measure callback counts/bytes, piece and page visits, physical backing growth, Commit substeps, and complete-command wall on the ordinary shell route under an eligible equal cache contract before stating a multiplier. The [1 MiB versus 10 MiB POSIX shift diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md) counted 51 versus 3,363 old-piece loads, but its cold latency was ineligible.

An arbitrary shell insert or prepend may rewrite its entire suffix through POSIX writes. This architecture removes repeated whole-index work **per callback**; it cannot infer a one-call splice or eliminate bytes the command actually submits. Large package installs and big files also require the separate namespace and streaming-Commit work above. The focused #245 verification should prove the earlier rename failure, small-write behavior, cleanup/progress symptom, and qualified performance before any greater-load claim.
