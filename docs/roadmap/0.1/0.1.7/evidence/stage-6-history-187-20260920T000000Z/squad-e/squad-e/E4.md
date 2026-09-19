# E4 — the smallest change to `core/` that unlocks cross-commit bases

> **Status:** design deliverable, 2026-09-19. **Diagnostic, not admission evidence.**
> Repo `66bce8378` + uncommitted harness-only changes. **Nothing under `core/crates/` or `crates/` was
> modified by this work** — the only files written are this document, its duplicate name, and its
> instrument. **No timing is reported** (the machine was loaded; load average 5.05 at 15:12). Byte
> readings are load-independent and are used; no timing reading was taken.
> Labels: **[M]** measured by E4 today · **[C]** campaign-measured, quoted · **[D]** derived by reading
> code · **[H]** hypothesis · **[E]** estimate.

---

## 0. Verdict

1. **The declaration costs zero lines of core.** The boundary already carries a caller-supplied
   predecessor per object (`FinalizedObject::with_predecessors`, `AdvisoryPredecessors::explicit`,
   `PredecessorProvenance::OriginalBase`), C2 already forwards it (`cas/save.rs:100` →
   `encoding/delta/select.rs:342-354`), and a caller that attaches it already moves **65,126,400 B**
   [C]. Nothing new is needed: no type, no trait, no entry point, no column.
2. **The one core change that *is* required is not an interface change.** `DepthCache::cost_of`
   (`layerfs-storage/src/encoding/delta/select.rs:94-144`) under-reports a chain's depth **by one edge**
   whenever its walk stops at an already-cached entry instead of at the chain root. The writer then
   admits a base whose *true* depth equals the policy cap and writes a child at **cap + 1**, which the
   reader refuses. **4 changed lines in one function in one file; no format change.**
3. **The campaign's step0 §5 mechanism is a misdiagnosis.** "The writer admits `depth < cap` and the
   reader refuses `depth > cap`, both 8, so the writer produces a chain one edge deeper than its own
   policy" is **not** what the code does. The two bounds are consistent (writer ⇒ `edges ≤ cap`; reader
   accepts `edges ≤ cap`), the product's own test suite pins that, and the extra edge comes from the
   **under-reported depth value**, not from the bounds disagreeing. §1.3–§1.4 carry the arithmetic.
4. **Anything that puts the correspondence inside core is 10×–1000× bigger and needs an owner
   amendment.** A store-side key index (b) is a **schema-version change** plus an input C2's contract
   explicitly denies; a Workspace/layer stack (c) is an unbuilt component with **no specification**.

---

## 1. What I measured

### 1.1 Instrument and exact commands

Instrument: a read-only SQLite walk of the persisted `base_object_id` links, fail-closed on a base that
names a row the Store does not hold (`dangling`), in
`…/stage-6-history-187-20260920T000000Z/squad-e/squad-e/e4_chain_depth.py`.

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-e/squad-e/e4_chain_depth.py /tmp/s0_l7/sample.sqlite /tmp/base187/sample.sqlite
```

Persisted policy of each Store (identical in both):

```sh
sqlite3 "file:/tmp/s0_l7/sample.sqlite?mode=ro" "select id,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth from store_policy;"
# 1|1|131072|8|4|8
```

Product boundary test (external test target; no product source touched):

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test delta_chains
# test result: ok. 9 passed; 0 failed; 0 ignored
```

Both Stores were present and unmodified: **52,032 objects each**, `dangling = 0` in both.

### 1.2 The instrument reproduces the campaign's own counts exactly [M]

| quantity | E4 walk | campaign |
| --- | --: | --: |
| `/tmp/base187` role-1 objects with a base | **18,344** | `prefix_selected = 18,344` [C] |
| `/tmp/s0_l7` role-1 objects with a base | **34,300** | `prefix_selected = 34,300` [C] |
| role-6 (InodeLeaf) objects with a base, both Stores | **917** | "the only cross-save bases at all are 917 InodeLeaf objects" [C] |
| `/tmp/base187` deepest role-6 chain | 9 records = **8 edges** | "max chain depth 8, reached only by InodeLeaf (22 objects)" [C] |

Two independent instruments agree, so the depth walk below is sound.

