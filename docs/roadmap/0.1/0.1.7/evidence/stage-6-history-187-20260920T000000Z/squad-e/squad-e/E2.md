# E2 — is a branch/commit/history model REQUIRED for the storage wins?

> **Status: diagnostic. Not admission evidence.** Every number below is a **byte or object
> count**, which is load-independent. **No timing figure appears anywhere in this document and
> none was taken** — the machine was shared with sibling squads, so a wall-clock reading would be
> invalid by the campaign's own rule.
>
> **Guardrails honoured.** Nothing under `core/crates/` or `crates/` was modified. No commit was
> made. The 217-row lane was not touched and `history-stride1` was not run. No benchmark lane was
> executed by this squad: every number is read-only analysis over the corpus, the two retained
> Stores and the product's own source. Every claim is labelled **[S]** source read at
> `66bce8378` (file:line), **[M]** measured here, **[C]** cited from a sibling squad's recorded
> measurement, or **[H]** hypothesis.

---

## 0. The one-paragraph answer

**The win needs a key, not a branch.** Measured here: a purely **content-keyed** index — no path,
no branch, no commit, no revision, no history entity of any kind — reaches an eligible delta base
for **37,200 of the 44,148 whole-file objects (84.3 %)**, **35,187** of them across a save
boundary, and it **strictly dominates** a path-keyed map, which reaches **29,056 (65.8 %)**. The
product **already contains that index**: `candidates.rs` builds an eight-hash content signature and
finds candidates by it. What it lacks is **lifetime** (it is dropped at the end of every save,
`cas/lifecycle.rs:90`, `cas/store.rs:392`) and **capacity** (a 1024-entry ring plus an 8192-entry
reference table). Of the **+18,767 objects** a persistent index buys over today's behaviour,
**88.3 % come from capacity and 11.7 % from lifetime** — so "make the cache persistent" is not the
fix; "make the index big enough to be worth persisting" is.

---

## 1. Every storage win, classified by what it needs as input

The question is not "how much does each win save" (Squads A–D measured that) but **"what does each
win need to be told, that it cannot derive from the new object's own bytes?"**

| # | win | what determines it, in the product's own code | function of the new object's content **alone**? |
| --: | --- | --- | --- |
| 1 | **exact dedup** | `ObjectId::for_bytes(canonical)` = `blake3("layerfs/object/v2\0" ‖ canonical)` (`object/id.rs:16,25`); membership is a lookup of that id, then `membership::reuse_or_collide` compares the **stored canonical bytes** with the offered ones and reuses the row only on exact equality (`cas/membership.rs:32-46`) | **YES — content alone.** The key *is* the content. No external correspondence of any kind. |
| 2 | **chunking (CDC)** | `policy.representation(logical_len)` chooses Empty / WholeFile / Chunked from the **length** (`content/policy.rs:122-126`); the partition is `FastCdc::new().scan(source, cb)` — the frozen two-byte rolling GEAR with a frozen table, frozen masks and a frozen seed (`file/cdc/gear.rs:1-30,64-104`) | **YES — the byte stream alone.** The scanner's only inputs are the bytes and its own frozen constants. |
| 3 | **same-save delta** | the per-save `Candidates` cache, keyed by `signature(raw)` = the eight smallest distinct `mix()`ed 16-byte rolling-257 hashes of the raw payload (`encoding/delta/candidates.rs:58-84`), consulted at `select.rs:257` when the advisory list is empty | **YES — content alone.** It is a *content* index, not a path or history index. It is bounded by **lifetime** (one save) and **capacity** (SLOTS = 1024, REFERENCES = 8192). |
| 4 | **cross-save / cross-commit delta** | the **advisory list**: `let advisory: Vec<ObjectId> = object.predecessors().ids().collect();` (`cas/save.rs:100`) → `acquisition()` probes the ids **in order** and returns the **first** eligible one (`select.rs:342-354`); for `ObjectRole::Chunk` only `advisory.first()` is ever read (`select.rs:246-252`) | **NO — not from the new object's content alone.** But see §1.1: the missing input is a **correspondence**, and a correspondence can itself be *derived from content*. It does not have to come from a history entity. |

