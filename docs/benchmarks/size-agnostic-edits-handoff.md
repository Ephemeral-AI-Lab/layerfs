# Size-agnostic durable edits - handoff note

Session-to-session handoff for the next milestone session. Read this before touching
code. Goal: make the durable one-byte edit cost independent of the file size (currently
~21 ms on a 1 MiB file vs ~236 ms on a 100 MiB file). Two reviewed design changes: a
bounded Merkle descent for the local rebuild, and count-only closure accounting. Both
were critically reviewed on 2026-08-11 by two research agents; this note records the
verified claims, the corrections, the concrete change list, and the acceptance criteria.

## 1. Problem statement (measured)

Per-edit steady-state cost, one-byte replace at mid-file (Windows x64, Node 24.11.1,
WAL, `synchronous=FULL`, 64 MiB SQLite page cache, mini-bench profile):

| File size | per-edit  | loaded manifest state | re-chunk work |
| --------- | --------- | --------------------- | ------------- |
| 1 MiB     | ~20-24 ms | 8 entries / 1 node    | 1 chunk       |
| 5 MiB     | ~29 ms    | ~35 entries           | 1 chunk       |
| 20 MiB    | ~46 ms    | 139 entries / 3 nodes | 1 chunk       |
| 100 MiB   | ~236 ms   | 668 entries / 6 nodes | 1 chunk       |

The re-chunk window is already size-agnostic: for equal-length edits the FastCDC gear
stream reconverges within ~32 bytes of the edit, so the bounded local rebuild re-chunks
exactly one chunk (verified at 1/20/100 MiB, `affectedEntries=1`, `newObjectCount=1`).

The scaling comes from three drivers:

1. **Full-manifest loader** - `loadAuthenticatedManifestState`
   (packages/fs/src/operations/durable-edit-prepare.ts:1128-1305) materializes the whole
   entry stream + all nodes (8 -> 668 entries) because the splice/regroup algorithm
   operates on full state.
2. **Segment-leaf closure bookkeeping** - the rebuilt segment leaf absorbs the whole
   canonical group (~130-256 entries on the 100 MiB file), and every entry gets an
   `efs_lease_objects` membership row + a reconciliation queue row + a re-verification
   pass, even though only 1 object is new.
3. **Disk** - the 100 MiB database (112 MB) exceeds the 64 MiB SQLite page cache, so the
   membership/reconcile object lookups on the mid-file objects hit the disk.

Ideal target (size-agnostic): ~20-40 ms for any file size above the leaf size.

## 2. Current routing and files to touch

- `packages/fs/src/operations/durable-edit-prepare.ts` - routing:
  `tryLocallyRebuiltContent` (bounded local rebuild, first) -> `buildCandidate`
  path-copy (fallback) -> streamed workspace rebuild (final fallback).
  `loadAuthenticatedManifestState`, `walkRebuiltSpine`, `persistLocallyRebuilt`,
  `runPersistenceSteps` (single-transaction persistence for small edits).
- `packages/fs/src/operations/local-rebuild.ts` -
  `rebuildManifestLocallyWithParametersOwned` -> `rebuildDiagnosticManifestLocallyOwned`
  (the splice + regroup over full state).
- `packages/fs/src/sqlite/manifest-tree-repository.ts` - `pathAtOffset` (the O(depth)
  descent that the bounded loader reuses), `registerReusedSubtrees`.
- `packages/fs/src/sqlite/staging-repository.ts` - `appendBatch` (batched membership),
  `reconcileBatch` (leaf-edge closure), `seal`, `#leaseCharge`, `expireBatch`.
- `packages/fs/src/sqlite/usage-repository.ts` - `staging_bytes` accounting.
- `packages/fs/src/sqlite/schema.ts` - `efs_lease_objects`, queue, certificate rows.
- `tests/algorithms/manifest.test.mjs`, `tests/storage/durable-edit.test.mjs`,
  `tests/storage/durable-local-rebuild.test.mjs`,
  `tests/storage/overlay-staging.test.mjs`.

## 3. Change A - bounded Merkle descent (removes drivers 1 and 3)

### Verified claims

