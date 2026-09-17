# Phase 1 terminal handoff: drive #178's algorithm phase to a clean pass

> **Status:** Executable assignment, 2026-09-17. You drive
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178)'s **Phase 1**
> (the 16 algorithm/call-pattern items plus the four prerequisite commits) to
> its terminal condition. Phase 0 is complete; **Phase 2 is not yours** — do not
> start it, do not tune a constant, do not touch a parked item.
>
> **Terminal condition (the only definition of done):** every prerequisite and
> every P1-1..P1-16 box on #178 is closed by its own single-variable, measured
> commit with an append-only receipt showing the counter movement its plan
> section predicts; the 34-test sealed-oracle parity set is green and unchanged
> throughout (except the two pre-authorized pinned updates); a closing
> verification-subagent pass on the final tree returns zero open findings; the
> documents record the final state; and the #178 boxes link their receipts. Work
> iteratively until that holds.

## 1. Where you start

| | |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main` |
| Plan | [`phase1-implementation-plan-20260917.md`](phase1-implementation-plan-20260917.md) — the checklist, Big-O targets, landing order, rulings. **Detail per item:** the four reports under [`../evidence/phase1-planning-20260917T230000Z/`](../evidence/phase1-planning-20260917T230000Z/) (`plan-nav-validate.md`, `plan-edit-assembly.md`, `plan-ordering.md`, `test-evaluation.md`) — these are the implementation plan; follow them item by item |
| Before anchors | Phase 0 receipts: [`../evidence/phase0-baseline-20260917T221759Z/`](../evidence/phase0-baseline-20260917T221759Z/) — `receipts/p0-3-counter-baseline.md` (D1–D27), `CONTRACT.md`, `RUNPLAN.md`. **Your receipts reuse this frozen workload set and its contract** |
| Tracker | #178 — tick a box only with its receipt link |

Read first: `AGENTS.md` (measurement rules are absolute), `core/AGENTS.md`
(product-source rules, 999/200-line limits, architecture-docs-follow-the-code),
the plan document, the four reports, the Phase 0 receipts, then
`gh issue view 178`.

**Phase 0 rulings you inherit** (owner-overridable on #178; if the owner
overrides on the issue, follow the issue): O1 deprioritized (no spilling —
P0-2); the vehicles V1–V3 are built first; P1-5/P1-13/P1-15 stay anchored on
the forced-64 shape with exact in-test counter assertions as the regression
guard; the `read_waves` double-count (`objects.rs:107` beside `note_read:124`)
is a bug and is fixed in C1.

## 2. The loop (one item per round, in the unified order)

```
ROUND
  1  take the next item from the landing order (plan §2):
        C1 → V1 → V2 → V3 → P1-2 → P1-1 → P1-3 → P1-4
        → P1-6 → P1-9 → P1-8 → P1-7 → P1-14 (P1-12 anywhere)
        → P1-5 → P1-13 → P1-15 → P1-10 → P1-16
        (P1-8 before P1-7: the comparing cursor builds on the ordered
         cursor — the fused-descent design for P1-7 was REJECTED because it
         would publish before an Equal verdict is known, breaking the
         zero-emission contract pinned at edit_noop.rs:82)
  2  implement it exactly per its section in the plan + reports; one commit:
     product change + its new tests + the pinned-test update if pre-authorized
     + the architecture-doc update in the SAME commit + the LOC disclosure
  3  run the checks that cover the change (§4), plus the whole-core set
  4  collect the item's receipt: before (Phase 0 anchor / re-baseline) vs after,
     on the frozen workload set, into the append-only round directory
     docs/roadmap/0.1/0.1.7/evidence/phase1-execution-<stamp>/
  5  launch a read-only verification subagent for the item (fresh context; its
     only write is verify-<item>.md in the same directory); adjudicate every
     finding at its path:line before acting on it
  6  tick the #178 box with the receipt link; if a finding was real, remedy and
     re-verify before ticking
  7  when the last box is ticked, run the closing pass (§5)
