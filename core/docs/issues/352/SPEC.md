# #352 — dsh history replay: SPEC

> **Status:** Current plan, 2026-10-18. The numbers come from dsh's git history.
> They stay estimates until the pilot (phase 1) measures them. Improve this file
> where the work shows it is wrong, in the same commit as the change.

Issue: [#352](https://github.com/Ephemeral-AI-Lab/layerfs/issues/352).

The experiment replays deepseek-harness's git history on LayerFS as multi-branch,
multi-agent development. It has three goals:

1. prove that main matches git at every checkpoint;
2. measure storage and latency as the file system and the history grow;
3. optimize for speed and compression, and fix every operation that slows down
   for a reason it should not.

It builds on the dsh plugin and its demos (#345, #346, #347).

## 1. Source facts

The source is deepseek-harness. The owner's checkout
(`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness`) is never read or edited for the
replay. It is a partial clone (`blob:none`): it holds commits and trees but not
historical file contents, so a local clone of it fails. The replay reads a full
bare clone of `https://github.com/deepseek-ai/deepseek-harness` at
`~/lfs-history/dsh.git` (304 MiB, every blob present), made once with
`git clone --bare`.

| Fact | Value |
| --- | --- |
| Commits | 22,395 (8,645 merges) |
| Mainline (first-parent) commits | 2,188 |
| Files at HEAD | 16,297 tracked (133 MB): 16,273 regular, 10 executable, 14 symlinks |
| Git pack | 303 MiB |
| Unique file versions in history | 268k, 6.9 GiB raw |
| Ignored files in a working copy | `node_modules` 78k files, 2.3 GiB; desktop build 4.8k files, 0.7 GiB; `lib/` and `dist/` ~40 MiB |
| Last 2,000 mainline steps | ~70 files, +1,889 / −613 lines each |
| Dates | 2026-06-10 to 2026-10-17 |

## 2. Plan shape

| Unit | Estimate |
| --- | --- |
| Ranges | 202 (P0), each 30–100 dsh commits (target 60), ending at a mainline commit |
| Lanes per range | 2–4 LayerFS Branches, each grouping side branches by repository area |
| Agents | 5–10 plus a lead; always more agents than lanes, several per lane |
| Sandboxes | 4–8, long-lived; each holds many Workspaces of agents from different lanes (P2 played one per agent, 10–11; corrected to a shared pool of 4 from P3, SCRIPTS.md) |
| Workspaces per range | ~50–100, at least 70% one-shot (mount → exec → commit → unmount) |
| fs : Sandbox file mutations | 9 : 1 |
| LayerFS Commits | ~249k in total (P0), one per fs mutation, Sandbox Commit, merge and reconcile write |
| Bash calls | ~30k–40k: one per dsh commit's fs batch, one per one-shot Workspace |

### Ignored files

> **Installs** (owner decision 2026-10-18, firm): [INSTALLS.md](INSTALLS.md). Every
> install is a real `pnpm install` by exec inside the Workspace, reading a pnpm
> store mounted from outside it. It wins over this section where they differ.
>
> **From P3: [NODE_RULE.md](NODE_RULE.md)** (owner-approved 2026-10-19) is the
> generic Node operation rule: installs, refreshes, builds, tests, merges and
> proofs. It wins over this section and over INSTALLS.md where they differ.

LayerFS stores everything `.gitignore` covers, so the replay makes those files as
a real project would. They dominate the Store's growth (§7, Disk), so they follow
these rules:

| Files | Made | Rule |
| --- | --- | --- |
| `node_modules` | In a Sandbox at range 1 and at each range that changes `package.json` or `pnpm-lock.yaml` (197 ranges) | Install on top of the existing tree (`pnpm install --offline --frozen-lockfile`). A clean install (`rm -rf node_modules` first) only at the milestones |
| `lib/`, `dist/` | Each range builds the packages it changed (`tsc -b`), before its unit tests | Every range |
| `apps/desktop/.desktop-build` | Never: the Electron build needs downloads and a desktop toolchain | Not made |
| Caches | `tsbuildinfo` and Vite caches, made by the builds above | Kept, as a real project keeps them. The pnpm store stays on its own Docker volume outside LayerFS (Speed-ups, 4) |

The oracle cannot take ignored files from git. Their expected state:
- `node_modules`: for each lockfile hash, the tree the first install made (each
  file's git blob id and mode), recorded once and then compared;
- build outputs: they exist and the tests pass.

### `.git` in Workspaces (owner decision 2026-10-18, F-031; from P3)

From P3 on, the replay's Workspaces carry a real `.git`, as AGENTS requires
(a Workspace includes `.git` and its index). Before P3 they had none: the seed
was checked out with `--git-dir`/`--work-tree` from a bare clone
([BUGS.md F-031](BUGS.md)). Ranges 0–14 keep their recorded workload.

**Design gate (owner, 2026-10-18): F-031 is not built until a short design is approved by the owner.** It must say (1) how lanes commit: which Workspace and Branch runs each `git commit`, and on which Git branch; (2) how a LayerFS merge of two Branches that both changed `.git` treats `.git/index`, `HEAD`, refs, `logs/`, `ORIG_HEAD` and packs written by auto-repack (conflict, rule or rebuild, with no retry or guessed resolution); (3) how dsh's merge commits are reproduced with their original parents (both parents' objects in one `.git`). The design names its proof and its expected Store and wall-time cost.

- **Seed:** the full history up to the P3 start commit only: every commit,
  tree and blob reachable from range 15's start commit (`6e5c908a1`, range 14's
  checkpoint), with refs and HEAD at that commit and nothing later. The bare
  repo holds history up to today, so a plain clone would leak future commits.
  The builder picks the method (for example, `git pack-objects --revs` from the
  start commit, or a clone pruned to refs at or before it) and proves it:
  `git rev-list --all` in the seeded `.git` contains no commit after the start.
  Measured by the lead (2026-10-18): 1,545 commits; `pack-objects --revs` gives a
  9.35 MiB pack and a 0.73 MiB index (the whole bare repo is 301 MiB). The
  replay's own `git commit` per dsh commit then adds later history one commit
  at a time.
- **Cadence:** one `git commit` per replayed dsh commit. After a step writes a
  commit's files, the agent runs `git add` on exactly that commit's paths, then
  `git commit` with the original message, author and dates, so the Workspace's
  HEAD equals the replayed dsh commit id where the history allows it. Each is
  an ordinary exec in a mounted Workspace. Git's index, objects, refs and any
  repack are captured like any other files. Merge commits (reconciled, not
  replayed, §P0) are handled as the builder records in SCRIPTS.md before P3.
- **Proof:** the checkpoint tree without `.git` is checked against git as
  before (§7, Proof checkpoint). In addition, `.git`'s HEAD must equal the
  checkpoint commit and `git status` must be clean.
- **Counts change:** every replayed dsh commit adds at least one exec and its
  Commit. P0's Bash, exec and Commit totals and §5/§7 growth expectations are
  re-derived at the P3 re-baseline from the builder's dev-range measurements.
  Those report Store growth per git commit, split into index rewrites, loose
  objects and packs, and the wall time added per range. A dated
  STORAGE_STUDY section, "Recalibration for `.git` (F-031)", follows.

### Tests: cheap unit tests only

The replay runs tests to give exec over FUSE real work, not to test dsh. So only
cheap, self-contained unit tests run. Never:
- dsh's own test entry points: `pnpm test` (it compiles the native addon first),
  `check:*`, `test:e2e`, `test:web*`, `test:gui`, `test:bench*`, `test:snapshot`,
  `test:expected`, desktop packaging;
- anything that needs a browser or a GUI: Selenium, Playwright, Puppeteer,
  WebdriverIO, vitest browser mode, Electron (`apps/desktop`);
- anything that needs the environment: the native addon (`native/`), the network
  or real model providers, Docker, SSH, Windows, a real shell sandbox, snapshot
  recording.

**Selection** is static: the planner writes each range's list into the plan.
1. **Candidates:** the `*.spec.ts` and `*.test.ts` files under the `tests/` folders
   of the packages the range changed. Milestones take every package's.
2. **Drop by path:** `apps/desktop`, `apps/web/tests`, any `e2e`, `bench`, `perf`,
   `stress` or `snapshot` folder, `native/`, `packages/ssh`, `packages/sandbox`.
3. **Drop by content:** a file that imports `playwright`, `puppeteer`,
   `selenium-webdriver`, `webdriverio`, `electron` or `@vitest/browser`, or reads
   `DSH_SNAPSHOT`.
4. **Calibrate once, natively.** Run each remaining file alone at the range's
   checkpoint in the one moving checkout, in the runtime image, outside LayerFS,
   with `--network none` and a 10 s timeout (`calibrate-tests.mjs`). Its install
   is the replay's own (`INSTALL`: offline, frozen lockfile, no install scripts). A file that fails or times out there
   is skip-listed for that range with its reason (`skipped` in
   `tests-NNN.json`, written beside the plan, which stays as written):
   it is environment-dependent or broken at that commit, not LayerFS's problem.
5. **Cap:** up to 60 s of native test time per range (at most 30 files, fastest
   first); milestones up to 5 min.

**Run:** `vitest run <files>` with the range's own root config, in a one-shot
Workspace, with `CI=1` and no network.

**Pass rule:** each file's result in LayerFS equals its native result. A file
that passes natively and fails in LayerFS is a stop finding (a LayerFS bug). It is
never skip-listed after the fact.

### Runtime image

Every replay Sandbox and the native calibration use one image, so a test sees
the same Node and package manager in both. Built on 2026-10-18 from
[`dsh/tools/history/runtime/Dockerfile`](../../../../dsh/tools/history/runtime/Dockerfile):

| Fact | Value |
| --- | --- |
| Tag | `layerfs-352-runtime:node24` |
| Image id (`LAYERFS_IMAGE`, `--image`) | `sha256:d3773b4eac6a7a644887b17e36956cd1553786d47103f0170a3bb2b0ff6988de` |
| Base | `node:24.21.0-bookworm@sha256:3d27e5c11e5786e309ec3e03f93ae536eb36e6e5eb3714d5eb3300a36157add0` (arm64) |
| Size | 414 MB |
| Node, npm, corepack | 24.21.0, 11.19.0, 0.36.0 |
| Package managers, prepared offline | yarn 4.14.1 (range 0), pnpm 11.7.0 (ranges 1–187), pnpm 11.28.5 (ranges 188–201); each project's `packageManager` picks its own |
| Also | git 2.39.5, python 3.11.2, gcc 12, make 4.3; Debian 12, glibc 2.36 (as the daemon's build) |
| Environment | `COREPACK_ENABLE_NETWORK=0`, `CI=1` |

Why not the earlier demos' image (`layerfs-issue337-runtime:frozen`, 9 GB): it
has Node 22.23.1, which ranges 0–9 refuse (`engines.node >=24`), and no pnpm
or yarn, which corepack would have to download with the network off.

Checked offline (`--network none`): each package manager answers its version
inside a project naming it. Not yet checked: a LayerFS `sandbox.create` with
this image id; the first P1 Sandbox proves it. A changed image gets a new id,
and every run records the id it used (snapshot manifests, `tests-NNN.json`,
the metrics' `binary.image`).

### The two nested cycles

**Outer cycle (one per range, ends at a checkpoint):**

1. *Prepare.* Load range k's plan and oracle. Run the disk guard (§7). Check the
   binary against the run's manifest.
2. *Run the inner cycle* on every lane, agents in parallel.
3. *Consolidate.* The lead merges the lanes into main and publishes.
4. *Verify the checkpoint* (§7, Proof checkpoint):
   - LayerFS's changes since the previous checkpoint are exactly git's changed
     tracked paths, with git's blob ids and modes;
   - history matches the plan;
   - every planned conflict happened and was resolved;
   - ignored files match their expected state;
   - the range's unit tests give their native results (§2, Tests).
5. *Write the range outputs* (§3) and *evaluate the gates* (§5).
6. *Seal and snapshot* when due (§7): every range during the pilot, then when
   fast-forwarding would cost more than a snapshot, before an optimize fix, and
   at milestones. A snapshot writes only the blocks that changed.

**Inner cycle (one per step):**

1. *Implement.* One agent's bash call. It is either a batch of fs writes for one
   dsh commit (each write its own LayerFS Commit) or one one-shot Workspace.
2. *Verify the step.* Every call's outcome (Committed, Conflict, UpToDate,
   Merged) must match the plan.
3. *On a mismatch:* stop and keep the run. Diagnose, fix, classify the fix, and
   recover (§6).

### Git rule (from P3, F-031)

From P3, every range also follows the [git rule](GIT_RULE.md). It says which
cycle step runs which git commands, in which Workspace and on which Branch,
and what each writes under `.git` (F-031).

### Agents and conflicts

Agents write file *content*, named by git blob id. They never apply patches. The
plan fabricates four kinds of conflict, each ordered by barriers and never by
pauses (the 5 agents demo lost its Conflict to pacing):

| Kind | Arrangement | Expected |
| --- | --- | --- |
| Same-lane Commit Conflict | B mounts, A commits the same file, B commits | B gets Conflict, remounts, rewrites, commits |
| Stale-Workspace clean merge | B mounts, A commits a *different* file, B commits | B merges cleanly |
| Decoy edit | Two agents edit one file; one edit is reverted before the checkpoint | The final tree still matches git |
| Merge/publish race | A lane publishes while the lead merges another lane | One wins; the other merges again |

### Scripts and players

One plan feeds two players.

```
dsh/tools/history/
  git.mjs       git access: raw diffs, trees, sizes, and the blob cache
  ranges.mjs    mainline steps, range cutting, snapshot cadence
  plan-range.mjs one range's plan, and replayPlan, its static check
  plan.mjs      read the dsh clone; write ranges, lanes, agents, steps and planned
                conflicts from a fixed seed
  oracle.mjs    each checkpoint: tree id and expected history; compareTree; checkout
  run.mjs       headless player: measured mode, and fast-rebuild mode (no timing,
                Sandbox tests or agents)
  gates.mjs     the gate decision (§5), latency aggregation, the pilot baseline
  report.mjs    ranges/range-NNN.md and RANGES.md; exit code for the runner
  snapshot.mjs  take, verify and fork block-incremental snapshots; when one is due (§7)
  inspect.mjs   a checkpoint's files, oracle diff and Store figures (§7)
  unit-tests.mjs  a range's unit test selection, run choice and comparison (§2, Tests)
  calibrate-tests.mjs  the one-time native run of each range's candidates (§2, Tests)
  scenario.mjs  turn a window of ranges into a dsh scenario
dsh/scenarios/dsh-history/
  scenario.json the Live demo: a window of ~3 ranges seeded from a milestone snapshot
  run/          final.json like Crew's, small enough to commit
~/lfs-history/  outside the repository
  plan/ oracle/ all ranges (too large for git)
  snapshots/    block-incremental Store snapshots: blocks/ and one manifest each
  metrics/      one line per call, one summary per range
```

- The headless player extends `run-scenario.mjs` and `scenarios/shared/replay.mjs`.
  It has no model loop: agents run as concurrent tasks and every call is timed.
- Small files reach a Sandbox inside the exec command. The planner sends changes
  above a size cap through fs instead, so every step is self-contained.
- The Live demo needs one new feature: a demo seeded from a Store snapshot.
  `tools/restore-store.mjs` already restores one for the checks.
- Existing checks are reused: `check-scenario.mjs` for the plan, and check-demo's
  gates (expected, closed, order, final tree) at every checkpoint.

### P0 results (2026-10-17, plan v1, seed 352)

> From P3, `.git` (§2, `.git` in Workspaces) adds about one exec and Commit
> per replayed dsh commit (13,749 in total) on top of these counts; they are
> re-derived at the P3 re-baseline.

`plan.mjs` and `oracle.mjs` are built and tested
(`dsh/tools/history/tests/history.mjs`, 15 tests on a synthetic repository). Over
the whole history the plan takes 16 s and the oracles 20 s.

| Total | Planned |
| --- | --- |
| Ranges | **202** (7 oversize, up to 246 commits, where one merge brings that many) |
| dsh commits replayed | 13,749 non-merge commits (merges are reconciled, not replayed) |
| Bash calls / steps | 37,292 / 35,847 (p50 171 per range, p90 240) |
| LayerFS Commits | 249,004 (p50 1,030 per range, max 6,207) |
| File mutations fs : Sandbox | 218,595 : 23,166 (9.4 : 1) |
| Workspaces | 19,038, of which 18,661 one-shot (98%) |
| Read calls in steps | 63,296 |
| Reconcile writes | 21,983; 84% are file versions that only git's merges produced |
| Planned conflicts | 202 Commit Conflicts, 175 stale clean merges, 202 decoys, 40 merge races |
| Installs | 197 ranges (`pnpm-lock.yaml` changes on 645 mainline commits) |
| Snapshots / milestones | 48 / ranges 4, 50, 100, 151, 201 |

**Plan v2 (written, not yet generated):** each range's closure carries its unit
test selection (§2, Tests), and the plan fixes snapshots only for the pilot
ranges and the milestones; other snapshots are decided at run time (§7, When).
The numbers above are v1's; regenerate before P1.

Design decisions the build settled:

- **Lanes by area.** A path's area (its first two folders) decides its lane, and a
  commit that spans lanes is split, one part per lane. Lanes never touch the same
  path, so their merges into main are clean. The only conflicts are the planned
  ones.
- **Agents.** A side branch's commits go to one agent of the lane. Agents are
  numbered `a01`…`a10` within a range.
- **Expected outcomes are computed per file.** A write equal to the lane's
  current content expects UpToDate (1,319 of them). An `rm` of a file the lane no
  longer has is dropped (401).
- **Symlinks, executables and mode changes go through a Sandbox `apply` step**,
  because an fs write cannot set a mode.
- **Static check.** `replayPlan` plays each range's steps and its reconcile over
  the previous checkpoint without LayerFS. It must give exactly the next
  checkpoint's tree, and every `check` or `hold-resolve` step must see the blob
  it expects. All 202 ranges pass. `plan.mjs` refuses to write a plan that fails.
- **The tree is compared by git blob id**, not SHA-256: the verifier hashes each
  LayerFS file as `sha1("blob <len>\0" + bytes)` and compares it with
  `git ls-tree`, so no file list is stored in the oracle.
- **Checkout.** One working tree, `~/lfs-history/checkout`, moves to each range's
  checkpoint with `oracle.mjs checkout`. Git rewrites only the files that changed
  (0.13 s for a range), so there is never a second copy. Intermediate versions
  inside a range come from the blob cache, a long-lived `git cat-file --batch`
  process with an LRU, because a checkout holds only the checkpoint.

### Script generation

[SCRIPTS.md](SCRIPTS.md) is the guideline for turning a range's plan into steps.
It covers:
- the scenario format;
- the main agent and subagents;
- how the plan's barriers become the lead's turns, using Crew's `turns.mjs`;
- the shared step library both players call;
- naming, `expect`, the seed, and validation.

### Maximum speed, always

Iteration never waits on purpose. Every second goes to LayerFS work, the commands
and the checks.

- **The headless player runs with no delays.** `run.mjs` has no pauses, typing
  delays or playback holds. It never passes `--pace` to the `run-scenario.mjs`
  machinery.
- **The mock agent script runs at Max.** `dsh/scenarios/dsh-history/scenario.json`
  sets `"speed": 0`. Every dsh run during iteration, including P4's Live window,
  plays at Max. A slower speed is only for the owner to watch, set from the
  strip, and is never part of a gate.
- **Order comes from barriers only.** Agents run concurrently, as planned. A run
  that passes only at a lower speed is a bug in the plan or in LayerFS; slowing
  down is never a fix.
- **Agents run in parallel.** Lanes run concurrently up to the planned agent
  count. The lead's consolidation waits only for its own lanes.
- **No idle time between ranges.** The runner starts range k+1 as soon as range
  k's checkpoint, report and (when due) snapshot are written.

### Speed-ups

These rules keep the build and the run short. The measured numbers are not
affected, because each one removes waiting or repeated work, never timed product
work.

1. **The plan is precomputed and static.** P0 tools run in seconds, so the plan is
   regenerated forward (`--from N`) instead of being edited by hand.
2. **Content comes from the blob cache and one moving checkout**, never from a
   git process per file or a fresh checkout per range.
3. **Verify only what changed.** Each range checks `diff_commits` against git's
   changed paths (§7, Proof checkpoint). The full tree is checked at snapshots
   with one exec: a one-shot Workspace on a fork of main runs one node script
   that prints each tracked file's git blob id and mode, and `compareTree`
   checks it against git. Never 16k `fs.read` calls. The tracked set is decided
   as `dsh/tools/checks/tree.mjs` does.
4. **Offline installs.** Fill a pnpm store once from the lockfiles of all ranges
   (`pnpm fetch`), keep it on its own Docker volume outside LayerFS, and install
   with `--offline --frozen-lockfile`. There is no network wait, and an install
   always gives the same result.
5. **Cheap unit tests only** (§2, Tests). Each range builds the packages it
   changed and runs their calibrated unit tests; milestones run every package's.
   Nothing that needs a browser, Electron, the native addon or the network.
6. **Fast-forward, never restart.** Recovery restores the newest good snapshot
   and fast-rebuilds the ranges after it with the fast-rebuild player (no timing,
   no tests, no Sandboxes beyond what content needs). No range is replayed from 0.
7. **Release daemon only.** Measure only the release build, so a debug build's
   speed never sets a baseline.
8. **Concurrency as planned.** Lanes and agents run at the same time. The runner
   moves to the next range as soon as the checkpoint is written, and the lead
   wakes only when the runner stops.
9. **Snapshots are incremental.** A snapshot writes only the blocks the Store
   changed, so the Store is closed for seconds and the next range starts at once
   (§7). Seal and copy time are recorded separately.
10. **Budget from the pilot.** P1 measures wall time per range. P1 ends only when
   wall time × 202 fits the budget the owner sets. Otherwise re-plan (larger ranges,
   fewer checks, lighter tests) before P2.

## 3. What each range produces

At each range's completion, the runner writes three things:

- `~/lfs-history/metrics/range-NNN.json`: the raw outputs, one file per attempt.
- `core/docs/issues/352/ranges/range-NNN.md`: the range's report, one file per
  range. `report.mjs` generates it from the JSON in the binding format of
  [REPORT_FORMAT.md](REPORT_FORMAT.md) (owner, 2026-10-18; `format.mjs`, checked
  by `report.mjs check`), whose Activity tables are
  [REPORT_OVERVIEW.md](REPORT_OVERVIEW.md)'s. Reports committed before it
  (ranges 000–004) keep the shape of [ranges/TEMPLATE.md](ranges/TEMPLATE.md)
  (F2). The lead writes the Lead's note. A rerun adds an attempt to the same
  file and never replaces one.
- `core/docs/issues/352/STATS.md`: running totals across ranges
  (REPORT_OVERVIEW.md §4), rewritten by `report.mjs` with every range; ranges
  000–004 derived from their metrics (`report.mjs stats` writes it alone).
- One row in `core/docs/issues/352/RANGES.md`, the index:

  | Range | dsh commits | Depth | Verdict | Worst speed ratio | Worst scaling slope | New stored bytes | Report |
  | --- | --- | --- | --- | --- | --- | --- | --- |

- One comment on issue #352, posted by the lead: `report.mjs` writes it to
  `~/lfs-history/metrics/comment-NNN.md` (`renderComment`) in this fixed format:

  ```text
  ### Range NNN · **<verdict>** · attempt K
  dsh `<first>`..`<checkpoint>` (N commits) · depth A → B · daemon `<id>` · wall T s

  | Check | Value | Result |      (only checks that did not pass; else "All N checks pass.")

  **Work:** bash calls · LayerFS Commits · conflicts hit/planned · unit tests equal/run
  **Slowest p95:** the three slowest operations by p95
  **Storage:** +new stored (total) · dedupe · compression · snapshot +new bytes in ms
  **Optimization:** none. | or: trigger, diagnosis (pending until the fixer's
  finding), fix, recovery, the check's value now, and an improvement table
  (measure | before | after | change), from finding-NNN.aK.json (findings.mjs)
  **Next:** continue, or why the runner stopped and who acts

  Report: link to ranges/range-NNN.md
  ```

  The lead commits and pushes the report before posting, so the link works
  (HANDOFF).

**At the end of the history** (202 ranges), the replay leaves:
- 202 proof checkpoints, each a section of its range report;
- 202 range reports in `ranges/` and 202 rows in `RANGES.md`;
- one #352 comment and one commit per attempt: 202 plus one more for each
  rerun;
- the snapshots §7 takes: the pilot's 5, the milestones, one before each
  optimize fix, and those `due` asks for;
- one finding record, and one entry in `BUGS.md` or `OPTIMIZATIONS.md`, per
  optimize or stop.

### Report format rule (owner, 2026-10-18; binding)

A range report is evidence. `report.mjs check <NNN>` enforces every rule below,
and the lead runs it before each checkpoint commit: a report that fails the check
is not committed and its comment is not posted.

| # | Rule |
| --- | --- |
| F1 | **Fixed shape.** Header table, Attempts, §1 Checks … §8 Evidence, in [TEMPLATE.md](ranges/TEMPLATE.md)'s order and headings. No section is dropped; an empty one says why (`n/a: P1 has no tests`). |
| F2 | **Recorded outcomes are frozen.** Each attempt's row (binary, start, verdict, reason) and its check rows come from the gate result stored in that attempt's `range-NNN.aK.json` when it ran, never from a re-evaluation under later gates. Playing range k writes only range k's report; every earlier report stays byte-identical (F-004). |
| F3 | **Every optimize or stop attempt has a Finding block in the report**, generated from `finding-NNN.aK.json` (findings.mjs): trigger (check, value, threshold); diagnosis (playbook step, cause, counter and its exponent before → after); fix (summary, kind, commits, regression tests); recovery; and the **before/after table**. Until the finding file exists, the block says `Diagnosis pending` and names the file. The issue comment carries the same content from the same source; the comment never has a finding the report lacks. |
| F4 | **Before/after table columns:** measure · before · after · change · old binary · new binary · snapshot or checkpoint · cache state. One row per measure; units on every number; the old and new series are never merged into one fit. |
| F5 | **Links.** Each Finding block links its `fixes/NNN-aK-<slug>.md` records (one per cause, each with old/new design and performance) and its BUGS.md or OPTIMIZATIONS.md line; every link is relative and resolves in the committed tree. |
| F6 | **The lead's §7 is never empty** when the latest verdict is not pass: watch items with the probe planned for the next milestone, the action taken, and any process note (a restored report, a skipped step). `(none yet)` fails the check. |
| F7 | **Labels.** Every latency or counter carries its binary; tooling time is labeled tooling, never operation latency; diagnostic and seeded points are labeled as such. |
| F8 | **No silent pass.** A check that cannot be judged (no baseline, too few same-binary points, missing input) says so and is at least watch. |

The sections below list what the report contains.

### a. Range

- Index; first and last dsh commit (full id).
- Mainline commits and side commits it covers.
- The checkpoint commit and its tree id.
- History depth before and after: LayerFS Commits, Layers, Branches.
- The binary id and Store snapshot id it ran on.

### b. Work done

| Count | Breakdown |
| --- | --- |
| Steps (bash calls) | total; per agent; fs batches vs one-shot Workspaces vs merges |
| Branches | lanes created, merged and published |
| Agents | active agents; each agent's steps, Commits and Conflicts |
| Commits | per Branch; total for the range; running total |
| Workspaces | created; one-shot share (gate: ≥70%); long-lived; open at range end (must be 0) |
| Sandbox operations | create, mount, exec, status, commit, unmount, close |
| fs operations | each write and read method |
| branch and project operations | fork, merge, publish, list_commits, diff_commits, diff_branches |
| Exec commands by kind | JS (node, npm, pnpm, test runners) vs other shell; exit codes |
| Conflicts | planned vs observed, by kind; time from Conflict to resolved Commit |
| File mutations | added, modified, deleted; through fs vs Sandbox (gate: ~9 : 1) |

### c. Operation latency table

Write one row per operation kind with these columns:

- n
- p50, p95, p99, max (ms)
- ratio of p50 and p95 to the baseline
- slope against history depth and against tree size
- the size class of its own input (bytes, files changed, entries in the directory, files in the scope)

Compare each kind only with calls of the same size class. Take the daemon phase
times from the call trace (`LAYERFS_TRACE`), so client queueing is separated
from daemon work.

| Group | Operations |
| --- | --- |
| Sandbox | **mount** (phases `store.bind`, `kernel.attach`; logical construction), **exec** (FUSE time vs the same command run natively on the same tree), **commit** (phases `capture`, `construct`, `merge`, `publish`, `install`), status, unmount, create, close |
| fs writes | write, edit, apply, mkdir, rm, cp, mv |
| fs reads | read, list, **stat**, **grep**, **glob**, **diff_files**, blame |
| History | **merge**, publish, fork, list_commits, **diff_commits**, **diff_branches** |
| One-shot Workspace | mount + exec + commit + unmount as one total |

Read-only calls are added to the plan on purpose, because dsh's history alone
produces few of them. Each step runs a fixed sample of reads (about 1 read call
per 2 writes). In addition, the **scale probes** run once per range:

| Probe | Input | Must not depend on |
| --- | --- | --- |
| `stat` / `read` on a fixed small file | one 2 KiB file | tree size, history depth |
| `list` of a fixed small directory | a 5-entry directory created at range 1 | total files, history depth |
| `list` of the largest real directory | grows with the tree | history depth (it may grow with that directory's entries) |
| `grep` / `glob` scoped to the small directory | 5 files | files outside the scope, history depth |
| `grep` / `glob` over the whole tree | grows with the tree | history depth |
| `diff_files` on two fixed files | two 8 KiB files | everything else |
| `diff_commits` of adjacent Commits | one file changed | history depth, tree size |
| `merge` with a fixed small change on each side | 2 + 2 files | history depth, tree size, distance to the merge base |
| `list_commits` first page | page of 20 | history depth |
| `fork` and `publish` | — | history depth, tree size |
| one-shot `mount` + `unmount` (around an `exec true`) | — | tree size, history depth |
| the `exec true` between them: its LayerFS share | — | tree size, history depth |
| one-shot `commit` of one 2 KiB file | one file | tree size, history depth |

> **As built (F-014, 2026-10-18).** Each fixed-input probe records its input
> in the metrics (`probes.<name>.input`): bytes (stat, read), entries
> (list_small), files in scope (grep/glob scoped), both files' bytes
> (diff_files), changed files (diff_commits), files per side (merge), page size
> (list_commits), one 2 KiB file (one-shot commit). The `diff_commits` probe is
> version 3 (`probes.diff_commits.version`): it forks its own Branch, writes a
> fixed 2 KiB file (Commit 1), rewrites it at the same size (Commit 2), and
> diffs exactly those two adjacent Commits (version 2 looked for a Commit on
> the fresh fork, which has none, and never produced a value); version 1
> diffed the lane's newest Commit, whose change grew with the range (4 → 60
> files). The gates fit a probe only over points of its version and recorded
> input; a different earlier input, a missing input or too few such points is
> "not judged" (watch). `list_largest` and grep/glob over the tree may grow and
> are exempt. The other probes' inputs were already fixed. Recorded values and
> verdicts stay as recorded; `report.mjs regate` re-evaluates recorded metrics
> and prints only.

> **As built (one-shot split, lead 2026-10-18, after 013.a1).** Launching
> the command belongs to the Sandbox, not LayerFS: 013.a1's optimize came
> from `exec true` (33 → 49 → 62 ms, no FUSE requests), while mount and
> unmount were flat. So the probe `oneshot_mount_exec` is replaced by two
> probes with new names. The old series ends, and no fit mixes old and new.
> - **`oneshot_mount_unmount`:** the traced ms of the mount and the unmount,
>   and their SQL counters, judged and scaled as before.
> - **`oneshot_exec`:** the exec's LayerFS share, judged and scaled. The
>   share is the daemon's traced spans for the exec (its bookkeeping). The
>   trace counts the exec's FUSE requests but does not time them, so a point
>   with any FUSE request is not judged (watch). The exec's wall time
>   (`wallMs`) is reported and never scaled.
>
> Thresholds and the one-sample rule are unchanged. 013.a1's record and
> verdict stay. `report.mjs regate --split-oneshot` re-reads earlier attempts'
> sequences from their stored traces, in memory only.

> **As built (probe samples, owner direction 2026-10-18).** Each scale probe
> runs five samples after its untimed setup. The fit uses their median
> (`probes.<name>.ms`); `samples` holds all five.
> - **Counters:** only the first sample's (label `probe:<name>`), so counter
>   gates read one sample as before. The other samples run as
>   `probe-s<i>:<name>`.
> - **Probes that make things:** each sample gets its own name. Fork and
>   merge samples after the first make their own Branches (`-s<i>`), so a
>   range adds 16 probe Branches more than before. Each `oneshot_commit`
>   sample writes the same 2 KiB with its own byte.
> - **One-shot split:** taken per sample from the trace, then the median.
> - **Load average:** the 1-min load average is recorded beside each sample
>   (`load`) and per range (`hostLoad.start`, `.end`). It is the load where
>   the player runs; in the runner that is the Docker VM's kernel. Report
>   appendix C shows it.
>
> Earlier ranges' single-sample points stay in the fits as recorded
> (`Samples: 1`).

### d. Storage

Read from `operator.store.stats()` and per-Commit `sizes`:

- **Size:** logical, canonical, stored, metadata, DB file pages and free pages,
  and WAL size before the seal. Report the new bytes in this range and the totals.
- **Deduplication:** logical / canonical, for the range and in total. Track
  growth per `node_modules` reinstall separately.
- **Compression:** canonical / stored.
- **Delta:** the share of objects stored as deltas; the depth distribution per
  role (whole-file, chunk, metadata); the share of objects at the depth cap; the
  share whose chain stopped at the chain-work budget instead of the depth cap.
- **Directory share:** metadata bytes as a share of the range's new bytes.
- **Efficiency:** the range's new stored bytes per changed git bytes.
- **Growth:** the range's new stored bytes against the median of earlier ranges,
  split into tracked files, `node_modules`, build outputs and metadata.
- **Snapshot:** new blocks and their bytes, seal time and copy time.
- **Projection:** the Store size at 202 ranges, against the disk budget.
- **Long history:** whether new bytes per changed byte or per-call latency rise
  with depth. Read this from the latency table's depth slope and from the internal
  counters (rows read, pages touched, ancestors walked per call). A counter that
  grows linearly with depth means the code is proportional to history. That is a
  bug to fix even while the latency is still small.

## 4. Optimization focus

The order is: **speed** first, then **storage compression**.

### Scaling rules

Each operation may grow only with its own input. Its input is what it reads or
changes: the file, the directory listed, the paths in the scope, or the changes
being merged. Neither the size of the whole file system nor the length of the
history is part of any operation's input.

| Operation | May grow with | Must stay flat with |
| --- | --- | --- |
| stat, read | file size, path depth | total files, history depth |
| list | entries in that directory (one page) | total files, history depth |
| grep, glob | files and bytes in the scope | files outside the scope, history depth |
| diff_files | the two files' sizes | everything else |
| diff_commits, diff_branches | entries changed between the two | tree size, history depth |
| fs writes | changed bytes, path depth | tree size, history depth |
| Sandbox commit | files changed in the overlay | tree size, history depth |
| merge | paths changed on both sides since the base | history depth; distance to the base beyond the changed paths |
| mount, fork, publish | nothing | tree size, history depth |
| exec (FUSE) | directories and files the command touches | untouched parts of the tree, history depth |
| list_commits | page size | history depth |
| blame | revisions of that file | other files' history |

So `ls` on a directory of 50,000 entries may be slow, but `ls` on a 5-entry
directory must stay as fast at range 202 as at range 1. A violation is an
*optimize* finding even when the time is still small.

### Logical construction and mount

Improving the speed of logical construction and mount is an explicit target:

- Break mount down into Branch lookup, bind, attach and the first access.
- Measure the first `lookup`/`readdir` on a fresh mount at each milestone.
- Mount should not construct anything in proportion to the tree.

Each fix must be compared on the same milestone snapshots with the probes (§6).

### Storage

Storage work comes after speed. AGENTS.md §3.9 applies: reject a ~50% speed loss
for a ~5% storage gain. Report raw deltas for both.

## 5. Gates per range

> From P3 (F-031), ranges carry `.git` and about one more exec and Commit per
> replayed dsh commit. P3's re-baseline is taken with `.git` in place, so no
> gate compares a `.git` range with a pre-`.git` baseline.

> **Time-only trips (owner direction 2026-10-18).** A probe's time trip
> (Scaling) whose SQL counters are identical at every point of its series
> reads **watch**. It escalates to optimize only when the same probe also
> tripped time-only at the previous range (consecutive; a gap is not a
> repeat). Counter gates and every threshold are unchanged. The probes' time
> comes from five samples per probe (§3c). Earlier verdicts stay as
> recorded. Validated by regate on ranges 11–14: 013.a2 (fork, counters
> 16 / 271 at every point) reads watch; 014.a1 (checkpoint `diff_commits`
> statements, a counter trip) stays optimize.

> **Uncontended vs contended (owner direction 2026-10-18).** The headless
> player has no delays, so contention is far above real use (agents' tool
> calls are seconds apart). Each operation's latency is reported in two
> series: **uncontended** (no other Commit or merge overlaps the call in
> time) and **contended** (one does). The speed gates (ratio to baseline,
> scaling, counters) judge the uncontended series against the baseline's
> uncontended series. The contended series is reported beside it and watched
> for growth of the contended work itself (merge cost against tree size,
> lane hold per Commit); it never trips on queueing that the compressed replay
> creates. Merges are never removed from either series; overlap is a
> classification of the input, like F-024's outcome classes, and it applies to
> every operation alike. Baselines are re-derived into both series from their
> stored traces; earlier reports keep their recorded verdicts. Design rule:
> [Store concurrency model](../../architecture/83-store-concurrency-model.md).

> **Reinstall gate unit (lead decision 2026-10-18).** Changed packages are
> lockfile package entries whose resolution (name, version, integrity) or
> workspace link target differs from the previous lockfile; a changed link is
> one package. The gate is the install's new stored `node_modules` bytes ÷ the
> raw bytes of those packages in the oracle's tree (a link counts its link
> entry only): watch over 2×, optimize over 5×. An install with no changed
> package is judged by its absolute new bytes: watch over 1 MiB, optimize over
> 16 MiB (`gates.mjs` reinstallGate, `closure.mjs` changedPackages).
> **Owner decision 2026-10-19 ("proceed with the fix"):** a link-only or
> tiny-raw reinstall, raw changed bytes under 1 MiB, is judged on the same
> absolute line (watch over 1 MiB stored new, optimize over 16 MiB). A larger
> reinstall keeps the ratio, unchanged. The row names the mode and shows both
> numbers. This closes the link-unit question.

> **Baseline classes (F-024, lead decision 2026-10-18).** Each baseline class
> records n and its calls per baseline range (`perRange`). A class with fewer
> than 5 calls in any baseline range (absent = 0) is never used for a ratio:
> Speed for it reads "not judged: baseline class has a/b/c calls" (watch, F8).
> Since fix 061 a thin class never hides the judged ones: the row is decided
> by the judged classes and lists each thin class over watch after them
> ("not judged: … a/b/c calls (r×)"); only a row with no judged class reads
> "not judged" as a whole.
> One rule for every op and class, no exceptions: fs.write ≤512 KiB (3/3/8)
> and branch.merge 17–64 entries are not judged, and neither are the
> once-per-range operations (publish, store stats). Per range, not summed: the
> lead's 3/3/8 example sums to 14. In the stored baselines this leaves 23 of
> 51 classes (P2) and 27 of 44 (P1) not judged. `branch.merge` and
> `sandbox.commit` are classed by outcome as well as size (UpToDate, Merged,
> Merged with a new Commit, Committed) when the trace has it: a classification
> of the input, not an exception. The rule does not hide F-025 (in-lane merge
> queueing, open until fixed): fs.* classes affected by merges are judged as
> they come out, and merges are never filtered out of them. Baselines written
> before carry n and perRange derived from their stored ranges; their merge
> classes have no outcome and match only attempts recorded before.

> **Checkpoint diff input (F-019, 2026-10-18).** The checkpoint diff_commits
> cost is normalised by its changed entries plus their changed ancestor
> directories (a changed directory is a changed entry; basis
> `checkpoint+dirs`, `dirs` recorded in the metrics). Tree-size growth is
> judged on the fixed-input one-file probe's raw counters. The fixed/per-unit
> fits need 4 same-binary points; fewer is "not judged" (watch).

> **Chain-reading probes (F-016, 2026-10-18).** stat, read, list_small,
> grep/glob scoped and diff_files read the fixture through metadata delta
> chains: their cost rises one link per range and resets at the metadata
> delta cap (8; list_small 60 → 228 blob reads, then 60). They are judged as a
> bound, not a depth exponent: the maximum over the latest cap period against
> the maximum over the first (counters: any rise over 1.0× is watch, over
> 1.05× optimize; ms: 1.2× / 1.5×). Fewer than a period plus one point, or no
> recorded cap, is "not judged" (watch). Each bound names the probe call
> that set both maxima and whether this range set the latest one or carries
> an earlier range's (fix 062). Cost per decoded link was rejected:
> it stays flat under unbounded chain growth. `list_largest` is bounded the
> same way per entry of the directory it lists (its recorded input; liaison
> 2026-10-19, after r017.a1), `glob_tree` per tree
> file like `grep_tree` per tree byte; a growing probe
> without its own input is "not judged" (`dsh/tools/history/gates.mjs` header).

> **Checkpoint diff_commits (F-013, 2026-10-18).** Cost = a + b·n: a fixed per
> checkpoint, b per changed entry n, each flat with depth and tree size. Per
> axis: if n does not grow with the axis, the checkpoint's total statements
> (then ms) are fitted; if n grows, per entry is fitted; if n grows and per
> entry does not rise, the parts cannot be separated and the check is "not
> judged" (watch). Never normalised by tree files. Why: per entry alone rises
> whenever n falls (a false optimize at p1-rebase-b range 2), and neither the
> one-file probe nor one checkpoint's pages give the fixed part
> (`dsh/tools/history/gates.mjs` header).

Thresholds compare against a baseline taken from ranges 1–5, for the same
operation kind and size class. Set the exact numbers after the pilot and record
them here.

| Metric | Pass | Watch | Act |
| --- | --- | --- | --- |
| Correctness: tree identity, history shape, planned conflicts, step outcomes | 100% | — | Stop: bug cycle |
| fs latency p50 / p95 per kind | ≤ 1.2× baseline | 1.2–1.5× | Optimize when > 1.5× and rising with depth for 3 ranges |
| Scaling rules (§4): depth or tree-size slope of a must-stay-flat probe | flat | sublinear | Optimize when linear |
| Internal counters per call: rows read, pages touched, ancestors walked | flat with depth | sublinear | Optimize when linear with depth |
| One-shot Workspace total; FUSE vs native exec | ≤ 1.2× baseline | 1.2–1.5× | Optimize when > 1.5× |
| Mount and logical construction | flat | rising | Optimize when rising for 3 ranges |
| Commit, merge, publish by depth and files changed | flat with depth | rising | Optimize when rising for 3 ranges |
| Concurrency: planned conflicts hit; Conflict to resolved Commit | as planned | slow resolution | Stop on an unplanned or missed conflict |
| Directory share of new bytes | < 50% | 50–70% | Optimize when > 70% |
| Deduplication per `node_modules` reinstall | ≤ 2× changed packages | 2–5× | Optimize when > 5× |
| Delta (§8): objects at the depth cap | < 10% | 10–25% | Evaluate deeper chains when > 25% and stored bytes grow faster than changed bytes |
| Store projection vs disk budget | < 80% | 80–100% | Stop when over budget |
| Health: Workspaces open at range end, overlay files left, daemon memory and open files | 0, 0, flat | rising for 1–2 ranges | Optimize when rising for 3 ranges (a leak) |
| Range growth: new stored bytes | ≤ 3× the median of earlier ranges | — | Optimize when > 3× the median or > 2 GiB |
| Snapshot time against the range's new blocks | grows with new blocks | — | Optimize when it grows with the Store's size instead |
| Seal: WAL before seal, seal time | steady | growing | Optimize if it blocks the next range |
| Wall time per range; projected total | within budget | within 1.5× | Re-plan range size or the test cap |

**Recorded at P1 close (2026-10-18):** the baseline is ranges 2–4 on daemon
`511dceed` (`~/lfs-history/metrics/baseline.json`); the thresholds are
gates.mjs `THRESHOLDS` (RESULTS.md, P1); the wall budget is 120 s per range,
24,240 s for 202 ranges, against the current phase's mean × 202. The runner reads the budget from `~/lfs-history/budget.json` (`wallBudgetMs`, `ranges`, the decision's reason) and records `wallBudgetMs` and `wallProjectedMs` in each attempt's metrics.