### 1.1 The crux, argued precisely

> *A content-addressed store can find IDENTICAL objects by hashing. Can it find SIMILAR ones?*

**Yes — and the product already does, within one save.** `candidates.rs` is a similarity index over
content: it stores an eight-value min-hash signature per admitted object and accepts a candidate
that shares **≥ 2 of 8** hashes with the target, highest overlap winning (`candidates.rs:136-164`).
Nothing in that structure mentions a path, a save, a commit or a branch. The only two things that
stop it from serving a *cross-save* delta are:

* **lifetime** — the cache is constructed per save (`cas/lifecycle.rs:90`) and the save owner is
  dropped when the save finishes (`cas/store.rs:392`), so after `finish` there is nothing left to
  consult; and
* **capacity** — `SLOTS = 1024` entries and `REFERENCES = 8192` reference slots
  (`candidates.rs:14-16`), i.e. 128 KiB of live index (`INDEX_BYTES`), which is smaller than one
  state of this lane.

The advisory route is a *different* mechanism with a *different* input: it is the caller saying "I
already know which stored object covers these bytes". That input is genuinely external. But the
external input it needs is **one id per object**, and §2 shows the harness supplied it from a
`BTreeMap<Vec<u8>, ObjectId>` — one key per path — inside a benchmark driver, with no branch, no
commit, no revision and no history entity anywhere in the process.

**So the crux resolves into two separable statements, and both are measured below:**

* **the advisory route needs a correspondence, and the cheapest sufficient correspondence is a
  per-path key** (§2, §4a/§4c); and
* **the same-save content index, given lifetime and capacity, needs no correspondence at all** and
  covers *more* objects than the per-path key does (§4b, §4 table).

### 1.2 The chunk lane, measured: CDC turns "similar" into "identical"

**[M]** I ported `file/cdc/gear.rs` line for line (`squad-e/squad-e/e2_chunk_cdc.py`, two-byte
rolling GEAR, frozen profile) and ran it over the lane's **91 chunked file versions / 24,492,001 B**.
The port is **validated to the byte** against the Store:

    emitted chunk emissions                          1,233
    distinct chunk contents                          1,098
    Store Chunk rows (object_role = 2)               1,098
    residual                                              0

    duplicate emissions                                135
      same path (an earlier version of the same file)  129
      cross path (a different file entirely)             6

    Q1 purity: the same 169,733 B payload partitions identically when fed in
               32,768 / 4,096 / 65,536-byte blocks (8 chunks each) — [M]

Two facts follow, and they are the whole of the chunk answer:

* **The 135 duplicate emissions cost the Store nothing today, with zero correspondence.** A chunk
  whose bytes repeat has the *same* `ObjectId`, so exact dedup finds it by hashing — including the
  **6 that match a different file's chunk**. 135 of 1,233 emissions (10.9 %) are already free.
* **The remaining 1,098 distinct chunks get no delta at all**, and the only route to one is
  `advisory.first()` (`select.rs:246-252`), which on this lane is always empty because
  `mapping::build::build_streaming` calls `builder.push_chunk(chunk, None, consumer)`
  (`file/mapping/build.rs:325`) — the predecessor slot is hard-coded `None` on the construction
  path. The reference tree's edit path does supply one (`edit/apply.rs:309-325`, the payload
  immediately left of the edit position in the *same file's previous version*).

**Ceiling on that opportunity, [C]:** v0.1.6 realised **650 chunk PREFIX records worth 1,737,621 B**
(Squad A1). Against a 79,548,416 B gap that is **2.2 %**. A perfect chunk correspondence would not
change this report's verdict.

---

## 2. The empirical proof: no branch / commit / revision concept was needed