- `pathAtOffset` (manifest-tree-repository.ts:93-218) already descends root -> leaf in
  O(depth) via the child spans and returns the leaf frame + entry indices; reuse it
  instead of the full-tree loader.
- The closure and validation walks stop at claimed nodes
  (staging-repository.ts:1446-1458, queue `processed=1`; 1526-1585), so only the old
  nodes **directly referenced by rebuilt nodes** need claims - i.e. the union of the
  path frames' children arrays. `registerReusedSubtrees` consumes exactly
  `{sourcePath, nodeHash, span, entryCount}` - all derivable from the path metadata
  (`[...frame.path, i]`). The current `walkRebuiltSpine` over-claims (every old node
  visited; durable-edit-prepare.ts:1367-1379) - the bounded version fixes this too.
- The canonical grouping constraints (leaf 64/128/256, internal 32/64/128,
  manifests/grouping.ts) only shape the segment length; nothing forces touching groups
  beyond the splice's neighborhood.

### Corrections from the review (must implement)

1. **Relative-coordinate regroup** - the absolute group indices
   (`prefixGroupCount`/`reconnectGroup`/`totalGroupCount` in local-rebuild.ts:330-411)
   are NOT derivable from entry counts (the ancestors' children metadata carries
   hash/span/entryCount, not child counts). Rewrite the regroup arithmetic as relative
   to the affected group: the affected group's start and the old boundaries inside the
   parent's subtree come from the path frames; the loop termination must be reframed via
   the path's root depth (keep the height-growth branch local-rebuild.ts:872-880
   relative).
2. **Fringe sibling loads** - when the rebuilt boundary crosses the parent's range, load
   the next sibling node's children array (a ~6 KiB BLOB for a 128-child internal) at
   each level, window-capped. The group-level reconnect (`reconnectGroupFor`, a
   boundary-coincidence search) is fast in practice but not worst-case bounded - add a
   hard window cap -> fallback (no regression, only lost wins).
3. **Dirty-end leaf** - the chunker reconnect checks `oldLayout.boundary`; when the
   delete crosses the affected leaf's end, the true reconnect boundary is inside a later
   leaf. Load the second leaf via `pathAtOffset(manifestHash, dirtyOldEnd)`; without it
   the chunker over-scans up to a full leaf and trips the 16 MiB `maxAffectedBytes`
   window (spurious fallbacks). The entry stream and resulting manifest stay
   byte-identical either way - only limits trip.
4. **Segment-node dedup claim paths** - a segment node that hash-matches an old node NOT
   on the path needs that old node's source path for its claim; not derivable from the
   path. Explicit throw -> fallback (a regression test with repeated content).
5. **Validation certificate** - `pathAtOffset` requires
   `efs_manifest_validations.tree_depth`; the current loader never reads it. One extra
   row in the bounded loader's read transaction.

### New code shape

- `loadBoundedManifestState(port, manifestHash, edit, storage, limits, cache)` - one
  read transaction: certificate + root + `pathAtOffset(edit.offset)` +
  `pathAtOffset(dirtyOldEnd)` + a capped right-fringe crawl. Returns path frames (node,
  childIndex, finalAtLevel, prefixEntryCount, childStarts), the affected leaf + its
  entries + start indices, the dirty-end leaf's entries, and the fringe sibling nodes.
- `rebuildManifestBoundedOwned(state, source, edit, limits)` - mirrors
  `rebuildDiagnosticManifestLocallyOwned` (local-rebuild.ts:582-935) with the relative
  regroup and the two-leaf boundary maps. Drops `orderedLevels` and
  `authenticateDiagnosticManifest` (the path walk validates instead).
- `walkRebuiltSpineBounded` - walks the segment nodes for
  newNodes/objects/validationRows; derives the frontier claims from the path frames;
  keeps the unchanged-root shortcut and the dedup fallback.
- Keep the full-state path (`loadAuthenticatedManifestState` +
  `rebuildManifestLocallyWithParametersOwned` + `walkRebuiltSpine`) as the fallback;
  `tryLocallyRebuiltContent` attempts bounded first and catches a new
  `BoundedRebuildFallbackError` (aligned with the existing RangeError /
  DurablePathCopyFallbackError handling).