- **Pass:** continue.
- **Watch:** continue, and run the probes at the next milestone.
- **Optimize:** finish the checkpoint and snapshot, then fix and recover (§6).
- **Stop:** the bug cycle, with no retry.

## 6. Fixes without redoing checkpoints

> **Incremental by default** (owner, 2026-10-18): [REBUILD.md](REBUILD.md) decides
> which recovery a fix may use: forward-only first, then an additive migration of
> the newest snapshot; a rebuild only with a written justification. It wins over
> this section where they differ.

A checkpoint's proof (git's tree plus the plan) is never redone. Its
measurements are kept under the binary that took them. Only Store bytes are
rebuilt, and only mechanically.

| Kind of fix | Example | Recovery |
| --- | --- | --- |
| Code path only | FUSE, exec, a query | Fork from the latest snapshot (§7), fast-forward to range k, rerun range k measured |
| Data already written | A bug stored something wrong | Verify each snapshot with the fixed checker; fork from the last good one |
| Store format | A fundamental data structure | Migrate the latest good snapshot, or fast-rebuild it from the plan in the new format, checking every checkpoint's oracle |

Every optimization is compared old binary vs new binary, with the scale probes on
the same milestone snapshots. Label each latency series with its binary; never
merge series across binaries.

### Diagnosis playbook

