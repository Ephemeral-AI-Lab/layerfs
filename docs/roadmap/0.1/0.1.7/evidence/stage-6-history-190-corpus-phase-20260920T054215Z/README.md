# Declaring the corpus reading, and measuring the corpus axis

> Status: Research; diagnostic evidence, not release admission. Round of
> 2026-09-20 on top of #202 (`4049e28b6`) and the qualification disposition
> (`99028875c`). One sample per case, retained `history-stride10` / `history-stride3`,
> retained source, no instrumentation patch, **no pin written and no budget class
> changed**.

## What was changed in the harness

Three files, +180 / −5 lines, all harness (benchmark) code — no product line:

| File | Change |
|---|---|
| `src/support/phases.rs` | `add_preparation(ns)`: attributes a measured span of harness **input assembly** to the preparation phase, so the four declared phases account for an invocation whose assembly happens between the measured children. `preparation_ns` becomes "close-of-preparation mark + declared spans" and says so. |
| `src/workload/history.rs` | `ReadProbe`: `mincore` over a fresh mapping of each corpus file **before this invocation first reads it**, once per distinct path, published as four resource counters. Diagnostic only: nothing fails on it, nothing is de-warmed. |
| `src/ops/history.rs` | Declares the corpus span with `add_preparation`, begins the probe after preparation closes, and publishes the probe plus the chain's `rusage` `disk_read_bytes` delta. |

The workload is untouched: same corpus, same states, same measured children, same
single construction worker, no cache invalidated, no timeout enlarged, no selection
shrunk. The declaration can only make the budgeted command **larger**.

## Measured

| Quantity | stride10 | stride3 |
|---|---:|---:|
| complete command (wall) | 37.613 s | 75.121 s |
| preparation (window + declared corpus) | 17.766 s | 26.033 s |
| operation (sum of children) | **19.739 s** | **48.976 s** |
| verification / cleanup | 0.016 / 0.000001 s | 0.028 / 0.000001 s |
| declared phases, summed | 37.521 s | 75.038 s |
| **reconciliation** | **PASS** (`phases.compose`) | **PASS** |
| budget, ordinary 15 s | NOT_RUN | NOT_RUN |
| budget, declared exception 25 s | NOT_RUN | NOT_RUN |
| corpus reading, published separately | 17.588 s | 25.463 s |
| corpus files probed before first read | 45,338 (489,820,959 B, 62,319 pages) | 78,439 (850,636,296 B, 107,780 pages) |
| **corpus pages resident before first read** | **0 (0.000%)** | **4,298 (3.988%)** |
| device bytes read across the chain (`rusage`) | 603,807,744 | 977,244,160 |

Retained comparison: the same selections measured 19,888,424,711 / 49,501,013,748 ns
of operation on the retained treatment with the instrumentation patch in both arms;
this build carries the retained source without that patch and measures
19,738,993,418 / 48,976,026,830 ns — within 0.8% / 1.1%, which is the expected
order of run-to-run variation and not a new claim.

Before this change the same phases could not reconcile: `phases.compose` reported
18,103,407,414 ns of the 40,220,718,250 ns wall outside every declared phase for the
retained baseline2 stride10 run, and 23,021,619,251 of 73,127,405,000 for candidate2
stride3. The unexplained remainder is now the child's own non-phase work: 25.8 ms
(stride10) and 46.4 ms (stride3), inside the ~1 s tolerance.

## What the cache diagnostic says, and what it does not

The corpus axis was the undeclared one (see the qualification disposition's
`CACHE-STANCE.md`). It is now measured, at first touch, per file:

* stride10 read 45,338 corpus files — **not one page was resident** when the chain
  first asked for it, and 604 MB came from the device while the corpus files
  themselves total 490 MB.
* stride3, run minutes later on the same machine, found **4,298 of 107,780 pages
  (3.99%) already resident** — the previous run's leavings — and read 977 MB from
  the device for 851 MB of corpus files.

Two independent instruments agree, and the second run shows the hazard is real but
small in this pair: the same corpus, read twice within an hour, is mostly device-
served and slightly warm the second time. The positive control passes: the Store
this run wrote reads back as 3,007/3,011 resident pages through the same
`mincore` method, so a zero here is a measurement and not a broken instrument.

