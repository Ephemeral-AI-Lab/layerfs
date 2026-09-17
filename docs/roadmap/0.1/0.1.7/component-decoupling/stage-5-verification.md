# Stage 5 verification contract

> **Status:** Corrected 2026-09-17. This document was published as a frozen
> campaign while §4 explicitly left performance gates unfrozen; those are two
> different claims and the banner was wrong. The **correctness, canonical and
> resource** sections below are frozen as written. The **measurement** sections
> (§2 sizes, §3 timing boundaries as applied to qualification, §4 gates and §5)
> are superseded for new rows by the
> [dated prospective addendum](stage-5-verification-addendum-20260917.md), which
> names the paired entry points, exclusions, cache state, sample policy, resource
> limits and numerical gates before any qualified candidate result was collected.
> Every result recorded before that addendum is preserved unchanged and is
> labelled a diagnostic, not qualification evidence.
>
> Companion to the [Stage 5 handoff](stage-5-handoff.md) and the
> [filesystem contract](filesystem-tree.md). It states what a measurement must
> satisfy *before* any collection happens.

## 1. Scope and seals

| Field | Frozen value |
| --- | --- |
| Source identity | the commit that carries this file (`git rev-parse HEAD`), clean tree |
| Reference oracle | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` (root workspace only) |
| Product under measurement | `core/` workspace: `layerfs-content` (C1), `layerfs-storage` (C2) |
| Write profile | compact scoped-inline; `layerfs/namespace-profile/scoped-inline/v1` |
| Toolchain | `cargo +1.85.1`, `--locked` |
| Harness | `core/crates/layerfs-content/examples/filesystem_timing_c1`, `core/crates/layerfs-storage/examples/measure_filesystem` |
| Storage | real SQLite MEMORY-journal Store, `StoragePolicy::frozen_default()` |
| Ordering backing | caller-owned `FileBacking` in the fresh `--output` directory |
| Worker policy | one producer: the C1 operation and the C2 save are single-threaded |

A rebuilt artifact invalidates a matched arm. A changed harness invalidates the
pair. The reference workspace is never linked into the measured product.

## 2. Families, cases and sizes

Case IDs are stable; sizes are realized physically, not nominal.

| Family | Case | Realized input | Compared arm |
| --- | --- | --- | --- |
| `filesystem.initial` | `empty` | one root directory, one inode | reference `build_initial_namespace` |
| `filesystem.initial` | `small` | 1 directory, 4 inodes | reference initial build |
| `filesystem.initial` | `wide` | 303 inodes, 6 inode leaves + 1 branch, 2 directories | reference initial build |
| `filesystem.initial` | `deep` | 8 directory levels, 24 inodes | reference initial build |
| `filesystem.update` | `noop` | wide base, zero property changes | reference update route |
| `filesystem.update` | `rename` | wide base, one rename | reference update route |
| `filesystem.update` | `remove` | wide base, one unlink | reference update route |
| `filesystem.update` | `create` | wide base, one new inode | reference update route |
| `filesystem.update` | `move` | wide base, cross-directory move | reference update route |
| `filesystem.update` | `subtree-remove` | wide base, one directory with 100 children released | reference update route |
| `filesystem.attributes` | `portable` | mode + mtime | reference `build_metadata_tree` |
| `filesystem.attributes` | `generic` | 24 opaque generic keys, patch one key | none: documented acceptance change |
| `filesystem.attributes` | `wide` | 60 `apple.xattr`-spelled keys | reference `build_metadata_tree` |
| `filesystem.bounds` | `paged-read` | 60-name directory listed at 4 names / 8 KiB per wave | none |
| `filesystem.bounds` | `tiny-budget` | 40-row directory at 64 B … 4 MiB scratch | none |
| `storage.pipeline` | `save-reopen` | one 5-inode tree with pooled leaves, attributes and a symlink | none |
| `storage.pipeline` | `patch-reopen` | one attribute patch over a reopened Store | none |

Fixed inputs: scope `scope_for_seed([0x5a; 32])` (reference cases) or
`[0x77; 32]` (pipeline), serials `1..=n` in ascending order, names
`entry-NNN`/`fNNNN`/`kNN`, and synthetic content/attribute roots
`layerfs/stage5-fixture/<label>`. A normalized change set is the strictly sorted
final binding list plus the strictly sorted typed value list; both sides receive
it byte for byte.

## 3. Timing, acknowledgement and cache boundaries

```text
filesystem.update
  validate                checked inputs, allocator precondition, effective topology
  directories             one sorted merge per affected directory
  references              reducer finish: base waves + final typed rows
  inodes                  one sorted inode-table merge
  root.encode             root framing and emission