The fixer follows these steps when a range ends as optimize or stop.

**What already exists:**
- per-call SQL work: statements, VM steps, full-scan steps, sorts, auto-index
  rows, reprepares, rows (`SqlWork`, `layerfs-persistence/src/backend/sqlite/connection.rs`);
- per-mount FUSE opcode counts (`layerfs-fuse/src/request/accounting.rs`);
- in the SDK call trace: phase timing trees, 42 storage counters and 31
  construction counters per exchange.

**Wiring (P0 found it missing; P1 wired it).** The trace reply
(`layerfs-daemon/src/telemetry.rs`, `value`) carries:
- phase timing, on every call;
- `sql`, on every call: `writer` and `readers` (all reader sessions, live and
  closed, summed), each the 27 scalar `SqlWork` counters as the session group's
  totals after the exchange minus before it. Concurrent exchanges' work on the
  same sessions is included, so read it from a call that ran alone. `call`
  (#367, fix 057) has the same counters for the calling thread's own work on
  any Store session: for an in-process SDK call that is the whole call, alone
  or contended;
- `delta`, on a call that finished a Save (a Commit or a merge): the
  daemon's Save selection totals after it minus before it. `payload` has the
  whole-file and chunk selection counts, the bases refused at the depth cap
  (`depth_capped`) or the chain-work budget (`work_exceeded`) and a depth
  histogram per role (`whole_file_depths`, `chunk_depths`: slot d counts the
  objects stored at chain depth d, 0 is FULL); `metadata` has the pooled
  leaves' counts and `depths`. The runner sums them per range for §3d's Delta
  row; Init's own objects are not in them;
- `fuse`, on `status` and `unmount` of an attached Workspace (Linux): its 39
  opcode counts since attach, the frame dispositions (`served`, `inline`,
  `refused`, `terminal`), forget and store units and the largest READ and WRITE.
  An exec's own opcodes are the difference between the status before it (the
  SDK's exec makes one) and the next status;
- storage and construction counters, only on calls that commit.

The SDK trace passes them through unchanged (`sdk/js/README.md`, Call trace).

**Counters before time.** Counters are deterministic, so they show growth without
noise. Diagnose from counters first, and confirm with time.

#### D0. Where the time goes

Split each slow call's wall time before choosing a step:

```
client wall (trace ms)
 ├─ client / native queue   = ms − Σ daemon exchanges
 ├─ round trips             = daemon exchanges × per-exchange overhead
 └─ daemon exchange
     ├─ SQL                 (SqlWork)
     ├─ content / storage   (decode, construct, chain links, pack reads)
     └─ FUSE kernel ↔ daemon (opcode counts; exec only)
```

The largest share decides which step below to follow.

#### D1. Time and speed (a regression against the baseline)

1. **Confirm it.** Compare against the same operation kind, size class, binary and
   cache state. Compare p50 and p95 distributions, never single calls.
   Outside load (Docker CPU, host disk, swap) makes the range INELIGIBLE, not a
   finding.
2. **Find the phase.** Diff the phase tree against the baseline, and follow the
   phase that grew down to its leaf.
3. **Work or waiting.**
   - The leaf's counters grew too: the call does more work. Go to D2 or D5.
   - The counters are flat: the call waits (cache misses, I/O, a lock, WAL). Go to D4.
4. **Concurrency.** Re-time the call alone on a restored snapshot, as a diagnostic.
   If it is fast alone, look for Store arbitration contention or serialization
   between agents.

#### D2. Scale and complexity analysis

1. For each operation, fit latency **and** every counter against:
   - its own input (bytes, files changed, entries, files in scope);
   - tree size;
   - history depth.
2. Read the exponent of `log(counter)` against `log(variable)`:
   - about 0: independent, which depth always requires;
   - about 1: linear;
   - above 1: superlinear, always a bug.
3. Check each operation against the scaling rules (§4). For example, `stat` has
   exponent 0 on tree size and on depth; `list` may reach 1 only on its own
   directory's entries.
4. Separate depth from tree size with the controls:
   - the same probe on two milestone snapshots;
   - the squashed-history Store (same tree, no history);
   - the one-Commit-per-git-commit Store.
5. Lock the finding in with a test. Build Stores at two depths (for example 100
   and 10,000), and assert that the counter is equal at both.

#### D3. Round-trip detection

1. **Exchanges per call.** The trace has one `daemon` entry per control exchange
   (a mount makes about 3: lookup, bind, attach). Compare each method's count
   with its expected count.
2. **Exchanges that grow with input.** One exchange per file, per directory or per
   page, where one batched exchange would do, is the N+1 pattern. Fit exchanges
   against input size; any exponent above 0 is a candidate.
3. **Chatty callers.** Many same-kind calls in a row from one step (`stat` after
   `stat`) mean the player or the dsh plugin works per item. Fix it with a batch
   or paged method.
4. **Inside the daemon:**
   - statements per call that grow with input mean a query in a loop: batch it
     with `IN (...)`, a join, or a temporary table;
   - more transactions per call than needed is the same problem one level up.
5. **Across FUSE:** many requests per syscall is a kernel-cache problem (D6).

#### D4. Inefficient database work

1. **Read SqlWork per call:**
   - `fullscan_steps` > 0 on a hot path: an index is missing or unused;
   - `autoindex_rows` > 0: SQLite is building a temporary index;
   - `sorts` where order should come from an index: the index does not match the
     `ORDER BY`;
   - many VM steps per returned row: the query reads far more than it returns.
2. **Read the plan.** Run `EXPLAIN QUERY PLAN` on each statement in the slow
   phase, against a restored snapshot (see `layerfs-persistence/examples/explain_reservation.rs`).
   Look for `SCAN` where `SEARCH … USING INDEX` should be, and for
   `USE TEMP B-TREE`.
3. **History-proportional SQL.** Recursive ancestor walks, `MAX()` or `COUNT(*)`
   over history tables: VM steps rise with depth even when the plan looks right.
4. **Physical state:**
   - WAL size before the seal;
   - free pages and fragmentation;
   - page-cache misses, when the DB is larger than the cache;
   - 4 KiB pages touched per call.

   The page size stays 4 KiB. Fix with layout and caching instead: tighter rows,
   covering indexes, clustering hot rows, cache size, checkpoint cadence.
5. **Verify by counter:** full-scan steps drop to 0, and VM steps per call stop
   depending on depth.

#### D5. Algorithm

1. **Find the loop.** The counter that grew (rows read, pages touched, ancestors
   walked, chain links decoded, objects constructed) names it. Read that code
   path in the leaf phase.
2. **History-proportional patterns:**
   - walking ancestors to find a merge base or a file's last change, with no skip
     index or cached generation number;
   - rebuilding a tree from Layers instead of reading a materialized one;
   - hot files at the delta depth cap (each read decodes a long chain);
   - recomputing per call what could be cached per Commit (merge base, directory
     sizes, logical construction).
3. **Tree-proportional patterns:**
   - a diff or merge that visits unchanged subtrees instead of skipping equal
     subtree ids;
   - a mount that constructs eagerly instead of lazily.
4. **State the intended bound**, for example "merge = O(paths changed since the
   base)". Show that the counter fits it, using the depth test from D2.
