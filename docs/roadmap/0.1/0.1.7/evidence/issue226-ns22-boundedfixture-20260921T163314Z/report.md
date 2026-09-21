# Report — #226 round 22: the 100,000-entry row's redundant fixture, removed and measured

Pre-registration: [`pre-registration.md`](pre-registration.md), written before the first locked run of
this round. Raw receipts under [`raw/`](raw/), copied out of the per-worktree receipt store because
`benchmark-results` is not tracked by git. **One sample per case per arm, `--verify full`, fresh `--out`
per run, no receipt retuned. Three cases: the session anchor `namespace-10000` and both `pipeline.*`
rows.**

## 1. The headline

The row's fixture held **164,347,158 bytes of redundant per-batch prefix snapshots**. They are gone, and
the row's lifetime peak fell by **165,412,864 bytes (15.7 %)** with every pin intact.

| | round 21 (`ns21-E1`, before) | this round (`ns22-D2`, after) | change |
| --- | ---: | ---: | ---: |
| status | `PASS` 13/13 | **`PASS` 14/14** | one more gate, §5 |
| **lifetime** peak RSS | 1,050,738,688 | **885,325,824** | **−165,412,864 (−15.7 %)** |
| measured-region baseline | 1,013,219,328 | **848,396,288** | −164,823,040 |
| measured-region **increment** | 37,519,360 | **36,929,536** | −589,824 (−1.6 %) |
| `pipeline.operation_work_ns` | 3,549,393,833 | 3,640,415,749 | +2.6 % |
| `pipeline.accept_span_ns` | 4,033,601,291 | 4,168,435,666 | +3.3 % |
| `pipeline.teardown_ns` | 484,207,458 | 528,019,917 | +9.1 % |
| `pipeline.construct_ns` + `construct_noise_ns` (untimed) | 588,907,769 | 594,445,575 | +0.94 % |
| complete command | 6,446,615,375 | **6,476,722,917** | +0.5 % |
| all fifteen pins | — | **reproduce exactly** | including both filesystem roots |
| session anchor `namespace-10000` | 68,951,625 | 77,814,875 | +12.9 %, inside the ±20 % bound |

**The declared figure moved +2.56 %**, against the **±10 %** tolerance this round registered before the
run (`pre-registration.md` §2a) and round 21's measured within-session spread of 3.38 %. The
measured-region increment moved **−1.6 %** against a registered band of ±20 %.

**Both figures are published, as the handoff §5.4 requires**: the lifetime peak *and* the
measured-region increment. A row reporting only one of them cannot be read for this question — the
lifetime peak is where the fixture is, and the increment is where the product is.

## 2. What was changed, in one paragraph

`prefixes` kept one `TreeStore` snapshot of the reader chain per batch, and `TreeStore::absorb`
**copies** (`workload/providers.rs:103`), so the driver retained the sum over batches of the chain so
far: 25 snapshots holding 66,824 objects to serve a chain whose final state is 4,221 objects. A batch is
owed the chain *as it stood before it* — a statement about which objects a reader may serve, not about
holding a second copy. `PrefixProvider` now holds the one complete chain and admits only the identities
that batch's prefix contained (`providers.rs:254`, `:299`); an identity outside it returns the same
`ContentError::MissingObject` a snapshot lacking the object returns, so the object set is identical and
the failure is identical. The identities cost 3,786,440 bytes where the copies cost 164,347,158.

## 3. Pre-registration, scored as written

