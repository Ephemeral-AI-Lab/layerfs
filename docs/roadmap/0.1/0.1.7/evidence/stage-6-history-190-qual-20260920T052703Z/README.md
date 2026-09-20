# #190 admission qualification: what moves `INELIGIBLE` and `INCOMPLETE`, and who moves it

> Status: Research; qualification disposition, informative and not a product
> contract. Round of 2026-09-20 on top of #202 (`4049e28b6`). **No measurement
> command, build or test-suite run was performed**: every figure is a constant read
> from source or a re-derivation over the retained campaign's existing receipts and
> traces. #190 stays open.

**The question.** Move the `history.*` rows out of `admission INELIGIBLE` and
`g1.o3-pinned-counters = INCOMPLETE`, or record exactly which owner decision blocks
each.

**The answer.** Two of the three items the handoff named are owner decisions, the
third is a modelling question, and **the two gaps are one knot**: a history row
cannot reach `PASS`, so `pin_expected.py` refuses its receipt, so the pin set that
would let it pass cannot be generated. No product change moves either label.

## Findings

1. **The runner does not merely disbelieve these rows, it fails them.**
   `runner.re_derive_pins` returns
   `FAIL: … has no pinned O3 constant; the oracle cannot have been gated` for all
   four retained traces — they publish 421 / 1,249 numeric counters, so the
   "publishes no counters" escape does not apply. `INCOMPLETE` is the lenient face
   of a check that would report FAIL.
   [runner-verdict.txt](runner-verdict.txt)
2. **The pin route is circular.** `pin_expected.counters_of` pins only receipts
   whose `status == "PASS"`; replayed against the status the runner's own rules
   compose for these rows it pins **0** constants, and against a `PASS` receipt it
   pins 421 / 1,249. The 217 rows were bootstrapped on a tree where the O3 gate did
   not yet exist; a new row has no such tree. Breaking the loop is an owner
   decision, not a fix. [runner-verdict.txt](runner-verdict.txt)
3. **The pin surface is measured.** 42 distinct counter names (19 row-level, 23
   per-state), **2 of 410 / 1,238** values moved under the retained legitimate
   optimization — both of them `*_ns` durations — 8 names have precedent in the
   217-row table and 34 do not. Pinning everything adds ~5.3 k constants across the
   three rows. [counter-selection.txt](counter-selection.txt)
4. **The budget arithmetic in the handoff is the raw wall, not the classified
   quantity.** `receipt.budget` classifies `preparation + operation + verification +
   cleanup + 0.250 s`. Measured: stride10 **20.348 s** (candidate2) / 22.367 s
   (baseline2) — inside a declared 25 s exception and outside the ordinary 15 s
   limit; stride3 **50.356 s** / 57.419 s — outside both. The handoff's "outside
   even the exception ceiling" holds for stride3 and for the wall, not for stride10
   as the runner classifies it.
5. **A new blocker: the declared phases do not reconcile.** 18.10 s (stride10) and
   23.02 s (stride3) of the complete command lie outside every declared phase, and
   `runner.py` fails that reconciliation closed to `INCOMPLETE`. The span is
   published, not hidden — `history.corpus_read_ns`, "the harness's own corpus
   reading: inside the root, between the children, untimed" — measured 16.16 s /
   22.94 s. Declaring it moves the honest stride10 command to **36.3 s**, not
   20.3 s, which is the number the budget decision must be taken on.
6. **`--verify full` exists**, so "a history invocation is always `sample` mode" is
   wrong as written; what is missing is the owner's ruling that these lanes are
   admission rows. The row, meanwhile, publishes `admission: admission` in its own
   trace while being counted in no admission set — *declared admission, never
   admitted*.
7. **The cache gap is on the corpus axis, not the Store axis.** `CreatedInSample`
   is correct for the Store and must not be "verified" by residency or device
   attestation; the undeclared input is the immutable, identity-pinned corpus whose
   residency is uncontrolled. [CACHE-STANCE.md](CACHE-STANCE.md)

## Dispositions

