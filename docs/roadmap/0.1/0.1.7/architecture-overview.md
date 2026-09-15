# LayerFS architecture overview — v0.1.7 exploration study

> Status: Research; informative and not a product contract.

This is the source-bound study requested by
[#156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/156). It describes
what exists; it does not select architecture, freeze scope, or authorize work.
Observations that imply work are recorded in the hand-off list at the end and
belong to the v0.1.7 checklist, not to this document.

- **Source pin:** every citation below was read at commit
  `40af8529f47dca75e198b312ab2e494ff768a625`. HEAD advanced to
  `87ed92df035075750fcac7e3d49ce7801e808849` while the study ran; the diff
  between the two touches no file under `crates/`, `sql/` or `tools/` (only
  `docs/roadmap/0.1/0.1.6*` and two `benchmark/fs-bench-pro/shared/` harness
  files), so every citation holds identically at both commits.
- **Method:** source reads only — no builds, tests, benchmarks or code changes.
  Evidence was gathered by read-only exploration subagents (13 breadth slices,
  3 depth follow-ups, 2 adversarial checks) and every load-bearing claim below
  was re-opened and verified by the coordinating author. Claims that could not
  be verified are marked `unknown`, not guessed.

---

## 1. Product identity — where the architecture gets its inspiration

The pitch, verbatim from [`README.md`](../../../../../README.md#L26):

> LayerFS is a **SQLite-backed, content-addressed time machine for agent
> Workspaces**. Give every filesystem-affecting tool call its own ephemeral
> Workspace; when retained, that call becomes one immutable Commit—the base
> unit of the filesystem timeline. CAS, CDC, and COW store one shared base
> plus unique deltas instead of cloning full environments. From any Layer or
> eligible Commit, agents can fork zero-copy Branches, run parallel rollouts,
> discard failures, roll back by forking an earlier state, and promote a
> winning Branch with `Add`. The filesystem remains load-bearing for recursive
> multi-agent exploration without multiplying storage.

North-star phrase: **"Ephemeral Workspaces. Durable Shared History."**
([`README.md`](../../../../../README.md#L28), repeated in
[`docs/roadmap/architecture.md`](../../architecture.md#L41)).

Each phrase in the pitch imposes an obligation the rest of this document must
verify:

| Claim in the pitch | Architectural obligation it imposes | Verified in |
| --- | --- | --- |
| "content-addressed time machine" | canonical identity + authenticated reads, immutable history | §2 rows 1, 3 |
| "every filesystem-affecting tool call → its own ephemeral Workspace" | Workspace is disposable; no database of its own | §2 row 2 |
| "when retained, that call becomes one immutable Commit" | Commit is the base unit; publication is compare-and-swap | §2 row 3 |
| "one shared base plus unique deltas instead of cloning" | CAS + small-file delta encoding + large-file CDC + COW | §2 row 4, §3 |
| "fork zero-copy Branches … discard failures, roll back by forking" | Branch/Workspace lifecycle with no canonical copies; no rewind operation | §2 rows 5–6 |
| "promote a winning Branch with `Add`" | explicit Branch→LayerStack publication | §2 row 7 |
| "load-bearing … without multiplying storage" | the cost-of-change invariant | §2 row 8, §3.5 |

### Sources of intent and their authority

| Source | Authority | Use in this study |
| --- | --- | --- |
| [`README.md`](../../../../../README.md) | shipped language (v0.1.5 developer preview) — highest authority for product identity | §1's pitch and obligations, quoted verbatim |
| [`docs/roadmap/architecture.md`](../../architecture.md) | north star and ownership boundaries (planning; LayerFS owns canonical identity, CAS/CDC/COW, entities, Workspace state, capture/reconcile/Commit/Add, projection contracts, import/export — not orchestration, Git semantics, network policy, microVM lifecycle, or model providers) | boundary language for §7; never quoted as behavior |
| [`docs/research/vision/`](../../../research/vision/README.md) | long-range vision, **explicitly non-binding** ("explains long-term ideas rather than the exact LayerFS … SDK, CLI, Store, or runtime contract") | cited as intent only, never as mechanism |

Rule applied throughout: only mechanisms verified in code enter the map.

---

## 2. Claim → mechanism table

The spine of the study. Each obligation from §1, the mechanism that implements
it at the pinned commit, its source, and a verification status
(`verified` = opened and confirmed at the pinned commit; `partial` = confirmed
with a stated caveat; `not-found` = no mechanism exists).

| # | Pitch claim | Mechanism at HEAD | Source | Status |
| --- | --- | --- | --- | --- |
| 1 | content-addressed identity | `ObjectId` = BLAKE3 over domain `layerfs/object/v2\0` + the complete canonical `LFSO`-framed object bytes (header, kind, length, payload — structure and length are authenticated, not assumed). Every durable read passes `authenticate()`: indexed length equality + recomputed digest; mismatch → `IdentityMismatch` / `Integrity` | [`object/digest.rs`](../../../../crates/layerfs-content/src/object/digest.rs#L6), [`object/codec.rs`](../../../../crates/layerfs-content/src/object/codec.rs#L163), [`objects/read.rs`](../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L2040) | verified |
| 2 | Workspace is disposable | `WorkspaceState { Active, Committed, Discarded, Ended, BrokenCleanup }`; a Workspace holds a private overlay (live nodes, dirty set, capture state, spool) and no database of its own; spool directory deleted at End; registry `Drop` force-discards active sessions. Caveat: `Committed` is never assigned anywhere (see §9) | [`lifecycle.rs`](../../../../crates/layerfs-workspace/src/lifecycle.rs#L21), [`registry.rs`](../../../../crates/layerfs-workspace/src/registry.rs#L53), [`session.rs`](../../../../crates/layerfs-workspace/src/session.rs#L32) | verified (with caveat) |
| 3 | Commit immutable, base unit | `commits` and `layers` rows are insert-only — the only `UPDATE` statements in the whole SQL corpus are the two head CASes (`advance_branch`, `advance_head`); the only `DELETE` is the workspace stage. `CommitId` = tag `0x12` + BLAKE3(root, parent, base); insert is `ON CONFLICT DO NOTHING` with a byte-equal collision check | [`sql/workspace/advance_branch.sql`](../../../../crates/layerfs-layerstack-store/sql/workspace/advance_branch.sql#L5), [`ids.rs`](../../../../crates/layerfs-layerstack-store/src/ids.rs#L108), [`sql/workspace/insert_commit.sql`](../../../../crates/layerfs-layerstack-store/sql/workspace/insert_commit.sql#L5), [`records.rs`](../../../../crates/layerfs-layerstack-store/src/records.rs#L84) | verified |
| 4 | CAS + small delta + large CDC + COW | small files (< 128 KiB) become one `LFS5SML` object stored in pack v3/v4 groups with bounded zstd FULL/PREFIX delta records and chains; large files CDC-chunk (8/16/32 KiB frozen gear profile) into extent ropes; edits splice the rope and re-chunk only the replacement; Workspaces overlay a private piece tree over immutable base roots | [`file/content.rs`](../../../../crates/layerfs-content/src/file/content.rs#L8), [`objects/admission.rs`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L517), [`file/cdc/gear.rs`](../../../../crates/layerfs-content/src/file/cdc/gear.rs#L7), [`file/rope/edit.rs`](../../../../crates/layerfs-content/src/file/rope/edit.rs#L69) | verified |
| 5 | fork zero-copy | `fork_branch` = one `INSERT` into `branches` (base Layer + optional head Commit); no object admission, no tree build, no payload copy | [`branch.rs`](../../../../crates/layerfs-layerstack-store/src/branch.rs#L51), [`sql/branch/insert.sql`](../../../../crates/layerfs-layerstack-store/sql/branch/insert.sql#L5) | verified |
| 6 | roll back by forking | forking from a Commit requires that Commit to be in Branch ancestry (bounded recursive CTE, depth ceiling 1,000,000); there is no rewind/move-head-back operation — history is only extended | [`branch.rs`](../../../../crates/layerfs-layerstack-store/src/branch.rs#L23), [`sql/branch/contains_commit.sql`](../../../../crates/layerfs-layerstack-store/sql/branch/contains_commit.sql#L10) | verified |
| 7 | promote with `Add` | `add_layer` checks idempotence/staleness/no-change, then one transaction: `INSERT_LAYER` (new Layer's `root_id` **is** the head Commit's `root_id` — a re-labeling, zero object copies) + `UPDATE layer_stacks … WHERE head_layer_id = expected` CAS. Outcomes: `Added / UpToDate / NoChanges / HeadMoved` | [`layerstack.rs`](../../../../crates/layerfs-layerstack-store/src/layerstack.rs#L210), [`sql/layerstack/advance_head.sql`](../../../../crates/layerstack-store/../layerfs-layerstack-store/sql/layerstack/advance_head.sql#L5) | verified |
| 8 | cost-of-change invariant | every state is complete logically, incremental physically; `canonical_storage` counts all admitted objects, `reachable_storage` walks layer/commit/branch roots; the difference (lost-race orphans) is measured by the Monitor and is **never reclaimed — no GC exists** (§3.5) | [`sql/query/canonical_storage.sql`](../../../../crates/layerfs-layerstack-store/sql/query/canonical_storage.sql#L5), [`query.rs`](../../../../crates/layerfs-layerstack-store/src/query.rs#L297), [`monitor/dedup.rs`](../../../../crates/layerfs-monitor/src/dedup.rs#L40) | verified (consequence recorded, not judged) |

Rows dropped during verification are recorded in §9.4 with the reason.

---

## 3. Storage model — the small-file / large-file split

The governing invariant, in the shipped language of
[`README.md`](../../../../../README.md#L52): *every filesystem state is
complete logically, but incremental physically. A new state should cost what
changed, not the size of the Workspace it exposes.*

### 3.1 Large files — region locality (CDC, extents, ropes)

One frozen FastCDC profile is in force; there is no second gear table in the
tree ([`file/cdc/gear.rs`](../../../../crates/layerfs-content/src/file/cdc/gear.rs#L7)):

```text
MINIMUM_CHUNK_BYTES = 8_192    (8 KiB)
TARGET_CHUNK_BYTES  = 16_384   (16 KiB)
MAXIMUM_CHUNK_BYTES = 32_768   (32 KiB)
NORMALIZATION_SHIFT = 2
PROFILE_SEED        = 0
```

`profile_id()` hashes the label strings, every constant, the four masks and
all 256 gear multipliers ([`gear.rs:18-38`](../../../../crates/layerfs-content/src/file/cdc/gear.rs#L18));
the profile id is pinned inside every `FileStateV3`, so any drift is a
`ProfileMismatch` on decode, not silent re-chunking.

An edit is a structural splice, not a rewrite
([`file/rope/edit.rs:69-88`](../../../../crates/layerfs-content/src/file/rope/edit.rs#L69)):

```text
  old file state (immutable FileStateV3, extent.rs:154)
     |
     v
  rope::replace(start, delete_len, replacement reader)   edit.rs:16
     |  split old mapping at start            edit.rs:415
     |  split tail at delete_len              edit.rs:398
     |  -> left / right subtrees REUSED by ObjectId
     |     (untouched ranges: not re-chunked, not re-read;
     |      boundary extents SLICED, never re-CDC'd  edit.rs:441)
     |  only the replacement bytes enter FastCdc   gear.rs:54
     v
  middle subtree from new chunk objects          rope/build.rs:209
  (LFS4CHK-framed, <= 32 KiB         extent_codec.rs:10,16)
     |
     v
  concat left + middle + right; coalesce mergeable neighbours
  (validate.rs:81; adjacent same-payload extents are NonCanonical)
     |
     v
  new FileStateV3 { logical_len, extent_count, tree_level,
                    profile_id, mapping_root }    extent.rs:154
     |
     v
  new FileContentRoot -> inode record -> namespace root
                                   filesystem/apply.rs:107
```

Bounds on the mapping tree ([`file/extent.rs:3-6`](../../../../crates/layerfs-content/src/file/extent.rs#L3)):
nodes carry 64–128 entries (root exempt from the minimum), tree level ≤ 31,
node object ≤ 8 192 bytes; read batches ≤ 127 payload objects (≤ 4 MiB)
([`rope/read.rs:11-13`](../../../../crates/layerfs-content/src/file/rope/read.rs#L11)).
`ExtentSliceV3 { payload_object_id, source_offset, logical_length }`
([`extent.rs:8-13`](../../../../crates/layerfs-content/src/file/extent.rs#L8))
references a slice of a chunk payload object's bytes.

### 3.2 Small files — whole-file locality (the live vs retained question)

The policy line is `SMALL_LIMIT = 131_072` (128 KiB) at
[`file/content.rs:8`](../../../../crates/layerfs-content/src/file/content.rs#L8):
files below it become one `LFS5SML` canonical object; files at or above it go
through CDC (a file between the CDC minimum and the small limit that is not
small-eligible still chunks through a rope,
[`objects.rs:3248-3286`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L3248)).
Three distinct 128-KiB constants exist near this boundary and must not be
conflated: `SMALL_LIMIT` (the representation policy line, `content.rs:8`), the
metadata-lane pack limit (also 128 KiB,
[`admission.rs:290-294`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L290)),
and the session candidate-index budget `INDEX_BYTES`
([`small_candidates.rs:4`](../../../../crates/layerfs-layerstack-store/src/objects/small_candidates.rs#L4)).

**Which small-file mechanism is live today — settled from call sites:**

- **Live writer path:** `objects/delta.rs`. Non-test `prepare_small` encodes
  every small object through `super::delta::encode(kind, …)`
  ([`admission.rs:517`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L517))
  into pack v3 (schema 8/9) or compact pack v4 (schema 10) groups — one delta
  record per group, kind 0 = FULL, kind 1 = anchor PREFIX, kind 2 = chain
  PREFIX ([`delta.rs:81-108`](../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L81)).
  A delta is selected only if strictly smaller (`delta.len() + 32 < full.len()`)
  and, for chains, if the encoded closure stays ≤ 256 KiB
  ([`admission.rs:505-513`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L505)).
  Bases come from an explicit predecessor hint or from the session-local,
  content-keyed candidate cache
  ([`small_candidates.rs`](../../../../crates/layerfs-layerstack-store/src/objects/small_candidates.rs#L1)),
  which is live for schema ≥ 9 stores.
- **Retained read-only path:** `objects/whole.rs` (`LFCNT1`, version 107). Its
  module header states "Read compatibility for previously compacted Stores;
  writers exist only in tests" — and the code matches the claim: `encode`,
  `slice`, `assemble` are `#[cfg(test)]`
  ([`whole.rs:120-207`](../../../../crates/layerfs-layerstack-store/src/objects/whole.rs#L120));
  crate-wide search finds zero non-test callers of the writers. The readers
  (`whole_record`, `read_whole`) are live product code, dispatched on the
  `LFCNT1` pack magic and gated on `compact_namespace()` (schema 10)
  ([`objects/read.rs:900-919`](../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L900)).
- **Physical-owner note:** the `LFSWFL1` whole-file object (128 KiB..2 MiB,
  [`content.rs:10-24`](../../../../crates/layerfs-content/src/file/content.rs#L10))
  is an authenticated *physical owner for native chunk slices*, not a logical
  file root; who writes it in production is recorded as unknown in §9.3.

Schema gates decide which representations a Store uses, and a fresh Store at
HEAD is created at `SCHEMA_VERSION = 10`
([`schema.rs:11`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L11),
[`:324-333`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L324)),
so all lanes are on: `native_format ≥ 7`, `small_content_format ≥ 8`,
`small_chain_format ≥ 9`, `compact_framing / compact_namespace ≥ 10`
([`schema.rs:167-184`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L167)).

```text
                 raw file bytes
    len < SMALL_LIMIT (131_072, content.rs:8) ?
  +--------------------+----------------------------------+
  | yes                | no                               |
  v                    v                                  |
  SMALL lane         NATIVE lane (file payloads)          |  ORDINARY / METADATA
  admission.rs:383   admission.rs:574 -> pack v2          |  lanes: pack v1/v5/v6
  pack v3/v4         Full (kind 0) / Prefix (kind 1)      |  instruction-delta
  one delta.rs       zstd records, raw <= 32_768;         |  groups; oversized
  record per group   depth >= 4 or closure > 1 MiB        |  objects -> RAW
  kind 0 FULL        -> NativeFallback::Depth -> FULL     |  singleton packs
  kind 1/2 PREFIX       (admission.rs:699-801)            |  (len+9 > 64 KiB)
  base: explicit predecessor hint (prior_ids[0])          |
     or session Candidates cache (small_candidates.rs:98) |
     accepted iff delta+32 < full (admission.rs:505)      |
  +-------------------+----------------------------------+
                      v
      PreparedAdmission.publish (admission.rs:1248)
        -> object_packs BLOBs + objects locator rows
                      |
                      v
      READ dispatch on pack magic (read.rs:883-919):
        LFPACK v1..v6 -> pack.rs / delta.rs / metadata.rs
        LFCNT1        -> whole.rs READERS ONLY (schema 10);
                         writers are #[cfg(test)] (whole.rs:120)
```

### 3.3 Trees — the canonical namespace graph

A namespace root ties three families together; every level is the same
content-addressing as §2 row 1:

```text
  NamespaceRoot object (LFS4FSR legacy / LFS6FSR compact)
    { profile_id, scope, root_inode, inode_table }   tree/compact.rs:250
        |
        v
  inode table (LFS4INT / LFS6INT)  B+-tree of records
        |  each record: content_root + metadata_root
        |             (tree/inode/record.rs:45-51)
        v
  +------------------+------------------+------------------+
  | directory root   | file content     | metadata root    |
  | (LFS4DIR state   | root: LFS5SML    | (LFS4MET tree;   |
  |  object ->       | inline < 128 KiB |  metadata_value_ |
  |  LFS4NSP nodes;  | or LFS4MAP       |  groups pooled   |
  |  compact: root   |  FileStateV3     |  packs, schema   |
  |  IS the node,    |  extent rope)    |  10)             |
  |  LFS6NSP)        |                  |                  |
  +------------------+------------------+------------------+
   graph edges by inner magic: object/references.rs:26-68
   (leaves — chunk, symlink, small, whole — yield no edges)
```

"Compact" here is a **wire format chosen at namespace construction**
(`store.compact_namespace()`, on for schema-10 stores), not a compaction pass:
inline 73-byte inode records in table leaves, `LFS6*` framing; no function
converts a legacy namespace to compact in this crate, and legacy `LFS4*`
roots remain fully readable (dual-format decode dispatch,
[`tree/inode/table.rs:59-92`](../../../../crates/layerfs-content/src/tree/inode/table.rs#L59)).
Mixing formats inside one merge is refused (`ProfileMismatch` /
`InvalidRecord("directory profile")`).

Canonical-root computation is hashing all the way down: directory entries are
strictly ascending by `CanonicalName` on encode and decode
([`object/codec.rs:303-307`](../../../../crates/layerfs-content/src/object/codec.rs#L303)),
trees are height- and size-canonical (fill ≥ 2/5 of 8 KiB for legacy nodes;
compact leaves 50–100, branches 64–127), and decode re-encodes and
byte-compares against the stored form
([`tree/compact.rs:406-410`](../../../../crates/layerfs-content/src/tree/compact.rs#L406)).

### 3.4 Three-root reconcile

`reconcile_with(store, base_root, branch_root, layer_root)` — the "three
roots" are the three *input* namespace roots (Base / Branch candidate /
Layer current); namespace identity (profile, scope, root inode) must match
across all three ([`filesystem/reconcile.rs:400-429`](../../../../crates/layerfs-content/src/filesystem/reconcile.rs#L400)).
Per-key three-way rule: source==base or source==destination → keep
destination; destination==base → take source; otherwise conflict. Files with
identical kind, ref count, metadata root, length and content digest
auto-merge (`semantic_eq`); divergent link-count changes block that
acceptance. Conflicts are classified `ReconcileConflictKind { Content, Type,
Directory, HardLink }` with all affected paths attached
([`reconcile.rs:97-113`](../../../../crates/layerfs-content/src/filesystem/reconcile.rs#L97)).
Choices are `ReconcileChoice { Branch, Layer, WorkingTree }`; only `Layer`
swaps the rewrite target — in the current engine `WorkingTree` behaves
identically to `Branch` (recorded in §9.3). Resolution state is held in
memory on the active Workspace; nothing durable is written for conflicts
([`workspace/reconcile.rs:109-119`](../../../../crates/layerfs-workspace/src/reconcile.rs#L109)).

### 3.5 Where the invariant can break

The invariant is bounded, not free. Recorded from source, without judgment:

- **Per-file cost floor:** a delta never wins unless strictly smaller
  (`delta + 32 < full`); incompressible small files store FULL.
- **Chain bounds:** small chains cap at 8 edges / 512 KiB canonical / 256 KiB
  encoded — beyond that the writer falls back to FULL
  ([`delta.rs:6-9`](../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L6),
  [`read.rs:669-676`](../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L669));
  native chains cap at depth 4 / 1 MiB closure
  ([`admission.rs:699-704`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L699)).
- **Page growth:** B+-tree node fill bounds (§3.3) keep pages canonical; a
  pathologically alternating workload grows the spine but not the leaves.
- **Lost races are permanent storage:** object admission commits in earlier
  transactions; `retain()` disables rollback before the publication
  transaction, so a CAS loser leaves admitted-but-unreachable objects that
  **no mechanism at HEAD reclaims** (no GC, no compaction, no vacuum — the
  0.1.5 store compaction was removed and nothing replaced it). The Monitor
  reports `unreachable_objects/bytes` = `canonical_storage −
  reachable_storage` ([`dedup.rs:40-72`](../../../../crates/layerfs-monitor/src/dedup.rs#L40)).
  Admission-transaction *failures*, by contrast, roll back exactly (delete
  above `baseline_pack`,
  [`objects.rs:2533-2564`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L2533)).

---

## 4. Codebase map

Workspace members (root [`Cargo.toml`](../../../../Cargo.toml#L3)): the ten
product crates below plus the non-product `benchmark/fs-bench-pro` and
`tools/layerfs-eval`. Measured at the pinned commit (`find … | xargs wc -l`):

| Crate | LOC | Role |
| --- | ---: | --- |
| `layerfs-layerstack-store` | 29 811 | one SQLite Store; durable facts, objects, admission, publication |
| `layerfs-content` | 19 883 | canonical objects, CDC, ropes, trees, Diff, reconcile |
| `layerfs-workspace` | 18 762 | Workspace lifecycle, capture, construction, execution, containers |
| `layerfs-fuse` | 14 669 | FUSE adapter, live owner, sandbox spool, wire |
| `layerfs-daemon` | 4 179 | container execution + FUSE control only |
| `layerfs-workspace-core` | 3 886 | portable live-Workspace core (no I/O, no SQL) |
| `layerfs-cli` | 1 219 | command-line adapter over the SDK |
| `layerfs-sdk` | 1 074 | public Rust API |
| `layerfs-monitor` | 450 | receipts, snapshots, dedup analysis |
| `layerfs-materialization` | 386 | host-directory projection |

Dependency directions, verified from the `Cargo.toml` dependency lists
(these are the rules [`docs/roadmap/0.1/development.md`](../development.md#L56)
states; all three still hold in the tree):

```text
                    CLI (no SQL)                    harness / tooling
                 layerfs-cli                        benchmark/fs-bench-pro
                     |                              tools/layerfs-eval
                     v                                       |
                 SDK layerfs-sdk  (public surface)           | uses SDK
             /       |          \                            |
             v       v           v                           |
   layerfs-workspace |    layerfs-monitor                    |
     |        |      |                                           |
     v        v      +-> (monitor shares the Store/Workspaces   |
  layerfs-   layerfs-     arcs the SDK holds)                   |
  workspace- fuse        v                                     |
  core (portable:        |                                     |
  deps = content only,   | owns LiveOwner + LocalSpool,        |
  #![forbid(unsafe)])    | no Store                            |
     |                   v                                     |
     +-> layerfs-content (SQL-free: deps = blake3 only)        |
                 ^                                           |
                 |  (content has no knowledge of stores)      |
     layerfs-layerstack-store --------------------------------+
     (owns all SQL; no dependency on workspace/fuse/daemon)

  layerfs-daemon: deps = blake3, nix, layerfs-fuse — owns NO Store
  layerfs-materialization: deps = layerfs-workspace (host projection)
```

Product vs harness: everything under `crates/` is product;
`benchmark/fs-bench-pro` (measurement families, `shared/cold.py` cache
contract, verify-selected) and `tools/layerfs-eval` are harness members of
the same workspace but not product crates.

---

## 5. Component inventory

One entry per crate: what it owns, the invariant it carries, what it must not
do, and its key modules.

| Crate | Responsibility | Invariant it owns | Must not | Key modules |
| --- | --- | --- | --- | --- |
| `layerfs-content` | the canonical model: object identity/encoding, file representation (CDC, extents, ropes), namespace trees, Diff/reconcile | same `ObjectId` ⇒ same canonical bytes; strict ordering and canonical encodings make identity deterministic | touch SQL, files, or platforms (deps: blake3 only) | `object/{id,digest,codec,canonical,access,references}.rs`, `file/{cdc,extent,extent_codec,rope,content}.rs`, `tree/{inode,directory,metadata,compact,root,path}.rs`, `filesystem/{reconcile,diff,apply,change,resolve}.rs`, `limits.rs` |
| `layerfs-layerstack-store` | one SQLite Store: schema, statements, staging, admission (packs/deltas/spill), publication, history ops, query, telemetry | visibility-last publication: a head pointer never moves before its objects are durable in committed transactions; commits/layers insert-only | run hashing, traversal, CDC or FUSE I/O inside write transactions | `store.rs`, `schema.rs` + `schema/`, `statements.rs`, `sql/` (51 static files), `staging.rs`, `objects.rs` + `objects/{admission,delta,pack,read,small_candidates,spill,whole,metadata,diagnostic}.rs`, `workspace.rs`, `layerstack.rs`, `branch.rs`, `query.rs`, `records.rs`, `ids.rs`, `telemetry.rs` |
| `layerfs-workspace` | Workspace lifecycle and lease-free sessions, capture, candidate construction, projections, container/execution plumbing, host-side Commit orchestration | a Workspace is disposable — its private overlay and spool are the only mutable state; publication is conditional on expected head/base | bypass the Store's CAS; keep state that must survive End | `lifecycle.rs`, `session.rs`, `changes.rs`, `capture.rs`, `cow_tree.rs`, `reconcile.rs`, `projection.rs`, `snapshot_input.rs`, `live_backing.rs`, `remote_commit.rs`, `registry.rs`, `worker.rs`, `execution.rs`, `output.rs`, `docker.rs`, `docker_engine.rs`, `container.rs`, `daemon.rs`, `file_io.rs` |
| `layerfs-workspace-core` | the portable live-Workspace core: nodes/attrs/pieces, prepared edits, frozen frontier, checkpoints, resource policy | prepare-then-apply: every mutation is prepared without state change and applied exactly once; stale prepared edits rejected | any I/O, SQL, `unsafe`, or platform code (single dep: `layerfs-content`) | `lib.rs`, `namespace.rs`, `file_edit.rs`, `frozen.rs`, `checkpoint.rs`, `backing.rs`, `limits.rs` |
| `layerfs-fuse` | FUSE projection: adapter/port, per-Workspace live owner, immutable read cache, sandbox spool, the live wire | write-before-apply (a piece is visible only after its bytes are physically readable); no durability flush is ever issued | mutate host state per operation; claim durability | `adapter.rs`, `filesystem.rs`, `port.rs`, `inode_table.rs`, `handles.rs`, `immutable_read_cache.rs`, `local_spool.rs`, `live_owner.rs`, `live_runtime.rs`, `live_transport.rs`, `live_wire.rs`, `host_mount.rs`, `write_metrics.rs`, `protocol.rs`, `proxy_{client,host}.rs` (retained/test-only) |
| `layerfs-daemon` | inside-container control: one owner per daemon, per-Workspace mounts, process-group exec, cgroup sampling | capability knowledge is the only credential; a second client can never take over a live daemon | own a Store, run a shell, keep payload caches | `main.rs`, `lib.rs`, `protocol.rs` |
| `layerfs-sdk` | the public Rust surface; one `Client` per Store+Workspaces, monitor-accounted | every public operation is observed and receipted | expose raw SQL, daemon frames, or canonical-object construction | `client.rs`, `request.rs`, `result.rs`, `query.rs` |
| `layerfs-cli` | command-line adapter; hand-rolled parser; context-owner daemon on unix | CLI grammar stays a strict subset of the SDK surface | embed SQL or Store internals | `lib.rs`, `runtime.rs` |
| `layerfs-monitor` | receipts (ring ≤ 512), snapshots, dedup analysis | candidate conservation: candidates = inserted + reused, validated on record | store unbounded history; make measurement claims | `collector.rs`, `operation.rs`, `snapshot.rs`, `dedup.rs` |
| `layerfs-materialization` | host-directory projection (write-out + capture) | materialized bytes capture back to the same canonical identity | fsync or mutate the Store | `materialize.rs`, `capture.rs`, `port.rs` |

Notable inventory facts against
[`docs/roadmap/0.1/development.md`](../development.md#L44), which still lists
nine crates and omits `layerfs-workspace-core` (split out 2026-09-06): the
tenth crate exists, is depended on by both `layerfs-workspace` and
`layerfs-fuse`, and is the portable seam the v0.1.6 sandbox architecture
runs on. See §9.1.

## 6. Key algorithms

Each entry: entry point, bounded input/output, the invariant, source.

### 6.1 Canonical identity and authentication

- Entry: `Object::id()` / `ObjectId::for_bytes`
  ([`object/digest.rs:69-88`](../../../../crates/layerfs-content/src/object/digest.rs#L69));
  read gate: `authenticate()` at
  [`objects/read.rs:2040-2047`](../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L2040)
  (length equality + recomputed digest), reached by every read route — full
  records, delta bases and outputs, singleton blobs, SmallContent, whole-file
  (`whole.rs:323-399`), native chains.
- Bounds: object ≤ 16 MiB, field ≤ 8 MiB, child references ≤ 100 000, name ≤
  255 bytes ([`content/limits.rs:2-6`](../../../../crates/layerfs-content/src/limits.rs#L2)).
- Invariant: an `ObjectId` commits to the complete canonical encoding —
  structure and length included. Where a hash is **not** the identity:
  `InodeId` (BLAKE3 over `layerfs/inode-id/v1\0` + store id + serial) names an
  allocation slot and authenticates nothing — the inode's content is guarded
  by the `content_root`/`metadata_root` ObjectIds inside the record
  ([`tree/inode/record.rs:5-50`](../../../../crates/layerfs-content/src/tree/inode/record.rs#L5));
  `Location { pack, group, record }` is a physical address that must be
  corroborated by the digest check; `ContentDigestWriter`
  (`layerfs/content-bytes/v1\0`) is a separate domain for logical stream
  digests and fingerprints, never stored as an ObjectId.

### 6.2 CDC chunking and the splice edit — §3.1–3.2 above.

### 6.3 Candidate construction and admission

- Entry: `build_frontier_candidate` /
  `build_frontier_candidate_with_workers`
  ([`changes.rs:344-349`](../../../../crates/layerfs-workspace/src/changes.rs#L344))
  → `LayerStackStore::construct_workspace_files`
  ([`objects.rs:4487`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L4487)).
- Dirty set: frozen per operation (`FrozenWorkspaceChanges`,
  [`workspace-core/lib.rs:125-153`](../../../../crates/layerfs-workspace-core/src/lib.rs#L125))
  — "maps may contain only dirty nodes and children named by their directory
  deltas"; no complete namespace manifest is built for Commit (manifests
  exist only for the reconciliation fingerprint).
- Worker model: `construction_worker_limit()`
  ([`changes.rs:572-583`](../../../../crates/layerfs-workspace/src/changes.rs#L572))
  = `LAYERFS_CONSTRUCTION_WORKERS` env (honoured in 1..=8), default
  `available_parallelism().min(8)`; further capped by task count, an
  I/O-derived budget, and `1` for predecessor-bearing plans
  ([`changes.rs:685-688`](../../../../crates/layerfs-workspace/src/changes.rs#L685));
  small-content construction capped at `SMALL_CONTENT_WORKERS = 4`
  ([`objects.rs:48`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L48));
  one process-wide construction gate for host Commit builds
  ([`changes.rs:585-592`](../../../../crates/layerfs-workspace/src/changes.rs#L585)).
  (AGENTS.md §3.8 additionally requires benchmark runs to export
  `LAYERFS_CONSTRUCTION_WORKERS=1`; the code default remains parallel —
  recorded in the hand-off list.)
- Spill: candidate sets spill to private temp files at 6 MiB memory / 64 MiB
  index charge; a failed spill latches permanently
  ([`objects/spill.rs:6-147`](../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L6),
  [`objects.rs:49-58`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L49)).
- Admission bounds: ≤ 8 191 objects / ≤ 4 MiB−1 bytes per SQL cohort; prepared
  batches ≤ 512 objects / 4 MiB−1; oversized objects become RAW singleton
  packs. Deliberately outside write transactions: hashing, CDC, encoding,
  reconciliation traversal, absence probes, delta search, spill
  ([`objects/admission.rs:183-333`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L183)).
- Double admission: membership check → byte-equal objects are skipped and
  counted `reused`; same id with different bytes → `Integrity("object
  collision")`
  ([`admission.rs:1871-1933`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1871)).

<!-- CONTINUED -->