5. **Profile last.** Use `perf` or a sampling profiler in the container, only after
   counters have located the phase, on a restored snapshot, as a diagnostic.

#### D6. FUSE operations (exec)

1. **Native comparison.** Run the same command on the same tree natively (an
   exported copy) and in the Workspace. Track the ratio per command type:
   `node`, `pnpm install`, test runners, tree walks.
2. **Opcodes per command.** Count LOOKUP, GETATTR, READDIR, OPEN, READ, WRITE and
   RELEASE per exec. Normalize per syscall (`strace -c` in the container, as a
   diagnostic) and per file touched.
   - Many GETATTRs per file: the attribute cache is not working.
   - The same LOOKUPs repeated across commands: entries are not retained.
3. **Cache settings.**
   - Entry and attribute TTL are 60 s (`request/reply.rs`). A one-shot Workspace
     is a new mount, so it pays every lookup cold; optimize that case.
   - Readahead and `max_read` are negotiated.
   - Writeback caching is not negotiated, so every `write(2)` is a round trip;
     `pnpm install` will show it.
   - Check READDIRPLUS. Without it, `ls -l` and tree walks pay one LOOKUP per entry.
4. **Time per opcode.** Daemon time per opcode against the counts shows whether
   a few slow requests or many fast ones dominate. A LOOKUP or GETATTR that slows
   with depth is a Store read: go to D4 or D5.