storage:                  admission / pooling / packing / SQL / finish (C2's own scopes)
readback                  labelled separately, never inside the operation timer
```

- A `--mode pipeline` row measures construction, the bounded handoff and the
  final save acknowledgement in one region; read-back is reported separately.
- A `--mode c2` row receives objects prepared outside the timed region and
  measures admission only; it includes no filesystem construction.
- A `--mode c1` row opens no database: `measure_filesystem` creates its Store
  only inside the C2 and integrated modes, and `filesystem_timing_c1` has no
  storage crate in its dependency graph at all. (Corrected 2026-09-17: the harness
  used to create the Store before its mode dispatch, so a C1 row opened a database
  it never used.)
- Scope names come from the run's own report. A report that clips marks itself
  incomplete; clipped rows are `INCOMPLETE`, never `PASS`.
- Cache state is `warm-process-in-memory-fixtures`: every case's fixture objects
  are created in-process and the Store lives in a fresh directory. No cold-OS
  claim is made, because none of these families reads a prepared on-disk master.
- One sample per case per arm. Diagnostics are labelled diagnostics. A failing
  row stays in the report with its measured wall time.

## 4. Numerical gates (correctness first, then resources)

1. **Canonical identity gate.** For every reference case the replacement's root
   identity must equal the sealed reference identity, and the complete reachable
   object set with its page shapes must equal the sealed set. Binary outcome.
2. **Read-back gate.** Every sealed final record must read back with the same
   kind, derived reference count, content root and attribute root; every sealed
   removal must be absent. Binary outcome.
3. **Boundary gate.** No emitted page may exceed 8,192 canonical bytes; every
   non-root page must satisfy its fill rule (inode leaf ≥ 50 rows, inode branch
   ≥ 64, directory/attribute page ≥ 3,277 bytes).
4. **Resource gate.** `peak_scratch_bytes ≤ scratch_bytes` for both merges, and
   `rows_spilled == 0` whenever `maximum_pending_records` exceeds the touched
   row count.
5. **Composition gate.** A saved tree must reopen with the same root, listing,
   counts, symlink target and portable/generic attribute values.

Performance gates were **not** frozen here. No latency, throughput or memory
superiority is claimed by this document: the owner waivers recorded for #168/#169
do not waive #170. The comparative arm this section said had not been built now
exists as a component comparison; its contract and collected rows live in the
[addendum](stage-5-verification-addendum-20260917.md), which is the document that
governs qualification rows. This section keeps its original text as the record of
what was frozen when it was written.

## 5. What is not yet measured

- No reference-versus-replacement comparative timing row exists. The reference
  side would have to run the equivalent Workspace-shaped operation; the native
  preordered C1 route is deliberately not comparable with the old complete
  pipeline, so no ratio is reported.
- No cold-cache, pack-footprint or simultaneous-memory row was collected.
- `filesystem_ordering`, `filesystem_failure`, `filesystem_bounds` (C1) and the
  C2 `filesystem_failure` target are defined in the file plan but not written;
  their criteria stay open and are listed in the
  [completion report](stage-5-report.md#8-open-criteria-and-blockers).
- A row may be `NOT_RUN` with its reason; a missing number is unavailable, never
  zero.

**Correction, 2026-09-17 (this section kept as written).** Two bullets above are
out of date and are not the current state: the comparative row now exists as the
component comparison in the [addendum](stage-5-verification-addendum-20260917.md),
and the four targets listed as "not written" are now
`filesystem_ordering` (8 cases), `filesystem_failure` (5), `filesystem_bounds` (4)
and C2 `filesystem_failure` (4). The open list is now only the independent review
and the complete-operation comparison, which the addendum's §6 keeps `NOT_RUN` with
its source-backed reason.

## 6. Evidence shape

```text
stage5_smoke="$(mktemp -d /tmp/layerfs-stage5.XXXXXX)"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-content \
  --example filesystem_timing_c1 -- --case hardlink-move --output "$stage5_smoke/c1"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage \
  --example measure_filesystem -- --mode c2 --case inode-update --output "$stage5_smoke/c2"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage \
  --example measure_filesystem -- --mode pipeline --case subtree-remove --output "$stage5_smoke/pipeline"
```

Each run prints its case, root, actual counters, `elapsed_ns` and the coarse
phase timings; `measure_filesystem --mode c2` also prints the admitted record
split. Receipts are appended, never rewritten.
