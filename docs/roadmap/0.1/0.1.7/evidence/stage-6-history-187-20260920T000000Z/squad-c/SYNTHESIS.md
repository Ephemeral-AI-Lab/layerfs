# Squad C — synthesis

Issue #187. **Every number here is diagnostic**, not admission evidence. HEAD `66bce8378` plus an
uncommitted harness-side change (`ops/history.rs`, production LOC delta **0**) and the pack decoder
moved into `shared/space.py`.

Inputs: `step0/README.md` (my Step 0 A/B), `squad-a/A1..A4`, `squad-b/B1`, `squad-b/B2`,
`squad-b/B4`. **Squad B3 did not report** and nothing here depends on it. B2 and B4 are
time-boxed partials and say so themselves.

---

## 1. Verdict

**The 2.65× is 81.9 % harness modelling, 9.3 % remaining coverage, 8.8 % schema overhead — and it
does not dissolve.**

```
this lane, apparent                    128,864,256
v0.1.6, apparent                        49,315,840     (retained artifact, sha256 80c2b10a…)
GAP                                     79,548,416   = 2.6130x

  cause 1  the driver declared no cross-commit base   65,126,400   81.87 %
  cause 2  remaining pack-blob gap (coverage)          7,403,406    9.31 %
  cause 3  non-pack SQLite overhead                    7,018,610    8.82 %
  ----------------------------------------------------------------
  total                                               79,548,416   residual 0
```

The exit criterion is **not met by the lane as it stands**: the faithful model lands at 63,737,856 B
apparent, 1.292× v0.1.6, not below 49,344,512 B. And "the two generations are not comparable" is
**not supported** — Squad A4 proved they are like-for-like artifacts. What *is* true is narrower:
**the registered `history-stride10` number measures a driver that declares no cross-commit base, so
it is not a product measurement of the retained-history workload.**

