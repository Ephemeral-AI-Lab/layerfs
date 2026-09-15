# Handoff prompt — v0.1.6 six-family rollout (#154, for #122)

You are the rollout agent for **[#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154)**:
port [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122)'s six families onto
the current `main` (the sandbox-local snapshot route), then **run each family,
fix what breaks at its root cause, and report the measured results** — phase by
phase, family by family, until every required row is terminal. Read #154 and #122
first: they are the contract. This prompt is how to execute them.

## Read before you touch anything

### Normative rules — read fully, they bind this rollout

| file | what you need from it |
|---|---|
| `AGENTS.md` | the repo-wide rules: no warm-cache credit, `--setup clone` reuse discipline, budgets, single construction worker, third-party ban, no-CI/preflight |
| `benchmark/AGENTS.md` | benchmark-tree mechanics: prepared inputs, `--reuse-pass`, clone semantics, budgets, the scoped v0.1.6 hosting exception |
| `docs/general/benchmark_rules.md` | the measurement contract: cache state, memory domains, reuse, reporting fields |
| `docs/general/release-policy.md`, `docs/general/documentation-policy.md` | what may be claimed, committed and released |
| `benchmark/fs-bench-pro/QUICKSTART.md` | build/reuse/run mechanics, `--setup fresh` vs `--setup clone`, family entry points |
| `tools/preflight.sh` | the local gate you run before **every** push — this repository has no CI |

### This rollout's contract

| file | what you need from it |
|---|---|
| issue **#154** | the phase order, the per-family loop and the reporting protocol you execute |
| issue **#122** | the requirement of record: 33 regular cases, 3 extensions, budgets, the honest-reporting contract; it receives the final matrix |
| the frozen specification, restored from `7b73c4b33`: `docs/roadmap/0.1/0.1.6/{cases.json,check_plan.py,benchmark-families.md,fixtures.md,workloads.md,execution-and-verification.md,review-decisions.md,benchmark-exclusions-issue122.json}` | exact case membership (36 cases), fixture algebra, the five-stage M1 schedule, topology cardinalities, oracles and deadlines. `cases.json` is the membership authority; `check_plan.py` is the product-free check |
| the archived implementation `7b73c4b33` (tag `archive/main-branch-cleanup-20260915-1005/heads/archive/v016-overlay-7b73c4b33`) | the code you port: three family dirs, two extensions, the v016 workload/oracle/host-orchestrator, the fixture-preparation and matrix tooling |
| `benchmark-results/v016/{ISSUE-122-REPORT.md,issue-122-update.md,issue-122-update-2.md,matrix.json}` | the discarded campaign's last state (21/33 at seed 1) — **reference only**, never evidence: its product was replaced and its receipts carry a control-arm seal |

### The architecture you are now measuring

The v0.1.6 direction is the sandbox-local snapshot route (`main`), not the archived
host-overlay line that produced the v016 harness. Read
`docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` §2 (ownership),
§3.4 (volatile contract), §5 (resource gates) and §6 (cases/timing); then the
commit path you will be exercising: `crates/layerfs-workspace/src/{lifecycle.rs,remote_commit.rs,changes.rs,snapshot_input.rs}`
and the sandbox lane `crates/layerfs-fuse/src/{live_owner.rs,local_spool.rs,live_backing.rs}`.
Commit is sandbox-owned: the sandbox owns mutable state and replacement bytes, and
the host keeps the Store and the canonical builder.

## Hard rules — not negotiable

1. **One sample per case, per arm, per mode. Always.** No n3, no "until stable",
   no medians over repeats, no re-running a case because the first number looked
   slow. The harness is a single-sample harness by design; stability hunting is the
   opposite of this rollout's fast path, and it also breaks the measurement
   contract. One perf invocation and one separate verify invocation is the whole
   budget for a case at a seed.
2. **One sample does not mean one attempt at the *work*.** You may iterate as many
   times as you need on *fixing* a defect; you may not iterate on *sampling*. Fix,
   rebuild, and take exactly one new sample on the new identity.
3. **Cache state is declared and equal.** No warm-start credit, no priming the
   paths a timed phase reads, no cold/warm pooling. `--setup clone` is required for
   every post-initialization case; `--setup fresh` only for initialization and
   fresh-output cases.
4. **Budgets.** A regular perf invocation and its separate verify invocation must
   each complete within **15 s**, or within a declared, measured **≤25 s** exception
   that you list by case. Anything else is `NOT_RUN` with its measured wall time.
   Never shorten K100, never weaken a workload, never move a valid miss to extended.
5. **One construction worker** for every case except `init_namespace`. No run raises
   `LAYERFS_CONSTRUCTION_WORKERS`; no helper lane or second worker is added to pass a gate.
6. **Never loosen the thing you are testing.** Oracles, assertions, expected values,
   limits and timeouts are fixed by the frozen specification. If a check fails,
   either the product or the check is wrong — decide which, with evidence, and fix
   *that*. A check modified to pass is a failed rollout.
7. **Receipts are append-only.** Fresh output paths per run; superseded attempts are
   archived beside the current one, never deleted or overwritten. A performance
   `PASS` is not release admission.
8. **Reuse setup, never measurement.** Prepared inputs, sealed builds, image layers
   keyed by compilation seal, `binary-archive/<sha256>/` executables and
   `--reuse-pass` are the reuse surface. Reuse that removes work *inside* a timed
   phase is cheating; reuse that removes work outside it is required.
9. **Identity discipline.** Every phase builds once and pins source commit/seal/tree,
   product, compilation and dependency seals, image ID, harness identity and
   workload-source hash. If the product seal moves, the affected matched arm must be
   rebuilt and its rows re-collected — never re-labelled.
10. **The measurement lock is shared.** Never overlap resource-sensitive work and
    never interrupt another owner's run.
11. **No CI claims.** Run `tools/preflight.sh` before every push; record the tree
    clean for sealed builds.
12. **No third-party patching, vendoring or forking.** Builds stay `--locked`.

## The fast path

Speed comes from *one sample* plus *short, complete loops* — not from skipping
work. Each iteration is:

```
implement/port → product-free checks → build once → 1 perf sample → 1 separate verify
  → triage every mismatch to a root cause → fix product or harness → regression test
  → rebuild → re-run only the affected rows → ledger entry → next family
```

* **Timebox triage.** One diagnostic cycle per failing case: read the first
  declared mismatch, reproduce it product-free if you can, name the owner (product
  or harness), then fix. If two hypotheses remain after one cycle, take the cheaper
  decisive experiment, not the longer one.
* **Batch the expensive steps.** Build once per identity, prepare fixtures once,
  and sweep the whole family's perf rows before its verify rows when the fix rate
  allows.
* **Never idle.** If a row is blocked on an owner decision, park it, record it, and
  work the next case, family or phase. The campaign stops only for the escalation
  list below.
* **Product-free first.** `rustc --test workload/main.rs`,
  `fs-benchmark-pro workspace-self-check`, `check_plan.py` and the family
  self-checks run in seconds. A workload that cannot describe its own expected
  state is not ready to measure, and never needs a container to find out.

## Phases

### Phase 0 — port the specification and the harness (no measurement)

1. Restore the frozen specification into `docs/roadmap/0.1/0.1.6/` from `7b73c4b33`;
   re-run `python3 docs/roadmap/0.1/0.1.6/check_plan.py` (expect: 33 regular /
   3 extended / 12 mixed / 10 fixtures).
2. Port the three family directories (`file_size_transition`,
   `multi_workspace_development`, `branch_development`), the two extensions
   (`dedup_branch_history`, `mixed_load_bearing`), the six `historical_access`
   additions, the v016 workload sources, the host orchestrator and oracle, and the
   fixture-preparation/matrix scripts — adapting only where `main`'s route differs.
3. Re-wire `workload/workspace_registry.rs` (12 families / 133 timed IDs → 14 / 154,
   declared counts and dispatch), `workload/main.rs`, `shared/runner.py`
   (`HOST_FAMILIES`), `verify-selected.py`, `shared/test_runner.py`,
   `shared/test_layout.py`.
4. Gates: `check_plan.py`, `rustc --test workload/main.rs`,
   `fs-benchmark-pro workspace-self-check`, `tools/preflight.sh`.
5. Commit the port (including this prompt) before taking any sample. **Report
   Phase 0 on #154**, then proceed.

### Phases F1–F6 — one family at a time

| # | family | cases to make terminal | why here | carried-over knowledge |
|---|---|---|---|---|
| F1 | `file_size_transition` | `v016-boundary-{small-control,below,exact,above,large-control,roundtrip,alias-roundtrip}-v1` (7) | smallest, previously 7/7 green: proves the port and answers the 15 s question first | none expected — a failure here is port/route breakage |
| F2 | `mixed_load_bearing` | `v016-mixed-development-{100mb-5000,500mb-30000}-k{10,100}-v1` (4) + extended `v016-mixed-exhaustive-{100mb-5000,500mb-30000}-k100-v1` (2) | highest schedule volume; owns the K100 budget decision | the overlay line's blocking defect was directory-mode persistence; the fix does not transfer by assumption — re-measure |
| F3 | `multi_workspace_development` | `v016-workspace-mixed-{100mb-5000,500mb-30000}-k{10,100}-v1` (4) + extended `v016-workspace-four-100mb-5000-k100-v1` (1) | two live workspaces, discard/reopen while the peer is dirty, then four workspaces | the discarded-session witness must follow the branch's own schedule |
| F4 | `branch_development` | `v016-branch-mixed-{100mb-5000,500mb-30000}-k{10,100}-v1` (4) + `v016-branch-convergent-content-v1`, `v016-branch-fork-descendant-v1` (2 compact controls) | fork/ancestry semantics; **the two compact controls are the sealed producers F6 consumes** | the child's first parent edge and forked-cycle inheritance were harness defects; the two controls were never implemented |
| F5 | `dedup_branch_history` | `v016-history-large-hotset-k{10,100}-v1`, `v016-history-namespace-inode-k{10,100}-v1`, `v016-history-boundary-cycle-k{10,100}-v1` (6) | large-file hotset, namespace/inode history, boundary-crossing history | `v016-history-namespace-inode` needs a host orchestrator that was never written |
| F6 | `historical_access` | `v016-access-{boundary-before,boundary-after,inode-before,inode-after,fork-point,divergent-head}-v1` (6, **verify-only**) | consumers of F4's sealed producer artifacts | the access route that mounts one selected retained state of a sealed producer was never implemented; performance is `N/A`, never 0 and never `PASS` |

For every family, in this order: port/implement → product-free checks → build once
and record the identity → one perf sample per case → one separate verify per case →
triage and fix → re-run affected rows only → ledger entry → **report the family on
#154**.

### Phase Q — seeds 2/3 qualification

After all six families are terminal at seed 1, collect the declared seed 2 and
seed 3 qualification for the regular matrix. No re-selection, no best-of, no
"first two looked good".

### Phase X — the three declared extensions

* `v016-mixed-exhaustive-100mb-5000-k100-v1` — verify-only, 120 s watchdog, perf `N/A`.
* `v016-mixed-exhaustive-500mb-30000-k100-v1` — verify-only, 300 s watchdog, perf `N/A`.
* `v016-workspace-four-100mb-5000-k100-v1` — 400 Commits across four workspaces,
  performance **and** verification separately, 60 s each.

If an extension was already made terminal during F2/F3, say so and cite the receipt
instead of re-running it.

### Phase R — final report

Aggregate on **#122**: per-case and per-family matrices, the identity chain,
verification coverage and omissions, and every non-passing row with its measured
reason. #154 carries the phase-by-phase history.

## Bug policy — a new architecture means bugs on both sides

Treat every mismatch as a defect to be located, never as noise:

* **Product defect** (the sandbox route publishes or behaves wrongly): fix it in
  the product, add a regression test that fails without the fix, re-run the affected
  rows, and record the before/after evidence. The v0.1.6 route is new: expect
  defects in truncate/size-change paths, mode and mtime persistence, hardlink and
  symlink class handling, open-unlinked lifetime, publication retry, and the
  single-worker construction path.
* **Harness/oracle defect** (the check describes the wrong expected state): fix the
  check, keep the product's real behaviour as the oracle's subject, and prove the
  fix by showing the check now agrees with an independently derived expectation —
  never by relaxing it to whatever the product produced. Four such defects were
  already found once (directory-mode measurement, the forked child's parent edge,
  the two-class alias oracle, exhaustive per-state alignment); expect the rest.
* **Route mismatch** (the ported workload calls a surface `main` no longer has):
  adapt the harness at the boundary, keep the declared operations and counters
  identical, and record what changed.
* **A fix that would require changing the workload, a limit, a timeout, a worker
  count or an oracle's expectation is not a fix.** Stop, escalate, keep going
  elsewhere.

## Reporting — a comment on #154 at every phase and family completion

Post the phase or family as soon as it is terminal, before starting the next one.
Each report carries:

1. **Identity**: source commit/seal/tree, product, compilation and dependency seals,
   image ID, harness identity, workload-source hash, and which arm was measured.
2. **The family's performance results**, one row per case: requested vs observed
   paths/bytes, local commits, total commits, ancestry, `Created` count, and
   `UpToDate` / `Busy` / `HeadMoved` / presentation failures / errors counted
   separately.
3. **Timing**: complete-command wall, preparation/workload/cleanup split where
   recorded, edits and Commits in raw units, max Commit latency, actual concurrency
   overlap; the 15 s gate verdict per row and any declared ≤25 s exception.
4. **Verification**: verdict, coverage and omissions, full-payload vs affected-set
   explicitly distinguished; the identity of any reused proof.
5. **Resources**: logical and physical bytes, Store growth, dedup/reuse, spool; host
   and container CPU/memory/I/O separately, with sampling caveats.
6. **Every non-passing row**, retained as `FAIL` / `TIMEOUT` / `NOT_RUN` / `N/A`
   with its measured reason and the fix status. A family with FAIL rows is still a
   completed phase — report it and move on.
7. **Defects found and fixed** in this phase: one line each — symptom, root cause,
   fix commit, regression test, rows re-run.
8. **The exact reproduction command.**

Never publish a success-shaped summary: unrun rows, waived targets, dirty seals and
incomparable baselines are stated as plainly as passes.

## Do not stop

Work iteratively until **terminal pass**: all 33 regular cases have a performance
result and a separate verification result, seeds 1/2/3 are qualified, the three
extensions are terminal in their declared modes, and the aggregated report is
posted on #122. Do not close #154 or #122 yourself.

Pause and ask the owner **only** for:

1. a case that honestly cannot fit the 25 s ceiling after measurement (needs an
   exception ruling or a `NOT_RUN` disposition);
2. a product change that is a design decision rather than a defect fix;
3. a rule contradiction between the frozen specification, #122 and `AGENTS.md`;
4. a blocker in an unowned third-party crate.

Park the blocked item, record it in the ledger and on #154, and continue with the
next case, family or phase. Never idle waiting for an answer, and never fill the
wait by sampling more.

## Per-family state you keep current

Maintain one rolling table in the ledger (and mirror it on #154) with, per case:
`family | case | seed | mode | status | identity | wall | gate verdict | blocker | next action`.
"Terminal" means a receipt exists on the final identity, the verdict is recorded,
and no further action is owed. Report state honestly as `in progress`, `blocked
(owner decision)`, `blocked (fix in flight)` or `terminal`.
