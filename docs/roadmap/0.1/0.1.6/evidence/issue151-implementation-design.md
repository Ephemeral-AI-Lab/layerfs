# v0.1.6 sandbox-local experiment: implementation design (issue #151)

Status: design frozen 2026-09-15 after the ordered source-reading phase; drives
I1–I6 implementation on `codex/v016-sandbox-local-experiment` from
`6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`. Source-map evidence lives in the
#151 ledger entries L2 (four parallel read-only source maps).

## 1. What stays (preserved pipeline)

- Canonical identity/dedup (`ObjectId` BLAKE3 over framed objects), SmallContent
  (<128 KiB nonempty, `LFS5SML\0`), bounded FULL/DELTA chains (8 edges,
  512 KiB decoded / 256 KiB encoded closure), savings tests and FULL fallback,
  candidate cache.
- Large-file CDC (`FastCDC` frozen profile) + extent rope; localized edits via
  `FileMutationBatch`/`mutate_existing_file` with `set_physical_predecessor`
  (`PredecessorCursor` hints, `first_span` provenance); unchanged extent reuse.
- Compression/packing (zstd, pack v3/v4 limits), checked admission
  (`AdmissionSession` baseline, `TicketGate` permit, rollback ownership,
  dependency closure), staging + conditional SQLite publication
  (`commit_workspace_candidate`: stage → retain → verify stage+branch CAS →
  INSERT_COMMIT (idempotent, derived id) → ADVANCE_BRANCH (1 row) → delete
  stage → COMMIT → precomputed outcome). Store config unchanged
  (journal MEMORY, synchronous OFF, mmap 0, cache 32 MiB).
- The immutable-base service (SEED/LOOKUP/DIRECTORY_PAGE/READ_BASE) and the
  control/observer lanes; FUSE dispatch, operation gate, kernel-cache
  reconciliation; SDK edit wire transaction (EDIT_BEGIN/PART/END).

## 2. What changes

### I1 — Local payload ownership (`layerfs-fuse`)

New `local_spool.rs` (daemon side): packed `LocalSegment` files under the
mount's private backing dir `/snapshots/<workspace-id>/` (derived from
`WorkspaceId`, not caller-supplied). `write_owned` keeps its per-node ordering
and `prepare_write`/`apply_edit` piece-tree flow, but bytes go into a
`LocalSegment` (buffered ≤1 MiB pending window, flushed with positioned writes
through the existing physical-worker permits; **no fsync/sync_data ever**).
`Piece::Spool` keeps referencing `BackingRef`, whose resource becomes the local
segment (adapter-defined resource — core unchanged). Reads of pending ranges
serve from the pending buffer; flushed ranges read the local file. Segment cap
1 MiB (bounds dead-segment retention). Retirement drops segment files when no
piece/reader references them. fsync becomes volatile: flush pending window,
surface known write/allocation errors, retire idle ranges; no host
CHECK/FACTS/CANCEL traffic. Host-side `BackingOwner` loses RESERVE/APPEND/
CANCEL_RESERVATION/CHECK/RELEASE/READ_BACKING/FACTS_* (facts mirror removed);
SEED/LOOKUP/DIRECTORY_PAGE/READ_BASE remain.

Mount layout: daemon workspace root prefix becomes `/workspaces`
(`protocol.rs::WORKSPACE_ROOT` + `validate_root`); caller-supplied placement
roots remain usable when unique/validated; private backing dir is
`/snapshots/<workspace-id>/`, created at mount, removed at End/Discard.

### I2 — Snapshot capture (workspace-core + live_owner)

`LiveWorkspace` grows an optional frozen frontier:
`FrozenFrontier { ids: BTreeSet<NodeId>, retained: HashMap<NodeId, Node> }`
plus `covered_generation`. Capture (new `CAPTURE` control op, host-initiated,
short state lock): compute `ids = dirty ∪ children named by dirty directories'
changes` (bounded by the changed frontier — no namespace scan), **move** the
dirty set into the frontier (O(1)), leave live dirty empty, record
`(incarnation, generation, base_root, head)`. Node values are NOT cloned at
capture. Every workspace-core mutation site that replaces/removes/pins a node
value calls `protect(id)`: if a frontier is active and `id ∈ ids` and not yet
retained, clone the current value into `retained` (piece trees share by Arc;
per-node, once). This is "copy only touched shared nodes; mutate exclusive
nodes in place". The frozen view of `id` = `retained[id]` if present else the
live value (unmodified since capture). Deleted nodes keep serving from
`retained`.

### I3 — Bounded frozen transfer (wire + transport + host client)

New wire opcodes (validated framing, ≤ `MAX_FRAME`, bounded pages):
- Control lane ('c', host→daemon): `CAPTURE` → token
  `{incarnation, generation, attempt, dirty_count, base_root, head}`;
  `COMPLETE` (completion records, see I5); `SNAP_CANCEL`.
- New snapshot lane ('s', host-initiated request/reply, daemon-dialed at mount
  like 'c'/'o', own bounded admission ~4 requests/8 MiB in flight):
  `SNAP_RECORDS {token}` pages (≤128 nodes/64 KiB, `wire::node_out` records,
  end-of-stream `{generation, count}`), `SNAP_READ {token, segment_id,
  offset, len ≤1 MiB}` (payload bytes from local segments — shared live/frozen
  ownership). Tokens are validated on every frame; a late/misaddressed frame
  is rejected. A stalled 's' lane cannot block FUSE ops (no shared locks),
  the 'c' lane (SDK edits/status/capture) or the 'd' lane (READ_BASE) — the
  I3 exit-check scenario.