5. **Mount and first access.** On a fresh mount, time the first `readdir` of the
   root and of a deep directory. Neither may depend on tree size or depth,
   because logical construction must be lazy.
6. **Overlay to Commit.** Count overlay files after exec. `capture` must follow
   files changed, not files read.

#### Recording a finding

The fixer writes `~/lfs-history/metrics/finding-NNN.aK.json` for attempt K that
tripped the gate (`dsh/tools/history/findings.mjs` has the format and
`checkFinding`): the trigger, the diagnosis (playbook step, summary, phase,
counter and its depth exponent before and after), the fix (summary, kind,
commit, regression test), the improvement rows (measure, before, after, on the
same snapshots), the recovery and the record. The rerun's issue comment shows
it (§3). Each finding also goes to `BUGS.md` or `OPTIMIZATIONS.md` with:
- its playbook step (D0–D6);
- the counter that shows it, with its exponent before and after;
- the fix commit;
- the old-vs-new probe table on the same milestone snapshots.

A finding closed without a code change (2026-10-18, first 013.a1) has fix
kind `none` and a summary of why it is not a product finding. It records no
commit, regression test, REBUILD.md fields, improvement rows, design or cost.
It carries an evidence table instead: measure, value, binary, snapshot and
cache. Its ledger row shows `—` for the commit.