- `persistLocallyRebuilt` is unchanged (its input contract stays).

### Test impact (Change A)

- `tests/algorithms/manifest.test.mjs` full-state call sites stay (they become the
  fallback's coverage). Add golden-vector tests: bounded output (rootHash, root, entry
  stream, splice) must equal the full path byte-for-byte across the seeded corpus at
  1/20/100 MiB + append/prepend/truncate/mid-leaf delete/cross-leaf delete/EOF, plus the
  dirty-end-leaf map and reconnect-window assertions.
- `tests/storage/durable-local-rebuild.test.mjs` and `durable-edit.test.mjs`: mode and
  metric-shape assertions only - expected unchanged (verify the bounded statement count
  still lands within the fault-loop's occurrence 1-12 range).

## 4. Change B - count-only closure accounting (removes driver 2)

### Verified claims

- The old records' objects are durable (loaded from the sealed source manifest's nodes)
  and GC-protected mid-edit: GC roots are inodes/revisions/lease-manifest links, not
  `efs_lease_objects` rows; the source closure is permanently marked in every GC run;
  `protectSourceManifest` links the source root to the lease and the generation checks
  force re-seeding on staging begin. The lease rows are only the sole protection for
  objects staged-but-not-yet-in-a-rooted-closure (the streamed window) - count-only
  members are never in that class. Document as an invariant.
- With the changes below, every seal cross-check passes unchanged:
  `reconciled.object_count/bytes/node_count/bytes/membership_count/next_sequence === certificate.*`
  (staging-repository.ts:1096-1106, 1150-1165), `validateSealed` (1201-1224),
  `#validateShape` membershipCount === objectCount + nodeCount (1789).