### I4 — Host build from frozen input (workspace crate)

Commit (remote path) becomes: lifecycle lock (one active committer; a second
commit queues — at most one) → **no pause/FREEZE/quiesce** → `CAPTURE` → pull
records over 's' into an owned `HashMap<NodeId, Node>` (+ dirty set,
`canonical_nodes`) → `CandidateInputs::build` unchanged, `worker_limit` from
new shared env knob `LAYERFS_CONSTRUCTION_WORKERS` (default preserves current
behavior; **set to 1 in both arms** — one construction worker in control and
candidate; plan already forces 1 when predecessors exist). `Piece::Spool`
resources resolve through a host-side `RemoteSegment` adapter that fetches
bytes via `SNAP_READ` (blocking bridge on builder threads, small bounded
window, sequential access) — payload streams from local backing; no full host
staging copy. A registry-level construction gate serializes builds across
workspaces (one shared compute worker). Publication unchanged. The build holds
no lock that live FUSE/SDK service needs (no backing mutex; the materialized
input is an owned copy).

### I5 — Generation-safe completion + lifecycle

After successful publication, the host derives completion records from the
existing `Checkpoint` journal (node, inode, content root, attr) plus each
node's captured revision, and sends `COMPLETE {token, root, head, records}`:
the daemon applies, under the state lock, per record: if the live node's
current revision equals the captured revision (untouched since capture), apply
the canonical re-base (`FileData::Base{root}`, `canonical = inode`,
directories: `base = Some(root)`, `changes.clear()`); nodes whose revision
advanced are left untouched (their G+1 pieces/dirty remain — a late C1
completion can never clear them). Then set `head`, `base_root = root` (and
`covered_generation = generation`, `known_names` invalidated as in
`finish_checkpoint`), drop the frontier (`retained`/`ids`), retire unowned
local segments. `mutation_generation` stays monotonic; dirty-ness is the live
dirty set (OBSERVE reports it; is_dirty follows). The old INSTALL_*/FREEZE/
RESUME route is removed from the candidate; host re-base of the
`Workspace` shell (reader/base_root/base_inodes/expected_head) happens as
today after publication. `pending_publication` (exact outcome retention) is
kept and extended with the token + completion records; a lost completion is
re-resolved on the next Commit/End (never recaptured as a new snapshot of
newer state). SDK edits stop taking the whole-Commit `lifecycle` lock (they
never needed it on the remote route; commit keeps it). End/Discard retires the
mount, the frontier, pending attempt resolution and the backing dir; it never
canonicalizes for disposal; siblings are untouched (per-Workspace registry
keying already exists).

### Attempt identity

Token = (WorkspaceId, mount incarnation (fresh per mount), generation,
attempt counter). Validated on CAPTURE results, every 's' frame, and
COMPLETE. A late message cannot act on a successor snapshot/workspace.

## 3. Shared adapters applied to BOTH arms (identical patch, hash recorded)

1. `LAYERFS_CONSTRUCTION_WORKERS` env knob (default = current
   `available_parallelism().min(8)`), set to 1 for both arms' benchmark runs —
   the one-worker rule in control and candidate. Control label: "v0.1.5 +
   shared experiment adapters", not an untouched release.
2. Benchmark B3 case `local-snapshot-create-25000-onebyte-v1` (3 steps:
   C1 create `f00000..f24999` one byte `ordinal % 251`, normalize
   (files 0644, root 0755, mtime 1700000000) + root fsync + Commit; C2 change
   ordinals `97*j`, j=0..255, to `(ordinal+1)%251`, normalize + root fsync +
   Commit; C3 restore, normalize + root fsync + Commit; End). Same workload
   steps and sync calls in both arms; empty initial namespace; nothing
   precreated.
3. Minimal additive metrics for the declared gates (temporary backing
   observation of the workspace-private backing trees, CPU/memory windows)
   collected identically in both arms.

## 4. Verification plan (I6, focused)

- Unit: local spool write/read/pending/retire; volatile fsync contract;
  capture COW stability (write A → capture → modify/rename/delete → frozen
  view sees A, live sees successor); token rejection (late/misaddressed);
  completion coverage semantics (revision match/advance, duplicate COMPLETE,
  delayed COMPLETE after G+1 edits).
- Integration (existing harness style): held-builder non-pausing check with
  latches (write during build); stalled 's' lane with live write/SDK
  edit/uncached base read; partial transfer cancel → release; two workspaces
  (equal paths different bytes; End A keeps B usable); Store-only reads after
  workspace destruction; encoding proofs (CAS reuse, DELTA+FULL fallback,
  128 KiB threshold, localized extent reuse) via existing tests
  (`fastcdc_shifted_stream.rs`, `extent_model.rs`, small-candidate tests).
- Publication faults: existing store tests + completion-loss path.

## 5. Honest limitations carried forward

- Writable shared mmap visibility remains unresolved (documented limitation;
  no silent disable, no claim ordinary-write tests prove mmap snapshots).
- No durability/fsync on workspace backing (by design; volatile sync only).
- Single-sample gates; release admission stays false for this profile.
