# Stage 6, round 5 — the honest numbers, and what they cost

> **Status:** Round-5 closure receipt for [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Append-only. It supersedes nothing and edits no earlier receipt; the round-3, round-3b,
> round-4, round-4b and round-4c directories are the historical record and are untouched.
> Stage 6's acceptance issue [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> is **closed** and was not reopened. Round 5 changes no verdict: every budget already
> passed, and it still does.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **263.776 s** |
| Source commit | `0ef2bed6ee8cf4838f861280bfbba6cab26261cd`, **clean tree** |
| Harness binary sha256 | `fb967c78d304e6edbb423c4205882694c09da57c3e6728b6b47eba06f6a583b2` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `5186d2f5e8574489ae91fe538c2e039f07406e03c558981e9ccceb1a82e685da` |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Verification mode | `full` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

Verification: **0 disagreements** across all 220 re-derived statuses; sealed call-graph
**PASS** over **120** product source files; runtime tripwires **PASS** over **159** stores;
verification budget `PASS` at **1.94 s** against the 60 s limit. Calibration: E1 `REFUTED`,
E2/E3/E4 `SATISFIED`, W1/W2/W4 `SATISFIED` — the same six outcomes round 4c recorded.

**A note on the tree.** An unrelated documentation change (a new
`core/docs/architecture/deferred/` paper and its index entry, plus
`core/docs/benchmark/.../history-storage-optimization/`) was present in the working tree
and is not this round's work. It was set aside for the duration of the run so the seal is
clean, and restored immediately afterwards. It is documentation: it is outside
`core/benchmark/**`, it is not compiled, and it does not move the harness binary hash the
receipt names.

## 2. The four phases, published for the first time

`benchmark_rules.md` §6 requires setup, performance, verification and cleanup to use
separate timing and resource scopes. Until round 5 the harness published only two of them,
and it published the second as the *complete-command wall*: preparation and verification
were billed to a performance budget, and the report's time column was not operation time
at all. This is the measurement that replaces the ~180 s / ~180 s **[D]** assumption the
round-5 plan was written against.

| phase | field | admission total | max per row |
| --- | --- | ---: | ---: |
| a preparation | `preparation_wall_ns` | 98.226 s | 5.448 s |
| — of which per-sample copy + de-warm | `acquisition_wall_ns` | 0.066 s | 0.011 s |
| b **real work** | **`operation_ns`** | **76.184 s** | 8.862 s |
| c verification | `verification_wall_ns` | 85.513 s | 7.636 s |
| d cleanup | `cleanup_wall_ns` | 0.000077 s | 5,917 ns |
| | harness work inside the timer | `handoff_ns` | 1.127 s | 0.115 s |
| | process wall | `complete_command_ns` | 260.128 s | 11.471 s |

**`sum(operation_ns) = 76.184 s` is the golden benchmark number**, published for the first
time in this round. The report's primary axis is now `operation_ns`, labelled
`operation max ms`, with the wall and the non-operation share beside it.

The four phases reconcile with the wall on **every** row — `unreconciled_rows: []` in
`run-full.json` — inside a declared tolerance of 250 ms plus 2% of the wall, covering
process start, the trace header, gate assembly and process teardown.

### 2.1 The before/after non-operation share

Round 4c published no operation time at all, so the only like-for-like comparison is over
the rows that publish one in **both** runs: **161 rows**.

| row set | run | wall | `sum(operation_ns)` | non-operation | share |
| --- | --- | ---: | ---: | ---: | ---: |
| 161 common rows | round 4c | 245.867 s | 63.497 s | 182.369 s | **74.2%** |
| 161 common rows | **round 5** | 239.998 s | 66.673 s | 173.325 s | **72.2%** |
| whole lane (sum of per-row complete commands) | round 3b (150 rows publish) | 394.57 s | 39.151 s | 355.42 s | **90.1%** |
| whole lane (sum of per-row complete commands) | **round 5 (217 rows publish)** | 260.141 s | 76.184 s | 183.956 s | **70.7%** |

The two wall figures are different measurements and are both published: **263.776 s** is
`run.json`'s wall for the whole `perf` invocation (identity, registry listing, golden check
and per-case bookkeeping included), and **260.141 s** is the sum of the rows' own
`complete_command_ns`. Round 4c's lane wall was **415.821 s**, so round 5's is **−36.6%**.
The operation total over the common rows **rose 5.0%**, which is the direction T4 requires.

## 3. The falsifier: `operation_ns` did not fall

T4 is the round's most important guard: *if the golden number improves, measured work has
moved into setup and the change is rejected.* Measured over the 161 rows that publish a
`timing.json` in both runs:

```text
round 4c  sum(operation_ns) = 63.497 s
round 5   sum(operation_ns) = 66.673 s   (+5.00%)
```

