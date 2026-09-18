# Phase 2 implementation handoff: drive #178's DB/engine phase

> **Status:** Executable assignment, 2026-09-18. You drive
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178)'s **Phase 2** —
> the structural split `P2-0`, the three instruments `V5`–`V7`, and `P2-1`..`P2-8`.
> Phase 1 is closed by owner ruling, and **the rulings Phase 2 needs are already
> made** (§5) — they are decisions, not defaults, and you do not re-litigate them.
> The parked register (producer pool, group target, branch-row summaries,
> pack-BLOB chunking, membership single-hash, persisted pool cursor) is **not
> yours**; neither are `P1-13`/`P1-15`, which the owner ruled closed as
> not-big-impact.
>
> **How you work (owner instruction): you are a SINGLE agent in DeepSeek Harness
> (DSH).** Do not launch, delegate to, or coordinate with any subagent, and do not
> invoke any external coding-agent or AI CLI for implementation, verification or
> review. Every action is your own, through the harness's own tools. Because you
> are alone, your verification is **author-verification and must be labelled as
> such**: the honest evidence is the **reproducible receipt** — a named tree, a
> command, and its output that any reader can re-run — never your word.
>
> **Terminal condition (the only definition of done):** every `P2-0`, `V5`–`V7`
> and `P2-1`..`P2-8` box on #178 is closed by its own single-variable, measured
> commit with an append-only receipt showing the counter movement its plan section
> predicts; the 34-test sealed-oracle parity set is green and unchanged
> throughout; every commit carries its architecture-document update and its
> measured production-LOC delta; the closing pass (§8) on the final tree is
> complete with every check answered; and the #178 boxes link their receipts. Any
> item that measurement shows to be pointless is closed **measured-and-declined**
> with its receipt — that is a completed item, not a missing one.

## 1. Where you start

