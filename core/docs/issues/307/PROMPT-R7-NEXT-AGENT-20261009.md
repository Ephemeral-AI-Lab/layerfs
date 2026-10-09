# Prompt for the next R7 lead: beat the baseline, generically

> **Status:** Current owner assignment, 2026-10-09. It supersedes the
> "Method" bullet and the delegation caveat of the
> [second handoff](HANDOFF-R7-BEAT-A2-20261009-2.md); everything else in that
> handoff and in the root and core `AGENTS.md` stands.

You are taking over the R7 optimization of the mounted LayerFS product
(cluster two: FUSE, daemon, Workspace, overlay) in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, on local `main`. The previous lead
stopped by owner direction after fixing the verifier failures. No cell is
established as beating the baseline yet.

## The owner's direction, verbatim

> 1. work with subagents
> 2. carefully decide biggest opportunities in high priorities and work them
>    in batch
> 3. make sure the solution is generic, does not cheat
> 4. work iteratively, and do not stop nor ask question until beating
>    baseline and no big room for improvements.
> 5. must not trade allocation storage or increase too much memory for
>    speeding up. using memory is allowed, but must be bounded and justified
>    for multi workspace and multi exec workload

Earlier words of the owner on point 3: "i encourage to be aggressive and
creative but did not mean to be cheating? in the past some agents used warm
cache, hard coded specialized datastructure, metadata or throughput
limitation but suit well to pass the benchmark. we must avoid those". And on
the method: "we need simplicity".

The sections below are the previous lead's reading of those five points. If
the owner's words and a reading differ, the words govern.

## Read first, in this order

1. Root [AGENTS.md](../../../../AGENTS.md) and [core/AGENTS.md](../../../AGENTS.md).
2. [Second handoff](HANDOFF-R7-BEAT-A2-20261009-2.md): where every cell
   stands, the A2 targets, what steps 4 to 10 changed, the ranked open
   candidates, the audit against benchmark-fitting, mechanics, known
   unrelated test failures.
3. [Benchmark instruction](../../../../docs/general/benchmark_instruction.md):
   the iteration rule, the metrics every iteration reports, the three test
   tiers, the round-trip ladder, the ranked batch, root-cause analysis.
4. [First handoff](HANDOFF-R7-BEAT-A2-20261009.md): "The target", "Still
   forbidden", "Gates on every kept change", "Exit condition",
   "Preservation", "Every commit".
5. The campaign [ledger](checks/r7-optimization-20261009/LEDGER.md) from
   "Third lead" onward, [CANDIDATES.md](checks/r7-optimization-20261009/CANDIDATES.md)
   and the research in [r7-open/](r7-open/).

## 1. Work with subagents

- You are the lead: you rank, decide, review and record. Subagents do the
  source models, the implementation and the suite runs.
- **Research agents are read-only and run in parallel**, one per request
  family or per ladder level, reading with `git show <commit>:<path>`. Each
  must reproduce the latest receipt's counts exactly from source, with file
  and line, and state the residual. A model that does not reproduce the
  counts is not used.
- **One writer per crate at a time.** No second worktree. Give each
  implementer a brief: the decided design, the rows of the batch it owns,
  the predicted counter for each, the proof required, the helper commands,
  and stages that each leave the tree green.
- **A subagent's report is a claim.** Read its diff, run the count tier
  yourself, and check every number it quotes against the receipt before the
  ledger says it.
- One Cargo, Docker, test or measurement command at a time across all
  agents; every test command at most 120 s.

## 2. Biggest opportunities first, in batches

Follow the benchmark instruction's
[opportunity list and batch](../../../../docs/general/benchmark_instruction.md#finding-the-cost-scaling-round-trips-and-one-ranked-batch):
one pass over every sampled cell, every excess listed, ranked by saved time
as a share of each cell's gap, then one batch with one commit and one
predicted counter per row, the gate and the samples once at the tip.

Start from this order; re-rank it from your own first sample of all cells:

1. **Sample every C cell at the current HEAD** (C02, C03 and C09 were not
   sampled after step 10 and the link-count fix).
2. **The five NOT_RUN cells (C06, C07, C08, C10, C11).** They have no
   number at all. The cause is local reclamation still writing the overlay
   file while the cache predicate is attested (handoff candidate 3). Fix it
   in the product or wait for the product's own readiness signal in the
   harness; do not weaken the predicate.
3. **Requests and per-request cost.** On C01 the time outside the owner
   (243 µs a file) already exceeds the target (183 µs), so database work
   alone cannot reach it. Measure the floor (passthrough arm, per-thread CPU
   time), then remove requests and per-request work.
4. **READ, OPEN, RELEASE and directory requests as single owner visits**
   (H01): a READ of base bytes was 6 jobs, 87 executions, 4 transactions
   and 2 reader grants at `fdc24ef3f`
   ([trace summary](r7-open/read-directory-path-summary-fdc24ef3f.md)).
5. **Payload layout and reclamation** (G01), **the rest of the statement
   diet** (E04b), **group commit** (E05), then the smaller levers.
6. **E cells**, which have no closed oracle or configuration yet.

"Baseline" is read here as the A2 column of issue #306 for every cell that
has one, with storage not worse, as in the first handoff's exit condition.

## 3. Generic, no cheating

A change is kept only if all of these hold, and the ledger says so per
batch:

- **No path recognises a workload**: no branch on a command, path, name,
  file count, size or pattern that a benchmark cell happens to have.
- **No warm-cache credit.** The measured phase pays for its own work from
  the declared cache state. Nothing is pre-touched, pre-built or carried
  over from the warm-up beyond what the cache class declares. Every sample
  so far is class B (warm daemon); say so wherever a number is quoted, and
  do not present it as cold.
- **No structure or constant fitted to a cell.** A capacity, window or
  threshold is justified from the product's limits, not from a cell's
  shape. State what happens just past it: the cost must degrade to the
  indexed path, not fail or fall off a cliff.
- **No limit on what the product accepts**: file count, file size,
  directory depth and width, edit count and edit size stay bounded by
  resources only. No dropped metadata, permission check, coherence
  guarantee or reclamation work.
- **It holds on a shape the cell does not have.** For each kept change, a
  count test at two sizes and beside unrelated rows; and, for anything in
  dispatch or the owner, concurrent callers and more than one mounted
  Workspace. The one-receive-loop change was validated on serial callers
  only: that check is owed before building on it.
- **Simplicity.** Prefer removing a step over adding a cache for it. Report
  Production LOC per commit; growth needs a reason.
- Oracle, workload, cache state, limits and timeouts are never edited to
  turn a miss into a pass. A failed or unrun row keeps its receipt.

Repeat the handoff's audit (product-source search for benchmark strings,
table of constants introduced or changed) at the end of every batch.

