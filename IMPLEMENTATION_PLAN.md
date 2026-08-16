# LayerFS Implementation Plan

This plan implements the restart specification in
[`SPEC.md`](SPEC.md) and uses the benchmark contract in
[`eval.md`](eval.md).

The evaluation harness starts before the storage engine. We do not wait for
the SDK or the final integration phase to discover that capture is still
`O(file size)`.

The first qualified host composition is:

```text
macOS + APFS + Rust release build + SQLite rollback journal
```

APFS is the first benchmark environment, not part of canonical identity or
the logical LayerFS format. The initial implementation must work on ordinary
APFS file I/O without depending on `clonefile`, reflinks, or APFS-specific
copy-on-write behavior. Any APFS optimization is a later measured option.

## Phase 0 — Repository, evaluation harness, and APFS baseline

### 1. What to implement

- Create the Rust workspace with these initial crates:

  ```text
  layerfs-core
  layerfs-engine
  layerfs-os
  layerfs-vfs
  layerfs-sdk
  ```

- Create the deterministic dataset generator for the single-file and mixed-tree
  datasets in `eval.md`.
- Create the benchmark result format and artifact writer.
- Create the correctness oracle that compares a materialized directory with an
  expected directory tree.
- Define timing boundaries and counters before production code depends on them.
- Record the host composition in every run:
  macOS version, APFS volume, filesystem case behavior, CPU, memory, SQLite
  version, Rust version, journal mode, and synchronous mode.
- Define the SQLite initial settings as rollback journal with WAL disabled.
- Add a macOS/APFS environment probe, but do not make APFS details part of
  canonical paths, hashes, object IDs, or root IDs.

### 2. What to test

- Run B0 from [`eval.md`](eval.md) twice and confirm identical dataset
  manifests and root inputs.
- Verify the result artifact records the source commit, dirty-tree state,
  dataset hash, benchmark command, and APFS environment.
- Verify that unavailable memory, cache, or filesystem observations remain
  explicitly unavailable rather than becoming zero.
- Verify the correctness oracle detects a changed byte, missing file, extra
  file, wrong file kind, and wrong file length.
- Verify cold, warm, and reopened labels are not interchangeable.
- Confirm the benchmark can run on the actual APFS volume before implementing
  the SQLite or filesystem path.

## Phase 1 — Canonical core: format, identity, and objects

### 1. What to implement

- Implement `layerfs-core` without a database dependency.
- Implement bounded canonical paths, immediate directory names, and unsigned
  byte ordering.
- Implement the fixed 9-byte object envelope:
  `LFSO` + kind byte + big-endian `u32` payload length + payload.
- Implement only the Phase 1 bytes and directory object kinds. Do not add a
  stored canonical `Root` object or a second root identity; a root is a typed
  handle to a directory object ID.
- Implement one typed `ObjectId` digest over the supplied canonical bytes.
- Implement object decoding with checked lengths, bounds, strong-edge validation,
  and exact EOF handling.
- Do not add flags, reserved fields, compatibility fields, generic registries,
  storage traits, or a final large-file manifest in this phase.
- Define typed errors for malformed bytes, invalid paths, type mismatches,
  identity mismatches, overflow, and unsupported operations.
- Keep backend values out of identity inputs. SQLite row IDs, APFS inode numbers,
  database page offsets, journal state, and host paths must never affect
  canonical bytes or IDs.

### 2. What to test

- Canonical encoding round trips to the same typed object.
- Same logical input produces byte-identical objects and IDs across runs.
- The envelope is exactly 9 bytes before the payload, has no version-like
  extension fields, and rejects unsupported marker/kind bytes.
- Directory objects contain immediate names only, sorted by unsigned bytes;
  duplicate or descendant-path entries fail.
- Root handles resolve to directory object IDs without a stored root object.
- Fragmentation, read-buffer size, and storage backend do not change identity.
- Malformed headers, lengths, paths, edges, trailing bytes, and wrong object
  kinds fail with the correct typed error.
- Checked arithmetic rejects oversized lengths and counts without partial state
  mutation.
- Canonical output is identical on APFS case-sensitive and case-insensitive
  volumes when the same canonical input is supplied.
- Core tests run without SQLite, APFS APIs, or filesystem-specific imports.
- Run the Phase 1 microbenchmark in [`eval.md`](eval.md) for representative
  byte objects, directory fan-outs, and short/maximal canonical paths.
