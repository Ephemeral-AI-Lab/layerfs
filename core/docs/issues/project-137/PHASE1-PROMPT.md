# Phase 1 prompt: CLI/API/SDK additions for Connect (layerfs-project#137)

> **Status:** Current work order for phase 1 of
> [layerfs-project#137](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/137).
> Phase 2 (the Connect adapter and UI, #145–#153) is separate work in
> `layerfs-connect` and is not part of this prompt.

## Assignment

Implement and verify the LayerFS sub-issues of #137 on `layerfs_v2_0_0`, open one
pull request into `layerfs_v2_0_0`, merge it after your own verification, and close
each issue with its evidence.

| Issue | Lane | Change |
| --- | --- | --- |
| [#138](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/138) | API | Verify the CLI contract Connect depends on, at the final build |
| [#139](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/139) | API | U1: framed exec output (`--events`): every output byte plus exactly one final result or refusal |
| [#140](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/140) | Control | U2: a cancel that arrives before an exec is registered must stop it |
| [#141](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/141) | API | U3: machine-readable capabilities and build identity |
| [#142](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/142) | Control | U4: `store stats <name>` over the existing StoreStatistics counters |
| [#143](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/143) | Engine | U8: `project stats <project> [<branch>] [<commit>]`, entry counts of one pinned saved tree |
| [#144](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/144) | Engine | U5/U6: scalable pinned comparisons with Layer endpoints, stateless continuation |

**Base:** `layerfs_v2_0_0` @ `ce0e00bef2c958686038544afbf7da4e23dce8af`.
**Worktree:** a dedicated worktree and branch registered with
[tools/worktree.py](../../../../tools/worktree.py); never switch another session's
checkout. **Design source:** the Connect review (layerfs-connect
`docs/specs/cli-integration.md`, `connect-cli-obs-stat.md`); each issue body
restates what this prompt needs.

## Rules that apply throughout

- Read the root, [API](../../../../api/AGENTS.md), [control](../../../../control/AGENTS.md),
  [engine](../../../../engine/AGENTS.md) and [core](../../../AGENTS.md) guides first.
  The layering is `core < engine < control < api`.
- A new call is one file `api/crates/layerfs-api/src/<family>/<call>.rs`, registered
  in its family's `mod.rs`, making exactly one control-crate call. Never name an op
  as a string. The packages follow the table; add no JavaScript or Python names.
- One attempt per operation: no retry, busy handler or replay. Readiness waits are
  not retries. Preserve exact failure and uncertainty.
- Disposable Store profile only.
- Every test command has an explicit wall timeout of at most 120 s. Build with
  `--no-run` first. A test reaching the ceiling has failed as a hang.
- New production files stay within 999 lines; `lib.rs`/`mod.rs` within 200.
- Every commit records `Production LOC: <before> -> <after> (delta …)` from
  `tools/production_loc.py`, with scope and method.
- Update the affected SDK pages, CLI guide and call-table pages with each change.
  Keep implemented, proposed and measured claims distinct.
- CI reports and never gates. Verify the changed scope yourself.

## Per-issue scope and acceptance

### #141 U3: capabilities and build identity
- `layerfs --version` keeps its first line. `layerfs --version --json` prints
  `{version, commit, dirty, features}` with the source commit embedded at build time.
- `layerfs help --json` prints the call table (`layerfs_api::table_json`) plus the
  outside-table commands: the Doctor targets with their options, `import-env`,
  `worker`, and the CLI switches with the calls they apply to.
- Tests in `api/crates/layerfs-cli/tests/`; document both in `front-ends/cli.md`.

### #139 U1: framed exec output
- A new switch `--events` for calls with output events (`workspace exec`). It is
  exclusive with `--stream` and `--text`.
- stdout carries NDJSON in order:
  - `{"progress":…}` events;
  - `{"stream":"stdout"|"stderr","bytes":…}` events (text, or `{"base64":…}` when
    not UTF-8);
  - exactly one final `{"result":…}` or `{"refusal":{code,message,detail}}`.
- Exit codes: 0 for a result whatever the command's exit; 125 for a refusal; 2 for
  usage. A usage error stays a `layerfs: …` line on stderr.
- Tests: output over 1 MiB, non-UTF-8 bytes, a command exiting 2 and 125, refusal,
  cancelled command, closed stdout.

### #140 U2: cancel before registration
- Reproduce: `exec-cancel` sent right after spawning `workspace exec --exec-id X`
  answers `ended`, and the command then runs.
- Fix: a cancel that finds no record leaves a pending-cancel mark keyed by the
  Workspace and exec id. `Running::start` honours it: the command is never spawned,
  or is ended at spawn, and the exec answers `kind:"cancelled"`. The mark is
  removed when consumed, and expires so it cannot cancel a later command reusing
  the id.
- Keep one explicit attempt. Unique-id rules and `ended` for a finished command
  are unchanged.
- Cover local, worker and forwarded routes where tests exist. Correct
  `sdk/workspace/exec.md` and `exec_cancel.md`.

### #142 U4: `store stats`
- `layerfs store stats <name>` →
  `{name, kind, history_logical, canonical, stored, metadata, file, free | null, init | null}`.
- Store-wide counters read O(1) from the existing statistics (no scan). Routed like
  `store status`: native SQLite in-process; other machines through the worker
  (`Unavailable` when down; nothing started); PostgreSQL in-process (`free` null).
- Document `history_logical` (Commit **and** Layer records) and `file` (database
  pages including WAL; PostgreSQL schema relations rounded to 8 KiB).

### #143 U8: `project stats`
- `layerfs project stats <store>/<project> [<branch>] [<commit>]` →
  `{project, at{id,kind,branch,root}, files, directories, symlinks, others, logical, complete, reason?}`.
- Defaults: `main` and its saved head. A given commit must be a full id admitted by
  the Branch context.
- Root directory not counted; hard links counted per path; symlinks never
  followed. Bounded by the read budget: `complete:false` with a `reason`, never a
  silent cap.

### #144 U5/U6: scalable comparisons and continuation
- **U5:** a pinned comparison with Layer endpoints must not walk unchanged
  subtrees. Either `commit diff` accepts Layers, or pinned `branch diff` prunes
  unchanged subtrees when attribution is off. Keep Capacity semantics for genuinely
  huge change sets.
- **U6:** numbered queries (`file revision`, pinned `branch diff`) return an
  optional stateless continuation token bound to the immutable pin and query. A new
  process resumes from it without replay; a mismatched token is `InvalidInput`.
- Tests: a large tree with one changed file, Layer↔Layer, first Commit↔base Layer,
  and resume in a new process.

### #138 Verify the contract (last)
- Build the final branch tip. Run every check in the issue against that build on
  fresh, owned, Disposable homes. Use native macOS for exec/cancel; use Docker or
  worker routes where available, otherwise NOT_RUN with the reason.
- Write the receipt under
  [project-137/receipts/](receipts/) with PASS / FAIL / NOT_RUN for each check,
  the binary identity and the commands.

## Delivery and closing

1. Commit per issue on the phase-1 branch, each with its LOC line.
2. Push the branch; open one PR into `layerfs_v2_0_0` that lists the issues, the
   checks run and the gaps.
3. Merge after verification passes. A FAIL that cannot be fixed in scope stays
   open with its evidence; do not close it.
4. Close each issue with a comment linking the PR, its commit and the receipt
   lines. Tick the parent's checklist and leave #137 open until phase 2 is done.
5. Record this phase's outcome in [PHASE1-RECORD.md](PHASE1-RECORD.md).
