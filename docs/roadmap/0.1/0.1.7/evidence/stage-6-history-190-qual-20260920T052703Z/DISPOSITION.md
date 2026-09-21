# Disposition: `admission INELIGIBLE` and `g1.o3-pinned-counters = INCOMPLETE`

> Status: Research; informative and not a product contract. Qualification round on
> top of #202 (`4049e28b6`), 2026-09-20. **No measurement command was run**: every
> number below is either a constant read from source or a re-derivation over the
> retained campaign's existing receipts and traces. Read
> [CHECKS.md](CHECKS.md) before quoting any figure.

## 0. Verdict

Neither label is a defect in the product and no product change moves either. Each
decomposes into one or more items, and every item is one of three kinds:

| # | Item | Kind | Movable by an agent now? |
|---|---|---|---|
| A1 | the `history.*` rows have no frozen oracle, so O3 cannot gate | **owner decision** | no |
| A2 | the O3 pin surface is 421 / 1,249 counters per row, and the generator has no selection principle | engineering, blocked behind A1 | no |
| A3 | `pin_expected.py` cannot bootstrap a pin set for a row that does not already pass | **owner decision** (bootstrap route) | no |
| B1 | admission mode is not declared for these lanes | **owner decision** | no |
| B2 | the complete command does not fit any frozen budget class | **owner decision** | no |
| B3 | the declared phases do not reconcile with the invocation wall | engineering + phase-accounting decision | measured here; fix is a declaration |
| B4 | the cache stance names the Store axis and is silent on the corpus axis | engineering; answered in writing | answered: [CACHE-STANCE.md](CACHE-STANCE.md) |

**Two of the three items the handoff named are owner decisions; the third is a
modelling question, and its answer changes which axis is actually undeclared.**
The one thing this round found that the handoff did not have is that the two gaps
are not independent: **A is downstream of B1 and B2.** A history row cannot be
`PASS`, so the pin generator refuses it, so the pins that would let it pass cannot
be generated. That knot is what makes both labels stand still.

## 1. Gap A — `g1.o3-pinned-counters = INCOMPLETE`

### 1.1 What it literally is (verified, not restated)

* `src/main.rs:563` applies two pinned-constant gates: **O3** against
  `tests/golden/expected.tsv`, **O1** against pinned identity digests.
* `src/main.rs:627-651`: the gate feeds on records that are
  `record.kind == "counter" && record.numeric`. A `resource`-kind record can
  never be O3-gated, however structural it looks.
* `src/workload/expected.rs:157-166`: a case with no pinned counters returns
  `Gate::incomplete("g1.o3-pinned-counters", "no pinned counters for this case", …)`.
* `tests/golden/expected.tsv` carries **217** case ids and **156** distinct pinned
  counter names. `grep -c history` over it is **0**.
* `src/families/history.rs:3` states the placement: "**This group is outside the
  217.** It is not in `families::ALL`, not in `FROZEN_CARDINALITY`, not in
  `registry::cases()`, not in `--smoke` and not in `--lane full`."
  `registry.rs:31/34/37` fix `ADMISSION_CASES = 217`,
  `DIAGNOSTIC_CASES = 3`, `REGISTERED_ROWS = 220`; `registry.rs:437/443` keep
  `history_cases()` and `HISTORY_CARDINALITY = 3` in a separate accessor.

So the gate is not reporting drift. It is reporting the absence of an oracle, and
the absence is deliberate.

### 1.2 What is new here: the runner does not merely disbelieve the row, it fails it

The handoff says the runner "would return FAIL rather than INCOMPLETE for a row
that publishes counters but has no pinned constant" and that whether a history
receipt carries counters "no campaign has yet exercised". It does, and it has been
exercised: [runner-verdict.txt](runner-verdict.txt) replays
`runner.py`'s own `re_derive_pins` (`runner.py:1464-1507`) over the four retained
traces.

* Published numeric counters in the retained whole traces: **421** (stride10) and
  **1,249** (stride3). The performance invocation alone publishes 410 / 1,238.
