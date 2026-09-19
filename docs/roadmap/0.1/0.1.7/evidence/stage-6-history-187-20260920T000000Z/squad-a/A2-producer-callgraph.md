# A2 — the producer: where does each tree declare a delta base?

> **Class: `diagnostic`.** Every number below is a diagnostic reading of an existing
> artifact plus a source-derived call graph. Nothing here is admission evidence, and
> nothing here was produced by a registered gate.
>
> Squad A2 of the #187 retained-history storage-loss investigation.
> Repo `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` at HEAD `66bce8378` (2026-09-19 14:04:57).
> Product source under `core/crates/` and `crates/` was **not modified**; this squad
> wrote one Markdown report and three throwaway `/tmp` analysis scripts.

---

## 0. What this report answers, and the one correction to the prompt's framing

The prompt's line numbers for the two delta-base routes are **C2 (storage)**, not C1:

```text
cas/save.rs:100              -> core/crates/layerfs-storage/src/cas/save.rs:100                  (C2)
encoding/delta/select.rs:342 -> core/crates/layerfs-storage/src/encoding/delta/select.rs:342    (C2)
encoding/delta/candidates.rs -> core/crates/layerfs-storage/src/encoding/delta/candidates.rs    (C2)
```

Both files live in `layerfs-storage`, not `layerfs-content`. The producers are in C1
(`layerfs-content`); the consumer is in C2. The whole call graph crosses that boundary
exactly once, at `FinalizedObject::predecessors()`.

Four questions are answered in §6, each with its own arithmetic or its own code proof.

---

## 1. Lineage: which v0.1.6 is `crates/`

**Confirmed: the reference tree is tag `v0.1.6` and its entire product source is
byte-identical to it.**

```console
$ git rev-parse v0.1.6^{commit}
44cf748486863ab7c21ca47e731bd88e2b9a7b4a
$ git for-each-ref --format='%(refname:short) %(objecttype) %(objectname:short) -> %(*objectname:short)' refs/tags/v0.1.6
v0.1.6 tag dbdf0fed6 -> 44cf74848
$ git log -1 --format='%H %ad %s' v0.1.6
44cf748486863ab7c21ca47e731bd88e2b9a7b4a Wed Sep 16 10:09:05 2026 Prepare the v0.1.6 release record and versioned manual
$ git diff --stat v0.1.6 HEAD -- 'crates/*/src/'
                       <-- empty
$ git diff --name-status v0.1.6 HEAD -- crates/
A  crates/layerfs-content/examples/rope_edit_oracle.rs
A  crates/layerfs-content/examples/rope_edit_timing.rs
A  crates/layerfs-content/examples/stage5_component_reference.rs
A  crates/layerfs-content/tests/stage5_reference_fixtures.rs
A  crates/layerfs-workspace/tests/deep_history_diagnostic.rs
$ git ls-tree -r --name-only v0.1.6 -- crates/ | grep -c '/src/'   ->  155
$ git ls-tree -r --name-only HEAD  -- crates/ | grep -c '/src/'   ->  155
```

**How this confirms it, in one sentence:** the 155 `crates/*/src/` files at HEAD are
byte-identical to the 155 at tag `v0.1.6` (empty `git diff`), and the only five
`crates/` files that differ are four examples/tests added later plus one
`tests/`-only history diagnostic. Therefore `crates/layerfs-layerstack-store/src/objects.rs:3311`
and `crates/layerfs-workspace/src/changes.rs:1837` are v0.1.6 source, not a later
divergence. The root workspace manifest also still declares `version = "0.1.6"`.

`set_physical_predecessor` **is** at line 3311 (verified by `git show v0.1.6:...`,
identical to the working tree) — the prompt's line is correct.

---

## 2. v0.1.7 core — the producer call graph (C1 → C2)

### 2.1 The C2 consumer side: exactly one entry point

```text
core/crates/layerfs-storage/src/cas/save.rs:100
    let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
    owner.offer(object, &advisory, &mut availability)          (:101)
      |
      +-- cas/selection.rs:28  MutationOwner::offer(object, advisory, availability)
            |
            +-- cas/selection.rs:41  self.select_record(object, advisory)
                  |
                  +-- role == InodeLeaf  -> cas/pool_lane.rs:60 select_pooled
                  |                          -> pool_lane.rs:141 pool_base(advisory, ...)
                  |                             -> pool_lane.rs:319  for id in advisory { ... }
                  |
                  +-- otherwise -> encoding/delta/select.rs:196 select(.., advisory, ..)
                        |
                        +-- select.rs:204-222  tree roles (ExtentLeaf/Branch, FileState,
                        |     DirectoryLeaf/Branch, InodeBranch, FilesystemRoot,
                        |     AttributeLeaf/Branch, Symlink)
                        |     -> encode_full(...) and RETURN.  ***advisory is never read***
                        |
                        +-- select.rs:223-228  InodeLeaf -> Integrity error (unreachable)
                        |
                        +-- select.rs:247-253  Chunk -> advisory.first() only, one probe
                        |
                        +-- select.rs:254      WholeFile -> acquisition(input, role, advisory, cap)
                                               select.rs:342-354  for id in advisory { probe }
                                               if none acquired -> select.rs:257
                                                   input.candidates.find(id, signature)
```