```

Rules that make the loop honest:

- **The parity invariant is absolute.** Persisted canonical bytes are identical
  before and after every item. The 34-test sealed-oracle set stays green and
  unchanged. The only pre-authorized pinned updates: C1's counter semantics
  (with its append-only re-baseline) and P1-1's `filesystem_bounds.rs:219`
  width bound. Anything else that would change a pinned test is a design
  failure — rework the item, do not re-pin the test.
- **One item, one commit, one variable.** No bundling, no drive-by fixes; a
  found counter bug or dead code beside your item is reported on #178, not
  silently fixed inside the item's commit (the C1 precedent shows how
  prerequisites are split out).
- **Gates are deterministic work counters, never elapsed.** Phase 0 proved
  bit-identical counter re-runs and a +17.6% same-binary elapsed spread —
  elapsed is diagnostic-only, labelled as such in every receipt.
- **Never mark a box on your own word.** A box flips only when the receipt
  exists and a verification subagent that did not write the code has reproduced
  the counter movement through public entry points.
- **Never silently drop an item.** If an item turns out wrong or pointless,
  record it on #178 as measured-and-declined (or blocked) with the receipt and
  the reason — the plan's targets are hypotheses the receipts confirm or
  refute.

## 3. Per-item acceptance (what each box needs)

Each item's section in the plan names its Big-O target, before anchor, change
sketch, tests and verification. The receipt for each box must show, at minimum:

- the **before** value (the Phase 0 D-row, or the V-baseline for items whose
  observable was `NOT_EXPOSED`) and the **after** value on the same frozen
  workload, same counter, one sample, deterministic re-run labelled;
- the counter moving in the **predicted direction and rough magnitude** (e.g.
  P1-5: forced-64 per-doubling `rows_read` ×3.0 → ~×2.1–2.4; P1-13:
  `merges` 61 → ~half-scale; P1-2: V3's opens O(waves) → O(1); P1-14: the
  counting-allocator peak ≤ ~2n);
- the new test passing **and failing on the pre-item tree** (demonstrate once
  per item: check out the parent, run the new test, show it fail, return);
- the parity set green; the eight checks green; the LOC line in the commit
  message; the architecture-doc update in the same commit.

Special cases:

- **C1** lands with both counter fixes and an append-only re-baseline of the
  affected Phase 0 rows (the old values stay, labelled inflated ×2 for batch
  reads).
- **P1-12** has no counter: its commit shows the incremental-arithmetic
  argument in the message, cites the loop, and its "receipt" is the
  review-subagent's verification of the invariant — say so plainly on #178.
- **P1-16** delivers documentation + a high-ceiling boundary test; **changing
  the default pending ceiling is an owner ruling on #178, not yours.**
- **P1-4** must demonstrate `entries_examined` bit-identical and pin the
  error-order behaviour; if the design cannot avoid an error-order change,
  stop and post the finding (§6).

## 4. Checks for every commit

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

Report every exit code and the test counts. Receipt collection additionally
respects the measurement lock (run alone), fresh outputs, one sample per
workload, and the Phase 0 contract's identity discipline (clean tree, release
profile, `+1.85.1`, `--locked`, one worker).

## 5. The closing pass

When the last box is ticked: launch closing verification subagents — one per
plan group (prerequisites, navigation+validate, edit path, ordering), one over
the receipts as a whole, and one falsifier that tries to break the terminal
condition (any box without a receipt, any receipt whose counter does not show
the claimed movement, any parity test quietly changed, any bundled commit, any
elapsed figure quoted as a gate). Adjudicate everything; remedy; re-verify.
Then: update the plan document with the final counter table (before → after
per item), post the summary on #178, and **stop** — Phase 2 is not started by
this handoff.

## 6. If you cannot finish

Post on #178 with: the item, the exact failing artifact, the command and its
output, the constraint that blocks it, and the dispositions you need from the
owner (extend scope, waive in writing, or change the contract). An honest
`blocked` or `measured-and-declined` box with its reason is an acceptable
outcome; a green-looking checklist with an unresolved item is not.

## 7. Anti-patterns that will fail the closing pass

- Ticking a box from a passing suite or an attractive diff instead of the
  counter receipt.
- Quoting elapsed nanoseconds as a gate.
- Bundling two items into one commit, or "fixing" a counter so a number looks
  better (C1 is the only authorized counter-semantics change, and it
  re-baselines).
- Changing a pinned test that the plan did not pre-authorize.
- Touching Phase 2, the parked register, the pending-ceiling default, a worker
  count, a timeout, or a cache policy.
- Re-running a workload until the number looks right; pooling warm/cold;
  editing a receipt after the fact.
- Starting Phase 2 work "while you're in there".
