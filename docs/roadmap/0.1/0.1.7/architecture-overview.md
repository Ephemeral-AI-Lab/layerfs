# LayerFS architecture overview — v0.1.7 exploration study

> Status: Research; informative and not a product contract.

This is the source-bound study requested by
[#156](https://github.com/Ephemeral-AI-Lab/layerfs/issues/156). It describes
what exists; it does not select architecture, freeze scope, or authorize work.
Observations that imply work are recorded in the hand-off list at the end and
belong to the v0.1.7 checklist, not to this document.

- **Source pin:** every citation below was read at commit
  `40af8529f47dca75e198b312ab2e494ff768a625`. HEAD advanced twice while the
  study ran (first to `87ed92df035075750fcac7e3d49ce7801e808849`, then to
  `6bcc1468e46c2615f57b86f2e4cd4db4b93d0c30`); the cumulative diff from the
  pin touches **no file under `crates/`** — only `docs/roadmap/0.1/0.1.6*`
  evidence/checklists and `benchmark/fs-bench-pro/` harness files — so every
  citation holds identically at the pin and at current HEAD. (Transparency
  note: one intermediate commit, `497138431`, incidentally swept a partial
  draft of this document into the tree alongside unrelated #154 changes;
  this finished document supersedes that snapshot and is the authoritative
  version.)
- **Method:** source reads only — no builds, tests, benchmarks or code changes.
  Evidence was gathered by read-only exploration subagents (13 breadth slices,
  3 depth follow-ups, 2 adversarial checks) and every load-bearing claim below
  was re-opened and verified by the coordinating author. Claims that could not
  be verified are marked `unknown`, not guessed.

---

## 1. Product identity — where the architecture gets its inspiration

The pitch, verbatim from [`README.md`](../../../../README.md#L26):

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
([`README.md`](../../../../README.md#L28), repeated in
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
| [`README.md`](../../../../README.md) | shipped language (v0.1.5 developer preview) — highest authority for product identity | §1's pitch and obligations, quoted verbatim |
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
| 3 | Commit immutable, base unit | `commits` and `layers` rows are insert-only — no product statement ever UPDATEs or DELETEs them; the only pointer-movement statements are the two head CASes (`advance_branch`, `advance_head`); the only other product writes to existing rows are admission-internal (the pack-append UPDATE, `objects.rs:2403`, and the inode-serial upsert, `sql/schema/reserve_inode_serials.sql:5`); object/pack DELETEs happen only on the admission-rollback path (§3.5) plus the stage delete. `CommitId` = tag `0x12` + BLAKE3(root, parent, base); insert is `ON CONFLICT DO NOTHING` with a byte-equal collision check | [`sql/workspace/advance_branch.sql`](../../../../crates/layerfs-layerstack-store/sql/workspace/advance_branch.sql#L5), [`ids.rs`](../../../../crates/layerfs-layerstack-store/src/ids.rs#L108), [`sql/workspace/insert_commit.sql`](../../../../crates/layerfs-layerstack-store/sql/workspace/insert_commit.sql#L5), [`records.rs`](../../../../crates/layerfs-layerstack-store/src/records.rs#L84) | verified |
| 4 | CAS + small delta + large CDC + COW | small files (< 128 KiB) become one `LFS5SML` object stored in pack v3/v4 groups with bounded zstd FULL/PREFIX delta records and chains; large files CDC-chunk (8/16/32 KiB frozen gear profile) into extent ropes; edits splice the rope and re-chunk only the replacement; Workspaces overlay a private piece tree over immutable base roots | [`file/content.rs`](../../../../crates/layerfs-content/src/file/content.rs#L8), [`objects/admission.rs`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L517), [`file/cdc/gear.rs`](../../../../crates/layerfs-content/src/file/cdc/gear.rs#L7), [`file/rope/edit.rs`](../../../../crates/layerfs-content/src/file/rope/edit.rs#L69) | verified |
| 5 | fork zero-copy | `fork_branch` = one `INSERT` into `branches` (base Layer + optional head Commit); no object admission, no tree build, no payload copy | [`branch.rs`](../../../../crates/layerfs-layerstack-store/src/branch.rs#L51), [`sql/branch/insert.sql`](../../../../crates/layerfs-layerstack-store/sql/branch/insert.sql#L5) | verified |
| 6 | roll back by forking | forking from a Commit requires that Commit to be in Branch ancestry (bounded recursive CTE, depth ceiling 1,000,000); there is no rewind/move-head-back operation — history is only extended | [`branch.rs`](../../../../crates/layerfs-layerstack-store/src/branch.rs#L23), [`sql/branch/contains_commit.sql`](../../../../crates/layerfs-layerstack-store/sql/branch/contains_commit.sql#L10) | verified |
| 7 | promote with `Add` | `add_layer` checks idempotence/staleness/no-change, then one transaction: `INSERT_LAYER` (new Layer's `root_id` **is** the head Commit's `root_id` — a re-labeling, zero object copies) + `UPDATE layer_stacks … WHERE head_layer_id = expected` CAS. Outcomes: `Added / UpToDate / NoChanges / HeadMoved` | [`layerstack.rs`](../../../../crates/layerfs-layerstack-store/src/layerstack.rs#L210), [`sql/layerstack/advance_head.sql`](../../../../crates/layerfs-layerstack-store/sql/layerstack/advance_head.sql#L5) | verified |
| 8 | cost-of-change invariant | every state is complete logically, incremental physically; `canonical_storage` counts all admitted objects, `reachable_storage` walks layer/commit/branch roots; the difference (lost-race orphans) is measured by the Monitor and is **never reclaimed — no GC exists** (§3.5) | [`sql/query/canonical_storage.sql`](../../../../crates/layerfs-layerstack-store/sql/query/canonical_storage.sql#L5), [`query.rs`](../../../../crates/layerfs-layerstack-store/src/query.rs#L297), [`monitor/dedup.rs`](../../../../crates/layerfs-monitor/src/dedup.rs#L40) | verified (consequence recorded, not judged) |

Rows dropped during verification are recorded in §9.4 with the reason.

---

## 3. Storage model — the small-file / large-file split

The governing invariant, in the shipped language of
[`README.md`](../../../../README.md#L52): *every filesystem state is
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
   (leaves -- chunk, symlink, small, whole -- yield no edges)
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

  layerfs-daemon: deps = blake3, nix, layerfs-fuse -- owns NO Store
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
- Initialization is the one multi-worker construction lane (AGENTS.md §3.8
  exempts it): `initialize_layerstack` builds Layer 0 from `Empty` or from a
  host `Directory` import (parallel
  `direct_initialize_root_directories_inner`, serial fallback); the
  namespace seed is `BLAKE3(layer_stack_id)` — overridable for benchmarks
  via `LAYERFS_BENCH_INITIALIZATION_SEED_HEX` — and it feeds `InodeId`
  allocation and the compact scope, so identical content yields different
  inode ids in different stacks
  ([`layerstack.rs:21-104`](../../../../crates/layerfs-layerstack-store/src/layerstack.rs#L21),
  [`filesystem/root.rs:10-16`](../../../../crates/layerfs-content/src/filesystem/root.rs#L10)).

### 6.4 Publication — visibility-last compare-and-swap

```text
 construction (no transaction)      admission: one tx per cohort
+----------------------------+    +----------------------------------+
| hash / CDC / encode /      |    | AdmissionSession objects.rs:2241 |
| reconcile / spill          |--> | BEGIN IMMEDIATE objects.rs:2334  |
| objects/admission.rs:183-  |    | INSERT object_packs + objects     |
| 333                        |    | (generated SQL admission.rs:1546,|
+----------------------------+    | 1602) COMMIT per bound/cohort    |
                                  | bound: 8191 objs / 4 MiB-1        |
+----------------------------+    +-----------------+----------------+
| staging.rs:24-52           |-->| final batch + publication tx      |
| stage row (own Immediate   |   | (workspace.rs:493 / :305-335):   |
| tx) then session.retain()  |   |   INSERT_COMMIT (idempotent)      |
| objects.rs:2484 (rollback  |   |   ADVANCE_BRANCH (CAS, ==1 row)   |
| DISABLED from here)        |   |   DELETE workspace_stages         |
+----------------------------+   |   COMMIT workspace.rs:593        |
                                 +-----------------+----------------+
                                                   |
                            CAS wins              CAS loses
                              |                      |
                              v                      v
                     head -> commit.root      CommitHeadMoved; pointer tx
                     (every object already   rolls back, but retained
                      durable & reachable)    admission-batch objects stay
                                              (permanent orphans, S3.5)
```

- `advance_branch` CAS:
  `UPDATE branches SET head_commit_id=?2, base_layer_id=?4 WHERE branch_id=?1 AND head_commit_id IS ?3 AND base_layer_id=?5`
  ([`sql/workspace/advance_branch.sql:5`](../../../../crates/layerfs-layerstack-store/sql/workspace/advance_branch.sql#L5))
  — `IS` matches the NULL head of a freshly forked Branch.
- Outcomes: `Created / UpToDate / Busy / HeadMoved`
  ([`session.rs:293`](../../../../crates/layerfs-workspace/src/session.rs#L293));
  `Busy` comes from active executions/writers (per-Worker admission,
  [`worker.rs:89-153`](../../../../crates/layerfs-workspace/src/worker.rs#L89)),
  not from a Branch lease — the lease is dead code at HEAD (§9.2).
- Why visibility-last: earlier batches may leave unreachable objects after a
  lost race, but a head can never point at a non-durable or half-visible
  root. The unstaged `commit_candidate` path is fully atomic (final batch +
  commit row + CAS in one transaction — no orphan leak on that path;
  [`workspace.rs:326-396`](../../../../crates/layerfs-layerstack-store/src/workspace.rs#L326)).
- Failure politics around the CAS: if an admission rollback's
  delete-above-baseline cleanup itself fails, the whole Store is
  **quarantined** — every later write fails until reopen
  ([`schema.rs:405-430`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L405),
  [`objects.rs:2579-2599`](../../../../crates/layerfs-layerstack-store/src/objects.rs#L2579));
  and every public Store operation (fork, Add, staging, admission)
  serializes through one FIFO `TicketGate` with disconnect-all-on-poison
  semantics — the actual ordering contract between concurrent Commit/Add/
  fork ([`schema.rs:96-151`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L96)).

### 6.5 Workspace lifecycle, capture and the Commit cut

- States and transitions: §2 row 2; Commit requires `Active`; a successful
  Commit **re-enters `Active`** (a Workspace is re-committable); Clean End
  refuses dirty state (`WorkspaceDirty`); Discard → `Ended`; cleanup failure
  → `BrokenCleanup`, which blocks later End
  ([`lifecycle.rs:933-1038`](../../../../crates/layerfs-workspace/src/lifecycle.rs#L933)).
- The Commit cut is two-sided. On the sandbox side the cut is non-pausing:
  the sandbox keeps serving operations while the frozen frontier is owned by
  the capture
  ([`frozen.rs:36-55`](../../../../crates/layerfs-workspace-core/src/frozen.rs#L36));
  first post-capture mutation of a frontier node copies its capture-time
  value (`protect`, [`frozen.rs:98-108`](../../../../crates/layerfs-workspace-core/src/frozen.rs#L98)).
  On the FUSE side the same cut is a two-phase `OperationGate`: kernel
  callbacks take a read lock on one of two lanes; capture drains ordinary
  callbacks while **still admitting writeback** (so the kernel can launder
  dirty pages during invalidation), then `finish()` takes both write locks;
  lifecycle frames get dedicated admission so Begin/End progress while
  filesystem callbacks are parked; Release/Releasedir bypass the gate so
  handle release can never starve
  ([`live_runtime.rs:307-373`](../../../../crates/layerfs-fuse/src/live_runtime.rs#L307),
  [`filesystem.rs:79-103`](../../../../crates/layerfs-fuse/src/filesystem.rs#L79),
  [`live_owner.rs:1555-1629`](../../../../crates/layerfs-fuse/src/live_owner.rs#L1555)).
- Three capture paths exist, per projection route — do not conflate them:
  (a) `capture.rs`: per-file streaming capture of sequential writes-from-zero
  (a new file written straight through never needs to be re-read from the
  spool, [`capture.rs:49-96`](../../../../crates/layerfs-workspace/src/capture.rs#L49));
  (b) the frozen **frontier** (`frozen.rs`) — the sandbox/host FUSE routes;
  (c) the materialized route Commits by **re-reading the projected host
  directory** — a `matches()` fast-path, then a localized patch (namespace
  walk with dev/ino hard-link identity), else a full re-read into a
  `clean_copy` swapped in under the workspace lock
  ([`projection.rs:604-688`](../../../../crates/layerfs-workspace/src/projection.rs#L604)).
- Projection defaults are platform-dependent: `Fuse` for Container placement
  or Linux, `Materialize` otherwise (a macOS host Workspace defaults to
  materialization); a third route — Linux host-FUSE (default `host-fuse`
  feature) — mounts in-process with no container and no daemon
  ([`lifecycle.rs:482-492`](../../../../crates/layerfs-workspace/src/lifecycle.rs#L482),
  [`Cargo.toml:9`](../../../../crates/layerfs-workspace/Cargo.toml#L9),
  [`host_mount.rs:88`](../../../../crates/layerfs-fuse/src/host_mount.rs#L88)).

### 6.6 Snapshot input and completion — the v0.1.6 sandbox Commit path

```text
 SANDBOX (layerfs-fuse LiveOwner)         HOST (layerfs-workspace)
+-----------------------------------+   +-----------------------------------+
| LiveWorkspace (core lib.rs:423)   |   | BackingOwner (live_backing.rs:19)|
|  private: dir deltas, piece trees |   |  immutable base from the Store:  |
|  shared: Base roots (read-only)   |   |  SEED / LOOKUP / DIRECTORY_PAGE  |
|  payload appends -> LocalSpool    |   |  READ_BASE (sandbox asks, on     |
|  capture_frontier (frozen.rs:36)  |   |  cache miss only)                |
+-----------------+-----------------+   +----------------+------------------+
                  |  'c' control lane:                   |
                  |  CAPTURE -> SnapshotToken{incarnation,
                  |    attempt, generation} (live_owner.rs:2542)      |
                  |                                      v
                  |  's' snapshot lane (host PULLS):     commit_remote
                  |   SNAP_RECORDS 64-KiB pages       (remote_commit.rs:38)
                  |   SNAP_READ <= 1 MiB / 8 MiB window  build_remote_candidate
                  |   (snapshot_input.rs:267-319)        (single producer)
                  |                                      publish_prepared ->
                  |  COMPLETE_BEGIN/NODE/END             Store CAS (S6.4)
                  |  (idempotent, revision-matched;      complete_generation ->
                  |   frozen.rs:133 install_covered_record)
                  |                                      rebase_host_workspace
                  +-------------------------------------- (remote_commit.rs:536)
   Mutations NEVER cross the wire per-op; payload crosses only at Commit.
```

- What still communicates outside Commit: SEED at connect; LOOKUP /
  DIRECTORY_PAGE / READ_BASE on cache miss ('d' lane, sandbox→host);
  control ops FREEZE/RESUME/INVALIDATE/metrics and host-initiated SDK edits
  (EDIT_* transactions) ([`live_wire.rs:578-603`](../../../../crates/layerfs-fuse/src/live_wire.rs#L578)).
- Failure: `SNAP_CANCEL` or exact-transaction retry of the same COMPLETE
  records; a failed Commit publishes nothing and leaves a resolvable slot
  ([`live_owner.rs:2584-2603`](../../../../crates/layerfs-fuse/src/live_owner.rs#L2584)).

### 6.7 FUSE projection internals

- Inode model: FUSE ino **is** `NodeId` (monotonic per mount, root = 1,
  never reused); each live node carries its canonical `InodeId` separately
  ([`inode_table.rs:7-17`](../../../../crates/layerfs-fuse/src/inode_table.rs#L7));
  handles own only `{node, writable}`. Dual lifetime mode: when the port
  supports kernel lifetime and the kernel advertises
  `FUSE_NO_OPEN_SUPPORT`, opens bypass the handle table (`fh = 0`), entries
  carry `KernelReferences`, and `forget`/`destroy` propagate kernel
  reference drops — a `KernelReferences` RAII converts unemitted
  readdirplus entries and failed replies into forgets
  ([`filesystem.rs:33-74`](../../../../crates/layerfs-fuse/src/filesystem.rs#L33),
  [`port.rs:63-123`](../../../../crates/layerfs-fuse/src/port.rs#L63),
  [`adapter.rs:85-126`](../../../../crates/layerfs-fuse/src/adapter.rs#L85));
  reference-accounting changes must hold in **both** modes.
- Entry TTL 1 s, revalidation by kernel re-lookup; immutable facts enter the
  cache only after passing immutability validation
  ([`live_owner.rs:939-963`](../../../../crates/layerfs-fuse/src/live_owner.rs#L939)).
- `ImmutableReadCache`: three immutable families (content ranges /
  authenticated positive name facts / validated directory pages), 32 MiB
  budget, FIFO, per-owner scopes; a miss falls through to the host, never an
  error ([`immutable_read_cache.rs:8-16`](../../../../crates/layerfs-fuse/src/immutable_read_cache.rs#L8)).
- `LocalSpool`: packed 1 MiB segments; write-before-apply; **no durability
  flush is ever issued**; a bounded 4-segment resident window is re-offered
  with `POSIX_FADV_DONTNEED` and served ranges are evicted — the #151 fix
  that keeps sandbox memory flat regardless of payload
  ([`local_spool.rs:12-24,72-106,258-289`](../../../../crates/layerfs-fuse/src/local_spool.rs#L12)).
- fsync is a volatile sync contract: it validates state, surfaces ENOSPC for
  allocation overruns, retires idle segments — and performs no wire traffic
  ([`live_owner.rs:1908-1937`](../../../../crates/layerfs-fuse/src/live_owner.rs#L1908));
  FLUSH replies ok
  ([`filesystem.rs:855-893`](../../../../crates/layerfs-fuse/src/filesystem.rs#L855)).
- Deliberate divergences from materialization: synthetic statfs, pinned
  uid/gid, `mknod` regular-files-only, RENAME_EXCHANGE/WHITEOUT unsupported,
  `create` replies `FOPEN_DIRECT_IO`
  ([`filesystem.rs:218-1431`](../../../../crates/layerfs-fuse/src/filesystem.rs#L218)).

### 6.8 Execution and the capability-authenticated daemon

- Fresh process per execution: new `ExecutionId`, `argv[0]` exec'd directly
  (never a shell), process group + pid file ownership, TERM to the whole
  group on stop
  ([`layerfs-daemon/main.rs:1171-1322`](../../../../crates/layerfs-daemon/src/main.rs#L1171),
  [`execution.rs:399-464`](../../../../crates/layerfs-workspace/src/execution.rs#L399)).
- Bounded output: 1 MiB tail, oversized writes truncated + flagged
  ([`output.rs:8,75-113`](../../../../crates/layerfs-workspace/src/output.rs#L8)).
- Capability: 32-byte secret at `/run/layerfs/capability` (0600), BLAKE3
  keyed proofs with fresh nonces — mutual on unix (peer pid/uid/gid bound),
  payload-bound on TCP streams so proofs cannot be replayed across requests;
  constant-time compares; first authenticated stream becomes the single
  owner, and owner loss terminates execs and cancels mounts
  ([`protocol.rs:380-550`](../../../../crates/layerfs-daemon/src/protocol.rs#L380),
  [`main.rs:325-491`](../../../../crates/layerfs-daemon/src/main.rs#L325)).
- The daemon owns no Store (no `layerfs-layerstack-store` dependency), no
  shell, no payload cache; its `/snapshots/<workspace>` dir is a transient
  per-mount spool destroyed on close
  ([`main.rs:1499-1507`](../../../../crates/layerfs-daemon/src/main.rs#L1499)).

### 6.9 Observation and bounds

- One Monitor per SDK `Client`, ring ≤ 512 `OperationReceipt`s; receipts
  carry outcome, timing, and `CandidateStats` (inserted/reused/batch/final)
  whose conservation equation is **validated on record**
  ([`operation.rs:56-88`](../../../../crates/layerfs-monitor/src/operation.rs#L56));
  dedup analysis recomputes from receipts + Store traversal on demand
  ([`dedup.rs:36-74`](../../../../crates/layerfs-monitor/src/dedup.rs#L36)).
- Product signals vs diagnostics: outcomes/timing/candidate stats are public
  (`monitor_snapshot`, `analyze_dedup`, QueryKind::Monitor); ~200 `diag_*`
  counters and per-op write metrics are diagnostics riding `StorageReceipt`s,
  explicitly non-attributable
  ([`telemetry.rs:5-15`](../../../../crates/layerfs-layerstack-store/src/telemetry.rs#L5)).
- Limits, hard vs advisory (verified enforcement sites): hard — content
  object/path limits, admission batch bounds (8 191 / 4 MiB−1 / 512), read
  pages (128 ids), pack framing (256 KiB / 256 groups / 8 191 records /
  64 KiB groups — hard in assemblers/parsers), delta frame limit (132 KiB),
  chain bounds (hard on read, advisory fallback-to-FULL on write), query
  pages (512 / 128), spool policy (1 GiB, pre-append), final delta (8 MiB),
  lifecycle timing equation; advisory — spool resident window (fadvise),
  snapshot cache (skip-not-error), comparison-reuse cache, fresh-admission
  filter, candidate index spill (memory→disk transition), diagnostic
  budgets. One dead declaration: `MAX_DECODE_NESTING_DEPTH` has no
  enforcement site anywhere (§9.2).

### 6.10 Store open, compatibility and the only migration path

- Connect accepts `user_version` ∈ {6, 7, 8, 9, 10} only, rejects WAL stores
  and v6 stores containing research-era native packs, and **never mutates
  the file on refusal**
  ([`schema.rs:469-508`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L469)).
- Opened stores are **version-frozen**: a schema-9 store reopens at 9 and
  keeps writing its old lane (pack v3) — "nonpromoting after reopen"
  ([`schema/compatibility.rs:190-214`](../../../../crates/layerfs-layerstack-store/src/schema/compatibility.rs#L190)).
  Format lanes are therefore chosen per store, forever.
- The only migration routine in the tree is the offline
  `LayerStackStore::upgrade_format`, promoting 7/8 → **9** (never to 10),
  under DELETE-journal + synchronous FULL + EXCLUSIVE locking, fail-closed
  even when the commit error arrives after promotion
  ([`schema.rs:975-1021`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L975)).
  There is no in-place upgrade to v10 and no upgrade from v6.
- Single-process Store ownership is enforced by `locking_mode=EXCLUSIVE`
  plus an Immediate transaction at open
  ([`schema.rs:526-540`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L526)).

A migration consequence worth stating plainly: planning "upgrade all
stores to v11" from this document would be planning a mechanism that does
not exist — any new schema version needs a new migration routine, and the
existing one stops at 9.

---

## 7. Runtime topology

```text
+------------------------------------------------------------------+
| HOST process (SDK Client / CLI context owner)                    |
|  LayerStackStore (SQLite, journal=MEMORY, synchronous=OFF)       |
|  Workspaces registry + construction (changes.rs)                 |
|  BackingServer: TCP listener, capability + role byte             |
|    lanes: 'd' data (sandbox asks)  'c' control (host asks)       |
|           's' snapshot (host pulls at Commit) 'o' observer       |
+-----------------------^------------------------------------------+
                        | sandbox is always the TCP client
                        | (live_transport.rs:45-52,636-646)
+------------------------------------------------------------------+
| CONTAINER (per Workspace)                                        |
|  mount route A (docker.rs:82-227): one layerfs-fuse helper       |
|    process per Workspace (ATTACH_SCRIPT, /dev/fuse, identity-    |
|    checked cleanup: pid+start-time+/proc/pid/exe+owned env)      |
|  mount route B (daemon.mount, docker.rs:229-282): in-process     |
|    mount inside layerfs-daemon (LiveRuntime::shared + mount_host,|
|    main.rs:1396-1413)                                            |
|  LiveOwner owns ALL mutable Workspace state:                     |
|    LiveWorkspace nodes/pieces, kernel refs, snapshot slot,       |
|    LocalSpool payload (1-MiB segments, no fsync ever)            |
|  execution: fresh process per exec, process groups, 1-MiB output |
+------------------------------------------------------------------+
  What crosses the boundary:
    'd'  SEED, LOOKUP(_METADATA), DIRECTORY_PAGE, READ_BASE
         (immutable base facts, on cache miss)
    'c'  CAPTURE / SNAP_CANCEL / COMPLETE_* / SHUTDOWN /
         FREEZE / RESUME / INVALIDATE / EDIT_* / metrics
    's'  SNAP_RECORDS (64-KiB pages), SNAP_READ (<= 1 MiB frames)
  What is deliberately NOT transferred:
    per-operation mutations (create/write/rename/unlink stay in the
    sandbox until Commit), the Store itself (never enters the
    container), POSIX host-side edits (the wire carries "resolved
    facts, never host-side POSIX edits", live_wire.rs:1), and any
    durability claim (no flush is ever issued on the backing path).
```

Where mutable state lives: **all mutable Workspace state is sandbox-local**
([`live_owner.rs:1-40`](../../../../crates/layerfs-fuse/src/live_owner.rs#L1));
the host holds immutable base + canonical construction + accepted history.
Docker lifecycle: `docker create --device /dev/fuse --cap-add SYS_ADMIN
--pids-limit --memory --cpus` — never privileged; defaults 512 MiB / 2 CPUs /
512 pids ([`container.rs:102-133`](../../../../crates/layerfs-workspace/src/container.rs#L102)).

Durability boundary (stated by the repo itself,
[`docs/versioned/0.1.5/limitations.md:13-16`](../../../versioned/0.1.5/limitations.md#L13)):
process-crash/OS-crash/power-loss guarantees are outside the
MEMORY-journal/synchronous-OFF profile; fsync does not upgrade database
durability. The sole durability-fenced transaction is inode-serial
reservation (temporarily DELETE-journal + synchronous FULL,
[`schema.rs:216-242`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L216));
the sole product-side `sync_data` call is the execution-output log rewrite
([`output.rs:224-230`](../../../../crates/layerfs-workspace/src/output.rs#L224)).
The host route is equally flush-free: spool segments are created then
immediately unlinked (held only by fds), and `Workspace::fsync` is
`#[cfg(test)]`-only ([`file_io.rs:128-137,548-584`](../../../../crates/layerfs-workspace/src/file_io.rs#L128)).

### 7.1 The concurrency substrate under the wire

One process-shared `LiveRuntime` (a 2-worker + 2-blocking-thread runtime)
serves **all** mounts, backing servers and the daemon route; every wire
frame and kernel callback is admitted against per-lane semaphore budgets:
ordinary 256 requests / 32 MiB, control 32 / 4 MiB, lifecycle 2 / 8 MiB,
snapshot 4 / 8 MiB, `backing` 2 physical jobs, `kernel` 1, `live` 128 MiB —
so "a stalled bulk transfer can neither consume ordinary request capacity
nor block control/lifecycle service"
([`live_runtime.rs:11-15,62-106,174-302`](../../../../crates/layerfs-fuse/src/live_runtime.rs#L11),
[`adapter.rs:24-29`](../../../../crates/layerfs-fuse/src/adapter.rs#L24)).
Backing handlers run through `physical()` so admission precedes Tokio's
blocking queue. These budgets — not the wire itself — are the host-side
memory bound of the whole sandbox architecture, and §6.5's operation cut is
implemented on this gate.

Transport lifecycle contracts worth knowing before touching the wire:
control/snapshot requests time out at 120 s, capability presentation at
10 s, FUSE mount-ready at 10 s, SQLite busy at 5 s, writer/quiesce drain at
1 s; the host obtains the daemon capability by `docker cp`-pulling
`/run/layerfs/capability` into a 0600 file under `runtime_root` (the secret
transits host disk; deleted on `remove`, left behind on `stop`); the
`BackingServer` binds `0.0.0.0` — all interfaces, capability-gated, with
single-occupancy role slots and ≤ 3 admitted connections (not
loopback-only); and any lane serve failure latches the server `failed`, so
the next capture fails the Commit with the recorded request rather than
retrying in place
([`live_transport.rs:51-331`](../../../../crates/layerfs-fuse/src/live_transport.rs#L51),
[`container.rs:206-264`](../../../../crates/layerfs-workspace/src/container.rs#L206),
[`docker.rs:327-333`](../../../../crates/layerfs-workspace/src/docker.rs#L327)).

---

## 8. Load-bearing read — what is important and what is not

Criteria (stated before the ranking; an item can hold more than one):

- **C1 carries a correctness invariant** — removing or weakening it corrupts
  data or history;
- **C2 is a contract surface** — an external or durable interface others
  already depend on;
- **C3 is an internal seam v0.2 will restructure** — the 0.2 work
  (multi-agent Branch reconciliation, projection conformance) lands here;
- **C4 is movable plumbing** — reshufflable as long as its bounds move with
  it;
- **C5 is harness-only or retained** — exists for tests, history, or
  measurement; a migration should not spend risk budget here.

| Rank | Item | Criteria | What breaks if you touch it |
| --- | --- | --- | --- |
| 1 | Canonical identity + authentication (`object/{digest,codec,id}.rs`, `authenticate` in `objects/read.rs`) | C1, C2 | Every stored object's identity changes or stops being verified; the Store becomes unreadable and all history would need re-minting. The root everything else hangs from |
| 2 | Publication CAS + visibility-last ordering (`advance_branch`/`advance_head` SQL, admission publish, staging) | C1, C2 | A head can point at incomplete closure, or a lost race is silently absorbed — torn history and lost updates without any error |
| 3 | Store schema v10 + format gates + compatibility (`sql/schema/`, `schema.rs`) | C2 | Every existing Store fails preflight or silently misreads; the version-gated lanes pick wrong representations per store. And the migration contract is narrow: stores are version-frozen at creation, connect never mutates, and the only migration routine is offline 7/8→9 (§6.10) — a "migrate everything to v11" plan has no mechanism to build on |
| 4 | CDC profile + extent/rope codec (`file/cdc/gear.rs`, `extent.rs`, `extent_codec.rs`) | C1, C2 | `profile_id` drift → `ProfileMismatch` on every stored `FileStateV3`; chunk identity drift breaks dedup across files and history |
| 5 | Small-file delta lane + pack formats v2–v6 (`objects/{delta,pack,admission}.rs`) | C1, C2 | The live small-file mechanism: storage blows up (always-FULL) or reads fail `Integrity`; pack framing is shared by every lane |
| 6 | Snapshot-input/complete protocol + frozen frontier (`live_wire.rs`, `snapshot_input.rs`, `remote_commit.rs`, `workspace-core/frozen.rs`) | C1, C3 | Capture divergence or double-applied edits (completion is revision-matched and idempotent — weakening that corrupts live state); exactly the seam the 0.2 `Proposal` boundary must reuse |
| 7 | LiveRuntime/Scheduler lane admission + OperationGate (`live_runtime.rs`, `adapter.rs`) | C3, C4 with C1 edges | The substrate ranks 6 and 8 run on: drop the per-lane admission and the host deadlocks or grows unbounded memory; break the two-phase gate and Commit hangs behind parked callbacks or admits namespace mutations mid-capture (§7.1) |
| 8 | FUSE port + LiveOwner (`port.rs`, `live_owner.rs`, `filesystem.rs`) | C3 | Projection conformance: canonical capture must stay projection-independent — diverge FUSE from materialization identity and the same edit produces different Commits |
| 9 | Three-root reconcile (`filesystem/reconcile.rs`, tree merges) | C1, C3 | Conflicts silently "resolved" or misclassified; 0.2 multi-agent acceptance depends on its exact conflict classes and affected-path evidence |
| 10 | Capability-authenticated daemon protocol + single-owner model (`layerfs-daemon/protocol.rs`, `main.rs`) | C1 (security), C2 | The sandbox isolation boundary: a replayable proof or a second-owner takeover breaks the execution trust model |
| 11 | Construction bounds + spill (worker limits, 6/64 MiB spill, batch bounds) | C4 with C1 edges | Candidate construction becomes unbounded in memory or transactions; the bounds are invariants even where the code is plumbing |
| 12 | Monitor/telemetry receipts (`layerfs-monitor`, `telemetry.rs`) | C4, C2 (candidate equation) | The candidate-conservation receipt is the audit surface benchmarks rely on; beyond that, reshufflable |
| 13 | Retained/test-only machinery: `proxy_{client,host}.rs`, `whole.rs` writers, LFCNT1 readers | C5 | Nothing in the live product route — but LFCNT1 readers must keep reading previously compacted Stores, so removal is a compatibility decision, not a cleanup |
| 14 | Dead/vestigial code (§9.2): Branch lease, `WorkspaceState::Committed`, `ChunkIdentityMismatch`, `queue_ns`, `MAX_DECODE_NESTING_DEPTH`, unused `clap` dep | C5 | Nothing — but each is a false signpost for readers of the tree (the lease actively misled the 0.2 planning doc) |

```text
                 migration risk concentration
   C1+C2 (invariant + contract)      C3 (0.2 seam)         C4/C5 (plumbing)
  +---------------------------+  +------------------+  +----------------------+
  | 1 identity/auth           |  | 6 snapshot/     |  | 11 construction      |
  | 2 publication CAS         |  |   complete      |  |    bounds + spill    |
  | 3 schema v10 + gates      |  | 7 runtime lanes |  | 12 monitor receipts  |
  |   (migration ends at 9)   |  |   + op. gate    |  | 13 retained: proxy,  |
  | 4 CDC profile + codec     |  | 8 FUSE port/    |  |    LFCNT1 (compat)   |
  | 5 small delta lane        |  |   LiveOwner     |  | 14 dead code         |
  |   10 daemon capability    |  | 9 reconcile     |  |    (signposts)       |
  +---------------------------+  +------------------+  +----------------------+
   touch only with a stronger   0.2 lands here;        move freely inside
   replacement (0.1 properties  reuse, don't           their stated bounds;
   to preserve: 0.2 current-    reinvent               removal of 13/14 is a
   model.md:342-357)                                   checklist decision
```

One-line read: the identity/authentication core, the CAS publication chain
and the schema gates are where correctness lives; the snapshot/complete
protocol, the FUSE port and the reconcile engine are precisely the seams the
v0.2 plan intends to restructure; everything else is bounded plumbing or
retained history.

---

## 9. Staleness, unknowns, method

### 9.1 Documents that describe older source

| Document | What it actually describes | Stale at HEAD |
| --- | --- | --- |
| [`docs/roadmap/0.1/development.md`](../development.md) | 0.1.x toolchain + gates + crate map | lists 9 crates — omits `layerfs-workspace-core` (split 2026-09-06); its gate list omits `tools/preflight.sh` which AGENTS.md §4 now names as the pre-push gate. Dependency rules it states still hold (§4) |
| [`docs/roadmap/0.1/0.1.5/existing_architecture.md`](../0.1.5/existing_architecture.md) | source-bound inventory pinned to evidence head `cf3a0589` (2026-09-09, schema-7 era) | "seven tables" → nine at schema 10; native chains "four edges" → eight; the per-write "reserve backing / append-window → grouped HOST spool append + ACK" write path was removed (#151, 2026-09-15): the host now **rejects** those opcodes. Still true: CDC 8/16/32 KiB, ≤128-entry nodes, 8 191/4 MiB ceilings, 256-KiB slabs, 4-KiB pages, MEMORY/synchronous-OFF |
| [`docs/general/concepts.md`](../../../general/concepts.md) | the shipped vocabulary (mostly accurate) | "one fresh FUSE helper per Workspace" is true only for the docker mount route — the daemon route mounts in-process; "keeps no … payload cache warm" overstates — a bounded 32-MiB immutable read cache is runtime-owned; "Commit captures the final filesystem state" is materialize-era phrasing — the live route takes a non-pausing operation cut. The lifecycle, entity, projection, daemon-no-Store and Monitor claims all verified |
| [`docs/roadmap/0.2/agent-branch-reconciliation/current-model.md`](../../0.2/agent-branch-reconciliation/current-model.md) | implemented 0.1 behavior + 0.2 requirements | its lease description is stale: "Workspace creation … acquires an in-process writable lease for that Branch … does not admit two writable Workspaces" — the lease was unwired (commit `30b54c44c`); `create_workspace_session` only pins a read-only snapshot, two Workspaces can share a Branch, and races surface at publication as `HeadMoved`. Its other 0.1 descriptions (Commit CAS, Add, stale behavior, reconcile) verified accurate |
| [`docs/versioned/0.1.5/`](../../../versioned/0.1.5/README.md) | the released manual (immutable) | schema 10, nine tables, 35 columns, SDK surface and fuser 0.18.0 all still match HEAD. Two wording items describe the pre-#151 route: "backing fences" language in sdk.md:242 and container-runtime.md:126-133 — at HEAD ordinary FUSE writes have no host append to fence |
| [`README.md`](../../../../README.md) | shipped pitch + preview status | the CI badge (line 10) links to `actions/workflows/ci.yml`, which no longer exists — CI is disabled (AGENTS.md §4) |

### 9.2 Dead or vestigial code at HEAD (verified, no judgment attached)

- **Branch lease**: `StoreDb::acquire_workspace_lease`
  ([`schema.rs:432`](../../../../crates/layerfs-layerstack-store/src/schema.rs#L432))
  and its wrapper — defined, exported, never called.
- **`WorkspaceState::Committed`** — declared, compared in `discard()`, never
  assigned anywhere in the tree.
- **`CoreError::ChunkIdentityMismatch`** — defined, no constructor in `crates/`.
- **`OperationReceipt.queue_ns`** — always 0; no writer exists.
- **`MAX_DECODE_NESTING_DEPTH`** — declaration only; no enforcement site
  anywhere.
- **`clap` dependency in `layerfs-cli`** — declared, never referenced by
  `src/` (the parser is hand-rolled `match`).
- **`proxy_client.rs` / `proxy_host.rs`** — a complete legacy port-forwarding
  wire with no non-test caller in `crates/` (the live wire is
  `live_wire.rs`).

### 9.3 Unknowns — behaviour not verifiable from this study's source reads

Recorded as open questions, not filled by assumption:

- Who writes `LFSWFL1` whole-file owner objects in the current production
  path (§3.2) — the object type is defined and its readers verified; the
  production writer was not traced.
- Whether `ReconcileChoice::WorkingTree` has distinct semantics anywhere —
  in the current engine it is treated identically to `Branch`.
- The exact concurrency guarantee for Store readers interleaving with an
  open admission transaction on the single shared connection mutex.
- Whether anything outside `crates/` consumes the non-Candidate
  `StorageReceipt` variants or `OperationReceipt::to_json`.
- The host-materialize execution route's fresh-process guarantee (verified
  on the container route only).
- Whether `add_layer`/snapshot operations can independently publish a layer
  root covering a lost race's objects (reachability is measured from three
  root families; the interaction was not traced).
- The intended semantics of `PredecessorCursor::hints`' 4096-descriptor
  exhaustion bound in compaction-adjacent reads.

### 9.4 Rows dropped or downgraded during verification

- Slice E's "schema 8/9/10 → feature mapping" arrived inferred from error
  strings; upgraded after the coordinator read `schema.rs:167-184` directly.
- Slice B's "≈64-byte effective hash window" — arithmetic inference with no
  declared constant; dropped (not contract).
- Slice F's "lost race leaves unreachable objects" — arrived as static
  reasoning; kept only after depth follow-up D3 walked the retain/rollback
  states and delete sites.
- Slice G's Branch-lease claims — the slice itself flagged the unwiring;
  coordinator re-verified by repo-wide search before accepting.
- The content spec's citation of `admission.rs:291` as "the" 128-KiB policy
  line — corrected to `SMALL_LIMIT` at `content.rs:8` (§3.2 distinguishes
  the three 128-KiB constants).

### 9.5 Method

Source pin and read-only statement at the top of this document. Evidence
gathering: 13 breadth exploration slices (one per subsystem) + 3 depth
follow-ups (durability semantics, limit enforcement, object lifecycle) + 2
adversarial checks (coverage and citations), all read-only subagents; the
coordinating author personally re-opened every claim marked load-bearing in
§2 and every §3 row. Subagent output was treated as untrusted observation
throughout.

**What the two adversarial checks found** (folded back into the body):

- *Citation check* — 20 highest-value claims (§2, §3.2, §8 ranks 1–9)
  re-opened at their cited lines: 19 supported **exactly**, 1 **partly**.
  The one defect was wording, not mechanism: "the only UPDATEs in the whole
  SQL corpus" overlooked the admission-internal pack-append UPDATE
  (`objects.rs:2403`) and the inode-serial upsert
  (`sql/schema/reserve_inode_serials.sql:5`) — neither touches
  commits/layers; §2 row 3 was re-worded accordingly. No claim was
  invalidated.
- *Coverage check* — 8 mechanisms the draft did not mention, 4 rated
  high-severity for a migration: (1) the `LiveRuntime`/`Scheduler`
  per-lane admission substrate (now §7.1, §8 rank 7); (2) the two-phase
  `OperationGate` operation cut behind the "non-pausing Commit"
  (now §6.5); (3) the store version-freeze + only-offline-7/8→9 migration
  contract (now §6.10, §8 rank 3); (4) the third projection route — Linux
  host-FUSE — and the materialized route's re-read capture (now §6.5).
  Medium findings (stateless-open dual mode, quarantine latch + TicketGate,
  initialization import + seed, transport timeouts/bind/capability transit)
  are folded into §6.3, §6.4, §6.7 and §7.1.

## Hand-off to the v0.1.7 checklist

Observations that imply work — recorded here, decided nowhere:

1. The single-construction-worker policy in AGENTS.md §3.8
   (`LAYERFS_CONSTRUCTION_WORKERS=1` per run) is harness-enforced; the code
   default remains `available_parallelism().min(8)` / `SMALL_CONTENT_WORKERS
   = 4` (§6.3).
2. Lost publication races permanently orphan admitted objects; nothing at
   HEAD reclaims them, and the Monitor measures the gap (§3.5).
3. Dead/vestigial code in §9.2 (lease, `Committed` state, error variant,
   `queue_ns`, dead limit constant, unused `clap`, legacy proxy wire) —
   removal or rewiring is a checklist decision.
4. Documentation corrections implied by §9.1: development.md crate map and
   gate list; concepts.md's helper/cache/"final" phrasing; the 0.2
   current-model's lease description; README's dead CI badge.
5. `MAX_DECODE_NESTING_DEPTH` is declared but never enforced (§6.9) — either
   enforce or remove.
6. The 0.1.5 manual's "backing fence" wording describes a route that no
   longer exists (§9.1) — a versioned-manual boundary question.
7. The unknowns in §9.3 — each is a small, source-readable question for the
   checklist, not for this study.
8. The store migration path stops at 9: no in-place upgrade to v10, no
   upgrade from v6, stores version-frozen at creation (§6.10) — any v0.1.7
   schema work inherits this gap.
9. The `BackingServer` binds `0.0.0.0` (all interfaces, capability-gated)
   and the daemon capability transits host disk via `docker cp` (§7.1) —
   recorded as observed facts; whether that posture is intended belongs to
   the checklist.

## Acceptance self-check (per #156)

- [x] Every §2 claim resolves to a path, or is marked not-found — all eight
  rows `verified` (two with recorded caveats); no `not-found` rows remain.
- [x] §3 states which small-file path is live today — `delta.rs` + pack
  v3/v4 + the candidate cache are the live writers (`admission.rs:517`);
  `whole.rs` LFCNT1 is retained-read-only with `#[cfg(test)]` writers.
- [x] §8 uses stated criteria (C1–C5), not taste — criteria listed before
  the ranking; every rank cites its criteria.
- [x] §9 lists stale documents and unknowns explicitly (§9.1–§9.4), and
  §9.5 records what both adversarial checks found.
- [x] Every structural section carries at least one ASCII diagram whose
  arrows trace to source: §3 has three, §4 one, §6.4 and §6.6 one each,
  §7 one, §8 one — 8 diagrams total (plus one constants block in §3.1),
  all plain ASCII.
- [x] One status banner: "Research; informative and not a product
  contract." Chosen because this document records what exists at a pinned
  commit and hands observations to the v0.1.7 checklist; it plans no work
  and admits nothing, so the planning-checklist banner would overstate it.
- [x] All local links resolve (link targets verified against the tree);
  source commit pinned at the top, with the HEAD-advance note.
- [x] Citation check: 19 of 20 highest-value claims verified exactly; the
  one wording defect was corrected in §2 row 3 (§9.5).






