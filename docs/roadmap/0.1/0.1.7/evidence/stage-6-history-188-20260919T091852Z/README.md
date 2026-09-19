# T1 implementation — first receipt (#188)

> **Status:** Partial. S1 (the gate harness) is built and runs; R3 is applied and
> falsified; R1 and R2 are **not** implemented, so the T1 target is **not** met.
> Append-only. Every number here is **diagnostic**, not admission evidence.
> Source pin `66bce8378` + this working tree. Machine shared; **no timing claim**
> is made except where the harness's own budget forces one to be stated.

## 1. What was built

### S1 — the gate harness (critical path, no ruling needed)

| gap (§5 of the handoff) | state |
| --- | --- |
| the verify phase for `history.*` | **built** |
| the runner's lane wiring | **built** |
| phase reconciliation for a driver-declared child sum | **built** |
| the golden table for `history.*` | **not built** |
| `tests/history_declarations.rs` | **not built** |
| per-lane complete-command ceilings | **not built** |
| `space.pack_directory` in the receipt | **not built** |

`ops/history.rs` refused `--phase verify` outright, so **no read-back had ever run
for this lane**. It now reads the Store the measured phase wrote — its path taken
from the trace's own `store.path` record — resolves a **declared sample** of each
state's paths through `FilesystemRead`, and compares presence, kind, size and
digest against the corpus oracle for that state.

The first version of the phase walked every state's whole tree. It was correct and
unusable: 0.36 ms per path-state is 36 s over stride 10's 101,477 path-states, and
reading every file on top of that took the phase past **four minutes**. It now
samples a declared **64 paths per state** — 1,083 of 101,477 path-states — and costs
**6.0 s** in the runner's own measurement.

Because it samples, the row is `INCOMPLETE` and never `PASS`. That is the honest
status: a sampled read-back is not the whole claim.

### R3 — the depth *measurement* (ruling A)

`DepthCache::cost_of` did not count the edge to an already-cached entry, so every
level of a cache-hit walk was recorded one edge short. **Falsified before it was
fixed**: with the driver declaring no base deeper than the policy's own cap of 8,
the unfixed product aborted with `Integrity("dependency chain depth")` — a producer
that respects the declared bound still made the writer build a chain the reader
refuses. With the fix, the same declaration completes.

**Production LOC: 84936 -> 84939 (delta +3)** — 3 non-comment lines, one function,
one file, matching §9's estimate of +4.

## 2. Two defects the read-back found

1. **Symlinks were stored as regular files.** The driver called `construct_bytes`
   for every changed path, so a symlink's target string was stored as a
   `RegularFile` content object. The read-back found it immediately
   (`.claude/skills`, oracle mode `120000`, `UnsupportedFraming` from the
   regular-file read). Fixed in the driver: symlinks are emitted as `Symlink`
   objects. **This is a harness defect, and it was invisible until the phase
   existed.**
2. **The phase reconciliation assumed the wrong invariant.** `shared/phases.py`
   required `operation_ns == timing root`. This lane deliberately publishes the
   **sum of its named children**, because the harness's own corpus reading sits
   inside the root and between the children and is not the product's work (owner
   ruling 2). The driver now publishes `history.children_ns` and the reconciler
   checks the invariant the driver **declares**, falling back to the root check
   when nothing is declared.

## 3. The measurement

`history-stride10`, 17 states, one construction worker, fresh output, faithful
advisory model (`LAYERFS_HISTORY_ADVISORY=1`):

| | apparent (`st_size`) | vs v0.1.6 |
| --- | --: | --: |
| registered lane (no base declared) | 128,864,256 | 2.6130x |
| **this receipt (R0 + R3)** | **63,164,416** | **1.2808x** |
| T1 target | 47,048,435 | 0.9540x |
| v0.1.6 (the gate) | 49,315,840 | 1.000x |

**R0 lands and is confirmed:** 128,864,256 -> 63,164,416 B, a fall of
**65,699,840 B**, against the specification's predicted `-65,126,400 B` — 0.9 %
more than predicted, in the same direction.

**The gate is not met.** The residual to v0.1.6 is **13,848,576 B**, and to the T1
target **16,115,981 B**. R1 (index capacity and lifetime) and R2 (lean row
grammar) are unimplemented, and the arithmetic attributes 16,689,421 B to them.

### The save counters, published for the first time

| counter | value |
| --- | --: |
| `delta.prefix_selected` | 34,405 |
| `delta.prefix_records` | 35,322 |
| `delta.full_records` | 16,710 |
| `delta.no_candidate` | 10,766 |
| `delta.ineligible_candidates` | 571 |
| `delta.work_exceeded` | 19 |
| `delta.absent_candidates` | 0 |
| `history.advisory_bases` | 29,192 |

`no_candidate` falls 26,847 -> 10,766, and `prefix_selected` rises 18,344 ->
34,405. The remaining 10,766 are the R1 opportunity: objects the caller still
offers no base for.

### Where the time goes, measured