* `runner.re_derive_pins` returns, for all four runs,
  `FAIL: history-stride10 has no pinned O3 constant; the oracle cannot have been gated`
  (and the same for stride3). This is not `INCOMPLETE`; under `runner.py verify`
  it increments `disagreements` (`runner.py:1392-1394`) and the case's re-derived
  status becomes `FAIL`.

`INCOMPLETE` is therefore the *lenient* face of the gap, exactly as the handoff
warned, and a runner-based verification of this lane today would report FAIL — not
because the product drifted, but because the row is unpinnable and the check has
no "declared absent oracle" branch for a row that publishes counters.

### 1.3 The pin surface, and why "state a selection principle" is the real work

`shared/pin_expected.py:40-66` is the only route by which a constant becomes
pinned. It pins **every integer counter of every receipt whose `status == "PASS"`**,
excluding exactly one instrumentation key (`INSTRUMENTATION_KEYS =
{"timing_json_bytes"}`). There is no selection mechanism to state a principle in.

[counter-selection.txt](counter-selection.txt) measures what that means for these
rows, from the four retained traces and the 217-row table:

| Quantity | stride10 | stride3 |
|---|---:|---:|
| published numeric counters (performance invocation) | 410 | 1,238 |
| distinct family-local counter names | 42 | 42 |
| row-level counters (not per state) | 19 | 19 |
| per-state counter names | 23 | 23 |
| **moved** by the retained legitimate optimization | **2** | **2** |
| invariant candidates (not a duration, not moved) | 408 | 1,236 |
| names the 217-row table already treats as pinnable | 8 | 8 |
| names with no 217 precedent | 32 | 32 |

The two moved counters are `history.root_ns` and `history.children_ns` — both
durations, both the row's own timing totals. The other forty names were byte-for-byte
identical between the two arms, which is the expected consequence of a treatment
whose saved Stores are byte-identical. The screen is a screen, not a proof: it
contains **one** legitimate change, and the table's own header records the
counterexample that a legitimate storage change moves a pinned counter — the
`795fb1a2f` re-baseline moved `counter:pool.commits` 19 → 18 because the codec
change crossed one fewer 4 MiB charge boundary. Thirty-four of the forty-two names
here are implementation-effort counters of exactly that kind
(`filesystem.validation.*`, `filesystem.references.*`, `delta.*`, `save.commits`),
and the per-state ones multiply with the selection: **23 names × 17 or 53 states**.

Pinning everything, which is what the generator does, would add one line per
counter: **421 + 1,249** as the receipt composes it (the deferred verify invocation
runs inside `runner.run_case`, `runner.py:947-980`, before the receipt is composed
at `runner.py:999+`, so the receipt's counters include the `verify.*` block; the
performance invocation alone publishes 410 / 1,238). That is against a table that
today holds 1,981 lines, and the third row (`history-stride1`, 157 states, never
measured) projects to a further ~3,630 (`NOT_MEASURED`; the same per-state code
path). Roughly **5.3 k** frozen constants for the three rows.

### 1.4 The bootstrap is circular, and that is a separate owner decision

`pin_expected.counters_of` skips any receipt whose `status != "PASS"`
(`pin_expected.py:49-52`: "A row that did not pass has no baseline number to pin").
Replayed over a runner-shaped receipt whose status is the value the runner's own
rules compose for these rows:

| receipt status | pinned constants |
|---|---:|
| `INCOMPLETE` | 0 |
| `NOT_RUN` | 0 |
| `PASS` (counterfactual) | 421 / 1,249 |

The 217 rows were pinned from "the round-4c full lane at `90bbb617d`, the last tree
on which all 217 admission rows passed with 0 `FAIL`" — a tree on which the row
passed *because the O3 gate did not yet exist*. A **new** row on today's tree has
no such tree. Its status is held at `INCOMPLETE` by the O3 gate itself, so
`counters_of` refuses its receipt, so the pin set that would clear the gate cannot
be generated. `pin_expected.py merge` does not break the loop either: it merges a
counters file with a digests file, and the counters file has the same origin.

