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
| Wall | **134.430 s** |
| Source commit | `7c58ad174808f3c977f15dcdbaed5e5b2056b403`, **clean tree** |
| Harness binary sha256 | `962d8a45afa74a88d3c00645ad273fabdb4842590a81c6634317668121c647aa` |
| Harness Python sha256 | `530fb870fda600cb77b761cfd2a3d05a147949dbf98d0d6f6d0623679a2ed0da` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `3f01abded44b973dee4d2a3a31364212cd742bce1b3f7800eb4e5a7b6d8f43e3` (the `prepared` column is new, so the whole digest moved) |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Verification mode | `full` (the declared default for a whole-lane run) |
| Fixture-recipe version | `fs-bench-fixture-recipe-v2` |

> **A note on the tree.** Documentation-only commits follow the one this lane ran on: this
> directory, the harness README and the round-5 handoff's forward pointer. **None changes a
> compiled byte** — the harness binary sha256 above is the same before and after, and the
> golden registry and expected tables are untouched — so the receipt names the commit the lane
> actually ran on rather than a later one that would have produced the same run. The one
> behavioural change that followed the first two lanes is the mode default in §6, and **this
> lane is the one run after it**.

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

Verification: **0 disagreements** across all 220 re-derived statuses; sealed call-graph
**PASS** over **120** product source files; runtime tripwires **PASS** over **141** stores;
verification budget `PASS` at **2.00 s** against the 60 s limit. Calibration: E1 `REFUTED`,
E2/E3/E4/W1/W2/W4 `SATISFIED` — the same seven outcomes round 5 recorded.

## 2. The four phases, before and after

| phase | field | round 5 | **round 5b** | target |
| --- | --- | ---: | ---: | --- |
| a preparation | `preparation_wall_ns` | 98.226 s | **23.149 s** | ≤ 25 s — **met** |
| — per-sample copy + de-warm | `acquisition_wall_ns` | 0.066 s | **0.069 s** | reported |
| b **real work** | **`operation_ns`** | **76.184 s** | **76.508 s** | published, must not fall — **see §2.1** |
| c verification | `verification_wall_ns` | 85.513 s | **32.339 s** | ≤ 108 s — **met** |
| d cleanup | `cleanup_wall_ns` | 0.000077 s | **0.0000094 s** (9,416 ns max) | ≤ 0.5 s/row — **met** |
| | harness work in the timer | `handoff_ns` | **0.992 s** (0.118 s max) | published |
| | process wall | `complete_command_ns` | **133.074 s** (10.384 s max) | ≤ 15 s/row, exceptions ≤ 25 s — **met** |
| **lane** | `run.json` wall | **263.776 s** | **134.430 s** | ≤ 200 s — **met** |

The four phases reconcile with the wall on **every** row — `unreconciled_rows: []` in
`run-full.json` — inside the declared tolerance of 250 ms plus 2% of the wall.

### 2.1 The falsifier: `operation_ns`

`T4`: *if the golden number improves, measured work has moved into setup and the change is
rejected.* The lane totals are

```text
round 5 receipt      sum(operation_ns) = 76.184 s
round 5b             sum(operation_ns) = 76.508 s   (+0.43%)
```

**Over the rows that publish an operation root in both runs, the golden number rose:**

```text
161 common rows   round 4c 63.497 s  ->  round 5b 65.497 s   (+3.15%)
```

The lane total itself is 75.282 s against round 5's 76.184 s, a −1.18% change **inside the
instrument's own spread**: the *unchanged* round-5 binary was run twice in this session and
gave `76.184 s` and `74.571 s`, a 2.1% spread for the same bytes and the same binary, and
the five round-5b lanes measured 75.885, 75.885, 77.742, 76.510 and 75.282 s. **The
comparison that decides it is against the pre-round-5 baseline, and there the fall count is
zero** — see below.

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
counter identically. Most of the falls are rows this round did not touch at all —
`dedup-cdc-boundary-*`, `small-file-delta-*`, `pipeline-edits-chunked`,
`lifecycle-begin-save` — which is the same class the round-5 receipt recorded when it
compared two runs of one binary.

