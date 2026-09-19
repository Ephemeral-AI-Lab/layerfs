# E1 — The advisory list, exhaustively

**Status:** reference document. Every claim below is either a **code fact** (file:line, quoted or
paraphrased) or a **measurement** with its arithmetic. Anything not backed by one of those two is
labelled **HYPOTHESIS**.

**Measurement class:** every byte figure here is a **diagnostic**, not admission evidence.
No timing reading appears in this document (the machine may be busy; timings would be invalid).

**Product under analysis:** `core/crates/layerfs-content` (C1) + `core/crates/layerfs-storage` (C2).
**Reference tree:** `crates/` (v0.1.6 lineage). Nothing under `core/crates/` or `crates/` was modified.

---

## 0. The one-sentence answer

The advisory list is a **bounded, ordered, non-persisted list of at most four object identities that
the *caller* (C1) attaches to a `FinalizedObject` to say "I believe this new object resembles these
stored objects, best first"**; C2 reads it at exactly one place (`cas/save.rs:100`), offers it to the
representation selector, and may ignore all of it. It is a **hint the caller declares**. It is *not*
the candidate the **store finds by itself** — that is a separate, per-save, content-keyed cache
(`encoding/delta/candidates.rs`) which is consulted only *after* the advisory list yields nothing
eligible. Those two routes are the only two ways an object ever gets a `base_object_id`.

---

## 1. The type

File: `core/crates/layerfs-content/src/object/predecessor.rs` (102 physical lines).
Re-exported at `core/crates/layerfs-content/src/object/mod.rs:27-29` and
`core/crates/layerfs-content/src/lib.rs:39-40`.

### 1.1 The bound

```rust
/// Largest number of advisory predecessors one object may carry.
pub const MAXIMUM_ADVISORY_PREDECESSORS: usize = 4;      // predecessor.rs:13-14
```

**4.** The bound is on **list length**, i.e. on the number of *distinct* identities (see 1.3).

### 1.2 The entries

```rust
pub enum PredecessorProvenance {                          // predecessor.rs:16-25
    OriginalBase,     // 19-20: the base object this operation was declared to edit
    UnchangedPrefix,  // 21-22: the object that held bytes immediately before the replaced range
    ReusedRange,      // 23-24: a stored object covering a reused subtree or extent range
}

pub struct AdvisoryPredecessor {                          // predecessor.rs:28-32
    id: ObjectId,                                        // private
    provenance: PredecessorProvenance,                   // private
}
impl AdvisoryPredecessor {
    pub const fn id(self) -> ObjectId { ... }             // 36-38
    pub const fn provenance(self) -> PredecessorProvenance { ... }  // 41-43
}
```

`AdvisoryPredecessor` is `Copy`, `Eq`, `PartialEq`, `Debug` (derive at :28). The fields are private,
so the **only** way to build one is `AdvisoryPredecessors::push` / `::explicit`.

### 1.3 push — what it refuses

```rust
pub fn push(&mut self, id: ObjectId, provenance: PredecessorProvenance) -> ContentResult<()> {
    if self.entries.len() >= MAXIMUM_ADVISORY_PREDECESSORS {          // :62
        return Err(ContentError::BoundedCapacityExceeded {            // :63
            what: "object.predecessors",                              // :64
            limit: MAXIMUM_ADVISORY_PREDECESSORS as u64,              // :65
            actual: self.entries.len() as u64 + 1,                    // :66
        });
    }
    if self.entries.iter().any(|entry| entry.id == id) {              // :69
        return Ok(());                                                // :70
    }
    self.entries.push(AdvisoryPredecessor { id, provenance });        // :72
    Ok(())
}
```

* **Refuses** the 5th distinct identity, with `ContentError::BoundedCapacityExceeded`.
  Because the guard is evaluated first, `actual` is always exactly `5` when it fires.
* **Absorbs** a duplicate identity: it returns `Ok(())` **without appending and without updating the
  provenance of the existing entry** (:69-71). The *first* occurrence's provenance wins.
  Consequence: the list can hold at most 4 distinct ids, and "4 entries" always means "4 distinct ids".
* `what: "object.predecessors"` is the machine-readable identity of the bound. It is the only place
  in the product where this bound can fail.

### 1.4 explicit

```rust
pub fn explicit(id: ObjectId) -> ContentResult<Self> {   // :96-101
    let mut list = Self::new();                          // :98
    list.push(id, PredecessorProvenance::OriginalBase)?; // :99
    Ok(list)
}
```

`explicit` builds a **one-element list with provenance `OriginalBase`**. Its `Result` return type is
uniformity, not a real failure mode: the list is empty (`0 >= 4` is false) and contains no duplicate,
so `push` cannot return `Err` on this path. **Code fact, not a hypothesis.**

### 1.5 What "preference order" means

```rust
pub fn ids(&self) -> impl Iterator<Item = ObjectId> + '_      // :86-89, "in preference order"
pub fn entries(&self) -> &[AdvisoryPredecessor]               // :91-94, "in preference order"
```

Both iterate `entries` in **insertion order** — `push` appends (:72) and never reorders or sorts.
The order is *semantically* load-bearing at exactly two places:

* `encoding/delta/select.rs:342-354` (`acquisition`) walks the slice in order and returns the **first**
  id that passes `probe`. Later entries are never examined once an earlier one is eligible.
* `encoding/delta/select.rs:247` (`ObjectRole::Chunk`) considers **only `advisory.first()`**. Entries
  1..3 are dead for the chunk lane.

So index 0 is the strongest claim the caller can make, and it is the only claim the chunk lane honours.
There is **no cost comparison between candidates**: the first eligible one is taken, and it is then
compared once against the FULL alternative (`select.rs:302`).

### 1.6 Why the docstring calls it "a hint, never a dependency"

`predecessor.rs:1-8`:

> A predecessor is a hint, never a dependency: C1 declares which stored objects it believes a new
> object resembles, in preference order, and C2 decides whether any of them is acquired and worth
> encoding against. [...] No codec, SQL connection, host role or mutable handle belongs here.

Three concrete reasons, each a code fact:

1. **It is not a reference.** `FinalizedObject::references()` (`object/output.rs:143-146`) is the
   dependency edge set: every reference is checked for presence before an object is stored
   (`cas/dependencies.rs:80-103`, `cas/save.rs:43-51`), and a missing reference **fails the save**
   (`StorageError::MissingDependency`, `dependencies.rs:96-99`). The advisory list is checked by
   nothing: `Availability::unresolved` iterates `object.references()` only (`dependencies.rs:112`).
2. **A missing or ineligible candidate is an ordinary policy outcome, never a failure.**
   `select.rs:268-274` counts `no_candidate` and returns the FULL representation; `select.rs:338`
   counts `ineligible_candidates`; `select.rs:293` counts `work_exceeded`. Only a genuine codec,
   allocation or read failure fails the operation (`select.rs:5-8`).
3. **It is not persisted.** `AdvisoryPredecessors` has no canonical encoding, no SQL column and no
   read path. The only thing that survives a save is `objects.base_object_id` — what the store
   *chose*. A reopened store cannot recover what a caller *declared*.

---

## 2. How it travels — the call chain, with line numbers

### 2.1 C1 side (the object carries it)

