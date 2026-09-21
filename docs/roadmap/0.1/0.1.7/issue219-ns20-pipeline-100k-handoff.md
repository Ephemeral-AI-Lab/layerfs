# Handoff: build the `pipeline-namespace-100000` row

> **Status:** handoff and commission. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> after round 20 (ledger L80–L82). **It makes no performance claim of its own.** Every number is
> sourced to a receipt, a counter or a `file:line`, and the ones that are inferences are labelled.
>
> **Owner ruling, 2026-09-21: build the rung.** Round 20's step 3 decided *not* to
> ([`issue219-ns20-scaling-decision.md`](issue219-ns20-scaling-decision.md) §3); the owner has taken the
> decision back and commissions the row. That page carries the ruling and its conditions, and two of
> those conditions are still unmet — §5 below is what to do about the one that matters.

**What this document is for.** It gives the next agent (a) the surfaces the row costs, priced by
reading rather than by assuming, (b) the **two hard constraints** that make the obvious declaration
wrong, (c) the one modelling decision that needs an owner ruling, (d) the session anchor that must
exist before the first timed run, and (e) the four things already answered that must not be
re-derived. It does **not** pre-authorise a product change: the row is a harness round.

---

## 1. What is commissioned, and what is not

**Commissioned:** one new registered row, `pipeline-namespace-100000`, in the core harness's
`pipeline.*` group — the `pipeline-namespace-10000` shape at ten times the entries — plus its tier
entry, its configuration, its pins and one measured run.

**Not commissioned:** any change to `core/crates/**`. Round 20's only product change was measured and
reverted (`8efc798de`); the product tree is byte-identical to the handoff head `b7a0ab0a2` for
`src/` and `sql/`. If the row shows a product problem, that is a **finding** and the next round's
subject, not this round's edit.

**Not commissioned either:** re-running the four `c1.fs.build-scale` rungs (§6), re-opening the pack
capacity lever (§6), or re-deriving the v0.1.6 premise (§6).

---

## 2. Where the row it extends stands, all of it measured

Control arm: `pipeline-namespace-10000`, source `b7a0ab0a2`, product source identical to round 19's
`e3a46d74b`, one sample, `--verify full`, **PASS 13/13**.

| | this session (`ns20-P2a-control-20260921T115000Z`) | round 19's session (`ns19-S1-indexdrop-20260921T103500Z`) |
| --- | ---: | ---: |
| `pipeline.operation_work_ns` — the declared figure | **943,318,416** | **1,126,731,417** |
| `pipeline.accept_span_ns` — the inclusive closure | **1,046,366,750** | 1,208,264,083 |
| `pipeline.establishment_ns` | 2,705,583 | 2,251,208 |
| `pipeline.teardown_ns` (excluded from the figure) | 103,048,334 | 81,532,666 |
| complete command | 1,658,319,583 | 2,017,207,209 |
| entries / directories / declared content | 10,000 / 100 / 300,000,000 B | same |
| `pipeline.packs_created` | 1,270 | 1,270 |
| `pipeline.commits` / `pipeline.statements` | 95 / 7,666 | 95 / 7,666 |

**Those two columns are 183,412,999 ns apart with the same product, the same harness driver and the
same case** — measured, not modelled (L80, pair report). That is the whole reason §5 exists, and it is
larger than every lever this campaign has priced (20–90 ms).

`establishment_ns + operation_work_ns + teardown_ns` is exactly the runner's `phases.operation_ns`
(1,049,072,333 against 1,049,102,250), so both boundaries are reconstructible from the row.

---

## 3. The two hard constraints, both verified by reading

### 3a. The row must declare **1,000 directories**, not 100

`namespace_content.rs:264` maps a position to a serial as

```text
index = directory * FILES_PER_DIRECTORY + ordinal      (ordinal = position / directories)
```

with `FILES_PER_DIRECTORY = 100` a **const** (`namespace_content.rs:35`) that `pipeline.rs:674` also
uses to re-derive the noise index. At `entries = 100,000` with `directories = 100`, `ordinal` runs to
999 and the index space wraps: **position 10,000 reuses index 100**, so 100,000 files collapse onto
10,000 distinct serials and the fixture's file serials collide. Worked, not asserted:

| entries | directories | distinct indices | verdict |
| ---: | ---: | ---: | --- |
| 10,000 | 100 | 10,000 of 10,000 | distinct — today's row |
| 100,000 | 100 | 10,000 of 100,000 | **collides at position 10,000** |
| 100,000 | **1,000** | 100,000 of 100,000 | **distinct** |

`directories = 1,000` also matches the C1 ladder's own 100,000-entry rung (`100,000 / 500 MB / 1,000`,
`c1_fs_build.rs:20`), so the two halves of this shape agree. **Do not change `FILES_PER_DIRECTORY` to
make 100 directories work**: it is read at two sites that must move together, and 1,000 directories is
the declaration the ladder already uses.

### 3b. The declared total has a **floor of 100,099,899 bytes**

`namespace_content.rs:222-225`: `distributable = total_bytes − anchor_bytes − positive`, where
`positive` counts every non-empty, non-anchor slot and `anchor_bytes = min(ANCHOR_BYTES, total_bytes)`
with `ANCHOR_BYTES = 100,000,000`. At 100,000 entries the declared classes contribute 100 empty files
and 1 anchor, so **`positive = 99,899`** and

```text
total_bytes >= 100,000,000 + 99,899 = 100,099,899
```

A smaller total is refused with `"namespace byte budget"`. Any declaration above that floor is
accepted and the bytes are distributed by the declared weights.

---

## 4. The one modelling decision that needs an owner ruling

`plan` distributes entries over the declared bands and, for anything beyond them, says so itself
(`namespace_content.rs:153-155`):

> *"Bands: the declared counts, then any surplus as more tiny files, so a larger `entries` degrades
> into a bigger tiny band rather than a different shape."*

The declared classes are `EMPTY 100`, `TINY 7,899` (1..8 B), `SMALL 1,500` (32..256 B),
`MEDIUM 500` (1,024..8,192 B) and `ANCHOR 1` — 10,000 in total. So at 100,000 entries **90,000 files
are surplus and every one of them lands in the tiny band**, whatever total is declared. Two coherent
declarations follow, and they measure different things:

| | declared total | what the row then measures |
| --- | ---: | --- |
| **A — same content, ten times the entries** | 300,000,000 | entry scaling: 10× the inodes, 10× the rows, the **same** bytes, with ~90 % of files tiny (≈300 B each after distribution) |
| **B — ten times the entries *and* the bytes** | ~600,000,000 | both axes; closer to the reference harness's `300 MB + 100 MB anchor` shape, and the one this handoff's own arithmetic below assumes |

**This document does not choose.** It is a declaration about what the row is evidence for, and round
20 was told in writing that a handoff "does not pre-authorise a case change". **Get the ruling before
the run and record it in the pre-registration**, because it changes the pins, the predicted bytes and
what any comparison to the 10k row means.

---

## 5. The session anchor — do this before the first timed run

Round 20 measured that two rows of **the same product, driver and case** differ by 183 ms across five
hours of session drift, while every lever priced in L67–L80 is 20–90 ms. A 100k row measured without a
same-session control produces a number that cannot be compared with the 10k row it extends — which is
the only comparison the row exists to make.

**The anchor is free and already registered:** `namespace-10000` in `c1.fs.build-scale`. It builds a
tree and writes **no Store at all**, so no product lever can reach it, and it is already the row whose
pack-free work moved 26.8 % between round 20's two sessions.

| anchor, `ns20-L1-10000-20260921T113500Z` | value |
| --- | ---: |
| `phases.operation_ns` | 68,514,625 |
| `phases.preparation_ns` | 13,852,625 |
| complete command | 94,736,583 |
| `fs_build.operations` / `fs_build.bindings_added` | 3 / 10,100 |