| component | ns | note |
| --- | --: | --- |
| `build`/`update_filesystem` | 25.67 s | product work, scales with tree size |
| `begin_save + accept + finish` | 8.68 s | product work |
| harness input assembly | 0.09 s | inside the child, untimed |
| harness corpus reading | 17.93 s | between the children, untimed, not the product's |
| verification invocation | 6.03 s | the runner's own measurement |

Complete command **58.3 s**; the 15 s ceiling is **exceeded** and recorded
`NOT_RUN`, never shrunk to fit.

## 4. Checks run, and checks not run

**Run:** `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` — all
suites pass, 0 failures (300+ tests across `layerfs-content`, `layerfs-storage`,
`layerfs-telemetry`). `shared/test_phases.py` — 2 passed.

**Not run, with reason:**

- `cargo clippy` / `cargo fmt --check` — not run; `fmt --check` is not clean on
  this tree by owner decision and no file outside the change was reformatted.
- The 217-row lane — untouched by construction; `--lane full` remains 220 rows.
- `history-stride3` and `history-stride1` — **not run.** The guardrail requires a
  stride3 confirmation, and none exists.
- The golden table, `tests/history_declarations.rs`, per-lane ceilings and
  `space.pack_directory` — **not built**, as listed in §1.
- R1 and R2 — **not implemented**; no ruling was recorded for them in this
  receipt, so no line of product code was written for either.

---

# Addendum — R1a, R2, and the producer, measured

## The waterfall, every step measured

| step | apparent | delta | what it was |
| --- | --: | --: | --- |
| registered lane | 128,864,256 | — | the driver declared no base |
| + R0 (declaration) + R3 (depth fix) | 63,164,416 | −65,699,840 | R0 lands; predicted −65,126,400 |
| + **R1a** (`SLOTS` 1,024→32,768, `REFERENCES` 8,192→65,536, `INDEX_BYTES` 128 KiB→4 MiB) | 63,135,744 | **−28,672** | **falsified as the lever** |
| + **R2** (both secondary indexes removed) | 57,749,504 | **−5,386,240** | measured, not estimated |
| v0.1.6 gate | 49,315,840 | | **residual +8,433,664, ratio 1.1710x** |

### R1a — the falsifier fired

§4.1 asked whether capacity was binding. It was not: widening the index from
1,024 to 32,768 slots bought **28,672 B** and 34 objects. The reason is structural,
not a tuning failure: `Candidates` is owned by **one save operation**
(`cas/lifecycle.rs`), so it can only ever propose bases from the objects that same
save admitted — and the caller already declares a base for 34,405 of them. The
index is not the missing coverage; the gap is **across** saves, which an in-save
index cannot reach at any size. **R1b (persisted) is therefore the only remaining
form of R1**, and it needs ruling B (new table, `SCHEMA_VERSION` 4→5).

### R2 — the certain win, and it is 90 % of what §3.5 predicted

`EXPLAIN QUERY PLAN` decided it, not the prose:

- `objects_bases` — **no query consumer at all.** The only statement that reads a
  base id filters on `object_id`, the primary key. `REQUIRED_INDEXES` listed it;
  nothing used it.
- `objects_locations` — one consumer, the failed-save cleanup's page query
  (`sqlite/cleanup.rs`), which seeks `pack_id > baseline` and orders by the same
  three columns. Removing it costs that query a scan; it runs only after a failed
  save.

Predicted cost 2,805,760 + 2,547,712 = 5,353,472 B; **measured 5,386,240 B**.
`REQUIRED_INDEXES` is now empty. A Store that still carries the indexes validates —
the requirement is a floor, not an equality — so **`SCHEMA_VERSION` stays at 4 and
no existing Store is invalidated.**

The `base_object_id` **column** (~614,400 B) was **not** removed: the delta and
pooled readers walk chains from it (`encoding/delta/read.rs:164`,
`encoding/pool/read.rs:233`), so removing it means decoding the base out of each
packed record instead. That is a real refactor, not a column drop, and it is not
in this receipt.

## The full-history producer — a negative result, retained

A producer that runs at **every** commit holds checkpoint *k−1* when it saves
checkpoint *k*. The selection skips checkpoints, so 9,702 whole-file objects arrive
with **no version anywhere in the Store** while such a producer would have had one.
That is the Stage 7 correspondence, and the harness can supply it.

It was built (`LAYERFS_HISTORY_FULL_PRODUCER=1`, harness-only, no product line) and
it is **worse**:

| | bases | `prefix_selected` | `ineligible_candidates` | apparent |
| --- | --: | --: | --: | --: |
| selection-only producer | 29,192 | 34,405 | 572 | 57,749,504 |
| **full-history producer** | **30,959** | **36,057** | **2,293** | **59,039,744** |

**More bases, more delta records, 1,290,240 B larger.** The extra bases are versions
from adjacent checkpoints, which are *more* similar to each other than the selection's
spaced versions are: they push chains past the depth cap, and `ineligible_candidates`
quadruples. `delta.full_records` also rises (16,710 → 16,747) because a deeper chain
costs more to read back than the base saves.