### 1.3 The measured violation [M]

`/tmp/s0_l7/sample.sqlite` — the `l7` arm, the campaign's own measurement basis (63,737,856 B apparent):

```text
objects=52032  policy whole/chunk/metadata depth=(8, 4, 8)  dangling=0
role 1: with-base=34300  chain records (edges+1): 2:11530 3:7520 4:5258 5:3722 6:2672 7:1875 8:1304 9:415 10:4
role 6: with-base=917    chain records (edges+1): 2:361 3:190 4:137 5:81 6:55 7:44 8:27 9:22
objects whose base has true depth >= 8: 4
  child=32c1d33dc1fdb124 role=1 child_edges=9 base_edges=8 child_canonical=2661
  child=4bf6f7ac79acaf2c role=1 child_edges=9 base_edges=8 child_canonical=1992
  child=9059524f0a402ad0 role=1 child_edges=9 base_edges=8 child_canonical=1315
  child=99dcca9d0a6c360a role=1 child_edges=9 base_edges=8 child_canonical=1269
```

`/tmp/base187/sample.sqlite` — the unmodified registered lane: **0** such objects; deepest role-1 chain
1 edge; deepest role-6 chain 8 edges = the metadata cap, legal.

Arithmetic, from the Store's own rows:

* the role-1 record histogram sums to `11530+7520+5258+3722+2672+1875+1304+415+4 = 34,300` ✓ equals
  `prefix_selected`;
* **4** objects sit at **9 edges** while the persisted `whole_file_delta_max_depth` is **8**, and each of
  the four names a base at **8 edges**. An accurate writer may not do that: `eligible` is
  `depth < depth_cap` (`select.rs:373`, called from `:335`), and the product's own test pins the
  consequence — a base at `depth == cap` is counted `ineligible_candidates` and the dependent is stored
  FULL (`delta_chains.rs:79-94`, passing [M]). Therefore the depth value the writer consulted for those
  four bases was **≤ 7** while the true depth was **8**: **the depth source under-reported.**
