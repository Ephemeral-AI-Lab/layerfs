# L4 — the C1 chunk cursor

**Diagnostic.** Source pin `66bce8378` + the working tree. **No timing figure appears
here**: the machine is shared and a wall-clock reading would be invalid. Access cost
is stated in **bytes per read**. Nothing under `crates/` (the reference tree) was
modified, the 217-row benchmark lane was not run, and `history-stride1` was not run.

**Verdict: built, measured, and it bought more than the estimate.** The native lane
fell from **5,695,678 B to 3,957,829 B** — a saving of **1,737,849 B**, which is
**228 B better** than Squad V2's measured anchor of 1,737,621 B and lands
**228 B below v0.1.6's own native lane** (3,958,057 B). `delta.trials` rose from
**37,886 to 38,538** (**+652** chunk trials, where the lane had previously run zero).
The apparent size fell **51,347,456 → 49,672,192 B**.

---

## 0. The amendment question, settled before any code was written

Read from the documents, not inferred:

| document | what it says |
| --- | --- |
| `core/docs/architecture/deferred/01-size-transition-delta-hints.md` §3 | claim **12** — "Core should consult the cursor per emitted chunk" — is labelled **proposed**; claim **11** — "A usable cross-role base at the transition" — is labelled **deferred** |
| the same paper, §4 | step 1, *"Implement the cursor in `core/` behind the existing four-slot type, with no storage-format change"*, carries **no gate**; step 3 gates *"the cross-role half only"* |
| the same paper, §4 step 3 | quotes the v0.1.7 design: *"no cross-role delta trick is required"* |

**The cursor was never deferred.** It is the paper's own *proposed* item, and issue
#185's revisit condition names it as the precondition for revisiting #185 rather than
as the deferred item. No owner amendment was needed and none was sought.

**The cross-role half was not implemented.** A base that is a whole-file object is
declined by the cursor's own classification before any chunk is emitted (§1.3), so a
`small → large` crossing still stores every chunk FULL. That half remains deferred and
still needs an amendment.

---

## 1. What was built

### 1.1 Diff summary

| file | change |
| --- | --- |
| `core/crates/layerfs-content/src/file/mapping/predecessor.rs` | **new** — `PredecessorBase` + `PredecessorCursor`; bounded forward walk over a stored chunked mapping |
| `core/crates/layerfs-content/src/file/mapping/mod.rs` | `mod predecessor;`, `pub use PredecessorBase`, `pub(crate) use PredecessorCursor`, `pub(crate) use build_streaming_with_predecessor` |
| `core/crates/layerfs-content/src/file/mapping/build.rs` | `build_streaming` now delegates; new `pub(crate) build_streaming_with_predecessor` consults the cursor once per chunk |
| `core/crates/layerfs-content/src/file/content.rs` | **new** public entry point `construct_bytes_with_predecessor`; `construct_bytes` and `construct_stream` delegate with `None` |
| `core/crates/layerfs-content/src/file/mod.rs`, `src/lib.rs` | re-exports |
| `core/crates/layerfs-content/tests/chunk_predecessor.rs` | **new** — 7 external tests |
| `core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs` | harness only — `LAYERFS_HISTORY_CHUNK_PREDECESSORS` switch and the base it offers |
| `core/docs/architecture/deferred/01-size-transition-delta-hints.md` | revised per its own **Upkeep** clause (the cursor now exists in `core/`) |
| `core/docs/architecture/09-delta-hints.md` | §14.1/§14.2/§14.4/§14.9 — the design is built for one route |
| `core/docs/architecture/08-representations.md` | §13.7 — the cursor is no longer absent |

**`core/crates/layerfs-storage/` was not touched.** Zero lines, exactly as Squad V2
predicted: `select.rs`'s `Chunk` arm already reads `advisory.first()` and
`push_chunk` already attaches a predecessor at slot 0.

### 1.2 LOC, by file

Production LOC is **non-blank, non-comment** lines in `core/crates/*/src/**/*.rs`
plus `core/crates/*/sql/**/*.sql`. `src/` contains no inline tests (forbidden by
`core/AGENTS.md`), so nothing has to be excluded for test code.