Breaking the loop requires an owner-authorised route, and there are only three
shapes: (i) pin from a named baseline run whose receipts are accepted *without*
`status == PASS`, with the deviation recorded in the table header; (ii) run and pin
on a tree where the O3 gate is temporarily not applied to these rows; (iii) rule
that these rows carry no oracle and stop asking O3 to gate them. Each is a
contract-level act, not a fix.

### 1.5 What closing A requires, in order

1. **D1** — the owner rules whether `history.*` carries a frozen O3 oracle at all.
2. **D3** — the owner names the bootstrap route (1.4), because the generator cannot
   create a first pin set for a row that is not already passing.
3. **D2** — a selection principle is *implemented* (an explicit exclusion list in
   `pin_expected.py`, sibling to `INSTRUMENTATION_KEYS`) or the full surface is
   accepted at ~5.3 k lines. `pin_expected.py counters --run <baseline-run>` then
   regenerates; no value is ever hand-edited.
4. Only then does the driver's `g1.o3-pinned-counters` gate stop being the reason
   the row is `INCOMPLETE`.

Steps 2-4 are blocked behind 1, and step 4 is *also* blocked behind Gap B
(section 3.4).

## 2. Gap B — `admission INELIGIBLE`

### 2.1 It is a declared stance, not a gate

No `INELIGIBLE` record appears in any of the four retained traces. The campaign
receipts carry it as a declaration:

```json
"cache_contract": "fresh-growing-store; OS/intra-chain residency uncontrolled",
"admission_eligible": false,
"cold_performance_status": "INELIGIBLE"
```

`src/gates.rs:24-26` defines `INELIGIBLE` as a failed precondition (residency,
coverage, cache arm mismatch). The rows declare
`CacheState::CreatedInSample` / `StoreState::CreatedInSample`
(`src/registry.rs:53-59/108-112`; `src/families/history.rs:37-38`), which the
registry describes as "Base written by the operation itself inside the timed
region"/"The Store is created inside the timed region" — a *declared* state, not an
unknown one.

One detail the handoff does not mention and that sharpens the framing: the row's
own trace publishes `{"key": "admission", "value": "admission"}`.
`CaseSpec::new` (`families/mod.rs:125`) defaults `admission: Admission::Admission`,
and nothing overrides it for `history.*`. So the row **declares itself an admission
row, is counted in no admission set, and is never selected by `--lane full`.**
"Declared admission, never admitted" is the precise statement; "declared
diagnostic" would be wrong.

### 2.2 Blocker 1, corrected: the mode is declarable; the *admission decision* is missing

`runner.py:136-147` returns `ADMISSION_MODE` only for `lane == "full"`, and
`runner.py:65` keeps the three history lanes out of `full`, so the **default** is
`sample`. But `perf --verify` exists and takes `full` explicitly, and the runner's
own docstring says "`--verify` always wins over this" (`runner.py:147`,
`runner.py:90-95`). So the handoff's "a history invocation is always `sample` mode"
is wrong as written: a history invocation that declares `--verify full` is not
sampled.

What that does **not** do is make the row admissible:

* the trace status is still `INCOMPLETE` from the O3 gate, and the runner's
  published status is the trace status unless something worse happens;
* the budget still overrides (`runner.py:1038-1050`);
* the reconciliation still fails (2.4).

So B1 is not "the runner forbids it"; it is "the owner has not ruled that these
lanes are admission rows, and the mechanical path to a `PASS` is blocked by A, B2
and B3 regardless". The decision required: **does an admission claim exist for
`history.*`, and if so, under which declared mode and with what `PASS` meaning?**

### 2.3 Blocker 2, with the arithmetic corrected: the budget classifies a formula

`runner.py:1016` classifies `receipt.budget(wall_ns, declared_exception,
declared_ns=…)`, and `shared/receipt.py:250-289` defines the budgeted quantity as

```
budgeted = preparation + operation + verification + cleanup + 0.250 s lifecycle
```