Run it **in the same session, under the same lock window, immediately before or after** the new row,
and publish its `operation_ns` beside the new row's counters. A change to `runner.py` that records the
anchor on the row is the clean way to do it and is a harness change, not a product one. **If the
anchor does not land near 68.5 ms, the session is not comparable and the run should be reported as
such rather than used.**

---

## 6. The price, by reading — every surface the row touches

| surface | `file:line` | what it needs |
| --- | --- | --- |
| the op variant | `registry.rs:359-373` (`PipelineOp::NamespaceScale`) | a second variant, e.g. `NamespaceScaleLarge` |
| its configuration | `pipeline.rs:153-162` | an arm with `files: 100_000, directories: 1_000` — **§3a** |
| the declared total | `pipeline.rs:86` (`NAMESPACE_SCALE_BYTES = 300_000_000`) | a second total, or a per-op total — **§3b** and **§4** |
| the driver | `pipeline.rs:624` (`fn namespace_scale`) | reused as it stands; it takes the batched route automatically at this size |
| the dispatch | `pipeline.rs:173-177` (`run`) | an arm |
| the family entry | `families/pipeline.rs:18-45` (the `(op, id)` list) and `:47-49` (`entry_tier(2, 10_000, "binary")`) | a sixth row, `entry_tier(3, 100_000, "binary")` — tier 3, matching the C1 ladder's `100000` rung |
| **the registry count** | `registry.rs:24` (`FROZEN_CARDINALITY`, last entry `4, // pipeline: 4`) and `:31` (`ADMISSION_CASES`) | **4 → 5 → 6 and 218 → 219.** This is the one-line defect filed in L80: it is *already* wrong (the family registers 5 rows against a frozen 4) and `runner.py self-check` fails on it today. You are changing the count anyway — **fix it in this round** |
| the golden tables | `tests/golden/registry.tsv`, `tests/golden/expected.tsv` | regenerate; the registry row is the declaration of record |
| pins | `expected.tsv` — bootstrapped from a `PASS` run (`shared/pin_expected.py` refuses anything else) | see §7 |

**No prepared master.** The registry row is `prepared = -` — `Preparation::InProcess`, the default
`CaseSpec::new` sets (`families/mod.rs:134`) and which the pipeline family never overrides — so the
fixture is built inside the row's own invocation (`pipeline.rs:631-637`) and `runner.py` acquires no
artifact and passes no `--load-input`. **A handoff that says this row needs "a new
prepared artifact" is wrong** and is corrected in the decision page; do not add one.

**No product change, no schema change, no new dependency.**

### What it should cost, as arithmetic to start from — not a registered prediction

The running agent registers its own prediction before the first run. This is the arithmetic, from
measured values:

- **untimed construction** scales with the **file count**: `construct_ns` 322,013,388 +
  `construct_noise_ns` 82,769,536 = 404,782,924 ns at 10,000 files → ≈ **4.05 s** at 100,000, and
  `preparation_ns` was 509,974,958 → ≈ **5.1 s**;
- **the timer's C1 half** is measured at this exact size: 25 operations,
  `phases.operation_ns` **665,695,208 ns** (`ns20-L1-100000-20260921T113500Z`);
- **the timer's save half** was 1,046,366,750 − 67,551,417 = 978,815,333 ns at 300 MB; under
  declaration A it stays there, under B it scales with the bytes;
- so the **complete command** lands near **6 s** under either declaration, inside the 15 s limit, and
  `pipeline-namespace-100000` is **not** in `runner.py`'s `DECLARED_EXCEPTIONS`.

Register the ratio, not a point: *if the save's cost per canonical byte is flat, the 100k row's save is
the 10k row's save times the byte ratio* — 1.0× under declaration A, ≈1.7× under B.

---

## 7. What the row must publish, and the pins

**Publish both boundaries.** `pipeline.operation_work_ns` **and** `pipeline.accept_span_ns`, with
`establishment_ns` and `teardown_ns`, so a reader can reconstruct the inclusive closure. Round 20
measured an arm that read as a 197 ms win on the declared figure and **+78.86 ms** on the closure,
because the figure subtracts the save's connection close (`pipeline.rs:1260-1263`). Do not publish the
new row without both.