## 7. Checkpoints, snapshots and disk

### Store rules

- **Store profile:** Disposable / WAL / `synchronous=OFF` only. Durable stays
  `NOT_RUN — disabled by owner until explicit reauthorization`. Latency therefore
  excludes fsync cost.
- **SQLite page size:** stays **4 KiB** (`profile.rs`, `PRAGMA page_size=4096`).
  Do not change it for storage gains. This is an owner rule.
- **The global Store is the central storage.** Workspace overlays are separate,
  throwaway databases and are never snapshotted.

There are two kinds of checkpoint:
- **Proof checkpoint:** every range; small and permanent.
- **Snapshot:** the Store's bytes at a checkpoint, kept incrementally (below).

### Proof checkpoint (every range, never redone)

After consolidation, the runner verifies the range's oracle:
- **tracked files:** `diff_commits` between the previous checkpoint and this one
  lists exactly the tracked paths git changed in the range, and each of them has
  git's blob id and mode. The previous checkpoint was verified, so the whole tree
  matches. A full `compareTree` of all files runs at every snapshot;
- **ignored files:** `node_modules` matches the lockfile's expected state (§2,
  Ignored files); the build outputs the range's commands make exist;
- history shape, step outcomes and planned conflicts;
- the range's unit tests, against their native results (§2, Tests).

