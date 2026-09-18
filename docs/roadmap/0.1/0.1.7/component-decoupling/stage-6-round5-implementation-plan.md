# Stage 6 round 5 — implementation plan for [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)

> **Status:** Plan; target LayerFS v0.1.7. Companion to
> [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184), which is the
> round-5 assignment. It is not a receipt and it contains no measurement of its own:
> every figure below is quoted from the round-3b run at
> [`../evidence/stage-6-round3b-20260919T000000Z/`](../evidence/stage-6-round3b-20260919T000000Z/README.md).
>
> **Scope:** `core/benchmark/**` plus harness documentation. **No product source
> change.** If any step below appears to need one, stop and report it rather than
> widening the product API.

## 1. The two defects, and the mechanism that fixes each

| | Defect | Fix |
| --- | --- | --- |
| **A** | Every `perf` child rebuilds its fixture from the recipe, so the campaign pays full construction on every run | One immutable prepared master per compatibility digest, byte-copied per case and destroyed after it |
| **B** | The published and budgeted "time" is the process wall, not the measured operation | Publish `operation_ns` (telemetry) and `complete_command_ns` (wall) as separate fields; derive the report's time axis and any performance statement from `operation_ns` |

The two are independent. **B is the smaller change and it is a prerequisite for
measuring A**, because until the operation time is published per row there is no
way to show that A helped.

## 2. What the plan must not do

- No product source change, no product API addition, no hook, no feature flag.
- No warm-cache credit. A prepared master is **preparation reuse**, never a cold
  claim, and never a reason a timed phase may skip work it would otherwise do.
- No workload shrunk, tier cut, limit relaxed, timeout inflated, worker added or
  case dropped.
- No aggregate gate, no CI workflow, no wrapper. `tools/preflight.sh` stays retired.
- Round-3 and round-4 receipts are not edited; a round-5 run goes into a new directory.

## 3. Feasibility — checked before planning, not assumed

### 3.1 The existing persistence is lossy and cannot be used as-is

`TreeStore::write_to_dir` (`workload/providers.rs:118`) writes only
`object.canonical()`, and `load_from_dir` (`:137`) re-wraps **every** object as
`ObjectRole::Chunk` with no references and no predecessors. Its own comment says so:
*"`load_from_dir` therefore serves C1 reads and the `--emit-objects` round trip, and
a C2 replay uses the in-process store instead."*

That is unusable for a C2 fixture. A save chooses its lane from the role
(`PackLane::for_role`), so a mapping page loaded as `Chunk` lands in the Native
lane; and the delta path reads the advisory predecessors as its candidate hints.
A master round-tripped through this path would store different bytes and produce
different counters than a built one — a silent fixture change, which is exactly
what the preparation rules exist to prevent.