Note what `save.rs:100` does *not* do: it calls `.ids()` and **discards the provenance**.
Nothing in C2 reads `AdvisoryPredecessor::provenance()`; the only mention of
`predecessor` in all of `layerfs-storage/src` outside this file is a comment
(`encoding/delta/select.rs:218`). Provenance is a C1-side vocabulary today.

### 2.2 The three C1 producers, and the fourth non-producer that matters

| # | site | role of the object | base it names | provenance | same-save or cross-save? | reachable from the history lane? |
| --: | --- | --- | --- | --- | --- | --- |
| P1 | `layerfs-content/src/file/edit/apply.rs:121` `object.with_predecessors(AdvisoryPredecessors::explicit(view.root())?)` | `WholeFile` | `view.root()` — the root of the immutable edit base, opened at `apply.rs:48` through the caller's `AuthenticatedObjects` | `OriginalBase` (`predecessor.rs:99`) | **cross-save** when the base root was stored by an earlier save (it is read back through the provider, not from memory) | **NO** — only on `apply_edits` |
| P2 | `layerfs-content/src/file/mapping/build.rs:127-129` `push_chunk(raw, predecessor, ..)` | `Chunk` | `rightmost_payload(&mut objects, left)` computed at `apply.rs:309-314` — the rightmost payload extent of the *left* part of the tree being rebuilt | `UnchangedPrefix` (`build.rs:128`) | same-save in the ordinary case; the left subtree may itself contain a reused stored payload, so it is *not* provably same-save | **NO** — only on `apply_edits` (its only product caller is `apply.rs:325`; `build.rs:326` passes `None`) |
| P3 | `layerfs-content/src/filesystem/sorted/page.rs:382-389` `if let Some(origin) = page.origin { push(origin, UnchangedPrefix) }` | every role the sorted-page builder emits, incl. `InodeLeaf`, `DirectoryLeaf/Branch`, `InodeBranch` | `page.origin` = the **stored page id being edited** (`merge.rs:188 page.origin = id`; `page.rs:473 page.origin = Some(id)` in `page_from_wire`) | `UnchangedPrefix` (`page.rs:386`) | **cross-save** — the origin page was read from the base tree, i.e. an earlier save | **YES**, via `update_filesystem` |
| P4 | `layerfs-content/src/file/content.rs:207-250` `construct_bytes` | `WholeFile` (line 235) and `Chunk` (line 247) | **nothing.** `FinalizedObject::new(role, canonical)` leaves `predecessors: AdvisoryPredecessors::new()` — empty (`object/output.rs:107`) | — | — | **YES — this is the lane's whole-file path** |

```console
$ grep -rn 'AdvisoryPredecessors::' core/crates/*/src/
core/crates/layerfs-content/src/object/predecessor.rs:97   (the type's own constructor)
core/crates/layerfs-content/src/file/edit/apply.rs:121     P1
core/crates/layerfs-content/src/file/mapping/build.rs:127  P2
core/crates/layerfs-content/src/filesystem/sorted/page.rs:383  P3
$ grep -rn 'PredecessorProvenance::' core/crates/*/src/
object/predecessor.rs:99        OriginalBase     (explicit())
filesystem/sorted/page.rs:386   UnchangedPrefix
file/mapping/build.rs:128       UnchangedPrefix
```

`ReusedRange` is constructed in **one test only** (`layerfs-storage/tests/delta_payload.rs:131`).
The issue-#186 comment 5739744053 inference is therefore wrong in the way the prompt says:
**one provenance variant is unused; all three producer sites construct the mechanism.**
That error is not repeated or built on here.

### 2.3 What the history lane actually calls

```text
core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs
  :578  construct_bytes(policy, &capacities, bytes, &mut consumer, scope)   <- whole-file objects, NO base
  :664  FilesystemObjects::new(&provider, &mut consumer)
  :670  build_filesystem(&mut objects, &input, backing)      (state 1, base: None)
  :671  update_filesystem(&mut objects, &input, backing)     (states 2..17, base = previous root)
  :679  consumer.cloned_object(*id)  -> the FinalizedObject, predecessors and all
  :689  operation.accept(object)
```

So the lane reaches **P3** (page origins, cross-save) and **P4** (whole-file bytes, no base).
It never reaches **P1** or **P2**, which live behind `apply_edits`.

### 2.4 The candidate cache's ownership, from code

```text
cas/store.rs:240      Store::begin_save
cas/store.rs:243-244  MutationOwner::acquire(connection, capacities, Arc::clone(&self.pool_index))
cas/lifecycle.rs:90   candidates: Candidates::new()?          <- a FRESH cache per save
cas/owner.rs:103-105  /// Admitted-FULL winner cache: owned by this operation, bounded and
                      /// dropped with it, so a failed save can never leave a partly
                      /// advanced cache usable.
cas/store.rs:381      SaveOperation::finish(mut self, ..)
cas/store.rs:392      self.owner = None;                      <- owner and cache dropped
```

Writers/reader of the cache, all inside `encoding/delta/select.rs`, all four writes
guarded by `lane == PackLane::WholeFile`:

| line | event | guard |
| --: | --- | --- |
| 236 | `depth_cap == 0` -> store FULL | `lane == WholeFile` |
| 271 | no candidate acquired -> store FULL | `lane == WholeFile` |
| 295 | chain budget exceeded -> store FULL | `lane == WholeFile` |
| 319 | trial lost the cost comparison -> store FULL | `lane == WholeFile` |
| 257 | `find` — consulted only on the `_` arm of the role match, i.e. only `WholeFile` | reachable for WholeFile only |

**Cross-save candidate in core: impossible by construction.** The cache is created with
the save, filled only by objects *that same save* is admitting, and dropped with the
save. Nothing copies it into the Store; the only Store-owned cross-save cache in core
is the **pooled-value index** (`cas/owner.rs:117 pool_index: Arc<Mutex<PoolIndex>>`,
`cas/store.rs:244`), which answers *value* dedup inside the InodeLeaf lane and is not a
delta-base candidate cache (`pool_base` iterates `advisory` only, `pool_lane.rs:319`).

---

## 3. v0.1.6 reference — the producer call graph

```text
crates/layerfs-workspace/src/changes.rs
  :1402  StableFileInputs::prepare_page(&self, page, removed, writer)
           :1417-1425  before[slot] = base record of this path from FrontierInodes::base_records(core, self.base_inodes, ..)
           :1441-1507  for paths with no base record: walk self.base_root's namespace
                       (filesystem::namespace -> directory_lookup_many) to find the path's
                       PREVIOUS-COMMIT inode record
           :1513-1515  base = prior[slot] filtered to InodeKind::RegularFile -> record.content_root
           :1516-1523  else, if the file is new and small: RemovedSmallCandidates::find(basename)
                       -> the content root of a DELETED file with the same basename
           :1524-1527  if let Some(base) = base { encoded[32..64] = base.as_bytes(); has_predecessor = true }
  :1555  produce_file(..)
           :1582-1586  predecessor = Some(FileContentRoot(ObjectId::from_bytes(&prepared[32..64])))
  :1814  FrozenFile::build(before, predecessor, correspondence_reserved, captured, partitions)
           :1836-1842  if let Some(predecessor) = predecessor {
                         objects.set_physical_predecessor(self.reader.clone(), predecessor, correspondence_reserved)? }

crates/layerfs-layerstack-store/src/objects.rs
  :3311  ObjectBuffer::set_physical_predecessor(reader, root, operation_reserved)
           :3318  self.objects.small_predecessor = Some(root.0);            <- whole-file lane
           :3320-3324  if the predecessor is already Small content, stop here
           :3351-3356  self.objects.predecessor = Some((reader, FileStateRoot(root.0), ..))  <- rope/chunk lane
  :2812-2814  small_predecessor = self.small_predecessor.or(self.predecessor.root)
  :2834-2840  push(): if object.is_small_content() { prior_ids[0] = small_predecessor }
  :2841-2846  else, attach the PredecessorCursor over the predecessor's FileStateRoot

crates/layerfs-layerstack-store/src/objects/admission.rs
  :415-421  batch distinct prior_ids[0] -> db.object_locations
  :441-450  if let Some(prior) = object.1.prior_ids[0] {
              db.small_predecessor(prior, locations[prior], object.bytes.len())   <- the whole-file base
            }
  :454-470  else { session.small_candidates.find(id, signature) }                 <- the cache fallback
```

**Two producers, one per lane, and both name a PREVIOUS-COMMIT root:**
the small/whole-file lane takes `prior_ids[0] = small_predecessor`, the rope/chunk lane
takes the `PredecessorCursor` over the predecessor's `FileStateRoot`.

Callers of `set_physical_predecessor` in the whole reference tree:

```console
$ grep -rn 'set_physical_predecessor' crates/
crates/layerfs-workspace/src/changes.rs:1837                 <- the only product caller
crates/layerfs-layerstack-store/src/objects/diagnostic.rs:399, :436, :476   <- inside #[cfg(test)] mod tests (starts :326)
```

### 3.1 v0.1.6's candidate cache is Store-owned and survives the save

```text
crates/layerfs-layerstack-store/src/schema.rs:77       idle_small_candidates: Mutex<Option<Candidates>>   (on StoreInner)
crates/layerfs-layerstack-store/src/schema.rs:368-378  StoreDb::take_small_candidates()
crates/layerfs-layerstack-store/src/schema.rs:380-390  StoreDb::return_small_candidates(cache)
crates/layerfs-layerstack-store/src/objects.rs:2292-2299  AdmissionSession::new:
        small_candidates: Some(Mutex::new(db.take_small_candidates().unwrap_or_else(Candidates::new)))
crates/layerfs-layerstack-store/src/objects.rs:2589-2597  impl Drop for AdmissionSession:
        if state == 1 { self.db.return_small_candidates(candidates) }
```

`objects/small_candidates.rs` is the **same structure as core's**
(`INDEX_BYTES = 128 KiB`, `SLOTS = 1024`, `REFERENCES = 8192`, `WINDOW = 16`, the same
`mix`/`signature`), with a header comment that says "Session-local" while the code stores
it on the Store. Core kept the structure and changed the owner.

The reference tree's own test states the cross-session consequence:

```text
crates/layerfs-layerstack-store/src/objects/admission/small_candidate_tests.rs:256
  selected_small_candidate_retained_handoff_rollback_and_cold_reopen
  :269  let mut owner = CheckedOutputAdmission::new(&db)      <- session 1 admits `base`
  :280  session.retain(); drop(session);                       <- cache returned to the Store
  :282  let next = prepare(target.clone());                    <- session 2, independent
  :283  assert!(next.objects[0].delta);                        <- session 2's target IS a delta
  :288  assert_eq!(db.small_physical_base(target.id).unwrap(), Some(base.id));  <- against session 1's object
  :313-314  let reopened = StoreDb::connect(&path); assert!(reopened.take_small_candidates().is_none());
```

---

## 4. The diff, as a graph

```text
                         v0.1.6 (crates/)                                v0.1.7 core (core/crates/)
                    ============================                    ==============================

  who knows "this path had a previous version at root R"?
      StableFileInputs::prepare_page  changes.rs:1402                  NOT PRESENT in core.
      (walks self.base_root's namespace;                                   core/ has content+storage+telemetry only;
       falls back to RemovedSmallCandidates by basename)                   the workspace/LayerStack layer is a later stage.
             |                                                                    |
             | FileContentRoot  (changes.rs:1582-1586)                            X   <-- the break
             v                                                                    v
  who records the declaration?
      ObjectBuffer::set_physical_predecessor objects.rs:3311            construct_bytes  content.rs:207-250
             |  small_predecessor / PredecessorCursor                            |  (P4) no base, empty predecessors
             v                                                                   |
  what carries it to admission?                                             apply_edits (P1/P2)  <- NOT on this lane's path
      prior_ids[0] / first_span   objects.rs:2834-2846                         |
             |                                                                   v
             v                                                            FinalizedObject.predecessors
      admission.rs:441-450 db.small_predecessor                        (populated only by P1/P2/P3)
             |                                                                   |
             |                                                          cas/save.rs:100 .ids()  (C2)
             |                                                                   |
             v                                                                   v
      stored base = previous commit's root                          advisory-first acquisition, then
      (cross-save, by construction)                                 the per-save candidate cache (same-save only)

  v0.1.6 cross-save base routes:  2 of 2 lanes  (small: small_predecessor; rope: PredecessorCursor)
  core    cross-save base routes: 1 of 3 roles  (InodeLeaf via P3 -> pool_base; NOT WholeFile, NOT Chunk)
```

---

## 5. Measurement M1 — same-save vs cross-save base attribution on the existing baseline

**This is a measurement of an artifact that already existed** (`/tmp/base187/sample.sqlite`,
produced 14:08, apparent 128,864,256 B). **No new lane run was made.** Byte readings are
load-independent; no timing is reported anywhere in this section.

### 5.1 Method (and why the attribution is exact)

1. The Store's `objects` table carries `object_id, object_role, canonical_length,
   base_object_id, pack_id, group_number, record_number` — so a base edge is readable
   without decoding the pack directory.
2. `trace.jsonl` carries `history.state.k.inserted` for k = 1..17. Their sum is
   364+1096+1197+1258+2842+2225+3130+1804+2408+2068+4153+4029+3863+5875+5884+4867+4969
   = **52,032** = exactly `select count(*) from objects`. Every object was written by
   exactly one state's save, and no object was written twice.
3. Order objects by `(pack_id, group_number, record_number)` and cut at the 17 cumulative
   `inserted` boundaries: 364, 1460, 2657, 3915, 6757, 8982, 12112, 13916, 16324, 18392,
   22545, 26574, 30437, 36312, 42196, 47063, 52032.
4. **Validation:** all 16 non-final boundaries fall exactly on the first object of a pack
   ("boundaries not at a pack start: []"). A save's objects are therefore whole packs, and
   the attribution is exact rather than approximate.
5. For each object with a non-null `base_object_id`, compare the state of the base to the
   state of the dependent: equal = **same-save**, lower = **cross-save**.

### 5.2 Result

| role | with base, **same-save** (n / canonical B) | with base, **cross-save** (n / canonical B) | no base (n / canonical B) |
| --- | --: | --: | --: |
| WholeFile | **18,344 / 82,033,173** | **0 / 0** | 25,804 / 266,427,536 |
| InodeLeaf | 0 / 0 | **917 / 4,461,895** | 821 / 3,899,257 |
| Chunk | 0 / 0 | 0 / 0 | 1,098 / 22,055,499 |
| DirectoryLeaf | 0 / 0 | 0 / 0 | 4,770 / 1,887,429 |
| DirectoryBranch | 0 / 0 | 0 / 0 | 48 / 16,442 |
| ExtentLeaf | 0 / 0 | 0 / 0 | 92 / 53,368 |
| FileState | 0 / 0 | 0 / 0 | 92 / 9,752 |
| InodeBranch | 0 / 0 | 0 / 0 | 29 / 74,756 |
| FilesystemRoot | 0 / 0 | 0 / 0 | 17 / 2,193 |
| **total** | **18,344 / 82,033,173** | **917 / 4,461,895** | 32,771 / 294,426,232 |

### 5.3 Arithmetic

```text
whole-file objects with a base             = 18,344
whole-file objects with a base, CROSS-SAVE =      0   ->  0.00 % of them, 0 B of 82,033,173 B
whole-file objects with a base, SAME-SAVE  = 18,344   -> 100.00 % of them, 82,033,173 B of 82,033,173 B
residual (unattributed base)               =      0 objects, 0 B