- The spec already mandates this: docs/spec/storage-and-data-model.md:653-654 ("must not
  duplicate closure as one lease row per object").

### Corrections from the review (must implement)

1. **Usage/release symmetry (option B - the hidden break).** Count-only members must NOT
   enter `staging_bytes`/ingest (they are already-durable content; quota-neutral) so the
   recount stays exact with no schema change. But release must then subtract the
   row-backed sum: `staged_bytes = c.node_bytes + SUM(efs_lease_objects.size)` instead
   of `c.object_bytes + c.node_bytes`, in BOTH `#leaseCharge`
   (staging-repository.ts:1670) and `expireBatch` (554). Missing this ->
   `ECORRUPT: usage counter underflow` on every count-only lease release (HIGH risk,
   silent failure). Option A (count-only enters staging_bytes) forces a schema v5
   migration and is rejected.
2. **Queue rows MUST stay.** Count-only edges still get reconciliation queue rows (PK
   `lease_id,kind,hash`); only the `efs_lease_objects` rows and the re-verify pass are
   dropped. Skipping queue rows is unsound for deduplicated files whose shared hash
   appears in two rebuilt leaves (the certificate counts it once; the queue dedupes it
   once).
3. **Hash-binding gap.** Relaxing the edge check from the membership JOIN to a CAS-only
   size check means a closure edge to a durable-but-undeclared object X with the same
   size as a declared member A would pass, and the completion cross-check compares
   counts only (the chain attests A, the closure contains X). Unreachable in the
   durable-edit flow (append set and closure built from the same data), but real for the
   relaxed API. Recommended fix at constant work: fold each leaf edge hash into a
   commutative accumulator on `efs_staging_reconciliations` (one UPDATE per batch
   computing a hash fold in JS) and compare against the chain fold at completion.
   Alternatively document the invariant explicitly and keep the JOIN for non-count-only
   edges.
4. **Dedupe/ordering invariant.** A count-only hash re-appended as full (or twice)
   extends the chain twice while the closure counts it once -> fail-closed at seal
   ("complete manifest closure differs"). Unreachable in-flow (the splice/full appends
   precede the spine count-only append; `spine.objects` is a deduped Map) - document the
   ordering and add a fail-closed regression test.

### Change list (Change B)

1. `StagingMember` gains an optional `counted` flag (staging-repository.ts:28,
   storage-ports.ts:380-404).
2. `appendBatch` (staging-repository.ts:678): for `counted` members - run the batched
   `SELECT hash,size FROM efs_cas_objects WHERE hash IN (...)` + size match; extend the
   chain + increment object_count/object_bytes/sequence/ membership_count; skip the
   `efs_lease_objects` insert, `#changeMetadataRows`, `stagedDelta`/ingest consumption,
   and `#admitStagingBytes`. Reject `counted` members that are already full members.
3. `reconcileBatch` leaf-edge backing (staging-repository.ts:995-1014): replace the
   `JOIN efs_lease_objects` with the CAS-only batched SELECT (size vs `edge.length`) for
   all edges; same relaxation in the `#enqueueVerified` kind-0 branch (1394-1407). The
   queue-row machinery (1016-1046) is untouched.
4. Release sites (option B): `#leaseCharge` and `expireBatch` (see correction 1).
5. `walkRebuiltSpine` (durable-edit-prepare.ts:1319): emit the object list split into
   full (splice hashes) and count-only (boundary records whose hash is not in the
   splice/put set) sets; pass to `persistLocallyRebuilt`.
6. `persistLocallyRebuilt` (durable-edit-prepare.ts:1688-1708): spine append uses the
   count-only variant for boundary records; shrink `durablePayloadReservation` and
   `metadataRows` accordingly (over-reservation only leaks at cleanup - safe).
7. Optional hardening: the closure fold accumulator (correction 3) + the fail-closed
   regression test (correction 4).

### Test impact (Change B)

- No m2 acceptance semantic change; the sealed-closure constant-row test
  (overlay-staging.test.mjs:1855-2079) exercises the streamed path (no count-only flow)
  and asserts equality + statement bounds, both preserved. No test pins the "complete
  manifest closure differs" message.
- New tests required: count-only seal + validateSealed; GC sweep survival of count-only
  objects; recount/release symmetry; ordering/dedupe fail-closed; the cross-leaf
  shared-hash case.

## 5. Implementation order and verification checkpoints

1. **Golden-vector harness from the current full path** - byte-identical oracle for the
   bounded path.
2. **Bounded loader + bounded builder** (pure, in-memory) - verified byte-identical
   against the oracle at 1/20/100 MiB + the edit-shape corpus.
3. **Frontier claim generator + SQLite fixture tests** - seal + reads after every edit
   (the existing validateSealed pattern).
4. **Fallback chain wiring** (bounded -> full-state -> path-copy -> streamed) +
   fault/limit tests. New `BoundedRebuildFallbackError`.
5. **Count-only closure** (Change B) - append + reconcile + release + the fold.
6. **Metrics + benchmark** - 1/20/100 MiB per-edit sweep (mini-bench A5/B4 cells);
   target ~20-40 ms per edit on the 100 MiB file (the load + disk drivers removed).

## 6. Acceptance criteria (size-agnostic edit)

- Per-edit cost on the 100 MiB file drops from ~236 ms to ~20-40 ms (within ~2x of the 1
  MiB per-edit cost) at the 1/20/100 MiB sweep.
- Byte-identical matrix unchanged (append/prepend/truncate/replace/EOF + cross-leaf
  deletes) vs the streamed rebuild reference.
- M2 acceptance semantics unchanged: seal cross-checks, validateSealed, the constant-row
  closure test, the usage recount, GC sweep survival of count-only objects.
- All fault-injection suites (every persistence statement position) stay green.
- `pnpm validate:m3` + `pnpm check:evidence` pass from a clean worktree.

## 7. Known constraints

- The group-level reconnect search is probabilistically fast but not worst-case
  bounded - the window cap + fallback is mandatory (no regression, only lost wins).
- The count-only API's ordering invariant (full appends before count-only, deduped sets)
  must be documented and pinned by tests.
- The closure fold (Change B correction 3) is the difference between "counts exact" and
  "chain binds the closure" - recommend implementing it, not just documenting.
- Ground rules: no node-only imports in `packages/fs/src`; hashes stay byte-identical to
  pure-JS (M1 golden vectors + workerd parity); memory admission, statement/ elapsed
  budgets, `efs_usage` exactness, quota ceilings, and WAL backpressure semantics are
  untouched; worktree stays clean before `check:evidence`; no commits without explicit
  instruction.