| file | production LOC before → after | physical lines after |
| --- | --- | --: |
| `file/mapping/predecessor.rs` | 0 → 171 (**+171**) | 220 |
| `file/content.rs` | 242 → 259 (**+17**) | 353 |
| `file/mapping/build.rs` | 313 → 328 (**+15**) | 422 |
| `file/mapping/mod.rs` | 18 → 22 (**+4**) | 27 |
| `file/mod.rs` | 17 → 18 (**+1**) | 24 |
| `lib.rs` | 31 → 32 (**+1**) | 49 |
| **`core/crates/` total** | **19,557 → 19,766 (delta +209)** | |

The new file is **220 physical lines**, inside the 999-line ceiling; every entry file
stays far inside the 200-line one.

**Snapshots, so the number is not read against the wrong baseline:**

| snapshot | core/crates | combined headline |
| --- | --: | --: |
| HEAD `66bce8378` (the stated baseline) | 19,537 | 84,936 |
| the working tree as received (HEAD + the parent's uncommitted storage work) | 19,557 | 84,956 |
| after L4 | **19,766** | **85,165** |
| **L4 alone** | **+209** | **+209** |

`git show HEAD:` over the whole `core/crates` tree counts **19,537** with this
counter, which reproduces the parent's stated core subtotal exactly. The **+20**
between 19,537 and 19,557 is the parent's own uncommitted `layerfs-storage` work,
not L4's.

Not production LOC, and reported so the accounting is complete:
`tests/chunk_predecessor.rs` is **297 physical lines** (tests do not enter the
production comparison), and the harness change is **+63 / −20** lines in
`ops/history.rs` (benchmark code is outside the product scope).

### 1.3 The shape that was built

```text
   construct_bytes_with_predecessor(policy, capacities, bytes,
                                    base: Option<PredecessorBase>, consumer, scope)
        │
        ├─ Representation::WholeFile  ──► base is IGNORED (no chunk, no cursor, no read)
        │
        └─ Empty | Chunked ──► construct_chunked
                 │
                 └─ content.chunk
                      ├─ PredecessorCursor::open(base, chunk)   1 read: the base root
                      │     classify() ─► not a FileState? ─► None   (cross-role declined)
                      └─ build_streaming_with_predecessor
                             start = builder.logical_len()
                             hint  = cursor.hint(start, chunk.len())
                             builder.push_chunk(chunk, hint, consumer)
```

The cursor holds a traversal frontier, **never payload bytes**: a mapping page is
acquired when the walk reaches it and released when the walk leaves it. Forward-only
(`start >= last_end`, else `InvalidRecord("predecessor span order")`), capped at
`PREDECESSOR_DESCRIPTOR_LIMIT = 4_096` descriptors, and exhaustion is **not** an
error — the chunk is then offered nothing and the construction is byte for byte the
one without a base.

**One attempted operation, no fallback.** A base the provider cannot serve is a
refusal that ends the construction (`MissingObject` propagates); it is never silently
treated as "no base". A base that is not chunked is declined by **classification**,
before any chunk is emitted — that is the deferred cross-role decision, not an
error-driven fallback.

---

## 2. The measured result

### 2.1 The two Stores

| | before | after |
| --- | --- | --- |
| path | `/tmp/l4-before/sample.sqlite` | `/tmp/l4-after/sample.sqlite` |
| sha256 | `83e12c92eb2ab75c90ddd260394f5e4ab768b3e8ba5bceee8c7ecf4010f1590e` | `888b6c4eb9af075c62eb13d117bb4fb5d57fba0b85b3a9b00ff8f23b762a3b91` |
| apparent | **51,347,456** | **49,672,192** |
| page_count / page_size | 12,536 / 4,096 | 12,127 / 4,096 |
| freelist | 30 | 37 |
| `schema_version` | 4 | 4 |
| corpus manifest sha256 | `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` | same |
| `PRAGMA quick_check` | ok | ok |
| `object_rows` | 52,032 | 52,032 |

The before Store **reproduces the parent's best arm exactly** — 51,347,456 B — so the
harness change is inert with the switch off.

**Against v0.1.6's gate (49,315,840 B):**

```text
  residual before   51,347,456 - 49,315,840 = 2,031,616 B   = 1.04120x
  residual after    49,672,192 - 49,315,840 =   356,352 B   = 1.00723x
  bought                                      1,675,264 B
```

### 2.2 The native (chunk) lane — decoded with `shared/space.py`

`pack_directory` for the lane bodies and framing; the record grammar inside a native
group body is read by the **existing, unchanged** E3 instrument
(`.../stage-6-history-187-.../squad-e/squad-e/e3_native_lane.py`), which shares
`space.py`'s container decoding and re-implements nothing in it. No new decoder was
written.

| native lane | before | **after** | v0.1.6 |
| --- | --: | --: | --: |
| records | 1,098 | **1,098** | 1,098 |
| raw bytes | 22,032,441 | **22,032,441** | 22,032,441 |
| FULL records | 1,098 | **448** | 448 |
| FULL raw | 22,032,441 | **8,604,206** | 8,604,206 |
| FULL stored | 5,690,762 | **2,378,527** | 2,378,527 |
| PREFIX records | 0 | **650** | 650 |
| PREFIX raw | 0 | **13,428,235** | 13,428,235 |
| PREFIX stored | 0 | **1,574,526** | 1,574,526 |
| group framing | 4,916 | **4,776** | 5,004 |
| native groups | 131 | **96** | — |
| `prefix_base_lane` | `{}` | **`{"native": 650}`** | `{"native": 650}` |
| **lane bodies** | **5,695,678** | **3,957,829** | 3,958,057 |

**Arithmetic (computed, residual 0):**

```text
  record bytes   5,690,762 - 3,953,053 = 1,737,709
  framing            4,916 -     4,776 =       140
  lane bodies    1,737,709 + 140        = 1,737,849   <- the saving
  Bound A (V2)                          = 1,737,621
  excess over Bound A                   =       228   = v0.1.6's framing 5,004 - ours 4,776
```

**The selection is byte-identical to v0.1.6's.** Not merely the same count — the same
1,098 records split 448 FULL / 650 PREFIX with the **same raw bytes in each class**
(8,604,206 / 13,428,235) and the **same stored bytes in each class**
(2,378,527 / 1,574,526). The whole 228 B advantage is group framing: core packs those
1,098 records into 96 native groups where v0.1.6 used more.

### 2.3 Why the file shrank by less than the lane did

```text
  lane bodies        46,085,449 -> 44,347,600   = -1,737,849
  pack framing          211,780 ->    211,108   =       -672
  pack blobs         46,297,229 -> 44,558,708   = -1,738,521
  page_count 12,536 -> 12,127 = -409 pages x 4,096 = -1,675,264   <- the apparent delta
  unexplained                                   =     63,257
     of which freelist grew 30 -> 37 pages      =     28,672
     remainder: partially-filled page slack     =     34,585
```

This is SQLite page granularity, not a second effect: the payload really did leave,
and 7 freed pages were left on the freelist rather than truncated.

### 2.4 The counters, and the chunk lane's first trials

Read from each Store's own `trace.jsonl`:

| counter | before | after | delta |
| --- | --: | --: | --: |
| `delta.prepared_full` | 45,239 | 45,239 | 0 |
| `delta.trials` | 37,886 | **38,538** | **+652** |
| `delta.no_candidate` | 7,326 | **6,674** | **−652** |
| `delta.work_exceeded` | 27 | 27 | 0 |
| `delta.prefix_records` | 38,714 | **39,364** | **+650** |
| `delta.prefix_selected` | 37,797 | 38,447 | +650 |
| `delta.full_records` | 13,318 | 12,668 | −650 |
| `delta.full_losses` | 89 | **91** | **+2** |
| `delta.ineligible_candidates` | 2,585 | 2,671 | +86 |
| `delta.absent_candidates` | 0 | 0 | 0 |
| `delta.reused` | 1,121 | 1,121 | 0 |
| `delta.inserted` | 52,032 | 52,032 | 0 |
| `delta.packs_created` | 261 | 254 | −7 |

**Residual 0 in both arms:** `trials + no_candidate + work_exceeded` =
37,886 + 7,326 + 27 = 45,239 and 38,538 + 6,674 + 27 = **45,239** = `prepared_full`.
The whole-file lane is counter-identical in both arms (44,141 objects = 37,886 trials
+ 6,228 no_candidate + 27 work_exceeded), so the entire delta is the chunk lane:

```text
  1,098 chunk objects =   652 trials        +   446 no_candidate
    652 trials        =   650 won (PREFIX)   +     2 lost (full_losses 89 -> 91)
    446 no_candidate  =    86 offered-but-ineligible (ineligible_candidates +86)
                       +   360 offered nothing at all
    738 offered a hint =   652 trials        +    86 ineligible
```

Every one of those four numbers is forced by the counters above and they close with
residual 0. The lane that had run **zero** trials now runs 652 and wins 650 of them
(**99.69 %**).

The **360 offered nothing** is a measured refinement of V2 §4.1's modelled "≈310
records have no chunked base": the model divided a byte total by an average chunk
size, and the measured figure is 50 records higher because chunks past the end of the
base mapping are also uncovered.

---

## 3. Access cost — bytes per read, never time

Measured from the after Store's `canonical_by_role` (identical in both arms):

| role | objects | canonical bytes | per object |
| --- | --: | --: | --: |
| `file-state` | 92 | 9,752 | **106.0 B** |
| `extent-leaf` | 92 | 53,368 | **580.1 B** |

A cursor open is **one read of the base root** (a `FileState`) and then **one read per
mapping page** the walk reaches. On this corpus every chunked file's mapping is a
single extent leaf, so a cursor costs **2 reads = 686.1 B** — exactly Squad V2 §6's
structural figure, now confirmed against the built code. The external test
`the_correspondence_reads_mapping_pages_and_no_payload` pins both halves: for a
600 KiB base the demanded ids are exactly `[base_root, mapping_root]`, the batch count
is **2**, and no chunk payload is ever demanded.

**Total, `est`** — the exact number of cursor opens was **not instrumented**. On V2's
population model (63 in-place edits + 17 crossings):

```text
  63 x 686.1 B  +  17 x 106.0 B   =  43,224 + 1,802  =  45,026 B
  share of the lane's 22,032,441 B of raw payload     =   0.204 %
  worst-case bound (105 chunked path-states)          =  72,041 B  =  0.327 %
```

---

## 4. Negative results, kept

1. **Squad V2's Bound B (1,893,759 B, est) is falsified.** It imported core's FULL
   ratio **3.87535x** from the *all-FULL* lane and applied it to the 448 records that
   survive selection. The observed stored bytes are **2,378,527**, i.e. a ratio of
   8,604,206 / 2,376,287 = **3.62086x** — v0.1.6's own figure, to the byte. The
   3.87535x was a **selection** effect (the all-FULL lane still contains the
   compressible records), not an encoder advantage, and it does not transplant.
   Its **PREFIX** import was exactly right — it predicted 1,574,526 B and that is
   what the lane stores — and its **FULL** import was 156,083 B short
   (2,222,444 predicted against 2,378,527 measured). The honest outcome is Bound A
   **plus 228 B of framing**.
2. **Two of 652 chunk trials lose.** The first overlapping base extent is not always
   the cheaper base; `full_losses` rises 89 → 91.
3. **86 of 738 offered hints (11.7 %) are ineligible before a trial.** The base chunk
   is itself a deep chain node at the chunk depth cap of 4, so the offer is spent
   without a comparison.
4. **360 of 1,098 chunk objects are offered nothing at all** — first appearances,
   representation crossings, and chunks past the end of the base mapping.
5. **`PredecessorProvenance::ReusedRange` is still never constructed by product
   code.** A cursor hint is a *reused range*, and `push_chunk` still tags whatever it
   is handed `UnchangedPrefix`. V2 §10 flagged this as cosmetic (C2's `select` drops
   provenance before choosing); it is unchanged, and deliberately so — changing it
   would alter an emitted advisory tag with no consumer, outside this item's scope.
6. **The `apply_edits` routes are untouched.** `replace_chunked` still offers
   `rightmost_payload(left)` for every chunk of a replacement and `stream_combined`
   still offers nothing, so chapter 14 §14.2's gap is still open on those two routes.
   `construct_stream` has no predecessor entry point either: nothing on this lane
   calls it, and an unused public variant is surface without a consumer.
7. **The cursor does not close the gate.** Residual **356,352 B = 1.00723x**. It is
   necessary, not sufficient.
8. **The apparent saving is 62,585 B smaller than the blob saving**, entirely SQLite
   page granularity and freelist (§2.3).

---

## 5. Checks run

| check | result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` | **481 passed, 0 failed** — 474 before this change, +7 new external tests; the T1 squad's 68 are a subset and none regressed |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test edit_pipeline -- --nocapture` | **6 passed**; the representation-transition pin reproduces: `store_edges=0 store_max_depth=0 store_canonical_bytes=0` |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | clean |
| `python3 core/tools/check_product_boundary.py` | **PASS**, 121 production Rust/SQL files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | **6 tests, OK** |
| `python3 -m unittest discover -s shared -p 'test_*.py'` (harness) | **136 tests, OK** |
| `--phase verify` on both Stores (declared sample) | 1,083 of 101,477 declared path-states compared across 17 states, 893 files / 5,619,947 B read, **0 mismatches, 0 missing, 0 unexpected**; gates `g1.o4-readback`, `g6.verify-state-count` and `g6.verify-sample-declared` all PASS. It is a **declared sample** (at most 64 paths per state), so the row is reported `INCOMPLETE`, never as a full PASS |
| the 7 new external tests | pass, including a **branched** base mapping (root at level 1, >128 extents) and a grown result whose appended tail is offered nothing |

`edit_pipeline.rs:641` was re-run rather than assumed, as the brief required. It
survives: the cursor changes in-place *chunked construction*, and that case is a
representation *transition*, where the cursor declines the whole-file base.

## 6. Checks NOT run, with the reason

| check | why |
| --- | --- |
| `--lane full` (220 rows) and `--lane smoke` (20 rows) | **forbidden by the brief** — the 217-row lane must not be touched. Both were already recorded as 220/20 and this change adds no row to either |
| `history-stride1` | **never optimised**, by rule |
| §14.6's four cases, two arms | not run. The measurement here is **one retained-history lane**, which is not the plan that paper asks for |
| the 4,096-descriptor ceiling | no test input reaches it — a base with more than 4,096 extents is tens of MB. The ceiling is read from source, not exercised |
| the exact number of cursor opens | not instrumented; §3's total is an `est` with its model stated |
| stride3 confirmation | none exists. **Nothing here is a gate claim** |
| timing of any kind | the machine is shared and the brief forbids it |

---

## 7. Reproduction

```sh
# build
cargo +1.85.1 build --release --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked

BIN=core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content
CORPUS=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data

# ARM A — the parent's best arm, unchanged (switch off)
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 \
LAYERFS_HISTORY_FALLBACK_PREDECESSORS=1 \
  $BIN --case history-stride10 --out /tmp/l4-before --corpus $CORPUS

# ARM B — the cursor
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 \
LAYERFS_HISTORY_FALLBACK_PREDECESSORS=1 LAYERFS_HISTORY_CHUNK_PREDECESSORS=1 \
  $BIN --case history-stride10 --out /tmp/l4-after --corpus $CORPUS

# the byte result, from the shared decoder (no new decoder)
cd core/benchmark/fs-bench-pro-storage-content && python3 - <<'PY'
import sys; sys.path.insert(0, ".")
from shared.space import footprint, pack_directory
for p in ("/tmp/l4-before/sample.sqlite", "/tmp/l4-after/sample.sqlite"):
    print(p, footprint(p).as_fields()["apparent_bytes"], pack_directory(p).by_lane)
PY

# the record grammar inside a native group, reused unchanged
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-e/squad-e/e3_native_lane.py \
  /tmp/l4-before/sample.sqlite /tmp/l4-after/sample.sqlite

# the sampled read-back (a second invocation over the same Stores)
for d in /tmp/l4-before /tmp/l4-after; do
  LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 \
  LAYERFS_HISTORY_FALLBACK_PREDECESSORS=1 \
    $BIN --case history-stride10 --phase verify --out $d --corpus $CORPUS
done
```

`LAYERFS_HISTORY_CHUNK_PREDECESSORS` **defaults off**, so every earlier arm on this
lane stays reproducible byte for byte and the new behaviour is one declared variable
wide against them.

---

## 8. What this changes for the campaign

- The chunk lane is **no longer a lane where no trial exists**. It runs 652 trials and
  wins 650; 3,957,829 B is **below v0.1.6's own native lane**.
- The remaining residual is **356,352 B = 1.00723x**, and the native lane is no longer
  part of it. V2's ordering — cursor first, #185 left deferred — was right, and #185's
  cross-role half is still **not** justified: it is unchanged and still needs an
  amendment.
- The falsified Bound B matters for the next estimate: **core's FULL frame ratio does
  not transplant across selections.** Any future "core encodes X % better" figure must
  be measured on the same record set, not imported from a different one.
