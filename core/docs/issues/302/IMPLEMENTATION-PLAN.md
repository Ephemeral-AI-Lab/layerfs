# #302 Phase 7 cluster 1 (storage): implementation plan

> **Status:** Current planning checklist; no release candidate exists.
> Planning-agent output, 2026-10-03. Issue
> [#302](https://github.com/Ephemeral-AI-Lab/layerfs/issues/302), parent
> [#301](https://github.com/Ephemeral-AI-Lab/layerfs/issues/301). Source base
> `7edddbdb8e8512627aed0ed42533ef099d802384` (branch
> `codex/phase7-cluster1-storage`). This document changes no product code, runs
> no benchmark and claims no measurement, qualification or release admission.
> Every number below is quoted from a named file or computed from such numbers
> with the arithmetic shown; everything else is marked **unknown**.

Inputs read: the uncommitted design packet at
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/301/` (README, 01,
03–06), the amended `core/AGENTS.md` in the same checkout, root `AGENTS.md`,
`core/benchmark/fs-bench-pro/AGENTS.md`, `docs/general/benchmark_rules.md`, and
the source and `core/docs/issues/286/` evidence at the base commit. Paths without
a prefix are relative to `core/crates/`.

**Counter correction.** The prompt names `python3 core/tools/production_loc.py
--detail`. At the base that file has no `--detail` flag and skips `<crate>/sql/`
(it reports core 69,981 in 355 files). The counter the #286 reports and the
packet used is the repository-root `tools/production_loc.py`, which has
`--detail`/`--files`, counts shipped SQL, and reports **core 70,279 in 357 files,
reference 65,417 in 193 files, combined 135,696** at `7edddbdb8`. All production
line figures here are from `python3 tools/production_loc.py --files` at the base.

---

## 0. Architecture

### 0.1 Components

| Crate | Kind | Single responsibility | Must not know |
| --- | --- | --- | --- |
| `layerfs-content` (C1) | domain, unchanged | Canonical objects, CDC, file and filesystem construction, bounded reads | Any store, pack, locator, network |
| `layerfs-storage` (C2) | domain, reworked | Exact reuse, FULL/PREFIX/STORED selection, pack framing, which store a body goes to, bounded read and save waves | SQL, HTTP, sockets, PostgreSQL, S3, history records, Workspace, Commit |
| `layerfs-history` (C5) | domain, trait unchanged | History identities, records and the `HistoryCatalog` contract | Packs, locators, SQL (after retirement), Workspace |
| `layerfs-s3` | engine, new | Implements C2's `ObjectStore` against an S3-compatible endpoint | Pack format, object identities, roles, PostgreSQL, history |
| `layerfs-metadata` | engine, new | Owns the PostgreSQL schema; implements C2's `MetadataStore` and C5's `HistoryCatalog` | Pack contents, codecs, S3, Workspace Commit, conflict, edits |
| `layerfs-project` | library, new (D13) | Namespace Init/import: scan a directory, construct through C1, save through C2, initialize through C5 | Any engine, the bridge, the daemon, a sandbox, Commit |

Dependency edges (compile time): `storage → content, telemetry`; `history →
content`; `s3 → storage`; `metadata → storage, history, content`; `project →
content, storage, history, telemetry`. `metadata → content` is one edge more than
the packet's table; it carries identity types only. No cluster 1 crate depends
on a cluster 2 crate. Engines appear only in `[dev-dependencies]` (tests, examples) of cluster 1
crates; no cluster 1 product source names an engine.

### 0.2 Ports

`AuthenticatedObjects`/`FinalizedConsumer` (C1, `layerfs-content/src/object/`)
and `HistoryCatalog` (C5, `layerfs-history/src/catalog.rs`, 20 methods) are kept
exactly as they are. Two new ports are defined in `layerfs-storage/src/port/`.

**`ObjectStore`** — as packet 04 §2, unchanged: `put_if_absent(key, body) ->
Put{Created|AlreadyPresent}`, `read(key, Option<ByteRange>, out)`, `head(key) ->
Option<u64>`; `ObjectError{Missing, Refused{status}, Malformed, Uncertain}`.
`ObjectKey` is the SHA-256 digest of the pack bytes. C2 computes it when it
seals a pack and checks it on every whole-pack read, so integrity never depends
on the engine.

**`MetadataStore`** (D10) — seven operations, each one round trip and at most
one transaction:

| Operation | Unit | Bound |
| --- | --- | --- |
| `policy()` | Read the persisted storage profile | Once per handle |
| `locate(ids, out)` | Locators and pack descriptors (`pack_id`, domain, digest, length) for a set of identities; absent ones omitted | ≤ `READ_OBJECT_LIMIT` (4,096) ids per call |
| `read_packs(pack_ids, out)` | Bodies of metadata-domain packs | ≤ `DEPENDENCY_PACK_CACHE_BYTES` (4 MiB) per call |
| `value_groups(query, out)` | Pooled value-group catalogue rows for a set of ordinals or a page from a start ordinal, plus the retained-window start | ≤ one page |
| `signatures(out)` | The content-signature ring | ≤ 8,192 rows, once per handle |
| `reserve(packs, ordinals)` | Allocate a block of pack ids (sequence) and/or pooled ordinals (counter row) | One per block |
| `register(batch)` | Insert-if-absent, atomically: pack rows (metadata packs with body), object locators, value-group rows, signature rows, window advance, unused-ordinal release. Returns the identities that lost a first-wins race | ≤ `TRANSACTION_ROW_LIMIT` (8,191) rows, ≤ `TRANSACTION_CANONICAL_BYTES_LIMIT` |

`MetadataError` has the same four classes as `ObjectError`. `Uncertain` maps to
the existing `StorageError::UnknownOutcome`; nothing is resent.

### 0.3 Write path

```text
C1 FinalizedObject ─> Save::accept ─> pending batch (≤512 objects, ≤4 MiB−1)
wave:
  1. locate(wave ids ∪ direct references ∪ advisory predecessor ids)        1 round trip
  2. chain prefetch, level by level: locate(frontier) + fetch packs         ≤ 2 per chain level
  3. per object, unchanged logic: exact reuse (byte comparison) or
     reference check → FULL/PREFIX/STORED selection → lane group
  4. full groups → sealed packs held in the wave buffer (readable by this save)
  5. closure: seal every lane holding a pending member or pooled ordinal
     that a to-be-registered object references
  6. payload packs: put_if_absent (object store)                            1 request per pack
  7. register(batch)                                                        1 round trip
  8. objects reported lost: locate + byte comparison (reuse or Collision)   only on a race
finish: seal remaining groups, run 5–8 once more. No publish step.
```

Domain routing is a function of the lane (`pack/layout.rs::PackLane::for_role`):
Native, WholeFile and Singleton packs go to the object store; Ordinary and
PooledMetadata packs go to PostgreSQL as `bytea`.

### 0.4 Read path

`Reader` (C1's `AuthenticatedObjects`): bounded locator cache → `locate` for
misses → pack bytes from the wave buffer, the bounded pack cache, `read_packs`
(metadata) or `ObjectStore::read` (payload, whole pack, SHA-256 checked) →
existing decode and chain resolution → BLAKE3 identity check (unchanged). Ranged
reads are not built; the port carries the range parameter and a pack's group
directory sits at a fixed offset (`pack/layout.rs::body_area_offset`), so a later
reader can obtain group offsets with one small ranged GET.

### 0.5 Round trips and requests

Counts at the base, from the retained #286 Stores and receipts (read-only
queries on `…/issue286-phase-b/layerfs/benchmark-results/fs-bench-pro/<run>/…/sample.sqlite`
or `store.sqlite`, and `receipt.json::observed_counters`):

| Selection (run) | Baseline wall | Payload packs → PUTs | Metadata packs | Objects | Chain edges | Reuse occurrences |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Init 10,000 (r042) | 1.591 s raw call | 1,320 (155 whole-file + 1,165 native) | 25 | 24,763 | n/a | n/a |
| Init 100,000 (r041) | 6.043 s raw call | 2,192 (1,321 + 871) | 87 | 112,424 | n/a | n/a |
| stride 10 (r046) | 32.153 s driver | 632 (570 + 62) | 82 (41 ordinary + 41 pooled) | 51,689 | 68,246 | 1,105 |
| stride 3 (r046) | 52.446 s driver | 814 (691 + 123) | 174 (102 + 72) | 73,447 | 159,401 | 20,221 |
| stride 1 (r047) | 136.703 s driver | 1,256 (1,013 + 243) | 447 (275 + 172) | 104,618 | 456,067 | 113,164 |

What these decide:

- **Per-edge lookups cannot become round trips.** Today every chain step is a
  point query (`encoding/delta/select.rs:137`, `encoding/delta/read.rs:213`,
  `encoding/pool/read.rs:260`). At stride 1, 136,703 ms ÷ 456,067 edges =
  0.30 ms per edge would consume the whole baseline. Hence step 2 of the wave
  (level-wise batched `locate`) and the locator cache. Design target, to be
  measured: metadata round trips per wave ≤ 2·(chain levels) + 4.
- **Init cannot upload serially.** At Init 10,000, 1,591 ms ÷ 1,320 PUTs =
  1.2 ms per PUT would consume the whole baseline. The cost of one PUT against
  local MinIO is **unknown** until step 5 measures it. Init is the path allowed
  multiple workers, so its uploads use a bounded window (step 8); history and
  every other path upload on the one construction worker.
- **Reuse verification reads packs.** Each reuse reconstructs the stored object
  and compares bytes (`cas/membership.rs`). GETs caused by that at the existing
  4 MiB pack-cache bound are **unknown**; the diagnostic in step 9 counts them.

Counters every save and read reports (count-driven, labelled diagnostics):
metadata round trips by operation, object-store requests and bytes by operation,
locator-cache hits and misses, pack-cache hits and misses, forced seals.

### 0.6 Where this differs from the packet

| Packet | Plan | Why (evidence) |
| --- | --- | --- |
| 13 crates; Init home open (D13) | A 14th crate, `layerfs-project` | Init needs C2 and C5 together and reads a directory; C2 may not depend on C5, and domain crates do no I/O |
| Only `layerfs-daemon` names engines | Cluster 1 engines are named only by tests and examples; the host-side product composition root is an owner question (Q4) | The daemon runs inside a sandbox; Init must not need one |
| `saves` removed; a registered object is valid for everyone | True only with a new rule: a registration batch is closed under direct references (§0.3 step 5) | `cas/selection.rs::offer` accepts a reference that is only pending in memory (`cas/dependencies.rs::validate`), and `cas/save.rs` reuses an existing row without checking its references. Save-level publication is what makes that safe today |
| Pooled packs sealed per Commit (D12) | Sealed when full or when a registered leaf references their ordinals | Leaf bodies live in the Ordinary lane and name pooled ordinals (`cas/pool_lane.rs`); registering the leaf first would break the same closure |
| `store_policy` without its counters | The ordinal counter and retained-window fields stay | `sqlite/ownership.rs::{reserve_ordinals, release_ordinals, note_window}` are part of pooled encoding, not of visibility |
| `pack.digest` unique; a `pack_group` table; a dependency table | Digest not unique; no group table; no dependency column | Two writers sealing identical bytes are both valid rows. Extra rows cost allocation against a stride-10 headroom of 1,805,108 B (§3.3); group offsets stay reachable (§0.4); chain location is batched instead (§0.5) |
| Locator lookup ≤128 ids per round trip | One array-parameter statement, ≤4,096 ids | 128 is `LOOKUP_PAGE_IDS`, a SQLite statement-size bound |
| Old SQLite code becomes the old path's implementation of the new ports | Old path stays behind its own API; new path is added beside it; they share `encoding/` through one internal read seam | SQL types reach 12 of 15 `cas/` files and 6 `encoding/` files; the new ports have no save scope, ceiling or in-place append for the old code to implement |
| `workspace_stages` proposed for removal | `HistoryCatalog` implemented whole, stages included | Decided: the existing trait. The history acceptance driver calls `stage_changes`/`commit_staged` (`core/benchmark/fs-bench-pro-storage-content/src/ops/history_retained.rs`) |

### 0.7 Simplification ledger

Production lines are whole-file figures from `tools/production_loc.py --files`
at `7edddbdb8`. Where a mechanism is part of a file, the file total is given and
the part is **not separately measured**. A file can appear in more than one row;
the delete-set total below counts each file once.

| Mechanism removed | Why the new stores make it unnecessary | Files today (production lines) | Replaced by |
| --- | --- | --- | --- |
| Save slots, writer budget, per-file process arbitration | PostgreSQL takes concurrent writers | `sqlite/ownership.rs` (201); parts of `sqlite/schema.rs` (284), `cas/store.rs` (583), `cas/lifecycle.rs` (296) | Nothing |
| Save identity and publication visibility: `saves`, publication sequence, pack ceilings, temp read scope, `Unpublished`/`VisibilityCeiling` | Objects are registered only after their bytes are stored and only in reference-closed batches | `sqlite/ownership.rs` (201), `sqlite/lookup.rs` (142), `cas/read.rs` (172), `cas/collision.rs` (77), `sql/schema.sql` (62) | The closure rule (one function in `save/register.rs`); first-wins insert |
| Failed-save paged cleanup, quarantine | Nothing is deleted; orphans are harmless until reachability GC (deferred) | `sqlite/cleanup.rs` (69), `cas/finish.rs` (16), part of `cas/lifecycle.rs` (296) | Nothing |
| Appendable pack rows: zero-filled capacity, incremental BLOB writes, tail close, append ownership check | A pack is assembled once and never changes | part of `sqlite/write.rs` (214), `pack/placement.rs` (191), parts of `pack/layout.rs` (454: `pack_capacity`, `append_fits`) | `pack/assemble.rs::assemble` on a full lane, in `save/seal.rs` |
| Physical preallocation of the Store file | No local Store file | `sqlite/reservation.rs` (105); the `nix` dependency of `layerfs-storage` | Nothing |
| Hand-rolled pack-id watermark | PostgreSQL sequence | parts of `sqlite/ownership.rs` (201), `cas/lifecycle.rs` (296) | `reserve` |
| SQLite profile and tuning: pragmas, page size, schema identity, statement chunking by engine limits, `SaveConnectionProfile` | The server owns its storage profile | `sqlite/connection.rs` (97), `sqlite/schema.rs` (284), parts of `sqlite/write.rs` (214), `cas/store.rs` (583) | Schema bootstrap in `layerfs-metadata` (new code, not a saving) |
| Transaction cadence inside a save: `BEGIN IMMEDIATE`, `maybe_commit`, `wave_held`, `TransactionState` | One `register` call per wave | parts of `cas/lifecycle.rs` (296), `cas/owner.rs` (307), `cas/placement.rs` (300) | Nothing |
| SQLite-residue diagnostics: `DiagProfile`, per-field drop timing, `ReuseProbe` | They measure costs of the embedded Store | most of `cas/owner.rs` (307); part of `cas/store.rs` (583) | The counters in §0.5 |
| C5 SQLite provider | Provider rewritten for PostgreSQL | `layerfs-history/src/sqlite/` (1,824), `sql/schema-v1.sql` (156) | `layerfs-metadata/src/history/` — **replacement, not simplification** |
| Host service around C1/C2/C5 | No process owns a Store | `layerfs-server` 3,996, less 817 relocated to `layerfs-project` = 3,179 | Nothing. **Removed by cluster 2 (M9)**; cluster 1 only guarantees nothing here needs it |

Delete set owned by this cluster, each file once: `layerfs-storage` `cas/`
2,712 less `cas/batch.rs` 58 (moved, not deleted) + `sqlite/` 1,254 +
`sql/schema.sql` 62 + `pack/placement.rs` 191 = **4,161**; `layerfs-history`
`sqlite/` 1,824 + `sql/schema-v1.sql` 156 = **1,980**. The size of the code that
replaces them is not predicted. Until step 12 both paths exist, so production LOC
**rises** during coexistence; a simplification is claimed only when step 12 lands.
Init code moved from `layerfs-server` (`import/scan.rs` 247, `import/batch/` 88,
`import/namespace.rs` 307, `import/mod.rs` 3, `save/metadata.rs` 172 = 817) is
**relocation with adaptation**, and is duplicated until cluster 2 deletes the
server.

Deliberately kept, and why:

- Exact-reuse byte comparison and collision refusal — C2's deduplication behaviour is fixed.
- Direct-reference validation — it is what the closure rule builds on.
- Wave, batch, read and chain bounds in `policy.rs` — bounded memory windows are fixed.
- Pooled metadata lane, ordinals, retained window, content-signature ring — physical encoding is unchanged.
- Pack framing including the reserved directory region — a format change is out of scope.
- The bounded pack, group and pooled-value caches at their current sizes — changed only with count evidence.
- One attempt per operation and the `UnknownOutcome` class.
- `HistoryCatalog` and its records in full.

---

## 1. File and folder structure and environment

### 1.1 Crate tree

Every file under `core/crates/` for cluster 1 at the end state (after step 12).
`+` new, `~` changed, unmarked kept as is. Every production file stays under
1,000 physical lines and every `lib.rs`/`mod.rs` under 200; product code only
under `src/` and `sql/`, tests under `tests/`.

```text
core/crates/
│
├─ layerfs-content/                       C1 — unchanged, no file touched
│  ├─ Cargo.toml
│  └─ src/
│     ├─ lib.rs  error.rs  policy.rs
│     ├─ object/        access.rs  codec.rs  id.rs  inode_leaf.rs  output.rs  predecessor.rs  mod.rs
│     ├─ file/          content.rs  read.rs  view.rs  mod.rs
│     │  ├─ cdc/        gear.rs  mod.rs
│     │  ├─ edit/       apply.rs  compare.rs  concat.rs  finish.rs  input.rs  split.rs  tree.rs  mod.rs
│     │  └─ mapping/    build.rs  codec.rs  predecessor.rs  read.rs  types.rs  mod.rs
│     └─ filesystem/    identity.rs  input.rs  limits.rs  objects.rs  path.rs  read.rs  root.rs
│        │              symlink.rs  update.rs  validate.rs  mod.rs
│        ├─ attributes/ build.rs  codec.rs  keys.rs  patch.rs  portable.rs  read.rs  value.rs  mod.rs
│        ├─ directory/  codec.rs  read.rs  update.rs  mod.rs
│        ├─ inode/      codec.rs  read.rs  update.rs  mod.rs
│        ├─ references/ backing.rs  merge.rs  record.rs  reduce.rs  release.rs  runs.rs  mod.rs
│        ├─ rows/       check.rs  source.rs  spool.rs  update.rs  mod.rs
│        ├─ sorted/     budget.rs  finish.rs  format.rs  merge.rs  page.rs  mod.rs
│        └─ validate/   cycles.rs
│
├─ layerfs-storage/                       C2
│  ├─ Cargo.toml                        ~ + sha2 =0.10.9 (already locked); − rusqlite, − nix
│  ├─ README.md                         ~ profile and commands for the port-based path
│  ├─ src/
│  │  ├─ lib.rs                         ~ module list, re-exports
│  │  ├─ error.rs                       ~ port error mapping
│  │  ├─ policy.rs                      ~ bounds; lane → store routing
│  │  ├─ location.rs                    + ObjectLocation, PackInfo, ValueGroupRow, SignatureRow
│  │  ├─ storage.rs                     + Storage handle: two ports, policy, shared bounded indexes
│  │  ├─ port/
│  │  │  ├─ mod.rs                      + declarations
│  │  │  ├─ object_store.rs             + ObjectStore, ObjectKey, ByteRange, Put, ObjectError
│  │  │  └─ metadata_store.rs           + MetadataStore, Registration, Registered, Reserve, MetadataError
│  │  ├─ read/
│  │  │  ├─ mod.rs                      + declarations
│  │  │  ├─ fetch.rs                    + locator cache, pack cache, store routing, digest check
│  │  │  ├─ prefetch.rs                 + level-wise chain location and pack fetch for a wave
│  │  │  ├─ objects.rs                  + bounded read wave: reconstruct, authenticate, counters
│  │  │  └─ provider.rs                 + Reader: C1's AuthenticatedObjects over a Storage
│  │  ├─ save/
│  │  │  ├─ mod.rs                      + declarations
│  │  │  ├─ operation.rs                + Save: accept, finish, counters; SaveSink for C1
│  │  │  ├─ batch.rs                    ~ pending batch (moved from cas/batch.rs in step 12)
│  │  │  ├─ wave.rs                     + membership, exact reuse, collision, reference checks
│  │  │  ├─ select.rs                   + FULL/PREFIX/STORED selection glue
│  │  │  ├─ pooled.rs                   + pooled lane: ordinals, value groups, pooled bases
│  │  │  ├─ seal.rs                     + lane groups, sealed packs, wave buffer
│  │  │  └─ register.rs                 + reference closure, payload upload, registration, lost ids
│  │  ├─ encoding/
│  │  │  ├─ mod.rs  codec.rs  full.rs
│  │  │  ├─ decode.rs                   ~ location type import
│  │  │  ├─ delta/
│  │  │  │  ├─ mod.rs  record.rs
│  │  │  │  ├─ read.rs                  ~ reads through the read seam, not a Connection
│  │  │  │  ├─ select.rs                ~ reads through the read seam
│  │  │  │  └─ candidates.rs            ~ index logic only; row load/flush moves to callers
│  │  │  └─ pool/
│  │  │     ├─ mod.rs  counters.rs  delta.rs  leaf.rs  value_group.rs
│  │  │     ├─ read.rs                  ~ reads through the read seam
│  │  │     └─ index.rs                 ~ reads through the read seam
│  │  └─ pack/
│  │     ├─ mod.rs                      ~ re-exports
│  │     ├─ assemble.rs
│  │     └─ layout.rs                   ~ drops pack_capacity in step 12
│  ├─ tests/                            + support/memory_engines.rs; C2 vectors on the port path
│  └─ examples/                         ~ measurement examples rebound or dropped in step 12
│
├─ layerfs-history/                       C5 — contract unchanged
│  ├─ Cargo.toml                        ~ − rusqlite, − `native` feature (step 12)
│  ├─ src/
│  │  ├─ lib.rs                         ~ drops the sqlite module (step 12)
│  │  ├─ catalog.rs                       HistoryCatalog (20 methods)
│  │  └─ records.rs  identity.rs  error.rs
│  └─ tests/                            ~ provider tests live in layerfs-metadata after step 12
│
├─ layerfs-s3/                          + engine: MinIO / S3
│  ├─ Cargo.toml                          layerfs-storage, sha2
│  ├─ src/
│  │  ├─ lib.rs                           declarations
│  │  ├─ config.rs                        endpoint, bucket, prefix, credentials, timeouts
│  │  ├─ sign.rs                          SigV4 canonical request, HMAC-SHA256, signing key
│  │  ├─ http.rs                          one persistent HTTP/1.1 connection: request, response, timeouts
│  │  └─ client.rs                        S3Objects: put_if_absent, read, head; error classes; counters
│  └─ tests/
│     ├─ minio_contract.rs
│     └─ support/mod.rs
│
├─ layerfs-metadata/                    + engine: PostgreSQL
│  ├─ Cargo.toml                          layerfs-storage, layerfs-history, layerfs-content, client per Q1
│  ├─ sql/
│  │  ├─ storage.sql                      store_policy, pack, object, metadata_value_group,
│  │  │                                   content_signature, pack id sequence
│  │  └─ history.sql                      history_meta, layer_stack, layer, commit, branch,
│  │                                      workspace_stage, scope_allocator
│  ├─ src/
│  │  ├─ lib.rs                           declarations
│  │  ├─ config.rs                        host, port, database, schema, credentials, timeouts
│  │  ├─ connection.rs                    connect, authenticate, one-round-trip batches, timeouts, counters
│  │  ├─ schema.rs                        bootstrap and identity validation; never migrates
│  │  ├─ storage/
│  │  │  ├─ mod.rs                        declarations
│  │  │  ├─ read.rs                       policy, locate, read_packs, value_groups, signatures
│  │  │  └─ write.rs                      reserve, register
│  │  └─ history/
│  │     ├─ mod.rs                        declarations
│  │     ├─ catalog.rs                    PgHistory: create, open, HistoryCatalog delegation
│  │     ├─ layerstack.rs  branch.rs  commit.rs  staging.rs
│  │     └─ allocation.rs  query.rs  rows.rs
│  └─ tests/
│     ├─ storage_contract.rs
│     ├─ history_*.rs                     the six C5 contract files, bound to PostgreSQL
│     └─ support/mod.rs
│
└─ layerfs-project/                     + namespace Init / import (Q3)
   ├─ Cargo.toml                          content, storage, history, telemetry; dev: s3, metadata, sha2
   ├─ src/
   │  ├─ lib.rs                           declarations
   │  ├─ error.rs                         ProjectError (replaces the bridge Failure/Code)
   │  ├─ init.rs                          scan → reserve_inodes → namespace → initialize_layerstack
   │  ├─ scan.rs                          directory walk, INIT_WORKERS construction, one file save
   │  ├─ namespace.rs                     prerequisite save and tree save
   │  └─ metadata.rs                      attribute and metadata builders
   ├─ examples/
   │  ├─ benchmark_init.rs
   │  └─ verify_namespace.rs
   └─ tests/
      ├─ init_memory.rs  init_services.rs  storage_parity.rs
      └─ support/
```

Present only until step 12, then deleted:

```text
layerfs-storage/
  sql/schema.sql
  src/source.rs                           crate-internal read seam shared by both paths (added in step 2;
                                          folded into read/fetch.rs in step 12)
  src/cas/        batch.rs  collision.rs  dependencies.rs  finish.rs  lifecycle.rs  membership.rs
                  owner.rs  placement.rs  pool_lane.rs  provider.rs  read.rs  save.rs
                  selection.rs  store.rs  mod.rs
  src/sqlite/     cleanup.rs  connection.rs  lookup.rs  ownership.rs  pool.rs  reservation.rs
                  schema.rs  write.rs  mod.rs   (+ source.rs, signatures.rs: the old path's side of the seam)
  src/pack/placement.rs
  tests/          the 34 existing test files for the old path

layerfs-history/
  sql/schema-v1.sql
  src/sqlite/     allocation.rs  branch.rs  commit.rs  layerstack.rs  open.rs  query.rs
                  rows.rs  staging.rs  mod.rs
  tests/          the 6 existing provider test files (copied to layerfs-metadata in step 7)
```

Outside `crates/`: `core/Cargo.toml` (members), `core/Cargo.lock`,
`core/tools/check_product_boundary.py` and its test (dependency table, new
crates in `UNSAFE_FREE_CRATES`), `core/tools/phase7_services.py` (new development
tool, §1.2), `core/benchmark/fs-bench-pro*` (step 10),
`core/docs/architecture/{01,05,07,13,14,15,16}` (updated in the commit that
changes what they describe).

### 1.2 Environment

**Decided** (packet 04 §3 and §9, amended `core/AGENTS.md`): one machine;
PostgreSQL and MinIO each in a local Docker container; MinIO reached over plain
HTTP with the S3 API, PostgreSQL over its own protocol on TCP; no TLS and no
remote endpoint; daemons connect over the Docker network and host tools through
a port published on localhost; both images pinned by digest and recorded in
every identity set. MinIO is single node, single drive, one bucket on a named
volume, with versioning, object lock, lifecycle rules, server-side compression
and encryption off. PostgreSQL uses its default durability settings and
`READ COMMITTED`.

**Proposed** (not yet confirmed by the owner; image pins are Q12). These are the
two products' default ports. `core/tools/phase7_services.py` is the only place
that starts, resets and stops the containers for cluster 1 tests and benchmarks.

| | PostgreSQL | MinIO |
| --- | --- | --- |
| Container name | `layerfs-postgres` | `layerfs-minio` |
| Port in container | 5432 | 9000 (S3 API); 9001 (console, optional) |
| Published on host | `127.0.0.1:5432` | `127.0.0.1:9000` |
| From a sandbox container | `layerfs-postgres:5432` on a shared Docker network | `layerfs-minio:9000` on the same network |
| Volume | named volume at `/var/lib/postgresql/data` | named volume at `/data` |
| Container settings | `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB=layerfs` | `MINIO_ROOT_USER`, `MINIO_ROOT_PASSWORD`; command `server /data` |
| Created by bootstrap | one schema with the twelve tables below (`layerfs_metadata::schema`) | one bucket; keys `<prefix>/packs/<hh>/<digest>` |
| Image | pin by digest (Q12) | pin by digest (Q12) |

Both ports bind to `127.0.0.1` only. Credentials are generated per deployment by
the tool and never committed. The tool prints the connection settings as
environment variables that the engine tests and benchmark drivers read; the
variable names are fixed in step 1. A missing service fails a test, it does not
skip it (Q10).

Tool commands (step 1): `up` (create network, volumes, containers; wait for
readiness; create bucket and schema), `reset` (remove containers and volumes and
run `up` again — the fresh-services contract of §3.2), `down`, `status` (image
digests, PostgreSQL settings hash, MinIO environment, for the identity set).

Observed on the planning machine, 2026-10-03: ports 5432, 9000 and 9001 have no
listener; no PostgreSQL image is present locally; the only MinIO image present
is `cgr.dev/chainguard/minio:latest`, which is a tag and not a pin.

**PostgreSQL tables.** Twelve, in two groups with no foreign key between them.
Exact types and constraints are frozen in steps 6 and 7.

| Group | Table | Key | Purpose |
| --- | --- | --- | --- |
| C2 | `store_policy` | single row | Storage profile; pooled ordinal counter and retained window |
| C2 | `pack` | `pack_id` (sequence) | One sealed pack: domain, SHA-256 digest, length; body for metadata packs only |
| C2 | `object` | `object_id` | Locator: role, canonical length, pack, group, record. First insert wins |
| C2 | `metadata_value_group` | `first_ordinal` | Pooled value-group catalogue |
| C2 | `content_signature` | `slot` | Bounded ring of delta-base candidates |
| C5 | `history_meta` | single row | Catalog identity, incarnation, next stage token |
| C5 | `layer_stack` | `layer_stack_id` | A project's stack and its head Layer |
| C5 | `layer` | `layer_id` | Immutable Layer and the root it selects |
| C5 | `commit` | `commit_id` | Immutable Commit |
| C5 | `branch` | `branch_id` | Branch head: the one conditionally updated fact |
| C5 | `workspace_stage` | `workspace_id` | Staged candidate awaiting commit |
| C5 | `scope_allocator` | `scope_id` | Inode-serial reservations |

Against the base schemas: `saves` and every `save_id` column are gone; the
writer budget, publication sequence and pack ceiling leave `store_policy`; the
pack-id counter becomes a sequence; `object` is keyed by `object_id` alone.
File-content packs are objects in MinIO and appear in no table.

---

## 2. Rollout plan

Common checks, run once per step at its final tree, from the repository root:

```sh
cargo +1.85.1 fmt   --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 test  --manifest-path core/Cargo.toml --locked -p <packages named in the step>
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Every commit message carries `Production LOC: <before> -> <after> (delta ±n)`
from `python3 tools/production_loc.py --json --root <snapshot>` on the exact
first-parent and staged trees, with subtotals: reference `crates/`; core old
storage path (the two delete sets, 6,141 at the base); core new storage path
(`port/`, `read/`, `save/`, `storage.rs`, `location.rs`, `source.rs`,
`layerfs-s3`, `layerfs-metadata`, `layerfs-project`); rest of core. Moves are
labelled relocation.

| # | Step | Depends on | Covering tests (`-p`) | Exit condition |
| ---: | --- | --- | --- | --- |
| 1 | **Contracts and tooling.** Commit this plan; extend the boundary guard with the allowed-dependency table for cluster 1 crates and the rule that no `layerfs-storage`/`-history`/`-project` `[dependencies]` names an engine or a cluster 2 crate; add `phase7_services.py` with both images pinned by digest | Q1–Q4, Q10, Q12 | tools unit tests | Guard fails on a seeded forbidden edge; LOC delta 0 |
| 2 | **Read seam.** Move the location and row types to `location.rs`; add `source.rs`; `encoding/` reads through it; the old path implements it in `sqlite/source.rs` | — | `layerfs-storage`, `layerfs-server`, `layerfs-sdk` | Every existing test passes unchanged; no `rusqlite` import under `encoding/` or `pack/` (added to the guard) |
| 3 | **Ports and read path.** `port/`, `storage.rs`, `read/`; in-memory engines under `tests/support/` | 2 | `layerfs-storage` | Objects registered through the port by a test read back authenticated, including PREFIX chains across packs and pooled leaves; wrong digest, missing pack and missing base are refused; round trips per read wave are asserted |
| 4a | **Write path, file objects.** `save/` for the Native, WholeFile, Singleton and Ordinary lanes: reuse, collision, selection, sealing, closure, register | 3 | `layerfs-storage` | The C2 file vectors (roundtrip, reuse, delta chains, delta payload, stored payloads, physical formats, edit pipeline, memory bounds) pass on the port path; sealed pack bytes equal the old path's for the same input; a batch whose reference is unregistered is never registered (test with a failing engine) |
| 4b | **Write path, pooled metadata (D12).** `save/pooled.rs`; ordinal reservation; sealed pooled packs | 4a | `layerfs-storage` | Filesystem and metadata-pool vectors pass; roots equal the old path's; pooled packs, forced seals and reserved-directory bytes are counted and recorded |
| 5 | **`layerfs-s3`.** | 3 (port types), Q2 | `layerfs-s3` (MinIO running) | The three calls and all four error classes pass against the pinned MinIO; `If-None-Match: *` create is proven or the provider is reported unsupported; a lost reply is `Uncertain`; requests and bytes counted; a labelled diagnostic records request cost for 256 KiB and 16 MiB bodies |
| 6 | **`layerfs-metadata`, C2 side.** Schema, connection, the seven operations | 3, Q1 | `layerfs-metadata` (PostgreSQL running) | The same contract file used for the in-memory engine passes; two connections exercise first-wins and report the loser; a statement timeout on `register` is `Uncertain`; one round trip per operation is asserted |
| 7 | **`layerfs-metadata`, C5 side.** `PgHistory` | 6 | `layerfs-metadata` | The six C5 contract test files, copied and bound to PostgreSQL, pass; two connections exercise the conditional head update; round trips per operation recorded |
| 8 | **`layerfs-project`.** Init over ports, with the bounded Init upload window | 4b | `layerfs-project`, then with services | Init of the 100- and 1,000-file fixtures over in-memory engines and over real engines passes `verify_namespace`; nothing in `src/` names an engine |
| 9 | **Storage parity (M4) and count diagnostic.** `tests/storage_parity.rs` on real engines | 5, 6, 8 | `layerfs-project` | Identical-content reuse, PREFIX selection, depth-bound FULL, cross-pack base, pooled metadata, corrupt and missing base refusal pass with payload on MinIO and metadata in PostgreSQL; the §0.5 counters are recorded per case as a labelled diagnostic |
| 10 | **Harness.** New, separately identified fs-bench-pro selections for both arms (§3); history driver bound to the port path; server cold contract and storage accounting implemented; focused harness tests | 7, 9, Q5–Q9 | harness Python tests | Self-checks pass; case specification frozen before any timed sample |
| 11 | **Final verification and acceptance rows** (§3) | 10 | all cluster 1 packages, once | §3 exit |
| 12 | **Retirement.** Delete the two delete sets; collapse `source.rs`; drop `rusqlite`, `nix`, the `native` feature | Cluster 2 has removed `layerfs-server` and the SDK's dependency on it (M9) | all | No reference to the removed paths; LOC reported as retirement, with the net against the base stated |

**Coexistence.** `layerfs-server` and `layerfs-sdk` are the only product
consumers of C2's `Store` and C5's SQLite provider (`grep` of every
`core/crates/*/Cargo.toml`). They keep building against the old API, which
changes only in step 2. The new handle uses different names (`Storage`, `Save`,
`SaveSink`, `Reader`), so both can be exported until step 12. If the owner lets
cluster 1 drop the server earlier (Q11), steps 2 and 12 shrink: `source.rs` is
never introduced.

**Shared files.** Both clusters will edit:

| File | Cluster 1 change | Rule to avoid conflict |
| --- | --- | --- |
| `core/Cargo.toml` | Three members | One member per line; append only |
| `core/Cargo.lock` | `layerfs-s3`, `layerfs-metadata` and its client, `layerfs-project` | A merge re-resolves only the union of both clusters' additions; no unrelated version moves; `cargo metadata --locked` passes |
| `core/tools/check_product_boundary.py` (+ test) | Dependency table rows and unsafe-free entries for cluster 1 crates | Table is data, one crate per row; each cluster adds only its rows |
| `tools/production_loc.py` | None (it globs `core/crates/*/src` and `sql/`); verify the new crates are counted in step 5 | — |
| `core/AGENTS.md`, `core/docs/issues/301/` | None | The amended file and the packet are uncommitted in the primary checkout; the owner lands them on the shared base before either cluster links to them |

**Interface requirements on cluster 2.** (a) The Commit orchestrator calls
`Storage::begin_save`/`Save::finish` and `Reader`; it needs no other C2 surface.
(b) The host-side caller of Init wires `layerfs-s3` and `layerfs-metadata` into
`layerfs_project::init` (Q4). (c) Container bootstrap (bucket, database, schema
via `layerfs_metadata::schema`) belongs to `layerfs-sandbox`. (d) From step 5,
`cargo test` of the engine crates needs the two containers (Q10).

---

## 3. Final verification and benchmark

### 3.1 What baseline evidence exists

There is **no eligible baseline speed row**, and no row at `7edddbdb8`.

| Selection | Receipt (`core/docs/issues/286/experiments/`) | Source | Number | Cache contract in receipt | Status |
| --- | --- | --- | ---: | --- | --- |
| Init 100 | `20260930-init-regression-r045.md` | `530dfa536` | 34,812,375 ns raw call | `source-cache-uncontrolled-v1` | functional PASS, numeric INELIGIBLE |
| Init 1,000 | same | same | 114,956,291 ns | same | same |
| Init 10,000 | `20260930-init-10000-v4-r042.md` | `0a8bac6fb` | 1.591348 s | same | same; `UNREGISTERED_DIAGNOSTIC` |
| Init 100,000 | `20260930-init-100000-v4-r041.md` | `fc7ed1a30` | 6,043,097,916 ns | same | same |
| stride 10 | `20260930-history-regression-r046.md` | `a12ab932e` | 32,153,086,958 ns driver / 60 s | `fresh-growing-store; untimed corpus reads; no cold time claim` | PASS storage, time INELIGIBLE |
| stride 3 | same | same | 52,445,633,542 ns / 170 s | same | same |
| stride 1 | `20260930-history-stride1-regression-r047.md` | `b2ea44f0f` | 136,703,421,625 ns / 170 s | same | same; internal operation span UNAVAILABLE |

These numbers are not the bar. The bar is a **matched baseline arm** taken at
`7edddbdb8` under the contract in §3.2: one sample per case, in a clean worktree
at the pin with its own Cargo target, release binaries, built from unmodified
product source, driven by the base drivers (`layerfs-sdk` example
`benchmark_init`; `fs-bench-storage-content`) under the step-10 harness. The
candidate arm uses the same fixtures, corpus, harness identity and machine
window. Baseline and candidate rows are never pooled, and neither is repeated.

Speed limit: `candidate ≤ matched baseline` per case, on the raw Init call and on
the complete history driver wall (the stride-1 internal span clips, r047). Any
tolerance is the owner's to state before collection (Q5). Hard bounds that also
apply: Init complete command ≤ 15 s and verifier ≤ 9.5 s; the history bounds
below need an explicit exception to the 15 s/25 s rule (Q7).

### 3.2 Cache contract `phase7-fresh-services-v1`

Declared identically for both arms and enforced, or the row is `INELIGIBLE`.

- **Inputs.** The Init fixture tree and the history corpus
  (`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, 2.8 G) are de-warmed
  and checked for zero resident pages immediately before the sample with the
  existing `mincore`/`msync(MS_INVALIDATE)` tool
  (`core/benchmark/fs-bench-pro-storage-content/shared/residency.py`). Nothing
  touches them between the check and the timer.
- **Servers.** Before each sample, outside every timer: both containers and
  their named volumes are removed, new volumes created, the pinned images
  started, readiness awaited, bucket and schema bootstrapped, a PostgreSQL
  `CHECKPOINT` issued. The stores are empty, so neither PostgreSQL's buffers nor
  the Docker VM's page cache can hold data that credits the sample. Container
  start is setup.
- **Inside the timer.** Engine connections are opened inside the timed call, as
  the base opens its SQLite connections inside it. Data the sample itself wrote
  may be served back by server caches; the baseline arm reads its own writes
  from the OS page cache in the same way. This is declared, not excused.
- **Populated-store rows** (none in this cluster; cluster 2 needs them): restart
  both containers and drop the VM page cache from a privileged helper, outside
  the timer; otherwise `INELIGIBLE`.
- **Declared asymmetry.** The baseline writes with no journal sync
  (`sqlite/connection.rs`); the candidate pays PostgreSQL's WAL sync per
  `register` and MinIO's own write discipline. No setting is changed to hide it
  (Q9).
- **Identity set.** Source commit and tree, product, compilation and dependency
  seals, harness seal, root `.cargo/config.toml` hash, both image digests,
  PostgreSQL settings hash, MinIO environment, Docker VM CPU and memory, corpus
  and fixture identity, `LAYERFS_CONSTRUCTION_WORKERS=1`. Init keeps
  `INIT_WORKERS = 4` (`layerfs-server/src/service/save/import/scan.rs:21`) in both arms.

### 3.3 Selections

Case identifiers are new and frozen in step 10; existing v1–v4 receipts are never
relabelled. Commands are the intended form after step 10 and do not exist at the
base.

| Selection | Baseline limit source | Storage limit | Command (per arm, fresh `--out`) |
| --- | --- | --- | --- |
| Init 100 / 1,000 / 10,000 / 100,000 (Q6) | Matched arm at `7edddbdb8` | None; MinIO and PostgreSQL allocation reported | `python3 core/benchmark/fs-bench-pro/runner.py run --case <init case> --out benchmark-results/fs-bench-pro/issue302-<case>-<arm>` |
| stride 10, 17 states | Matched arm; command ≤ 60 s, verifier ≤ 10 s | `< 54,278,964 B` | `… run --case <stride-10 case> --out …` |
| stride 3, 53 states | Matched arm; command ≤ 170 s, verifier ≤ 20 s | `< 70,427,034 B` | `… run --case <stride-3 case> --out …` |
| stride 1, 157 states, run-only | Matched arm; command ≤ 170 s, verifier ≤ 30 s | `< 92,342,273 B` | `… run --case <stride-1 case> --out …` |

Storage ceilings are the owner-approved v4 values
(`core/docs/issues/286/HISTORY-STORAGE-TOLERANCE-V4-20260930.md`): original
strict targets 49,344,512 / 64,024,576 / 83,947,520 B plus 10%. Deviation is
reported against the original target, as in r046/r047.

**Storage accounting** (collected after the operation, outside timers):

- MinIO: `st_blocks × 512` summed over every file under the bucket's data path
  in the fresh volume, object metadata files included; object count and summed
  object lengths reported beside it.
- PostgreSQL: after `CHECKPOINT`, the sum of `pg_total_relation_size` over every
  LayerFS relation (heap, indexes, TOAST), reported per table and split C2/C5.
  `pg_database_size`, the WAL directory and MinIO's system directory are
  reported separately as server overhead (Q8).
- Total = MinIO + PostgreSQL C2 + PostgreSQL C5, compared with strict `<`. A
  missing count is `INCOMPLETE`; a shared or non-fresh volume is `INELIGIBLE`.
- Retained rows must equal the base expectation: 1 stack, `states` branches and
  layers, `states − 1` commits, 0 stages (`shared/history_storage.py`).

What the new stores add or remove is **unknown until measured**. Known inputs:
base totals 52,473,856 / 65,142,784 / 86,179,840 B leave headroom of 1,805,108 /
5,284,250 / 6,162,433 B under the ceilings. At stride 10 the base C2 file holds
45,594,892 B of packs in a 49,573,888 B database, so all rows and indexes cost
3,978,996 B today. Added: PostgreSQL tuple and index overhead, a 32-byte digest
per pack row, per-object MinIO metadata and block rounding over 632–1,256
objects, and 4,120 B of header and reserved directory per additional pooled pack
(`pack/layout.rs`: 24 + 16 × 256). Removed: the `saves` table, `save_id` columns,
SQLite page slack. Stride 10 is the row most likely to miss; a miss is reported
as `FAIL`, never adjusted.

### 3.4 Correctness before any timing

Run once at the frozen source identity; a red result is diagnosed from its
output, fixed once, and the covering commands run once.

1. The common checks of §2 for all cluster 1 packages, with both containers up.
2. Step 9's storage parity on real engines.
3. Init: the separate verifier reopens through the engines, inventories every
   path and kind, checks directory metadata, and checks metadata and every byte
   of the declared deterministic file sample (the r045 scope, stated as such).
4. History: the separate verifier reopens C5 from PostgreSQL and compares all
   17 / 53 / 157 roots with the independent pins
   (`core/docs/issues/286/oracles/history-reference-v2`); canonical bytes and
   object counts equal 380,559,460 / 51,689, 589,480,854 / 73,447 and
   871,337,620 / 104,618 (`families/history_retention.py::V3_CANONICAL`); full
   state trees and the declared content selection match. Equal roots are the
   proof that identities and formats did not change.
5. Cleanup gate and container teardown recorded.

Verification wall is separate from the performance timer. Exit of step 11: every
selection has one baseline and one candidate receipt with status, cache
declaration, counters and storage accounting; every FAIL, INELIGIBLE and NOT_RUN
is in the ledger entry.

---

## Decisions made

| Decision | Result | Evidence |
| --- | --- | --- |
| D10 port shape | Seven transactional units (§0.2) | Operations sharing one SQL transaction today: the wave (`cas/save.rs` → `owner.with_wave`: lookup, pack and row inserts, collision validation, signature flush, commit), same-save reads of unpublished rows (`cas/store.rs::SaveOperation::read_batch`), ordinal reservation (`cas/pool_lane.rs`), publish (`cas/lifecycle.rs::finish_inner`). Each maps to `locate` + wave buffer + `register`; none needs a transaction held across calls |
| D12 pooled packs | Sealed when full or when referenced by a registered leaf; in-place append removed | `pack/placement.rs` keeps only this lane open; `lifecycle.rs` already closes the tail at every save. Base shape: 1,585 / 4,035 / 8,959 appends, 41 / 72 / 172 pooled packs, 1.10–2.41 pooled packs per save. Cost in extra packs: unknown, counted in step 4b |
| D13 Init home | New crate `layerfs-project` | C2 may not depend on C5; Init reads a directory; `examples/verify_namespace.rs` already needs only C1, C2 and C5 |
| D5 S3 client | Own client, pending approval (Q2) | `sha2 0.10.9` is already a product dependency (`cargo tree -i sha2`); no new package |
| D11 PostgreSQL client | Presented, not chosen (Q1) | Resolved trees, 2026-10-03, `aarch64-unknown-linux-musl`, not built: `postgres 0.19.14` = 58 packages, 35 new names including `tokio 1.53.1`, `tokio-util`, `mio`, `socket2`, `futures-*`, and 12 second versions of locked crates; `postgres-protocol 0.6.12` = 29 packages, 16 new names, no runtime; `pq-sys 0.7.6` = 2 packages plus native libpq in every build |
| D6 pack size | Unchanged | No count yet says otherwise; the Init-10,000 request count (1,320) times the PUT cost measured in step 5 is the evidence that would |
| `saves` and ceilings | Removed, with the closure rule | §0.6 row 3 |
| Coexistence | Additive; old path retired in step 12 | §0.6 row 8; only `layerfs-server` and `layerfs-sdk` consume the old API |
| Baseline | No eligible row exists; a matched arm is taken | §3.1 |

## Questions for the owner

1. **D11:** `postgres-protocol 0.6.12` with our own blocking connection (recommended), `postgres 0.19.14` with tokio, or no dependency?
2. **D5:** approve the own S3 client using the already-locked `sha2`, with HMAC written in `layerfs-s3`?
3. Approve a 14th crate for Init, named `layerfs-project`?
4. Which host-side crate may name the engines to call Init outside a sandbox — `layerfs-api`'s SDK (recommended) or another?
5. Is "meet the baseline" strictly `candidate ≤ matched baseline` on one pair, or is there a tolerance, and what is it?
6. Are all four Init tiers in the acceptance set, or only 100 and 1,000?
7. Do the history schedules keep their frozen 60 / 170 / 170 s command and 10 / 20 / 30 s verifier bounds as a declared exception to the 15 s / 25 s and <10 s rules?
8. Does "PostgreSQL allocation" mean LayerFS relations only (recommended) or the whole data directory with catalogs and WAL; and is MinIO's system directory excluded?
9. If PostgreSQL's commit sync alone causes a speed miss, is a declared `synchronous_commit = off` profile acceptable, or is the default fixed?
10. May `cargo test` for the engine crates fail when the containers are absent (recommended), rather than skip?
11. Must `layerfs-server` build until cluster 2's M9 (assumed), or may cluster 1 remove it and delete the old paths at its own exit?
12. Which PostgreSQL major version and MinIO release are pinned?
13. Does the cold 2.7 s Init target apply to this cluster, and to which case? (r045 records that it does not apply to the SDK route.)

## Verified in source versus inferred

**Verified at `7edddbdb8` or in retained receipts:** production line figures;
where SQL reaches in C2; the wave transaction, pending-reference acceptance and
unchecked reuse path; per-edge point lookups; pooled lane mechanics and lane
limits; the Init flow (three saves, `reserve_inodes`, `initialize_layerstack`,
`INIT_WORKERS = 4`, Store created before the timer in `benchmark_init.rs`); the
old API's consumers; all baseline receipts, their sources and cache contracts;
pack, object, edge and reuse counts; the candidate dependency trees.

**Inferred or unverified:** that the pinned MinIO honours `If-None-Match: *` on
`PutObject` and accepts the planned request shapes; that level-wise prefetch
keeps round trips within the §0.5 target; that producers always emit children
before parents (the closure rule does not rely on it); that a bounded Init
upload window is sufficient; every cost of PostgreSQL and MinIO in time or
bytes; that the resolved dependency versions build on 1.85.1 and unify the same
way inside `core/Cargo.lock`; that the benchmark history driver can be bound to
the port path without changing its workload.