- Record the benchmark source fingerprint, per-case timings, correctness, and
  an external peak-memory observation or an explicit unavailable status.

### 3. Phase 1 evidence and closure gate

The implementation evidence for Phase 1 is present on the current worktree:

- canonical paths and immediate names are bounded and deterministic;
- the fixed 9-byte `LFSO` envelope has only `Bytes` and `Directory` kinds;
- one typed `ObjectId` authenticates the supplied canonical bytes directly;
- streaming decode checks payload bounds before allocation and requires exact
  end-of-input; and
- root-path iteration, boundary limits, fragmented reads, malformed lengths,
  oversized names, and manually malformed directory ordering have direct
  regressions.

Phase 1 is not closed until the canonical-object baseline is also produced.
The baseline is intentionally smaller than the Phase 2 large-file evaluation:
it measures the bounded core primitive, not CDC, CAS, SQLite, materialization,
or small-edit scaling.

Verification evidence:

```text
cargo fmt --all -- --check                                  PASS
cargo test -p layerfs-core --offline                         PASS: 17 tests
cargo test --workspace --offline                             PASS: 22 unit tests
cargo clippy -p layerfs-core --offline --all-targets -- -D warnings
                                                             PASS
git diff --check                                            PASS
```

Required closure run:

```text
cargo build --release -p layerfs-eval
/usr/bin/time -l -o eval/phase1-<commit>/time.txt \
  target/release/layerfs-eval phase1 eval/phase1-<commit>
```

The closure artifact must contain `environment.json`, `results.jsonl`,
`summary.md`, and `time.txt`. Every result must be correct. `time.txt` is the
external RSS/maximum-resident-size observation; if the host cannot provide it,
the summary must say `unavailable` rather than report zero.

The Phase 1 performance contract is therefore:

- canonical work is linear in the supplied bounded input;
- the benchmark exercises the actual `Read`/`Write` and identity entry points;
- no benchmark case fails correctness;
- memory observations are explicit; and
- no Phase 1 claim is extended to large-file locality or high-concurrency
  process-wide memory bounds.

Large-file layout, tree-search locality, CDC/CAS small-edit scaling, SQLite
throughput, and cold/warm materialization benchmarks begin in Phase 2 after the
content-tree shape is selected; no Phase 1 format decision depends on them.

## Phase 2 — CDC, CAS semantics, and logical content

### 1. What to implement

- Implement the frozen CDC profile as a streaming scanner with bounded memory.
- Implement chunk identity and immutable CAS semantics in `layerfs-core`.
- Implement logical files as ordered chunk identities and lengths.
- Target production content as `File -> bounded immutable content tree ->
  Chunk IDs -> CAS`.
- Benchmark a flat manifest, a segmented layout, and a fixed-fanout content
  tree before selecting the large-file layout. A tree candidate uses bounded
  `ContentLeaf` nodes for chunk references and `ContentBranch` nodes for child
  references plus subtree byte lengths.
- Freeze the `File`, `ContentLeaf`, and `ContentBranch` encodings only after
  the benchmark selects the shape; Phase 1 does not carry this format risk.
- Implement streaming create and explicit full replace.
- Implement bounded range update and CDC rejoin verification.
- Reuse authenticated unchanged prefix and suffix chunks.
- Define the core-side object store ports needed by content reconstruction;
  keep their implementation out of `layerfs-core`.
- Track the counters required by eval: CDC bytes scanned, chunks reused,
  chunks created, bytes hashed, and bytes delivered.

### 2. What to test

- CDC vectors are deterministic and fragmentation-independent.
- Create and full replace stream source bytes without a source-sized buffer.
- Equal input reuses the same authenticated chunks and objects.
- A small range update verifies the rejoin before accepting the new sequence.
- Prefix and suffix chunks are reused without rereading their payloads when the
  proof permits reuse.
- Failed rejoin returns a typed bounded-resynchronization failure; it does not
  silently claim edit-sized work.
- Run the core portion of B6, B7, and B8 using an in-memory test port.
- For B7, verify that a one-byte edit on 16 MiB, 100 MiB, and 512 MiB reports
  its actual scan and reuse counters before SQLite is introduced.

## Phase 3 — Copy-on-write trees and deltas

### 1. What to implement

- Implement immutable directory and file tree nodes.
- Implement `cow/` views and copy-on-write mutation.
- Reuse unchanged files, directories, chunks, and subtrees by identity.
- Recreate only changed nodes and affected ancestor spines.
- Implement parent-root plus delta representation.
- Represent additions, removals, replacements, and metadata changes as bounded
  canonical delta entries.
