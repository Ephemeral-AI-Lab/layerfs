# Handoff prompt — finish #154: implement the remaining ten cases, one run each, fix at root cause

You are the implementation agent for
[#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) on `main`. The port
is done, the six families are measured, and the verifier's redundancy has been
removed. **Ten of the 33 regular cases are still not implemented.** Your job is to
build them, fix whatever breaks at its root cause, run each case exactly once, and
report. #122 is the requirement of record and receives the final matrix.

Read first: this prompt, `docs/roadmap/0.1/0.1.6/evidence/issue154-rollout-ledger.md`
(L1–L10, especially **L10**), `docs/roadmap/0.1/0.1.6/issue154-remaining-work-handoff.md`,
`AGENTS.md`, `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md`,
`benchmark/fs-bench-pro/QUICKSTART.md`, and the frozen specification
`docs/roadmap/0.1/0.1.6/{cases.json,benchmark-families.md,fixtures.md,workloads.md,execution-and-verification.md,review-decisions.md}`.
`cases.json` is the membership authority.

## 1. Where the campaign stands (do not re-derive this)

Final seed-1 matrix, one run per case per mode, identity `ed9cacebd` (clean),
product seal `970964e9…` unchanged, image `layerfs-bench-infra:221f445fabac7e39`:
**40 PASS inside the 15 s target, 6 declared ≤25 s exceptions, 4 FAIL, 16 not run**
(evidence: `evidence/issue154/final-seed1-matrix.json`).

| group | cases | state |
| --- | ---: | --- |
| measured, passing | 23 | terminal |
| `v016-history-namespace-inode-k{10,100}-v1` | 2 | **FAIL** — workload exits `unsupported dedup native workload`; the five-stage HN schedule has no host orchestrator |
| `v016-branch-convergent-content-v1`, `v016-branch-fork-descendant-v1` | 2 | **not registered** — compact schedule and oracle never written |
| six `v016-access-*` | 6 | **not registered** — the route that mounts one selected retained state of a sealed producer never written |

Three extensions still carry the **older** `8ef48ec2…` seal and must be re-run once
on your final identity: `v016-mixed-exhaustive-100mb-5000-k100-v1` (was PASS
38.16 s), `v016-workspace-four-100mb-5000-k100-v1` (was PASS 32.62 s), and
`v016-mixed-exhaustive-500mb-30000-k100-v1` (was TIMEOUT 298.13 s against its own
300 s watchdog — the verifier fixes should cut this materially).

## 2. Non-negotiable rules for this phase

1. **One run per case, per mode, seed 1 only.** No `n2`/`n3`, no seeds 2/3, no
   best-of, no re-running because a number looked slow. If a fix changes source,
   seals or the harness, re-collect **only the affected rows** on the new identity
   — re-collection after a verified change is required; re-sampling for a nicer
   number is forbidden.
2. **Budgets.** A v0.1.6 **performance** invocation carries the owner-granted
   **60 s** complete-command allowance. The **verification gate is unchanged**:
   15 s target, **≤25 s declared exception** (list such rows by case), 3 s cleanup
   reserve inside the ceiling, else `FAIL`/`NOT_RUN` with the measured wall.
3. **Verification stays simple and fast.** Making a verification fit is a job for
   removing redundant work, never for widening a limit, trimming coverage,
   sampling fewer paths, or shrinking the fixture. The three fixes already made are
   the pattern to follow (L10): a namespace walk rebuilt per verified range; a
   recursive validator re-walking shared `Arc` subtrees; a file-state re-read per
   referencing path. Profile first (`sample <pid>` on the **child**
   `fs-benchmark-pro workspace-run` process — the host wrapper only `wait4`s), then
   remove the duplication. Every check must still run on every distinct object.
4. **Never loosen the thing you are testing.** Oracles, assertions, expected
   values, limits and timeouts come from the frozen specification. If a check
   fails, decide with evidence whether the product or the check is wrong and fix
   *that*. A check edited to pass is a failed rollout.
5. **Cache state is declared and equal.** `--setup clone` for every
   post-initialization case; run `--prepare` first so first-use fixture
   construction is its own selected step and is never charged inside a gated
   invocation (doing it inside cost 13–16 s per L500 case and produced two bogus
   budget failures — ledger L7). Fresh append-only `--output` per run; failures,
   `INELIGIBLE` rows and superseded attempts stay on disk.
6. **One construction worker**; no run raises `LAYERFS_CONSTRUCTION_WORKERS`, and
   no helper lane is added to pass a gate. `init_namespace` remains the only
   exception.
7. **Identity discipline.** Build once per identity (`--build-host`,
   `--build-image`), record source/product/compilation/dependency seals, image ID,
   harness identity and workload-source hash. Keep the tree **clean** for sealed
   builds (`LAYERFS_SOURCE_DIRTY=false`). Never relabel an old receipt.
8. **No CI claims:** run `tools/preflight.sh` before every push; this repository
   has no CI. No third-party patching, vendoring or forking; builds stay `--locked`.
9. **Do not overload the host.** One benchmark invocation at a time, no sweeps
   "just in case". Record the host load with each run (the driver does) and wait
   for a quiet machine rather than measuring under contention — eight seed-3 rows
   were invalidated that way (ledger L7). Remove only this benchmark's own orphaned
   sample containers before a phase.
10. **Report honestly.** Unrun rows, waived targets, dirty seals and incomparable
    baselines are stated as plainly as passes.

## 3. What to implement

### 3.1 F5 — the HN (`namespace-inode`) history schedule — 2 cases

* Frozen contract: `benchmark-families.md` §`dedup_branch_history`. Each cycle is
  five stages: (1) unlink/recreate two tiny files, one recurring and one
  generation-specific; (2) two single-file SDK 256 B overwrites on two further tiny
  files; (3) rename the populated `tiny` directory to its alternate name;
  (4) create one alias to a fifth tiny file, chmod two files plus that directory,
  set both files' explicit mtimes; (5) one 4 KiB atomic save over the aliased
  destination, observe the old inode through the alias, then remove the alias.
  Four POSIX helper executions and two SDK calls per cycle; helpers finish before
  each Commit; every stage must produce a real `Created` Commit. Envelope: S+2
  names, S+8192 logical bytes, directories unchanged.
* Existing pieces: fixture S and `SProfile::NamespaceInode` already exist
  (`workload/v016_compact.rs`); the case IDs are registered; the host has no
  orchestrator for the interleaved POSIX/SDK stages, which is why `v016_edits`
  returns *"the five declared HN stages have no host orchestrator"* and the
  workload exits with `unsupported dedup native workload`.
* Work: the orchestrator (model on `src/v016_mixed.rs`'s session/exec/SDK/Commit
  receipt structure, but single-branch on fixture S), the per-state oracle, and
  the route wiring (`dedup_workloads::is_sdk` / the native dispatch).
* Consumers: `v016-access-inode-before-v1` (Commit 94, stage-4 cycle 19) and
  `v016-access-inode-after-v1` (Commit 95, target replaced, alias ENOENT) of
  `v016-history-namespace-inode-k100-v1`.

### 3.2 F4 — the two compact branch controls — 2 cases

* Frozen contract: `benchmark-families.md` §"Two compact branch controls".
  `v016-branch-convergent-content-v1`: fixture S, trunk10, A10 and B10 forked from
  trunk commit 5, 30 new commits, longest ancestry 15, 31 retained roots.
  `v016-branch-fork-descendant-v1`: the same plus C10 forked from **A's local
  commit 5**, 40 new commits, ancestry 20, 41 roots.
* Per-commit edit: one 256 B SDK overwrite on the first medium file, offset
  `4096*((j-1) mod 2)`, payload alternating A/B by `floor((j-1)/2) mod 2`, with
  those two regions initialised to a distinct Z. `j` is the ancestral ordinal:
  trunk 1..10, children 6..15, descendant 11..20. Convergent children make
  **identical** content changes and must have **equal file-content IDs**; the
  descendant salts B/C changes by branch and preserves all sibling heads.
* Existing pieces: `SProfile::BranchControl` already declares exactly those two Z
  regions and `dedup_workloads::BRANCH_Z` exists.
* Work: a compact topology in `workload/v016_stages.rs` (30/40 total commits,
  31/41 roots, 15/20 ancestry), registration in `branch_development::cases()`
  (4 → 6, and its self-check), the compact round in `src/v016_mixed.rs`, and the
  oracle. Add a self-check that convergent children's content function is identical
  and the descendant's B/C content is branch-salted.
* Consumers: `v016-access-fork-point-v1` (trunk commit 5, 31 roots) and
  `v016-access-divergent-head-v1` (B local commit 10, 41 roots).

### 3.3 F6 — the `historical_access` route and its six cases (verify-only)

* Frozen contract: `benchmark-families.md` §"historical_access: six additions" and
  `execution-and-verification.md`. Each invocation mounts **one selected retained
  state of a sealed producer** with one live workspace, creates no commits, and
  reports the reader profile (fresh-session/application-cold, uncontrolled OS
  cache, no pre-read). Producer identity, graph size and selected ordinal are
  explicit inputs, never an automatic history-building setup step. Performance is
  **`N/A`**, never 0 and never `PASS`.
* States: `boundary-before` Commit 48 / full 131 071 B read of the below-role file;
  `boundary-after` Commit 49 / full 131 072 B read of the same role;
  `inode-before` Commit 94 / full 4 KiB reads of target and alias with stat/nlink
  equivalence; `inode-after` Commit 95 / target full 4 KiB, alias `ENOENT`, target
  identity and content at replacement; `fork-point` trunk commit 5 / full 64 KiB
  read of the first medium file plus branch metadata; `divergent-head` B local
  commit 10 / full 64 KiB medium read compared against expected B content without
  altering A or C.
* Start with the `boundary-before/after` pair: its producer
  (`v016-history-boundary-cycle-k100-v1`) already runs green today, so the route
  can be built and proved before 3.1/3.2 land. `families/historical_access/fixture.json`
  is still the inherited 11-case artifact; `infra-list historical_access` returns
  nothing.
* The producer's commits are not attributed to read latency. The access verifier
  repeats the declared reads independently and compares them with an original
  derivation, not with a value the product reported.

## 4. Tooling you will use

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --build-host      # seals host identity
IMG=$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)
python3 benchmark/fs-bench-pro/shared/v016_rollout.py \
  --image "$IMG" --tag <phase> --seed 1 [--family FAMILY] [--case ID] [--prepare] [--extended]
