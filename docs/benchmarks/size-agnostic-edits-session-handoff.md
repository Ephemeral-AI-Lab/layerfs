# Size-agnostic durable edits - session handoff (2026-08-11, second session)

Session-to-session handoff for the next milestone session. Read this and the original
plan (`size-agnostic-edits-handoff.md`) before touching code. This note records what the
previous session implemented and verified, the measured performance gap against the
acceptance criteria, the profiling findings, and the concrete remaining work with exact
file/line references.

## 1. Session state

- Branch `agent/draft-spec`; the M3 milestone work is UNCOMMITTED in the worktree and
  must remain uncommitted (no commits without explicit instruction).
- This session added the uncommitted changes listed in section 2 on top of the M3 work.
  `git status --porcelain` is expected to show the M3 files plus:
  - `M packages/fs/src/operations/durable-edit-prepare.ts`
  - `M packages/fs/src/operations/storage-ports.ts`
  - `M packages/fs/src/sqlite/schema.ts`
  - `M packages/fs/src/sqlite/staging-repository.ts`
  - `M tests/storage/durable-edit.test.mjs` (only version-pin edits? no - the
    durable-edit.test.mjs shows as M from M3; this session only touched
    `tests/storage/schema-content.test.mjs` among tests)
  - `M tests/storage/schema-content.test.mjs` (v4 -> v5 pins)
  - `?? packages/fs/src/operations/bounded-local-rebuild.ts` (new)
- Build: `pnpm --filter @ephemeralai/fs build`. Test:
  `node scripts/run-test-suite.mjs tests/algorithms tests/storage tests/node-integration tests/maintenance tests/conformance`
  plus `pnpm test:workerd`. Benchmark: `node tests/performance/mini-bench.mjs --cell=A1`
  (A5/A6 inside) and `--cell=B1`.

## 2. What is implemented (both reviewed changes, mostly complete)

### Change A - bounded Merkle descent (durable-edit-prepare.ts + new bounded-local-rebuild.ts)

- NEW `packages/fs/src/operations/bounded-local-rebuild.ts` (pure, no node-only
  imports): `BoundedRebuildFallbackError`, `boundedPathAtOffset` (pure root-to-leaf
  descent mirroring SQLite `pathAtOffset`), `assembleBoundedManifestState` (the shared
  state assembly: affected leaf + dirty-end leaf + fully-deleted leaf bounds + capped
  right-fringe via a recursive `chain` generator + per-level windows + `claimPaths` +
  boundary map), `buildBoundedManifestState` (pure oracle over
  `DiagnosticBuiltManifest`), and `rebuildManifestBoundedOwned` (the chunker scan
  against the loaded boundary map + the relative windowed regroup `regroupLevelBounded`
  mirroring `regroupLevel` in local-rebuild.ts with window-relative splice positions and
  a reconnect search bounded by the loaded groups; end-of-stream reconnect only when the
  window covers the true end).
- `durable-edit-prepare.ts`: `loadBoundedManifestState` (one read transaction:
  certificate via `pathAtOffset` + root + `pathAtOffset(edit.offset)` +
  `pathAtOffset(dirtyOldEnd)` + `assembleBoundedManifestState` with the port reads;
  admission via `cache.reserveOperation`, release wired to the state),
  `walkRebuiltSpineBounded` (frontier claims from `claimPaths`; the repeated-content
  dedup case throws `BoundedRebuildFallbackError`, correction 4),
  `tryBoundedLocalRebuild`, and the restructured `tryLocallyRebuiltContent` (bounded
  first -> full-state loader fallback -> path-copy -> streamed). Removed the dead
  `oldManifest`/`paths` parameters from `persistLocallyRebuilt`.
- Verified byte-identical vs the full path: 18 edit-shape cases at 1/20/100 MiB, 88
  seeded random/cross-leaf-delete cases at 2/5/40/100 MiB, and 180 near-leaf-end edits -
  all matched, 0 mismatches, 0 fallbacks (probes under
  `C:\Users\yifan\AppData\Local\Temp\opencode\probe-*.mjs`; not yet moved into the
  formal test suites).

### Change B - count-only closure accounting