## 4. Iterate without stopping or asking

- The owner's decisions are delegated to you for this run. Decide, record
  each decision with its reason in the ledger's delegated-decisions table,
  and continue. Do not end a turn to ask, report or wait.
- This does not lift a hard rule. Still forbidden: push, pull request,
  publish, another worktree; Durable execution; kernel writeback cache;
  changed permission checks; a changed cluster-one canonical format or
  public contract; a new dependency or third-party edit beyond the
  authorized fuser patches; retry or replay; a test hook in product source;
  more threads, loops, spinning, larger limits or longer lifetimes as a fix;
  CPU pinning; touching the listed unrelated containers, the listed
  untracked files, `deepseek-harness`, or the retained sample containers and
  volumes of failed rows. If a change needs one of these, set it aside,
  record why, and take the next row.
- Ordinary obstacles (a failing test, a stale binary, a NOT_RUN row, a lock)
  are diagnosed and worked through, not reported as blockers.
- **Stop when both hold:** every cell with an A2 target is below it with
  storage not worse, verifier PASS, and the gate green on host and Linux;
  and the ranked list has no row left whose predicted saving exceeds the
  one-sample spread (about 10 %) of any cell's command time without one of
  the forbidden items above. Then write
  `R7-OPTIMIZATION-COMPLETION-<date>.md` with the final table, what remains
  and why it was not taken.
- If a cell cannot reach its target inside the rules, say so with the floor
  measurement that shows it, keep working the others, and list it in the
  completion document. Do not relabel it.

## 5. Storage and memory are not the currency

- **Allocated storage does not rise to buy speed.** For every sampled cell,
  Store and overlay bytes, logical and allocated, are not higher after the
  batch than before it. No larger preallocation or reservation, no extra
  index, denormalized copy or retained layer whose purpose is speed. The
  overlay's existing 256 MiB reservation is not to grow.
- **Memory may be used, bounded and justified.** For each new or enlarged
  resident structure, record in the ledger:
  - its capacity in entries and in bytes, fixed by source;
  - what the bound is per: daemon, mounted Workspace, open handle, or
    in-flight request;
  - the total for W mounted Workspaces and E concurrent Execs, and when it
    is released (last owner, unmount, eviction);
  - that it does not grow with files, bytes, directories or mutations;
  - `daemon_vmhwm_bytes` before and after, per cell.
  Prefer one fixed-capacity structure shared per daemon over one per
  Workspace or per Exec. Between two designs with the same counts, take the
  one that holds less.
- The owner gave no number for "too much". Do not invent one; state the
  bound and the W × E total so the owner can judge, and reject a change
  whose time gain is within the spread while its memory or storage cost is
  not.
- Per-tool-call Workspaces are the common case: a structure that only pays
  off on a long-lived warm mount, or that makes a fresh mount slower, is not
  a win.

## Mechanics you will need at once

- Host cargo is `+1.85.1 --locked` through the helpers in the git-ignored
  `core/target` (`r4-build.sh`, `rx-run.sh`, `rx-linux.sh`, `rx-suite.py`,
  `r7-iterate-all.sh`, `r7-summary.py`, `r7-statements.py`, `rx-count.py`);
  the handoff's "Mechanics" section lists them, the pinned image and the
  volumes.
- Receipts are append-only in `checks/r7-optimization-20261009/`; the next
  free sample number is 600. Take the next free `r7-loc-<n>-staged.json`
  from that directory.
- A sample needs a clean HEAD and no idle daemon left in a retained
  container (stop, never remove).
- Every commit carries `Production LOC: <before> -> <after> (delta <signed>)`
  with scope and method.