### 2.1 What the Step 0 driver change actually did

**[S]** `core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs` (uncommitted working-tree
change at `66bce8378`; production LOC delta **0**, `core/benchmark/` is not product source).

Four edits, and nothing else:

1. **One declaration, outside the per-state loop** (`history.rs:526-536`):

       let faithful = faithful_history_model();
       let depth_limit = advisory_depth_limit();
       let mut previous_content: BTreeMap<Vec<u8>, ObjectId> = BTreeMap::new();
       let mut chain_depth: BTreeMap<Vec<u8>, u8> = BTreeMap::new();

2. **Per state, one lookup per changed path** (`history.rs:642-666`): for each `Change::Added` or
   `Change::Modified` path, take the content root the driver just constructed for that path and the
   one it remembers for the same path, skip if they are equal, and record
   `bases.insert(new_root, old_root)`.

3. **One attachment per object** (`history.rs:720-725`):

       if let Some(base) = bases.get(id) {
           object = object.with_predecessors(
               AdvisoryPredecessors::explicit(*base)?)?;
       }

4. **One write-back at the end of the save** (`history.rs:735-739`):

       for (path, root) in &constructed { previous_content.insert(path.clone(), *root); }

The switch that enables it is read from the environment (`LAYERFS_HISTORY_ADVISORY`) so **one
binary measures both arms** and the only difference between them is the declaration.

### 2.2 What it did **not** have — the complete list

There is **no** branch, **no** commit, **no** revision, **no** history entity, **no** LayerStack,
**no** parent pointer, **no** named or addressable past state, **no** merge or ancestry query, and
**no** store-side query of any kind. The driver never asks C2 "what did the previous save write for
this path?" — C2 has no such query and never gains one. There is exactly **one data structure**:
`BTreeMap<Vec<u8>, ObjectId>`, local to one function, dropped when the process exits.

### 2.3 The minimum information it needed: **one key per path**

| what it needed | why | where it came from |
| --- | --- | --- |
| the path (the key) | to know which previous version corresponds to which new one | the corpus transition the driver was already iterating |
| the previous content root (the value) | to name the object C2 should try as a base | the driver's own `constructed` map from the **previous iteration** — C1 had just produced it |
| the new content root (the lookup key) | because the save offers objects by id | `constructed.get(&changed.path)`, in hand |

Nothing else. The value written back is the root C1 emitted, not a Store read, not a snapshot id,
not a commit id. The whole mechanism is: **remember the last root you built for each path, and hand
it back as the predecessor next time.**

### 2.4 What it bought — [C], Step 0

| | `off` (the registered lane) | `l7` (faithful, depth ≤ 7) | delta |
| --- | --: | --: | --: |
| advisory bases declared | 0 | 28,491 | +28,491 |
| `delta.prefix_selected` | 18,344 | 34,300 | +15,956 (+87.0 %) |
| `delta.no_candidate` | **26,847** | 10,878 | −15,969 |
| Store apparent bytes | **128,864,256** | **63,737,856** | **−65,126,400 (−50.5 %)** |

with `absent_candidates = 0`, `ineligible_candidates = 0`, `work_exceeded = 0` in the baseline:
the Store was **supplied no base**, it did not decline one. The whole-file lane moves from
110,941,054 B to 44,510,533 B under the same declaration.

### 2.5 The negative result that must travel with it

**[C]** The faithful model is **rejected by the product** with
`Integrity("dependency chain depth")` at `delta/read.rs:159-168`. The writer admits depth <
`depth_cap` and the reader refuses depth > `depth_cap`, both 8, so the writer produces chains one
edge deeper than its own policy permits. Depth limits 8 / 10 / 12 / 16 all abort; 7 completes.
**This is an unpatched product defect and it is not E2's to fix**; it is recorded here because the
44,510,533 B figure is a *depth-7* figure, and because a production correspondence would hit it.

---