* the only depth source on that path is `DepthCache::cost_of` (`select.rs:303-306`, `:369`, and the
  pooled lane's `pool_lane.rs:326`). **The [H] of step0 §5 is therefore proved by the writer's own
  output**, not by code reading alone.
* reading those four children back fails: `delta/read.rs:162-179` refuses when the chain exceeds
  `role_depth`, and a 9-edge chain is 10 records, so the refusal fires **[D]** — the gate that would have
  exercised them was never run (§1.5).

### 1.4 The writer/reader bounds are **consistent** — the campaign's framing is wrong [M]+[D]

* Writer: `eligible` admits a base iff `depth < depth_cap` (`select.rs:356-374`), so a correct writer's
  deepest child is `edges = cap`; `select.rs:303-313` records `base.depth + 1`.
* Reader: `read.rs:162-179` pushes the requested object first and evaluates `chain.len() > role_depth`
  **only while the current node still has a base**, breaking at the chain root before the check. It
  therefore fires exactly when `edges > cap`; **`edges == cap` is readable** (`read.rs:180` then reports
  `depth = chain.len() - 1`).
* Measured on the product itself [M]: `cargo +1.85.1 test … --test delta_chains` → **9 passed**,
  including
  * `a_reduced_depth_stops_at_its_own_boundary` (cap **2**): a 2-edge chain is **stored**, the next
    dependent naming it is `ineligible_candidates == 1, trials == 0`, and `read_objects` succeeds with
    `counters.max_depth == 2`;
  * `an_increased_depth_is_honoured_up_to_its_boundary` (cap **16**): a 16-edge chain is read back,
    `counters.edges == 16`.
* Second, independent confirmation [M]: `/tmp/base187` holds **22 role-6 objects at 8 edges = the metadata
  cap**, and the campaign reports that arm readable / PASS [C].

So with accurate depth reporting the two bounds agree. Step0 §5's sentence "the writer is producing a
chain one edge deeper than its own policy" describes the **symptom**, not the mechanism: the extra edge
comes from the under-reported depth.

**Why the existing suite never caught it [D]:** every depth test builds its chain one object per
`save_one`, and the `DepthCache` is created per save (`cas/lifecycle.rs:91`) and dropped with it. With
one object per save the cache is always empty at the query, the walk always terminates at the chain
root, and only the accurate branch runs. The defect needs a **cache hit strictly below the queried
object inside one save** — many dependent objects offered in one save, exactly what the history lane
does (16,815–34,300 prefix records per save, with the 4,096-entry cache cleared whole many times per
save).

### 1.5 The `l7` arm has no read-back evidence [M]

`/tmp/s0_l7/trace.jsonl` records exactly three gates: `g1.o1-chain-complete` **PASS**, `g4.swaps`
**PASS**, `g1.o3-pinned-counters` **INCOMPLETE**. A count of `"key":"g1.o2"` and of `"key":"g1.o4"`
returns **0** for both the `l7` and the `base187` traces: no sampled-bytes gate and no state-tree gate
ran. The `history.*` op **refuses** the verify phase outright (`ops/history.rs:426-428` returns
`Err("history.* verifies in a second invocation; run --phase verify")`), so the §7 final gate has no
implementation for this case yet.

Consequence: **"l7 completes" carries no readability evidence**, and the four unreadable objects were
never touched. `phases-perf.json` publishes `verification_ns = 5,935,750` — a diagnostic figure, not a
timing claim. **Prediction [H]:** running the §7 gate over `/tmp/s0_l7` aborts at the first of those four
files it samples. Unmeasured: the harness cannot run that phase yet.

### 1.6 Negative results kept

* The unmodified lane is not corrupted by the defect, it is **latent** in it: `/tmp/base187` has 0 objects
  whose base sits at true depth ≥ cap, because its whole-file lane never exceeds 1 edge [M]. Fixing the
  defect changes nothing in the registered lane's bytes.
* The defect is present in **both** lanes that share the cache: the payload lane (`select.rs:369`) and
  the pooled-metadata lane (`pool_lane.rs:315-331`, the same `depth < cap` rule and the same
  `DepthCache`). One fix covers both.
* The defect is **writer-side only**. Fixing it does not make the four already-written objects readable;
  there is no repair path, and adding one is a format/compatibility question.
* Raising the *policy* depth to ≥ 9 would make `/tmp/s0_l7` readable (configuration only, schema
  unchanged), but it accepts chains the writer should not have produced — a diagnostic stopgap, not a fix.
* I ran no timing, no new product run and no harness A/B. The four-object violation, the histograms and
  the boundary semantics are the whole of E4's new measurement.

---

## 2. The interface — nothing new is needed

**What must be told, by whom, in what form.** The caller that knows the previous state must attach, per
object it offers, the *previous version's content root of the same path* as an `OriginalBase` advisory
predecessor. That is the whole interface, and it already exists on both sides:

```rust
// C1 — core/crates/layerfs-content/src/object/predecessor.rs (unchanged, 102 lines)
pub const MAXIMUM_ADVISORY_PREDECESSORS: usize = 4;
pub enum PredecessorProvenance { OriginalBase, UnchangedPrefix, ReusedRange }
pub struct AdvisoryPredecessors { /* ordered, deduped, bounded to 4 */ }
impl AdvisoryPredecessors {
    pub fn push(&mut self, id: ObjectId, provenance: PredecessorProvenance) -> ContentResult<()>;
    pub fn explicit(id: ObjectId) -> ContentResult<Self>;          // one OriginalBase entry
    pub fn ids(&self) -> impl Iterator<Item = ObjectId> + '_;
    pub fn entries(&self) -> &[AdvisoryPredecessor];
}

// C1 — core/crates/layerfs-content/src/object/output.rs (unchanged)
impl FinalizedObject {
    pub fn with_predecessors(self, predecessors: AdvisoryPredecessors) -> Self;
    pub fn predecessors(&self) -> &AdvisoryPredecessors;
}

// C2 — unchanged: the save path already consumes the list
impl Store         { pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation>; }
impl SaveOperation { pub fn accept(&mut self, object: FinalizedObject) -> StorageResult<()>;
                     pub fn finish(self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome>; }
// cas/save.rs:100            let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
// delta/select.rs:342-354    fn acquisition(input, role, advisory, depth_cap) -> Option<ObjectId>
//                            // probes the advisory ids IN ORDER, returns the FIRST eligible one
```

**Is anything new needed at all? No.** Measured [C], and the shape is in the driver:

```rust
// core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:719-732  (harness; production LOC delta 0)
for id in consumer.insertion_order() {
    let mut object = consumer.cloned_object(*id).ok_or(...)?;
    if let Some(base) = bases.get(id) {                       // BTreeMap<new_root, previous_root>, :640-667
        object = object.with_predecessors(AdvisoryPredecessors::explicit(*base)?);
    }
    operation.accept(object)?;
}
```

Effect of that attachment alone [C]: apparent **128,864,256 → 63,737,856 B** (−65,126,400),
`delta.no_candidate` **26,847 → 10,878**, `prefix_selected` **18,344 → 34,300**.

Two honest qualifications:

* **The producer is not in core, and core cannot own it today.** C1's `construct_bytes` takes bytes only
  (`file/content.rs:207-219`) and cannot name a path; C2's contract states it "can save and read them
  without running file construction, a Workspace, a history entity or a mount"
  (`layerfs-storage/src/lib.rs:5-7`). The path → previous-content-root map is a **Workspace** fact. The
  reference keeps it in the COW tree (`crates/layerfs-workspace/src/cow_tree.rs:481-483,505-513`) and the
  LayerStack (`crates/layerfs-layerstack-store/src/objects.rs:3311 set_physical_predecessor`); `core/`
  has neither and Stage 7 has no specification. For the measured lane the driver holds the map, which
  §3 of the specification permits ("the product's own public C1/C2 API … no new product entrypoint").
  Whether a **driver-supplied** correspondence satisfies the roadmap claim ("the Store deduplicates it
  automatically") is an **owner ruling**, not a code question.
* **C1 already declares a cross-save base when it knows one.** `file/edit/apply.rs:120-122` attaches
  `AdvisoryPredecessors::explicit(view.root())` for an edited whole-file object — and `view.root()` is a
  stored object from an earlier save, so that route is already cross-save. `file/mapping/build.rs:119-136`
  (`push_chunk(raw, predecessor, consumer)`) does the same for a chunk the caller knows continues a
  retained payload. Both are *caller-fed*: `construct_bytes` passes no predecessor, and the chunked route
  is reached with none.

**The four slots are enough.** One `OriginalBase` entry per object; `push` dedups by identity
(`predecessor.rs:61-74`).

---

## 3. Three candidate designs, costed

| | design | files touched | est. production LOC | format change | owner amendment | verdict |
| --- | --- | --- | --: | --- | --- | --- |
| **a0** | **caller attaches the existing `AdvisoryPredecessors` (recommended)** | **none in core** | **0** | no | none (a ruling on the *claim*, not the code) | **the smallest change is no interface change** |
| a1 | new C1 entry point `construct_bytes_with_predecessors` | `content/file/content.rs`, `content/lib.rs` | ≈ 15–20 | no | none | a convenience, not the unlock |
| b | store-side key → last-object index C2 consults | `sqlite/schema.rs`, `sqlite/write.rs`, `sqlite/lookup.rs`, `cas/store.rs`, `cas/save.rs`, `cas/selection.rs`, new module | ≈ 150–300 | **yes** (schema 4 → 5, new table + index) | **required** (C2 contract + format) | not smallest |
| c | full Workspace / layer stack | new crate(s) under `core/crates/` | thousands, unbounded | n/a (new components) | **required**, plus a Stage 7 specification | out of scope |

**(a0) — caller-supplied predecessor per object, through the existing builder. Recommended.**
0 production lines. No constraint is touched: no new dependency, no file grows, no persisted byte
changes, no public item added. It is not a proposal — it is the measured configuration of §2 [C], and the
four-object defect of §1.3 is the only thing that makes that configuration illegal today.
*Cost:* the caller must maintain path → previous content root, i.e. the caller owns a Workspace-ish
correspondence. In the history lane that is the driver; in production it is Stage 7.

**(a1) — a new C1 entry point that takes the predecessor explicitly.**

```rust
// proposed; NOT implemented
pub fn construct_bytes_with_predecessors(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    bytes: &[u8],
    predecessors: &AdvisoryPredecessors,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile>;
```

Smallest honest implementation: thread the list into `construct_bytes_in` and attach it at the single
whole-file construction site (`content.rs:235`) — ≈ 15–20 lines; `content/lib.rs` (48 lines, ≤ 200) gains
one re-export. Constraints: `content.rs` 323 → ≈ 345 physical lines (≤ 999 ✓); no new dependency; no
format change; `core/AGENTS.md`'s architecture rule makes the affected `docs/architecture/` document part
of the same commit. **It unlocks nothing a0 does not already unlock** — it only spares a caller from
wrapping its own `FinalizedConsumer`. It covers the **whole-file** lane only: the chunked route needs a
*chunk-level* correspondence (the previous version's chunk objects), a different input shape
(`push_chunk` takes one `Option<ObjectId>` per chunk, not one list per file).

**(b) — a store-side key → last-object index C2 consults.** C2 holds no key: an object arrives as
identity + role + canonical bytes + references + predecessors. Two sub-variants:

* key inside the canonical bytes — a **C1 canonical-format change**: the path enters the bytes, so every
  `ObjectId` changes and every pinned canonical total, the O3 pin and the O4 tree gate break. Rejected.
* key out-of-band — `SaveOperation::accept_keyed(key, object)`, a persisted `object_keys(key, object_id)`
  table with a recency rule, a lookup on the selection path, and `SCHEMA_VERSION` 4 → 5 with the existing
  refusal for older Stores: ≈ 150–300 production LOC and a **format change**, and it duplicates the
  Workspace's job inside C2, which C2's contract denies in writing (`lib.rs:5-7`). **Owner amendment
  required.**
* a path-free variant (persist the per-save min-hash candidate index: an 8 × u64 signature → `object_id`
  table for admitted FULL whole-file objects) needs **no path knowledge and no C1 change**, but is still a
  new table and a schema bump — and B2's byte-exact measurement puts the cross-path rule at
  **35,937,886 B** against the same-path-previous rule's **45,926,982 B** on the 348,460,709 B whole-file
  canonical population [C]. Bigger change, worse correspondence than a0.

**(c) — a full Workspace/layer stack.** What the reference does, and the only design in which core owns
the correspondence. In `core/` it means two components that do not exist, against a Stage 7 with **no
specification**; `core/AGENTS.md` requires reading a component's approved roadmap before implementing it.
The cost is not bounded and cannot be called "smallest". Rejected here not because it is wrong
architecturally, but because it is the wrong *size* for this question and cannot be costed before Stage 7
is specified.

---

## 4. The defect that blocks all of them

**Where.** `core/crates/layerfs-storage/src/encoding/delta/select.rs`, `DepthCache::cost_of`, lines
94-144:

```text
101  let cost = loop {
102      if let Some(cost) = self.costs.get(&current).copied() {
103          break cost;                                            // (B) cache hit — the hit is NOT pushed
104      }
105      let Some(location) = lookup::location(connection, current, i64::MAX)? else { … };
108      path.push((current, location.canonical_length as u64));
116      match location.base_object_id {
117          Some(base) => current = base,
118          None => { break ChainCost { depth: 0, canonical: 0 } }   // (A) chain root — it IS in path
124      }
125  };
132  for (position, (id, own)) in path.iter().rev().enumerate() {
133      let depth = cost.depth.saturating_add(u8::try_from(position).unwrap_or(u8::MAX));
```

**Why it is wrong.** In exit (A) the chain root was pushed, so `position = 0` is genuinely at depth
`cost.depth + 0`. In exit (B) the cached entry is **not** in the path, so the path's last element sits
**one edge above** the cached entry and its true depth is `cost.depth + 1`. Every level of that walk is
recorded one edge short, and the shortfall is inherited by anything that later hits those records.
`eligible` (`:369` → `:373`) then admits a candidate whose true depth equals the cap, and `:307-313`
records the child at `cap + 1`.

**The minimal correct fix (described, NOT applied).** Distinguish the two exits, carry the missing edge
out of the loop, and add it in the walk-back; the self-cached query (empty path, exit (B) on the first
iteration) must keep returning the stored cost unchanged:

```rust
// lines 101-104: carry the offset with the cost
let (cost, offset) = loop {
    if let Some(cost) = self.costs.get(&current).copied() {
        break (cost, 1_u8);                     // the cached entry is one edge below path[last]
    }
    …
// lines 118-124: the chain root is in the path, so it contributes no offset
    None => break (ChainCost { depth: 0, canonical: 0 }, 0_u8),
    …
};
// lines 133-135: the walk-back adds the offset
let depth = cost.depth.saturating_add(offset)
    .saturating_add(u8::try_from(position).unwrap_or(u8::MAX));
```

**One-line or structural? Not structural — 4 changed lines in one function, in one file.** No signature,
no type, no trait, no bound, no persisted value, no caller change. The `canonical` accounting is already
correct (`cost.canonical` covers the cached entry and its dependencies; the walk-back adds the pushed
nodes' own lengths) and must not change. Physical-line effect: `select.rs` 392 → ≈ 394 (999 cap).
Estimated production LOC delta ≈ **+2** [E]; a commit must measure it with the repository counter.

**Not the fix, and why.** Widening the reader to `chain.len() > role_depth + 1` is a one-line change that
*compensates* rather than repairs: it silently raises the effective persisted policy to `cap + 1`, leaves
the writer admitting bases it must refuse, and does not bound the shortfall (a repeated cache-hit chain
can under-report by more than one edge, so the compensating reader aborts later anyway). The boundary is
already consistent (§1.4); moving it would break the property the product's own tests pin.

---

## 5. What it would buy — arithmetically

Quoted from the campaign's decomposition [C]; E4's instrument reproduces its object counts exactly (§1.2).

```text
apparent      128,864,256  ->  63,737,856   = -65,126,400 B   (-50.53 %)
gap vs v0.1.6 128,864,256  -   49,315,840   =  79,548,416 B
cause 1  no cross-commit base declared       65,126,400 B   81.87 % of the gap
cause 2  remaining pack-blob gap (coverage)   7,403,406 B    9.31 %
cause 3  non-pack SQLite overhead             7,018,610 B    8.82 %
         sum                                  79,548,416 B   residual 0
delta.no_candidate  26,847 -> 10,878   (-15,969)
prefix_selected     18,344 -> 34,300   (+15,956, +87.0 %)
```

**The declaration buys cause 1 (65,126,400 B). The depth fix buys legality, not bytes.**

* Without the fix that configuration writes **4 objects at 9 edges** the reader refuses (§1.3), so it
  cannot be read back in full. With the fix those four bases become ineligible and the four children store
  FULL instead. Estimated cost, using `l7`'s own measured bucket ratios (with-base
  `271,589,818 / 16,082,951 = 16.887×`; without-base `76,870,891 / 28,427,582 = 2.704×`):
  `(2661+1992+1315+1269) = 7,237` canonical B × `(1/2.704 − 1/16.887) = 0.31059` = **≈ 2,248 B** [E],
  i.e. **+0.0035 %** on 63,737,856 B. The fix is byte-neutral to within 2.2 KB on this lane.
* **It does not unlock more bytes under the frozen policy.** Every base at `depth ≥ 8` is refused by
  `eligible`, so the "unlimited declaration" arm cannot beat the depth-7 arm while
  `whole_file_delta_max_depth = 8`. The only route to cause 2's `4,315,905 B` sub-figure [C] is a
  **policy depth above 8** — a configuration value in the existing schema (0 … 50, already persisted), not
  a code change — paid for by the chain-work budgets (`chain_canonical_limit` 512 KiB,
  `chain_encoded_limit` 256 KiB) and by read amplification (a depth-`d` chain costs `d` record reads).
* **Not bought by this fix:** the chunk/Native lane (1,098 objects / 22,055,499 canonical / 5,695,678
  stored, **zero** deltas, against v0.1.6's 650 chunk PREFIX records worth 1,737,621 B [C]). Its interface
  exists (`push_chunk(raw, predecessor, consumer)`) but its producer — the previous version's chunk map —
  does not; that is separate C1 work. Cause 3 (197.5 vs 63.0 B/object, 7,018,610 B) is a
  schema/placement question, untouched.

---

## 6. What it would cost

* **API surface: zero new public items.** The change is inside a private method's arithmetic — no new
  type, trait, function, module, column, table, index or environment variable.
* **Ceilings:** `encoding/delta/select.rs` 392 → ≈ 394 physical lines (999 cap). No `lib.rs`/`mod.rs` is
  touched (`content/lib.rs` 48, `storage/lib.rs` 39, `delta/mod.rs` 8 — 200 cap). Nothing is near a
  ceiling.
* **Persisted format: NO CHANGE.** No column, no table, no index, `SCHEMA_VERSION` stays **4**, the record
  grammar `tag(1) [+ base_object_id(32)] + zstd_frame` is untouched, and `objects.base_object_id` keeps
  its meaning. The fix changes **which bases are admitted**, never how a record is encoded. A Store
  written by the fixed writer is readable by the unfixed reader and vice versa: no compatibility gate, no
  migration, no re-baselining, and every pinned canonical total, O3 pin and O4 tree gate is unaffected.
  If a design in §3(b) or §3(c) is chosen instead, that sentence stops being true — which is the strongest
  single argument for (a0) plus the fix.
* **Residual:** already-written Stores containing over-deep objects stay unreadable at those objects
  (§1.6). The fix is writer-side only.

---

## 7. The falsifier

**F1 — for the mechanism and the fix (an external test I deliberately did not write).** One Store, cap
**2**, a chain built across saves: `Q → B1 → root` (Q at true depth 2, B1 at 1). Then, **inside a single
`begin_save`/`finish`**, accept `W` with `AdvisoryPredecessors::explicit(B1)` and then `Z` with
`explicit(Q)`. Order matters, and both objects must be in **one** save because the `DepthCache` dies with
the save.
*Predicted with the defect:* `delta.trials == 2`, `Z` stored as PREFIX at true depth 3, and reading `Z`
fails with `Integrity("dependency chain depth")`.
*Predicted with the fix:* `W` PREFIX at depth 2; `Z` FULL (`ineligible_candidates == 1`,
`trials == 1`); every object reads back.
**If the unfixed product does not reproduce that abort, my mechanism is wrong.**

**F2 — for the fix, on the real lane.** Re-run an `l7`-class arm and then `e4_chain_depth.py`: **if any
object's base still sits at true depth ≥ the persisted cap, the fix is incomplete** (or the mechanism is
wrong). The pre-fix reading is 4.

**F3 — for the recommended design (a0).** If a caller cannot attach the correspondence to the object
`accept` receives, or attaching it does not move the bytes, a0 is wrong. Both were measured: the builder
is public (`output.rs:118`) and the attachment alone moved 65,126,400 B [C]. A subtler falsifier: if a
future Workspace proves it must own the map *inside* C2 rather than hand it over, then a0's zero-LOC
claim is an artifact of the driver being the caller, and the real cost is design (c).

**F4 — for §1.5.** If running the §7 gate over `/tmp/s0_l7` completes with those four objects sampled,
then either the reader accepts 9-edge chains (contradicting §1.4's measurement) or the store walk is
wrong. Unrunnable today: `ops/history.rs:426-428` refuses `Phase::Verify`.

---

## 8. Reproduction

```sh
# 1. depth histogram + the over-deep objects (read-only, both Stores)
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-e/squad-e/e4_chain_depth.py /tmp/s0_l7/sample.sqlite /tmp/base187/sample.sqlite

# 2. the persisted policy each Store was created with
sqlite3 "file:/tmp/s0_l7/sample.sqlite?mode=ro" "select id,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth from store_policy;"

# 3. the boundary semantics the product itself pins
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test delta_chains

# 4. the gates the l7 arm actually recorded (and did not)
grep '"kind":"gate"' /tmp/s0_l7/trace.jsonl
grep -c '"key":"g1.o2' /tmp/s0_l7/trace.jsonl /tmp/base187/trace.jsonl   # 0 and 0
```

Store files used: `/tmp/s0_l7/sample.sqlite` (63,737,856 B, 52,032 objects) and
`/tmp/base187/sample.sqlite` (128,864,256 B, 52,032 objects). If either is missing, this document's
measurements cannot be reproduced; nothing here is inferred from their absence.