**This is not a cold claim and must not be quoted as one.** Nothing was invalidated,
the residency is whatever the OS chose, the probe covers the files the *chain* read
(the prepare phase's oracle reads sit inside the preparation window and are not
probed), and one run per selection establishes no distribution. It establishes
exactly what the owner needed: the corpus pages this row reads are predominantly
paid for from the device, and the residue an earlier run leaves is measurable
rather than assumed.

## Correction to the qualification disposition, by measurement

`DISPOSITION.md` §2.3/§2.4 projected the declared figures at 36.3 s / 73.0 s and
concluded that stride10 would fit a declared 25 s exception. **Measured, it does
not**: declaring the corpus reading puts stride10 at 37.521 s and stride3 at
75.038 s, both outside the 25 s ceiling and both far outside the ordinary 15 s
limit. The projection's arithmetic was right and its input was wrong — the corpus
reading in this pair is 17.6 s / 25.5 s, not the 16.2 s / 23.0 s of the retained
runs.

So the budget decision (D4) now has exactly three shapes, and only one of them
fits an existing class:

| Option | stride10 | stride3 | Nature |
|---|---:|---:|---|
| (i) new frozen large-history class | 37.5 s | 75.0 s | owner contract change, `benchmark_rules.md` §716-718 contemplates the shape but freezes nothing |
| (ii) prepared, identity-checked corpus input | ~20 s (projected) | ~50 s (projected) | harness work: pack the row's read set once per campaign, then the row reads one artifact instead of 45,338 files. **NOT_MEASURED**; motivated by the measured rate below |
| (iii) diagnostic only | — | — | the status quo; no admission claim |

The measured motivation for (ii): 489,820,959 bytes in 17.588 s is **27.9 MB/s across
45,338 files (2,578 files/s, 388 µs per file)** — a per-file cost, not a bandwidth
cost. A packed artifact read sequentially should not cost that, and packing is
setup reuse of the kind `AGENTS.md` §2 requires ("reuse that removes work outside
the timers is required"). It would also turn the corpus axis into an ordinary
prepared-fixture cache question, which the harness already knows how to declare and
enforce. Its benefit is a projection and is labelled as one.

## Checks

* `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked`:
  **117 tests PASS** on the exact source committed here.
* `cargo +1.85.1 build --release … --locked`: **PASS**; binary SHA256
  `190424195506d4d54b24d253b483986d9aadf542fe125ae61225d833c2102ab1`.
* Harness Clippy: **21 warnings and 1 denied-lint error** (`this loop never actually
  loops`, `src/ops/history.rs:1855` at HEAD) — **all inherited**; none lies in the
  added lines. Recorded, not fixed, not claimed as a pass.
* Harness format: the three touched files are rustfmt-clean; the harness carries
  **121 pre-existing format diffs in other files**, which `cargo fmt` produced when
  run and which were **reverted rather than swept into this commit**
  (`checks/` records the lock-wrapped build, test, and clippy invocations).
* Both resource commands ran under the two global flocks (`with_locks.py`, the
  retained campaign's wrapper with this round's record directory) after the quiet
  preflight. One collector attempt was refused by the child for pre-creating its
  output directory; it consumed no sample and is retained in
  `runs/refused-history-stride10-exit2/`.

## Identities and reproduction

Isolated worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual`, branch
`codex/190-corpus-phase-declaration`, base `99028875c` (the qualification
disposition on top of #202). Rust 1.85.1, Python 3.14.3, `--locked`. Corpus manifest
SHA256 `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip
`b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. All eight behavioral history switches
unset, `LAYERFS_HISTORY_PHASES=1`, `LAYERFS_CONSTRUCTION_WORKERS=1`.

```
python3 with_locks.py perf-history-stride10 python3 collect.py history-stride10
python3 with_locks.py perf-history-stride3  python3 collect.py history-stride3
python3 analyze.py
```

`collect.py` refuses an existing output, runs the quiet preflight, and applies the
established #190 diagnostic caps (120 s / 240 s) — **not** ordinary admission
budgets, and not promoted here. `analyze.py` imports the runner's own
`phases.compose` and `receipt.budget`; it re-implements neither.