## 3. What v0.1.6's branch structure actually bought, separated from the correspondence

**[S]** the reference tree. Three facts, in the order the data flows.

**3.1 The graph told the runtime which inode table to read.** `cow_tree.rs:481-513` looks up the
inode record in the **base layer's** inode table and turns it into
`Data::File(FileData::Base { root: FileContentRoot(record.content_root), len })`. The branch/commit
graph is what makes "the base layer" a meaningful phrase.

**3.2 The correspondence is a per-path slot, and it is one hop.** `changes.rs:1509-1538` builds a
per-file task record; the base is taken from `prior[slot]` — the inode's own slot in the base
layer's page — with a fallback that searches the removed set by **basename**. `changes.rs:1582-1586`
reads that slot back, and `changes.rs:1836-1842` calls
`objects.set_physical_predecessor(self.reader.clone(), predecessor, ...)`. Exactly **one**
predecessor per file: `predecessor.map(|r| r.0).or(before.map(|r| r.content_root))`.

**3.3 The store then converted that one root into per-object base ids.** `objects.rs:2812-2827`
attaches a `PredecessorCursor::new(FileStateRoot(root.0))`, and `objects.rs:2841-2855` calls
`cursor.hints(...)` per delivered object, which walks the **previous version's rope**
(`rope/read.rs:395-431`) and returns up to **4** `prior_ids` for the object's span. Those become
the candidates the encoder tries (`objects/admission.rs:1720-1760`).

**Verdict.** Split the win into two parts and put a number on each:

| part | what it is | worth |
| --- | --- | --- |
| **the graph** | the identity of the parent state — i.e. *which* root to read the previous inode records from. One root, one hop, no ancestry walk, no merge-base search. | The graph's **extra depth** over "the immediately previous version" is bounded by B2's measured same-path window result: widening from 1 slot to 4 slots (or to *all* earlier versions) moves the whole-file lane from 45,926,982 B to 45,230,840 B — **[C] 696,142 B, 1.5 % of the lane, 0.88 % of the 79,548,416 B gap.** No branch- or merge-crossing measurement exists. |
| **the correspondence** | "this path's previous inode record", materialised as one `content_root` per path | **[C]** Step 0 measured it directly: 65,126,400 B of the gap, **81.87 %**, from a driver with no graph at all. |

**And the graph is not the only way to obtain the correspondence.** v0.1.6 obtained it by reading the
base layer's inode table through `SnapshotReader`. Step 0 obtained the same value by keeping a
`BTreeMap` in the driver. §4b obtains it by hashing content and never learning a path. The bytes
are identical in all three cases because the bytes come from the *correspondence*, not from the
graph.

**Independent corroboration, [C]:** v0.1.6's run receipts record `base_fetches = 80,361`
predecessor base hops, 100 % after state 1, and `delta_selected = 72,934` (Squad A3). A per-path,
per-version hop — not a graph traversal.

---

## 4. The candidate minimal correspondences, cheapest first — and their measured coverage

### 4.1 The instrument, and its calibration

**[M]** `squad-e/squad-e/e2_correspondence.py` reproduces the lane's population **to the byte**
against two independent sibling pins:

    union oids 44,240  bytes 371,937,306     [B2/B3 pin 44,240 / 371,937,306]   residual 0
    whole-file objects 44,148                [B2 pin 44,148]                    residual 0
    whole-file content 347,445,305 B         [B2 pin 347,445,305]               residual 0
    whole-file canonical 348,460,709 B       [B2 pin 348,460,709]               residual 0

**[M]** `squad-e/squad-e/e2_content_index.py` reimplements `candidates.rs` exactly — the 16-byte
window, the 257 rolling hash, `mix()`, the eight smallest distinct values, `EMPTY` padding, the
`hash & (REFERENCES-1)` reference table, the 1024-slot ring, the ≥ 2-of-8 acceptance and the
`FULL`-only admission rule. Its signature function is checked against a **literal transcription
of the Rust loop** on real corpus blobs:

    python3 e2_content_index.py --selfcheck                  -> PASS
    vectorised == literal on 131,005 / 131,006 / 131,079 B corpus blobs -> True