| Gap | Disposition |
|---|---|
| `g1.o3-pinned-counters = INCOMPLETE` | **Owner-blocked on D1** (does `history.*` carry a frozen oracle?), with **D3** (bootstrap route) and **D2** (selection principle) behind it. No agent can decide any of the three. |
| `admission INELIGIBLE` | **Owner-blocked on D4** (admission claim + frozen budget class) and **D5** (corpus-axis cache stance), with the phase-accounting choice B3 as a prerequisite for an honest budget figure. |
| the engineering item | answered in writing: [CACHE-STANCE.md](CACHE-STANCE.md), plus the measured counter-selection and runner-verdict evidence. Not implemented — the answer changes what would be built, and building it is budget-affecting. |

The five decisions, with options and measured consequences, are in
[DISPOSITION.md](DISPOSITION.md) §3.

## Evidence index

| Artifact | What it is |
|---|---|
| [DISPOSITION.md](DISPOSITION.md) | the gap-by-gap disposition, verified source chain, and the decision list |
| [CACHE-STANCE.md](CACHE-STANCE.md) | the written answer for the cache-stance question |
| [counter_selection.py](counter_selection.py) / [counter-selection.json](counter-selection.json) / [counter-selection.txt](counter-selection.txt) | what an O3 pin set would contain, with the R1-R4 screens and the pin-table size |
| [runner_verdict.py](runner_verdict.py) / [runner-verdict.json](runner-verdict.json) / [runner-verdict.txt](runner-verdict.txt) | `runner.py`'s own functions replayed over the retained evidence: pin verdicts, budget classification, published status under each option |
| [state_provenance.py](state_provenance.py) / [state-provenance.json](state-provenance.json) / [state-provenance.txt](state-provenance.txt) | the state-1-silent-read fingerprint of `CreatedInSample` |
| [CHECKS.md](CHECKS.md) | exactly what ran, what did not, and why |
| [custody.json](custody.json) | checkout identity, toolchain, SHA256 of every input read, corpus manifest re-verification |
| [issue-update.md](issue-update.md) | the #190 update body |

## What this round did not do

No product, harness, test, build or measurement change. No pin written, no value
hand-edited, no cold claim, no cache pooled, no selection shrunk, no timeout
enlarged, no case re-run, no diagnostic cap promoted. The history rows were not
added to `registry::cases()`, `FROZEN_CARDINALITY` or the 217-row self-check. The
`history.*` lanes were **not** run through the runner, because a history
performance command cannot fit the frozen budgets and a run that cannot fit is
recorded `NOT_RUN` rather than started
([runner-verdict.txt](runner-verdict.txt) carries the measured walls).

## Identities and reproduction

Isolated worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual`, branch
`codex/190-admission-qualification`, HEAD **437683aa054d4cbd618d9f7abba509cd88660f54**
(contains #202 `4049e28b6` / `3cec2f8da` / `5c6f15ba5` / `20fd68918` / `40054a403`,
then `dfe587912`, then `437683aa0`). Rust 1.85.1, Python 3.14.3, `--locked`, no
third-party change. Corpus manifest SHA256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` re-verified in
[custody.json](custody.json); tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`.

Reproduce every figure in this directory with:

```
python3 counter_selection.py && python3 runner_verdict.py && python3 state_provenance.py
```

Each script reads only the retained campaign (`stage-6-history-190-read-20260920T042617Z`)
and the tree's own `tests/golden/expected.tsv`; none writes to the harness or the
golden table. `runner_verdict.py` imports `runner.py` and calls its real functions;
it does not invoke the runner.

## Limits

The invariance screen has **one** legitimate change in it, so it bounds hazard, it
does not prove invariance — the golden table's own header records a legitimate
storage change that moved a pinned counter (`counter:pool.commits` 19 → 18 at
`795fb1a2f`). `history-stride1` was never measured in any campaign; its pin-table
projection is arithmetic over the shared per-state code path and is labelled
`NOT_MEASURED`. The status matrix is the runner's *rules* replayed on retained
artifacts, not a runner receipt: no history row has ever produced one. The corpus
residency diagnostic specified in CACHE-STANCE.md §3 is specified, not implemented,
and no residency or device-read number for the corpus exists anywhere in the
retained evidence.

## Production LOC

**85,725 → 85,725 (delta 0)**; reference 65,417, core 20,308. Documentation and
evidence only; same counter (`tools/production_loc.py`), same scope (first-parent
and final staged `crates` + `core/crates` snapshots, runtime SQL included, tests,
harness, docs, tools and generated artifacts excluded). See [CHECKS.md](CHECKS.md).