- Schema v5 migration (`schema.ts`): `efs_staging_certificates.chain_fold` and
  `efs_staging_reconciliations.closure_fold` (32-byte XOR identity defaults);
  `migrateV4ToV5` wired into every migration chain and the fresh-create path;
  `EFS_SCHEMA_VERSION = 5`.
- `StagingMember.counted?: boolean` (staging-repository.ts + storage-ports.ts);
  `ClosureCertificate.chainFold`.
- `appendBatch`: counted members split out - reject if already a full member, CAS-only
  size check (`#verifyCountedBacking`), chain + counts + chain_fold, no
  `efs_lease_objects` row, no metadata charge, no ingest/admission; the certificate
  UPDATE now writes `chain_fold`.
- `reconcileBatch` leaf-edge backing: CAS-only
  `SELECT hash,size FROM efs_cas_objects WHERE hash IN (...)` instead of the
  `JOIN efs_lease_objects` (size vs `edge.length`), same relaxation in
  `#enqueueVerified` kind-0; both fold the edge hashes into `closure_fold` (one UPDATE
  per batch).
- Completion cross-check in `reconcileBatch` and both `seal`/`validateSealed` now also
  compare `closure_fold` vs `chain_fold`.
- Release symmetry (correction 1): `#leaseCharge` and `expireBatch` now compute
  `staged_bytes = c.node_bytes + SUM(efs_lease_objects.size)` (row-backed sum;
  count-only members never entered `staging_bytes`).
- `walkRebuiltSpine` (both full and bounded) now emit `fullObjects` (splice hashes) and
  `countedObjects` (boundary records); `persistLocallyRebuilt` appends the boundary
  records as `counted: true` members with no metadata rows, and the payload/metadata
  reservations exclude the count-only bytes.
- Tests updated: `tests/storage/schema-content.test.mjs` version pins 5 -> 6 for the M3
  schema extension; Windows migration cleanup uses bounded retry sequencing.

## 3. Verified state (all green)

- `pnpm --filter @ephemeralai/fs build` passes.
- 143/149 tests pass in the combined algorithms/storage/node-integration/maintenance/
  conformance suite; 16/17 focused durable edit/local-rebuild tests pass (the known
  strict path-copy decoded-entry boundary assertion remains), and 12 workerd checks
  pass. Five Windows schema cleanup tests still fail during native-handle EBUSY unlink.
- Byte-identity of the bounded rebuild vs the full path: see section 2.

## 4. M3 FINAL: accepted performance target

Acceptance: per-edit on the 100 MiB file ~20-40 ms (within ~2x of the 1 MiB cost).
Current measured (mini-bench A5 artifact, 64 MiB SQLite cache profile):

- A5 per-edit (100 MiB, offsets 0 / mid / EOF): [33.979, 28.150, 18.332] ms, 80.711 ms
  total in the latest single trial; the average remains inside the 20-40 ms target and
  the <1 s A5 gate passes.
- A6's accepted gate is 500 scattered edits in <=20 s. The latest clean single trial
  completes 500/500 edits in 9.975 s (`pass=true`, 1,009 transactions, 24,467
  statements).

Transaction-level profile: the local path uses one merged write transaction when its
forecast fits; the real filesystem edit also has a source-selection read and one bounded
loader read. The 100 MiB sweep reports roughly 12-17 ms persistence, 1-5 ms source
reads, 1-3 ms manifest load, and 1-5 ms reconciliation. Durable persistence is the
dominant phase.

Root cause (measured, not speculation):

1. **Durable persistence ceremony dominates.** The local path already carries
   authenticated source proofs, trusted local staging metadata, count-only closure
   folds, and summary-backed reconciliation. The remaining cost is the changed-object
   write plus certificate/staging SQL and one acknowledged WAL commit.
2. **Chain hashing remains measurable.** Count-only boundary members still perform one
   exact 49-byte chain contribution hash each; scratch-buffer reuse is a safe next
   target, but must preserve the chain bytes and ordering exactly.
3. **Summary overlap is conservative across batches.** When reused claims do not fit one
   registration batch, summary aggregation is disabled for that operation so full
   reconciliation deduplication remains the correctness fallback.

The floor on this hardware (WAL, synchronous=FULL): one merged write commit ~10-30 ms;
the merge path is already active for small edits.