- Keep mutable workspace state separate from immutable root identity.

### 2. What to test

- A one-file mutation creates a new root while the parent root remains usable
  and byte-identical.
- Unchanged sibling subtrees retain their identities.
- Add, remove, replace, rename, and metadata changes produce exact deltas.
- Applying a delta to its parent reconstructs the expected child root.
- Replaying a delta twice is rejected or remains explicitly idempotent according
  to the chosen contract; it must not corrupt the root.
- Parent, child, and unrelated workspace roots cannot be silently retargeted.
- Run B7 and B8 through the core tree and delta path, still using a test store.
- Fail the phase if a one-byte edit rebuilds an entire unchanged directory tree
  or materializes all unchanged file payloads.

## Phase 4 — SQLite storage engine

### 1. What to implement

- Implement `layerfs-engine` with SQLite as the first backend.
- Define tables for immutable objects, root/checkpoint metadata, deltas,
  path/index metadata, and store metadata. Root rows are engine metadata around
  typed directory handles, not canonical `Root` objects.
- Use parameterized prepared statements for hot operations.
- Implement no-replace object insertion and authenticated incumbent reuse.
- Implement exact object range reads.
- Implement root and delta lookup.
- Implement one atomic capture transaction:

  ```text
  durable objects → durable delta → visible root
  ```

- Configure rollback-journal mode with WAL disabled.
- Configure and record the selected SQLite synchronization and macOS durability
  settings, including any `fullfsync` choice.
- Map SQLite busy, constraint, I/O, corruption, quota, and transaction errors
  into LayerFS typed errors.
- Keep SQLite row IDs and schema details outside core types and canonical IDs.
- Keep all database files and temporary journal files on the measured APFS
  volume for the first qualification composition.

### 2. What to test

- Put/get an object and read exact ranges from it.
- Insert the same authenticated object twice without replacing it.
- Reject an unequal occupant for an existing object identity.
- Reject malformed or corrupted stored object bytes before reuse.
- Commit objects, a delta, and a root atomically.
- Inject a failure before object publication, before delta publication, and
  before root publication; reopen SQLite and verify no partial root is visible.
- Verify a failed transaction does not advance the parent/root head.
- Test concurrent readers and the supported single-writer policy on APFS.
- Count actual SQLite transactions and statements.
- Run B4, B5, and B6 with the SQLite engine and record the first baseline.
- Measure database growth, rollback-journal growth, engine bytes read/written,
  and peak temporary storage.

## Phase 5 — OS adapter and native directory projection

### 1. What to implement

- Implement `layerfs-os` for the qualified macOS/APFS filesystem operations.
- Implement `layerfs-vfs::materialize` for a normal macOS directory.
- Materialize files by streaming or bounded range reads from the engine.
- Materialize directories in canonical order.
- Track destination provenance sufficiently to distinguish a matching,
  incrementally refreshable, replaced, or unknown destination.
- Implement cold materialization into an empty directory.
- Implement warm no-op materialization without rewriting unchanged files.
- Implement incremental materialization from a known parent root to a child root.
- Use safe temporary files and atomic replacement for individual file updates.
- Keep APFS-specific file operations inside `layerfs-os::macos`.
- Do not use APFS `clonefile` in the correctness path yet.

### 2. What to test

- Run B1 and compare the complete materialized tree with the expected tree.
- Run B2 and assert zero unchanged-file rewrites.
- Run B3 and assert only changed paths and affected metadata are updated.
- Verify large files are not staged in a source-sized userspace buffer.
- Test destination file replacement, deletion, directory creation, and empty
  directories.
- Test a destination with changed bytes, missing files, extra files, wrong
  file kinds, and changed metadata.
- Test symlink and dangling-symlink behavior explicitly on APFS.
- Test both case-sensitive and case-insensitive APFS behavior if available;
  canonical path semantics must not rely on volume case behavior.
- Measure B1/B2/B3 cold, warm, and reopened states using the precise labels in
  `eval.md`.
- Test interruption or failure during a file replacement and verify that the
  LayerFS root remains unchanged.

## Phase 6 — Capture projection and DeltaGit workflow

### 1. What to implement

