# Pre-registration — #226 round 22, step 1: the per-batch prefix keys

Written **at 2026-09-21T16:33:14Z, before the first locked run of this round**: no `runner.py perf` had
been invoked from this worktree when this file was written, and `.measurement.lock` did not exist. One
**unlocked** diagnostic of the new binary and one of the 10,000-entry row were taken before it and are
declared in §5; every prediction below says which of its numbers came from a diagnostic and which came
from round 21's control.

## 1. What this round builds

One harness change, and the two rows that read it:

* `pipeline-namespace-100000` — the treatment. Its fixture's per-batch prefix snapshots are replaced by
  one complete chain plus one cumulative identity set per batch (`PrefixKeys` / `PrefixProvider`,
  `workload/providers.rs`), which is the fix round 21 §11.4 filed and did not apply.
* `pipeline-namespace-10000` — the same driver at one tenth the size. It is the check that the change
  did not move the older fixture, and it is the row whose pins are longest-standing.
* `namespace-10000` — the session anchor (§2 prediction 11), **not** a row this round changes.

**No product line changes.** `core/crates/**` and `core/*/sql/**` are byte-identical to `5be4b7ae0`;
`tools/production_loc.py --root .` reports `97100 -> 97100 (delta 0)` (core 31683 in 194 files,
reference 65417 in 193 files).

**What is measured does not change.** The fixture is still constructed before the timer, still held in
memory, and still read from memory inside it; the timer's boundary
(`test_setup_and_cache_discipline.md` §2.2, the driver's own statement at `ops/pipeline.rs:945-967`) is
untouched. The row's `cache_state` and `store_state` are unchanged, and `g4.residency` still reads 0.
**This round therefore claims no cache-contract change**, and step 2 of the handoff — options A and D —
is not run here. The change is to *how much of the fixture the harness holds*, not to when it reads it.

## 2. What is predicted

Round 21's control is `ns21-E1-100000-final-20260921T164500Z` (and its 10,000-entry sibling
`ns21-D2`); its numbers are `report.md` §11. Every prediction below is stated as a direction and a band
**before** the run.

| # | quantity | registered prediction | basis |
| --- | --- | --- | --- |
| 1 | **lifetime** peak RSS, 100,000 row | **falls by 159–164 MB** from round 21's 1,050,738,688, i.e. to **886,738,688–891,738,688** | §5.1's measured key-set cost, subtracted from the snapshot cost round 21 measured |
| 2 | measured-region **increment** | **within ±20 %** of round 21's 37,519,360 → **30,015,488–45,023,232**, and **not higher than 48,000,000** | the handoff's step-1 wording; the wider absolute bound is the one a reader should use |
| 3 | `pipeline.operation_work_ns` | **within ±10 %** of round 21's 3,549,393,833 → **3,194,454,450–3,904,333,216** | §2a — the tolerance this round registers |
| 4 | all **fifteen pins**, 100,000 row | **reproduce exactly**, including `digest:filesystem_root 2412681d…fd954` | the change is harness-only; a pin that moves means the change reached the measurement |
| 5 | all **fifteen pins**, 10,000 row | **reproduce exactly**, including `digest:filesystem_root 1d6fba29…7847` | the same driver, one tenth the size |
| 6 | `pipeline.construct_ns` + `pipeline.construct_noise_ns`, 100,000 row | within **±25 %** of round 21's total for the pair | untimed, unchanged code; the band is the machine's |
| 7 | `pipeline.teardown_ns`, 100,000 row | **not** above round 21's 484,207,458 by more than 30 % | the row's boundary is not this round's subject |
| 8 | complete command, 100,000 row | **≤ 15 s**, no `DECLARED_EXCEPTIONS` entry | `AGENTS.md` §3.7 |
| 9 | `pipeline.prefix_keys_total` | **66,824** identities, `prefix_keys_largest` **4,221** | §5.1, derived from the pinned chain |
| 10 | `pipeline.prefix_objects_refused` | **0** | the key set must admit everything the snapshot admitted |
| 11 | session anchor `namespace-10000` | inside **±20 %** of **68,514,625 ns** → 54,811,700–82,217,550 | round 21 §2's registered rule, reused |

**Refuted if any of:**

* a pin of either `pipeline.*` row moves — the change reached the measurement, and the change is wrong;
* the lifetime peak does not fall by at least 100 MB (prediction 1's direction is wrong, or the
  snapshots were not what round 21 measured them to be);
* the measured-region increment rises above **48,000,000** (prediction 2's upper bound) — the identity
  sets are not free, and this is where they would show;
* `pipeline.prefix_objects_refused` is non-zero, or `pipeline.prefix_objects_served` differs from
  §5.1's diagnostic by more than the batch composition can explain — a batch was refused an object the
  snapshot would have served it, which is the failure mode this design can have;
* the declared figure lands outside prediction 3's band in the direction of *improvement* while the
  pinned counters say the work was performed — that would mean a reader stopped reading, not that the
  save got faster;
* the anchor lands outside prediction 11's band (the run is then reported as within-session only, not as
  refuted data).

Note the deliberate asymmetry: prediction 1 has a **specific** band because the key-set cost was
measured, and prediction 3 does not, because the row's own within-session spread on that figure is
3.38 % (round 21 §11.6) and a tighter band would be a claim about the machine rather than about the
change.