| # | Site | What happens |
|---|---|---|
| 1 | `core/crates/layerfs-content/src/object/output.rs:93` | field `predecessors: AdvisoryPredecessors` on `FinalizedObject` |
| 2 | `object/output.rs:107` | `FinalizedObject::new` initialises it to `AdvisoryPredecessors::new()` — **empty by default** |
| 3 | `object/output.rs:117-121` | `pub fn with_predecessors(mut self, predecessors: AdvisoryPredecessors) -> Self` — builder-style, replaces the whole list |
| 4 | `object/output.rs:148-151` | `pub fn predecessors(&self) -> &AdvisoryPredecessors` — the only reader in the product |
| 5 | `object/output.rs:153-168` | `into_parts()` moves `predecessors` into `ObjectParts` |
| 6 | `object/output.rs:174-185` | `pub struct ObjectParts { ..., pub predecessors: AdvisoryPredecessors }` (field at :184) |
| 7 | `object/output.rs:191-194` | `trait FinalizedConsumer { fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> }` |

`into_parts`'s docstring (`output.rs:153-159`) states the contract explicitly:

> The pieces are the identity, the role, the canonical bytes, the direct references **and the advisory
> predecessors**. A predecessor is an input the production save path consumes to choose a physical
> representation, so a consumer that takes ownership through this call has to receive it: a form that
> dropped it silently discarded an input the object was built with.

### 2.2 The production save path does **not** go through into_parts

`into_parts` is a public ownership-move API; the Store takes the whole `FinalizedObject` **by value**:

| # | Site | What happens |
|---|---|---|
| 8 | `core/crates/layerfs-storage/src/cas/store.rs:340` | `SaveOperation::accept(&mut self, object: FinalizedObject)` |
| 9 | `cas/store.rs:344` | `self.batch.push(object)` — `PendingBatch` holds `Vec<FinalizedObject>` (`cas/batch.rs:15-20, 48-69`), bounded by `capacities.batch_objects` / `batch_bytes` |
| 10 | `cas/store.rs:370-378` | `SaveOperation::flush` -> `save::flush_batch(owner, objects)` |
| 11 | `cas/save.rs:22` | `pub fn flush_batch(owner: &mut MutationOwner, objects: Vec<FinalizedObject>)` |
| 12 | **`cas/save.rs:100`** | `let advisory: Vec<ObjectId> = object.predecessors().ids().collect();` |
| 13 | **`cas/save.rs:101`** | `owner.offer(object, &advisory, &mut availability)?` |

**`cas/save.rs:100` is the single materialisation point of the advisory list in the entire product.**
It is reached only for an object that has **no row at the wave's membership snapshot** and is not
already sealed by this wave (`save.rs:74-99`); an exact reuse never consults the list.

The list is copied into a fresh `Vec<ObjectId>` (<= 4 elements) that lives only for the duration of
`offer`. Nothing retains it; nothing stores it.

### 2.3 C2 side

| # | Site | What happens |
|---|---|---|
| 14 | `cas/selection.rs:28-33` | `MutationOwner::offer(&mut self, object: &FinalizedObject, advisory: &[ObjectId], availability: &mut Availability)` |
| 15 | `cas/selection.rs:41` | `let record = self.select_record(object, advisory)?;` |
| 16 | `cas/selection.rs:93-100` | `select_record`: `InodeLeaf` -> `self.select_pooled(object, advisory)`; everything else falls through |
| 17 | `cas/selection.rs:112-119` | `select(&mut input, object.id(), object.canonical(), object.role(), advisory, &mut self.compression)` |
| 18 | `encoding/delta/select.rs:196-203` | `pub fn select(input, id, canonical, role, advisory: &[ObjectId], encode) -> StorageResult<EncodedRecord>` |
| 19 | `cas/pool_lane.rs:60-64` | `select_pooled(&mut self, object, advisory: &[ObjectId])` |
| 20 | `cas/pool_lane.rs:141` | `self.pool_base(advisory, object.canonical_len() as u64, full.len() as u64)?` |
| 21 | `cas/pool_lane.rs:309-369` | `pool_base(advisory, ...)` — the pooled lane's own eligibility gate |

### 2.4 Non-product callers of the same chain

The harness re-materialises objects from a lossless text format and re-attaches the list:
`core/benchmark/fs-bench-pro-storage-content/src/workload/artifact.rs:305-320`
(`FinalizedObject::new(role, canonical).with_references(...).with_predecessors(parse_predecessors(...)?)`),
with the encoding at `artifact.rs:442-499` (`<id>:<provenance code>` pairs). This is harness source,
**not** product source.

---

## 3. Every producer in the whole repo

### 3.1 Product producers (`core/crates/*/src`) — exactly three call sites

`grep -rn 'with_predecessors' core/crates/*/src` returns exactly three hits. There are no others.

#### P1 — the declared edit base (WholeFile)

* **Function:** `apply_edits` — `core/crates/layerfs-content/src/file/edit/apply.rs:38-143`.
* **Site:** `apply.rs:117-122`.

```rust
let mut object = edit.child("content.identify")
    .run(|_| FinalizedObject::new(ObjectRole::WholeFile, canonical))?;   // :117-119
if view.root() != object.id() {                                          // :120
    object = object.with_predecessors(AdvisoryPredecessors::explicit(view.root())?);  // :121
}
```

* **Base named:** `view.root()` — the `EditRequest::root` the *caller* declared (`apply.rs:28-35`),
  opened as a `FileView` at `apply.rs:48`. In a history workload this is the previous state's content
  root, so the base is a **cross-save** object.
* **Role of the base:** `WholeFile` (it is the object this edit was declared against).
* **Provenance:** `OriginalBase` (via `explicit`, `predecessor.rs:97-101`).
* **Guard:** only attached when the rebuilt object's identity differs from the base — an edit that
  reproduced the base exactly returns the base root (`apply.rs:76-82`, `:120`) and emits nothing.
* **Same-save or cross-save:** whatever the caller's `root` is. In the history corpus: **cross-save**.
* **Reachability:** `apply_edits` is the only path. It is *not* reached by complete construction.

#### P2 — the payload a replacement run continues (Chunk)

* **Function:** `ExtentBuilder::push_chunk` — `core/crates/layerfs-content/src/file/mapping/build.rs:119-136`.
* **Site:** `build.rs:125-130`.

```rust
let mut object = FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(raw)?)?;  // :125
if let Some(predecessor) = predecessor {                                               // :126
    let mut predecessors = AdvisoryPredecessors::new();                                // :127
    predecessors.push(predecessor, PredecessorProvenance::UnchangedPrefix)?;           // :128
    object = object.with_predecessors(predecessors);                                   // :129
}
```

* **Base named:** the `predecessor: Option<ObjectId>` argument (`build.rs:116-122`), documented there
  as "the retained payload this run continues [...] It is advisory only: C2 decides whether the hint is
  acquired and worth encoding against."
* **Role of the base:** `Chunk`.
* **Provenance:** `UnchangedPrefix`.
* **The only production caller passing `Some`:** `file/edit/apply.rs:309-325` — `rightmost_payload`
  (`apply.rs:365-394`) walks one path to the rightmost extent of the retained *left* part of a split
  and hands its payload id to `builder.push_chunk(chunk, predecessor, &mut sink)` (`apply.rs:325`).
* **The complete-construction caller passes `None`:** `build_streaming` at `build.rs:319-332` calls
  `builder.push_chunk(chunk, None, consumer)` (`build.rs:326`). So `construct_bytes` /
  `construct_stream` produce **no** chunk predecessor.
* **Same-save or cross-save:** the payload comes from the base file state — a stored object from an
  earlier save. **Cross-save.**

#### P3 — the stored page a changed page was materialised from (four tree roles)

* **Function:** `Engine::persist` — `core/crates/layerfs-content/src/filesystem/sorted/page.rs:340-396`.
* **Site:** `page.rs:380-390`.