and the whole model is calibrated against the measured baseline:

| | objects with a base | canonical bytes covered |
| --- | --: | --: |
| **P0 model** (per-save, ring 1024, refs 8192, FULL-only) | **18,433** | **84,902,793** |
| **measured baseline** (`/tmp/base187/sample.sqlite`, [C] B2) | **18,344** | **82,033,173** |
| residual | **+89 (+0.485 %)** | **+2,869,620 (+3.498 %)** |

The residual is the one thing the model does not simulate: the single prefix trial
(`full_wins` 11 / `full_losses` 28 objects, [C] B2) and the chain-budget refusals (10–14 objects),
both of which need the real codec. **Every model row below is therefore an upper bound**, and the
bound is measured at under 3.5 % of canonical bytes.

### 4.2 The measured coverage table

Population: **44,148 whole-file objects / 348,460,709 B canonical**. Coverage is counted per
**object**; canonical bytes are `content + 23` (the frozen canonical header, [C] B3, residual 0).

| # | correspondence | key | objects reaching a base | % of 44,148 | canonical B covered | cross-save objects |
| --: | --- | --- | --: | --: | --: | --: |
| **a** | caller-held per-path map, as the harness built it — **immediate previous selected state** | `path` | **29,056** | 65.8 % | 263,452,684 | 29,056 |
| **c** | per-path version chain in C1 — **any earlier version**, nearest first | `path` | **29,087** | 65.9 % | 264,029,990 | 29,087 |
| **b1** | store-side "most recent object stored under key K", **K = path** | `path` | **29,056** | 65.8 % | 263,452,684 | 29,056 |
| **b2** | store-side index, **K = the product's own 8-hash content signature** | *content* | **37,200** | **84.3 %** | **303,595,980** | **35,187** |
| — | today's per-save cache (reference row, P0) | *content, one save* | 18,433 | 41.8 % | 84,902,793 | **0** |

**b1 is identical to a by construction** and is not a second measurement: a store that remembers
"the most recent object stored under path K" knows exactly what a caller that remembers the same
thing knows. Its only advantage is that the caller cannot forget it.

### 4.3 The four sub-experiments behind b2 — lifetime vs capacity

**[M]** `python3 e2_content_index.py`. All rows are FULL-only admission; "exact index" means no
8192-entry reference truncation.

| model | persistent? | ring | reference table | objects | canonical B | cross-save |
| --- | --- | --: | --- | --: | --: | --: |
| **P0** | no | 1024 | 8192 | 18,433 | 84,902,793 | 0 |
| **P1** | no | 1024 | exact | 19,023 | 91,607,869 | 0 |
| **P2** | no | unbounded | exact | 19,284 | 94,373,272 | 0 |
| **S1** | **yes** | 1024 | 8192 | 20,632 | 110,466,408 | 6,418 |
| **S2** | **yes** | 1024 | exact | 21,475 | 119,429,418 | 7,254 |
| **S3** | **yes** | unbounded | exact | **37,200** | **303,595,980** | **35,187** |

The arithmetic that decides the recommendation:

    S3 - P0   37,200 - 18,433 = +18,767 objects   (303,595,980 - 84,902,793 = +218,693,187 B)
    S1 - P0   20,632 - 18,433 =  +2,199 objects   = 11.72 % of the gain   <- lifetime alone
    S3 - S1   37,200 - 20,632 = +16,568 objects   = 88.28 % of the gain   <- capacity

**Negative result, and it is the important one: merely persisting the product's existing cache
(S1) buys +2,199 objects (+11.9 %) and only 6,418 cross-save matches.** The cache's 1024-slot ring
and 8192-entry reference table are what is binding, not its lifetime. "Make the cache persist"
is not the fix.

