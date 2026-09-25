# #245 Scalable range-based COW Workspace architecture

> **Status:** Current planning checklist; no release candidate exists.

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

`G1` and `G2` name successive private metadata generations of the **same mount**. They are not directories or copied file trees. The current [`capture_submission`](../../../crates/layerfs-workspace/src/overlay/snapshot.rs) already pins a root and advances a generation; extending its scale must preserve the captured view and prove that later writes and post-Commit reconciliation do not alter it.

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

Represent each file as a persistent, paged **sequence of extents**. A leaf stores bounded `Base` / `Local` / `Zero` descriptors; each internal child records its subtree's **logical byte length** and page reference. Seeking offset `x` subtracts preceding child lengths on the way to a leaf. Split or concatenate only the affected leaves and ancestor paths; rebalance locally and merge adjacent compatible extents. Keep the maximum height, page fanout, arithmetic, and maximum file length checked.

This changes the persisted piece-page format: an extent's logical start is derived by its position in the sequence, while its source offset and payload identity remain explicit. Define versioned reading or an explicit migration for retained private roots; never silently interpret an old absolute-key page as a length-indexed page.

Do **not** key every page by absolute file offset. A prepend or insertion changes the logical start of every suffix extent; absolute-offset keys would require rewriting those keys and defeat local COW. Subtree lengths move the suffix logically without touching its pages. A write crossing `K` extents should visit those extents and a bounded number of tree paths, targeting work proportional to `K + log P` for `P` pieces, rather than `P` for every callback. A large contiguous replacement legitimately visits many extents. A cursor supports ordered reads and a single Commit walk without a whole-file `Vec`.

For each accepted FUSE `WRITE` or size mutation: resolve the current inode/root; validate offset, length, holes, and quota; create bounded private payload for the new bytes; build and seal a candidate page path; then compare the expected revision/root and publish the candidate under the existing mutation ordering. Reads after success see it through the same mount. On refusal, do not publish a partial candidate or claim bytes that were not accepted. `CREATE`, `UNLINK`, and `RENAME` remain namespace operations, including tombstones and temp-file replacement. The [current write publication](../../../crates/layerfs-workspace/src/filesystem/write.rs) is the starting point for these ordering and visibility rules.

The tree must preserve payload **custody** across shared roots: copied index pages add references to unchanged child pages and `Local` payloads; replacement drops references only after no active root, frozen Commit, handle, or uncertain outcome needs them. Reclamation must charge retained payload bytes and pages, make progress across partial failures, and never free a payload by looking only at the latest root. Adapt the current [custody](../../../crates/layerfs-workspace/src/backing/payload.rs) and [metadata reclamation](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs) rules rather than starting an unrelated storage path. No durability guarantee or Workspace-backing `fsync` is introduced.

## Incremental, continuously writable Commit

At the capture boundary, pin one immutable upper root (`G1`) and let subsequent calls update a successor (`G2`) that initially sees **all** of G1's changes while sharing its pages. Stream `G1`'s changed identities and ordered pieces with bounded cursors. For an existing file, reuse canonical `Base` ranges and feed its `Local`/`Zero` replacements to a streaming file builder; for a new file, stream complete content. Untouched file roots are reused. A file that has a huge changed region still sends those bytes; bounded memory is not a claim of sublinear I/O. Canonical chunking may read neighboring bytes and persistent growth must be measured, not asserted to equal the nominal edit length.

The change must cross **all** current request boundaries. [`lower_file`](../../../crates/layerfs-workspace/src/commit/lower.rs) presently collects the full piece vector and edit vector. [`save`](../../../crates/layerfs-workspace/src/commit/save.rs) sends one `EditFile` or `ConstructFile`; the [Bridge contract](../../../crates/layerfs-bridge/src/contract/request.rs) and [server save path](../../../crates/layerfs-server/src/service/save/content.rs) admit bounded edits and materialize replacement buffers. A streamed cursor/framed request must validate ordering, lengths, base identity, acknowledgements, and total resource charges across Workspace, Bridge, server, and the [content builder](../../../crates/layerfs-content/src/file/edit/mod.rs). Raising only one limit moves the failure to the next layer.

Likewise, a larger package frontier needs paged enumeration and bounded namespace batches through Workspace lowering, Bridge, server, and content construction. The current [content namespace walk](../../../crates/layerfs-content/src/filesystem/limits.rs) has its own 4,096-entry validation ceiling to review for that later scale. Candidate objects may be built in stages, but only **one** final Branch-head update publishes the Commit. A failed or unknown publication retains its frozen root and evidence until reconciliation resolves it. `G2` must retain all changes made after capture and be rebased on the newly published canonical root without silently losing, duplicating, or reordering them. Current [Commit reconciliation](../../../crates/layerfs-workspace/src/commit/reconcile.rs) provides a starting mechanism; correctness and resource use under long, overlapping Commit are **unproven** for this design. “Non-pausing” means Store construction does not hold the mutation lock; the capture/reconcile boundaries can still briefly synchronize and must be measured.

## Bounds, speed target, and limits of the claim

Keep explicit bounds on logical file length, actual private backing bytes, retained generations, indexed metadata pages, memory windows, transport frames, and construction workers. Charge work before admitting a mutation or stream stage; report `Capacity` or another precise failure when a real budget is exhausted. Avoid a fixed per-file piece or changed-interval count as the product contract. A workload with more changes may still hit its declared disk, memory, time, or file-size limit.

The first speed target is **structural**: a fixed 4 KiB overwrite in increasingly fragmented files should touch affected pieces and tree paths, not reload/rebuild all `P` pieces; Commit should walk a frozen file once with bounded memory. Measure callback counts/bytes, piece and page visits, physical backing growth, Commit substeps, and complete-command wall on the ordinary shell route under an eligible equal cache contract before stating a multiplier. The [1 MiB versus 10 MiB POSIX shift diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md) counted 51 versus 3,363 old-piece loads, but its cold latency was ineligible.

An arbitrary shell insert or prepend may rewrite its entire suffix through POSIX writes. This architecture removes repeated whole-index work **per callback**; it cannot infer a one-call splice or eliminate bytes the command actually submits. Large package installs and big files also require the separate namespace and streaming-Commit work above. The focused #245 verification should prove the earlier rename failure, small-write behavior, cleanup/progress symptom, and qualified performance before any greater-load claim.