python3 benchmark/fs-bench-pro/shared/v016_report.py --tag <phase> \
  --out docs/roadmap/0.1/0.1.6/evidence/issue154/<name>.json
```

* The driver runs perf and verify as separate invocations, measures the complete
  command wall itself, records the host load, classifies the verdict
  (`PASS` ≤15 s, `EXCEPTION` ≤25 s, `GRANTED` ≤60 s for **performance** only,
  `FAIL` otherwise), retains superseded attempts and re-runs only
  infrastructure-invalid invocations (no sample record produced).
* Product-free checks before any measurement: `python3
  docs/roadmap/0.1/0.1.6/check_plan.py`,
  `rustc --edition 2021 --test workload/main.rs`,
  `target/release/fs-benchmark-pro workspace-self-check`, `tools/preflight.sh`.
* Registry bookkeeping: `workspace_registry::self_check` declares
  `[8,20,12,4,4,16,10,10,20,14,26,7,5,4,1]` families and 161 timed IDs. Registering
  the compact controls takes `branch_development` 4 → **6**; update the totals,
  `steps()`, and `sample_slot_count` consistently.

## 5. Environment notes (learned the hard way)

* **Docker tag resolution flaps.** `docker image inspect <repo>:<tag>` can return
  `No such image` for a tag `docker images` still lists; `docker tag <id> <tag>`
  clears it. An invocation that never reached the product is
  infrastructure-invalid: keep the attempt, re-run the pair once.
* **Another writer may be active in the checkout.** The source seal covers only
  `crates/`, `tools/` and `benchmark/fs-bench-pro/`, but `LAYERFS_SOURCE_DIRTY` is
  the whole-tree `git status`, so their uncommitted docs make sealed builds
  "dirty". If that happens, measure from a pinned worktree
  (`git worktree add … <commit>`) with `benchmark-results` and `target` symlinked
  to the main checkout, and add `benchmark-results` to `.git/info/exclude` so the
  worktree reads clean. Remove your worktree when done.
* **`/Users/yifanxu/layerfs-v016-control` is not yours.** It is a detached control
  worktree holding ~1.1 GB of `issue152-control*` measurement evidence and 10
  uncommitted files. Leave it alone.
* Do not run multi-seed sweeps. One run per case per mode is the contract and the
  host is shared with a desktop workload.

## 6. Loop and reporting

For each piece: implement → product-free checks → build once and record the
identity → **one** perf run and **one** separate verify run per case → triage every
mismatch to a root cause and fix it → re-run only the affected rows → append the
ledger (`evidence/issue154-rollout-ledger.md`, next free L-number) → **post the
family report on #154** (identity chain; per-case rows with requested vs observed
paths/bytes, Created count and `UpToDate`/`Busy`/`HeadMoved`/presentation
failures/errors separately; timing; verification coverage and omissions with
full-payload vs affected-set distinguished; resources; every non-passing row with
its measured reason; defects found and fixed; exact reproduction command).

Escalate to the owner only for: a case that honestly cannot fit the 25 s verify
ceiling after the redundancy work (needs a ruling or `NOT_RUN`); a product change
that is a design decision rather than a defect fix; a rule contradiction between
the frozen specification, #122 and `AGENTS.md`; or a blocker in an unowned
third-party crate. Park, record and continue elsewhere.

## 7. Definition of done

* All **33** regular cases terminal at seed 1 with **one performance result and one
  separate verification result** each (perf `N/A` for the six verify-only access
  cases).
* The three extensions re-collected once on the final identity.
* `docs/roadmap/0.1/0.1.6/evidence/issue154/final-*-matrix.json` committed with the
  identity chain and the raw commands; ledger appended; `tools/preflight.sh` green
  and `main` pushed.
* #122 receives the aggregated matrix with every non-passing row stated plainly.
* Do not close #154 or #122 yourself.
