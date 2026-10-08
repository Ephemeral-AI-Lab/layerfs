# Handoff: R7, the benchmark-driven optimization loop

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`. R2 to R6 are
implemented and functionally verified at their declared scopes
([rollout ledger](ROLLOUT-LEDGER-20261008.md)). This handoff assigns one stage:
**build the benchmark, then loop — measure, analyze, fix, optimize, measure
again — until no room for optimization is left.** Do not push, open a pull
request, start another worktree or publish anything remotely.

You start with no other context. Everything you need is in this file and the
documents it names.

## Owner direction, 2026-10-09

The owner dispatched this run with these words:

> write the handoff prompt for the next agent to work iteratively on r7 until
> no room of optimization
>
> 1. it is allowed to optimize aggressively if it is best for speed and does
>    not introduce overhead of memory or extra disk allocation. make sure all
>    roundtrips and bad scalling factor got removed
> 2. it must work with subagents iterate recursively in a way, run the
>    benchmark, analyze, fix the issues, optimize and loop again
> 3. i am going to sleep, do not ask question or pause.

What that means here. The reading of each point is the previous lead's and is
recorded as such.

- **"R7" now names this optimization loop.** The ledger's earlier R7 (retiring
  the excluded predecessor crates) is a separate row, deferred, and is not
  part of this run. Neither is frozen qualification (R8) or reference
  retirement (R9).
- **Do not ask the owner anything and do not pause.** Where a decision is the
  owner's, apply [Authority](#authority) and
  [Deciding without the owner](#deciding-without-the-owner), record it, and
  continue. A failed test, a refuted hypothesis, a busy lock, a long build or
  a context compaction is ordinary work. Stop only at the
  [exit condition](#exit-condition).
- **Optimize aggressively.** You may change first-party product code as far as
  speed requires, including interfaces between active core crates, subject to
  the two gates the owner named (no memory overhead, no extra disk
  allocation) and to the standing rules that this direction does not lift.
- **Remove every round trip and every bad scaling factor.** These are the two
  things the loop must drive to their floor; see
  [What must be removed](#what-must-be-removed).
- **Work with subagents, recursively.** Every iteration fans out analysis,
  implementation and review to subagents and folds the result back; an
  unfinished diagnosis is sent out again, narrower, until it has a cause with
  numbers.

## Checkpoint

- `HEAD` is the commit that adds this handoff, on top of `9a2af7d7c`
  (handbook). **Product source identity: `2b4dc28a6`.** Nothing after it
  changed product source.
- Production LOC at that identity, pinned `tools/production_loc.py`
  (SHA-256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`):
  combined 184,297; core 118,880; active core 76,006; excluded predecessors
  38,878; excluded integration 3,996; root reference 65,417.
- What works on Linux today, functionally: install of a sealed Store, mount,
  service to any external process, Commit of the shared frontier, fresh-mount
  survival, normal unmount, forced unmount, concurrent Workspaces. See the
  [R6 record](R6-COMPLETION-20261009.md) and
  [R5 record](R5-COMPLETION-20261009.md).
- **No timed result of the mounted product exists.** Every test so far ran a
  debug build with the daemon assembled in-process. A changed Commit through
  the real daemon binary is not yet proven (R5-1).
- No harness arm drives the mounted product. `core/benchmark/fs-bench-pro`
  holds families for earlier stages only. No sealed Store of the full fixture
  exists.
- Known counted cost, before any optimization: about 5 owner jobs per FUSE
  request; about 31 owner jobs and 130 statements per captured row in a
  Commit; every READ window takes a Store reader.

## Read first, in this order

1. [Root instructions](../../../../AGENTS.md) and
   [core instructions](../../../AGENTS.md).
2. The [optimization handbook](../../../../docs/general/optimization-handbook.md).
   It is the working procedure for this run: counters, triage ratios, twelve
   diagnostic groups, scaling checks, the candidate list and the record
   templates. Follow it.
3. The [optimization guide](../../../../docs/general/optimization-guide.md):
   binding rules.
4. The [measurement workflow](../../../../docs/general/agent-measurement-policy.md),
   [benchmark rules](../../../../docs/general/benchmark_rules.md), the
   [report template](../../../../benchmark_agent_report.md) and the
   [core harness guide](../../../benchmark/fs-bench-pro/AGENTS.md).