**No row's operation time fell for a structural reason.** Thirty-five rows rose beyond the
±17.6% same-binary spread and nine fell, and every one of the nine is a sub-10 ms operation
whose absolute change is at most 5.9 ms (`lifecycle-create`, 8.8 ms → 2.9 ms): at that
magnitude the reading is scheduling noise, not a moved timer. Their preparation spans are
unchanged and their counters are byte-identical, so no work left the measured region — the
falsifier's purpose is to catch a *large* operation's work moving into setup, and the 35
rises plus 0 counter changes are the evidence that none did.

The 56 rows that publish no `timing.json` in round 4c now publish one; their operation
times were never part of round 4c's total and are added, not compared.

## 4. Every counter identical to round 4c

`shared/compare_runs.py` compares the two runs row by row, counter by counter
(`compare-round4c-vs-round5.txt`):

```text
compare: 220 -> 220 rows, {'instrumentation': 68, 'added': 50, 'resource-drift': 16}, verdict IDENTICAL
```

| class | rows | meaning |
| --- | ---: | --- |
| `instrumentation` | 68 | `timing_json_bytes` — the size of the harness's own `timing.json`, which moves when the tree gains a node |
| `added` | 50 | counters round 4c never published (`verify.units`, `verify.sampled`, `reuse.distinct_readbacks`, the 41 new identity digests) |
| `resource-drift` | 16 | `heap.peak_incremental_bytes` (2–18 bytes) and `space.allocated_bytes` (0.2–2.7%) |
| `counter-drift` | **0** | — |
| `status` | **0** | — |
| `missing` | **0** | — |

**Zero product counters moved.** The 16 resource readings are measurements of the machine
and of the harness's own allocation inside the window, not work counters; the two
`space.allocated_bytes` rows that moved most are the `c2.footprint` rows, whose whole
subject is allocated bytes, and their G3 band is unaffected. `heap.peak_incremental_bytes`
moved by 2–18 bytes on five rows: the counting allocator is deterministic for a fixed
input, and a different binary can put a different temporary at the peak. No gate band moved.

## 5. What changed, and what it bought

| # | item | state | measured |
| --- | --- | --- | --- |
| V1 | the four phases, six published fields, `verify` re-derives and fails closed | **done** | 220/220 rows reconcile; 56 rows publish an operation time for the first time |
| V2 | O1 as a pinned expected identity, not replay self-consistency | **done** | 1,969 pinned constants; O1 pinned for 212 of 217 admission rows |
| V3 | O3 counts gated against pinned values | **done** | 217 of 217 admission rows pin at least one counter; coverage asserted by the registry self-check |
| V4 | the unrequired read-back removed from the four families whose oracle does not ask for O2 | **done** | `c2.delta.cdc-locality` verification 12.1 s → 19 ms per 500-tier row |
| V5 | where O2 is required, each distinct `(root, expectation)` pair verified once | **done** | `c2.reuse.cross-file` `identical` profile: 128 read-backs → 1, complete rather than sampled |
| V6 | expectation digests persisted for every family | **partial** | the delta family persists them in the artifact; the others still call `Expectation::of` |
| V7 | `--reuse-pass` | **done** | `verify --reuse-pass` **0.02 s**; `perf --reuse-pass` skips the deferred invocation; both fail closed on schema, identity, hard limit and wall |
| V8 | the mode ladder `full` / `sample` / `none` | **done, slow** | every row in a non-`full` mode is `INCOMPLETE`; the quick lane is 254.6 s, not ≤70 s (see §7) |
| V9 | prepared masters for the remaining fixture-heavy families | **not done** | 34 rows still exceed the 1.0 s preparation target |
| V10 | pack the object set into one file | **not done** | — |
| V11 | remove the per-member base clone and the double allocation | **not done** | — |
| V12 | the digest key is the product identity plus the recipe version | **done** | `prepare` re-keyed; 20 stale masters superseded; all 20 rows report `reused` with `producer_is_current_binary: true` |
| V13 | remove the per-object handoff, or publish `handoff_ns` | **done** (published) | 1.127 s of the 76.184 s golden number, visible rather than absorbed |

### 5.1 The pinned constants

`tests/golden/expected.tsv` holds **1,969** rows: one `counter:` constant per published
counter of every admission case (**1,757**), plus one `digest:` identity per row (**212** of
217). The counters were taken from the **round-4c full lane** at `90bbb617d` — the last tree
on which all 217 admission rows passed with 0 `FAIL` — which is what turns *"every counter
identical to round 4c"* from a comparison a reader performs into a gate the row must pass.
The identity digests were first published in round 5 (round 4c recorded no roots), so they
come from harvest runs of this tree; the closure run then had to reproduce them, which is
the only thing that makes them evidence rather than a transcription.

