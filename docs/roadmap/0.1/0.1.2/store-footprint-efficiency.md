# Store footprint efficiency

> **Status:** Implemented and in baseline/candidate measurement: 3 controls,
> each measured with 3 fresh Stores per unchanged baseline and retained
> candidate. Interrupted or diagnostic evidence is not a frozen v0.1.2
> baseline.
> Tracked by [GitHub issue #16](https://github.com/Ephemeral-AI-Lab/layerfs/issues/16).

## Problem statement

The current 100,000-file mixed-size namespace fixture contains exactly
500,000,000 logical file bytes, but one retained metadata-dedup-friendly sample
produced approximately:

```text
500.0 MB logical file bytes
542.9 MB unique canonical object bytes
660,865,024 bytes total SQLite Store growth
422,071 object rows
```

That is approximately `1.086x` canonical amplification and `1.322x` durable
Store amplification. An earlier fixture with generation-time metadata reached
approximately 732.5 MB (`1.465x`), so fixture metadata cardinality materially
changes the result. Neither sample is a v0.1.2 baseline: their fixture identity
and metadata profiles differ, and the active v0.1.1 optimization candidate is
not a released comparison point.

The retained Store census also shows where the current space goes:

```text
objects rowid table       about 642.9 MB
  payload                 about 558.1 MB
  unused page space       about  81.8 MB
ObjectId primary-key index about 18.8 MB
```

About 88.8 percent of object rows are smaller than 128 bytes. Large content
objects own most canonical bytes, while hundreds of thousands of small
structural, inode, metadata, and file-state rows own much of the row, index,
and page-layout overhead.

The logical payload is deterministic, unique, and intentionally resistant to
compression. Exact 500 MB total storage is therefore not a valid target: the
500 MB payload is already the irreducible input, before canonical structure,
ObjectIds, authentication, and database indexing. The product question is how
close LayerFS can move toward that floor without hiding bytes in another file,
changing canonical identity, or trading storage for CPU, memory, or read/write
latency.

## Goal

Measure, explain, and minimize **total durable Store footprint** for the
500,000,000-byte unique-content namespace control while preserving the released
v0.1.x compatibility surface.

Keep the family under the existing benchmark with one pure control-definition
file and one runner:

```text
benchmark/fs-bench-pro/families/store_footprint.rs
benchmark/fs-bench-pro/run-store-footprint.sh
```

The runner requires an explicit control unless `--all` is supplied. Its default
development mode measures one selected control; exact reopen/census validation
and full three-Store admission remain explicit modes defined by the
[`fs-bench-pro` v0.1.2 format](fs-bench-pro-format.md).

Use these decision targets:

| Level | Total durable Store | Amplification | Meaning |
| --- | ---: | ---: | --- |
| v0.1.2 admission | at most 600 MB | at most `1.20x` | Patch-compatible layout or admission improvement worth retaining. |
| Preferred | at most 590 MB | at most `1.18x` | Strong patch-compatible result above the preserved-schema lower marker. |
| Stretch | at most 580 MB | at most `1.16x` | Near the preserved rowid-schema lower marker with every byte counted. |
| Invalid target | exactly 500 MB | `1.00x` | Omits unavoidable canonical and index ownership for unique incompressible bytes. |

The preferred and stretch targets do not authorize a Store-schema, page-size,
canonical, CDC, identity, SDK, CLI, daemon, or wire change in a patch release.
If an incompatible mechanism is necessary, v0.1.2 owns the evidence and
admission decision; implementation moves to the first compatible minor release.

## Footprint accounting

`total_durable_store_bytes` includes every persistent Store-owned byte:

```text
store.sqlite
+ WAL or rollback journal retained at measurement time
+ durable object packs
+ location indexes
+ manifests and checksums
+ persistent sidecars
= total durable Store footprint
```

Moving payload out of `store.sqlite` does not reduce total footprint unless the
new file is included. The report also retains, but does not add to the final
durable total:

```text
peak temporary spool bytes
temporary bytes written and read
process and cgroup page-cache ownership
peak on-disk bytes during construction
cleanup residue after reconnect
```

Every sample reports at least:

| Metric | Equation or source |
| --- | --- |
| `logical_bytes` | Exact fixture manifest. |
| `canonical_objects` | Exact authenticated Store census. |
| `canonical_bytes` | Sum of unique canonical object byte lengths. |
| `sqlite_database_bytes` | SQLite file size after clean reconnect. |
| `other_durable_store_bytes` | Sum of every other persistent Store-owned file. |
| `total_durable_store_bytes` | SQLite plus all other durable Store bytes. |
| `canonical_amplification` | `canonical_bytes / logical_bytes`. |
| `durable_amplification` | `total_durable_store_bytes / logical_bytes`. |
| table/index payload and unused bytes | SQLite `dbstat` or an equivalent retained raw census. |
| temporary and physical I/O | Operation-site counters plus process/cgroup evidence. |

Unavailable fields are evidence errors, never inferred zeros.

## Fixture and controls

Use the v0.1.1 namespace distribution only after its replacement fixture,
digest, mode, and mtime contract is versioned and frozen. Do not compare or
pool historical rows that used different metadata semantics under the same
profile name.

The storage family has three non-interchangeable controls:

1. **`store-footprint-unique-100000`:** 100,000 files, 1,000 data directories,
   500,000,000 logical bytes, fully materialized path-derived content, and the
   frozen namespace metadata profile.
2. **`store-footprint-metadata-cardinality-100000`:** identical paths, sizes, and contents with
   deterministic path-derived or bucketed mtimes. This prevents one uniform
   timestamp from becoming an unlabelled best-case dedup shortcut.
3. **`store-footprint-large-object-500m`:** the same logical byte budget concentrated into a
   small file count. This separates SQLite BLOB/page layout from namespace row
   cardinality.

The controls form one complete storage-evidence family in the existing LayerFS
benchmark environment. They do not enter `registered_total_ns`, replace the
unique-content namespace performance row, or weaken exact FUSE reopen
verification.

The `600 MB` admission target applies to the primary
`store-footprint-unique-100000` question. Metadata-cardinality and large-object
rows are explanatory controls: they must retain complete accounting, exact
identity, baseline-relative performance/resource gates, and no Store-footprint
regression, but their deliberately different canonical object sets are not
individually compared with the primary control's `600 MB` limit.

## Required execution order

### 1. Freeze a fair baseline

- Finish the owning v0.1.1 namespace fixture and profile correction.
- Run the unoptimized v0.1.2 comparison source and every candidate against the
  exact same fixture, Store creation path, cache condition, and container.
- Retain three fresh Stores per control and preserve every valid result.
- Record logical, canonical, SQLite, index, unused-page, temporary, CPU, RSS,
  I/O, and exact reopen evidence.
- Establish whether insertion order, metadata cardinality, or object-size
  distribution explains the retained page slack before changing storage code.

### 2. Test patch-compatible admission order

Reuse the existing bounded admission batch. Test only deterministic orderings
that change no canonical bytes, object membership, batch bound, transaction
bound, schema, page size, or public behavior:

```text
current generation order
ObjectId order within one bounded batch
encoded-length order within one bounded batch
```

The ObjectId candidate tests primary-key locality. The encoded-length candidate
tests row-table packing for mixed BLOB sizes. Do not globally sort all objects,
add a complete index, or add an external spool merely to reorder admission.

Retain a candidate only when total durable footprint improves outside noise.
Initialization, Commit, and exact reopen target at most `1.05x` baseline and
may use the global tolerated band through `1.10x` with explicit disposition;
anything above `1.10x` is no-go.

### 3. Measure SQLite layout alternatives without admitting them prematurely

Create disposable evidence Stores for page-size and table-layout experiments:

```text
SQLite page size: 4, 8, 16, 32, and 64 KiB
rowid table plus ObjectId index
WITHOUT ROWID object table
```

Record final footprint, table/index payload and unused bytes, page count,
initialization, point and batch reads, exact reopen, CPU, RSS, page cache, and
physical I/O. Do not mutate or migrate user Stores during this experiment.

LayerFS currently validates the released page size and five-table schema. A
candidate that changes either is incompatible with the v0.1.x patch boundary
unless the existing format remains fully readable and writable without
changing its identity or guarantees. Record such a result, but do not ship it
as v0.1.2 production behavior.

### 4. Decide whether physical packing is necessary

Consider canonical-preserving object packs only if bounded order and compatible
SQLite layout work cannot reach the preferred target. A pack design must keep
the exact canonical bytes and ObjectIds while replacing per-object payload rows
with bounded pack locations.

Count the pack file, location index, manifest, checksums, journals, and cleanup
state in total durable footprint. Prove exact ObjectId authentication on every
read and failure-atomic publication before accepting its measurement.

Physical object packing is not a v0.1.x production change. If measurement
admits it, create a focused minor-release issue rather than weakening the
v0.1.2 compatibility contract.

### 5. Consider structural-pack compression last

The fixture payload is intentionally incompressible. Do not spend CPU trying to
compress large content objects. Compression is eligible only for separately
identified structural packs and only after physical packing is independently
admitted.

Retain compression only when total durable bytes improve materially while
initialization, exact reopen, CPU, RSS, page cache, and physical I/O remain
within their gates. Background or post-return compression is forbidden.

## Compatibility boundary

Patch-compatible candidates may change bounded insertion order or another
internal mechanism that leaves all of these unchanged:

- the released five-table Store schema and accepted Store files;
- SQLite page-size compatibility;
- canonical bytes, ObjectIds, roots, and CDC behavior;
- public Rust SDK, CLI, daemon, proxy, and FUSE contracts;
- visibility, acknowledgement, durability, collision, and integrity behavior;
- exact materialization and fresh-reopen results.

`WITHOUT ROWID`, a new required page size, durable packs, structural
compression, canonical inlining, larger CDC chunks, or removal of a canonical
tree layer are evidence candidates, not pre-authorized patch changes.

## Anti-cheating and resource gates

Reject a candidate that saves reported SQLite bytes by:

- moving bytes into an uncounted pack, sidecar, journal, or source directory;
- retaining the import fixture as durable backing;
- using sparse files, reflinks, clones, repeated content, or compression in the
  unique-content control;
- omitting mode, mtime, file boundaries, integrity data, or exact authentication;
- weakening collision checks, Commit acknowledgement, or reconnect verification;
- shifting compaction before the timer, after the operation returns, or into a
  background worker;
- adding a product worker or raising a resource ceiling;
- increasing initialization, Commit, or exact reopen by more than five percent;
- increasing CPU, RSS, cgroup/page-cache memory, temporary bytes, or physical
  I/O materially without a separately admitted necessity;
- reporting only `store.sqlite` when other durable Store-owned files exist.

The selected result must retain zero swap, no OOM, deterministic cleanup, and
no residual pack, journal, spool, process, mount, Workspace, or Branch lease.

The runner freezes these additional envelopes before candidate collection:

- selected performance samples: hard 5-second supervision;
- admission performance samples: hard 30-second supervision;
- exact verifier phase: hard 60 seconds; its whole-process watchdog is 90
  seconds so the separately gated initialization/setup phase does not consume
  the verifier allowance; verifier processes remain 180 seconds aggregate;
- CPU, process RSS/physical footprint, cgroup peak, temporary bytes, physical
  I/O, and complete lifecycle: candidate at most `1.10x` the exact baseline;
- primary storage: at most `600,000,000` durable bytes;
- explanatory-control storage: no increase from the exact baseline.

Baseline/candidate custody requires identical host, Docker, harness, workload,
CDC, fixture, cache, Store-creation, SQLite schema/page-size, canonical object
shape, and object/byte counts. Before each full arm, an untimed exact tree digest
of every shared fixture-schema-keyed sealed fixture provides equal explicit source-cache
preconditioning. Each fresh Store retains and authenticates its exact canonical
root and object-set digest through reconnect. The ordinary public path seeds
inode identity from a fresh UUIDv7 LayerStackId; Store-footprint evidence uses a
diagnostic-only deterministic initialization seed equal to the fixture digest,
making roots and object sets exactly replayable without changing ordinary
product behavior. v3 also retains the semantic tree digest and the
tag/encoded-length/count object-shape digest. The only admitted product
diff is the measured bounded encoded-length ordering in object admission.
Source-arm and seed labels never enter Store entity metadata. The exact verifier
streams lexicographically ordered records in bounded waves of at most 16
directories instead of retaining all 100,000 paths and records.

A failed full admission can be replaced only after a source, harness, workload,
or frozen-environment correction that directly addresses its retained failure,
or after a prospectively defined invalid-host condition is independently
proven. An unchanged rerun cannot supersede a valid failure. Every retry keeps
the failed evidence immutable and first passes the affected selected case.

## Admission decisions

Classify each mechanism independently:

- **accept:** compatible, exact, and improves total durable footprint while
  passing every performance and resource gate;
- **defer:** evidence supports the mechanism, but it requires an incompatible
  Store, page-size, canonical, or physical-format change;
- **reject:** savings are within noise, accounting is incomplete, or the
  mechanism purchases storage with unacceptable CPU, memory, I/O, latency, or
  correctness cost.

The retained sample's canonical bytes, 32-byte ObjectIds, and measured primary-
key index already total about 575.1 MB before row headers and B-tree slack.
Targets below that marker are incompatible with the preserved rowid schema.

v0.1.2 may complete this workstream with a measured `defer` or `reject`. It
must not invent a patch-compatible implementation merely to claim the 580 MB
stretch target.

## Files to read

- [v0.1.2 scope](README.md)
- [0.1.x roadmap](../README.md)
- [0.1.x benchmark contract](../benchmarking.md)
- [v0.1.1 namespace optimization specification](../0.1.1/namespace-optimization-spec.md)
- [v0.1.3 namespace and deduplication contract](../0.1.3/namespace-initialization-scale.md)
- [`fs-bench-pro` namespace runner](../../../../benchmark/fs-bench-pro/run-namespace.sh)
- [`fs-bench-pro` namespace harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Store schema](../../../../crates/layerfs-layerstack-store/src/schema.rs)
- [Store object admission](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Store SQL statements](../../../../crates/layerfs-layerstack-store/src/statements.rs)
- [Canonical object contract](../../../../crates/layerfs-content/src/object/canonical.rs)
- [Object identity contract](../../../../crates/layerfs-content/src/object/id.rs)
- [CDC profile](../../../../crates/layerfs-content/src/file/cdc/mod.rs)
- [Retained terminal namespace report](../../../../benchmark-results/fs-bench-pro/namespace/v011-rc-terminal-all4-r001-20260903/report.md)

## Acceptance criteria

- [ ] The owning namespace fixture, digest, and metadata profiles are versioned
  and frozen before the v0.1.2 baseline.
- [ ] Every comparison uses the same source, fixture, container, cache profile,
  Store creation path, sample count, and exact verification oracle.
- [ ] Three fresh Stores are retained for every admitted control and candidate.
- [ ] Reports distinguish logical, canonical, SQLite, other durable, total
  durable, temporary, and physical I/O bytes.
- [ ] `dbstat` or equivalent raw evidence explains table, index, payload, and
  unused-page ownership.
- [ ] Bounded admission-order candidates are tested before any Store-format
  prototype.
- [ ] The selected patch-compatible result reaches at most 600 MB total durable
  Store footprint, or the exact measured blocker is retained.
- [ ] Preferred and stretch results count every durable Store-owned byte and do
  not rely on hidden files or background work.
- [ ] Exact canonical bytes, ObjectIds, roots, collision behavior, Commit,
  materialization, FUSE, fresh reopen, and cleanup proofs pass.
- [ ] Initialization, localized Commit, and exact reopen meet the `1.05x`
  target or explicitly disposed `1.10x` tolerated band; CPU, RSS, page cache,
  temporary bytes, and physical I/O satisfy the retained resource envelope.
- [ ] No Store-footprint candidate alters the Store schema, required page size,
  canonical format, CDC profile, identity, public API, or daemon/proxy contract.
- [ ] Every incompatible but evidence-backed mechanism receives a focused
  minor-release handoff with retained commands, identities, measurements, and
  acceptance criteria.
- [ ] The final v0.1.2 report states accept, defer, or reject for bounded order,
  page-size layout, `WITHOUT ROWID`, physical packing, and structural
  compression independently.

## Stop conditions

Stop the v0.1.2 implementation path and retain evidence when:

- the next step requires a Store-schema, page-size, canonical, CDC, identity,
  SDK/CLI, daemon, proxy, or FUSE contract change;
- total footprint cannot be computed because a durable or temporary Store file
  is not under custody;
- a storage win depends on missing CPU, RSS, page-cache, or physical-I/O
  evidence;
- exact reopen or object authentication would be weakened;
- a proposal needs global in-memory object state, an extra worker, unbounded
  compaction, or background work;
- a fixture change would make historical evidence appear faster or smaller
  without a new profile identity.
