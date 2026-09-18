# Phase 1 terminal handoff: drive #178's algorithm phase to a clean pass

> **Status:** Executable assignment, 2026-09-17. You drive
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178)'s **Phase 1**
> (the 16 algorithm/call-pattern items plus the four prerequisite commits) to
> its terminal condition. Phase 0 is complete; **Phase 2 is not yours** — do not
> start it, do not tune a constant, do not touch a parked item.
>
> **How you work (owner instruction): you are a SINGLE agent in DeepSeek Harness
> (DSH).** You must not launch, delegate to, or coordinate with any subagent —
> no `subagent`, `subagent_fork`, workflow or similar tool is to be used for any
> part of this assignment — and you must not invoke any external coding-agent or
> AI CLI (`codex`, `claude`, or anything like them) for implementation,
> verification or review. Every action is your own, through the harness's own
> tools: `bash` for commands, the file tools for edits. Because you are alone,
> your verification is **author-verification, and must be labelled as such** —
> the honest evidence is the **reproducible receipt** (a command any reader can
> re-run on the named tree to get the same counter), never your word. §2 and §5
> define the single-agent verification protocol that replaces independent
> reviewers.
>
> **Terminal condition (the only definition of done):** every prerequisite and
> every P1-1..P1-16 box on #178 is closed by its own single-variable, measured
> commit with an append-only receipt showing the counter movement its plan
> section predicts; the 34-test sealed-oracle parity set is green and unchanged
> throughout (except the two pre-authorized pinned updates); the closing
> self-falsification pass (§5) on the final tree is complete with every check
> answered; the documents record the final state; and the #178 boxes link their
> receipts. Work iteratively until that holds.

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
  2  implement it yourself, exactly per its section in the plan + reports; one
     commit: product change + its new tests + the pinned-test update if
     pre-authorized + the architecture-doc update in the SAME commit + the
     LOC disclosure
  3  run the checks that cover the change (§4), plus the whole-core set
  4  collect the item's receipt: before (Phase 0 anchor / re-baseline) vs after,
     on the frozen workload set, into the append-only round directory
     docs/roadmap/0.1/0.1.7/evidence/phase1-execution-<stamp>/
  5  run the SINGLE-AGENT VERIFICATION PROTOCOL on the item (below), and write
     verify-<item>.md yourself, labelled "author-verified"
  6  tick the #178 box with the receipt link; if any falsification check
     failed, remedy and re-verify before ticking
  7  when the last box is ticked, run the closing pass (§5)
```

### The single-agent verification protocol (replaces a second reviewer)

Because no independent agent will check your work, the receipt must be strong
enough that any reader can check it, and you must actively try to break your
own change before ticking. For each item, in order:

1. **Reproduce from a clean tree.** Export the committed tree to a fresh
   directory (`git archive <commit> | tar -x -C /tmp/verify-<item>`), build
   there with the Phase 0 identity discipline, run the frozen workload, and
   confirm the after-counter. The receipt names the tree, the command and the
   output — that trio is the evidence, not your assertion.
2. **Falsify the change.** Work through the falsification list, recording each
   answer in `verify-<item>.md`:
   - the new test **fails on the parent tree** (check out or archive the
     parent, run it, show the failure, return);
   - the counter moved in the **predicted direction and rough magnitude**, and
     the *other* counters on the same workload did not move unexplained;
   - the parity set is green and `git diff <parent>..<commit> -- '*tests*'`
     shows only the pre-authorized pin change (if any) and the new tests;
   - the commit is single-variable (`git show --stat` touches only the plan's
     files; nothing rides along);
   - the error paths: the malformed input, the refusal, the empty case —
     the item's plan section names its risks; probe each one;
   - the elapsed column of your receipt is labelled diagnostic, never a gate.
3. **Write `verify-<item>.md`** with: the label **author-verified** (never
   "independent" — you wrote the code), every command with its exit code, the
   falsification answers, and an explicit UNVERIFIED list for anything you
   could not check. If a check cannot be performed in this environment, say so
   rather than skipping it silently.

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
- **Never mark a box on your own word.** A box flips only when the receipt is
  reproducible by any reader (named tree + command + output), the falsification
  list is answered in `verify-<item>.md`, and the closing pass (§5) confirms it.
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
  counting-allocator peak ≤ ~2n; P1-7: D27 `nodes_read` 9 → ≈7);
- the new test passing **and failing on the pre-item tree** (falsification
  check 2a — the failure run is part of the receipt);
- the parity set green; the eight checks green; the LOC line in the commit
  message; the architecture-doc update in the same commit;
- the completed `verify-<item>.md`, labelled author-verified.

Special cases:

- **C1** lands with both counter fixes and an append-only re-baseline of the
  affected Phase 0 rows (the old values stay, labelled inflated ×2 for batch
  reads).
- **P1-12** has no counter: its commit shows the incremental-arithmetic
  argument in the message, cites the loop, and its `verify-<item>.md` records
  the invariant argument and the loop's line citations — say so plainly on
  #178.
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

## 5. The closing pass (self-falsification, single agent)

When the last box is ticked, perform the closing pass yourself — no subagents.
Each check gets a written answer in `closing-pass.md` in the final round's
evidence directory:

1. **Boxes ↔ receipts:** every prerequisite and P1 box on #178 links a receipt;
   every receipt links a box; nothing is ticked without one.
2. **Counter re-run:** on the final tree, re-run the full frozen workload set
   once and tabulate every item's before → after counter beside its plan
   prediction; any counter that did not move as predicted is flagged and
   reconciled (the plan's targets are hypotheses — a refuted one is recorded,
   not explained away).
3. **Parity audit:** `git diff <phase1-start>..<final> -- '*tests*'` contains
   only the two pre-authorized pin updates and the new tests; the sealed-oracle
   set green; `git diff --stat` over product files matches the plan's file map.
4. **Commit audit:** every commit in the range is single-variable, carries its
   LOC disclosure (re-run `tools/production_loc.py` on a sample of three and
   compare), and carries its architecture-doc update.
5. **Gate audit:** grep the receipts for any elapsed figure quoted as a gate;
   grep for any counter "fixed" outside C1; confirm every verification file is
   labelled author-verified.
6. **Scope audit:** no Phase 2 change, no parked item, no default change, no
   worker/timeout/cache tuning anywhere in the range.

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

- Launching or delegating to any subagent, or invoking any external agent CLI
  (`codex`, `claude`, or similar) — this assignment is single-agent by owner
  instruction.
- Ticking a box from a passing suite or an attractive diff instead of the
  counter receipt.
- Calling your own verification "independent" — it is author-verified by
  construction; the receipt's reproducibility is the evidence.
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