| # | registered | measured | verdict |
| --- | --- | --- | --- |
| 1 | lifetime peak falls by 159–164 MB, to 886,738,688–891,738,688 | **−165,412,864** to **885,325,824** | **direction right, band missed by 1.4 MB** — see §3a |
| 2 | increment within ±20 % of 37,519,360 (30,015,488–45,023,232), ≤ 48,000,000 | **36,929,536** (−1.57 %) | **fired** |
| 3 | `operation_work_ns` within ±10 % of 3,549,393,833 | 3,640,415,749 (+2.56 %) | **fired** |
| 4 | fifteen pins, 100,000 row, reproduce | **15/15**, `2412681d…fd954` | **fired** |
| 5 | fifteen pins, 10,000 row, reproduce | **15/15**, `1d6fba29…7847` | **fired** |
| 6 | `construct_ns` + `construct_noise_ns` within ±25 % of round 21's 588,907,769 | 594,445,575 (+0.94 %) | **fired** |
| 7 | `teardown_ns` not above 484,207,458 by more than 30 % | 528,019,917 (+9.05 %) | **fired** |
| 8 | complete command ≤ 15 s, no `DECLARED_EXCEPTIONS` entry | 6.477 s, `declared_exception: false` | **fired** |
| 9 | `prefix_keys_total` 66,824, `prefix_keys_largest` 4,221 | **66,824 / 4,221** | **fired** |
| 10 | `prefix_objects_refused` 0 | **0** | **fired** |
| 11 | anchor inside ±20 % of 68,514,625 (54,811,700–82,217,550) | **77,814,875** (+13.57 %) | **fired** |

Refutation conditions: **none fired.** No pin moved; the peak fell by more than 100 MB; the increment
stayed under 48,000,000; nothing was refused; the declared figure did not move toward *improvement*
while the counters said the work happened; the anchor landed inside its band, so this session **is**
comparable with round 21's for the rows measured in it.

### 3a. The one prediction that missed its band, reported plainly

Prediction 1 registered **159–164 MB** and the measurement is **165.4 MB** — 1.4 MB outside the top of
the band, in the direction the prediction called for. The band was built as round 21's measured snapshot
cost (164,347,158) minus this round's measured key-set cost (3,786,440), i.e. 160,560,718 bytes, widened
to 159–164 MB to carry up to 4 MB of single-reading error. The overshoot is **bigger than that margin**,
and the honest statements are:

* the **key-set cost is measured** (3,786,440 B, `namespace_memory_probe`, §4.2) and is not the source
  of the difference;
* the **snapshot cost is round 21's number**, taken on a *different binary* — before the RSS sampler
  existed and before this round's two other harness edits. 164,347,158 B is a measurement of the old
  shape by the old probe, and the difference between it and this round's saving is 4,852,846 B (2.9 %);
* so the row's own fixture cost fell by **165.4 MB**, and the registered band's top end was **164 MB**.
  The prediction understated the saving by 1.4 MB because it carried round 21's snapshot measurement as
  an exact constant rather than as a measurement with an error bar. **That is a defect in how the band
  was registered, not a result that contradicts the direction** — and it is recorded here rather than
  rounded away.

## 4. The evidence

### 4.1 Identity — one binary, three cases, one lock window

| | `ns22-D1` anchor | `ns22-D2` 100,000 row | `ns22-D3` 10,000 row |
| --- | --- | --- | --- |
| case | `namespace-10000` | `pipeline-namespace-100000` | `pipeline-namespace-10000` |
| harness binary sha256 | `d6c1d363c9f8471f…` | `d6c1d363c9f8471f…` | `d6c1d363c9f8471f…` |
| source commit | `47cf17049a27ab3ff2ebb77a7cae449eed8821e5` | same | same |
| `source_dirty` | **false** | **false** | **false** |
| started (UTC) | 16:39:55Z | 16:39:57Z | 16:40:05Z |
| lane / samples | `smoke` / 1 | `smoke` / 1 | `smoke` / 1 |
| verification mode | `full` | `full` | `full` |
| `runner.py verify` | 0 disagreements | 0 disagreements | 0 disagreements |
| status | `PASS` 10/10 | **`PASS` 14/14** | **`PASS` 14/14** |

`core/crates/**` and `core/*/sql/**` are byte-identical to `5be4b7ae0`. `runner.py self-check`
**passes** on this tree, registry rung included.