**b2 strictly dominates the path key.** `37,200 - 29,056 = +8,144 objects (+28.03 %)`;
`303,595,980 - 263,452,684 = +40,143,296 B`. And of b2's 37,200 covered objects, **24,232
(65.14 %) matched at a *different path*** — a path key cannot see two thirds of what content
keying sees. This is the same effect B2 measured from the other direction: *"33,210 of 43,887
non-first-state objects (75.7 %) have an already-stored near-identical object at a DIFFERENT
path."*

**Structural negative.** In every per-save row the same-path hit count is **0**. Within one save a
path is constructed once, so the per-save cache is *structurally incapable* of a same-path match —
the only cross-save route in the product is the advisory list. This is measured, not inferred.

### 4.4 The chunk lane's correspondence

**[M]** of the **91** chunked file versions (24,492,001 B):

    80 of 91 have a previous same-path version at all
    63 of 91 have a previous same-path version that is ALSO chunked (69.2 %)

So a per-path correspondence is *available* for 69 % of chunked versions — and is worth at most
**1,737,621 B, 2.2 % of the gap** ([C] A1). The 135 exact-duplicate chunk emissions (§1.2) are
already free today.

### 4.5 (d) a full branch/commit graph — NOT MEASURED here

No measurement of branch- or merge-crossing reuse exists in this campaign, by this squad or any
other. The only number that bears on the graph's *extra* value over a one-hop correspondence is
B2's same-path window result: **[C] 696,142 B (1.5 % of the whole-file lane, 0.88 % of the gap)**,
and that was measured by widening a *path* window, not by crossing a branch. **This is the honest
limit of the evidence: the graph's marginal value above a key is unmeasured, and the measured
one-hop value is 81.87 % of the gap.**

---

## 5. What breaks without a history concept — and what #187 actually speaks to

Being honest about scope: **#187 measured the delta-base axis and nothing else.** Four candidate
costs, each separated into measured and unmeasured.

### 5.1 Naming and reaching a past version — **real cost, UNMEASURED by #187**

C2 is content-addressed and its only read key is an `ObjectId` (`StoreProvider` /
`SaveOperation::read`). There is no enumeration, no name, no "state k" and no parent pointer
anywhere in core: **[S]** after excluding the extent-tree `Branch` node kind and SQL `COMMIT`,
every remaining occurrence of branch/commit/revision/layerstack/snapshot in
`core/crates/layerfs-storage/src` and `core/crates/layerfs-content/src` is either a
`COMMIT`/`ROLLBACK` transaction word, a within-save *membership snapshot*
(`cas/save.rs:26-27`, `cas/placement.rs:165` — a set of rows, not a state), or the
decompression/compression workspace buffer.

**Consequence:** an unnamed version is reachable **iff the caller kept its id**. If the caller did
not, the object is unreachable *and still occupies bytes* — there is no route back to it. This is a
genuine cost of not having a history concept. It is a **product capability** cost, not a storage
cost: the bytes on disk are identical with or without the concept, so it does not touch this
report's verdict. **#187 does not measure it** — the Step 0 driver keeps the ids in a local map and
the corpus oracle is the only thing that names a state.

### 5.2 GC / reclamation of superseded objects — **no cost, because neither generation has it**

**[S]** `core/crates/layerfs-storage/src/sqlite/cleanup.rs` is documented and implemented as
*"bounded cleanup of a definitely failed, unpublished save"* — it removes every object and pack a
**failed** save created, walking owned locators newest-first from a baseline pack id. It does not
and cannot reclaim a *superseded* object: it has no notion of supersession, and nothing in core
does.

**[S]** the reference tree does not either: a recursive grep for
`fn (collect|reclaim|prune|sweep|vacuum)|reclaimable|unreachable|garbage` across
`crates/layerfs-layerstack-store/src/` returns **6 hits, all `unreachable!()` macros plus one
test helper** (`statements.rs:369 collect_sql`). There is no reachability-based collector in
v0.1.6.