The table is `include_str!`-embedded, so the `harness_binary_sha256` every receipt names
covers the constants that ran. Five admission rows pin no identity because their frozen
oracle is O7 (`c2.lifecycle`: schema identity, four tables, two indexes, watermark,
`quick_check`) and they have no single root to pin.

## 6. Targets: what was met, and what was not

Baseline: round 4c, **415.821 s** lane, 217 of 217 admission rows `PASS`.

| line | round-4c | target | round-5 measured | verdict |
| --- | ---: | ---: | ---: | --- |
| per-row preparation | 0.03–11.95 s | ≤ 1.0 s | 0.0002–**5.448 s** | **NOT MET** — 34 of 217 rows over |
| lane preparation | ~180 s [D] | ≤ 25 s | **98.226 s** | **NOT MET** |
| `prepare --lane full`, once per digest | ~180 s+ [D] | ≤ 90 s | **75.39 s** (20 masters, 73.835 s acquisition) | **MET** |
| largest single verification invocation | 12.7 s [M] | ≤ 5 s | **7.636 s** | **NOT MET** (12.7 → 7.6) |
| lane verification | ~180 s [D] | ≤ 108 s | **85.513 s** | **MET** |
| per-row cleanup | not measured | ≤ 0.5 s | **5,917 ns** max | **MET** |
| lane, warm, full verification | 415.821 s | ≤ 200 s | **263.776 s** | **NOT MET** (−36.6%) |
| lane, warm, quick mode | — | ≤ 70 s | **254.6 s** | **NOT MET** |
| **operation (golden)** | unknown | published, must not fall | **76.184 s**, +5.0% on the common rows | **MET** |
| budgets | max 9.952 s | keep passing | max **11.471 s**, all `PASS` | **MET**, no tier shrunk |

The two structural reasons for the misses, stated rather than implied:

1. **Preparation is 98.2 s of the 263.8 s lane, and it is the same work as before.** V9–V11
   — prepared masters for the remaining fixture-heavy families — were not implemented. The
   cost is concentrated and identifiable: `c1.edit.length-changing` 29.7 s,
   `c2.reuse.workspace` 12.0 s, `c1.cdc.chunk-count` 11.5 s, `c1.edit.length-preserving`
   11.5 s, `c1.fs.build-scale` 10.0 s, `c2.read.waves` 6.5 s — 81 s of the 98.2 s in six
   families that build a base object set from recipe bytes inside their one invocation. The
   delta family, which round 4 already gave a prepared master, spends **0.19 s per row**;
   that is the number the other six would reach.
2. **The quick mode does not omit the oracles that matter.** `--verify none` skips the
   *deferred* verification invocation, which only `c2.delta.cdc-locality` has: 254.6 s
   against 263.8 s. For the other 200 admission rows the oracle is a second, unmeasured,
   byte-identical operation **inside** the performance invocation, and omitting it needs a
   driver change, not a runner flag. The ladder is implemented and fails closed — every row
   in a non-`full` mode is `INCOMPLETE`, published in the receipt and in the report header —
   but it is not yet a fast loop.

## 7. The mode ladder and `--reuse-pass`, measured

| arm | command | wall | statuses |
| --- | --- | ---: | --- |
| full | `perf --lane full` | 263.776 s | 217 `PASS`, 3 `NOT_RUN` |
| none | `perf --lane full --verify none` | 254.642 s | 220 `INCOMPLETE` |
| sample | `perf --lane full --verify sample` | 258.383 s | 220 `INCOMPLETE` |
| reuse | `verify --run <dir> --reuse-pass <dir>/verification.json` | **0.02 s** | 0 re-derived, 220 covered |

The sample rule is declared once and applied wherever a row declares a countable
verification unit: **`max(1, ceil(units/10))`, selected by `index % 10 == 0` in declaration
order** — a strided sample, never a prefix. `c2.delta.cdc-locality` declares its distinct
member identities as the unit and publishes `verify.units` and `verify.sampled`.

The two non-`full` arms were taken on `cb3dabbe1`, the commit before the digest-key fix,
with the **same harness binary** (`fb967c78…`) as the closure run: the difference between
the two commits is Python-only and does not move the binary a receipt names. Both are
recorded `source_dirty: true` because an unrelated documentation change was present in the
working tree at that moment. They are supplementary arms for the mode ladder, not admission
evidence, and `quick-modes.json` declares that rather than presenting them as
identity-matched to `run-full.json`.

`--reuse-pass` fails closed, and each refusal was exercised:

| offered proof | outcome |
| --- | --- |
| the run's own identity-matched `PASS` receipt | accepted, 0.02 s, `reused_proof_identities` recorded |
| a receipt from another tree | **refused**, naming `source_commit` and `harness_binary_sha256` |
| a receipt carrying `disagreements: 1` | **refused**: "1 disagreement(s) recorded" |
| a receipt covering no proof for a measured case | **refused**, naming the cases |