The switch is kept so the result is reproducible. **It is not the default.**

## Why the gate does not close — the structural finding

v0.1.6's own Store, dbstat: `object_packs` 46,505,984 and `objects` 2,650,112 over
**51,722 objects**, against this receipt's `object_packs` 53,374,976 and `objects`
4,124,672 over 53,721 objects.

Two differences, and neither is available to T1:

1. **v0.1.6 ran its producer at every commit.** Its Store holds the intermediate
   versions, so its delta bases are the *nearest* version of each path. T1's
   selection holds only the 17 selected states, so the base for a file that changed
   inside a skipped span is up to nine checkpoints stale. Modelling the richer pool
   made things **worse**, which is the measurement above.
2. **v0.1.6's Store is a different object.** It carries `layers`, `layer_stacks`,
   `branches`, `commits` and `workspace_stages` — the LayerStack layer that
   `core/` does not build — and its `objects` table is 2,650,112 B for *more*
   objects than this Store holds in 4,124,672 B.

So the residual is not a tuning gap. It is the absence of the layer that R0's
producer lives in, which is **Stage 7 (#172), unbuilt**, and which §4.3 names as the
critical-path blocker.

## Status

**T1 does not meet its gate.** Measured best: **57,749,504 B = 1.1710x v0.1.6**,
residual **+8,433,664 B**. The levers that remain are R1b (persisted, ruling B) and
the `base_object_id` column, and the evidence above is that neither reaches 8.4 MB.
**#188 stays open.**

Verification: **4.01 s**, 1,083 of 86,064 declared units, every sampled path matching.
Complete command **41.0 s**. Product LOC: **84936 → 84943 (delta +7)** — R1a 3 lines of
constants, R2 1 line of Rust (`REQUIRED_INDEXES`; the `schema.sql` change is a
**deletion** of two index definitions plus a comment, so it adds no production
line), R3 3 lines. Counted as non-blank, non-comment first-party product lines over
the same scope, including required runtime SQL.

---

# Addendum 2 — the four-slot ordered declaration, built and falsified

§3.3 and the T1 specification both say R0's declaration should carry **four**
candidates in a measured order — `cross-path best, cross-path 2nd, same-path
previous, same-path earlier` — because `select.rs:342` returns the **first
eligible** one. This receipt's driver declared exactly **one**. That was the
largest untested lever, and it has now been tested.

## What was built

`SimilarityIndex`, harness-only, **0 product lines**: a cross-save content-keyed
index using the product's **own** `candidates::signature` (8 mixed rolling hashes,
`>= 2`-of-8 match, the product's rule). It is what the product's per-save
`Candidates` cannot be at any size, and it is the caller R0 is missing. The
declaration then pushes up to four identities in the measured order.

Two arms were run, one variable apart — the list.

## Result: it is worse, twice

| arm | apparent | whole-file lane | `prefix_selected` | `full_records` | `ineligible` |
| --- | --: | --: | --: | --: | --: |
| **one base (default)** | **57,749,504** | **43,873,480** | 34,439 | 16,676 | 572 |
| four slots, measured order | 68,157,440 | 54,115,790 | 38,223 | 12,892 | 1,563 |
| four slots, depth-ranked first | 63,414,272 | 49,358,024 | 38,229 | 12,886 | 1,116 |

**The counters improve and the bytes get worse.** The four-slot arms declare 3,784
more bases, store 3,784 fewer FULL records, and are **10.4 MB / 5.7 MB larger**.
Base *coverage* rose; base *quality* fell. The whole-file lane grew from
43,873,480 to 49,358,024 B while covering more objects.

Depth-ranking the candidates — preferring a shallow base to a deep one, which is
the spec's own diagnosis of why "try all four and keep the smallest" fails —
recovers **4,743,168 B** and still loses by **5,664,768 B**. So depth is *part* of
the mechanism and not all of it.

## What this says about the §3.3 measurement

§3.3 measured the ordering on a **synthetic whole-file canonical set** of
348,460,709 B with the product's byte-exact codec, and found cross-path-first
better. That measurement is not wrong about its own set; it does not transfer to
**this** Store. The likely reason is the match rule: a `>= 2`-of-8 signature
overlap is a weak similarity test, and on this corpus a cross-path object that
shares two hashes is a much worse predictor than the same path's own previous
version. The spec's set was built to make the cross-path case well-conditioned;
stride-10's real content is not.

**The lesson is the campaign's own**: a measured ordering on a synthetic set is a
hypothesis about a real Store, and this one is falsified.

## Status after the experiment

The default is the single same-path previous version, **confirmed at exactly
57,749,504 B** on a re-run. The ordered arm is kept behind
`LAYERFS_HISTORY_ORDERED_PREDECESSORS=1` so the negative result reproduces.

**T1 remains BLOCKED on its gate: 57,749,504 B = 1.1710x v0.1.6, residual
+8,433,664 B.** The one lever the specification named as decisive has now been
built and does not close it. #188 stays open.
