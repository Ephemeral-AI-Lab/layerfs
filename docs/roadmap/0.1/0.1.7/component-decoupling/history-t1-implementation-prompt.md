# T1 implementation — handoff

> **Status:** Paste-ready entry point for the implementation squad.
> Tracked as [#188](https://github.com/Ephemeral-AI-Lab/layerfs/issues/188), a **sub-issue of**
> [#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187), itself a sub-issue of
> [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186).
> Deferred and **out of scope**: [#189](https://github.com/Ephemeral-AI-Lab/layerfs/issues/189)
> (T2/T3/floor, label `deferred`) and [#185](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185)
> (a different axis — do not conflate).
> Working directory: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

**Read §3 before §2, and read §5 before writing a line of product code.** The gate is only meaningful
once §5's harness gaps are closed; today a PASS covers no read-back.

---

## 1. Your mandate and the terminal condition

Implement **T1** and drive it to a terminal state. T1 is specified in
[`docs/roadmap/0.1/0.1.7/retained-history-t1/`](../retained-history-t1/README.md) — read both
documents there before starting. The target:

````text
history-stride10, 17 states, apparent bytes (st_size)
  today     128,864,256
  T1 target  47,048,435   = 0.954x v0.1.6's 49,315,840
```

### Terminal SUCCESS — all six, or it is not success

1. `history-stride10` re-run **under #186's contract**: one sample, declared cache state, one
   construction worker, fresh output path, receipt retained.
2. **Apparent bytes <= 49,315,840** on the re-run, read as `st_size`. **Never** `st_blocks x 512` — §6.3.
3. **The verify phase runs and PASSES**: every state's root read back and compared against the corpus
   oracle. Today this cannot run at all (§5).
4. The **golden table** matches, and every **residual is zero or stated as a number**.
5. **Stride3 confirmation** — the guardrail requires it, and none exists today.
6. A ledger entry, and the evidence directory, both append-only.

**Then, and only then, close #188** — with a comment that carries the receipt path, the identity pins,
the gate number, and the residual.

### Terminal BLOCKED — also a legitimate outcome, and it does NOT close #188

If a required **owner ruling** is refused, or the producer genuinely requires an unbuilt layer, **stop
and report** with the arithmetic. Leave #188 **open** and write the blocker in a comment. A blocked
issue with a measured reason is a good outcome; a closed issue without the gate is not.

### You are explicitly encouraged to

- **run small experiments** rather than reason in the abstract. This campaign has already been wrong
  three times by inference — a misdiagnosed defect mechanism, an "empty advisory list", and an
  "encoder defect" that was a size-distribution artifact. Each was caught by measurement;
- **use subagents heavily and in parallel**, in the squad structure of §7;
- **falsify your own result** before claiming it.

---

## 2. The target arithmetic

````
  128,864,256  registered lane
               |  R0  declare the base            -65,126,400   (81.87 % of the gap)
   63,737,856  the faithful model                                 [measured]
               |  R1  index: capacity + lifetime   -9,670,811
   54,067,045                                                    [computed]
               |  R2  lean row grammar             -7,018,610
   47,048,435  T1 TARGET
   49,315,840  v0.1.6                          margin 2,267,405 B = 0.954x
````

**The margin is thin.** Three independent models agree the **schema** decides it: B4's blind floor is
56,162,647 B under the real schema and **47,648,422 B** with a lean one; B1's blind P2 lands at
**0.995x–1.051x** of the gate. **If R2 does not land, T1 does not land.**

---

## 3. What is established — measured, reproducible, do not re-derive

### 3.1 The root causes, residual 0

| # | cause | bytes | share |
| --: | --- | --: | --: |
| 1 | the caller declared no cross-commit delta base | 65,126,400 | 81.87 % |
| 2 | remaining pack-blob gap (coverage) | 7,403,406 | 9.31 % |
| 3 | non-pack SQLite overhead | 7,018,610 | 8.82 % |

### 3.2 The mechanism, measured

- A whole-file record is `tag(1) [+ base oid(32)] + zstd frame`; the base content is a **raw prefix
  dictionary**. Compressed **alone: 2.976x**; with a stored version as the dictionary: **23.416x**.
  **The dictionary is worth 7.8678x.**
- Whole-file lane today: 25,804 objects / 266,427,536 B canonical -> **93,744,892 B at 2.842x** with
  **no base**; 18,344 / 82,033,173 B -> 17,196,162 B at 4.770x with a same-save base.
- `delta.no_candidate = 26,847` with `absent_candidates = 0`, `ineligible_candidates = 0`,
  `work_exceeded = 0`. The Store is **supplied** no base; it does not decline one.
- All 18,344 whole-file bases are **same-save**. **Zero cross-save.** The only cross-save bases at all
  are **917 InodeLeaf** objects.
- **The codec is not the cause**: the v0.1.7 parameter set is byte-identical to v0.1.6's, and on the
  44,141 whole-file objects both Stores hold, v0.1.7 is **2.27 % better** (FULL rate `0.977288x`).

### 3.3 R0 — the declaration, and its ordering

`AdvisoryPredecessors` already crosses the C1/C2 boundary (four slots, provenance tags) and is already
consumed at `cas/save.rs:100` -> `encoding/delta/select.rs:342`, which probes **in order** and returns
the **first eligible** one. **R0 needs 0 lines of core.**

Measured with the product's **own byte-exact codec** (whole-file lane, 348,460,709 B canonical):

| order | stored |
| --- | --: |
| today | 110,941,054 |
| same-path **previous** first | 45,926,982 |
| **cross-path best, cross-path 2nd, same-path prev, same-path earlier** | **35,937,886** |
| ...plus the measured cache elsewhere | **34,839,722** |

**"Try all four and keep the smallest frame" is worse by 433,443 B** — it deepens chains past the depth
cap. Ordering is a load-bearing parameter, not a detail.

### 3.4 R1 — the index

The product **already contains the right index**: `encoding/delta/candidates.rs` is a content-keyed
similarity index (16-byte window, rolling 257, `mix()`, eight smallest hashes, >=2-of-8 match). It
mentions no path, no save, no commit. Measured coverage of 44,148 whole-file objects:

| correspondence | key | objects |
| --- | --- | --: |
| today's per-save cache | content, 1 save | 18,433 (41.8 %) |
| caller per-path map | PATH | 29,056 (65.8 %) |
| **store index, K = the product's own 8-hash signature** | **CONTENT** | **37,200 (84.3 %)** |

**The content key strictly dominates the path key: +8,144 objects (+28.0 %).** Of the **+18,767**
objects a persistent unbounded index buys: **lifetime alone +2,199 (11.7 %)**, **capacity +16,568
(88.3 %)**. **Merely persisting the existing cache buys +11.9 %.**

### 3.5 R2 — the row grammar, decomposed

`dbstat` on the faithful Store (`/tmp/s0_l7`), non-pack **9,609,216 B**:

| component | bytes | removable? |
| --- | --: | --- |
| `objects_bases` | 2,805,760 | **yes** — the base id is **already in the packed record** (`record.rs:87-89`); the column and index are redundant |
| `base_object_id` column inside `objects` | ~614,400 | **yes** — same reason |
| `objects_locations` | 2,547,712 | **only with a cleanup change** — `cleanup.rs:48` is its only consumer |
| | **5,967,872** | **85 % of R2** |

````
  l7 non-pack (dbstat)                        9,609,216
    - objects_bases                          -2,805,760  -> 6,803,456
    - the base_object_id column                -614,400  -> 6,189,056
    - objects_locations                      -2,547,712  -> 3,641,344
  v0.1.6's measured non-pack                   3,259,108
  residual                                       382,236
````

**Dropping only `objects_bases` recovers 40 % of R2, not 100 %.**

### 3.6 R3 — the depth defect

The faithful model is rejected with `Integrity("dependency chain depth")`. **The bounds are
consistent** — the writer produces edges <= cap, the reader refuses exactly edges > cap; the product's
own `delta_chains` suite passes (9 tests, including a cap-2 policy storing *and reading back* a 2-edge
chain). **The defect is depth *measurement*:** `DepthCache::cost_of` (`select.rs:94-144`) does not push
the cache-hit entry onto `path` yet computes `depth = cost.depth + position`, so every level is recorded
**one edge short**. **Proved by the writer's own output:** `/tmp/s0_l7` holds **4 role-1 objects at 9
edges whose bases sit at true depth 8**. **R3 buys legality (~2,248 B), not bytes.**

---

## 4. What is NOT established — the open questions you must close

1. **The optimal `SLOTS` / `INDEX_BYTES` is unmeasured.** E2 measured 1,024 slots -> 18,433 objects and
   unbounded -> 37,200. **Nothing in between was swept.** `INDEX_BYTES` is a *declared bound*, and
   raising it to 7 MiB holds **~15 % of a 47 MB Store resident**. **Sweep it before choosing.**
2. **`objects_locations`'s removal is unscoped.** It needs the failed-save cleanup path to stop
   scanning by location (`cleanup.rs:48`). Not yet designed.
3. **The producer.** A benchmark driver is not a layer. In production the caller is **Stage 7 /
   Workspace, and the Stage 7 specification does not exist** — the entire in-tree normative text is one
   row (`component-decoupling/implementation-issues.md:21`). **This is the critical-path blocker.**
4. **No stride3 confirmation exists.** No stride3 claim may be made until one does.
5. **The registered lane's number is not a product measurement** — the driver declared no base.

---

## 5. The gate is not real until these harness gaps are closed

From the #187 handoff, still open: **the verify phase, the golden table,
`tests/history_declarations.rs`, the runner's lane wiring and the per-lane complete-command ceilings
are not built.** `ops/history.rs:426-428` **refuses `--phase verify` for `history.*`**, so **no
read-back ever runs and a PASS does not cover readability** — the `l7` arm recorded only `g1.o1` and
`g4.swaps`, with **zero `g1.o2`/`g1.o4` records in either trace**.

**Squad S1 owns this and it is the critical path.** Nothing may be called a gate result before it lands.

---

## 6. Method requirements

- **Arithmetic or it did not happen.** Every claim carries bytes in, bytes out, ratio, residual.
- **Label every estimate.** `measured` / `computed` / `est` / `hypothesis`. This campaign has published
  three retractions; do not add a fourth.
- **One variable per experiment.** A change that moves two things proves neither.
- **Attribute only what is attributable.** The `Native`/`Ordinary` lanes hold multi-record groups; a
  naive join multiply-counts. Use `space.whole_file_records`, which refuses them.
- **Retain negative results.** A ruled-out hypothesis is a deliverable.
- **Read the specification before optimising.**
- **Nothing is admission evidence until it is re-run under #186's contract.**
- **No timing claim** unless the machine is quiet, the cache state is declared, and `AGENTS.md` §1 is
  satisfied. Every access cost in this campaign is stated in **bytes per read**.

### 6.1 The rulings — Step 0, before any product line

The owner has directed T1 implementation. **Record each ruling explicitly before the change it
authorises**, because two are irreversible:

| ruling | covers | format change | reversibility |
| --- | --- | --- | --- |
| **A** | **R3** depth measurement; **R1a** index constants; **R1b in-memory** | none | reversible |
| **B** | **R1b persisted** (survives a restart) | new table + `SCHEMA_VERSION` 4->5 | **needs an amendment** |
| **C** | **R2** lean row grammar | `SCHEMA_VERSION` bump, DDL, record/column move | **needs an amendment** |
| **D** | **R0's producer** — who owns the correspondence in production | none | blocked on Stage 7 |

**Do not fold B or C into A.** If a ruling is not granted, that work is **BLOCKED**, not skipped
silently.

### 6.2 The allocated-bytes contract

`st_blocks x 512` on this volume is **not reproducible**: the same closed file read 135,118,848,
135,192,576, 130,799,104 and 134,537,216 B, and two Stores of identical size and content read 262,768
and 262,992 blocks. **The gate is decided on `st_size`** (= `page_count x page_size`, exact on every
arm), with `freelist_count` stated.

---

## 7. Squad structure

Launch **S1 immediately** — it gates everything. S2–S4 may run in parallel behind it; S5 last.

### S1 — the gate (harness, **critical path**, no ruling needed)
Build the verify phase for `history.*`, the golden table, `tests/history_declarations.rs`, the runner's
lane wiring, and the per-lane complete-command ceilings. Wire `space.pack_directory` into the receipt.
**Deliverable: a `history-stride10` run whose PASS covers read-back of every state's root.**

### S2 — the index (product, ruling A)
R1a capacity **and the sweep §4.1 demands**; R1b in-memory lifetime. Report the memory-vs-coverage curve
and the chosen `INDEX_BYTES` with its justification. **Falsifier: if capacity is not binding, widening
it moves nothing.**

### S3 — the row grammar (product, rulings B/C)
R2, the amendment draft, and the `objects_locations` cleanup-path design. **This is the change the gate
depends on.** Report the `dbstat` decomposition before and after.

### S4 — depth and the producer (product, ruling A for R3, D for R0)
R3 (~4 lines, one function). Then R0's producer question: what is the smallest correspondence core can
own, and what does Stage 7 have to supply? **E4's finding: the declaration needs 0 lines of core.**

### S5 — adversarial verification (last, and it must be hostile)
Re-run the gate independently. **Try to falsify every claim.** Re-run E4's F1 (cap 2, chain
`Q -> B1 -> root`; accept `W` with `explicit(B1)` then `Z` with `explicit(Q)` inside one save: with the
defect `trials == 2` and reading `Z` fails). Re-check the residuals. **If you cannot falsify it, say
what you tried.**

---

## 8. Guardrails — not negotiable

- **The 217-row lane is not yours to change.** Registry, cardinality, golden table, `--lane full`
  composition and `full` verification default all stay. `--lane full` is **220 rows**, `--lane smoke` is
  **20**. `history.*` is outside the 217 by construction.
- **Never optimise `history-stride1`.** Iterate on stride10; confirm on stride3.
- **A canonical pin that does not match stops the phase.** It is a finding, not a fixture tweak.
- **No format change without ruling B or C.** A `SCHEMA_VERSION` bump invalidates every existing Store.
- **Production LOC: `84936 -> 84936 (delta 0)`** is the *baseline*; T1 will legitimately raise it. Report
  **before -> after (delta)** per commit, with the core subtotal separate. No lockfile move, no new
  dependency; `shared/test_lock_parity.py` fails a harness-only registry package.
- **No CI claim.** This repository runs no CI, `tools/preflight.sh` is permanently retired, and
  `cargo fmt --check` is not clean on this tree — do not reformat files outside your change.
- **Do not edit the historical record.** Rounds 1–5b and their prompts, plans, handoffs and receipts are
  not rewritten. `#171` is closed; do not reopen it.
- **`core/` product rules apply:** files < 1000 physical lines, `lib.rs`/`mod.rs` <= 200, no inline
  tests in `src/`, no third-party patches, one attempted operation.
- **Never patch, vendor or fork a third-party crate.**

---

## 9. Blast radius and size — measured, so you can plan

**11 files, 2 crates, 1 SQL file. No new files, no splits needed.** Every file in scope has **>= 383
lines of headroom**; no `lib.rs`/`mod.rs` exceeds 150 lines. The only product file near the ceiling is
`layerfs-content/src/file/edit/tree.rs` (978) and **it is out of scope**.

| change | net LOC (estimate) |
| --- | --: |
| R0 | **0** (optional C1 entry point: +18) |
| R1a | +3 |
| R1b in-memory | +45 |
| R1b persisted | +145–295 |
| R2 | +15 |
| R3 | +4 |
| **R0–R3, in-memory** | **+95 to +185** |

**The LOC is small and that is the risk.** The change is wide and shallow, across a schema boundary,
with a format change and a memory statement in it.

---

## 10. Deliverables

1. **The gate receipt**, with identity pins and the verify phase passing.
2. **A residual register** — every cause, its bytes, its measurement, and a residual that sums.
3. **The rulings**, recorded, each against the change it authorised.
4. **The two amendments** (rulings B and C) if those changes proceed.
5. **Everything falsified**, with the number that falsified it.
6. **Evidence under** `docs/roadmap/0.1/0.1.7/evidence/<stamp>/`, append-only, with the commands run and
   every check *not* run named with its reason.
7. **A ledger entry**, per `AGENTS.md` §3.6.
8. **The #188 closure comment** — receipt path, pins, gate number, residual — or a blocker comment that
   leaves it open.

---

## 11. Where everything is

Below, `…/` is `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/`.

| | |
| --- | --- |
| T1 index and target | `docs/roadmap/0.1/0.1.7/retained-history-t1/README.md` |
| T1 specification | `docs/roadmap/0.1/0.1.7/retained-history-t1/t1-implementation.md` |
| Blast radius and LOC | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-c/T1-BLAST-RADIUS.md` |
| Recommendation and tiers | `…/squad-c/RECOMMENDATION.md`, `…/squad-c/TIERS-AND-COMPLEXITY.md` |
| Step 0 and the root-cause register | `…/step0/README.md` |
| The synthesis | `…/squad-c/SYNTHESIS.md` |
| The advisory list, exhaustively | `…/squad-e/squad-e/E1-advisory-list.md` |
| Is a history model required? | `…/squad-e/squad-e/E2-history-concept-necessity.md` |
| The smallest core change | `…/squad-e/squad-e/E4-minimal-core-change.md` |
| #185 assessment | `…/squad-e/squad-e/E3-issue185-assessment.md` |
| Ledger entry | `…/LEDGER-ENTRY.md` |
| Deferred (out of scope) | `core/docs/architecture/deferred/02-history-tier-floor-deferral.md` |
| Specification | `docs/roadmap/0.1/0.1.7/retained-history-storage.md` |
| Case documents | `core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/` |
| v0.1.6 record | `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` |

**Known harness gaps that will bite you:** the verify phase, the golden table,
`tests/history_declarations.rs`, the runner's lane wiring and the per-lane complete-command ceilings are
**not built**. S1 exists because of this.