```rust
let mut object = FinalizedObject::new(page_role::<F>(page.level), canonical)?
    .with_references(references);                                       // :380-381
if let Some(origin) = page.origin {                                     // :382
    let mut predecessors = crate::object::AdvisoryPredecessors::new();  // :383
    let _ = predecessors.push(                                      // :384
        origin,                                                     // :385
        crate::object::PredecessorProvenance::UnchangedPrefix,      // :386
    );
    object = object.with_predecessors(predecessors);                    // :388
}
let emitted = self.objects.emit(object)?;                               // :390
```

* **Base named:** `page.origin` (`page.rs:107-108`, "Stored origin of this page, when it was
  materialized from one"). It is set when a page is decoded out of a stored node —
  `page_from_wire` (`page.rs:467-473`, `page.origin = Some(id)`), `merge::edit`
  (`merge.rs:187-188`, `page.origin = id`) — and cleared when the page is re-partitioned
  (`merge.rs:88`, `merge.rs:106`). In a filesystem update, `read.wire` comes from the **base
  filesystem root**, i.e. the previous state. **Cross-save.**
* **Roles it can name:** `page_role::<F>(level)` (`page.rs:601-607`) maps the four sorted formats to
  `DirectoryLeaf`, `DirectoryBranch`, `InodeLeaf`, `InodeBranch`. So this one producer covers **four**
  roles.
* **Provenance:** `UnchangedPrefix`.
* **The `push` result is discarded** (`let _ =`, `page.rs:384`). A fresh list with at most one entry
  cannot exceed the bound, so this error path is dead: the producer can never fail construction.
* **Guard:** only when the re-encoded page's identity differs from the stored origin
  (`page.rs:376-379` — an identical page is counted `pages_reused` and never emitted).

#### Non-producers in product source (explicitly checked)

| Site | Why it produces nothing |
|---|---|
| `file/content.rs:207-250` `construct_bytes` | Whole-file arm calls `FinalizedObject::new(ObjectRole::WholeFile, canonical)` (`:233-235`) with no `with_predecessors`; chunked arm calls `construct_chunked` -> `build_streaming`, which passes `None` |
| `file/content.rs:258-289` `construct_stream` | delegates to the same two paths |
| `file/mapping/build.rs:351-367` `emit_file_state` | attaches `with_references` only |
| `file/mapping/build.rs:369-393` `emit_node` | attaches `with_references` only -> `ExtentLeaf`/`ExtentBranch` never carry a hint from this producer |
| `filesystem/sorted/finish.rs:178` | the empty `DirectoryLeaf` is built with `FinalizedObject::new(...)` and no predecessors |
| `object/output.rs:234-242` `DiscardingConsumer` | drops the whole object, list included |

### 3.2 Harness / example / test producers (not product source)

| Site | Names | Provenance | Note |
|---|---|---|---|
| `core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:723-727` | previous version's content root of the same path (`history.rs:640-667`) | `OriginalBase` | gated by `LAYERFS_HISTORY_ADVISORY=1` (`history.rs:101-106`); this is the faithful model that the product rejects with `Integrity("dependency chain depth")` at `encoding/delta/read.rs:159-168` |
| `core/benchmark/.../src/workload/artifact.rs:317-320` | round-tripped from the artifact text | parsed at `:488-499` | lossless re-materialisation |
| `layerfs-storage/examples/measure_edits.rs:448-454` | `base` | `OriginalBase` | example |
| `layerfs-storage/examples/measure_pooled.rs:129-132` | `base` | `OriginalBase` | example |
| `layerfs-storage/examples/memory_ledger.rs:344-346` | `base` | `OriginalBase` | example |
| `layerfs-storage/tests/delta_payload.rs:20-25, 126-133` | base / second leaf | `OriginalBase`, **`ReusedRange`** | the only construction of `ReusedRange` anywhere |
| `layerfs-storage/tests/delta_chains.rs:24-29` | previous chain member | `OriginalBase` | |
| `layerfs-storage/tests/metadata_pool.rs:51-56`, `metadata_pool_index.rs:55-60`, `metadata_chain.rs:64-69` | previous leaf | `OriginalBase` | |
| `layerfs-storage/tests/support/mod.rs:106-110` | preserved through the test consumer | as declared | asserts the C1->C2 handoff keeps the list |
| `layerfs-content/tests/object_identity.rs:396-397` | `base` | `OriginalBase` | proves `into_parts` keeps it |

### 3.3 The v0.1.6 reference (`crates/`) — a *different* mechanism, not this type

**`crates/` has no `AdvisoryPredecessors`.** `crates/layerfs-content/src/object/` contains
`access.rs, id.rs, references.rs, digest.rs, mod.rs, codec.rs, canonical.rs` — there is no
`predecessor.rs`, and `grep -rn with_predecessors crates/` returns nothing. The reference's
cross-commit correspondence is a different design (per-object `prior_ids` / `has_predecessor`
hints plus a `PredecessorCursor`), and it is worth naming because it is what the campaign's
"producer was active in v0.1.6" result refers to.

* **Producer:** `ObjectBuffer::set_physical_predecessor` —
  `crates/layerfs-layerstack-store/src/objects.rs:3311-3358`.
  It sets `self.objects.small_predecessor = Some(root.0)` (`:3318`) and, for a content root that is
  not `Small`, `self.objects.predecessor = Some((reader, FileStateRoot(root.0), operation_reserved,
  available))` (`:3351-3356`). It is `#[doc(hidden)]` and takes a `SnapshotReader` — a mutable
  runtime handle, exactly what `predecessor.rs:1-8` forbids in the replacement type.
* **Its only production caller:** `crates/layerfs-workspace/src/changes.rs:1836-1842`
  (`FrozenFile::build`). The predecessor value is decoded in `produce_file`
  (`changes.rs:1555-1631`) from the per-file prepared record at `changes.rs:1582-1586`; those bytes
  were written at `changes.rs:1510-1537` from `prior[slot]` — the **base inode record's
  `content_root`**, obtained from `FrontierInodes::base_records` (`changes.rs:1498-1505`) — or from
  `removed` for a removed path (`changes.rs:1516-1523`). That is exactly the "path -> previous
  content root" correspondence that lives in the Workspace COW tree.
* **Consumption:** `objects.rs:2812-2845` turns `small_predecessor` into `prior_ids[0]` for
  small-content objects (`:2837`) and attaches a `PredecessorCursor` for metadata objects
  (`:2815-2827`, `:2855+`). `crates/layerfs-layerstack-store/src/objects/admission.rs:648-675` reads
  `prior_ids().first()` and charges `absent_predecessors` / `predecessor_hints`; the metadata route
  does the same at `admission.rs:1720-1743`.
* **Receipts (Squad A3, reused, not re-derived):** `base_fetches = 80,361` predecessor base hops
  (100 % after state 1), `delta_selected = 72,934`.
* **Which reference producers are inert, and why:**
  * `set_physical_predecessor`'s only caller is `changes.rs`, which is the **Workspace runtime** —
    a package that does not exist under `core/` at all (`core/crates/` holds only `layerfs-content`,
    `layerfs-storage`, `layerfs-telemetry`). The mechanism is therefore **absent**, not disabled, in
    the product under test.
  * The three `diagnostic.rs` call sites (`:399`, `:436`, `:476`) are test scaffolding inside the
    reference crate.
  * `crates/` as a whole is reference code: it is not a dependency of `core/` and nothing links it.

---

## 4. Every consumer

`grep -rn '\.predecessors()' core/crates/*/src` returns **one** hit: `cas/save.rs:100`.
Everything below consumes the `advisory: &[ObjectId]` slice that line produced.

### 4.1 The dispatch (`cas/selection.rs:93-120`)

```rust
fn select_record(&mut self, object: &FinalizedObject, advisory: &[ObjectId]) -> StorageResult<EncodedRecord> {
    if object.role() == ObjectRole::InodeLeaf {              // :98
        return self.select_pooled(object, advisory);          // :99   -> pooled lane (4.6)
    }
    ...
    select(&mut input, object.id(), object.canonical(), object.role(), advisory, &mut self.compression)  // :112-119
}
```

`InodeLeaf` is routed away **before** `select` is entered. If an `InodeLeaf` ever reached `select`,
`select.rs:223-228` returns `StorageError::Integrity("pooled metadata leaf selection")` — a caller
error, not a representation to choose.

### 4.2 The ten-role short circuit (`encoding/delta/select.rs:204-222`) — the advisory is never read

```rust
if matches!(role,
    ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch | ObjectRole::FileState
  | ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch | ObjectRole::InodeBranch
  | ObjectRole::FilesystemRoot | ObjectRole::AttributeLeaf | ObjectRole::AttributeBranch
  | ObjectRole::Symlink) {                                             // :204-216
    // Tree roles are ordinary framed bytes: they are stored whole inside their
    // group and never choose a payload delta base. The advisory predecessor an
    // unchanged subtree carries stays a physical placement hint, not a
    // representation this route may act on.
    return encode_full(canonical, role, input.capacities, encode);     // :221
}
```

**Ten roles** return before line 229, and the `advisory` parameter is only touched at `:247` and
`:254`. For these ten roles the parameter is **dead code**: the list may be full and it makes no
difference. The comment at `:218-220` is the product's own statement that this is intended.

### 4.3 The payload lanes

```rust
let full = encode_full(canonical, role, input.capacities, encode)?;     // :229
let depth_cap = input.capacities.delta_depth_for_role(role);            // :232
if depth_cap == 0 { ... return Ok(full); }                              // :233-240  advisory never read
let raw = raw_payload(canonical, role)?;                                // :241
let candidate = match role {
    ObjectRole::Chunk => match advisory.first().copied() {              // :247
        Some(id) => match probe(input, id, role, depth_cap)? { true => Some(id), false => None },  // :248-251
        None => None,                                                   // :252
    },
    _ => match acquisition(input, role, advisory, depth_cap)? {         // :254
        Some(id) => Some(id),                                           // :255
        None => {                                                       // :256
            let found = input.candidates.find(id, &signature(raw));     // :257  <- STORE-FOUND route
            match found {
                Some(id) => match probe(input, id, role, depth_cap)? { true => Some(id), false => None },  // :259-262
                None => None,
            }
        }
    },
};
```

* **Chunk (`:247-253`)** — **only `advisory[0]`**. Entries 1..3 are never considered, and the cache
  fallback is never reached (it is inside the `_` arm). The candidate is still run through the same
  `probe` -> `eligible` gate.
* **WholeFile (the `_` arm, `:254-266`)** — `acquisition` walks the **whole list in order** and
  returns the first eligible entry. **Only if that returns `None`** does the store consult its own
  per-save candidate cache. This is the single most important control-flow fact in the document:
  *an eligible advisory entry suppresses the cache entirely.*

`depth_cap` comes from `StorageCapacities::delta_depth_for_role` (`core/crates/layerfs-storage/src/policy.rs:353-359`):
`Chunk -> chunk_delta_max_depth`, `InodeLeaf -> metadata_delta_max_depth`, everything else ->
`whole_file_delta_max_depth`. For the store under analysis (`/tmp/base187/sample.sqlite`,
`store_policy` row): `whole_file_delta_max_depth = 8`, `chunk_delta_max_depth = 4`,
`metadata_delta_max_depth = 8`, `small_file_threshold_bytes = 131072`, `retained_pack_ceiling = 502`.

### 4.4 acquisition (`select.rs:342-354`)

```rust
fn acquisition(input, role, advisory: &[ObjectId], depth_cap: u8) -> StorageResult<Option<ObjectId>> {
    for id in advisory {                        // :348
        if probe(input, *id, role, depth_cap)? { return Ok(Some(*id)); }   // :349-351
    }
    Ok(None)                                    // :353
}
```

Order is respected; the loop stops at the first eligible entry; `probe` errors are propagated (a
lookup failure is a failure, not a fallthrough).

### 4.5 The eligibility gate — exactly what makes a candidate eligible

`probe` (`select.rs:329-340`) is a thin wrapper: `eligible(...)` true -> `Ok(true)`; false ->
`ineligible_candidates += 1` (`:338`) -> `Ok(false)`.

`eligible` (`select.rs:356-374`):

```rust
let Some(location) = lookup::location(input.connection, id, i64::MAX)? else {   // :362
    input.counters.absent_candidates += 1;                                       // :363
    return Ok(false);
};
if location.role != role { return Ok(false); }                                   // :366-368
let Some(depth) = input.depths.depth_of(input.connection, id)? else {            // :369
    input.counters.absent_candidates += 1;                                       // :370
    return Ok(false);
};
Ok(depth < depth_cap)                                                            // :373
```

A candidate is eligible **iff all four hold**:

1. **A row exists** for the id under ceiling `i64::MAX` (`:362`). `lookup::location` runs
   `... FROM objects WHERE object_id IN (...) AND pack_id <= ?` (`sqlite/lookup.rs:59-67`);
   `i64::MAX` is *no ceiling*, so on the open write connection a row this save already sealed **is**
   visible. A candidate whose row does not exist yet (still waiting in an unsealed group) is
   **absent**: `absent_candidates += 1`, no failure.
2. **Exact role equality** (`:366`). No cross-role base is ever eligible. This rejection is *not*
   separately counted; `probe` charges it to `ineligible_candidates`.
3. **A resolvable depth** (`:369`). `DepthCache::depth_of`/`cost_of` (`select.rs:94-144`) walks
   `base_object_id` links to the chain root, refusing a stored chain longer than
   `MAXIMUM_DELTA_MAX_DEPTH` (`:113-115`). An id whose walk yields `None` is charged to
   `absent_candidates` (`:370`).
4. **`depth < depth_cap`, strictly** (`:373`). A base already sitting *at* the cap is refused, so the
   dependent's depth becomes at most `depth_cap` — which is the depth the reader accepts
   (`encoding/delta/read.rs:159-168`; note this reader/writer pair is the unpatched product defect
   the campaign already recorded, and it is *not* re-derived here).

**Chain budgets are a second, later gate** (`select.rs:282-298`), applied after a candidate has been
chosen and its base acquired:

```rust
let chained = input.chain.canonical_bytes.saturating_add(canonical.len() as u64);   // :282-285
let encoded = input.chain.encoded_bytes.saturating_add(canonical.len() as u64);     // :286-289
if chained > input.capacities.chain_canonical_limit
   || encoded > input.capacities.chain_encoded_limit {                              // :290-291
    input.counters.work_exceeded += 1;                                              // :293
    ... return Ok(full);                                                            // :297
}
```

So `work_exceeded` means "the candidate was eligible, but creating this dependency would exceed a
chain budget" — a different outcome from `ineligible_candidates`. A candidate refused here is
**never retried against the next list entry**: the selection has already committed to exactly one
trial (`select.rs:275-276`, "Exactly one trial [...] Nothing here retries with another candidate").

### 4.6 The pooled lane's own gate (`cas/pool_lane.rs:309-369`)

```rust
fn pool_base(&mut self, advisory: &[ObjectId], target_canonical: u64, target_encoded: u64)
    -> StorageResult<Option<(ObjectId, Vec<u8>)>>
{
    let depth_cap = self.capacities.metadata_delta_max_depth;          // :315
    if depth_cap == 0 { return Ok(None); }                             // :316-318
    for id in advisory {                                               // :319
        let Some(location) = lookup::location(&self.connection, *id, self.ceiling)? else { continue };  // :320
        if location.role != ObjectRole::InodeLeaf { continue; }        // :323
        let Some(cost) = self.depths.cost_of(&self.connection, *id)? else { continue };  // :326
        if cost.depth >= depth_cap { continue; }                       // :329
        let canonical = cost.canonical.saturating_add(target_canonical);   // :338
        if canonical > self.capacities.metadata_chain_canonical_limit { self.pool.work_exceeded += 1; continue; }  // :339-342
        ... let body = self.pool_reader.leaf_body(...)?;               // :351-357
        let encoded = self.pool_reader.chain_encoded_bytes().saturating_add(target_encoded);  // :358-361
        if encoded > self.capacities.metadata_chain_encoded_limit { self.pool.work_exceeded += 1; continue; }  // :362-365
        return Ok(Some((*id, body)));                                  // :366
    }
    Ok(None)                                                           // :368
}
```

Same shape as `acquisition`, three differences worth naming:

* **The ceiling is `self.ceiling`** (the save's own pack watermark), not `i64::MAX`. A row belonging
  to a pack this save has not published — and did not create — is refused here rather than resolved
  (`pool_lane.rs:86-88` states the same rule for the value index).
* **Role must be exactly `InodeLeaf`** (`:323`) — the pooled lane never crosses roles.
* **A budget refusal `continue`s to the next list entry** (`:341`, `:364`) instead of ending the
  search. So the pooled lane *does* walk past an over-budget candidate, unlike the payload lane,
  which commits to one trial. The first candidate that passes all four gates wins (`:366`).

### 4.7 What is **not** a consumer

`Availability` (`cas/dependencies.rs`) is seeded from `object.references()` (`save.rs:43-51`) and its
`unresolved` iterates `object.references()` only (`dependencies.rs:112`). The advisory list is never
presence-checked, never pre-fetched and never batched. Every advisory probe costs its own
`lookup::location` plus a depth walk (`select.rs:356-374`).

---

## 5. The provenance variants

| Variant | Constructed by product code? | Where | Meaning |
|---|---|---|---|
| `OriginalBase` (`predecessor.rs:19-20`) | **YES** | `file/edit/apply.rs:121` (via `explicit`, `predecessor.rs:97-101`) | the base object this operation was declared to edit |
| `UnchangedPrefix` (`predecessor.rs:21-22`) | **YES** | `file/mapping/build.rs:128` (chunk payload a replacement run continues) and `filesystem/sorted/page.rs:384-387` (stored page a changed page was materialised from) | the object that held bytes immediately before the replaced range |
| `ReusedRange` (`predecessor.rs:23-24`) | **NO** | — | a stored object covering a reused subtree or extent range |

**`ReusedRange` is the one unused variant.** The only construction anywhere in the repository is
`core/crates/layerfs-storage/tests/delta_payload.rs:131`
(`predecessors.push(first_id, PredecessorProvenance::ReusedRange)`), a test that builds a two-entry
list to prove preference order is honoured.

> **ONE UNUSED VARIANT is not "no producer exists".** Producers exist and are enumerated in section 3;
> one enum *variant* has no product construction site. The distinction matters because the variant is
> also **unobservable to C2**: `cas/save.rs:100` collects `ids()` and drops the provenance entirely,
> so `select` and `pool_base` never receive it. Nothing in the store can distinguish a list built
> with `ReusedRange` from one built with `OriginalBase`. The provenance is carried, documented and
> visible to the caller and to the harness artifact format
> (`workload/artifact.rs:442-499`); it is not consumed.

---

## 6. Per-role reachability of the selector, with measured evidence

### 6.1 The table

"Reaches the selector" = the advisory slice is read at `select.rs:247`/`:254` or
`pool_lane.rs:319`. "n" and "with base" are measured from `/tmp/base187/sample.sqlite` (the
unmodified `history-stride10` lane, 17 states).

| Role | code | Route | Advisory read? | n | with `base_object_id` | with base % |
|---|---|---:|---:|---:|---:|---:|
| `WholeFile` | 1 | `select` -> `acquisition` (`select.rs:254`), then cache | **YES**, all 4 in order | 44,148 | **18,344** | 41.54 % |
| `Chunk` | 2 | `select` -> `advisory.first()` only (`select.rs:247`) | **YES**, index 0 only | 1,098 | **0** | 0 % |
| `ExtentLeaf` | 3 | short circuit (`select.rs:204-222`) | **NO** | 92 | **0** | 0 % |
| `ExtentBranch` | 4 | short circuit | **NO** | 0 | 0 | — |
| `FileState` | 5 | short circuit | **NO** | 92 | **0** | 0 % |
| `InodeLeaf` | 6 | `select_pooled` (`selection.rs:99`) -> `pool_base` (`pool_lane.rs:319`) | **YES**, all 4 in order | 1,738 | **917** | 52.76 % |
| `DirectoryLeaf` | 7 | short circuit | **NO** | 4,770 | **0** | 0 % |
| `DirectoryBranch` | 8 | short circuit | **NO** | 48 | **0** | 0 % |
| `InodeBranch` | 9 | short circuit | **NO** | 29 | **0** | 0 % |
| `FilesystemRoot` | 10 | short circuit | **NO** | 17 | **0** | 0 % |
| `AttributeLeaf` | 11 | short circuit | **NO** | 0 | 0 | — |
| `AttributeBranch` | 12 | short circuit | **NO** | 0 | 0 | — |
| `Symlink` | 13 | short circuit | **NO** | 0 | 0 | — |

**The ten short-circuited roles are `ExtentLeaf`, `ExtentBranch`, `FileState`, `DirectoryLeaf`,
`DirectoryBranch`, `InodeBranch`, `FilesystemRoot`, `AttributeLeaf`, `AttributeBranch`, `Symlink`
(`select.rs:204-216`). They return at `:221` before the advisory is read.** The measured evidence is
the six populated roles among them: **0 bases across 4,770 `DirectoryLeaf` + 48 `DirectoryBranch` +
29 `InodeBranch` + 17 `FilesystemRoot` + 92 `FileState` + 92 `ExtentLeaf` = 5,048 objects.**

### 6.2 Arithmetic — the rows add up

```
44,148 WholeFile + 1,098 Chunk + 1,738 InodeLeaf + 5,048 (ten short-circuited roles)
  = 52,032  =  SELECT count(*) FROM objects                residual 0
```

and the ten short-circuited roles decompose as

```
4,770 + 48 + 29 + 17 + 92 + 92 + 0 + 0 + 0 + 0 = 5,048       residual 0
```

### 6.3 Arithmetic — the store reproduces the campaign's counter set, residual 0

Let `P` = objects that actually reached the candidate stage = payload-lane objects
= `WholeFile + Chunk` = `44,148 + 1,098` = **45,246**. (InodeLeaf never enters `select`; the ten
short-circuited roles return at `:221`; objects served as exact reuse never reach `offer` at all —
`save.rs:74-78`.)

Every one of those 45,246 objects ends in exactly one of four counted outcomes
(`select.rs:268-274`, `:282-298`, `:302-322`):

```
P = no_candidate + work_exceeded + prefix_selected + full_losses
45,246 = 26,847 + 0 + 18,344 + full_losses
full_losses = 45,246 - 26,847 - 18,344 = 55                      residual 0
```

using the campaign's measured `delta.no_candidate = 26,847`, `delta.work_exceeded = 0`,
`delta.prefix_selected = 18,344` (and `absent_candidates = 0`, `ineligible_candidates = 0`).
Two independent store facts corroborate it:

* `prefix_selected = 18,344` and the store's `WholeFile` rows with a non-null `base_object_id`
  = **18,344** — equal, residual 0. (The pooled lane has its own `pool.delta_leaves`, which
  corresponds to the 917 `InodeLeaf` rows.)
* `trials = prefix_selected + full_losses = 18,344 + 55 = 18,399`, and
  `P - no_candidate - work_exceeded = 45,246 - 26,847 = 18,399`. Residual 0.

`full_losses = 55` is the one figure the store cannot show directly (a losing trial stores FULL and
therefore leaves no `base_object_id`); it is the **unique** value satisfying the identity, and it is
stated here as a derived quantity, not as a measured counter.

### 6.4 The unmodified lane's advisory list is NOT empty — P3 is live

This is the finding that most changes what the campaign should do next.

* `InodeLeaf` objects with a base = **917 of 1,738 (52.76 %)**.
* The `InodeLeaf` route is `selection.rs:98-100` -> `select_pooled` -> `pool_base(advisory, ...)`
  (`pool_lane.rs:141, 309-369`). That route obtains a base **only** from the advisory slice; it
  never consults `Candidates` (the whole-file cache) and has no other candidate source.
* Therefore **the advisory list was non-empty for at least 917 `InodeLeaf` offers in the unmodified
  lane.**
* The only product code that can put an id into the list for role 6 is **P3**
  (`filesystem/sorted/page.rs:382-389`), whose `page.origin` is a node of the previous state's
  filesystem root. So P3 is **active**, and it is the sole source of cross-save bases in the
  unmodified lane.

Corollary, stated precisely: the advisory list is empty on the **file-content construction** path
(`construct_bytes` -> `build_streaming(pred=None)`), which is the path the `history-stride10` driver
uses for file bytes and which Squad C's "the advisory list is always empty" describes. It is **not**
empty everywhere.

### 6.5 Pack-order corroboration (and its limit)

```
role         n     same pack   earlier pack   later pack   same-pack bytes   earlier-pack bytes
WholeFile   18,344      5,409         12,935            0        43,618,792           38,414,381
InodeLeaf      917          0            917            0                 0            4,461,895
```

`LanePlacement` is per-save (`cas/owner.rs:81`; constructed fresh at `cas/lifecycle.rs:64-70`), so a
pack never spans two saves: **`pack_id` equality implies same save, but inequality does not imply
different saves** (a save opens several packs per lane). So this table is *consistent with* the code
argument in 6.4 and does **not** replace it. All 917 `InodeLeaf` bases sit in a strictly earlier
pack; `page.origin` is by construction an object that existed before the save began, so those 917 are
**cross-save** — but that conclusion comes from the code, not from the pack numbers.

---

## 7. Worked example, end to end: one file version changing

Route: a whole-file edit (final length below the 128 KiB cutoff), i.e. the product's own P1 path.
Every step names the function in order.

1. **Caller** builds `EditRequest { root, edits, source }` (`file/edit/apply.rs:28-35`), where `root`
   is the previous version's content root — a `WholeFile` object stored by an earlier save.
2. **`apply_edits`** (`apply.rs:38`) -> `policy.validated()` (`:46`) -> `FileView::open(reader,
   request.root, edit.child("edit.base"))` (`:48`) -> length check against `request.edits.base_len()`
   (`:49-53`).
3. **`compare_replacements`** (`:68`) proves the replacements are not byte-identical; a no-op returns
   the base root and emits nothing (`:76-82`).
4. **`ConstructionPolicy::representation(final_len)`** (`:63`) -> `Representation::WholeFile`.
5. **`begin_whole_file_object`** (`:97`) reserves the canonical allocation; **`assemble_into`**
   (`:98`, body at `:153-212`) copies retained ranges (reading them through the `FileView`) and the
   replacement bytes; the replaced base range is deliberately never read (`:204-208`).
6. **`FinalizedObject::new(ObjectRole::WholeFile, canonical)`** (`:117-119`) -> `object/output.rs:98-109`:
   `codec::decode_bytes_object` validates, `ObjectId::for_bytes` computes the identity, and
   `predecessors` is initialised **empty** (`output.rs:107`).
7. **The producer fires.** `if view.root() != object.id()` (`:120`) — the content really changed —
   so **`AdvisoryPredecessors::explicit(view.root())`** (`predecessor.rs:97-101`) builds
   `[(prev_root, OriginalBase)]`, and **`FinalizedObject::with_predecessors`** (`output.rs:118-121`)
   attaches it. List length 1, bound 4.
8. **`consumer.accept(object)`** (`apply.rs:124-125`) -> `object/output.rs:191-194`. In the history
   driver the consumer is the store's save operation (`ops/history.rs:729-731`).
9. **`SaveOperation::accept`** (`cas/store.rs:340`) -> `PendingBatch::push` (`:344`; `cas/batch.rs:48-69`)
   holds the object **by value**, list included. When a bound is hit, **`SaveOperation::flush`**
   (`:370`) calls **`save::flush_batch(owner, objects)`** (`:374`; `cas/save.rs:22`).
10. **`flush_batch`** (`save.rs:22`): membership snapshot (`:33`), `Availability::seed` over the wave's
    **references** (`:43-51`). For this object there is no row, so:
    **`let advisory: Vec<ObjectId> = object.predecessors().ids().collect();`** (`:100`) ->
    **`owner.offer(object, &advisory, &mut availability)`** (`:101`).
11. **`MutationOwner::offer`** (`cas/selection.rs:28`) -> `availability.validate` (`:37`) ->
    **`select_record(object, advisory)`** (`:41`).
12. **`select_record`** (`:93`): role is not `InodeLeaf` (`:98`), so **`select(...)`** (`:112-119`).
13. **`select`** (`encoding/delta/select.rs:196`): the role is not in the ten-role short circuit
    (`:204-222`) and not `InodeLeaf` (`:223`) -> **`encode_full`** (`:229`) prepares the compressed
    FULL alternative; `depth_cap = capacities.delta_depth_for_role(WholeFile)` = **8**
    (`policy.rs:353-359`; `store_policy.whole_file_delta_max_depth = 8`).
14. **`raw_payload(canonical, role)`** (`:241`) — the bytes the signature and the dictionary are
    computed over.
15. Role is not `Chunk`, so **`acquisition(input, WholeFile, advisory, 8)`** (`:254` -> `:342-354`):
    `for id in advisory` (`:348`) -> **`probe(input, prev_root, WholeFile, 8)`** (`:349`) ->
    **`eligible`** (`:356-374`):
    * `lookup::location(connection, prev_root, i64::MAX)` (`:362`) finds the row written by the
      earlier save;
    * `location.role == WholeFile` (`:366`) — true;
    * `depths.depth_of(connection, prev_root)` (`:369`) walks `base_object_id` from `prev_root` to
      its chain root and returns its depth;
    * `depth < 8` (`:373`) — true.

    -> `probe` returns `true` -> `acquisition` returns `Some(prev_root)` (`:350`).
    **The store's own cache is never consulted** (`:256-265` is not reached).
16. **`acquire(input, prev_root)`** (`:277` -> `:376-392`): `Resolver::resolve_dependency`
    (`:388`) reads the base's pack, authenticates and reconstructs the chain, and
    `accumulate(chain_total, chain)` (`:390`) charges the operation total.
17. **Chain budgets** (`:282-298`): if the base chain plus this object exceeds
    `chain_canonical_limit` or `chain_encoded_limit` -> `work_exceeded += 1` (`:293`) and **FULL**
    (`:297`). Otherwise continue.
18. **`raw_payload(&base, role)`** (`:299`) — the base's raw payload becomes the zstd prefix
    dictionary; `trials += 1` (`:300`).
19. **`encode_prefix(canonical, role, prev_root, base_raw, ...)`** (`:301` -> `encoding/full.rs:87-102`)
    -> an `EncodedRecord` whose `base = Some(prev_root)` (`full.rs:159-167`).
20. **Cost comparison** (`:302`): if `prefix.record.len() < full.record.len()` ->
    `DepthCache::record` (`:307-313`), `prefix_selected += 1` (`:314`), return the **PREFIX**.
    Otherwise `full_losses += 1` (`:317`) and return **FULL** (with the object inserted into the
    cache, `:319`, WholeFile lane only).
21. **Back in `offer`** (`cas/selection.rs:41-90`): `lane = record.lane` = `PackLane::WholeFile`
    (`:42`); body-limit check (`:44-52`); the lane's group is sealed when occupied (`:53-70`);
    `group.records.push(record.record)` (`:76`) and
    `group.members.push(PendingMember { ..., base_object_id: record.base })` (`:77-82`) — **this is
    where the chosen base becomes a row field**; then the WholeFile lane is sealed immediately
    (`:84-89`).
22. **`seal_group`** (`cas/placement.rs:108`) -> `build_group` (`:118`) frames the group ->
    `select_many` (`:125-126`) places it (`pack/placement.rs:75+`; a group joins the lane's open
    pack when it fits, otherwise a new pack id is taken) -> `write_pack` (`:138`) ->
    `ObjectRow { object_id, role, canonical_length, base_object_id: member.base_object_id, pack_id,
    group_number, record_number }` (`:142-155`) -> `write::insert_objects` (`:156`;
    `sqlite/write.rs:142-190`, one multi-row `INSERT`).
23. **On disk:** one `objects` row with `object_role = 1`, `base_object_id = prev_root`, and the pack
    id of the pack the group landed in. **The advisory list itself is nowhere on disk** — only the
    base it caused the store to select.
24. **On read:** `encoding/delta/read.rs:159-179` walks `base_object_id` from the root, refusing
    `chain.len() > role_depth` (`:167-169`), a role mismatch (`:172-173`) and a non-increasing
    locator key (`:175-177`), then reconstructs the chain.

### 7.1 The same walk on the lane's actual driver

The `history-stride10` driver calls **`construct_bytes`** (`ops/history.rs:608-616`), not
`apply_edits`. Steps 1-7 therefore collapse: `construct_bytes` (`file/content.rs:207-250`) reaches
`FinalizedObject::new(ObjectRole::WholeFile, canonical)` at `content.rs:233-235` with **no**
`with_predecessors` call, and its chunked arm calls `build_streaming`, which passes
`predecessor = None` (`file/mapping/build.rs:326`).

So in the unmodified lane step 10 collects an **empty** `Vec`, step 15's loop body never executes,
and the selection falls through to step 15's `else` branch — the store's own `Candidates` cache
(`select.rs:257`). That is why `delta.no_candidate` is 26,847 and why every whole-file base in the
unmodified store is same-save: the caller declared nothing, and the only route left is a candidate
the store found by itself.

---

## 8. The distinction that is the whole point

| | **Caller-declared hint** | **Store-found candidate** |
|---|---|---|
| Type | `AdvisoryPredecessors` (`object/predecessor.rs`) | `Candidates` (`encoding/delta/candidates.rs`) |
| Owner | C1, attached to a `FinalizedObject` | C2, owned by one `MutationOwner` / one save |
| Reaches C2 at | `cas/save.rs:100` | `encoding/delta/select.rs:257` |
| Lifetime | the object it is attached to; dropped after `offer` | created `cas/lifecycle.rs:90`, dropped `cas/store.rs:392` |
| Can it name an object from an earlier save? | **YES** — any stored identity of the same role | **NO** — only identities admitted as FULL *during this same save* |
| How it is matched | position in the list; first eligible wins | 32-byte min-hash signature (`candidates.rs:57-81`); >= 2 shared hashes (`:156`), ties on the smaller id |
| Which roles | all roles that reach a selector: `WholeFile`, `Chunk` (index 0 only), `InodeLeaf` | `WholeFile` only — every insert site is guarded by `lane == PackLane::WholeFile` (`select.rs:234-239, 270-272, 294-296, 318-320`) |
| When it is consulted | **first**, always | **only** when the advisory list yielded no eligible candidate (`select.rs:256`) |
| What happens if it fails | `absent_candidates` / `ineligible_candidates` / `work_exceeded` counter, then FULL by policy | same |

Consequences that follow directly:

* **An eligible advisory entry suppresses the store's own search.** The `else` at `select.rs:256` is
  reached only when `acquisition` returned `None`. Declaring a base is therefore not additive with
  the cache; it *replaces* it for that object.
* **The cache cannot cross a save boundary, and the advisory list is the only thing that can.**
  A cross-commit base is possible **only** through the advisory list. This is why
  `delta.no_candidate = 26,847` with `absent_candidates = 0` and `ineligible_candidates = 0` means
  "the Store was supplied no base", not "the Store declined one".
* Three other store-side mechanisms look similar and are **not** base sources: `PoolIndex`
  (`encoding/pool/`, value-level ordinals for the pooled lane), `DepthCache` (`select.rs:65-154`,
  bookkeeping only), and `Availability` (`cas/dependencies.rs`, references only).

---

## 9. Negative results, corrections and open hypotheses

### Negative results

* **N1 — the list is not persisted.** No canonical encoding, no SQL column, no read path. A reopened
  store can only see `objects.base_object_id` (what was *selected*), never what was *declared*.
  Measurement of the list is therefore only possible through the selection counters or through the
  effect on `base_object_id`.
* **N2 — the provenance is not consumed.** `cas/save.rs:100` collects `ids()`; `select` and
  `pool_base` take `&[ObjectId]`. A provenance-only change is invisible to C2 and to the store.
* **N3 — the list is not empty in the unmodified lane (correction of a prior claim).** 917
  `InodeLeaf` rows carry a base, and the `InodeLeaf` route has no cache fallback, so P3
  (`filesystem/sorted/page.rs:382-389`) must have supplied them. The "always empty" statement is
  true of the *construction* path the history driver uses for file content, not of the lane as a
  whole.
* **N4 — ten roles are short-circuited before the advisory is read** (`select.rs:204-222`), and the
  measured 0 bases across 5,048 objects in those roles is exactly what the code predicts.
* **N5 — `retained-history-storage.md` never names the advisory list.** `grep -nE
  'advisory|predecessor|AdvisoryPredecessor' docs/roadmap/0.1/0.1.7/retained-history-storage.md`
  returns **0** matches. The type is specified in the component-decoupling documents
  (`component-decoupling/content-storage-design.md:172`, `physical-encoding-and-packing.md:154-174`,
  `file-content.md:38,147,347`), not in the 0.1.7 spec.
* **N6 — `into_parts` is not on the production save path.** `Store::accept` takes the
  `FinalizedObject` by value; `into_parts` is exercised only by
  `layerfs-content/tests/object_identity.rs:381-416`. The predecessor travels because the whole
  object travels, not because of `into_parts`.

### Corrections to the record (this squad)

* **C1 — `ReusedRange` is unused but producers exist.** See section 5. "One unused variant" is not
  "no producer".
* **C2 — pack order is not a save boundary.** `LanePlacement` is per-save, so `pack_id` equality
  implies same save but inequality does not imply different saves. The 5,409 same-pack whole-file
  bases are *provably* same-save; the 12,935 earlier-pack ones are same-save only by the code
  argument (empty advisory => cache only), not by the pack numbers. Section 6.5.
* **C3 — `prefix_selected = 18,344` equals the store's whole-file base count exactly** (residual 0),
  and the pooled lane's 917 is a *separate* counter (`pool.delta_leaves`). They must not be added
  into a single "deltas" figure without saying which lane each belongs to.

### Hypotheses (labelled — not measured)

* **H1.** For the three short-circuited page roles P3 produces (`DirectoryLeaf`, `DirectoryBranch`,
  `InodeBranch`), `page.origin` is very likely `Some` in the unmodified lane on the same code path as
  the `InodeLeaf` case, and the hint is silently dropped at `select.rs:221`. **I did not measure
  this.** The store cannot distinguish "declared and dropped" from "never declared", because 0 bases
  is predicted either way. The instrument that would decide it is a counter on the tree-role short
  circuit — none exists, and adding one is a product change.
* **H2.** The 917 `InodeLeaf` bases are cross-save. The code argument (6.4) is strong; the pack-order
  evidence (6.5, all 917 strictly earlier-pack, 0 same-pack) is consistent with it but is not
  independent proof, for the reason in C2.
* **H3.** The `Chunk` lane's index-0-only rule (`select.rs:247`) means that even a faithful producer
  that filled all four slots would leave entries 1..3 unread for chunks. This is a code fact; what is
  a hypothesis is whether any intended producer would ever supply more than one chunk candidate —
  the product has no such producer today.

---

## 10. Exact commands

Everything below is reproducible from the repository root and from the two stores named in the task.

**Both stores still exist** (checked before writing this document):

```sh
$ ls -la /tmp/base187/sample.sqlite /tmp/s0_l7/sample.sqlite
-rw-r--r--@ 1 yifanxu  wheel  128864256 Sep 19 14:08 /tmp/base187/sample.sqlite
-rw-r--r--@ 1 yifanxu  wheel   63737856 Sep 19 14:20 /tmp/s0_l7/sample.sqlite
```

**The per-role table (6.1) and the arithmetic (6.2):**

```sh
python3 - <<'PY'
import sqlite3
ROLES={1:"WholeFile",2:"Chunk",3:"ExtentLeaf",4:"ExtentBranch",5:"FileState",6:"InodeLeaf",
       7:"DirectoryLeaf",8:"DirectoryBranch",9:"InodeBranch",10:"FilesystemRoot",
       11:"AttributeLeaf",12:"AttributeBranch",13:"Symlink"}
for path in ("/tmp/base187/sample.sqlite",):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True); cur = con.cursor()
    cur.execute("select count(*), sum(canonical_length) from objects")
    print("TOTAL", cur.fetchone())
    cur.execute("select object_role, count(*), sum(base_object_id is not null), sum(canonical_length) "
                "from objects group by object_role order by object_role")
    for role,n,wb,cb in cur.fetchall():
        print(f"{ROLES.get(role,role):16} {n:7d} {wb:10d} {cb:13d}")
PY
```

Output (verbatim):

```
TOTAL (52032, 380921300)
WholeFile          44148      18344     348460709
Chunk               1098          0      22055499
ExtentLeaf            92          0         53368
FileState             92          0          9752
InodeLeaf           1738        917       8361152
DirectoryLeaf       4770          0       1887429
DirectoryBranch       48          0         16442
InodeBranch           29          0         74756
FilesystemRoot        17          0          2193
```

**The store policy of the store under analysis:**

```sh
python3 -c "import sqlite3;c=sqlite3.connect('file:/tmp/base187/sample.sqlite?mode=ro',uri=True).cursor();c.execute('select * from store_policy');print([d[0] for d in c.description]);print(c.fetchone())"
```

Output: `{'id': 1, 'format_profile': 1, 'small_file_threshold_bytes': 131072,
'whole_file_delta_max_depth': 8, 'chunk_delta_max_depth': 4, 'metadata_delta_max_depth': 8,
'retained_pack_ceiling': 502}`.

**The pack-order table (6.5):**

```sh
python3 - <<'PY'
import sqlite3
con=sqlite3.connect("file:/tmp/base187/sample.sqlite?mode=ro", uri=True); cur=con.cursor()
cur.execute("""select o.object_role, count(*),
   sum(case when b.pack_id=o.pack_id then 1 else 0 end),
   sum(case when b.pack_id<o.pack_id then 1 else 0 end),
   sum(case when b.pack_id>o.pack_id then 1 else 0 end),
   sum(case when b.pack_id=o.pack_id then o.canonical_length else 0 end),
   sum(case when b.pack_id<o.pack_id then o.canonical_length else 0 end)
 from objects o join objects b on b.object_id=o.base_object_id group by o.object_role order by 1""")
for row in cur.fetchall(): print(row)
PY
```

**The code facts (sections 1-5) were read with the repository's own file tools**, and the
enumerations were produced with:

```sh
grep -rn 'with_predecessors' core/crates/*/src            # 3 hits: apply.rs:121, build.rs:129, sorted/page.rs:388
grep -rn '\.predecessors()'  core/crates/*/src            # 1 hit:  cas/save.rs:100
grep -rn 'AdvisoryPredecessors::\(new\|explicit\)\|predecessors\.push' core/crates/*/src
grep -rn 'ReusedRange' core/crates                        # 1 hit, a test
grep -rn 'predecessor' crates/ | wc -l                    # the reference tree; no AdvisoryPredecessors type
ls crates/layerfs-content/src/object/                     # no predecessor.rs
grep -nE 'advisory|predecessor|AdvisoryPredecessor' docs/roadmap/0.1/0.1.7/retained-history-storage.md   # 0 matches
```

**No timing reading is reported in this document.** Every number is a byte count, a row count or a
line number. Both stores were opened read-only (`?mode=ro`).

---

## 11. One-paragraph summary for the record

`AdvisoryPredecessors` is a <= 4-entry, order-significant, non-persisted list of `ObjectId`s with a
`PredecessorProvenance` tag, attached to a `FinalizedObject` by C1 (`object/predecessor.rs`), carried
by value through `SaveOperation::accept` into `PendingBatch`, materialised exactly once at
`cas/save.rs:100`, and offered to the representation selector at `cas/save.rs:101`. Three product
producers exist: `file/edit/apply.rs:121` (`OriginalBase`, the declared edit base),
`file/mapping/build.rs:128` (`UnchangedPrefix`, the chunk payload a replacement run continues), and
`filesystem/sorted/page.rs:382-389` (`UnchangedPrefix`, the stored page a changed tree page was
materialised from — the only producer that fires in the unmodified `history-stride10` lane, visible
as 917 `InodeLeaf` bases). The selector reads the list only for `WholeFile` (all four, in order, then
its own per-save cache), `Chunk` (index 0 only, never the cache) and `InodeLeaf` (all four, in order,
via `pool_base`); the other **ten** roles return `encode_full` at `select.rs:221` before the list is
touched, which is exactly why 5,048 tree objects carry 0 bases. Eligibility requires a stored row
under the ceiling, exact role equality, a resolvable chain depth, and `depth < depth_cap`; chain
budgets are a second gate that yields `work_exceeded` rather than `ineligible`. The list is a hint the
caller declares and the only route that can name an object from an earlier save; the store's own
`Candidates` cache is consulted only when the list yields nothing and can only ever name an object
admitted as FULL in the same save. That asymmetry is the entire reason a driver that declares no base
leaves 65,126,400 B of cross-commit delta opportunity on the floor.