**Publish the session anchor** from §5, and `construct_ns` / `construct_noise_ns` — they are the
pack-free fingerprint already on the 10k row.

**Pins.** Bootstrap from the new row's own `PASS` run. Decide deliberately whether to pin
`pipeline.packs_created` and `pipeline.pack_appends`: they are **published but not pinned** on the 10k
row today, and L80 measured that they move only with packing (1,270/6,603 at 256 KiB packs vs
295/7,578 at 1 MiB). Pinning them would have caught round 20's arm; not pinning them keeps the row's
pins to work that no policy constant can move. Either is defensible — say which and why.

---

## 8. What must not be redone

| closed | why |
| --- | --- |
| **the pack-capacity lever** | L80: `PACK_LIMIT` 1 MiB was measured on a matched pair and **reverted** (`8efc798de`). The page accounting fired exactly (295 packs, 309,329,920 B, −5,825 pages, commit −49,659,579) and the row effect was **negative** (+78.86 ms on the closure, +35.95 ms flush-shaped). Re-opening it needs §9's instrument first |
| **the C1 ladder** | L81: 100/1k/10k/100k measured, four `PASS`, cost per binding flat at −2.8 % from 10k to 100k. Do not re-run the rungs; reuse them |
| **the v0.1.6 premise and the case mapping** | L79 and the decision page: the row is ahead of its fastest comparable reference row on the only shared boundary, and the bar's case is a **different case in a different harness** |
| **the four corrections** | L79: two denominators in circulation; "v0.1.6 did 700 MB/s" unsupported; the fixture identical across the six reference rows; the bar's case ≠ this campaign's row |
| **codec level, stored frames, page size, cache-in-pages, journal modes** | refuted in L67/L72; the page size is owner-ruled at 4 KiB |
| **transaction cadence, more workers, whole-file lane granularity, ordinal reservations, the locator's second B-tree** | L73/L74/L77 closed them |

---

## 9. The two open questions this row does not answer, and should not be asked to

1. **Why a connection close with fewer pages to write costs 85.6 ms more** — `NOT_MEASURED` (L80). The
   round-19 instrument `core/benchmark/fs-bench-pro-storage-content/tests/commit_page_price.rs`
   already reads `CACHE_WRITE`, `CACHE_SPILL`, `CACHE_USED`, `page_count` and `freelist_count`, and
   does not read them **across a close**. That extension is the cheapest next instrument in this
   campaign and it decides whether the whole "shrink the reserved pages" family is dead or alive.
2. **Whether the row should exclude the save's connection close at all.** The exclusion is
   pre-existing (`a35d9aa3a`) and declared; round 20 is the first arm to exploit it. Either the close
   comes inside `operation_work_ns` — every row's number shifts and the bar needs re-reading — or
   every arm publishes the inclusive closure beside it. **The second is cheaper and is what §7 asks
   for.**

---

## 10. Housekeeping

- Branch `codex/219-ns10000`; round 20 is pushed at `89c7d61b9` and the tree is **clean**. Push before
  filing anything that cites a hash.
- The measurement lock is **per worktree**
  (`core/benchmark/fs-bench-pro-storage-content/.measurement.lock`). Never interrupt another owner's
  run.
- Harness receipts are **not** tracked by git (`benchmark-results` is in `.git/info/exclude`), so any
  receipt a report depends on must be copied into the evidence directory or it will not survive. Round
  20's receipts are under `evidence/issue219-ns20-*/raw/` as the pattern to follow.
- `AGENTS.md` §3: one sample per case per arm, fresh `--out` per run, `--verify full` for anything
  called evidence, never retune a receipt, report FAIL and `INCOMPLETE` as plainly as `PASS`.
- The harness **rebuilds by default**; `--no-build` is for iteration only, and a rebuilt binary
  invalidates its pair.
- Every commit reports production LOC (`tools/production_loc.py --root <tree>`). A harness-only commit
  reports the unchanged total and delta 0; say so rather than inventing a zero-sized product.