**One reconciliation, because the receipts cannot be edited.** Their `source_commit` is
`47cf17049a27ab3ff2ebb77a7cae449eed8821e5`, and that hash is no longer in the branch's history: the
commit that filed the receipts was replayed during this round's review so that the round's history has
one commit per coherent change instead of a commit that added one run set and a second that removed it.
The receipt's own `source_dirty` is **false** and the tree it names is **identical** to this report's
tree — `c3e28f63ef3ee15f21753c4ebb7c2287b34baffc`, the tree of every commit from `cbbae1ee8` onward —
so the identity a reader needs (the measured tree) is unchanged and the hash a reader sees in the
receipt is the hash that tree had when it was measured. Nothing in `raw/` was retuned.

### 4.2 The fixture's cost, measured two ways

**From the row's own counters** (`ns22-D2`, unpinned, published for this question):

```text
pipeline.chain_objects            4,221      pipeline.prefix_keys_largest     4,221
pipeline.prefix_keys_total       66,824      pipeline.prefix_objects_served  19,378
pipeline.prefix_objects_refused       0      (round 21: 25 snapshots, 66,824 objects retained)
```

**From the probe** (`cargo test --release --locked --test namespace_memory_probe -- --nocapture`), which
now builds the shape the driver builds:

```text
PREFIX KEYS
  batches 25  final chain 4221 objects  key sets 25  identities retained 66824  largest 4221
  heap with the key sets      16,239,225 bytes
  heap with the chain only    12,452,785 bytes
  the key sets cost            3,786,440 bytes (30.4 % of the chain they serve)
  round 21's snapshots cost  164,347,158 bytes (13.2 x the chain); the key sets are 2.3 % of that
```

The probe's part 1 no longer keeps the dead snapshot shape alive to print a comparison: it runs the
driver's own two pieces and carries round 21's figure as a labelled constant.

### 4.3 The measured region did not move

| | round 21 `ns21-D2` (10k) | this round `ns22-D3` (10k) | round 21 `ns21-D1` (100k) | this round `ns22-D2` (100k) |
| --- | ---: | ---: | ---: | ---: |
| canonical bytes | 301,171,810 | 301,171,810 | 502,914,928 | 502,914,928 |
| measured-region increment | 27,197,440 | **27,262,976** | 37,519,360 | **36,929,536** |
| increment / canonical | 9.03 % | 9.05 % | 7.46 % | **7.34 %** |

And the `heap.peak_incremental_bytes` the row has published all along moves by **95 bytes** between
round 21's `ns21-E1` and this round's `ns22-D2`: 27,557,611 → 27,557,706 (+0.0003 %). **The product was
not touched**, and the two independent memory instruments agree about it.

## 5. The pin set, and the two rows in it

| | 100,000 row | 10,000 row |
| --- | --- | --- |
| counter pins | 14 | 14 |
| identity pins | 1 (`2412681d335571082c4dfbc1df2117bc015b7f6132fca17cb0d11bbeaaefd954`) | 1 (`1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`) |
| reproduced | **15 / 15** | **15 / 15** |
| gates | 14, all `PASS` | 14, all `PASS` |

`g1.o3-pinned-counters` and `g1.o1-pinned-identity` are what make the pin set a gate rather than a
comparison a reader performs. The row gained one gate, `g6.prefix-keys`, which requires every batch
after the first to be served at least one object from its own prefix — the reading that would catch a
provider that stopped admitting objects while the counters still looked right.

## 6. What was run

```text
# one lock window, one rebuild, three cases, one sample each, fresh --out each
python3 runner.py perf --case namespace-10000           --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-100000 --out <fresh> --verify full
python3 runner.py perf --case pipeline-namespace-10000  --out <fresh> --verify full

# re-derivation, from the raw artifacts
python3 runner.py verify --run <each run directory>          # 0 disagreements, call graph PASS over 194 files

# the fixture's own cost, and the instrument self-checks
cargo test --release --locked --test namespace_memory_probe -- --nocapture
cargo test --release --locked                                # 133 tests, all passing
cargo +1.85.1 clippy --release --locked --all-targets        # no new findings in the changed files
python3 runner.py self-check                                 # PASS
```