**Consequence:** retaining 17 states costs exactly what retaining them costs in *both* generations.
A history concept would be a *precondition* for deciding what is superseded — but it is not a
substitute for a collector, and no collector exists. **#187 does not measure this**, and the
negative here is a bounded grep, not a proof.

### 5.3 Dedup ACROSS branches — **no cost; dedup is already global and unscoped**

**[S]** exact reuse is one `objects` table keyed by `ObjectId::for_bytes(canonical)`. There is no
branch column, no branch scoping and no per-branch namespace in the schema
(`objects(object_id, object_role, canonical_length, base_object_id, pack_id, group_number,
record_number)`). **[M]** this is not theoretical: of the lane's 1,233 chunk emissions, **6 are
byte-identical to a chunk of a completely different file**, and the Store holds one row for each.
Across the whole-file lane, **1,322 of 45,470 occurrences** re-construct content the Store already
holds. A branch concept could only *reduce* this reuse by scoping it.

### 5.4 Is an unnamed version reachable at all? — **only by id; measured relevance: none**

Reachable by id, unreachable by anything else. §5.1 states the cost. For the storage question it is
irrelevant: the bytes are the same either way.

### 5.5 The one thing the history concept *would* change, stated plainly

A history entity is a **convenient place to keep the key** — it can answer "what was this path's
content root in state k-1?" without the caller carrying a map. That is a **bookkeeping** benefit,
not a storage benefit, and §4.2 shows it is not even the *best* bookkeeping: content keying covers
84.3 % where the path key covers 65.8 %. **[H]** Whether an owner prefers the path key because it
is cheaper to reason about, or the content index because it covers more, is a design ruling, not a
measurement.

---

## 6. Verdict

> **The storage win does not need branches, commits, revisions or history. It needs a key.**
>
> Measured: **one key per path**, carried across saves by whatever layer already constructs the
> content, takes the Store from 128,864,256 to 63,737,856 B apparent — **81.87 % of the whole gap** —
> with no history entity in the process (§2, [C] Step 0). Measured here: a **content key alone**,
> with no path and no caller bookkeeping at all, reaches an eligible base for **37,200 of 44,148
> whole-file objects (84.3 %)**, **35,187** of them across a save boundary, and strictly dominates
> the path key by **+8,144 objects (+28.0 %)** (§4.2/§4.3). The product **already owns that index**
> (`candidates.rs`); it needs **lifetime and capacity**, and capacity is the binding one —
> **88.28 % of the gain** comes from capacity, **11.72 %** from lifetime (§4.3).
>
> A branch/commit graph is **one way to obtain the path key** — v0.1.6 used it for exactly one hop
> and one root per file (§3) — and its *marginal* value over that one hop is **unmeasured** and
> bounded above by **696,142 B (0.88 % of the gap)** on the only evidence that exists (§4.5).

---

## 7. Negative results and corrections, in one place

1. **Persisting the product's existing cache is nearly worthless on its own.** S1 (ring 1024 +
   refs 8192, made persistent) covers 20,632 objects vs P0's 18,433: **+2,199 objects, +11.9 %**.
   Capacity, not lifetime, is the binding constraint. **[M]**
2. **The per-save cache can never make a same-path match.** Same-path hits are **0** in every
   per-save row, structurally: one construction per path per save. **[M]**
3. **Widening the same-path window is not worth it.** (a) immediate previous → (c) any earlier
   version: **+31 objects, +577,306 B canonical**. B2's simulated version of the same widening:
   **+696,142 B stored, 1.5 %**. **[M]/[C]**
4. **Cross-*file* exact chunk reuse is negligible**: **6 of 1,233** emissions (0.5 %). The 129
   same-path duplicates (10.5 %) are the whole of the free chunk reuse. **[M]**