**Therefore a lossless object-set serializer is required.** Round 4 has since
written one — `src/workload/artifact.rs`, whose header states this exact finding
(*"The older `TreeStore::write_to_dir` round trip keeps only the bytes and re-wraps
them as chunks, which is why a C2 replay could not use it"*) — so this is no longer
new code to write. See §12.2.

### 3.2 Everything the serializer needs is already public

| Need | Public API | Where |
| --- | --- | --- |
| All four fields at once | `FinalizedObject::into_parts() -> ObjectParts { id, role, canonical, references, predecessors }` | `object/output.rs` |
| Rebuild | `FinalizedObject::new(role, canonical)?.with_references(..).with_predecessors(..)` | `object/output.rs` |
| Role code | `ObjectRole::code()` / `ObjectRole::from_code(u8)` | `object/output.rs:47,66` |
| Predecessor list | `AdvisoryPredecessors::{new, entries, push, len}` | `object/predecessor.rs` |
| One predecessor | `AdvisoryPredecessor::{id, provenance}` | `object/predecessor.rs:36,41` |
| Provenance | `PredecessorProvenance` — three variants, mapped harness-side | `object/predecessor.rs:18` |

The harness already calls `code()`/`from_code()` on `InodeKind`
(`ops/fs_fixture.rs:364,429`), so this is established practice, not a new reach.

**Conclusion: the whole of #184 is implementable inside `core/benchmark/**`.** No
product change is required, and none may be made.

### 3.3 What already works and should be reused rather than rebuilt

| Piece | Where |
| --- | --- |
| `prepare` verb, "acquires the prepared artifacts a selection needs, once" | `runner.py:183` |
| Compatibility-keyed prepared root, `layerfs-prepared-manifest-v1` | `runner.py:185-192` |
| Immutable-master serialisation for a C1 filesystem input | `--emit-input` / `--load-input`, `ops/fs_fixture.rs:467,475` |
| Byte-copy rung R2 of a closed Store | `ops/c2.rs` `prepare_sample`, `COPY_RUNG` |
| Store manifest and per-file hashing | `runner.py` `manifest.json` |

## 4. Where the time actually is

Round-3b, `PASS` rows only, seconds. "non-op" is `wall − operation`:

| family | rows | wall | operation | non-op |
| --- | ---: | ---: | ---: | ---: |
| `c1.edit.length-changing` | 32 | 55.23 | 0.004 | **55.22** |
| `c2.reuse.workspace` | 14 | 38.06 | 7.011 | **31.05** |
| `c2.delta.cdc-locality` | 15 | 32.42 | 5.591 | **26.83** |
| `c2.reuse.cross-file` | 10 | 30.02 | 5.672 | **24.35** |
| `c1.cdc.chunk-count` | 12 | 21.61 | 0.003 | **21.61** |
| `c1.edit.length-preserving` | 12 | 21.03 | 0.002 | **21.03** |
| `c2.read.waves` | 4 | 11.80 | 0.004 | 11.80 |
| `c1.construct.chunked` | 4 | 7.92 | 0.851 | 7.07 |
| `c1.construct.whole-file` | 4 | 7.88 | 0.852 | 7.03 |
| `c2.footprint` | 6 | 22.46 | 19.134 | 3.33 |
| everything else (9 families, 63 rows) | 63 | 0.65 | 0.030 | 0.62 |
| **total** | **150** | **249.08** | **39.151** | **209.93** |

Plus the 26 `NOT_RUN` rows: **145.22 s** of wall for ~24 s of operation, of which
the five `dedup-cdc-*-500` rows are ~143.6 s.

The lane is **394.57 s** for **~39 s** of measured operation. **This table is the
work order.** `c1.edit.*` and `c1.cdc.chunk-count` are pure preparation plus
verification — their operation is 4–9 ms; `c2.reuse.*` and `c2.delta.*` are
preparation plus verification around a real 5–7 s operation; `c2.footprint` is
mostly real work and is not a target.

## 5. Phase 0 — make the numbers honest (mechanical, no cache)

Nothing here needs the prepared cache, and it is what makes Phase 2 falsifiable.

**0.1 — every row publishes its operation time.** Add `write_timing` to the
`ops/fs.rs` drivers (`run_build_row`, `run_traverse_row`, and the locality / tiny /
namespace drivers), so all 220 rows write `timing.json` and a
`timing_json_bytes` counter. Today 44 of 194 `PASS` rows publish neither.

**0.2 — the child marks its own phases.** Add a small `PhaseClock` to
`support/instruments.rs` with three markers — preparation, measured, verification —
and have each driver call them. Write them as trace counters
`phase.preparation_ns`, `phase.measured_ns`, `phase.verification_ns`. Three calls
per driver, mechanical.

**0.3 — the receipt publishes four separate numbers.** In `shared/receipt.py` and
`runner.py:run_case`:

| field | source |
| --- | --- |
| `operation_ns` | `timing.json` root `elapsed_ns` |
| `preparation_wall_ns` | `phase.preparation_ns` (0 once a master is reused) |
| `acquisition_wall_ns` | `phase.acquisition_ns` — the per-sample copy and de-warm, which D2 already makes mandatory and which today appears nowhere but `shared/copyladder.py:121` |
| `complete_command_ns` | the process wall, as now |

**0.4 — `verify` re-derives all four** and fails closed: if
`operation + preparation + acquisition + verification` does not reconcile with
`complete_command_ns` inside a declared tolerance, the row is `INCOMPLETE`, not
quietly wrong.

**0.5 — the report's time axis becomes `operation_ns`.** `shared/analyze.py:245`
currently prints `max(row.wall_ns) / 1e6` under the header `time max ms`. Change it
to `operation_ns` and label the column so it cannot be read as the wall. Add the
wall, and the non-operation share, as separate columns.

**0.6 — report the lane's share.** Add the operation-vs-wall totals to the report
header so the before/after is one line.

**Verification:** re-run the lane; expect ~39 s of operation inside ~394 s of wall,
220/220 `timing.json`, `verify` reconciling every row, and the five budget rows
still `NOT_RUN` at 15 s — Phase 0 changes what is *published*, not what is
*budgeted*. If a budget verdict moves, something is wrong.

## 6. Phase 1 — the prepared cache

**1.1 — the lossless object-set serializer already exists.** Round 4 wrote
`src/workload/artifact.rs`: it records the persisted role code beside each object,
persists each member's `Expectation`, and provides `copy_store`, `directory` and
`seal`. **Do not rewrite it.** The round-5 work here is to (a) confirm it
round-trips references and advisory predecessors — the role is handled, the
predecessor list is not visible in its header — and (b) extend its use to every
fixture-heavy family, since today only `c2.delta.cdc-locality` declares
`oracle_phase: verify-invocation`.

One deliberate divergence from the layout this plan first proposed: the frozen
specification already fixes the artifact layout in
`test_setup_and_cache_discipline.md` §3 —

```text
prepared/<compatibility-digest>/
  manifest.json   {compatibility, producer, created_ns, files{path -> {bytes, sha256}}, data_bytes}
  inputs/         fixture bytes
  objects/        canonical object set (C1-only cases)
  store.sqlite    base Store (C2 cases)
```

— with `seal: chmod removes 0o222` to make the master immutable. **Follow the spec,
not this document.**

**1.2 — one flag pair, family-agnostic.** Generalise `--emit-input`/`--load-input`
into `--emit-prepared DIR` / `--load-prepared DIR`. Each driver writes or reads
whatever its family needs in that one directory (a `prepared-tree.tsv`, an
`objects/` set, a `base.sqlite`, an `expectation.json`). Keep the old names as
aliases so the C1 filesystem rows do not change behaviour.

**1.3 — the compatibility digest.** `sha256` over a canonical JSON of: schema id,
`case_id`, `declared_bytes`, `declared_entries`, tier, tier_label, profile, shape,
cache_state, store_state, the frozen construction-policy identity and capacities,
product source commit, product lock sha256, harness lock sha256, producer binary
sha256. Unknown or missing input **fails closed**. Including the producer binary
means a harness edit invalidates every master; that is the conservative choice and
a `--rebuild-prepared` flag is the escape hatch.

**1.4 — atomic publication.** Build into `<root>/.building-<digest>-<pid>` on the
same filesystem, write `manifest.json` **last**, then `rename()` into
`<root>/<digest>`. Never overwrite an existing entry. Half-built entries, unknown
files and a manifest whose per-file hashes disagree are refused, not consumed.
Validate once per acquisition and retain the sealed digest.

**1.5 — `perf` consumes.** Resolve the digest; refuse a missing or incomplete
entry with the reason rather than silently rebuilding. Byte-copy the entry into a
per-case scratch path (never a reflink — a COW clone's `st_blocks` double-counts
and is already forbidden for allocated-byte rows), pass `--load-prepared`, run the
case, then destroy the copy. The **master is never opened writable**.

**1.6 — `prepare` produces.** `cmd_prepare` runs the child once per case with
`--emit-prepared`, then publishes. Its manifest records `produced` / `reused` /
`failed` per case with the producing revision and binary, replacing today's
`state: "not-produced"` for every case.

**Verification:** `prepare` twice — the second run must report `reused` for every
case and produce byte-identical files. Then run one row of each family and compare
its counters against the round-3b receipt: **every counter must be identical**,
because a loaded fixture must be indistinguishable from a built one. Any counter
that moves is a serializer defect, not a speed-up.

## 7. Phase 2 — per-family producers and consumers, in payoff order

| # | Family | Artifact in the master | non-op removed | Notes |
| ---: | --- | --- | ---: | --- |
| 1 | `c1.edit.length-preserving`, `c1.edit.length-changing` | base object set + `expectation.json` | ~76 s | base bytes are regenerated from the recipe (cheap PRNG); the 500 MiB CDC construction is what is saved |
| 2 | `c1.cdc.chunk-count` | base object set + `expectation.json` | ~22 s | same shape as (1) |
| 3 | `c2.reuse.workspace`, `c2.reuse.cross-file` | `base.sqlite` + the offered object sets | ~55 s | the 128-file base and the member sets are the cost |
| 4 | `c2.delta.cdc-locality` | `base.sqlite` + the derived member sets | ~27 s | 500 × 4 MiB members built once instead of per run |
| 5 | `c2.read.waves` | `base.sqlite` | ~12 s | |
| 6 | `c1.construct.*` | — | 0 | **the construction *is* the measured operation**; nothing to reuse, leave it alone |
| 7 | `c2.footprint` | — | ~3 s | mostly real work (19.1 s of 22.5 s); not worth the risk |

Phase 2 removes preparation only. The oracle read-back stays inside the process
until Phase 3, so expect the wall to fall by roughly the "non-op removed" column
minus the verification share — for `dedup-cdc-overwrite-500`, 11.5 s of the 23.5 s
non-operation, leaving ~17 s.

## 8. Phase 3 — the budget formula, after the owner rules

`CONTRACT.md` §4 freezes *"Complete command <= 15 s; declared exceptions <= 25 s"*
and §6 freezes *"`elapsed_ns` never gate-decides"*. A telemetry-derived budget makes
`elapsed_ns` decide admission, so this phase **waits for the #184 §7.1 ruling** and
is not implemented on assumption.

Recommended shape, for the ruling to accept or reject:

- `budget(operation_ns, complete_command_ns, declared_exception)` — a ceiling on
  `operation_ns` for the performance claim, **and** a separate wall ceiling on
  `complete_command_ns` that keeps lifecycle and cleanup bounded.
- The budget is recorded as an **admission classification**, distinct from the seven
  gate classes, so §6 keeps its meaning.
- Recorded as a new errata row in `CONTRACT.md` §11 with a new pin. The contract is
  never re-dated.

Then: re-measure the five `dedup-cdc-*-500` rows into a **new** directory and report
their new wall — `PASS` or `NOT_RUN` with the measured time, whichever the numbers
say. If they still do not fit, that is an owner question about declaring them, not
a reason to shrink a tier.

## 9. Phase 4 — closure

A fresh full lane into a new directory, `verify`, `report`, `calibrate`, a new dated
round-5 evidence directory, and the before/after non-operation share in one table
(Phase 0.6 makes it one line). Then re-check the #171 checkboxes this touches.

## 10. Risks, and how each is retired

| Risk | Retirement |
| --- | --- |
| A loaded fixture is not byte-identical to a built one | Phase 1 verification: per-family counter comparison against the round-3b receipt; any difference is a defect |
| The lossy `load_from_dir` is reused by mistake | Phase 1.1 writes a new serializer and the old one keeps its documented C1-read-only role; a test asserts role and predecessors survive the round trip |
| A master is read as a cold claim | The declared cache state does not change; `prepared-dewarmed` rows still de-warm and still gate `resident_pages == 0` and `disk_read_bytes` |
| A half-built master is consumed | Phase 1.4: manifest written last, atomic rename, fail-closed validation, per-key lock |
| Preparation reuse hides a real regression | Counters, not time, decide every gate; `verify` re-derives them from the trace as it does today |
| The digest is too coarse and a stale master is used | Unknown compatibility fails closed; the digest includes the producer binary and both locks |
| Phase 3 is implemented before the ruling | Phase 3 is gated on the #184 §7.1 owner decision and is explicitly not part of Phases 0–2 |
| Effort spent on the wrong family | §4's table is the order; `c1.construct.*` and `c2.footprint` are explicitly out |

## 11. Sequence

Phase 0 is independent and lands first — it is mechanical, it needs no cache, and
it produces the number that shows whether Phase 1 worked. Phase 1 is the
infrastructure and the new serializer. Phase 2 is then a per-family loop in the
order of §7, each family independently verifiable against its round-3b counters.
Phase 3 waits on the owner. Phase 4 closes.

Round 4 may land the phase split (its WP-1) first; if it does, Phase 3's
verification half is already done and this plan reduces to Phases 0, 1, 2 and 4.
Check what round 4 actually committed before starting — do not implement the phase
boundary twice.


## 12. The four measurement phases, and round-5 targets (owner directives)

### 12.1 Four phases, six published numbers

| | phase | what | published as | budgeted |
| --- | --- | --- | --- | --- |
| a | preparation | acquire the fixture (copy or load the master) + de-warm | `preparation_wall_ns`, and the copy/de-warm part also as `acquisition_wall_ns` (D2) | no — published only |
| b | **work** | the operation the row claims | `operation_ns` (telemetry root) | **yes — the performance claim** |
| c | verification | the oracle: replay + byte-exact read-back | `verification_wall_ns` | its own 60 s budget |
| d | cleanup | destroy the per-case copy, close | `cleanup_wall_ns` | inside the complete-command wall |
| | | process wall | `complete_command_ns` | the lifecycle/cleanup ceiling |

Phase 0 must publish all six and have `verify` re-derive them and fail closed on a
reconciliation failure. Note that **(d) does not exist today**, because nothing is
copied per case yet: the moment a master is copied per case, cleanup becomes real
work and must be timed and bounded like the rest.

### 12.2 Targets

Baseline: round-3b, 394.57 s lane, 39.15 s of operation over the 150 rows that
publish a timing receipt. Preparation and verification are each estimated at ~165 s;
the split is *measured* at 49.6% preparation on `dedup-cdc-overwrite-500` and assumed
elsewhere, which Phase 0 replaces with a measurement.

| # | target | from |
| --- | --- | --- |
| T1 | `preparation_wall_ns <= 1.0 s` for every row at every tier | 0.03–11.95 s |
| T2 | lane `sum(preparation_wall_ns) <= 25 s` | ~165 s |
| T3 | `prepare --lane full <= 90 s`, once per compatibility digest | ~165 s plus bookkeeping |
| T4 | `sum(operation_ns)` **unchanged** at 39.15 s; the five 500-tier delta rows stay 4.77–5.23 s | 39.15 s |
| T5 | lane `sum(verification_wall_ns) <= 100 s` | ~166 s |
| T6 | an unchanged case with an identity-matched receipt: `verification_wall_ns = 0` | ~166 s |
| T7 | `cleanup_wall_ns <= 0.5 s` per row, published for the first time | unmeasured |
| T8 | every row `complete_command_ns <= 15 s`, no tier shrunk | 28.4–29.6 s on five rows |
| T9 | all six fields in every receipt, re-derived by `verify` | none |

**T4 is a falsifier, not a goal.** The operation time must not fall. If it does,
measured work has been moved into setup and the change is rejected — which is
exactly the failure mode this whole plan exists to prevent.

Worst-row decomposition for T8: work 5.0 s + preparation 1.0 s + cleanup 0.5 s +
lifecycle ≈ 0.3 s = **6.8 s** against 15 s, with the 12.1 s verification charged to
its own 60 s budget. No tier is shrunk to reach any of these.

### 12.3 Skipping verification for fast iteration

1. **`--reuse-pass <verification.json>`** — mandated by `AGENTS.md` §2 and absent
   from this harness. Accepts one identity-matched `status=PASS`, cleanup-`PASS`
   receipt instead of re-running verification; fails closed on schema, identity,
   hard-limit or wall mismatch; records `reused_proof_identities` plus an explicit
   omission. This is the **sanctioned** fast path and the row can still be `PASS`.
2. **`--skip-verification`** (development only) — **every row it touches is
   `INCOMPLETE`, never `PASS`**, with `verification: omitted` in the receipt and the
   report. Useful for iteration, useless for evidence.

### 12.4 Reducing verification time

1. **Stop re-hashing what the harness already owns.** `Expectation::of` hashes the
   fixture bytes; `delta_members` does it in preparation and `workspace`'s oracle
   loop repeats it over the same 2.10 GB. `artifact.rs` already persists
   expectations — extend that to every family that builds one.
2. **Share the oracle's page cache across a case's members.**
   `read_back_through_store` builds a fresh provider and cache per member, and
   consecutive members share almost all their chunks, so the same packs are read and
   decoded 500 times.
3. **Do not re-derive what the artifact holds.**
4. **`--reuse-pass`** (§12.3) removes the phase entirely for an unchanged case.

Not proposed: parallel verification, or skipping the byte-exact read-back for a
changed case. The read-back **is** the proof.

### 12.5 What round 4 already landed — do not rebuild it

| Done | Where |
| --- | --- |
| The lossless artifact format | `src/workload/artifact.rs` |
| `--phase prepare\|verify`, the second invocation, `VERIFICATION_BUDGET_NS = 60 s` | `src/main.rs`, `runner.py` |
| Fail-closed acquisition (an unsealed artifact is refused) | `runner.py` |
| `acquisition_wall_ns` | `runner.py` acquisition record |
| The integrated C1→C2 family | `ops/pipeline.rs` |
| The declared-exception list corrected against the registry | `runner.py` |

| Still absent — round 5's scope | Evidence |
| --- | --- |
| `operation_ns` and the other phase fields | `runner.py:360` still calls `receipt.budget(wall_ns, ...)` |
| Split coverage beyond one family | `PHASE_SPLIT_FAMILIES = {"c2.delta.cdc-locality"}`; `verify-invocation` in one driver |
| `--reuse-pass` | grep over the harness returns nothing |
| Operation time for 44 rows | `grep -c write_timing ops/fs.rs` is **0** |
| The report's time axis | `shared/analyze.py:245` still uses `row.wall_ns` |
| Cleanup as a phase | nothing copied per case yet |
| Verification reduction | the oracle still re-hashes per member with a fresh cache per member |


## 13. Simplifying verification, and the quick mode (owner directive)

### 13.1 The harness over-verifies against its own frozen oracle

`gates_and_oracles.md` §5 freezes a per-family oracle. The four C2 families with the
largest verification cost are exactly the four whose frozen oracle **does not ask for
a read-back**:

| family | frozen oracle | driver does O2 read-back |
| --- | --- | --- |
| `c2.delta.cdc-locality` | **O1 + O3** | yes — 12.125 s, not required |
| `c2.reuse.workspace` | **O1 + O5** | yes, not required |
| `c2.footprint` | **O6 + O1** | yes, not required |
| `c2.pool.cold-warm` | **O1 + O3** | yes, not required |
| `c1.construct.*`, `c1.edit.*`, `pipeline.*`, `c2.read.waves` | includes **O2** | yes — **required** |
| `c1.many-tiny` | O4 + sampled O2 | already sampled |
| `c1.tree.*`, `c1.change-locality`, `c1.fs.build-scale` | O4 (+ O5) | no full read-back needed |

1. **Removing the unrequired read-back is compliance, not relaxation.** No contract
   change, no new stamp, no sampling — and it is the largest verification saving
   available. Do it **before** any sampling.
2. **The required oracle may be missing while the unrequired one is done.** The
   drivers gate "replay root == measured root", which is self-consistency, not O1
   ("expected root `ObjectId`, from a frozen constant or recomputed off the product
   path"); and the delta counters are written but I find no gate comparing them to
   **pinned** values, which is what O3 means. Real O1 = pin the expected result root
   per case (deterministic from the recipe, and transitive over the whole tree
   because the root identity digests canonical bytes that reference children by id).
   Real O3 = gate the counts against pinned values. Both are cheap.

The specification sanctions the cheap oracle for an expensive family in §4: *"the
parity set stays green plus the pinned identity constants match"*, with the
sealed-oracle parity set pinned at **35** external tests.

### 13.2 The deterministic 10% sample

Quick mode samples **10% of the workload, deterministically and declared**. Not
random: the harness is "deterministic from its recipe", and a random sample would let
the same tree `PASS` one run and `FAIL` the next.

| row shape | declared unit `n` | sample |
| --- | --- | --- |
| member rows (`c2.reuse.*`, `c2.delta.*`, `c2.pool.*`) | the member set | `max(1, ceil(n/10))`: member 0, member `n-1`, and every 10th between — endpoints always included, because that is where boundary defects live |
| single-object rows (`c1.construct.*`, `c1.edit.*`, `c1.cdc.chunk-count`, `c1.transition.*`) | logical length `L` | `max(1, ceil(L/10))` bytes as 10 evenly spaced windows, plus the first and last 64 KiB |
| tree rows (`c1.many-tiny`, `c1.tree.*`, `c1.change-locality`, `c1.fs.build-scale`) | the manifest | the specification's **already-frozen** `TreeSample` bounds (≤11 files, ≤11 dirs, 3 ranges/file, 64 KiB/range) — no new threshold invented |
| `c2.footprint` | — | O6 + O1 are O(1); no sample needed |

The selection rule is named in the receipt, so a sample is reproducible from the
receipt alone.

### 13.3 The mode ladder

| mode | oracle | status it can produce |
| --- | --- | --- |
| `full` | the frozen per-family oracle, O2 deduplicated where required | `PASS` |
| `sample` | the deterministic 10% sample | `INCOMPLETE`, never `PASS` |
| `none` | nothing | `INCOMPLETE`, never `PASS` |
| `reused` (`--reuse-pass`) | one identity-matched `status=PASS` receipt | `PASS` |

Every receipt carries `verification_mode`, `verification_declared_units`,
`verification_sampled_units`, `verification_selection`, `verification_omitted` and
`reused_proof_identities`, and the report header states the run's mode. **A sampled
or omitted row is `INCOMPLETE`**, so an iteration run cannot be mistaken for
admission evidence — which is what makes quick-by-default safe.

Quick is the default for iteration (`--lane smoke`, explicit `--case`); an admission
run is `full` or declares itself otherwise and is ineligible. Making a sampled oracle
the default *for evidence* changes a gate frozen before measurement (`CONTRACT.md`
§1: a change after collection needs a new stamp and a new directory) — an owner
decision, and per §13.1 it should not be needed, because the specification's own
oracle is already O(1).

### 13.4 Where O2 is required, deduplicate it

`dedup-cdc-overwrite-500` accepts 110,022 occurrences over roughly **3,665 distinct
objects** — 30x structural redundancy. Verifying each distinct payload once and then
verifying each member's traversal, assembling that member's digest from the verified
payload cache, is **complete** and about a fifth of the cost:

```text
O2 full read-back (today)                12.125 s   measured
O1 + O3 as the specification requires      ~0.01 s   projected
O2 deduplicated by distinct object          ~2-3 s   projected
deterministic 10% sample                    ~0.2 s   projected
--reuse-pass                                   0 s
```

**Do not weaken the read-back where the frozen oracle requires it.** The read-back
*is* the proof for the logical-equality families. The saving is in not doing it 30
times over, and in not doing it where it was never required.

### 13.5 Sequence this adds to §11

Phase 0 gains the mode ladder and the six verification fields. Phase 2's order does
not change. A new Phase 2.5 sits between them: remove the unrequired read-back,
implement pinned O1 and pinned O3, and deduplicate the required O2 — all before any
sampling is added, so that "quick" never becomes a substitute for doing the required
oracle correctly.
