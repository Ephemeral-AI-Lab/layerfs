# Handoff prompt — #208 the chunk-predecessor mechanism's time cost: measure the price of a measured space win

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after `d05b80bba`. This prompt carries existing
> measurements and one bounded experiment; it is **not** a new measurement, not a
> design freeze, and not a release claim.

## Mission and decision rule

Continue [#208](https://github.com/Ephemeral-AI-Lab/layerfs/issues/208). The
chunk-predecessor mechanism (`LAYERFS_HISTORY_CHUNK_PREDECESSORS`, introduced by
`795fb1a2f`, closes #186) **defaults ON because it was measured and it wins on space**.
Its **time cost has never been measured on any row**. `795fb1a2f` is explicit:

> *"No timing claim is made anywhere in this commit. The machine was not quiet (load ~5)
> for the lane runs; bytes and RSS are load-independent and stand."*

**Run the three-row, two-arm experiment (§3) and then state a judgement.** The order is
fixed: measure the price on every row before anyone argues about whether to pay it.

Do not change the default, the switch's semantics, or the mechanism. This is a
**measurement** round: the deliverable is a two-axis table and a judgement, not a product
change. If the measurement says the default is wrong, say so and leave the change to an
owner ruling.

## 1. The correction this issue exists to record — read this first

An earlier reading in this lane stated: *"declining prior-state bases cut the stride10
operation 33 %, at the cost of a larger Store."* **The causality was backwards.**

`CHUNK_PREDECESSORS=0` *"reproduces the arm every earlier measurement on this lane was
taken on, byte for byte"* (`ops/history.rs:293`). So:

| arm | what it is | Store | time |
| --- | --- | --- | --- |
| **ON** (shipped default) | the #186 feature | **smaller** | slower |
| **OFF** (`=0`) | the pre-#186 behaviour | larger | faster |

The control does not *remove a cost*; it *removes a feature*. The question is therefore
**"how much time does the space win cost, and is that price worth paying?"** — not
"can we save time by declining bases".

## 1a. WITHDRAWN: the 33 % time price was a cross-binary measurement artifact

**The single most important thing in this prompt.** The earlier claim — *"declining
prior-state bases cut the stride10 operation 33 % (16.296 → 10.882 s)"* — is **not a
measurement of the feature**. It compared two runs from **different binaries**:

| run | binary sha256 | built from |
| --- | --- | --- |
| `nopred-history-stride10` (OFF, 10.882 s) | `441099a0c3af7c63…` | `6ee45af92` |
| `cp-off-history-stride10` (OFF, 16.150 s) | `418ee5085664a696…` | `d05b80bba` |

Between those two builds, `core/crates/layerfs-telemetry/src/timer` changed by **171 lines**
(`RecordingLimits` added to `Recording::start`, `limits.rs`, `report.rs`, `scope.rs`), and:

- `operation_ns` is defined as *"the sum of this row's named children"* — i.e. it is built
  **entirely from telemetry's timer**, the exact component that changed;
- the new recording limits **omit children while still executing them**, so a different limit
  set yields a different sum for identical work.

The signature confirms it: the cross-binary ratios are near-identical across independent
quantities — operation **0.674**, scope **0.669** — which is a uniform scaling, not a
phase-specific effect.

### The only valid comparison is same-binary

| arm | binary | operation |
| --- | --- | ---: |
| `cp-on-history-stride10` | `418ee508…` | 16.463 s |
| `cp-off-history-stride10` | `418ee508…` | 16.150 s |
| | | **Δ = 0.313 s (1.9 %)** |

**So the feature's stride10 time price is ~2 %, not 33 %** — and 0.313 s is within the
0.05–0.85 s reproducibility band this lane has observed across arms, so even that is not
established. **One sample per arm; do not quote the 1.9 % as a result either.**

`invocation_ns` (the harness's own wall clock, not telemetry) also fell 25 % between the two
sessions (33.083 → 24.780 s), which a timer change cannot explain and which remains
**unidentified**. It means the two sessions differed in machine conditions as well, so the
cross-binary comparison is confounded twice over.

**Consequence for this round's design: sample all six arms from ONE binary.** The §3 table
below already requires it; §1a is why.

## 2. What is already measured (do not re-derive)

### The space win, from `795fb1a2f` (the reason the switch defaults ON)

| row | states | apparent | v0.1.6 | ratio |
| --- | ---: | ---: | ---: | ---: |
| `history-stride10` | 17 | 49,053,696 | 49,315,840 | 0.99468 |
| `history-stride3` | 53 | 61,767,680 | 64,000,000 | 0.96512 |
| `history-stride1` | 157 | 80,273,408 | 82,685,952 | 0.97082 |

All three **below** the v0.1.6 reference. The registered lane fell **128,864,256 →
49,053,696, −61.9 %** (2.6130× → 0.99468×). The enabling measurement recorded on the
switch itself: **51,347,456 → 49,672,192 B apparent (−1,675,264)**; the native lane fell to
3,957,829 B, **228 B below v0.1.6's own 3,958,057 B**, with the selection becoming
byte-identical to v0.1.6's (448 FULL / 650 PREFIX). `delta.trials` rose 37,886 → 38,538 —
the first chunk trials this lane has ever run.

**So the feature is the reason this lane now sits below v0.1.6 on bytes.** That standing is
the stake; if it is a requirement, the time price is not optional regardless of its size.

### ~~The time price, stride10, measured~~ — WITHDRAWN, see §1a

The figure that stood here (ON 16.296 s vs OFF 10.882 s, "Δ = 5.414 s, 33 %") is
**withdrawn**. It compared two different binaries whose telemetry timers differ. See §1a.

### The space cost of the OFF arm, measured

| | ON | OFF | delta |
| --- | ---: | ---: | ---: |
| apparent | 49,324,032 B | 51,040,256 B | **+1,716,224 (+3.48 %)** |
| allocated | 50,249,728 B | 51,298,304 B | +1,048,576 (+2.09 %) |
| pack BLOB bytes | 45,297,954 B | 47,036,475 B | +1,738,521 (+3.84 %) |
| packs created | 255 | 262 | +7 |
| objects | 52,032 | 52,032 | 0 |
| value groups | 1,737 | 1,737 | 0 |

**Caveat:** the allocated axis carries a documented **±1.5 % band** (the scaling handoff
recorded allocated bytes moving 1.5 % between campaigns *for Stores that hash identically*).
Quote **apparent** for this claim; treat allocated as corroborating, not independent.

### Partial results from this round — read the warning

Three arms were sampled from one binary (`418ee5085664a696055d65aadba3d19bbd0acc4134dce81e1551c8a37c045bcf`)
before the round was stopped. **They are not a verdict and one is suspect:**

| arm | operation | status |
| --- | ---: | --- |
| `cp-on-history-stride10` | 16.463 s | complete, exit 0 |
| `cp-off-history-stride10` | **16.150 s** | complete, exit 0 — **see warning** |
| `cp-on-history-stride3` | 36.281 s | complete, exit 0 |
| `cp-off-history-stride3` | — | **INCOMPLETE, aborted mid-run** |

> **Resolved by §1a, and the resolution removes the headline.** The 5.3 s disagreement was a
> **binary difference**: the 10.882 s sample came from `441099a0` (telemetry timer before the
> `RecordingLimits` change) and the 16.150 s sample from `418ee508` (after). `operation_ns` is
> the sum of telemetry-recorded children, so the two are not comparable. **The valid
> same-binary delta is 0.313 s (1.9 %), not 5.414 s (33 %)** — and even that is one sample
> per arm and must not be quoted as a result. Re-measure all six arms from one binary.

## 3. The experiment to run

One switch, two arms, three strides, **both axes on every row**:

| row | states | arm | switch | status |
| --- | ---: | --- | --- | --- |
| `history-stride10` | 17 | ON | default | **re-measure** (see warning) |
| `history-stride10` | 17 | OFF | `LAYERFS_HISTORY_CHUNK_PREDECESSORS=0` | **re-measure** |
| `history-stride3` | 53 | ON | default | sampled, keep |
| `history-stride3` | 53 | OFF | `LAYERFS_HISTORY_CHUNK_PREDECESSORS=0` | **run — aborted** |
| `history-stride1` | 157 | ON | default | **run** |
| `history-stride1` | 157 | OFF | `LAYERFS_HISTORY_CHUNK_PREDECESSORS=0` | **run** |

**Reported per row:** operation ns; `content` / `filesystem` / `accept_loop` node ns;
apparent bytes; allocated bytes; pack BLOB bytes; packs created; object count; and the
harness's own gate outcome.

**Why stride1 matters most.** Reuse volume dominates there — 121,301 reuse occurrences,
16.544 s, 34.6 % of scope — so if the cursor's per-chunk base resolution grows with history,
stride1 is where it shows. The space cost should also be largest there. **A stride10-only
verdict is the wrong basis for a policy that ships on all three rows.**

## 4. Two limits the experiment cannot remove

1. **The switch is confounded by construction.** `=0` disables *both* the predecessor
   offering **and** the chunked-construction path that consumes it. The measured delta is the
   cost of the whole #186 chunk-cursor mechanism, not of base resolution alone. Separating
   them needs a narrower switch; **do not build one in this round** — record it as a
   limitation.
2. **The OFF arm's Store is not byte-identical** to the recorded constant, by declaration.
   Byte-identity is not a criterion for the OFF arm and its absence is not a defect. The
   harness's own gates still apply and must be reported as they come out.

## 5. What the judgement needs

- The **time price per row** (this experiment).
- The **space saving per row** (§2 plus this experiment).
- Whether the time cost **grows with history** faster than the space saving does. Flat cost +
  growing saving is comfortable; growing cost means a history length beyond which the trade
  inverts — state that point as a finding rather than leaving it implicit.
- **Whether the space win is still required.** It is what took the lane below v0.1.6 on
  bytes. If that is a standing requirement, the price is not optional.

## 6. Owner rulings that apply

- **One second of stride10 operation reduction is worthwhile.** A price ≥ 1 s on stride10 is
  not a rounding error and needs an explicit ruling.
- **A small allocated-storage overage is acceptable when accompanied by good time
  reduction.** Here the axes point in **opposite** directions — time up, space down — so the
  ruling's premise does not apply directly and an explicit decision is required rather than
  an inference from it.
- **Report both axes; never relabel a historical miss as a pass.**
- **No automatic cache growth**; no invented cold stance; no pre-touched inputs; no shrunk
  selection; no re-run for a better number.
- **Do not change the shipped default** on the strength of this round alone.

## 7. Measurement protocol

One sample per case per arm; fresh `--output` per run; receipts append-only; failures,
deferrals and incomplete runs retained on disk. Both global flocks held for every resource
command (`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`)
— a held lock defers, never waits. Quiet preflight: no named `cargo`/`rustc`/`fs-bench`
competitor and ≥ 70 % CPU idle on the second of two one-second observations; a busy preflight
consumes no sample. **Watch for your own build's `cargo` process outliving the build** — this
round's first stride10 attempt was deferred for exactly that reason.

Rust 1.85.1, `--locked`, `LAYERFS_CONSTRUCTION_WORKERS=1`, one construction worker, no second
lane. Diagnostic caps unchanged and not to be promoted: **120 s stride10 / 240 s stride3 /
720 s stride1**. Sample all six arms from **one binary** so the comparison is internally
consistent, and record its sha256 with each row.

## 8. Deliverable

A three-row, two-axis table with the mechanism's time price stated per row; the stride10
discrepancy in §2 resolved rather than inherited; a judgement on whether the shipped default
is right; and, if the price grows with history, the crossover point stated as a finding.

Append an entry to the active ledger (next free is **L54**) and update #208. **Do not close
#186, #190, #205 or #208.**

## 9. Do not

- Do not treat the §2 partial arms as a verdict; the stride10 OFF disagreement must be
  resolved first.
- Do not flip the shipped default.
- Do not build a narrower switch to decompose the confound in this round.
- Do not quote the allocated axis without its ±1.5 % band.
- Do not present any `history.*` row as admission evidence — all are diagnostics, admission
  `INELIGIBLE`, O3 `INCOMPLETE`, every budget class `NOT_RUN`.
- Do not hand-edit a pin; `shared/pin_expected.py` is the only way a constant freezes.
- Do not use `tools/preflight.sh` or restore CI.