5. **The chunk lane's entire delta opportunity is ~2.2 % of the gap** (1,737,621 B, [C] A1) even
   though a same-path predecessor exists for 69.2 % of chunked versions. **[M]/[C]**
6. **Correction to B2's label, arithmetic not prose.** B2 wrote *"44,240 union contents = 44,148
   whole-file + 92 chunked file roots."* Measured here: **44,148 + 91 Chunked + 1 Empty = 44,240**
   (the empty file `e3b0c442…` appears in 6 states and stores no object, [C] B2 itself). The 92 is
   91 Chunked + 1 Empty. **[M]**
7. **My models are upper bounds.** P0 over-states the measured baseline by +89 objects (+0.485 %)
   and +2,869,620 canonical bytes (+3.498 %). The unmodelled parts are the prefix trial's
   `full_wins`/`full_losses` (11/28 objects) and the chain-budget refusals (10–14 objects).
   **[M]**
8. **The registered `history-stride10` number is not a product measurement.** It measures a driver
   that declares no cross-commit base at all ([C] Step 0). It is cited here, not re-derived, and not
   used as a baseline.
9. **The faithful arm is rejected by the product** at depth ≥ 8
   (`Integrity("dependency chain depth")`, `delta/read.rs:159-168`). Unpatched product defect,
   reported by Step 0, **not patched here**, and a production correspondence will hit it. **[C]**
10. **No branch- or merge-crossing measurement exists.** §4.5 states this as the limit of the
    evidence rather than guessing.
11. **No timing.** None taken, none reported, by the campaign's own rule.

---

## 8. Exact commands

Everything is reproducible from a clean checkout of `66bce8378` with the corpus present. **No lane
is run; no product source is touched.**

    C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
    E=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-e/squad-e
    cd $E

    # population + the path-keyed census (a) and (c)        -> e2_correspondence.json
    python3 e2_correspondence.py

    # the signature port, checked against a literal transcription of the Rust loop
    python3 e2_content_index.py --selfcheck

    # the content-keyed index models P0..S3                  -> e2_content_index.json
    python3 e2_content_index.py

    # the CDC port, validated against the Store's own Chunk rows -> e2_chunk_cdc.json
    python3 e2_chunk_cdc.py

Retained artifacts read **read-only**: `/tmp/base187/sample.sqlite` (the unmodified lane, 52,032
objects, apparent 128,864,256 B) and `/tmp/s0_l7/sample.sqlite` (the faithful model, apparent
63,737,856 B). Both were present at the start of this work. Python 3.14.3, `numpy 2.4.3`,
stdlib `sqlite3`.

Source reads, by file:line, are cited inline in §1–§5.

---

## 9. What was NOT run, and why

| not run | why |
| --- | --- |
| any benchmark lane, including `history-stride3` and `history-stride1` | E2 is an architecture question answered from the corpus, the two retained Stores and source. `history-stride1` is never run by guardrail. |
| a branch/merge-crossing reuse measurement | no corpus state pair in this selection crosses a branch; the corpus is one linear history. Would need a different corpus. |
| the depth-cap sweep | needs the real codec; B2 also left it NOT RUN. |
| a full codec simulation of S3 | S3 is a **candidate-availability census**, not a stored-bytes simulation: it counts objects that *would reach a base*, not bytes saved. B2's 35,937,886 B for R4_crossfirst is the stored-bytes number for a related rule and is the right companion figure. |
| stride3 confirmation | not run, so **no stride3 claim is made**. |
| GC / reachability behaviour under a real supersession workload | §5.2 is a bounded grep over two source trees, not a workload. |
| `cargo fmt --check` / the core test suite | nothing under `core/crates/` or `crates/` was modified; there is nothing new to verify there. |

**Production LOC: 84936 -> 84936 (delta 0).** `core/benchmark/` is its own Cargo workspace and is
not product source; no file under `core/crates/` or `crates/` was opened for writing.