So the check grows with the range's changes, not with the tree (§4).

From P3, the checkpoint also checks `.git`: HEAD equals the checkpoint commit and
`git status` is clean (§2, `.git` in Workspaces).

The result goes to `ranges/range-NNN.md` and `RANGES.md`, with:
- the LayerFS main head Commit id and the depth;
- the binary and the plan version.

That record is the checkpoint. Git's tree and the plan never change, so a passed
proof is never rerun, even after a fix. Rebuilt Store bytes are checked against
the same oracle. The head Commit id also lets anyone read the checkpoint's files
and history in the live Store later (Inspecting a checkpoint, below).

### Snapshot: block-incremental, one shared block store

A snapshot is the Store's SQLite file at a checkpoint, kept as a manifest of
block hashes over one shared block store. Every snapshot is complete, but each
writes only the blocks the Store changed since the snapshots before it. LayerFS
content is mostly added and rarely rewritten, so a range changes its new content
plus the index pages it touched: an estimated 100–300 MiB, against a Store of
~7 GiB at the end.

```
~/lfs-history/snapshots/
  blocks/            every distinct block, once, named by its SHA-256
  range-NNN.json     manifest: the block list, and the facts below
```

**When:**
- every range in the pilot (P1);
- after it, when fast-forwarding from the last snapshot would take longer than
  taking a new one (the seal and copy times in the manifests give both);
- always right after the checkpoint of a range whose gate is optimize, before the
  fix starts, so a restore never replays the range that triggered it;
- at the milestones (ranges 4, 50, 100, 151, 201, as P0 planned them).

**How:**
1. **Quiesce** at the end of consolidation, when nothing is mounted anyway. Long-
   lived Sandboxes may stay, empty.
2. **Seal.** `Handles::seal()` checkpoints the WAL into the main file and closes
   the Store. The WAL is kept small during the range by SQLite's automatic
   checkpoints, so the seal is short.
3. **Copy the new blocks.** Read the file in blocks (4 KiB pages grouped into
   1 MiB blocks), hash each one, and write only those not in `blocks/` yet. Write
   the manifest.
4. **Reopen** and continue. The Store is closed for seconds. Seal and copy time
   are recorded separately and are never part of operation latency.

Step 3 reads the whole file (about 10 s at 20 GiB) even though it writes only the
changes. If that read shows in the snapshot gate (§5), the fallback is to take
the changed page numbers from the WAL frames before the seal, so only those pages
are read. Automatic WAL checkpoints are never turned off for this: a large WAL
slows reads and would distort the latency being measured.

**Manifest:**
- range, dsh commit and tree id, LayerFS main head Commit id, depth;
- daemon, SDK and image ids, and the plan version;
- the `store_policy` row (cutoff and depths), the schema version, and page size 4096;
- the block list, the file's size and SHA-256, and the new blocks' count and bytes;
- seal time and copy time;
- the WAL size before the seal (a health signal; see §5).

**Verification:**
- Every snapshot: the rebuilt file's SHA-256 matches the manifest.
- Each milestone: restore into a fresh volume and LayerFS home. Run the oracle
  check on main and `PRAGMA quick_check`. A snapshot that was never restored is
  not proven restorable.

### Forking from a snapshot

Forking at range k rebuilds k's file from its manifest into a fresh Docker volume
and LayerFS home, and the run continues there. Nothing is replayed: the cost is
one sequential write of the Store's size at k (about 10–20 s for 10 GiB).

After a fix at k, the forked run becomes the main line, and its snapshots add
blocks next to the old ones. So the snapshots form a tree, as Branches do. Blocks
reachable only from abandoned snapshots after k are kept while they are evidence
(the failed Store, until its diagnosis is in `BUGS.md`), then removed with
literal paths.

Two other ways to fork were rejected:

| Way | Why not |
| --- | --- |
| Extract Commits ≤ k from one full Store into a new database | Costs as much as a copy (it walks everything up to k); the new database is compact, unlike one that grew range by range, so replays would measure a cleaner Store; and after a data bug the full Store holds the bad rows |
| Open the full Store read-only, hide rows after k, write to a second database on top | O(1), but it is a new storage path in LayerFS: two databases on every read, and refs, totals and policy are updated in place. The replay would measure that path instead of the product. A possible future feature, not part of #352 |

Reading a checkpoint's files needs no fork: the live Store reads any Commit by id.

### Retention

| Kind | Kept |
| --- | --- |
| Snapshots on the main line | All of them (their blocks are shared; ~15 GiB for all 48 planned) |
| Milestones | Forever, and restored at least once (Verification) |
| Snapshots of an abandoned line | Until the failure's diagnosis is in `BUGS.md` |

Milestones serve:
- the old-vs-new binary probes (§6);
- the Live demo seed (P4);
- data-fix verification.

### Uses

| Use | How |
| --- | --- |
| Code-path fix | Fork from the latest snapshot; fast-forward later ranges with the fast-rebuild player; rerun the failing range measured |
| Data written wrong | Verify snapshots newest first with the fixed checker; fork from the newest good one |
| Store format change | Migrate the newest good snapshot, or fast-rebuild from the plan in the new format, checking every range's oracle |
| Probes | Both binaries run the same probes on forks of the same milestone snapshots |
| Live demo | Seed the dsh demo from a milestone fork and play ~3 ranges from it |

**Snapshots stay valid.** Open uses the Store's recorded policy, so later default
changes (such as the delta depths) do not invalidate a snapshot.