- Implement the materialized-workspace handle.
- Implement changed-path and changed-range evidence from the ordinary directory.
- Freeze the evidence before capture begins.
- Revalidate changed paths and file identity before reading content.
- Route changed content through core CDC, CAS identity, COW, and delta logic.
- Write new objects, the delta, and the new root in one engine transaction.
- Advance the workspace head only after the root is durably visible.
- Return an opaque checkpoint and bounded change summary.
- Implement explicit discard.
- Ensure `Drop` is not the only mandatory cleanup path.

### 2. What to test

- Run B7 across 16 MiB, 100 MiB, and 512 MiB files.
- Run B8 for equal-length replacement, prepend, append, truncate, and EOF edit.
- Run B9 with 50 edits in the development loop and 500 edits in the checkpoint
  loop.
- Run B10 for repeated checkpoint storage growth.
- Materialize every captured root and compare the result byte-for-byte with the
  expected final directory.
- Verify unchanged objects and chunks are reused.
- Verify capture does not scan or rewrite the complete file for a bounded edit.
- Verify a changed file with unavailable or ambiguous evidence fails closed.
- Inject failures during hashing, object insertion, delta write, root commit,
  cleanup, and workspace-head advancement.
- Reopen the engine after every injected failure and verify that no incomplete
  root or half-published workspace head is visible.
- Verify capture followed by discard publishes nothing.
- Make this phase a hard gate: do not begin SDK polish while B7 still shows
  linear full-file work.

## Phase 7 — Minimal SDK

### 1. What to implement

- Expose only:

  ```text
  LayerFs::open
  LayerFs::materialize
  MaterializedWorkspace::capture
  MaterializedWorkspace::discard
  ```

- Keep backend selection private or fixed to SQLite for the first release.
- Keep object IDs, SQLite types, CDC settings, SQL errors, and storage
  transactions out of the public namespace.
- Map internal failures into a small stable SDK error surface without losing
  important typed distinctions.
- Make workspace and checkpoint handles move-only where ownership matters.
- Document that Git or the caller edits the materialized directory directly.

### 2. What to test

- Run the complete DeltaGit workflow from the public SDK:

  ```text
  open → materialize → modify directory → capture → materialize new root
  ```

- Verify the public API cannot mutate a parent root.
- Verify capture and discard are mutually exclusive terminal actions.
- Verify a failed capture leaves the workspace usable or explicitly terminal
  according to the error contract.
- Verify raw backend details do not appear in public types or errors.
- Run the complete B1–B10 evaluation through the SDK.
- Run the public API tests on the actual macOS/APFS composition.

## Phase 8 — Optimization and backend qualification

### 1. What to implement

- Use the B1–B10 baseline to select one bottleneck at a time.
- Optimize only the measured owner:

  ```text
  CDC/CAS work       → layerfs-core
  SQL round trips    → layerfs-engine
  host filesystem I/O → layerfs-os
  file rewrites      → layerfs-vfs
  buffering/memory   → the owning boundary
  ```

- Consider APFS `clonefile` or other same-volume optimizations only after the
  normal path is correct and its benefit is measured.
- Keep APFS optimizations optional and non-canonical.
- Add a PostgreSQL backend only if shared or remote storage is a real
  requirement or SQLite becomes a measured bottleneck.
- Treat the recovered custom engine as a backend candidate, not as a reason to
  move persistence policy back into `layerfs-core`.

### 2. What to test

- Rerun the unchanged correctness suite after each optimization.
- Rerun the affected benchmark case and one clean sibling case.
- Compare APFS normal-copy materialization with any APFS clone optimization
  under the same-volume and cross-volume conditions.
- Verify unsupported APFS operations fail with typed results and do not trigger
  hidden copy, retry, or fallback behavior.
- Verify optimized and baseline implementations produce identical canonical
  bytes, object IDs, roots, deltas, and final materialized bytes.
- Run the complete B1–B10 matrix at a stable source fingerprint.
- Report median, p95, CPU, memory, engine counters, storage growth, and exact
  environment for every claimed improvement.

## Phase completion rule

A phase is complete only when both sections have evidence. Compilation alone is
not phase completion.

The first performance stop condition is Phase 6:

```text
If B7 is O(file size), stop and redesign capture/CAS/CDC locality.
Do not continue to SDK polish or PostgreSQL work.
```

The first backend stop condition is Phase 4:

```text
If SQLite transaction or statement counts dominate, fix the engine boundary
before tuning CDC or claiming Rust throughput.
```

The final evaluation is a regression checkpoint, not the first performance
investigation.