## 2a. The tolerance registered for the declared figure, and why it is ±10 %

The handoff's step-1 acceptance asks for no pin movement and says the declared figure is "expected to
move by less than the round's own spread", measured in round 21 at **3.38 %** within one session. A
3.38 % band would make the acceptance depend on landing inside a spread that was itself measured once,
and the effect this round must be able to see — a reader that stopped reading, or a save that got
faster because it was handed less — is far larger than 10 %. So the band is registered at **±10 %**:
the loosest bound that is still tighter than any effect this round is meant to permit a comparison of.
The arithmetic is `0.10 × 3,549,393,833 = 354,939,383 ns` either side.

## 3. The instrument this round does not add

`pipeline.rss_phase_*` already exists (round 21 added it) and is published by every run; nothing new is
instrumented. Four counters are added to the same trace and are **not pinned**:
`pipeline.prefix_keys_total`, `pipeline.prefix_keys_largest`, `pipeline.prefix_objects_served` and
`pipeline.prefix_objects_refused`. They exist so the row's own fixture cost is readable from the row
rather than re-derived from a probe, and so a key set that stopped admitting objects fails the row
instead of quietly shrinking it. `g6.prefix-keys` is added to the row's gate list on the same basis: it
requires every batch after the first to be served at least one object **from its own prefix**.

## 4. The runs, in order, one lock window

```text
# the anchor (a tree, no Store) and the two rows, back to back, one lock window
python3 runner.py perf --case namespace-10000           --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-100000 --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-10000  --out <fresh> --verify full
```

One sample per case, fresh `--out` per run, `--verify full`, the harness rebuilding by default. If the
anchor lands outside prediction 11 the session is **not comparable** with round 21's, and every number
is reported as within-session only.

## 5. Declared diagnostics, taken before this file was written

Two **unlocked** invocations of the rebuilt binary were run for a compile, counter and pin check. They
hold no lock, wrote no receipt under `benchmark-results`, are not `--verify full`, and their `--out`
directories are scratch under `/tmp`. They are **not evidence and not samples**; what they are used for
is the one thing a diagnostic may be used for — deciding whether the change is worth registering a
prediction against, and catching a pin movement before a locked run rather than during one.

### 5.1 `pipeline-namespace-100000`, `/tmp/ns22-probe-h`

```text
REBUILT BINARY, before this file was written
pipeline-namespace-100000 PASS gates=12 trace_bytes=30526
all fifteen pins reproduce, including digest:filesystem_root 2412681d…fd954

pipeline.prefix_keys_total          66824      pipeline.prefix_keys_largest       4221
pipeline.prefix_objects_served      19378      pipeline.prefix_objects_refused       0
pipeline.chain_objects               4221      pipeline.content_bytes        502914928

pipeline.rss_phase_baseline_bytes    850329600  (round 21: 1,013,219,328)
pipeline.rss_phase_peak_bytes        889372672  (round 21: 1,050,738,688)
pipeline.rss_phase_incremental_bytes  39043072  (round 21: 37,519,360, +5.0 %)

pipeline.operation_work_ns         3622589833  (round 21 E1: 3,549,393,833, +2.1 %)
pipeline.accept_span_ns            4187592875  pipeline.teardown_ns   565003042
```

and the probe's own measurement of the two shapes, from
`cargo test --release --locked --test namespace_memory_probe` on the same tree:

```text
  batches 25  final chain 4221 objects  key sets 25  identities retained 66824  largest 4221
  heap with the key sets      16,239,225 bytes
  heap with the chain only    12,452,785 bytes
  the key sets cost            3,786,440 bytes  (30.4 % of the chain they serve)
  round 21's snapshots cost  164,347,158 bytes  (13.2 x the chain); the key sets are 2.3 % of that
```

**The key-set cost is measured, not estimated**, which is what makes prediction 1 a band rather than a
direction: `164,347,158 - 3,786,440 = 160,560,718` bytes is what the driver stops holding. Prediction 1
uses 159–164 MB, i.e. the measured saving less the 4 MB a single diagnostic reading can carry.

### 5.2 `pipeline-namespace-10000`, `/tmp/ns22-probe-10k`

```text
pipeline-namespace-10000 PASS gates=12   digest:filesystem_root 1d6fba29…7847 (pinned, reproduces)
pipeline.prefix_keys_total 881   pipeline.prefix_objects_served 1001   refused 0
pipeline.rss_phase_incremental_bytes 26574848   pipeline.operation_work_ns 886007792
```

### 5.3 One diagnostic that is **not** this round's work, and is reported because it was found here

`instruments_selfcheck::the_heap_window_attributes_a_known_allocation_pattern` **fails in `--release`
at the commit this round starts from** (`2f0736f41`) as well as on the changed tree, and **passes in
debug** on both. It is not caused by this round's fixture change — the test file and
`support/instruments.rs` are byte-identical to the parent, and the parent was built in a separate
worktree to confirm it. Cause, measured on the changed tree: the test's 4 MiB buffer and its page
touches are dead code with optimisations on, so the window reads 0 bytes; pinning the buffer with
`std::hint::black_box` makes release pass, and the assertion immediately below it in the same file
already pins its own buffer that way. The fix is one line and is committed separately from the fixture
change, so the two can be read apart. Recorded here because a release-mode failure in the instrument
suite is a fact about the tree this round measures on, whether or not this round caused it.