**Two meanings of "checkpoint":** SQLite's WAL checkpoint (merging the WAL into
the main file) is not a proof checkpoint. It runs inside the seal and
automatically during the run.

### Inspecting a checkpoint

Troubleshooting needs a checkpoint's files, history and Store figures.

**`dsh/tools/history/inspect.mjs`** (P1, for the builder and the fixer). It only
reads; it works on the live Store or a fork, never on the snapshot files in place.

| Command | Shows |
| --- | --- |
| `ls <range\|commit> [path]`, `cat …` | The tree and file contents at a range's checkpoint or any Commit, through the SDK |
| `diff <range>` | LayerFS main against the oracle, with `compareTree` |
| `stats <range>` | Bytes per table and index (SQLite `dbstat`), object counts by kind and size class, delta depth and chain length histograms, dedup and compression ratios, WAL size; read-only, on a fork |
| `trend` | The same figures from every snapshot's manifest and fork, in a row: growth with history (§4) |
| `open <range>` | Forks the snapshot and attaches it to dsh as a read-only project |

**In dsh's LayerFS page** (P4), at two levels:
1. **Files and history at a checkpoint.** Each checkpoint records main's head
   Commit id (the demo's `verify` prints it; the checkpoint journal keeps it). The
   project header gets the same 0..n number row as the demo strip; choosing k
   shows Files and History at k's Commit, from the live Store. Every checkpoint
   works, with or without a snapshot.
2. **The database at a checkpoint.** The live Store's size and tables are today's,
   not k's. For a range with a snapshot, `open` forks it and the page lists it as
   its own read-only project (`demo-dsh-history @ 120`), so its Store tiles and
   storage drawer show the database as it was at k.

### Disk

Measured on 2026-10-18:

| Fact | Value |
| --- | --- |
| Mac disk | 356 GiB free |
| Docker Desktop disk image | 64 GiB in use of a 200 GiB maximum; 41 GiB of it is old volumes Docker reports as reclaimable (the owner's to prune) |
| Tracked history | 268k unique file versions, 6.9 GiB raw; git packs them into 301 MiB |
| One `node_modules` install | 78k files, 2.3 GiB |
| `apps/desktop/.desktop-build` | 4.8k files, 0.7 GiB |
| `lib/` and `dist/` outputs | ~2k files, ~40 MiB |

Estimate at range 202 (to be replaced by the pilot's numbers):

| Part | Estimate |
| --- | --- |
| Tracked history and Commit metadata (249k Commits) | ~3 GiB (1.5–6) |
| `node_modules`: the first install and 197 incremental ones | ~2.5 GiB (1.2–3.5) |
| Build outputs every range | ~0.5 GiB |
| **Working Store** | **~7 GiB (4.5–10.5)** |
| All snapshots, block-incremental | ~15 GiB (10–25) |
| One fork at a time | ≤ 7 GiB |
| Install overlay peak (deleted after the commit) | ~3 GiB |
| **Peak extra disk** | **~33 GiB; worst case ~55 GiB** |

Docker would stay near 100 of its 200 GiB, and the Mac above 290 GiB free. The
uncertain part is LayerFS's ratio on `node_modules`: P1 starts by Initing a dsh
checkout with `node_modules` once and reading Init's statistics. If the pilot
measures more than 2× this estimate, re-plan before range 6.

What would break the estimate, and is therefore ruled out (§2, Ignored files):
- a clean install every range: ~78k deletes and 78k writes each, ~15 million
  file changes over 197 installs;
- the desktop build: its packed app changes every build and does not
  deduplicate, an estimated 50–100 GiB if built every range (§2 never builds it);
- caches that churn on every command;
- full-copy snapshots: up to ~140 GiB for 2 rolling copies, 5 milestones and a
  failed copy.

**The disk guard** runs before each range. It projects the space the run will
hold:
- the working Store after the next range;
- the snapshot blocks;
- one fork;
- the install overlay peak.

The Docker disk and the host disk are checked separately. The guard stops when
free space falls under 2× that projection. Over 80% of the budget is a watch
item and over 100% is a stop (§5).

## 8. Delta depth

**The content model:**

- CAS plus CDC, with 8–32 KiB chunks;
- whole-file objects below 128 KiB (`small_file_threshold_bytes` 131072);
- chunked files at and above it;
- delta chains per role.

**Change made with this issue (owner request):** the default depths are doubled
for new Stores.

| Role | Old | New | Where |
| --- | --- | --- | --- |
| Chunk | 4 | **8** | `layerfs-content/src/contract/policy.rs` `DEFAULT_CHUNK_DELTA_MAX_DEPTH` |
| Whole-file | 8 | **16** | `DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH` |
| Metadata | 8 | 8 (unchanged) | `layerfs-storage/src/store/policy.rs`; see below |

Why this is safe:

- Depth is a validated policy field in `0..=50`.
- Init records it in the `store_policy` row, and open uses the recorded row.
  Existing Stores and snapshots keep their old depths.
- A deeper chain is still bounded by the fixed chain-work budgets: 512 KiB
  canonical and 256 KiB encoded per chain. The selector skips a base that would
  exceed either one and stores the object without that base.
- The selector's off-by-one on cached depth walks is already fixed
  (`encoding/delta/select.rs`).

**Effective depth is smaller than the cap for large files:**

- A whole-file chain of 100 KiB files reaches its 512 KiB canonical budget at
  about 5 links.
- A chunk chain of 32 KiB chunks fits 8 links (9 × 32 KiB = 288 KiB).

Metadata stays at 8, because its own budget (8 × 8 KiB canonical) already stops
most chains before depth 8. Doubling its depth would change little.

**What the run must decide:**

1. The control arm. During the pilot, replay ranges 1–5 on a second Store created
   with the old depths. Use `StoragePolicy::new(1, 131072, 8, 4)` through the
   project Init request. Compare:
   - stored bytes per changed byte;
   - read latency at the deepest chains (`read`, `diff_files`, `blame`, exec
     reading files);
   - Commit latency and its `construct` phase.
2. Track the share of objects at the depth cap, and the share stopped by the
   budget instead (§3d), for every range.
3. Decide using AGENTS.md §3.9:
   - Keep the new depths if storage improves and read p95 stays within 1.2×.
   - Go deeper (or raise the budgets) if many objects sit at the cap and storage
     still grows faster than changed bytes.
   - Go back to the old depths if reads slow down by more than the storage gain
     justifies.

   Record the decision and its numbers in `RESULTS.md`.

## 9. Rollout

| Phase | Scope | Done when |
| --- | --- | --- |
| P0 · Plan and oracles | `plan.mjs` and `oracle.mjs` over the whole history, without LayerFS | **Done 2026-10-17:** plan and oracle for all 202 ranges; every plan passes the `replayPlan` static check |
| P1 · Pilot | First, one Init of a dsh checkout with `node_modules` (its stored-to-raw ratio). Then the first ~300 commits, fs only, one lane, a snapshot every range, plus the delta control arm. Builds `snapshot.mjs` and `inspect.mjs` | Every checkpoint matches git; baselines and gate thresholds recorded; growth per range and per install within 2× the §7 estimate, or re-planned |
| P2 · Multi-agent (ranges 5–14) | Lanes, the 5–10 agent crew, one-shot Workspaces, planned conflicts | Every planned conflict hit and resolved; identity holds |
| P3 · Sandbox share (ranges 15–29) | The 9 : 1 split, FUSE timing, `node_modules` installs, unit tests (§2, Tests) | Deduplication and FUSE gates pass |
| P4 · Recovery and demo (ranges 30–50) | Milestone snapshots, probes, one kill and resume from a fork, one fast rebuild, the Live demo window seeded from a fork; the LayerFS page's checkpoint views (§7, Inspecting a checkpoint) | Recovery proved; the demo passes check-demo in dsh |
| P5 · Full history (ranges 51–201) | All 202 ranges. Controls: squashed history, and one Commit per git commit | Latency-vs-depth and storage curves, with history cost separated from tree size and from Commit granularity |

## 10. Outputs in this folder

- `ranges/range-NNN.md`: one report per range (§3), following `ranges/TEMPLATE.md`.
- `RANGES.md`: the index, one row per range, linking its report.
- `RESULTS.md`: baselines, thresholds, the delta decision, and each phase's results.
- `BUGS.md`: each failure, its diagnosis, its fix kind and its recovery.
- `OPTIMIZATIONS.md`: each optimization with its old-vs-new probe table on the
  same snapshots.
- `fixes/NNN-aK-<slug>.md`: one record per fix (owner request, 2026-10-18): the
  old and new design, the proof, and old-vs-new performance on the same
  snapshots ([fixes/README.md](fixes/README.md) has the template). BUGS.md and
  OPTIMIZATIONS.md keep one line per finding and link the record.

## 11. Open decisions for the owner

- The Docker Desktop disk limit, if the pilot projects past 80% of it.
- Pruning Docker's 41 GiB of reclaimable old volumes (§7, Disk). Not needed for
  the estimate, but it is the owner's to do.
- Durable mode stays NOT_RUN unless reauthorized.
- Merging `dsh-history-replay` into main is for the owner, because it may carry
  core format changes.