| | |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main` |
| **The plan (read it whole, then per item)** | [`phase2-implementation-plan-20260918.md`](phase2-implementation-plan-20260918.md) — §1 the checklist with each item's gate, §2 the prerequisites in detail, **§3.1 the implementation shape** and §3.2 the verified sites, §4 what may and may not become configurable, **§5 the final file/folder structure** and the physical-line budget, **§6 the LOC accounting**, §7 the landing order, §8 the verification protocol, §9 the rulings |
| Rulings | [#178 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178#issuecomment-5731834780) — the owner's nine answers, reproduced in §5 below |
| Registers (for *why*, never for sites — the register's paths are partly stale) | [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176) complexity + round trips; [#177](https://github.com/Ephemeral-AI-Lab/layerfs/issues/177) parallelism + batching |
| Before anchors | Phase 0: [`../evidence/phase0-baseline-20260917T221759Z/`](../evidence/phase0-baseline-20260917T221759Z/) (`CONTRACT.md`, `RUNPLAN.md`, `receipts/p0-3-counter-baseline.md`). Phase 1: [`../evidence/phase1-execution-20260918T090000Z/`](../evidence/phase1-execution-20260918T090000Z/) (`CONTRACT.md`, `collect.py`, `ROUND-README.md`, `rounds/`) — **reuse this frozen workload set, its contract and its probe client; do not invent a new one** |
| Tracker | #178 — tick a box only with its receipt link |

Read first, in this order: `AGENTS.md` (the measurement rules are absolute) ·
`core/AGENTS.md` (product-source rules, the 999/200-line limits,
architecture-documents-follow-the-code) · the plan · this handoff · the Phase 0 and
Phase 1 evidence above · then `gh issue view 178`.

**What Phase 1 leaves you, usable as-is:** the frozen workload set and its
frozen contract; the per-tree probe client (`collect.py`'s `B2` rebuilds it and
records its sha256 in `artifacts.txt` — a stale client invalidated one Phase 1
round, so never reuse a `/tmp` copy); the vehicle extensions `V1`–`V4` and the
additive `M4`/`--case split` row; the eight checks; the per-commit production-LOC
discipline; and 18 `verify-*.md` files as the worked form of author-verification.

## 2. The loop (one item per round, in the landing order)

```text
ORDER
  P2-0  the owner.rs split            (first: it is a pure move, and it creates
                                       the room P2-2 and P2-5 need)
  V5    statement counter             (before P2-2)
  V6    group-decode counter          (before P2-4)
  V7    save-connection cache         (before P2-1's write-path claim)
  P2-8  append_fits running total     (small, independent)
  P2-6  hash once per wave            (small, independent)
  P2-7  drop copy_run                 (only after the aliasing proof)
  P2-4  decoded-group cache           (read path; needs V6)
  P2-5  batched presence + one reader (read path; same pooled-lane state as P2-4)
  P2-2  multi-row INSERT              (write path; needs V5)
  P2-1  cache_size + cache_spill      (profile; read half first, V7 for the write half)
  P2-3  locking_mode = EXCLUSIVE      (profile; write owner only)
```

Read-path (`P2-4`, `P2-5`) and write-path (`P2-2`) work may interleave with a
Phase 1 continuation round if the paths are measured separately: they share no
counter and no file.

```
ROUND
  1  take the next item in the order above; re-read its plan section (§2 for a
     prerequisite, §3.1/§3.2 for an item) and its register entry
  2  identify the item's before-anchor on the frozen set and its counter BEFORE
     implementing — Phase 1's P1-3 prediction was refuted precisely because the
     prediction was written down first
  3  implement it yourself, exactly per the plan; ONE commit: product change +
     its new tests + the architecture-document update in the SAME commit + the
     measured production-LOC disclosure
  4  run the checks that cover the change (§4) plus the whole-core set
  5  collect the receipt into the append-only round directory
     docs/roadmap/0.1/0.1.7/evidence/phase2-execution-<stamp>/rounds/<item>/
     (receipt.md + verify-<item>.md), with CONTRACT.md frozen before collection
  6  run the SINGLE-AGENT VERIFICATION PROTOCOL below and write verify-<item>.md
     yourself, labelled author-verified
  7  tick the #178 box with the receipt link; if a falsification check failed,
     remedy and re-verify before ticking
  8  when the last box is ticked, run the closing pass (§8)
```

### The single-agent verification protocol (replaces a second reviewer)

1. **Reproduce from a clean tree.** `git archive <commit> | tar -x -C
   /tmp/verify-<item>`, build there with the frozen identity discipline, run the
   frozen workload, confirm the after-counter. The receipt names tree + command +
   output; that trio is the evidence.
2. **Falsify the change**, answering each in `verify-<item>.md`:
   - the new test **fails on the parent tree** (archive the parent, run it, show
     the failure, return);
   - the counter moved in the **predicted direction and rough magnitude**, and the
     other counters on the same workload did not move unexplained;
   - the parity set is green and `git diff <parent>..<commit> -- '*tests*'` shows
     only the new tests — **no re-pinned parity test**;
   - the commit is single-variable (`git show --stat` touches only the plan's
     files);
   - the item's named risks are probed one by one (§3.2 lists them; e.g. P2-2's
     per-row error attribution, P2-4's ceiling-before-cache order, P2-5's reader
     state across trials);
   - `P2-0`'s own falsification is the absence of a test diff: a pure move cannot
     need one.
3. **Write `verify-<item>.md`** with the label **author-verified** (never
   "independent"), every command and exit code, the falsification answers, and an
   explicit UNVERIFIED list. If a check cannot be run in this environment, say so
   rather than skipping it silently.

## 3. Per-item acceptance (what each box needs)

| item | gate (deterministic work counter) | the receipt must also show |
| --- | --- | --- |
| **P2-0** | every counter bit-identical before/after on the frozen set | **no test file changed**, parity green, `check_product_boundary.py` PASS, LOC delta = imports + visibility markers only, labelled **relocation** |
| **V5** | statements ≈ rows on `c2.ceiling` (before) | the counter proven live by a control; `SaveOutcome`/vehicle print additive; no other counter moves |
| **V6** | decodes > distinct groups (before) | charge site named; existing read counters unchanged |
| **V7** | spill counter proven live on a forced-small cache, then read on the product's own save connection | the accessor is bounded and documented; the profile itself unchanged |
| **P2-1** | A/B one save at the transaction ceiling; read-path case first | per-connection memory declared; `V7` for the write-path claim; may close **measured-and-declined** |
| **P2-2** | statements `r` → `⌈r/k⌉`; `commits` unchanged | `k` derived from SQLite's limits (not hardcoded); per-row error attribution preserved; `sqlite_master` unchanged |
| **P2-3** | commits / statements / wall on the same save | write-owner only; a read-only Store refuses it; the multi-store test surface exercised |
| **P2-4** | decodes → distinct groups | ceiling checked **before** any cache hit (pin it in `visibility.rs`); the cache's bound/lifetime/release documented like the pooled one |
| **P2-5** | `edges` / pages and V6 | one presence query per wave; no reader state leaks across trials (a test) |
| **P2-6** | one hash per requested object | tamper detection unchanged (a mismatched hash still fails) |
| **P2-7** | `rows_written` / `runs_created` move | the aliasing proof is a test that fails if `merge_runs` writes into an input; output byte-identical; may close **not delivered** if the proof fails |
| **P2-8** | `packs_created` / `pack_appends` and emitted pack bytes identical | a boundary test at `pack_limit` pins the first-fit decision |

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

(`--detail` instead of `--files` gives the per-crate view the commit message quotes;
`--files` is the per-file view when a move has to be shown as a move.)

This repository runs **no CI and no aggregate preflight** (both retired by owner
decision). Report exactly which checks ran, which did not, and why — never claim
"CI green". State the production-LOC line in every commit message exactly as the
root `AGENTS.md` requires:

```text
Production LOC: <before> -> <after> (delta <signed>). Scope: core/crates/*/src plus
shipped SQL, replacement core; reference crates/ unchanged at 65417, combined <total>.
Method: tools/production_loc.py over the staged tree.
```

## 5. Rulings you inherit (decided 2026-09-18 — do not re-litigate)

1. **`P2-0` authorised — split `cas/owner.rs`.** The full responsibility split:
   `pool_lane` / `placement` / `lifecycle` / `selection`, with `owner.rs` reduced
   to the state struct, `OutcomeCounters` and the demand path.
2. **`V5`–`V7` authorised — add them.** Instrument-only commits, before their
   dependents; the same class Phase 1 approved as `V1`–`V4`.
3. **`P2-1` authorised — 32 MiB is fine.** The reference profile (`cache_size`
   32 MiB, `cache_spill` OFF); read-path half first, `V7` gates the write-path
   claim.
4. **`P2-3` scope — the conservative form.** `locking_mode = EXCLUSIVE` on the
   save owner's connection only, never on a read connection; the reconciliation is
   recorded in the item's receipt.
5. **`P1-13` / `P1-15` — closed as not-big-impact.** Do not implement them, do not
   redesign the ordering store; their receipts stay on disk with their findings.
6. **Single worker confirmed** for every Phase 2 measurement. `P2-2` stays a
   batching change; no run raises `LAYERFS_CONSTRUCTION_WORKERS`, and the parked
   producer pool stays parked.
7. **The parked register stays parked**, and the pending-ceiling default stays as
   P1-16 documented it.
8. **`P1-7`'s third pinned test is accepted** as a deviation.
9. **`P1-10`'s architecture-document gap** was not ruled on: the default is to add
   the `ordering_bytes / 16` update; if the owner corrects that, follow the issue.

Standing Phase 0 disciplines you inherit: work counters gate, `elapsed` is
diagnostic-only (P0-3 measured +17.6% on the same binary and input); the ordering
items keep the forced-64 anchor; frozen-workload one-sample-per-case-per-arm with
a fresh `--output`.

## 6. Anti-patterns that will fail the phase

- **Starting an item before its gate exists.** `P2-2` without `V5` and `P2-4`
  without `V6` can only be argued, not measured.
- **Making a declared number a knob.** No environment variable or per-open option
  for the connection profile or a worker count (plan §4f); no knob for anything
  that changes emitted partitions (§4e).
- **Widening visibility beyond `cas`.** `pub(super)` in `owner.rs` means
  `pub(in crate::cas)`; `pub(crate)` or `pub` is not needed by any sibling.
- **Adding a test-only branch to product source.** The boundary checker rejects
  `#[cfg(test)]`, test-only attributes and fenced doc examples; verification lives
  in `core/crates/*/tests/`.
- **Re-pinning a parity test to make an item land.** Canonical bytes are
  identical before and after every item. A pinned test that must move is a design
  failure: rework the item.
- **Bundling.** One item, one commit, one variable; a counter bug or dead code
  found beside your item is reported on #178, not fixed inside the item's commit.
- **Gating on elapsed time**, or quoting a lifetime counter as a phase number.
- **Deleting or rewriting a receipt**, including one that records a failure; the
  evidence directory is append-only, corrections are appended with a date.
- **Raising a worker count, timeout or cache policy to turn a failing case
  green**, and **dropping a failing cell** from a report.
- **Leaving a half-landed item.** The P1-13 precedent is the form: reverted in
  full, tree clean, tests green, the finding recorded in the receipt.

## 7. If you cannot finish

Record it, do not hide it. Every box ends in exactly one of five states, each with
its receipt: **landed** (counter moved as predicted), **landed with a correction**
(the plan's sketch was wrong; the receipt says which and why), **measured-and-
declined** (measurement shows the item is pointless), **not delivered** (the gate
could not be satisfied — P2-7's aliasing proof is the expected case), or
**blocked** (name the blocker and the evidence, as P1-13 did before its ruling).
Then stop and report; a stopped phase with honest receipts is worth more than a
ticked box without one.

## 8. The closing pass (after the last box)

1. Re-collect the frozen set on the final tree; tabulate every item's prediction
   against its measured movement, including the misses.
2. Re-verify the parity set (34 tests), the commit audit (one variable each, no
   unplanned re-pins), the gate audit (work counters, not elapsed), and the scope
   audit (nothing from the parked register or Phase 1's leftover items was pulled
   in).
3. Confirm every commit's production-LOC line and the architecture-document update;
   report the phase total (core before → after, signed delta, method).
4. Write the closing record in this directory, tick the remaining #178 boxes with
   their receipt links, and post the summary on the issue — PASS, INCOMPLETE,
   DECLINED and unrun work reported as plainly as the wins.