— **not** the raw process wall, which is published beside it as
`complete_command_ns`. The handoff reasons from the wall ("38.1 s / 73.1 s —
outside even the exception ceiling"). Re-derived from the retained phase artifacts
(`runner-verdict.txt`), the classified quantities are:

| run | preparation | operation | verification | cleanup | **budgeted + 0.25 s** | raw wall | limit |
|---|---:|---:|---:|---:|---:|---:|---|
| baseline2 stride10 | 0.189 | 21.906 | 0.023 | 0.000 | **22.367 s** | 40.221 s | 15 / 25 s |
| candidate2 stride10 | 0.182 | 19.888 | 0.027 | 0.000 | **20.348 s** | 38.101 s | 15 / 25 s |
| baseline2 stride3 | 0.542 | 56.597 | 0.030 | 0.000 | **57.419 s** | 80.483 s | 15 / 25 s |
| candidate2 stride3 | 0.571 | 49.501 | 0.034 | 0.000 | **50.356 s** | 73.127 s | 15 / 25 s |

`DECLARED_EXCEPTIONS` contains none of `HISTORY_LANES` (`runner.py:89-98`), so every
one of the four is `NOT_RUN` on the ordinary 15 s limit today. **stride10's
classified quantity fits the declared 25 s exception (20.348 s); stride3's does not
(50.356 s).** The handoff's conclusion holds for stride3 and for the raw wall, but
not for stride10 as the runner actually classifies it — and that difference is the
whole content of the budget decision: stride10 is one declaration away from a
budget-passing command; stride3 needs a new frozen class.

### 2.4 Blocker 3 (new): the declared phases do not reconcile

Between 31% and 43% of the complete command is not in any phase field.
`phases.compose` (quoted verbatim by the replay) reports for all four runs:

```
perf: 18103407414 ns of the 40220718250 ns wall is outside every declared phase,
      more than the declared 1054414365 ns tolerance
```

and `runner.py:1060-1075` fails the reconciliation closed to `INCOMPLETE`. The span
has a name in the driver's own trace — it is published, not hidden:

| run | `history.corpus_read_ns` (resource) | `history.root_ns` (counter) | `history.children_ns` (counter) | root − children |
|---|---:|---:|---:|---:|
| baseline2 stride10 | 16,248,603,710 | 38,174,166,166 | 21,905,900,168 | 16,268,265,998 |
| candidate2 stride10 | 16,161,397,833 | 36,067,700,458 | 19,888,424,711 | 16,179,275,747 |
| baseline2 stride3 | 23,208,107,041 | 79,832,087,833 | 56,597,343,506 | 23,234,744,327 |
| candidate2 stride3 | 22,943,708,667 | 72,472,473,000 | 49,501,013,748 | 22,971,459,252 |

Its basis string says exactly what it is: *"the harness's own corpus reading:
inside the root, between the children, untimed"*; the row's notes repeat it
("corpus_reading: between the children, untimed"), and
`src/ops/history.rs:12-15` documents the intent — `operation_ns` is the sum of the
named children *because* a root would include `Corpus::transition`'s work, "which
is the harness's".

So this is a **phase-accounting gap, not concealment**: the work is named,
measured and published, but it is not one of the four fields the runner reconciles
and budgets. Its measured size is ~16.2 s (stride10: 42.4% of the 38.101 s wall)
and ~23.0 s (stride3: 31.4% of the 73.127 s wall). Until it is declared, (i) the
reconciliation holds
the row at `INCOMPLETE` however the gates resolve, and (ii) the budgeted quantity
understates the command a reader would run by ~16–23 s.

Three ways to declare it, with their measured consequences:

| Option | Effect on reconciliation | Budgeted quantity | Effect elsewhere |
|---|---|---|---|
| (a) declare it as `preparation` | passes | **36.3 s** (stride10) / **73.0 s** (stride3) | `operation_ns` unchanged, so the #202 reductions are unchanged |
| (b) fold it into the operation | passes | same 36.3 / 73.0 s | `operation_ns` becomes 36.05 / 72.47 s; the 2,017,475,457 / 7,096,329,758 ns reductions stay absolute but their share falls from 9.209735% / 12.538274% to ~5.60% / ~9.79% |
| (c) declare a fifth phase | needs a tolerance/contract change | same 36.3 / 73.0 s | the four-phase model in `CONTRACT.md` §4 changes |

(a) and (b) are the same decision wearing different names — whether the harness's
per-run input assembly is charged to the row's command as preparation or to its
operation. Either way, **the honest stride10 number against the 25 s ceiling is
36.3 s, not 20.3 s**, and the decision in 2.3 must be taken on the declared figure.

### 2.5 Blocker 4: the cache stance — see CACHE-STANCE.md

Answered in writing in [CACHE-STANCE.md](CACHE-STANCE.md). The short version: the
`CreatedInSample` declaration is correct and sufficient for the **Store** axis, and
it must *not* be verified by residency or device attestation, because
`resident_pages == 0` is false by construction for a Store written inside the timed
region and `device_attestation` is defined only for cold/de-warmed claims
(`src/gates.rs:380-387`; it has **no call site anywhere in the tree**, and no op
records `disk_read_bytes`). What is actually undeclared is the **corpus** axis: the
rows read an immutable, identity-pinned corpus whose pages may be resident from an
earlier run of the same case, and nothing measures or enforces that. So the owner
decision is not "may we claim a cold Store" — it is **"is 'immutable identity-pinned
corpus, residency uncontrolled and unenforced, read wholly inside the timed region'
an acceptable declared stance for an admission row?"**

### 2.6 Two claims in the handoff that are true of the reference tree only

* "The only enforced cache contract in the tree is `shared/cold.py`" — true of
  `benchmark/fs-bench-pro/shared/cold.py` in the **reference** harness, whose
  `applies` fires only for `init_namespace`/`namespace-100000`. The core harness
  that produced this evidence has **no `cold.py` at all**; `shared/residency.py` is
  imported only by `shared/experiments.py` and by `runner.py:1660`'s
  `self-check`. The core harness therefore has no enforced cold contract for *any*
  family, not merely none for `history.*`.
* "`disk_read_bytes` exists as the device attestation" — it exists as a field and a
  gate function, unused: `gates::device_attestation` has zero call sites and no
  operation records the instrument.

## 3. The decisions, in the form the owner needs them

* **D1 — oracle.** Do the three `history.*` rows carry a frozen O3 oracle?
  *Yes* → D3 and D2 become live and the pin work is ~5.3 k constants or an explicit
  exclusion list; *No* → the rows are declared diagnostics and
  `g1.o3-pinned-counters = INCOMPLETE` stops being a gap and becomes the declared
  status (and `runner.re_derive_pins` needs a declared-absent-oracle branch, or a
  runner verification of this lane keeps reporting FAIL).
* **D2 — selection.** If yes: which counter names may be frozen? The measured
  candidate set is 408/1,236 values over 42 names (1.3), of which 8 names have
  217-row precedent. The two durations must be excluded in any case.
* **D3 — bootstrap.** Which route creates the first pin set for a row that cannot
  pass before it is pinned (1.4)? All three routes are contract-level.
* **D4 — admission and budget.** Does an admission claim exist for these lanes, and
  in which frozen budget class? Options measured here: stride10 at the 25 s declared
  exception (20.348 s classified, **36.3 s once the corpus reading is declared**);
  stride3 in a new large-history class (50.356 s → **73.0 s** declared); or the rows
  stay diagnostic. Coupled to D4 is the phase-accounting choice in 2.4 (a/b/c),
  which must be settled before the budget figure is honest.
* **D5 — cache stance.** Is the corpus-axis stance in CACHE-STANCE.md acceptable
  for an admission row, or must the corpus carry its own cold contract?

Standing owner rulings unchanged and not revisited here: do not shrink a selection
to fit; do not enlarge a timeout; do not hand-edit a pin; do not invent a cold
claim; do not add the history rows to `registry::cases()`, `FROZEN_CARDINALITY` or
the 217-row self-check; do not promote the 120/240 s diagnostic caps; do not close
#190.