**What was not run, and why.** No product test suite (`cargo test --manifest-path core/Cargo.toml`):
`core/crates/**` is byte-identical to `5be4b7ae0` and the round's change is inside the harness, so the
product suite would verify a tree this round did not touch. No `--reuse-pass`: every row was measured.

## 7. Superseded runs, named, with the reason

Three other sets — the anchor plus both `pipeline.*` rows in each, three cases per set — were taken this
round and are **on disk but not in `raw/`**. They are listed because a report that hides its discarded
attempts is worse than one that lists them. The acceptance set is `ns22-D1…D3`.

| set | where | why it is not the result |
| --- | --- | --- |
| `ns22-A1…A3` | receipt store, deleted | the `--out` path was relative, so it resolved against the harness directory and left an untracked directory inside the tracked tree. Two of the three receipts record `source_dirty: true` — the harness's own identity said the tree was not clean, which is correct and disqualifying |
| `ns22-B1…B3` | this worktree's receipt store, `core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/` | `source_dirty: false`, pins intact, taken on binary `e386225e9371a374…` — the same code as the acceptance runs, **before** a six-line comment-only clippy allowance. A comment-only edit changed the binary hash (§8.1), so a run whose binary is not the current tree's binary is not the acceptance evidence |
| `ns22-C1…C3` | the same receipt store | `source_dirty: true`: the evidence directory these receipts now live in was still untracked when they were taken |

The B and C sets are not lost and are not hidden: they agree with the acceptance set to within **1.2 %**
on the declared figure and **0.3 %** on the lifetime peak, which is worth saying because it is
independent evidence that the change is stable across three consecutive windows. Their numbers are in
§8.2, where they are used for the question they can answer.

## 8. Two findings this round did not go looking for

### 8.1 A comment-only edit produces a different harness binary

`e386225e9371a37461d3f62b195b61bd21582ad75cbaa67ad42085c0b873d361` and
`d6c1d363c9f8471fb0d96fb32d1e6522953ce489a195ec8ab7af3e8da250735e` are the **same code**. The only
difference is a six-line doc comment and an `#[allow(clippy::len_without_is_empty)]` attribute added
after the B runs. Every receipt carries the binary's sha256 as part of its identity, so two runs of
identical code can report different identities, and a reader comparing runs by hash is comparing builds
rather than sources. **Nothing here is new policy** — `AGENTS.md` §3.3 already says a rebuilt artifact
needs a rebuilt matched arm — but this is a concrete case of a rebuild with no source change at all, and
it is the reason this round re-ran its acceptance set rather than reuse the B set.

### 8.2 The save's connection close swings 8.8× inside one session