cross-save bases that exist at all         =    917 objects / 4,461,895 B, ALL of them InodeLeaf
   (these are P3 -> pool_base: a page origin read from the previous root)

objects    : 18,344 + 25,804 + 7,884 = 52,032                     residual 0
canonical  : 82,033,173 + 266,427,536 + 32,460,591 = 380,921,300 residual 0 B
other lanes: 1,738 + 1,098 + 48 + 4,770 + 92 + 92 + 17 + 29 = 7,884 objects
             8,361,152 + 22,055,499 + 16,442 + 1,887,429 + 53,368 + 9,752 + 2,193 + 74,756 = 32,460,591 B
```

**Cross-validation against an independent instrument.** These three buckets reproduce the
prompt's pack-directory decode **bucket for bucket, byte for byte**: whole-file with a
base 18,344 / 82,033,173 ✓; whole-file without one 25,804 / 266,427,536 ✓; other lanes
7,884 / 32,460,591 ✓; total 52,032 / 380,921,300 ✓. Two different instruments (SQL
`objects` rows + trace counters vs. a pack-directory decoder) agree exactly, with zero
residual, which is what licenses reading the same-save/cross-save split off this table.

### 5.4 Harness identity of the baseline (a caveat the parent needs)

```console
$ stat -f '%Sm %N' -t '%Y-%m-%d %H:%M:%S' core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs
2026-09-19 14:17:17 core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs
$ git status --porcelain
 M core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs      (+220 / -3, uncommitted)
?? docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/
$ git show HEAD:core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs | grep -c LAYERFS_HISTORY_ADVISORY
0
```

The baseline Store was written at **14:08**; `history.rs` was modified at **14:17** and the
switch is **not in HEAD**. The baseline trace's counter set (85 per-state counters only:
`changed_bytes, changed_paths, inserted, objects, root`) matches the committed harness,
which has neither `LAYERFS_HISTORY_ADVISORY` nor the `delta.*` counters. **The baseline is
therefore a pure product-path run: no advisory predecessor was declared for any object,
by the harness or by the product.** That is exactly what M1 measures.

The uncommitted Step-0 switch would inject, harness-side, the declaration
`AdvisoryPredecessors::explicit(previous version's root)` at `history.rs:683-687` on
objects obtained from `TreeStore::cloned_object` (which clones the whole
`FinalizedObject`, so the declaration does reach `cas/save.rs:100`). It *simulates* the
missing producer; it does not add one to the product.

---

## 6. Answers

### Q1 — In v0.1.6, which code path gives a whole-file object a base from a PREVIOUS commit's version?

**The declaration is written by `StableFileInputs::prepare_page`,
`crates/layerfs-workspace/src/changes.rs:1524-1527`; it is recorded by
`ObjectBuffer::set_physical_predecessor`, `crates/layerfs-layerstack-store/src/objects.rs:3311`
(line **3318**), and consumed by `admission.rs:441-450` (`db.small_predecessor`).**

Chain, with the exact lines:

| step | file:line | what happens |
| --: | --- | --- |
| 1 | `crates/layerfs-workspace/src/changes.rs:1513-1515` | `base = prior[slot]` filtered to `InodeKind::RegularFile` -> `record.content_root`; `prior` came from `FrontierInodes::base_records` / a namespace walk from `self.base_root` (the previous commit's root) at `:1417-1507` |
| 2 | `crates/layerfs-workspace/src/changes.rs:1516-1523` | fallback: a **new** small path takes the content root of a deleted file with the same basename (`RemovedSmallCandidates::find`) |
| 3 | `crates/layerfs-workspace/src/changes.rs:1524-1527` | `encoded[32..64].copy_from_slice(base.as_bytes()); has_predecessor = true` |
| 4 | `crates/layerfs-workspace/src/changes.rs:1582-1586` | `produce_file` decodes `prepared[32..64]` into `predecessor: Option<FileContentRoot>` |
| 5 | `crates/layerfs-workspace/src/changes.rs:1836-1842` | `FrozenFile::build` -> `objects.set_physical_predecessor(reader, predecessor, correspondence_reserved)` |
| 6 | `crates/layerfs-layerstack-store/src/objects.rs:3318` | `self.objects.small_predecessor = Some(root.0)` (and `:3351` the rope `PredecessorCursor` when the predecessor is not already Small) |
| 7 | `crates/layerfs-layerstack-store/src/objects.rs:2834-2840` | `if object.is_small_content() { object.1.prior_ids[0] = small_predecessor }` |
| 8 | `crates/layerfs-layerstack-store/src/objects/admission.rs:441-450` | `db.small_predecessor(prior, location, len)` -> the whole-file object's stored base |

Same-save or cross-save? **Cross-save.** `self.base_root` is the previous commit's
filesystem root, so the named content root was stored by an earlier save. The
rope/chunk lane gets the same treatment through the `PredecessorCursor` at `:3351` ->
`:2841-2846`.

### Q2 — Does v0.1.6's producer live in a layer core does not have yet (LayerStack)?

**Yes, and in two layers, neither of which core has.**

```console
$ ls core/crates/            ->  layerfs-content  layerfs-storage  layerfs-telemetry
$ sed -n '/\[workspace\]/,/^\[/p' core/Cargo.toml
members = ["crates/layerfs-content", "crates/layerfs-storage", "crates/layerfs-telemetry"]
$ sed -n '/\[workspace\]/,/^\[/p' Cargo.toml
members = [... layerfs-layerstack-store, layerfs-workspace, layerfs-sdk, layerfs-fuse,
           layerfs-daemon, layerfs-materialization, layerfs-monitor, layerfs-cli ...]
exclude = ["core"]
```

- The **decision** ("this path had a previous version, at this root") is made in
  `crates/layerfs-workspace` — the Workspace layer, absent from core.
- The **recording** (`small_predecessor`/`prior_ids[0]`, the small-chain format) is made in
  `crates/layerfs-layerstack-store` — a layer core replaced with `layerfs-storage`, whose
  analogue is the advisory predecessor.

Core does have a live cross-save producer for a *different* role (P3 -> `InodeLeaf`), so
this is not "the whole idea is missing"; it is "the workspace-level producer is missing".

### Q3 — Expected migration-stage consequence, or genuine omission in core?

**Both readings are defensible; they are about different layers. Decision: EXPECTED
MIGRATION-STAGE CONSEQUENCE, with one honest qualification that the qualification is
load-bearing for the parent's investigation.**

**The case for "genuine omission":**
1. The consumer side is fully migrated and works today: 917 cross-save `InodeLeaf` bases
   exist in the baseline Store, produced by core's own `sorted/page.rs:383` and consumed
   by core's own `pool_base`. The mechanism is not un-ported.
2. The gap is one declaration wide. `apply_edits` declares a base (`apply.rs:121`) and
   `construct_bytes` does not (`content.rs:207-250`), yet both produce `WholeFile` objects
   for the same lane and both feed the same `select` path.
3. The information needed is already inside core at the moment it matters:
   `update_filesystem` receives `base: Option<FilesystemRootId>` and each `InodeUpdate`
   carries both the path and the new `content_root`, and the old `content_root` is one
   lookup away in the base tree — which is exactly the walk core's own
   `filesystem::sorted` engine already performs to set `page.origin` (`merge.rs:188`).
   `page.origin` for an inode leaf *is* the previous version of that page.
4. Migration-tracker language already treats this as a known reachable-only-here fact:
   `docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-review-20260916T233008Z.md:110`
   — "The only producer of a `Chunk` advisory is `predecessor = rightmost_payload(left)`".

**The case for "expected migration-stage consequence":**
1. Core's C1 contract assigns predecessor retention to the **edit** path only.
   `file-content.md:147` requires, for the `Small -> small` row, "retain eligible whole-file
   predecessor information"; `file-content.md:152` requires of the `Complete input` row only
   "Consume all supplied bytes through the selected complete builder" — no predecessor
   clause, because a complete input has no declared base. `construct_bytes` is the
   complete-input entry point. **It is not violating its contract.**
2. The v0.1.6 behaviour is not a C1 behaviour at all. `prepare_page` decides the base from
   the *namespace* before any content object exists, and `set_physical_predecessor` hands
   it to the *store* as a physical hint. It bypasses the content-construction API entirely.
   That layer — Workspace — is a later migration stage; core is Stages 0–5.
3. `crates/` is explicitly a temporary reference (`core/AGENTS.md`: "Existing root
   `crates/` is a temporary reference, not a dependency, binary fallback or source
   include"), so a capability that exists only there is by definition not yet migrated.

**Decision.** The absence is expected at the *layer* that v0.1.6 put the producer in, and
the C1 contract as written does not require it on the complete-input path. It is **not** a
missing mechanism: the mechanism, its type, its provenance vocabulary and a live cross-save
producer are all present in core. What the history lane exposes is that **the only C1
entry point that can declare a whole-file base is `apply_edits`, and the lane does not use
it** — the workload feeds complete bytes with no declared base, so nothing declares one.
Whether that is "expected" or "a finding" therefore turns on a product decision the
migration has not recorded: *should C1's complete-input path accept a declared base?*
Today it cannot, and the contract does not ask it to.

**What would falsify this decision:**
- A later-stage plan that explicitly assigns "resolve the previous version of a path and
  declare it to the Store" to a named Workspace/adapter stage would make "expected"
  unambiguously correct. **Falsifier for the "omission" side.**
- Conversely, if the Stage 3–4 closure record claims C1's file-content component is
  *complete* against the v0.1.6 file-construction path, then `construct_bytes` having no
  base parameter is an omission with a document to point at. **Falsifier for the
  "expected" side.** I did not find such a claim in the closure record, and I did not
  search it exhaustively — this is the one check I would hand to a reviewer.
- A third falsifier, and the cheapest: if a *registered* 217 row exercises
  `construct_bytes` on a second version of an already-stored file and its golden numbers
  were derived from a base declaration, the declaration is missing from a measured path
  rather than from an unmeasured one.

### Q4 — Does the per-save delta candidate cache ever see a cross-save candidate in EITHER tree?

**Core: NO — proven from code and confirmed by measurement. v0.1.6: YES — by design, and
its own test asserts it.**

**Core, from code.** `Candidates` is created in `MutationOwner::acquire`
(`cas/lifecycle.rs:90`), which is called once per `Store::begin_save`
(`cas/store.rs:240-244`), and is dropped with the owner (`cas/store.rs:392 self.owner = None`
inside `finish(mut self)`). It has exactly four writers and one reader, all inside
`encoding/delta/select.rs` and all inside one `select` call for one save. There is no
accessor that returns it, no field that outlives the owner, and no path that copies it
into the Store. The comment at `cas/owner.rs:103-105` states the intent; the two lines
above are the proof.

**Core, from measurement.** If the cache could name a cross-save object, a cross-save
`WholeFile` base would exist. M1 counts **0 cross-save `WholeFile` bases out of 18,344**
(0 B out of 82,033,173 B), against **917** cross-save `InodeLeaf` bases from the *other*
route. The two routes are therefore cleanly separated in the data, and the cache route is
100 % same-save.

**v0.1.6, from code.** `idle_small_candidates` lives on `StoreInner`
(`schema.rs:77, 352`); `AdmissionSession::new` takes it (`objects.rs:2292-2299`) and
`Drop for AdmissionSession` returns it (`objects.rs:2589-2597`). The cache is consulted only
when there is no explicit predecessor (`admission.rs:454`), i.e. exactly where core
consults its own cache — but here it is warm from the previous save.

**v0.1.6, from the tree's own test.**
`small_candidate_tests.rs:256-317`: session 1 admits `base` and is dropped
(`retain` at `:280`); session 2 admits `target` and `assert!(next.objects[0].delta)`
(`:283`) with `db.small_physical_base(target.id) == Some(base.id)` (`:288`). A cross-session
delta through the cache, asserted by the reference tree itself. `:313-314` shows the
retention is in-memory on the Store handle, not persisted across a cold reopen.

**The one-sentence asymmetry:** the same 128 KiB content-keyed signature cache is
**per-save** in core and **Store-owned** in v0.1.6.

---

## 7. Hypotheses (explicitly labelled — none of these is a measurement)

**H1 (hypothesis, scaling argument).** The v0.1.6 gap is dominated by the whole-file FULL
bucket. Arithmetic: the v0.1.6/core allocated gap is
`130,863,104 - 49,344,512 = 81,518,592 B`. The whole-file *without* a base bucket stores
93,744,892 B. If those 266,427,536 canonical bytes had been delta-encoded at the **same
ratio the with-base bucket already achieves** (82,033,173 / 17,196,162 = 4.770x), they
would occupy 266,427,536 / 4.770 = 55,849,736 B, closing 37,895,156 B — **46.5 % of the gap**
— and leaving 43,623,436 B unexplained by this one mechanism.
**This is a bound, not a prediction.** The 4.770x is achieved against *same-save*
near-duplicates (small files sharing a state's content); stride-10 versions are ten
checkpoints apart, and the recorded stride-3 union/canonical ratio (below) shows how
little of this content is duplicated at all. Treat 4.770x as an unreachable upper bound.

**H2 (hypothesis).** The residual in H1 is not a second whole-file mechanism. The rest of
the Store is 32,460,591 B canonical in 8,953,237 B stored (3.63x) across 7,884 objects, so
the non-whole-file lanes are already compressed comparably to the whole-file with-base
bucket and cannot carry an 81 MB gap. Stated as a hypothesis because I did not measure
v0.1.6's role split.

**H3 (hypothesis, "not a workload difference" — the parent's argument, reproduced).**

```text
core,     stride10, 17 states: canonical 380,921,300 / union 371,937,306 = 1.02415
                               -> 8,983,994 B (2.36 % of canonical) is unreferenced by any final tree
v0.1.6,   stride3,  53 states: canonical 589,423,458 / union 583,508,923 = 1.01014
                               -> 5,914,535 B (1.00 %)
```

97.6 % of core's canonical bytes are referenced by the 17 final state trees, so almost none
of the 380.9 MB is redundant cross-state duplication: v0.1.6 storing the same content in
49.3 MB (7.720x) cannot be explained by exact reuse. It must come from delta encoding of
near-identical versions — which is precisely the route Q1/Q4 show is undeclared in core.

**H4 (hypothesis).** The headline ratio reproduces from the spec's own numbers:
`130,863,104 / 49,344,512 = 2.6520x` allocated, `128,864,256 / 49,315,940 = 2.6130x`
apparent; canonical/allocated is `2.9108x` in core against `7.7196x` in v0.1.6 for the
same 380,921,300 B. Labelled a hypothesis only because the v0.1.6 17-state canonical total
is inferred from the spec's "content pinned byte-identical" statement rather than re-run.

---

## 8. Negative results (what I ruled out, how, and with what number)

1. **Ruled out: "the candidate cache might carry a base across saves in core."**
   `cas/lifecycle.rs:90` + `cas/store.rs:392` + the four guarded writers in
   `select.rs`; measured consequence **0 of 18,344 whole-file bases are cross-save**.
2. **Ruled out: "core has no live cross-save producer at all."** It has one, and it fires:
   **917 objects / 4,461,895 B** of cross-save `InodeLeaf` bases, from
   `sorted/page.rs:383` -> `pool_base`. This is the number that makes "the mechanism is
   missing" false.
3. **Ruled out: "the tree-role advisories do something in C2."** `select.rs:204-222`
   returns `encode_full` before reading `advisory` for all ten tree roles. Measured
   corroboration: **0 of 4,770 `DirectoryLeaf`, 0 of 48 `DirectoryBranch`, 0 of 29
   `InodeBranch`, 0 of 17 `FilesystemRoot`, 0 of 92 `FileState`, 0 of 92 `ExtentLeaf`
   objects have a base**, even though P3 attaches a predecessor to every one of them.
   P3's cross-save hint is *inert* for every role except `InodeLeaf`.
4. **Ruled out: "the Chunk lane got a base from somewhere."** `Chunk` has exactly one
   route in C2 (`select.rs:247-253`, `advisory.first()`) and one producer (P2, behind
   `apply_edits`). Measured: **1,098 `Chunk` objects, 0 with a base, 22,055,499 B stored
   FULL.**
5. **Ruled out: "the baseline ran with the Step-0 advisory switch."** The switch is not in
   HEAD, the file was modified 9 minutes *after* the run, and the trace's counter set
   matches the committed harness. The 18,344 same-save deltas are the product's own work.
6. **Ruled out: "the v0.1.6 candidate cache is per-save."** `schema.rs:77/368-390`,
   `objects.rs:2292-2299/2589-2597`, and the tree's own cross-session test at
   `small_candidate_tests.rs:256-317`.
7. **Ruled out: "the reference tree is a later, diverged v0.1.6.x."** Empty
   `git diff v0.1.6 HEAD -- 'crates/*/src/'` over 155 files.
8. **Ruled out: "the prompt's line numbers point into C1."** `cas/save.rs` and
   `encoding/delta/` are in `layerfs-storage` (C2), not `layerfs-content` (C1).
9. **Not ruled out / not attempted:** I did not re-derive the corpus union numbers
   (371,937,306 / 44,240 / 583,508,923 / 589,423,458) — they are taken as given. I did not
   decode the pack directory; I reproduced its three buckets from a different instrument
   and got an exact match, which is a cross-check, not an independent derivation of the
   stored-byte split. I did not read the v0.1.6 evidence report that records the
   49,344,512 B figure. I did not run any lane.

---

## 9. Exact commands

```console
# lineage
git rev-parse v0.1.6^{commit}
git for-each-ref --format='%(refname:short) %(objecttype) %(objectname:short) -> %(*objectname:short)' refs/tags/v0.1.6
git diff --stat v0.1.6 HEAD -- 'crates/*/src/'
git diff --name-status v0.1.6 HEAD -- crates/
git show v0.1.6:crates/layerfs-layerstack-store/src/objects.rs | sed -n '3305,3320p'
git show v0.1.6:crates/layerfs-workspace/src/changes.rs | sed -n '1830,1845p'

# producer sites
grep -rn 'AdvisoryPredecessors::' core/crates/*/src/
grep -rn 'PredecessorProvenance::' core/crates/*/src/
grep -rn 'predecessor' core/crates/layerfs-storage/src/
grep -rn 'set_physical_predecessor' crates/

# harness identity of the baseline
stat -f '%Sm %N' -t '%Y-%m-%d %H:%M:%S' core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs
git status --porcelain
git diff --stat HEAD -- core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs

# measurement M1 (read-only SQLite + the existing trace; scripts in /tmp)
python3 /tmp/a2_packs.py     # pack/state boundary validation
python3 /tmp/a2_bases.py     # same-save vs cross-save base attribution
python3 /tmp/a2_arith.py     # the ratios quoted above
# the SQL is:
#   select object_id, object_role, canonical_length, base_object_id, pack_id, group_number, record_number
#     from objects order by pack_id, group_number, record_number
# against  sqlite3.connect('file:/tmp/base187/sample.sqlite?mode=ro', uri=True)
```

No product source file under `core/crates/` or `crates/` was created, modified or deleted by
this squad. No commit was made. No lane was run. No benchmark registry row was touched.

---

## 10. Residual risk

- **Single instrument for the same-save/cross-save split.** The split comes from the SQL
  `objects` table plus the trace's `inserted` counters. Its *bucket totals* are
  cross-validated against the pack-directory decode to the byte; the *split within* the
  whole-file-with-base bucket is not, because the pack directory does not carry state
  boundaries. A second instrument would be the per-state `delta.*` counters the
  uncommitted Step-0 harness now publishes — running the lane once at HEAD and once with
  `LAYERFS_HISTORY_ADVISORY=1` would confirm both the 0-cross-save finding and the
  counterfactual. That run is the parent's Step 0, not this squad's.
- **The state attribution assumes pack ids are allocated in save order.** Validated
  (all 16 boundaries land on pack starts) but not proven from the writer's source; I did
  not read `pack/placement.rs`.
- **Q3 is a judgement, not a measurement**, and it is labelled as one. The falsifier I
  would apply first is the Stage 3–4 closure record's completeness claim over C1's
  file-content component.
