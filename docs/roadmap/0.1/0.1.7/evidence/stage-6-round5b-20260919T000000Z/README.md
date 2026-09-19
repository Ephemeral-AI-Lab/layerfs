# Stage 6, round 5b — the preparation half of the cheap campaign

> **Status:** Round-5 continuation receipt for [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Append-only. It supersedes nothing and edits no earlier receipt; the round-3, round-3b,
> round-4, round-4b, round-4c and round-5 directories are the historical record and are
> untouched. Stage 6's acceptance issue [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> is **closed** and was not reopened. This round changes no verdict: every budget already
> passed, and it still does.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **173.566 s** |
| Source commit | `395d22cf1178b3b818d4f89bba017eb737d78568`, **clean tree** |
| Harness binary sha256 | `72639666c8aaabfa4f0a7991aa223ae0e1e38b22712510bc641c7a69e9dd12c1` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `3f01abded44b973dee4d2a3a31364212cd742bce1b3f7800eb4e5a7b6d8f43e3` (the `prepared` column is new, so the whole digest moved) |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Verification mode | `full` |
| Fixture-recipe version | `fs-bench-fixture-recipe-v2` |

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

Verification: **0 disagreements** across all 220 re-derived statuses; sealed call-graph
**PASS** over **120** product source files; runtime tripwires **PASS** over **141** stores;
verification budget `PASS` at **1.96 s** against the 60 s limit. Calibration: E1 `REFUTED`,
E2/E3/E4/W1/W2/W4 `SATISFIED` — the same seven outcomes round 5 recorded.

## 2. The four phases, before and after

| phase | field | round 5 | **round 5b** | target |
| --- | --- | ---: | ---: | --- |
| a preparation | `preparation_wall_ns` | 98.226 s | **26.619 s** | ≤ 25 s — **missed by 1.6 s** |
| — per-sample copy + de-warm | `acquisition_wall_ns` | 0.066 s | **0.066 s** | reported |
| b **real work** | **`operation_ns`** | **76.184 s** | **75.885 s** | published, must not fall — **see §2.1** |
| c verification | `verification_wall_ns` | 85.513 s | **68.675 s** | ≤ 108 s — **met** |
| d cleanup | `cleanup_wall_ns` | 0.000077 s | **0.00000125 s** (1,250 ns max) | ≤ 0.5 s/row — **met** |
| | harness work in the timer | `handoff_ns` | **0.975 s** (0.106 s max) | published |
| | process wall | `complete_command_ns` | **172.225 s** (9.895 s max) | ≤ 15 s/row, exceptions ≤ 25 s — **met** |
| **lane** | `run.json` wall | **263.776 s** | **173.566 s** | ≤ 200 s — **met** |

The four phases reconcile with the wall on **every** row — `unreconciled_rows: []` in
`run-full.json` — inside the declared tolerance of 250 ms plus 2% of the wall.

### 2.1 The falsifier: `operation_ns`

`T4`: *if the golden number improves, measured work has moved into setup and the change is
rejected.* The lane totals are

```text
round 5 receipt      sum(operation_ns) = 76.184 s
round 5b             sum(operation_ns) = 75.885 s   (-0.39%)
```

**A −0.39% lane change is inside the instrument's own spread, and this session measured
that spread directly.** The *unchanged* round-5 binary was run twice in this session: once
at the round-5 closure (`76.184 s`) and once as this round's baseline
(`/tmp/base-r5c`, **74.571 s**) — a 2.1% spread for the same bytes and the same binary. The
round-5b value, 75.885 s, sits between them.

**No row's operation time moved for a structural reason, and the counters prove it.**
`shared/compare_runs.py` reports `verdict IDENTICAL` against the round-5 closure run
(`compare-round5-vs-round5b.txt`): every one of the 1,757 pinned counters and every one of
the 212 identity digests is unchanged, and every gate status is unchanged. The only
differences the comparator classifies are the two it already classifies **between two runs
of one binary** — `timing_json_bytes` (±1 byte, the product's own tree rendering) and
`resource-drift` on `space.allocated_bytes` / `heap.peak_incremental_bytes`.

Against this round's own same-session baseline, 26 rows are reported as having fallen. The
largest absolute fall is 9.5 ms, and the two families it is concentrated in were measured
for their own spread:

| row | baseline | round 5b | three repeat runs of the same binary |
| --- | ---: | ---: | --- |
| `dedup-workspace-unique-100-compact-v2` | 570.2 ms | 372.3 ms | **637.6 / 426.2 / 438.3 ms** |
| `dedup-workspace-unique-10-compact-v2` | 51.4 ms | 41.9 ms | **38.3 / 42.4 / 38.9 ms** |

Neither row's change is larger than its own run-to-run spread, and both published every
counter identically. Fifteen of the 26 falls are rows this round did not touch at all —
`dedup-cdc-boundary-*`, `small-file-delta-*`, `pipeline-edits-chunked`, `lifecycle-begin-save`
— which is the same class the round-5 receipt recorded when it compared two runs of one
binary.

## 3. What produced the 90 s

**Preparation, entirely.** Nothing was removed from a timed phase: the measured operation is
the same `begin_save`/`accept`/`finish`, `apply_edits` or `build_filesystem` call over the
same bytes, and every counter proves it.

| item | what changed | rows |
| --- | --- | ---: |
| **V11** | `delta_fixture` allocated and freed a fresh copy of the whole base per member; one buffer is now reused | 20 |
| **V10** | the artifact's object set is one indexed payload file instead of one file per object, and `Artifact::read` no longer hashes every object twice | all |
| **V6** | every family's expectation is computed once at acquisition and read back by the oracle, instead of being re-derived inside the performance invocation | all |
| **V9a** | prepared object sets for `c1.edit.*` and `c1.cdc.chunk-count` | 56 |
| **V9b** | prepared base Store / object set for `c2.reuse.cross-file`, `c2.reuse.workspace`, `c2.read.waves`, `c2.footprint` | 34 |
| **V9c** | prepared input tree for `c1.fs.build-scale`, including the fixture chain every batch above the walk ceiling reads its base from | 8 |
| **registry** | `registry::Preparation` is a column of the golden registry table; `PHASE_SPLIT_FAMILIES` is deleted | — |

### 3.1 The `c1.fs.build-scale` chain, the largest single item

At the 100,000-file tier the row's measured chain is 33 `build_filesystem` /
`update_filesystem` operations, and the harness built a **second, byte-identical** chain
before the timer as the fixture its batches read through. Both chains cost ~4.4 s. The
acquisition now builds the chain once, persists its objects as the artifact's packed object
set and its per-batch roots as the artifact's members, and the performance invocation loads
them: preparation fell from **4.627 s to 0.195 s** per row with the operation unchanged
(4.409 s against 4.384 s).

The chain remains a second, unmeasured, byte-identical operation; it now runs in the
acquisition invocation rather than inside the performance one. Its roots are the oracle the
measured chain is compared against, and the final root is additionally pinned by
`tests/golden/expected.tsv` (`digest:filesystem_root`) — a frozen expectation that does not
depend on the chain at all.

### 3.2 Per-row preparation, and the rows that miss

34 of 217 admission rows exceeded 1.0 s in round 5; the worst was 5.448 s. **Four** exceed it
now, and the worst is 2.364 s (`preparation-breakdown.txt`):

| row | prep | why it is not lower |
| --- | ---: | --- |
| `payload-create-500m` | 2.364 s | `c1.construct.*` **must not be prepared** — the construction *is* the measured operation — so the row generates its 500 MiB fixture and hashes it to state the oracle's expectation before its timer. The hash is the harness's scalar SHA-256 over 500 MiB. |
| `payload-create-chunked-500m` | 2.342 s | same |
| `dedup-workspace-unique-500-compact-v2` | 1.175 s | loads a ~500 MiB packed object set **and** copies an 805 MB base Store |
| `store-footprint-unique-100000` | 1.005 s | loads 100,000 small objects |

Everything else is at or below 0.761 s. `dedup-cross-file-unique-500` (1.038 s) and
`store-footprint-metadata-cardinality-100000` (1.044 s) were also over 1.0 s in an earlier
lane of the same binary at `248a95cf9`, so **six rows sit within a few per cent of the
boundary and flip across it between runs.** That is the target's own precision, not a
distinction the harness can draw.

**This is the T1 floor, measured rather than estimated.** A prepared master is loaded by
reading its packed object set and re-identifying every object: `FinalizedObject::new` hashes
the canonical bytes, and the harness cannot avoid that without a product change. At 500 MiB
that is a ~0.5 GB read plus ~0.5 GB of BLAKE3 plus one allocation per object — 0.7–1.2 s,
before any other work in the phase. Lazy loading would break the declared
`warm-in-process-fixture` cache state, which `AGENTS.md` §1 forbids. **T1 still needs its
ruling:** a tier-scaled per-row target, an allowed cache-state change, or these rows
accepted as `INCOMPLETE`.

### 3.3 `prepare --lane full`, and the T2/T3 tension

| | round 5 | **round 5b** |
| --- | ---: | ---: |
| `prepare --lane full` wall | 75.39 s (20 masters) | **139.4 s** (118 masters) |
| acquisition wall inside it | 75.39 s | **127.864 s** |
| masters produced | 20 | **118 of 220 rows** |
| payload written | — | **15.567 GB** |

Round 5 passed the 90 s ceiling *because only twenty masters existed*: the other six
fixture-heavy families had none, so there was nothing to acquire. Acquiring them costs
127.9 s of fixture construction that the campaign previously paid inside every performance
invocation instead — 98.2 s of it in the lane, once per run, forever, versus 127.9 s once per
compatibility digest. **The two targets do pull against each other, and the honest reading is
that a ceiling on a once-per-digest acquisition is the wrong shape for a campaign whose lane
target it makes reachable.** The ruling is the owner's.

Where the 127.9 s goes (`prepare-summary.json`): `c1.edit.length-changing` 34.0 s,
`c2.delta.cdc-locality` 20.2 s, `c1.cdc.chunk-count` 13.5 s,
`c1.edit.length-preserving` 13.4 s, `c2.reuse.workspace` 13.1 s,
`c2.reuse.cross-file` 12.1 s, `c1.fs.build-scale` 10.2 s, `c2.read.waves` 6.8 s,
`c2.footprint` 4.8 s.

Two families that gate no read-back at all (`c2.delta.cdc-locality` is O1 + O3,
`c2.reuse.workspace` is O1 + O5) were hashing every member to record an expectation nothing
consults. That dead work is gone, and on the delta family alone it was worth **~53 s** of the
acquisition — the five `dedup-cdc-*-500` rows went from ~12 s each to ~4 s each.

## 4. The verification target

The largest single verification invocation was **7.636 s** in round 5
(`dedup-cross-file-unique-500`), against a 5 s target. It is **4.257 s** now — the target is
met — and `payload-random-read-500m` carries it.

**The cost was the harness's own SHA-256, not the product.** `dedup-cross-file-identical-500`
spent 4.317 s of verification hashing 1 GiB of fixture bytes **twice** to re-derive an
expectation the artifact already carried; it is 0.007 s now. This is V6, and it turned out to
be worth more than the handoff estimated: it is what makes the 5 s target reachable without
sampling anything.

| | round 5 | **round 5b** |
| --- | ---: | ---: |
| lane `verification_wall_ns` | 85.513 s | **68.675 s** |
| largest single invocation | 7.636 s | **4.257 s** |
| deferred verify invocations | 1.905 s | **0.801 s** |

## 5. Defects fixed rather than recorded

Three, all in `core/benchmark/**`, all exposed by this change:

1. **A sealed master made every per-sample copy read-only.** `std::fs::copy` copies the
   source's permission bits, so the sample inherited the master's `0o444` and the product
   correctly refused it (`SqliteFailure(ReadOnly)`). `test_setup_and_cache_discipline.md` §4
   step 4 is `chmod 0600`; `prepare_sample` now applies it. The seal is what exposed it.
2. **`perf --reuse-pass` marked every row with a deferred oracle `INCOMPLETE`.** A reused
   invocation has no process wall and publishes no `phases-verify.json`, and composing a wall
   for it demanded a phase file that deliberately does not exist. The omission is now named
   (`phases.reused_invocations`) instead of being inferred from a missing file.
3. **The W2 calibration arm drove a case that now needs a master, without one.** It starts
   `dedup-cross-file-unique-10` with `--hold-save` and no `--load-input`, so the child failed
   closed with `NOT_RUN` and the arm reported a product refusal that never happened
   (`W2 REFUTED`). `calibrate` now acquires the master its arm needs. Caught by running
   `calibrate`, not by assuming it still worked.

A fourth is recorded rather than fixed, because its fix is a scope question: **the harness
identity still does not cover the harness's own Python.** A receipt names the Rust binary's
sha256, both lockfiles and the registry table; a Python-only change to `runner.py` or
`shared/` leaves every one of them unchanged. `harness_binary_sha256` in this receipt is
therefore a statement about the compiled half only.

## 6. The mode ladder and the reuse path, still failing closed

`quick-modes.json`:

| run | tally |
| --- | --- |
| `perf --lane smoke --verify none` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --verify sample` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --reuse-pass <proof>` | **20 `PASS`**; `dedup-cdc-overwrite-1` carries `verification.status: REUSED`, the omission recorded, and `phases.reused_invocations == ["verify"]` |
| `verify --run <lane> --reuse-pass <proof>` | 220 cases, **0 re-derived**, 0.06 s |

Refusal paths exercised: a missing proof path and a `run.json` offered as a proof are both
refused with their reason.

**The ≤ 70 s quick lane is still not reachable, and the reason is unchanged.** `--verify none`
skips the *deferred* verification invocation, and only `c2.delta.cdc-locality` has one
(0.801 s of a 173.6 s lane). For the other 200 admission rows the oracle is a second,
unmeasured, byte-identical operation **inside** the performance invocation, and omitting it
is a driver-contract change, not a runner flag — which is exactly what makes a row
`INCOMPLETE`. **Decision 3 still needs its ruling.**

## 7. What is *not* true here

- **Four rows exceed the 1.0 s per-row preparation target** (six in an earlier lane of the
  same binary), and two of them are the rows this assignment forbids preparing. §3.2.
- **`prepare --lane full` is 139.4 s against a 90 s ceiling**, having acquired six families'
  masters for the first time. §3.3. T2/T3.
- **The lane `sum(preparation_wall_ns)` is 26.619 s against a 25 s target.** §2.
- **The ≤ 70 s quick lane is not reachable without a driver-contract change.** §6.
- **`FilesystemRead::inode` (owner ruling 2) was not done.** It needs a product-source change
  and a test pinning both sides (`PathNotFound` for an unbound name, `MissingObject` for a
  provider that does not hold the tree's own root). It blocks nothing else, and guessing at
  the classification is explicitly forbidden. **Reported as a blocker, not attempted.**
- **The harness identity does not cover the harness's own Python.** §5.
- **`cargo clippy` and `cargo fmt` were not run.** Neither is a gate for this workspace: the
  harness is not rustfmt-clean at HEAD (24 files, most untouched by this round), and running
  `fmt` over the workspace would bury this round in a reformat that changes no behaviour.
- **The `complete_command_ns` budget still classifies the process wall**, and `elapsed_ns`
  still never gate-decides. Owner ruling 1 defers that to a `CONTRACT.md` change.

## 8. Reproduction

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test  --locked --manifest-path $H/Cargo.toml            # 92 passed / 0 failed
python3 -m unittest discover -s $H/shared -p 'test_*.py'              # 112 tests, OK
python3 $H/runner.py self-check                                       # PASS
python3 $H/runner.py prepare                                          # 139.4 s cold, once per digest
python3 $H/runner.py perf  --lane full --out /tmp/r5b                 # 173.6 s
python3 $H/runner.py verify --run /tmp/r5b                            # 0 disagreements
python3 $H/runner.py report --run /tmp/r5b
python3 $H/runner.py calibrate --out /tmp/r5b                         # E1 REFUTED, six SATISFIED
python3 $H/shared/compare_runs.py /tmp/r5-final2 /tmp/r5b             # verdict IDENTICAL
python3 core/tools/check_product_boundary.py                          # PASS, 120 files
python3 tools/production_loc.py                                       # 84936 combined
```

## 9. Files

| file | what it is |
| --- | --- |
| `run-full.json` | the lane's own record: identity, selection, tally, phase totals |
| `report-full.txt` | the four-axis report, including the before/after family table |
| `verify-pass.json` | the re-derivation: 220 findings, 0 disagreements |
| `compare-round5-vs-round5b.txt` | row-by-row, counter-by-counter against the round-5 closure run |
| `preparation-breakdown.txt` | `preparation_wall_ns` / `verification_wall_ns` / `operation_ns` for all 217 admission rows, worst first |
| `prepare-summary.json` | the cold acquisition: 118 masters, 127.9 s, 15.567 GB, per family and per row |
| `quick-modes.json` | the mode ladder, the reuse path and the refusal paths |
| `reused-proof.json` | `verify --reuse-pass`'s own record |
| `experiments-E1-E4-W1-W2-W4.json` | E1–E4, W1, W2, W4 with their fields |