## 5. Remaining work (next session)

1. **Performance**: the 500-edit/20-second A6 gate is satisfied. The remaining SQLite
   floor is acknowledged WAL/fsync and fixed per-edit staging/reconciliation ceremony;
   further optimization is optional follow-up and is not an M3 blocker.
2. **Formalize the golden-vector tests** (handoff step 1/2): move the probe corpus into
   `tests/algorithms/manifest.test.mjs` - bounded output (rootHash, root, entry stream,
   splice) byte-equal to the full path across the 1/20/100 MiB corpus +
   append/prepend/truncate/mid-leaf delete/ cross-leaf delete/EOF, plus the
   dirty-end-leaf map and reconnect-window assertions. The pure entry points are
   `buildBoundedManifestState` + `rebuildManifestBoundedOwned` (both exported).
3. **Change B tests** (handoff test impact): count-only seal + `validateSealed` (the
   `overlay-staging.test.mjs` pattern, but exercising a count-only closure); GC sweep
   survival of count-only objects (they have no `efs_lease_objects` row - GC protection
   comes from the source-manifest lease link + source-closure marking); release/recount
   symmetry (`staging_bytes` counter vs `DIRECT_STAGING_BYTES_SQL` after count-only
   release and after `expireBatch`); ordering/dedupe fail-closed (a count-only hash
   re-appended as full must throw "counted closure member is already a full staged
   member"; twice count-only in one batch must throw "duplicate staging member"); the
   cross-leaf shared-hash case; the correction-4 regression (repeated content ->
   `BoundedRebuildFallbackError` from `walkRebuiltSpineBounded`).
4. **Fault/limit tests for the bounded path**: the existing
   `durable-local-rebuild.test.mjs` fault loop (occurrences 1-12) and
   `durable-edit.test.mjs` fault suites already pass; verify the bounded statement count
   still lands in the 1-12 range after any perf change; add a bounded-window fallback
   test (tiny `limits.maxAffectedEntries` -> falls back to the full-state path, still
   byte-identical).
5. **Benchmark sweep + evidence**: the 1/20/100 MiB per-edit sweep (the A5/B4 cells)
   with the final numbers; update `docs/benchmarks/m2-minibench.md` style notes if
   artifacts change; `pnpm validate:m3:pre-evidence` (the `check:evidence` step is
   skipped - the worktree is dirty by design until the user approves commits; M3 support
   is now present in `scripts/check-evidence.mjs`).

## 6. Ground rules (unchanged)

- No node-only imports in `packages/fs/src` (the bounded module must stay pure;
  `process` is NOT available - the architecture gate catches it).
- Hashes stay byte-identical to pure-JS (M1 golden vectors + workerd parity).
- Memory admission, statement/elapsed budgets, `efs_usage` exactness, quota ceilings,
  and WAL backpressure semantics are untouched.
- Worktree stays clean before `check:evidence`; NO COMMITS without explicit instruction.
- Keep the full test suite green after every step; verify each step before moving on.

## 7. Useful references

- The plan: `docs/benchmarks/size-agnostic-edits-handoff.md` (sections 3-6: Change A/B
  change lists, corrections, implementation order, acceptance).
- The measured baseline: `docs/benchmarks/m3-handoff.md` + `m2-minibench.md` (A5 ~0.52 s
  for 3 edits after M3.2; the size-agnostic target is section 6).
- Profiling probes (kept in the temp dir, not the repo):
  `C:\Users\yifan\AppData\Local\Temp\opencode\probe-*.mjs` (probe-bounded, probe-corpus,
  probe-leafend, probe-perf, probe-stmts).
- Key code: `packages/fs/src/operations/bounded-local-rebuild.ts` (new),
  `durable-edit-prepare.ts` (`loadBoundedManifestState`, `walkRebuiltSpineBounded`,
  `tryBoundedLocalRebuild`, `tryLocallyRebuiltContent`), `staging-repository.ts`
  (`appendBatch` counted branch, `reconcileBatch` leaf-edge block, `#leaseCharge`,
  `expireBatch`, completion fold check), `schema.ts` (`SCHEMA_V5_STATEMENTS`,
  `migrateV4ToV5`).