| run | `teardown_ns` | `span_finish_ns` | `span_finish_ns − teardown_ns` | `operation_work_ns` | `accept_span_ns` | `phases.operation_ns` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ns22-B2` | 60,190,625 | 72,939,000 | 12,748,375 | 3,642,802,292 | 3,702,992,917 | 3,705,821,041 |
| `ns22-C2` | 60,392,959 | 72,153,667 | 11,760,708 | 3,685,116,916 | 3,745,509,875 | 3,748,415,666 |
| `ns22-D2` (**acceptance**) | **528,019,917** | **540,065,458** | 12,045,541 | 3,640,415,749 | 4,168,435,666 | 4,171,219,416 |

`teardown_ns` — the save's connection close, which the row's declared formula **excludes** — moved
**467,829,292 ns (8.77×)** between two runs of the same binary in the same lock window. The declared
figure moved **44,701,457 ns (1.2 %)**, because `operation_work_ns` is `accept_span_ns − teardown_ns`
and both terms moved together. The difference `span_finish_ns − teardown_ns` is stable to **8.4 %**
across all three, which is evidence — labelled as evidence, not proof — that the connection close is
nested inside `span_finish_ns`.

**Why this matters beyond this round.** The handoff's option A would put a fixture read inside the
accept span. That span is the region whose terms swing by hundreds of milliseconds between runs in one
window, and its one stable part is the declared figure. A round that measures inside it must publish
the split, which is what the design page's §4.5 asks for.

## 9. The design page

[`issue226-bounded-fixture-design.md`](../../issue226-bounded-fixture-design.md) prices the four options
the handoff names, recommends **A** (spill the constructed objects and stream them back through a bounded
window inside the timer) with **C** as the fallback, and **stops at a recommendation**:

* **A and D need an owner ruling; B and C do not.** The ruling request is written as three questions in
  the page's §7, not decided there. A is preferred over C because A is what the reference harness does
  (`benchmark/fs-bench-pro/workload/main.rs:120`, `:936-948`) and because C pays the C1 tree build twice
  to avoid a read the row would have to declare anyway.
* **The contract that decides it** is the page's §4: declaration, enforcement through
  `shared/residency.py:154`'s `msync(MS_INVALIDATE)` plus `mincore`, device attestation through
  `gates::device_attestation` (`gates.rs:382-405`) and `process_usage().disk_read_bytes`
  (`support/instruments.rs:346`), equal treatment of both arms, and the publication of a
  `spill_read_ns` split beside `operation_work_ns`.
* **No product change is proposed and none was made.** The page's prices are surfaces and measured
  inputs; its ~120 MB peak for A is labelled arithmetic, not measurement.

## 10. Production LOC

| commit | what | production LOC |
| --- | --- | --- |
| `c224b04ce` | the prefix keys, the four counters, the gate, three provider tests, the probe's part 1 | `97100 -> 97100 (delta 0)` |
| `861caf949` | the release-mode `instruments_selfcheck` fix (one `black_box` line) | `97100 -> 97100 (delta 0)` |
| `2536e0b54` | the pre-registration | `97100 -> 97100 (delta 0)` |
| `47cf17049` | the raw receipts | `97100 -> 97100 (delta 0)` |
| `cbbae1ee8` | the clippy allowance | `97100 -> 97100 (delta 0)` |

Every commit is a **harness-only** commit: `tools/production_loc.py --root .` reports the unchanged
total (core 31683 in 194 files, reference 65417 in 193 files) and the delta is 0 because nothing in the
production scope changed. **This is not a claim that the round did no work**; it is what the counting
rule says about a round whose entire edit is inside `core/benchmark/**`, which
`core/tools/check_product_boundary.py` scans and does not count.

## 11. What must not be redone

| closed | why |
| --- | --- |
| **the redundant prefix snapshots** | removed and measured here; 164,347,158 B → 3,786,440 B; fifteen pins intact on both rows |
| **the product's measured-region memory** | 27.3 MB / 36.9 MB and 7.3–9.1 % of canonical bytes, falling per byte; unchanged by this round (§4.3) |
| **the v0.1.6-vs-replacement memory comparison** | the pair cannot be formed; the reference harness publishes no per-region figure for `init_namespace`. The one valid comparison is fixture residency (~500×) and it is a **harness** difference |
| **round 21's row, declaration, pins, anchor** | registered and measured; the declaration is the reference's and is not a lever |
| **the pack-capacity lever** | refuted on a matched pair and reverted (`8efc798de`, ledger L80) |
| **the row's time boundary** | not moved this round; option D would move it and that is an owner ruling |
| **the registry cardinality defect** | fixed in round 21 (`pipeline.*` `4 → 6`); `runner.py self-check` passes here |
| **option A, B and C prices** | priced in the design page §3 from measured inputs; do not re-derive them from the handoff's prose |
