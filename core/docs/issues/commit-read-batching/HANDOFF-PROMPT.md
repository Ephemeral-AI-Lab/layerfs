# Handoff prompt: Commit read batching

Give the text below to the next agent as its first message. It matches the "Handoff prompt" section of the doc https://claude.ai/artifact/HV5FkH17y4w1SmuGk5WyYB.

```text
You are the lead agent for LayerFS issue #810, https://github.com/Ephemeral-AI-Lab/layerfs/issues/810 ("PostgreSQL Commit: batch per-file and per-directory reads before the lane"). The goal is to cut PostgreSQL Commit round trips by batching per-file and per-directory Store reads before the publication lane, within the conservative scope.

Work with subagents in parallel, and iterate until the spec's terminal pass condition holds on one final commit. Do not ask the owner questions: decide within the spec and record each decision. Escalate only three things: a pass-condition miss you could not fix, a stored-bytes increase over 5%, or anything that would need a Store format change.

READ FIRST
- Spec: /Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/issues/commit-read-batching/SPEC.md. This is the local copy: it is untracked in the main checkout, which is on old main. Add it to claude/commit-batching at the same path in your first commit, and keep it as the working spec from then on.
  The same content is in the doc https://claude.ai/artifact/HV5FkH17y4w1SmuGk5WyYB; if the two differ, the local file wins.
  Its "Terminal pass condition" section is the contract, and "Where the round trips come from" and "The fixes" give file:line entry points.
- AGENTS.md, engine/AGENTS.md, core/AGENTS.md, docs/architecture/engine/store/postgresql.md, and the disk and worktree rules in ~/.claude/CLAUDE.md.
- Receipts from 2026-11-25 at ca4be0bf3c:
  - ~/.cache/layerfs-measure/2026-11-25-commit-shapes/ (REPORT.md, cells.json, harness/zzshape_commit.rs)
  - ~/.cache/layerfs-measure/2026-11-25-commit-rt/ (statement log, lldb backtraces, harness/zzrt_commit.rs)
  - ~/.cache/layerfs-measure/2026-11-25-commit-scaling/

SCOPE (owner, 2026-11-26): conservative only. That means F1-F5, with F6 optional. Excluded: a Store format change, a server-side SQL tree walk, parallel construction or a second connection, a cache sized to a whole Commit, and stretch targets. Keep LAYERFS_CONSTRUCTION_WORKERS=1, windowed read-ahead, and POOLED_VALUE_CACHE_BYTES unchanged.

PHASE 0: BASELINE (you, or one subagent)
1. Start from the current tip of origin/layerfs_v2_0_0. Create the integration branch claude/commit-batching and its worktree with tools/worktree.py.
2. Port harness/zzshape_commit.rs as an untracked test, plus a small script that runs one cell per command under 120 s.
3. Re-run the base arm on that tip for every cell in the pass table, on PostgreSQL and SQLite. Use statement logging off for timing and on for per-statement counts. Record round trips by phase and statement, time, the resulting root, the Save's stored_bytes, inserted, reused and packs, peak RSS, and host load. These numbers replace the 2026-11-25 "before" values for judging.

PHASE 1: IMPLEMENT IN PARALLEL
One implementer subagent per lane. Each lane gets its own worktree and branch off claude/commit-batching (via tools/worktree.py, with its own Cargo home and target) and its own postgres:17 container on 127.0.0.1 with a distinct name and port.
- Lane A: F1 + F4. Both touch store/hints.rs and store/commit.rs: batch the bindings walk, read inode values once per leaf set via inode_values_ahead, and merge the logical_after and collect_new_bindings walks into one.
- Lane B: F3, the directory-rewrite base reads. First confirm the N2 call path with a backtrace.
- Lane C: F5. Batch the base-file locate_chains and ranges reads, and make no base content reads when only metadata changed.
- Lane D: F2.
  - Start with an investigation: why does the layerfs-project #67 guard (save/select.rs:52-70) miss ~60 B near-identical files?
  - Then choose a design by measured storage on B1, B6e and B6n.
  - Implement only an option within +5% stored bytes. If none qualifies, report and stop the lane.
- F6 comes last, and only if time allows, under the same storage rule.
Every lane delivers:
- counted round-trip pin tests for its shapes;
- exact storage identity against phase 0 (F1, F3, F4, F5);
- drop and kill sweeps at every server message of any new or changed batched message;
- its affected suites passing;
- one commit per fix with a Production LOC line.
At most three lanes build at once (Docker has 8 GB and 8 CPUs); use cargo -j 2 per lane while three are building.

PHASE 2: INTEGRATE
Merge the lanes into claude/commit-batching one at a time, resolving conflicts yourself. Re-run the affected pins after each merge.

PHASE 3: VERIFY (three subagents in parallel)
- Matrix runner: runs the whole pass-table matrix once on the integrated head against the phase 0 base. ABBA order, one sample per arm, both backends. It fills the pass table with round trips, time, stored bytes, roots and RSS.
- Independent reviewer: gets the diff and the spec's requirements, not your conclusions. Checks correctness, custody, bounded memory, hidden retries, and SQLite and PostgreSQL parity.
- Independent tester: writes adversarial outside-in tests:
  - fault sweeps;
  - a seeded SQLite-PostgreSQL equivalence run;
  - large and wide shapes;
  - concurrent Commits on several Workspaces;
  - metadata-only changes that hide a real content change.

PHASE 4: ITERATE
- Send findings and misses to the lane implementer that owns them; use SendMessage so it keeps its context.
- Re-verify only the affected cells and tests, then repeat phases 2 and 3.
- Stop when every row of the terminal pass condition holds on one final commit and the latest review and test rounds find no open product bug.
- A time miss is diagnosed from the receipts and never resampled. A miss that survives a fix attempt is recorded for the owner, not relabeled.

RULES (binding)
- Every test command has a wall timeout of at most 120 s. This Mac has no `timeout`, so use a wrapper script. Build with --no-run first. Never loop, background, or grep/tail-filter test runs: save the full output to a file and read it. No command runs longer than about 2 minutes.
- One sample per arm, in ABBA order. Record host load, and never wait for it to drop.
- Product code makes one attempt per operation: no retry, busy handler or replay.
- Normal SQLite profile only.
- A Production LOC line (tools/production_loc.py, before -> after, delta) in every commit, as AGENTS.md describes.
- Tests that are tricky to adapt are deleted, not nursed, and listed. Never weaken a surviving assertion.
- Never change host network settings. Publish containers on 127.0.0.1 only, and remove them at the end.
- Run python3 tools/cleanup.py check at the start, every 30 minutes and after build batches. Follow ~/.claude/CLAUDE.md when free space is below 100 GB.
- Do not push or open a PR until the owner asks. Do not touch the main checkout or other agents' worktrees.
- Overlap: local branch claude/writer-reopen (not merged) changes PostgreSQL session/pipeline.rs and lane/lane.rs. If it lands on layerfs_v2_0_0 first, rebase onto it.
- Update docs/architecture/engine/store/postgresql.md, sqlite.md and engine/commit/ for what changed.

REPORT AT TERMINAL PASS
- The filled pass table: base against head, per cell, both backends.
- Storage deltas, memory, and every test run with its NOT_RUN count.
- The review and test rounds and their findings.
- Production LOC per commit, the branch and its head.
- Each miss with its diagnosis.
Add a "Results" section to SPEC.md on the branch, and to the doc, and comment on #810 with the same results.
```
