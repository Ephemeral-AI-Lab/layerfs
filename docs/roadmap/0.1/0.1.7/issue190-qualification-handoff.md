# Handoff prompt — #190 admission qualification for the retained-history lane

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, after PR #202. This prompt contains existing evidence and
> a bounded next investigation; it is not a new measurement or a release claim.

## Mission and decision rule

Continue [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190) from the
retained read-path improvement in `4049e28b6` (PR #202). The product work for this
round is finished. **This round is about qualification, not speed:** decide and then
implement what it takes to move the `history.*` rows out of
`admission INELIGIBLE` and `g1.o3-pinned-counters = INCOMPLETE`, or record exactly
which owner decision blocks each.

Do not start by optimizing anything. Two of the three items are **owner decisions**
that an agent cannot make by itself; the third is engineering. Establish which is
which before writing code.

Owner rulings persist:

- **One second of stride10 operation reduction is worthwhile.** Preserve the five
  retained improvements.
- **Do not manufacture a cache claim.** No invented cold stance, no pre-touching
  measured inputs, no moving required reads into setup, no shrinking a selection to
  fit a budget.
- **Never hand-edit a pin.** `shared/pin_expected.py` is the only way a constant
  becomes frozen, and "a key that the baseline does not carry is **not** pinned".
- **Do not communicate with #192 or other independent user-owned tasks.** Use the
  global locks directly; do not revive permission handshakes. Do not create a new
  task merely to run this prompt.
- **Do not close #190** because a local improvement works.

## 1. Preserve these completed improvements

| PR | Retained change | Production LOC delta in that commit |
|---|---|---:|
| [#194](https://github.com/Ephemeral-AI-Lab/layerfs/pull/194) | Batched parent lookups and bounded authenticated base-record reuse | +49 |
| [#195](https://github.com/Ephemeral-AI-Lab/layerfs/pull/195) | Pooled physical-group telemetry and reuse through the existing 512 KiB cache | +141 |
| [#196](https://github.com/Ephemeral-AI-Lab/layerfs/pull/196) | Existing bounded prepared-statement cache for catalogue `group_for` queries | −1 |
| [#199](https://github.com/Ephemeral-AI-Lab/layerfs/pull/199) | Group compression level 19 → 1, payload level 3 unchanged | 0 |
| [#202](https://github.com/Ephemeral-AI-Lab/layerfs/pull/202) | Ordinal-ordered pooled-leaf resolution: one catalogue statement per distinct covering group | +3 |

Product total after #202: **85,725 production LOC** — reference `crates/` 65,417;
replacement `core/` 20,308. Recount exact snapshots for any later commit; do not
assume a newer main still has these totals.

[PR #197](https://github.com/Ephemeral-AI-Lab/layerfs/pull/197)'s New-row filter in
`zero_count_serials` stays archived and reverted. Do not reintroduce it.

## 2. Latest measured starting point

Retained campaign:
[`stage-6-history-190-read-20260920T042617Z`](evidence/stage-6-history-190-read-20260920T042617Z/README.md),
[results](evidence/stage-6-history-190-read-20260920T042617Z/results.json),
[call flow](evidence/stage-6-history-190-read-20260920T042617Z/CALLFLOW.md),
[review](evidence/stage-6-history-190-read-20260920T042617Z/REVIEW.md),
[independent re-derivation](evidence/stage-6-history-190-read-20260920T042617Z/rederive.py).

| Quantity | Stride10 baseline2 | Stride10 candidate2 | Stride3 baseline2 | Stride3 candidate2 |
|---|---:|---:|---:|---:|
| Operation ns (all selected states) | 21,905,900,168 | **19,888,424,711** | 56,597,343,506 | **49,501,013,748** |
| Reduction ns | — | **2,017,475,457 (9.209735%)** | — | **7,096,329,758 (12.538274%)** |
| Provider ns (nested) | 9,019,067,220 | 7,254,356,455 | 31,150,817,381 | 23,865,118,886 |
| Catalogue statements | 278,927 | **65,337** | 1,334,277 | **368,074** |
| Complete command ns | 40,220,718,250 | 38,101,144,250 | 80,483,422,875 | 73,127,405,000 |
| Verification work ns | 5,593,199,541 | 4,606,757,792 | 19,864,253,708 | 16,126,274,334 |

Both selections' saved Stores are **byte-identical between arms** and equal the
retained L40 level-1 candidate Stores. All 17 / 53 roots and both inventories match;
four identity-matched verifications PASS with 1,083 / 3,377 sampled paths and zero
mismatch. **The operation figure sums every selected state including the first**,
which is what `phases-perf.operation_ns` measures; see the campaign's
`SUPERSEDED-ANALYSIS.md` before reusing any derived JSON from an earlier revision.

## 3. Gap A — `g1.o3-pinned-counters = INCOMPLETE`

**What it literally is.** Every one of the four retained performance traces carries:

```text
g1.o3-pinned-counters = INCOMPLETE|no pinned counters for this case|at least one O3 constant pinned for every admission case
```

The trace-derived row status is therefore `INCOMPLETE` for all four rows
(`g1.o1-chain-complete` PASS, `g4.swaps` PASS, `g1.o3-pinned-counters` INCOMPLETE;
`shared/trace.py` severity order `PASS < TARGET_MISS < NOT_RUN < INELIGIBLE <
INCOMPLETE < FAIL`). The driver's own stdout line `history-stride10 PASS gates=2`
counts only the driver's two gates and is **not** the row status.

**Where it comes from.**

- `src/main.rs:563` — "the pinned-constant gates: **O3** against
  `tests/golden/expected.tsv` and **O1** against its pinned identity digests."
  O3 is pinned *counters*; O1 is pinned *identities*.
- `src/workload/expected.rs:157-166` — a case with no pinned counters returns
  `Gate::incomplete(...)` with exactly the string above.
- `tests/golden/expected.tsv` pins **217** case ids. `history-stride10` and
  `history-stride3` are not among them.
- `src/families/history.rs:1-9` states why: the `history.*` group is "**outside the
  217** … not in `families::ALL`, not in `FROZEN_CARDINALITY`, not in
  `registry::cases()`, not in `--smoke` and not in `--lane full`. It carries its own
  cardinality of three, its own lanes, its own golden table and its own verification
  default, and it amends no 217-row verdict."

So the gate is not reporting drift or a defect. It is reporting that the harness has
**no frozen oracle for these rows at all**.

**How a pin is actually made.** `shared/pin_expected.py counters --run
<baseline-run>` reads per-case `receipt.json` files from a named baseline run — the
table in the tree was pinned from the round-4c full lane at `90bbb617d`, "the last
tree on which all 217 admission rows passed with 0 `FAIL`". A key the baseline does
not carry is not pinned, and a value is never hand-edited. Regeneration, not
editing, is the only route.

**Two hazards to state before pinning anything.**

1. **A pinned counter must be invariant under every legitimate future
   optimization.** The table's own header records the worked example: the codec
   change re-baselined at `795fb1a2f` moved `counter:pool.commits` 19 → 18 because
   the group-level change shrank rewritten packs and the save crossed one fewer
   4 MiB charge boundary. Pinning a counter that a legitimate change may move turns
   every later improvement into a `FAIL` and forces a re-pin argument each time.
2. **The runner is stricter than the driver.** `runner.py:1464-1491` returns
   **`FAIL`** — not `INCOMPLETE` — for a row that publishes counters but has no
   pinned constant ("has no pinned O3 constant; the oracle cannot have been
   gated"). The `INCOMPLETE` above is the lenient face of the same gap; do not
   assume a runner pass is closer than it is. Whether a history receipt populates
   `counters` is a property of the runner's receipt for that lane, which no campaign
   has yet exercised.

**What closing Gap A requires, in order.**

1. An **owner decision** that the `history.*` rows carry a frozen oracle at all —
   i.e. that they are admission rows rather than declared diagnostics.
2. A qualifying **baseline run through the runner** whose receipts carry the
   counters, on a tree the owner accepts as the baseline.
3. A stated **selection principle** for which of the roughly fifty published
   counters are pinned, with an invariance argument for each. Prefer counters that
   are structural facts (object counts, group counts, state counts, commit counts
   under frozen thresholds) over counters that measure effort (decode counts, fetch
   counts, byte counters that a legitimate cache or encoding change may move).
4. Regeneration with `shared/pin_expected.py`; never a hand-edited value.

Do **not** add the history rows to `registry::cases()`, `FROZEN_CARDINALITY` or the
217-row self-check to make coverage assertions happy. `registry.rs:485-515` asserts
O3 coverage over the 217 rows and a frozen cardinality array; changing either is a
contract change, not a fix.

## 4. Gap B — `admission INELIGIBLE`

**What it literally is.** It is **not** a gate. No `INELIGIBLE` record appears in
any of the four retained traces. It is the lane's declared qualification stance,
recorded in the campaign receipts as `"admission_eligible": false,
"cold_performance_status": "INELIGIBLE"`.

- `src/gates.rs:24-26` — `INELIGIBLE` means "a precondition failed: residency,
  coverage, cache arm mismatch."
- The rows declare `CacheState::CreatedInSample` ("base written by the operation
  itself inside the timed region") and `StoreState::CreatedInSample`
  (`src/registry.rs:53-60,108-115`). That is a *declared* state, not an unknown one.
- The only enforced cache contract in the tree is `shared/cold.py`, and
  `cold.py::applies` fires only for `family == init_namespace` and
  `case == namespace-100000`. **No residency contract exists for `history.*`.**
- AGENTS.md §1 and `benchmark/AGENTS.md:95-97` make an undeclared or unenforced
  cache stance `INELIGIBLE`/`INCOMPLETE`, never `PASS`, and an `INELIGIBLE` row
  keeps its raw timing and is not admission evidence.

**Primitives that already exist** (so this is buildable, not blocked on tooling):
`shared/residency.py` provides `residency()` and `de_warm()` (mmap + `mincore` +
`msync(MS_INVALIDATE)`, "never touch-every-page"), `src/support/instruments.rs`
mirrors the same three rules, and `disk_read_bytes` exists as the device attestation
that `mincore` cannot substitute for.

**Three structural blockers, in order of severity.**

1. **There is no admission path for this lane.** `runner.py:136-147`:
   `default_verification_mode` returns `ADMISSION_MODE` **only** when
   `lane == "full"`, and `HISTORY_LANES` are explicitly not members of `full`
   (`runner.py:60-65`). A history invocation is always `sample` mode, and "a row it
   sampled or omitted is `INCOMPLETE`". So even a perfect cache contract and a
   perfect pin set would still produce a sampled, `INCOMPLETE` row until the owner
   changes what admission means for these lanes.
2. **The wall budget does not fit.** `benchmark/AGENTS.md:78-84`: a performance
   selection's complete command is **≤ 15 s**, with a small declared exception list
   up to **25 s**. Measured history complete commands are **38.1 s** (stride10) and
   **73.1 s** (stride3) — outside even the exception ceiling.
   `benchmark_rules.md:716-718` contemplates exactly this shape ("large-history,
   resource-boundary, and sustained-endurance cases may take longer and MUST be
   explicitly selectable… an ordinary-lane pass is not complete admission"), so a
   longer frozen budget for this family is *contemplated but not frozen*. That is an
   owner decision. **Do not shrink the selection to fit, and do not enlarge a
   timeout.**
3. **The cache stance is a modelling question, not a mechanical one.** The history
   workload is a same-process write-then-read replay: state *N* reads what states
   *1..N−1* wrote, inside one timed operation, and the Store is created in-sample.
   Invalidating the Store's pages between states would invalidate the operation's own
   work microseconds after writing it. AGENTS.md §1's forbidden list is about
   *setup, preparation, an earlier sample, another arm or another phase* serving a
   timed phase — this is none of those, and the rule's own test ("would the work
   still have to happen inside the timed phase?") is satisfied. The honest question
   is therefore **whether `CreatedInSample` is the correct declaration for this
   shape and whether it can be *verified*** — residency plus `disk_read_bytes`
   measured at state boundaries and asserted, rather than assumed — not whether a
   cold claim can be manufactured. Answer that question in writing before building
   anything.

## 5. Evidence that must not be misused

- **Do not call the two gaps defects in the product.** Neither is caused by, nor can
  be moved by, any product change. They were present in every earlier #190 round
  (L40, L41) and are produced by oracle coverage and lane configuration.
- **Do not present an instrumented diagnostic as an admission row.** The retained
  pair was measured with a recorded instrumentation patch present in both arms. The
  retained source differs from the measured candidate only by that patch
  (`source/retained-vs-measured-read.diff`).
- **Do not promote the diagnostic caps.** 120 s (stride10), 240 s (stride3) and the
  60 s verification hard budget are the established #190 *diagnostic* ceilings. They
  are not ordinary 15/25 s admission rows and must not be enlarged to pass.
- **Do not pool cache states, invent a cold claim, or re-run a case for a better
  number.** One sample per case/arm; append-only receipts including failures and
  deferrals.
- **Do not conflate this with the historical tripwire.** The unmatched v0.1.6
  Commit (11,370,679,212 ns) is a separate unresolved item and is not a
  qualification gate for these rows.
- **`history_corpus.PINS` carries `canonical_bytes`/`canonical_objects` that the
  corpus probe does not compare** (`runner.py:1624-1630` checks only `states`,
  `path_states`, `logical_bytes`). Confirm those totals' semantics before treating
  any difference between them and a Store inventory as drift; this prompt does not
  assert a mismatch.

## 6. Measurement and resource protocol

Read [AGENTS.md](../../../../AGENTS.md), [core/AGENTS.md](../../../../core/AGENTS.md),
[benchmark rules](../../../general/benchmark_rules.md),
[benchmark agent rules](../../../../benchmark/AGENTS.md) and the
[quickstart](../../../../benchmark/fs-bench-pro/QUICKSTART.md) before work.

- Serialize builds, tests, performance, verification and large artifact inspection.
  Acquire global `flock(LOCK_EX|LOCK_NB)` on the resolved
  **`$TMPDIR/layerfs-infra-measurement.lock` then
  `/tmp/layerfs-infra-measurement.lock`**, deduplicating identical resolved paths,
  and hold the descriptors for the whole resource command. A held lock means defer.
- The harness private `.measurement.lock` is a **different protocol**
  (`shared/receipt.py::measurement_lock`, `O_CREAT|O_EXCL` marker). Never open it
  with append+flock; never blindly delete a marker.
- Quiet check: no named competing `cargo`/`rustc`/`fs-bench` process, at least 70%
  CPU idle on the second of two one-second observations. A busy preflight consumes
  no sample and is retained as a deferral.
- Rust 1.85.1 and `--locked`. No third-party edits, vendoring or patches. No CI and
  no retired `tools/preflight.sh`.
- If a runner-based baseline is needed, check whether it fits the declared budgets
  **before** starting it; a run that cannot fit is recorded `NOT_RUN` with its
  measured wall time and reason.

## 7. Source, corpus and artifact custody

Primary checkout may contain another task's work; do not reset or clean it. Start
from a clean isolated checkout containing #202 (`4049e28b6` / `3cec2f8da` /
`5c6f15ba5` / `20fd68918` / `40054a403`) and record actual HEAD.

Corpus: `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`.
Manifest SHA256: `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`.
Tip: `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`.
Stride10 selection `range(1,158,10) ∪ {157}`, 17 states; stride3 `range(1,158,3)`,
53 states; stride1 157 states. Confirm from `shared/history_corpus.py`; do not
shrink selections.

Large binaries and Stores remain local under the retained campaigns' custody
manifests and are excluded from Git. If an artifact is unavailable, state the
custody gap instead of substituting a rebuilt binary.

## 8. Work organization, proof and deliverables

Keep the round finite. Produce:

1. A **written disposition of each gap** — closed with evidence, or explicitly
   owner-blocked with the exact decision required, the options and their measured
   consequences. Do not leave either gap as an unexplained label.
2. If the owner authorises qualification: the **declared cache stance** for
   `history.*` with its verification method (residency plus device reads) and its
   evidence; the **O3 selection** with an invariance argument per pinned counter,
   regenerated through `shared/pin_expected.py`; and the **budget decision** with its
   measured wall times.
3. Measured evidence under the protocol above, with exact identities, fresh
   append-only outputs and every failure and deferral retained.
4. Required checks for whatever tree is changed: `cargo +1.85.1
   test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
   `core/tools/check_product_boundary.py` and its self-tests, and the affected
   harness tests. Do not rerun unchanged known-failing harness lint and call it a
   pass; cite gaps precisely.
5. A report, an append-only ledger entry after **L42** in
   [the active ledger](../0.1.6/evidence/issue151-experiment-ledger.md), a #190
   update, and exact first-parent/staged production LOC for each commit. Keep
   reference/core subtotals and disclose scope changes.
6. **Do not close #190.** Closing requires the owner's qualification decision and
   the historical-comparison item, neither of which this prompt can supply.