5. The [proof plan](S8-PROOF-PLAN-20261008.md), sections 2.2, 2.3, 6, 7 and
   8.1: arms, workloads and oracles, cache classes, priced boundaries and the
   owner rulings.
6. The R6, R5 and [R4](R4-COMPLETION-20261008.md) records, for the counted
   work and the open limits.
7. This stage's own ledger, once it exists
   ([State that must survive](#state-that-must-survive)), and `git log`.

## Authority

### Allowed now, by the owner's direction

Record each use in the stage's decision list as "taken under the owner's
direction of 2026-10-09".

- Any change inside the active core crates that lowers counted work, including
  interfaces between them: a batched sealed-record read, grouped point
  questions in Content's streamed row source, combined acquire-decide-release
  jobs, a local read path that needs no Store reader, job ordering in the
  owner, admission and parking internals.
- Kernel request reduction that keeps permissions, stable identity, coherence
  and kernel writeback off: negative entries, adaptive READDIRPLUS, FLUSH
  elision, wider directory replies, attribute and entry reuse. Each needs its
  coherence proof before it is kept.
- Control behaviour that only removes waiting or round trips, with the wire
  kept additive.
- Restructuring, moving and deleting first-party active code where that is the
  fastest design. Report relocation and deletion honestly in the LOC line.

### Still forbidden: this direction does not lift these

- Durable execution: `NOT_RUN — disabled by owner until explicit
  reauthorization`. Global Store is Disposable / WAL / synchronous=OFF only;
  the daemon overlay stays MEMORY / OFF / EXCLUSIVE. No `fsync`-family call on
  Disposable backing.
- Kernel writeback caching. Dropping or weakening permission checks.
- Any change to cluster one's canonical format or public contracts. A parent
  pointer or ancestry evidence adds stored bytes and is a proposal only.
- A new dependency; any edit to third-party or vendored code beyond the
  already authorized fuser patches.
- Retry, replay, refresh, busy handler, or a guessed outcome. One attempted
  operation; the first original failure is kept.
- Test hooks, fault injection or test-only API in product source. A path that
  recognises a benchmark command. A second construction producer
  (`LAYERFS_CONSTRUCTION_WORKERS=1`). Sealing per file or per Commit.
- More threads, receive loops, background depth, larger limits, longer cache
  lifetimes or longer timeouts used as the fix.
- Retiring the excluded predecessors or the root reference; editing the
  pinned counter; pushing or publishing; touching what
  [Preservation](#preservation) lists.

### The two gates on every kept change

**No memory overhead.**

- No new resident state whose size follows any input dimension (base size,
  changed rows, operations, bytes, history, Workspaces).
- No enlarged cache allowance, queue capacity, buffer, credit or worker count.
- A bounded transient window inside the existing admitted byte credit is not
  overhead: one job that returns 64 records instead of 64 jobs that return one
  each is allowed.
- Source review is the gate. Beside every timed sample also record the
  daemon's peak resident size (`VmHWM` in `/proc/<pid>/status`) and the
  existing byte counters (`scheduler_bytes`, `peak_credited_bytes`, the cache
  allowance). If an increase cannot be ruled out as the change's own, the
  change is rejected.

**No extra disk allocation.**

- For the same workload cell, the Store file with its sidecars and the overlay
  database must not be larger, in logical bytes or in allocated blocks, after
  the phase. Store growth per Commit must not rise.
- No new persistent index, table, column, sidecar or scratch file. A new index
  is disk allocation: reject it unless it replaces something larger and the
  measured total is not higher.
- Report speed and storage together for every kept change.

A change that passes both gates and lowers counted work is kept even if it is
large. A change that fails either gate is rejected even if it is fast.

## Deciding without the owner

- If the choice is between a faster design and a more conservative one, and
  the faster one passes both gates and every proof: take the faster one.
- If the choice touches a forbidden item: do not build it. Write a proposal
  with the counted benefit and continue with the other candidates.
- If two readings of a contract differ in correctness: take the stricter
  reading, record it, continue.
- If you cannot tell whether something is allowed: treat it as forbidden,
  write the proposal, continue.
- Never invent a numerical performance target. Report measured gaps; the
  owner sets thresholds afterwards.

## What must be removed

A **round trip** is any of: an owner job submitted across threads; a Store
reader acquired; a kernel request that brings no new information; a
transaction opened for one point question. For each ratio below, derive the
structural floor from the contract and write it down **before** optimizing.
The work on a ratio ends when the measured value equals its floor, or the rest
is blocked by a forbidden item named in the record.

| Ratio | Today | Direction of the floor |
| --- | --- | --- |
| Owner jobs per FUSE request | about 5 | one where overlay state must be read or written; none where held state answers |
| Store readers per READ of wholly local bytes | 1 | none |
| Object demands per cold request | about 1, separate per neighbour | one grouped demand per bounded read plan |
| Owner jobs per captured row in a Commit | about 31 | a constant number per bounded window of rows, not per row |
| Statements per captured row | about 130 | follows the jobs |
| Jobs from Mount to Ready, and in unmount | counted in `startup_cost` | independent of root size and of Workspace history |
| FUSE requests per syscall | unknown until measured | no higher than the passthrough arm under the same kernel profile |
| Wait against service per owner class | unknown until measured | no class waits behind unrelated work without a stated reason |

A **bad scaling factor** is any counter whose work per unit rises with size,
or whose total rises with a dimension that should not matter. Run the
handbook's section 6 for every path you measure and every path you change, at
1×, 2× and 4×, over these dimensions and shapes:

- base size, with a fixed change;
- changed rows, files and bytes;
- one huge directory; a deep tree; many hard links;
- a heavily fragmented file; thousands of tiny appends; truncate and regrow;
- many small Commits on one mount (history); many mount and unmount cycles;
- concurrent Workspaces and concurrent processes on one Workspace;
- lookups outstanding at unmount; cleanup debt.

Every superlinear finding is fixed before any constant-factor work, and each
fix leaves a count test that asserts equal or bounded counts at two sizes.

## Stage 0: make it measurable

Harness code lives under `core/benchmark/`, never in product source. Finish
this before the first iteration; do not shrink it to start sooner.

1. **The product arm `L`, through the real path.** A release build of the real
   daemon binary in the pinned image, controlled through the SDK
   (`ProjectApi`, `WorkspaceApi`), with the command launched as an ordinary
   external `bash -c` that nothing registers. First prove the whole lifecycle
   functionally on this arm: install, mount, command, Commit, unmount, fresh
   mount, oracle. Anything missing in the product for that path is a defect to
   fix in this stage, with a reproduction receipt.
2. **The native arm `N`**: the same command on the container's own
   filesystem, same image, same user, same setup rule.
3. **The passthrough arm `P`**: a FUSE passthrough under the same negotiated
   kernel profile as `L`, as harness code with existing dependencies. If it
   cannot be built without a forbidden step, record why and continue with
   `L`, `N` and counts.
4. **Fixtures.**
   - An empty base for C01 to C12.
   - The full fixture: `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness` at
     commit `639ed01539` (103,108 files, 3.237 GiB). **Never run a command in
     that checkout or write to it; work on a copy.** Build one sealed Store
     from the copy by host Project Init, once, as setup under a declared wall
     stop, and record its size, time and identity. Do not assume Docker has
     network access.
   - If the full fixture cannot be prepared, use a declared reduced fixture
     cut from it, say so in every receipt, and keep trying the full one as a
     separate recorded item.
5. **Cache classes** A, B and C exactly as proof plan section 6 defines them,
   with the per-file residency check of the owner's ruling. No VM-wide cache
   drop: it touches other owners' containers and proves nothing.
6. **Counters in every receipt**: per-opcode requests, mount work, owner work
   per class with wait and service, statement work per family, Store demands,
   reader work, Commit construction work, storage diagnostics, Store and
   overlay sizes (logical and allocated), daemon `VmHWM`. The handbook's
   section 3 says where each lives.
7. **Phase boundaries** as proof plan section 7 prices them: setup (not
   product time), mount to Ready, command, Commit when selected, unmount to
   its reply, cleanup to `Gone` (reported, never hidden), verifier (separate).
8. **Fix the stale bound** in
   `core/benchmark/fs-bench-pro/shared/evidence_engine.py` (about line 104):
   it bounds `peak_queued` by the old per-lane ceiling of 18; a lane now
   admits 34. Run the harness's own tests.
9. **Register the matrix** prospectively in the harness registry, labelled
   exploratory and not admission-eligible.

### The matrix

| Set | Cells | Classes |
| --- | --- | --- |
| Fixed cost | E01 `true`; repeated mount, command, unmount with a fresh identity each | A, B |
| Read and metadata | E02, E04, E05, E06, E07, E18 (labelled unrefreshed index), E19 | A, B, C |
| Write and churn | E10, E11, E15, E16, E17; C01 to C12 on the empty base | B, C |
| Priced diagnostics, no pass or fail, declared wall stop | E03, E12, E13, E14 | A, C |
| Commit | a small change; a large change after E12 or E13; five incremental Commits on one mount; Commit then fresh mount | — |
| Concurrency | two Workspaces, one scanning and one writing; several commands on one Workspace | C |
| Not run | E08, E09 | — |

Command bodies and oracles are in proof plan section 2.3. Timing rows use a
scoped oracle, labelled scoped.

After the baseline, choose a **loop set**: the cells with the largest gap
weighted by how often a tool call pays them, with at least one cell per phase
(mount and unmount, read and metadata, write, cold read, small Commit, large
Commit). Run the loop set every iteration and the full matrix at the baseline,
whenever the loop set stops improving, and at the end.

## The loop

One iteration is one source identity. Repeat until the exit condition.

1. **Measure** (lead only). Run the loop set once at the current identity. One
   sample per cell and arm. `N` and `P` are sampled once per identity of
   theirs and reused by exact identity; they do not change when the product
   does.
2. **Triage** (lead). Split each cell by phase; form the handbook's ratios;
   rank cells by gap to `P` and `N` weighted by frequency; map each to a
   diagnostic group.
3. **Analyze** (subagents, read-only, in parallel; one per group with a gap).
   Each returns cause sentences with numbers and candidate entries in the
   handbook's templates, plus the scaling sweep for its path. An analysis that
   comes back without a numbered cause is sent out again on the narrower
   question it exposed. Repeat until the cause is a sentence with counts, or
   the instrument that would show it is named as unavailable.
4. **Plan** (lead). Merge the candidates into the stage's candidate list.
   Scaling defects first, then round trips, then requests, then waits, then
   constants, then fixed cost. Reject at once anything that fails a gate or is
   forbidden, and write its proposal. Cut the rest into tracks with disjoint
   file lists. Commit the plan before product edits.
5. **Implement** (subagents, one per track). Each track delivers the change, a
   count test that shows the counter dropped, the scaling test at two sizes,
   the affected proofs passing on host and Linux, and the affected
   architecture document updated. A track may run its own inner loop of
   change, count, change — on counts, never on timings.
6. **Integrate** (lead). Apply the two gates. Run Clippy `-D warnings` on host
   and Linux, `fmt --check`, the boundary guard. One commit per candidate with
   its counts and production LOC line.
7. **Review** (fresh-context subagents, read-only) for every kept change that
   touches coherence, custody, failure scope, admission or teardown. Verify
   each finding against source; fix what is real as a new identity; record
   what is rejected and why.
8. **Confirm** (lead). One timed sample of the affected cells at the new
   identity, speed and storage together. A change whose counts dropped but
   whose time or storage got worse is diagnosed, not kept on faith: revert it
   with a new commit (never a reset) unless the cause is found and fixed.
9. **Record** the iteration's row in the stage ledger and go to step 1.

Run the full suites — every active package, host and Linux, one run per test
binary — at least every third iteration that kept a product change, and at the
end. A functional failure outranks all optimization work: reproduce it, fix
it, keep the failing receipt.

## Exit condition

The loop is finished only when **all** of these hold, each shown by a receipt:

1. Every ratio in [What must be removed](#what-must-be-removed) is at its
   written floor, or its remaining gap is blocked by a forbidden item named in
   the record with a proposal.
2. The scaling sweep shows no superlinear counter in any listed dimension or
   shape, on every measured and every changed path, and each has its count
   test.
3. A full-matrix run at the final identity produces no candidate that is
   allowed, passes both gates and has a non-zero expected count reduction.
   Every remaining candidate is recorded as rejected, blocked or proposed.
4. Two consecutive iterations kept nothing.
5. The full suites pass at the final identity, apart from cases whose
   precondition the suite does not supply, listed by name.

Then write the completion record and stop. Do not start predecessor
retirement, frozen qualification or reference retirement.

If the loop cannot make progress for a reason outside these rules — the
fixture cannot be built at all, the image cannot run FUSE — record the exact
blocker with its receipt, finish everything that does not depend on it, and
close the stage as `INCOMPLETE — <blocker>`. That is a truthful closed state.

## Measurement rules for this stage

- Every sample is exploratory and not admission-eligible. Say so in each
  receipt. Frozen qualification is a later stage.
- One sample per case, arm and identity. No best-of. No rerun of an unchanged
  treatment. A new source identity is a new treatment and is sampled once.
- Release build, repository ARM flags, pinned image, pinned workload, declared
  and enforced cache class. Record every identity.
- A complete performance command is at most 15 s by default, 25 s for a
  declared exception. The priced diagnostics carry their own declared wall
  stop. An independent verifier is under 10 s. Do not change a workload, a
  cache state or a limit to turn a miss into a pass.
- Every test command has a wall limit of at most 120 s. Build `--no-run`
  first. Never loop, background or output-filter a test. A test that reaches
  its limit has failed as a hang.
- One Cargo, Docker, test or measurement command at a time, under the
  checkout lock. A measurement never overlaps a build.
- Fresh, append-only outputs. Failed, ineligible and unrun selections stay in
  the ledger and are never relabelled.
- A database claim needs the plan and a runtime profile from the engine that
  ran the statement. The overlay engine records VM steps, full-scan steps,
  sorts, automatic-index rows and re-prepares per statement family.
- Store, overlay and scratch on the container's own filesystem or the host
  temporary directory, never under the repository bind mount.

## Working with subagents

- One checkout, no worktrees. Each implementing subagent gets an explicit file
  list and edits nothing else; a needed change elsewhere is reported.
- **Only the lead** runs `git add`, `commit`, `stash`, `checkout` or `reset`,
  counts LOC, edits the ledger or a record, removes anything, takes a timed
  sample, or decides to keep or reject.
- Give every subagent the binding rules in full; it starts with no context.
  The block below is the minimum. Add the stage's checks directory, the
  track's file list and the exact question.

```text
Repository /Users/yifanxu/Ephemeral-AI-Lab/layerfs, branch main, ONE checkout,
no worktrees. Touch only the files in your list; report any other needed
change. Never run git add/commit/stash/checkout/reset. No push or publish.
One Cargo/Docker/test command at a time through the lock helpers in
core/target (r4-build.sh, rx-run.sh, rx-linux.sh); a busy lock (exit 75) is
not an attempt. Host cargo is +1.85.1 --locked; Linux is image
sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6.
Every test command: wall limit at most 120 s, build --no-run first, never
loop/background/filter, a test at its limit FAILED as a hang. Receipts are
append-only; every attempt is reported; nothing is relabelled.
Store profile Disposable/WAL/OFF only; Durable is never executed; overlay
MEMORY/OFF/EXCLUSIVE; no fsync-family call. LAYERFS_CONSTRUCTION_WORKERS=1.
One attempted operation: no retry or replay; keep the first original failure.
Product source only under core/crates/<package>/src: no test hooks; files at
most 999 lines; lib.rs/mod.rs at most 200 lines of declarations. No new
dependency; no third-party edit.
Optimization gates: no new input-sized or enlarged resident state; no larger
Store or overlay, logical or allocated; counts must drop; proofs must pass.
You take no timed measurement; you work on counts.
Nothing writes under /sys/fs/fuse/connections except the product's own abort
for its own mount; tests read only their own entry.
Do not touch the protected containers and paths (list from the handoff).
If you meet an ambiguity, a conflict with source or a result you cannot
explain: stop and report to the lead. Do not ask the owner.
```

- A subagent's report is a claim. Verify it against source and receipts before
  acting on it or repeating it.

## State that must survive

Context will be compacted during this run. Keep the state on disk.

- Stage directory: `core/docs/issues/307/checks/r7-optimization-20261009/`.
- `LEDGER.md` there, updated after **every** commit: one row per iteration
  with the identity, the cells run, each ratio, each candidate with its
  outcome, the gates, and the next step.
- `CANDIDATES.md` there: the live candidate list in the handbook's template,
  including rejected and proposed entries. Start it from the handbook's
  section 9.
- The [rollout ledger](ROLLOUT-LEDGER-20261008.md) row for this stage.
- On resuming: read this handoff, the stage ledger, the candidate list and
  `git log` before anything else.

## Environment

- Host: macOS ARM64, `cargo +1.85.1 ... --locked`. The machine default
  toolchain is not the pinned one. macOS has no `timeout`; use a bounded
  runner.
- Linux: image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
  repository bound at `/work`,
  `CARGO_HOME=/work/core/target/cluster2-linux-cargo`,
  `CARGO_TARGET_DIR=/work/core/target/cluster2-linux`. Mount runs need
  `--device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined`.
  Use a separate target directory for release builds.
- Untracked helpers under `core/target/`, kept from R4 to R6: `r4-locked.pl`
  (the checkout lock, wall limit, group kill); `r4-build.sh` (`test-no-run`,
  `clippy`, `check`, `fmt`, `fmt-check`, `guard`; `fmt` and `fmt-check` need a
  package argument); `rx-run.sh` and `rx-linux.sh` (one bounded run with an
  append-only receipt; `RX_STAGE` is the checks directory **name**, and the
  binary argument is the hashed file name under `deps`); `rx-suite.py`
  (`host` or `linux`, build once, one run per binary, `R4_SLICE=lo:hi`);
  `rx-count.py` (`staged` or `committed`, run from the repository root). If
  any is missing, write an equivalent that keeps the one lock and the wall
  limit. The pinned counter is the authority for LOC.
- The fusectl filesystem in a test container lists other containers'
  connections. Read only your own mount's entry. Never write there.

## Preservation

Do not touch these unrelated running containers:

- `9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`
- `ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`
- `d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`
- `d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`

Leave the exited `layerfs-e04-*` and `cf-build-probe` containers and the
long-lived `layerfs-experiment-305-dev` container alone. Remove only
containers and volumes you create.

Preserve these untracked files and do not stage them:
`HANDOFF-PRE-S8-SERVERLESS-20261007.md`, `HANDOFF-S7-S9-RESUME-20261006.md` and
`S7-S9-SPEED-TEST-PLAN.md` in this directory, and
`checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
Do not rewrite raw evidence. Do not touch
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.
Do not run commands in or write to
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness`. The experiment worktree
(`codex/phase7-experiment-305`) may be read with `git show` only.

## Every commit

Local `main` only. Each commit message records
`Production LOC: <before> -> <after> (delta <signed>)` with scope, method and
subtotals from the pinned counter, compared between the first parent and the
staged tree and confirmed on the committed tree. Harness, test and document
commits report the unchanged total and delta 0. End each message with the
attribution line the session gives you.

## Done means

The stage is closed when the exit condition holds, or the truthful
`INCOMPLETE` state above is recorded. Then write
`core/docs/issues/307/R7-OPTIMIZATION-COMPLETION-<date>.md` in the shape of
the R6 record and stop. It states:

- the baseline and the final measurement for every cell and arm, with every
  identity, speed and storage together, and every failed, ineligible and unrun
  selection;
- each ratio: start, floor, end, and what blocks any remaining gap;
- the scaling sweep: every dimension and shape, its growth factor before and
  after, and the count test that guards it;
- every candidate: kept, rejected, or proposed, with its counts, gates and
  receipts;
- every functional defect found and fixed, with its reproduction;
- every failed attempt; every correction of the lead's own statements; what is
  not verified;
- the decisions taken under the owner's direction, for review;
- how subagents were used and every review finding with its action;
- production LOC per commit, and what is not claimed.

No measured number in that record is a qualification result. The owner sets
the targets from it.