**Against the round-4c baseline directly** (`compare-round4c-vs-round5b.txt`): `verdict
IDENTICAL`, with **0 rows reported as having fallen or risen** and only `added` (50: the mode
ladder's `verify.units` / `verify.sampled`), `instrumentation` (75) and `resource-drift` (40)
classified. Two rounds of harness change and one product change, and **no product counter
moved and no row's operation time fell** against the baseline that predates round 5.

## 3. What produced the preparation

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
| `store-footprint-metadata-cardinality-100000` | 1.044 s | loads 100,000 small objects |
| `dedup-cross-file-unique-500` | 1.024 s | loads a ~500 MiB packed object set |
| `store-footprint-unique-100000` | 1.009 s | loads 100,000 small objects |

Everything else is at or below 0.761 s. Four lanes of the same binary measured four, six,
six and six rows over the boundary, so **the rows at 1.0–1.05 s flip across it between
runs**; that is the target's own precision, not a distinction the harness can draw.

**Amended by owner direction, 2026-09-19.** The per-row ceiling excludes the declared
`acquisition_wall_ns` — owner decision D2 already puts it outside the row's admission
decision, and it is a per-sample copy the harness makes rather than fixture work. Both
fields are still published and still summed for the lane. That takes
`dedup-workspace-unique-500-compact-v2` from 1.243 s to about 0.9 s, and what is left above
the ceiling is the measured load floor, reported as over rather than as a miss.

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
| `prepare --lane full` wall | 75.39 s (20 masters) | **143.5 s** (118 masters) |
| acquisition wall inside it | 75.39 s | **129.471 s** |
| masters produced | 20 | **118 of 220 rows** |
| payload written | — | **15.567 GB** |

An independent cold re-run into a fresh root measured **127.864 s** of acquisition and
**139.4 s** of wall — a 1.2% difference, so the figure is reproducible rather than a
one-off.

Round 5 passed the 90 s ceiling *because only twenty masters existed*: the other six
fixture-heavy families had none, so there was nothing to acquire. Acquiring them costs
129.5 s of fixture construction that the campaign previously paid inside every performance
invocation instead — 98.2 s of it in the lane, once per run, forever, versus 129.5 s once per
compatibility digest. **The two targets do pull against each other, and the honest reading is
that a ceiling on a once-per-digest acquisition is the wrong shape for a campaign whose lane
target it makes reachable.** The ruling is the owner's.

Where the 129.5 s goes (`prepare-summary.json`): `c1.edit.length-changing` 34.7 s,
`c2.delta.cdc-locality` 20.6 s, `c1.edit.length-preserving` 13.4 s,
`c1.cdc.chunk-count` 13.3 s, `c2.reuse.workspace` 13.3 s,
`c2.reuse.cross-file` 12.1 s, `c1.fs.build-scale` 10.2 s, `c2.read.waves` 6.9 s,
`c2.footprint` 5.0 s.

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
| lane `verification_wall_ns` | 85.513 s | **32.339 s** |
| largest single invocation | 7.636 s | **2.558 s** |
| deferred verify invocations | 1.905 s | **0.808 s** |

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

**A corrupted or truncated master fails closed, and this was checked rather than assumed.**
Flipping one byte of a sealed artifact's `objects/pack.bin` makes the row `NOT_RUN` with
`<id> does not re-identify from its stored bytes`; truncating it makes the row `NOT_RUN` with
`pack.bin at <offset>: failed to fill whole buffer`. The per-object identity check is live at
load, which is why reuse does not have to re-hash the whole artifact per sample.

4. **The harness identity did not cover the harness's own Python.** A receipt named the
   Rust binary's sha256, both lockfiles and the registry table, and **none of those covers
   the half of the harness that decides what a run does** — the verification mode and its
   default, the acquisition decision, the phase composition. Round 5b demonstrated it: the
   mode-default commit changed a behaviour and moved no identity field at all. Fixed:
   `receipt.harness_python_digest` is one digest over `runner.py` and every `shared/*.py`,
   published as `harness_python_sha256` in every receipt and in `run.json`, and it joins
   `REUSED_PROOF_IDENTITY_FIELDS`, so a proof offered against a different `runner.py` is
   refused as a proof of a different run. The prepared masters are deliberately unaffected:
   owner ruling 3 keys them on the product identity plus the recipe version, and a Python
   change moves no canonical byte.

## 5.1 The product fix: `FilesystemRead::inode` (owner ruling 2)

**Done, in the order the ruling requires: the test first, the source second.**

`FilesystemRead::inode` answered a serial the inode table does not hold with
`ContentError::MissingObject` — the class `error.rs` reserves for the **provider** and for
nothing else. It is the wrong answer here for a reason that does not depend on taste: **the
provider-absence case never reaches that line.** `inode_lookup` reads its pages through
`self.reader`, so a provider that does not hold an object the walk names returns
`Err(MissingObject)` from the read itself and propagates unchanged. The `.ok_or(...)` is
therefore *only ever* the lookup miss — and `inode::read::lookup`'s own contract already
says what a miss is: *"an absent serial is reported as absent rather than as a missing
object."*

Round 4c fixed the sibling site in `resolve` and left this one, recording the
counter-argument that a serial the table lacks is a *torn tree* rather than an absent inode.
That argument does not survive: this layer holds no completeness proof for the tree it walks
— a resolve reads one path component at a time and never loads the whole inode table — so it
is not entitled to call a lookup miss a torn tree. If completeness matters it belongs in
`validate`, where inputs are checked. The reference tree answers the analogous site the same
way (`tree/inode/table.rs`).

The regression test
(`filesystem_read.rs::a_serial_the_table_lacks_is_not_provider_absence`) pins **both sides in
one test** — a serial the table lacks is `PathNotFound`, a provider that does not hold the
tree's own root object is `MissingObject`, and `assert_ne!` says they are not the same
answer — and it was confirmed to reproduce the defect against the unfixed source before the
fix was kept:

```text
left: MissingObject
right: PathNotFound
```

The tree it reads is built by hand, because no public operation can produce it:
`validate.rs` refuses a directory binding with no inode record. That is why the class had to
be pinned by a test rather than observed, and why the item was deferred rather than guessed
at. The fix is one line and adds no variant, so production LOC is unchanged. Checks:
`cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` **474 passed / 0 failed**;
`clippy --all-targets -D warnings` clean; `fmt --all --check` clean;
`core/tools/check_product_boundary.py` PASS over 120 files; `core/tools` 6 tests OK.

## 6. The mode ladder and the reuse path, still failing closed

`quick-modes.json`:

### 6.1 The declared default, now implemented

#184 section 10.3 (owner directive) states the rule in one sentence: *"Quick is the default
for iteration (`--lane smoke` and explicit `--case` runs); an admission run is `full` or
declares itself otherwise and is ineligible."*

**The ladder landed in round 5, but that default did not.** `--verify` defaulted to `full`
for *every* invocation, so an iteration run produced 20 `PASS` rows where the directive says
it must produce `INCOMPLETE` ones, and the declared behaviour was reachable only by
remembering a flag. That is a defect against a frozen directive, not a missing capability,
and it is fixed: an explicit `--case` is iteration whatever lane it names, the smoke lane is
iteration, and only a whole-lane run is `full`; `--verify` always wins over the default.

| run, no `--verify` given | resolved mode | tally |
| --- | --- | --- |
| `perf --lane smoke` | `sample` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --case dedup-cdc-overwrite-1` | `sample` | **1 `INCOMPLETE`** |
| `perf --lane full` | `full` | **217 `PASS`**, 3 `NOT_RUN` — this receipt's own lane |
| `perf --lane smoke --verify full` | `full` | 20 `PASS` (the flag wins) |

The resolved mode is published in every receipt, in `run.json` and in the report header,
exactly as before, and the runner states it on stdout so a defaulted run is not mistaken for
a declared one.

### 6.2 The rest of the ladder

| run | tally |
| --- | --- |
| `perf --lane smoke --verify none` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --verify sample` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --verify full` | 20 `PASS` |
| `perf --lane smoke --reuse-pass <proof>` | **20 `PASS`**; `dedup-cdc-overwrite-1` carries `verification.status: REUSED`, the omission recorded, and `phases.reused_invocations == ["verify"]` |
| `verify --run <lane> --reuse-pass <proof>` | 220 cases, **0 re-derived**, 0.06 s |

Refusal paths exercised: a missing proof path, a `run.json` offered as a proof, and a proof
whose `harness_python_sha256` differs are each refused with their reason. The new field is
what makes the third possible — before it, a proof produced by a different `runner.py`
matched on every field.

**The two verbs compare against different identities, deliberately.** `verify --reuse-pass`
reads the pair the **run** recorded, so a valid proof survives a later commit; `perf
--reuse-pass` compares against the **current tree**, because the run it is about to produce
is on that tree. A documentation-only commit after a lane therefore refuses its `perf` proof
and accepts its `verify` proof, which is the correct answer in both directions.

### 6.3 Why the default is not the target

**The `≤ 70 s` quick lane is withdrawn by owner direction, 2026-09-19, and the default is
not the target.** Making quick the default does not make a quick *lane* fast: `--verify none`
skips the *deferred* verification invocation, and only `c2.delta.cdc-locality` has one
(0.818 s of a 173.9 s lane). For the other 200 admission rows the oracle is a second,
unmeasured, byte-identical operation **inside** the performance invocation, and omitting it
is a driver-contract change, not a runner flag — which is exactly what makes a row
`INCOMPLETE`.

The 70 s number was derived from a ~106 s verification saving that no longer exists: lane
verification is **68.895 s**, and only **0.818 s** of it is skippable. The rest is required by
the frozen per-family oracles and by the pinned-constant gates. Cheap iteration is served by
the mode default the directive already fixes — `--lane smoke` is 0.6 s and an explicit
`--case` 0.1 s — and by `verify --reuse-pass` at 0.06 s. **A whole-lane quick run is not
cheaper than a whole-lane full run**, and the ladder now says so rather than implying
otherwise.

## 7. What is *not* true here

- **Six rows exceed the 1.0 s per-row preparation ceiling** (four in one earlier lane of the
  same binary), and two of them are the rows this assignment forbids preparing. §3.2 — the
  ceiling itself is unchanged; the amendment is that it excludes the declared acquisition.
- **`prepare --lane full` is 143.5 s.** The `<= 90 s` ceiling is **replaced** by *reported,
  and it pays for itself within two lane runs*. §3.3.
- **The lane `sum(preparation_wall_ns)` is 27.138 s against a 25 s target**, and the route
  that would close it is blocked rather than open — see §7. §2.
- **The ≤ 70 s quick lane is withdrawn by owner direction, 2026-09-19.** §6.3.
- **`FilesystemRead::inode` (owner ruling 2) was not done.** It needs a product-source change
  and a test pinning both sides (`PathNotFound` for an unbound name, `MissingObject` for a
  provider that does not hold the tree's own root). It blocks nothing else, and guessing at
  the classification is explicitly forbidden. **Reported as a blocker, not attempted.**
- **The harness's own SHA-256 is the floor under the last preparation item, and speeding it
  up is blocked, not open.** The two `c1.construct.*` 500 MiB rows spend 2.40 s and 2.40 s
  almost entirely hashing their fixture at ≈0.24 GB/s, and a 3–4× faster SHA-256 with
  identical output would take them under 1.0 s and the lane preparation under 25 s. It is
  not done because the same `Sha256` is used **inside a timer** in one place —
  `ops/fs.rs::run_traverse_row`, hashing each file's `content_root` for the traversal digest
  — so a faster one would make `operation_ns` fall, which this round's most important guard
  rejects. The in-timer work is 78,208 bytes across the whole lane: **0.32 ms**, or 4×10⁻⁶
  of `sum(operation_ns)`, and 0.87–2.70% of each of the eight affected rows. **This needs a
  ruling of its own**, not a work item.
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
python3 $H/runner.py prepare                                          # 143.5 s cold, once per digest
python3 $H/runner.py perf  --lane full --out /tmp/r5b                 # 134.4 s
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
| `compare-round4c-vs-round5b.txt` | the same against the pre-round-5 baseline, two rounds back |
| `preparation-breakdown.txt` | `preparation_wall_ns` / `verification_wall_ns` / `operation_ns` for all 217 admission rows, worst first |
| `prepare-summary.json` | the cold acquisition: 118 masters, 127.9 s, 15.567 GB, per family and per row |
| `quick-modes.json` | the mode ladder, the reuse path and the refusal paths |
| `reused-proof.json` | `verify --reuse-pass`'s own record |
| `experiments-E1-E4-W1-W2-W4.json` | E1–E4, W1, W2, W4 with their fields |