**But the criterion is reachable, and §4 gives the arithmetic: two product changes — cross-path-first
base selection (B2, measured with the product's own byte-exact encoder) and a lean SQLite row grammar
— land the Store at 47,048,435 B apparent, 0.954× v0.1.6.** Note that **neither change is the depth
fix**: E4 measured that fixing the depth bookkeeping buys legality, not bytes, so an unlimited
declaration cannot beat the depth-7 arm while the cap is 8. Two blind squads reached the same
conclusion about the deciding term before I computed it: B4's sensitivity #1 is the schema, and B1's
P2 floor "lands at 0.995×–1.051× of the gate" on the measured overhead. **The margin is ~2.3 MB, not
a comfortable one, and both product changes need an owner ruling.**

---

## 2. Where A and B agree — that is a finding

The two halves were run blind to each other. They agree on the load-bearing question.

| claim | Squad A (comparative) | Squad B (blind) |
| --- | --- | --- |
| The codec is not the cause | A1: parameters **byte-identical**; on the 44,141 common whole-file objects the FULL rate is **0.977288×** — v0.1.7 is 2.27 % *better*; worst-case sum of every policy/codec default difference = **11.69 %** of the excess | B1 (H3): "missing delta bases, not codec choice, explain the gap"; per-object no-delta baseline **114,511,800** is within 3 % of the Store's whole-file lane **110,941,054** |
| The bytes are in delta-base availability | A1: coverage collapsed **81.93 % → 23.54 %** of canonical bytes; the single class "v0.1.6 delta → v0.1.7 FULL" is 18,271 obj / **208,015,392** canonical = **78.74 %** of the excess | B1: the Store's pack bodies are **2.934×** above the per-object-delta floor |
| Duplicate storage is not the cause | A4: same 44,141 lane-4 oids, +7 objects / +414 B | B1: content = **0.99618×** the union |

**Four independent instruments, same conclusion.**

1. **A1** — decoded both pack directories and joined per object: coverage collapsed 81.93 % → 23.54 %.
2. **A2** — from the Store's SQL alone: of the 18,344 whole-file objects with a base, **18,344 / 82,033,173 B
   are same-save and 0 / 0 B cross-save**; the only cross-save bases at all are 917 InodeLeaf objects.
3. **B1** — from the corpus alone, never reading `crates/`: the Store is 2.934× above the per-object-delta floor.
4. **B2** — rebuilt the product's codec byte-exactly and measured: "**the advisory route carries ZERO
   deltas in this lane.** All 18,344 delta bases the Store holds are *same-save* matches from the
   per-save candidate cache; not one crosses a save boundary."

My Step 0 counters agree from a fifth direction: `delta.no_candidate = 26,847` with
`absent_candidates = 0`, `ineligible_candidates = 0`, `work_exceeded = 0` — the Store was not
*declining* bases, it was **supplied none**.

### 2.1 A calibration that changes how the rest must be read

**B4 rebuilt `codec.rs:258 compress_prefix` in a standalone Rust binary against the product's own
`libzstd.a`** and reproduced **44,146 of 44,148** real whole-file record lengths **exactly** —
residual **−8 B on 110,941,054 B**. B2 independently rebuilt the same call. **The product's delta
encoder faithfully implements its own specification.** Any claim that it is "broken" is wrong, and
this is the instrument that settles it.

---

## 3. Where A and B disagree — and the measurement that settles it

### 3.1 The disagreement

On essentially the **same byte set** — the delta-able whole-file versions — the two halves imply very
different stored sizes:

| source | bytes covered | stored | ratio |
| --- | --: | --: | --: |
| v0.1.6's own product (A4, decoded) | 285,486,222 | 16,310,397 | **17.503×** |
| my faithful arm, product encoder (Step 0) | 271,589,818 | 16,082,951 | **16.887×** |
| **B1's blind model** (`zstd --patch-from`, level 19) | 285,289,945 | 9,990,146 | **28.555×** |

B1's model is **1.63×** better than v0.1.6's own product on the same bytes. If B1 is right, the whole
"v0.1.6 was good" premise is wrong and there is a second, independent lever.

### 3.2 The measurement

I ran the full population — **no sampling** — over every same-path changed version in the stride10
selection: **29,302 pairs, 285,191,546 B canonical** (within 0.1 % of A4's 285,486,222 and B1's
285,289,945, three-way cross-check).

```sh
python3 /tmp/level_full.py      # 29,302 pairs, zstd CLI, 4 workers
```

| method | stored | ratio |
| --- | --: | --: |
| plain `zstd --patch-from` **-3** (the product's own level) | 11,971,222 | **23.823×** |
| plain `zstd --patch-from` **-19** | **9,977,483** | **28.584×** |

My independent `-19` figure, **9,977,483 B**, matches B1's **9,990,146 B** to **0.13 %** — two
separate implementations of the same model. B1 is **right**: the model is real, not an artifact.

### 3.3 But the first comparison was wrong, and this is the interesting part

23.823× against the product's 16.887× looks like a **1.411× encoder defect**. It is not — the two
populations have different size distributions, and compression ratio depends steeply on size:

| raw-size band | probe `-3` ratio |
| --- | --: |
| 0 – 2,048 | 9.269× |
| 2,048 – 8,192 | 16.761× |
| 8,192 – 32,768 | 22.681× |
| 32,768 + | 38.734× |

The product's with-base objects average **7,918 B**; the 300-pair sample averaged **10,980 B**. The
aggregate comparison was measuring the size distribution. **Banded**, and with the product's exact
record framing removed (the stored WholeFile PREFIX record is `tag(1) + base oid(32) + frame`, the
two length fields being dropped at assembly — **33 B/record × 34,300 = 1,131,900 B**):

| raw band | product canonical | product stored | product | probe `-3` | expected at probe ratio |
| --- | --: | --: | --: | --: | --: |
| 0 – 2,048 | 10,555,649 | 1,941,027 | 5.44× | 9.27× | 1,138,813 |
| 2,048 – 8,192 | 52,640,748 | 4,372,379 | 12.04× | 16.76× | 3,140,749 |
| 8,192 – 32,768 | 131,197,097 | 7,050,343 | 18.61× | 22.68× | 5,784,411 |
| 32,768 + | 77,196,324 | 2,719,202 | 28.39× | 38.73× | 1,992,997 |
| **total** | **271,589,818** | **16,082,951** | **16.887×** | **23.823×** | **12,056,971** |

```
product stored                    16,082,951
  less record framing             -1,131,900   (34,300 x 33 B, exact)
  = product frames                14,951,051
banded plain --patch-from -3      12,056,971   (22.526x)
  frame ratio                          1.2400x   excess 2,894,080 B
```

**A residual of 2,894,080 B (1.2400×) remains — and it is NOT an encoder defect.** B4 rebuilt
`compress_prefix` against the product's own `libzstd.a` and reproduced **44,146 of 44,148** real
record lengths exactly (−8 B on 110,941,054 B); B2 re-encoded all 44,148 records and got **FULL
93,744,892 = 93,744,892 and PREFIX 17,196,162 = 17,196,162, 0 per-object mismatches**. The product's
encoder does exactly what `codec.rs:287-296` specifies — level 3, an explicit `windowLog`,
contentSize+checksum, base via `ZSTD_CCtx_refPrefix`.

So the 1.2400× is a **codec-mode** difference between `ZSTD_CCtx_refPrefix` as configured and the
CLI's `--patch-from` mode, not a bug. It is real and measurable, but it is **not** the lever I would
recommend, because B2 measured a larger and product-expressible one (§3.4). Level is separately worth
**1.200×** (−3 → −19 on the identical population: 23.823× → 28.584×); `--long=27` adds nothing
(32.439× → 32.459×), confirming B1's N2.

### 3.4 The lever that is bigger and shippable: base *selection*, not the codec

B2 measured base-choice rules with the **product's own byte-exact codec**, whole-file lane,
348,460,709 B canonical, depth cap 8 and both chain budgets enforced:

| rule | whole-file lane stored | ratio |
| --- | --: | --: |
| today (measured) | 110,941,054 | 3.141× |
| no delta at all | 119,815,884 | 2.908× |
| R1 same-path **previous** version | 45,926,982 | 7.587× |
| R2 same-path **any** earlier version | 45,230,840 | 7.704× |
| R3 cross-path top-4 | 36,405,522 | 9.572× |
| **R4 cross-path-first, 4 slots** | **35,937,886** | **9.696×** |
| R4 + the measured cache elsewhere | **34,839,722** | **10.002×** |

Three things in that table matter more than the headline:

* **R2 is a negative result.** Searching *all* earlier versions of a path buys **+696,142 B, 1.5 %**.
  The immediately-previous version is essentially the whole same-path win.
* **Cross-path is the real win**: **33,210 of 43,887** non-first-state objects (**75.7 %**) have an
  already-stored near-identical object at a **different** path.
* **"Try all four and keep the smallest frame" is WORSE** (35,937,886 → 36,371,329, **+433,443 B**),
  because it deepens chains and pushes later objects past the depth cap. The order must be
  `[cross-path best, cross-path 2nd, same-path previous, same-path earlier]`; putting `previous`
  first costs **297,925 B**.

**My own l7 whole-file lane was 44,510,533 B; R4 + cache is 34,839,722 B — 9,670,811 B better, with
the same codec.** That is the shippable lever, and it is a **C1** change: B2 showed C2's selection is
already correct and C1 simply never hands it a cross-save candidate.

---

## 4. A proposal that beats v0.1.6, with the arithmetic

Four changes, in descending order of size. **Changes 2–4 are `core/crates/` changes and require an
owner ruling; none is made here.**

**Two changes carry it. Both are `core/crates/` changes and neither is made here.**

| # | change | layer | saving | resulting apparent | basis |
| --: | --- | --- | --: | --: | --- |
| 0 | `l7` faithful model, as measured | harness | — | **63,737,856** | **measured** |
| 1 | **Base selection → R4_crossfirst** (cross-path-first, 4 slots). Replaces l7's whole-file lane 44,510,533 with B2's 34,839,722, **same codec** | C1 | 9,670,811 | 54,067,045 | B2, measured with the product's own byte-exact encoder |
| 2 | **Non-pack SQLite overhead** 197.5 → a lean grammar | C2 | 7,018,610 | **47,048,435** | both sides measured; B4's independent model says −8,514,225 |
| | v0.1.6 | | | 49,315,840 | measured |
| | **margin** | | | **−2,267,405 (0.954×)** | |

**The margin is thin, and two blind squads independently said so before I computed it.**

* **B4, blind:** modelled floor **56,162,647 B** under the real grammar *and* the real SQLite schema,
  and an explicit negative result — *"this corpus does not admit a Store below v0.1.6's 49,344,512 B"*
  (+6,846,707, +13.9 %). **Its sensitivity #1 is the SQLite schema**: a lean 16 B/row grammar gives
  **47,648,422 B (−8,514,225)** — below the gate. B4 and my Step 0 landed on the *same deciding term*
  from opposite directions.
* **B1, blind:** a store rebuilt at its own P2 floor, paying the *measured* overhead, lands at
  **≈49.1–51.8 MB = 0.995×–1.051× of the gate.** "Closing the delta-base gap is necessary but probably
  not sufficient to clear the gate with margin."

**Three further levers are measured but not needed to clear the gate, and each carries a cost:**

| lever | worth | cost |
| --- | --- | --- |
| Grouping (B3) | **1.2637×** at the product's own 256 KiB pack size | **2.95 % WORSE on the 18,344 records that already carry a prefix dictionary**, and **59.8×** single-record read amplification. Grouping does **not** subsume the prefix dictionary |
| Level 3 → 19 (B3) | 1.0895× per record; **L15 per record (110,333,892 B) already matches the entire current lane (110,941,054 B)** | CPU cost unmeasured — timing was invalid on this machine |
| Codec mode `refPrefix` → patch-style (§3.3) | 1.2400× | mechanism not pinned down; superseded by lever 1 |

**Where real margin would come from:** B1's P1 — **27,184,431 B (13.682×)** at 254 group frames over
per-path chains, ≤ 11.26 MB decoded per read. That is 1.50× below P2. It needs a grouping change and
an access-cost trade the owner has to accept. **No timing was taken anywhere in this campaign** — the
machine was shared by eight analysis agents — so every access cost is stated in **bytes per read**,
never seconds.

---

## 5. The product defect Step 0 found — **reported, not patched**

The faithful arm is rejected: `Integrity("dependency chain depth")` (`delta/read.rs:159-168`).

* `off` arm: max chain depth **8** (InodeLeaf only, 22 objects) → readable → **PASS**.
* `on` arm: max chain depth **9** — 301 objects at depth 8, **62 at depth 9**, all WholeFile → one is
  read back → **ABORT**.
* Depth limit **7 completes; 8, 10, 12 and 16 all abort.**

**The bounds are consistent — this is NOT a bound off-by-one** (corrected by E4 against the
product's own `delta_chains` suite: 9 passed, including a cap-2 policy storing *and reading back* a
2-edge chain). The writer produces edges ≤ cap and the reader refuses exactly edges > cap. **The writer
is not exceeding its policy — it is mis-measuring the depth.** The mechanism, **[H] now proved by the
writer's own output**: `DepthCache::cost_of` (`select.rs:94-144`) records every level one edge short
when its walk terminates on an already-cached entry, because the terminating entry is not pushed onto
`path` but `position` starts at 0 for the element whose base it is. **Proof:** `/tmp/s0_l7` holds
exactly **4 role-1 objects at 9 edges whose bases sit at true depth 8** — a base at depth == cap is
ineligible, so those bases were admitted on a depth value ≤ 7 while the truth was 8.
**Fix: ~4 lines, one function, no format change. It buys LEGALITY, NOT BYTES (~2,248 B)** — every base
at depth ≥ 8 is refused regardless, so an unlimited arm cannot beat the depth-7 arm while the cap is 8.
An earlier draft attributed 4,315,905 B to this fix; **that was wrong**, and §4 no longer rests on it.

**Gate caveat.** The `l7` arm is **not gate-clean**: only `g1.o1` and `g4.swaps` PASS, `g1.o3` is
INCOMPLETE, there are **zero `g1.o2`/`g1.o4` records in either trace**, and `--phase verify` is
refused for `history.*`. **No read-back ran, so the 4 unreadable objects were never touched.** The
size reading stands; the PASS does not cover readability.

**Why it was latent:** it needs a chain longer than the cap, and in the `off` arm the WholeFile lane
never exceeds depth 1 — the per-save candidate cache is whole-file-only and is dropped at the end of
each save. **No producer declared cross-commit whole-file bases, so the two bounds were never pushed
apart.** The reference tree has no such asymmetry (write declines at 8, read tolerates 50 — A3).

---

## 6. Ruled out, with the number that ruled it out

| ruled out | by |
| --- | --- |
| A codec or zstd-parameter regression | A1: parameters byte-identical; FULL rate 0.977288× |
| Level as the *sole* cause of the delta gap | this report §3.3: the probe is at the product's level 3 and still wins 1.2400× |
| A workload difference | A4: both 561,010,345 logical bytes; content 0.99618× the union |
| The 128 KiB cutoff / #185's churn axis | A2/A4: identical lane-4 set, 44,148 vs 44,141 objects, +414 B |
| The whole-file depth cap being the blocker | A1: v0.1.7 already builds to depth 8 |
| Chunk chain closure, `GROUP_TARGET`, metadata depth/closure | A1: ceilings 0 B, +19,564 B, and a **wrong-signed** 221,539 B |
| Tree-role advisories mattering | A2: **inert** — `select.rs:204-222` returns `encode_full` before reading the advisory; 0 bases across 4,770 DirectoryLeaf / 92 FileState / 17 FilesystemRoot |
| The candidate cache reaching a previous state | A2: per-save, dropped at `cas/store.rs:392`; v0.1.6's persists across sessions |
| `--long=31` or a bigger window helping | B1 N2 and this report §3.3: byte-identical / +0.02 % |
| **"WholeFile and Native = `GroupCodec::Raw`, one record per group"** (the brief's premise) | **B3 refutes it**: WholeFile bodies are `[tag] (+32 B base id) + a zstd frame`; the encoder is exactly zstd **L3 + checksum** (25,804/25,804 frames byte-identical); and the "delta base" is a **raw prefix dictionary**, not a delta stream (all 18,344 tag-1 frames decode with the base as a dictionary) |
| The delta encoder being defective | B4: 44,146/44,148 record lengths exact, −8 B on 110,941,054; B2: FULL 93,744,892 = 93,744,892 and PREFIX 17,196,162 = 17,196,162, **0 mismatches** |
| **A writer/reader bound off-by-one** (my own §5 first reading) | **E4 refutes it**: the bounds are consistent, and the product's `delta_chains` suite (9 passed) stores *and reads back* a chain at exactly the cap. The defect is depth *measurement*, not the bound |
| **The advisory list being globally empty** (my own PROPOSAL.md first draft) | **E1 refutes it**: `filesystem/sorted/page.rs:380-390` is a **live** producer and the sole source of the **917 cross-save InodeLeaf bases**. Empty only on the file-content path |
| **The depth fix unlocking 4,315,905 B** (my own §4 first draft) | **E4 refutes it**: it buys legality, ~2,248 B. Every base at depth ≥ 8 is refused under a cap of 8 |
| Grouping subsuming the prefix dictionary | B3: the 18,344 based records get **2.95 % worse** at 256 KiB grouping |
| Trained dictionaries as the lever | B3: 1.14–1.17× on a held-out split, where the raw prefix dictionary is **7.87×** |
| Same-path *any* earlier version helping | B2 R2: **+696,142 B, 1.5 %** over the immediately-previous version |
| "Try all 4 slots, keep the smallest frame" | B2: **worse by 433,443 B** — it deepens chains past the depth cap |
| "16 B per object row" | B3/B4: it is **172.4 B/object** including the two indexes |
| Cross-path dedup as a lever | B1 N5: +1.55 % |
| `st_blocks × 512` as a gate quantity | this report §7 |
| The recorded v0.1.6 totals being wrong | A4: reconciled to the byte, residual 0, against the retained sha256 |

---

## 7. The allocated-bytes axis is unusable — resolve §9's blocker this way

For the **same, closed, unmodified** Store file, `st_blocks × 512` read: **135,118,848**, then
135,192,576, then 130,799,104, then **134,537,216** — a **4,393,472 B** range. Two Stores of identical
size and identical content read **262,768** and **262,992** blocks.

Reproducible instead: `PRAGMA page_count × page_size = st_size` exactly on every arm, with
`freelist_count` 26–28 pages (**106–115 KB**) as the only genuine slack.

**Recommendation: decide the gate on apparent bytes (`st_size`), state the freelist, and never on
`st_blocks × 512`.** v0.1.6's recorded allocated/apparent pair differs by 28,572 B = 0.058 %; this
lane's readings differ by up to 4.4 MB = 3.4 %. The two generations' *allocated* readings are not the
same quantity, which is a second, independent reason the registered gate number cannot be used as
written.

---

## 8. Owner rulings required

1. **The depth bookkeeping defect** (`DepthCache::cost_of` / the writer-reader bound). A
   `core/crates/` change. It blocks the faithful model outright.
2. **The producer.** A benchmark driver is not a layer. The layer that declares cross-commit bases is
   **Stage 7 / Workspace**, and **the Stage 7 specification does not exist** — the entire in-tree
   normative text is one row (`implementation-issues.md:21`). A3's call: **expected migration-stage
   consequence**, with the qualification that core has the type, the vocabulary and a *live*
   cross-save producer (917 InodeLeaf bases) but no whole-file entry point that accepts a base.
3. **Which baseline is admissible** — the retained 49.3 MB with in-record cross-commit bases, or
   core's own no-base cost of ~135.8 MB (A4's measured counterfactual). This decides whether "beat
   v0.1.6" is even the right framing.
4. **The access-cost trade.** B1's P1 (27.18 MB) is where the margin is; it needs a grouping change.
5. **The registered lane's number.** On this evidence it should not be carried as a product
   measurement.

---

## 9. Checks NOT run, and why

* **`runner.py perf` / the registered lane.** Verify phase, golden table,
  `tests/history_declarations.rs`, lane wiring and per-lane command ceilings are not built.
* **`history-stride3` confirmation.** Guardrail says iterate on stride10, confirm on stride3.
  **No stride3 confirmation exists and no stride3 claim is made.** (B1 read stride3 from the corpus
  for its floor; B2 and B3 both left it NOT RUN. That is not a lane confirmation.)
* **Squad B2 Q5 (depth-cap sweep)** and **B3 items 4b/4c (per-extension dictionaries, LDM sweep)**
  were time-boxed out. B3 wrote and did not run the content-hash join that would separate "had a
  predecessor but got no base" from "got a base"; it names that as the highest-value follow-up.
* **B3's CPU cost of level 15/19.** Measured ratio only; no timing, by the same rule as everything else.
* **Any product-source change.** Owner ruling required.
* **Any timing.** Eight analysis subagents shared the machine; every timing reading would be invalid.
* **`cargo fmt --check`.** Not clean on this tree; nothing was reformatted.
* **The 217-row lane.** Untouched: registry, cardinality, golden table, `--lane full` (220 rows) and
  the `full` verification default (20 smoke rows) unchanged.
* **`history-stride1`.** Never run, never optimised.

## 10. Production LOC

`core/benchmark/` is its own Cargo workspace and is **not** product source.
**Production LOC: 84936 -> 84936 (delta 0)**; harness lines stated separately. No lockfile move, no
new dependency. `shared/test_space.py` 136 tests pass; `python3 shared/space.py` self-check passes.