## 8. Three defects this round found and fixed

1. **56 of 217 passing rows published no operation time.** `ops/fs.rs` never called
   `write_timing`, so `c1.many-tiny`, `c1.tree.construct-traverse`, `c1.change-locality`,
   `c1.fs.build-scale` and `c1.tree.namespace-mutation` had no `timing.json` at all. The
   one choke point every driver already reached now writes it, and `g7.tree-complete` still
   gates its completeness.
2. **A prepared master sealed by another binary was consumed anyway.** `acquire()` returned
   a `stale` record and the runner passed `--load-input` regardless. The closure run before
   the fix reported `stale` for all 20 delta artifacts and measured them. The key is now the
   compatibility digest (V12), a mismatched or digest-less entry is superseded rather than
   consumed, and the producer binary is published as provenance.
3. **`load_reused_proof` was deleted by the digest-key commit** and `verify --reuse-pass`
   failed with a `NameError`. The full lane did not exercise it — the closure run does not
   use the flag — and running the flag's own acceptance is what caught it. Restored verbatim
   in `0ef2bed6e`, with both sides of its contract re-checked.

A fourth is recorded rather than fixed: the harness identity covers the **Rust** binary, both
lockfiles and the registry table, but not the harness's own Python. A Python-only harness
change is therefore invisible in a receipt's identity. It is outside this round's scope and
is stated so a reader is not misled by `harness_binary_sha256`.

## 9. Production LOC

`84936 -> 84936` (**delta 0**). No product source changed: this round is
`core/benchmark/**` plus harness documentation. Reference `65417 -> 65417`; core
`19519 -> 19519`. Counting method and scope are recorded in each commit message and
reproducible with `python3 tools/production_loc.py`. Scope: first-party product
implementation (`core/crates/*/src` + runtime SQL), comments, blanks and tests excluded.

## 10. Checks run

| Check | Result |
| --- | --- |
| `cargo +1.85.1 build --release --locked --manifest-path $H/Cargo.toml` | clean, no warnings |
| `cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml` | **92 passed / 0 failed** across 13 test binaries (84 before, plus `pinned_expectations`' 8) |
| `python3 -m unittest discover -s $H/shared -p 'test_*.py'` | OK, **112 tests** (106 before, plus the three new self-check wrappers) |
| `python3 $H/runner.py self-check` | PASS — lock parity 46 entries / 0 mismatches, registry, exceptions (8 declared), golden, phases |
| `python3 $H/runner.py verify --run <closure>` | 0 disagreements, call-graph PASS over 120 files, tripwires PASS over 159 stores |
| `python3 $H/runner.py calibrate --out <closure>` | E1 `REFUTED`; E2/E3/E4/W1/W2/W4 `SATISFIED` |
| `python3 core/tools/check_product_boundary.py` | PASS — 120 production Rust/SQL files scanned |
| `python3 tools/production_loc.py` | 84936 combined, unchanged |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | **473 passed / 0 failed** — unchanged from round 4c, as expected: no product source changed and the product lock is byte-identical |
| `cargo +1.85.1 clippy` / `fmt` | **not run as a gate** (see below). The harness workspace is not rustfmt-clean at HEAD (24 files, most of them untouched by this round) and has never been a clippy gate; this round did not change that and did not add one |

`cargo clippy` and `cargo fmt` are **not** gates for this workspace and were not made ones.
The harness is not rustfmt-clean at HEAD — 24 files carry a diff, most of them untouched by
this round — and running `fmt` over the whole workspace would bury a 5-commit round in a
24-file reformat that changes no behaviour. The three files this round added are formatted;
the rest of the workspace is left as it was found, and the harness's own declared checks are
the ones above.

`tools/preflight.sh` was not run and was not restored. No CI workflow, no aggregate gate and
no wrapper was created.

## 11. Files in this directory

| File | What |
| --- | --- |
| `run-full.json` | the run's tally, identity, declarations and the lane's phase totals |
| `report-full.txt` | the rendered report, including every non-`PASS` row |
| `verify-pass.json` | the verification pass: 0 disagreements, both tripwire verdicts, per-row phase and pin re-derivations |
| `experiments-E1-E4-W1-W2-W4.json` | the calibration run |
| `reused-proof.json` | the `--reuse-pass` receipt: `reused_proof_identities` and the explicit omission |
| `compare-round4c-vs-round5.txt` | every difference between round 4c and this run, classified |
| `quick-modes.json` | the `none` and `sample` lanes, with the reason the quick lane is not fast |
| `prepare-summary.json` | the `prepare --lane full` manifest summary: 20 masters, all superseded and re-acquired under the new key |
