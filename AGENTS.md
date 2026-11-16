# AGENTS.md

> **Status:** Current general guide.

Instructions for agents working in LayerFS. Read this file, then the guide of
each layer you change: [core](core/AGENTS.md), [engine](engine/AGENTS.md),
[control](control/AGENTS.md), [API](api/AGENTS.md). A layer guide adds only
what differs in its layer. Current owner instructions and documented
supersessions govern; a closed handoff or historical receipt is not a new
assignment. Earlier wording of these guides is in the
[archived guide history](docs/research/history/agents-guides/README.md).

## Rules you must not break

| Rule | Where |
| --- | --- |
| Disposable / WAL / synchronous=OFF is the only SQLite Store profile; Durable execution is `NOT_RUN` | [Persistence profile](#active-persistence-profile--owner-direction-2026-10-07) |
| One attempted operation: no automatic retry, busy handler, refresh or replay | [§4](#4-code-build-and-docs) |
| Build tests with `--no-run` first; every test command stops at 120 s, and reaching it is a hang | [§4](#4-code-build-and-docs) |
| CI reports and never gates; verify your own change and report checks and gaps | [§4](#4-code-build-and-docs) |
| No new dependency an existing crate covers; no third-party patch, fork or vendor except the fuser Git fork | [§4](#4-code-build-and-docs) |
| Refactors delete tests that are tricky to adapt; never weaken a surviving assertion | [§4](#4-code-build-and-docs) |
| Architecture docs change in the same commit as the code | [§4](#4-code-build-and-docs) |
| Every commit records production LOC, split by core, engine, control and api | [Production LOC](#production-loc-comparison-for-every-commit) |
| A warm cache never credits a measured phase; one sample per case | [Measurements](#measurements) |
| Retired stays retired: the v0.1.6 reference, `layerfs-server`, standalone UI pages, removed platforms; fs-bench-pro is frozen | [Retired and frozen](#retired-and-frozen) |

## Layers

| Layer | Crates (root manifest) | Guide |
| --- | --- | --- |
| core | `layerfs-content`, `layerfs-history`, `layerfs-storage`, `layerfs-telemetry` | [core](core/AGENTS.md) |
| engine | `layerfs-bridge`, `layerfs-daemon`, `layerfs-fuse`, `layerfs-mount-ports`, `layerfs-nfs`, `layerfs-overlay`, `layerfs-persistence`, `layerfs-project`, `layerfs-store-service`, `layerfs-workspace` | [engine](engine/AGENTS.md) |
| control | entities `layerfs-home`, `layerfs-machine`, `layerfs-store`, `layerfs-sandbox` (with its `workspace/` module); drivers `layerfs-platform`, `layerfs-bucket`, `layerfs-sandbox-backend`; and `layerfs-control` | [control](control/AGENTS.md) |
| api | the call surface `layerfs-api`, the `layerfs` command `layerfs-cli`, the Node binding `layerfs-node`, the PyO3 module `layerfs-python`; the JavaScript and Python packages under `api/` | [API](api/AGENTS.md) |

- **Dependencies point down:** `core < engine < control < api`, including dev
  dependencies and test includes; nothing in engine depends on control. The
  control layer (`control/crates`, renamed from `connect/` by the owner
  2026-11-06, layerfs-project#101 D-6; #535 D4) holds the management code and
  the crates that reach a machine or the network.
- **[Cargo.toml](Cargo.toml) decides membership**; a directory or old test does
  not establish a built replacement.
- **Two workstreams, one set of layers.** Cluster one is the core algorithms
  (Content, Storage, History, Telemetry) with engine Persistence and Project
  Init: start with the [cluster-one handbook](docs/general/cluster_one_handbook.md)
  and the [CAS/CDC/delta handbook](docs/general/cas_cdc_deltaencoding_handbook.md).
  Cluster two is engine Workspace, FUSE, daemon and Sandbox with API execution
  and Commit integration: start with the
  [architecture guide](docs/architecture/README.md) and its
  [flows](docs/architecture/flows/README.md). C1/C2/C5 in older documents mean
  content/storage/history.
- **What establishes what.** Source and public contracts establish implemented
  behaviour. Owner requirements and proposals establish target behaviour;
  research establishes neither capability nor qualification. Preserve source
  pins, limits, failures and open prerequisites.

## Retired and frozen

- **The v0.1.6 reference** under root `crates/` was retired on 2026-10-10 (R9c)
  and is absent. It was never a dependency, source include or fallback for
  core and must not return as a second product; its last source is at local
  tag `reference-v0.1.6-final` (`d296981ac`, `crates` tree `498dd1917812`); see
  the [R9c record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R9C-COMPLETION-20261010.md).
  Its retirement is a legacy retirement in the source-size comparison, not a
  simplification.
- **`core/docs/`** (issue records, numbered papers, receipts) was removed by
  #671 and stays readable at commit `af2ad92bce`; link it there by pinned URL
  and do not restore it. Historical evidence, receipts and verdicts stay
  unchanged.
- **`layerfs-server` and the host-mediated SDK runtime** are retired, not
  extended; build no host/server adapter in the data path.
- **The LayerFS UI is the dsh LayerFS plugin** (owner, 2026-10-19), the LayerFS
  tab in DeepSeek Harness, in its own repository
  [dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs) (moved out on
  2026-10-23, #428), which pins the LayerFS commit it builds on; this
  repository holds no dsh code. Build no standalone HTML or TUI pages. The
  `dsh-layerfs-demo` bundle is disabled permanently (owner, 2026-10-21): it
  loads nothing, is not released and no guide installs it.
- **Firecracker, Microsandbox and SSH machines are removed** (owner,
  2026-11-08); see [Where calls run](#where-calls-run).
- **fs-bench-pro is frozen and left broken** (owner, 2026-10-18). Do not edit,
  build, run or repair it, and leave it out of workspace builds and checks.
  Its call sites broken by #352 F-020 (`ProjectApi` fork and commits,
  `ForkRequest`, a `SandboxCreate` without `connect_host`) stay broken; a
  product change never adapts it. Only an explicit owner instruction reopens
  it. Benchmarks, harnesses and released data live in
  [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark)
  (moved out of `benchmark/` on 2026-10-23; routing is its
  [fs-bench-pro/AGENTS.md](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/fs-bench-pro/AGENTS.md)).
  Older documents' `benchmark/` paths are that repository's paths without the
  prefix; the root harness for the retired reference is at tag
  `reference-v0.1.6-final`.
- **Old prompts and research.** Completed Stage 0–6 prompts are historical.
  The former #303 contracts and the
  [optimization investigation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/fuse-optimization-investigation.md)
  are archived research that selects no algorithm, profile or capability. Do
  not resume old loops, reopen closed rows or run old campaigns unless the
  task selects that work, and do not choose authority by issue number alone.

## Active persistence profile — owner direction 2026-10-07

- **Disposable / WAL / synchronous=OFF is the only permitted profile for a
  SQLite Store.** Durable execution is disabled indefinitely, until the owner
  explicitly allows it again; this supersedes earlier instructions to exercise
  both profiles or resume Durable. A milestone, old benchmark selection,
  default argument or passing build does not reauthorize Durable.
- Select Disposable explicitly for every new Store, application run, test,
  diagnostic and measurement; never execute Durable or fall back to it.
  Retained Durable code may compile; report its execution as
  `NOT_RUN — disabled by owner until explicit reauthorization`. Historical
  Durable receipts keep their verdicts. This policy does not claim the Durable
  API was removed.
- Disposable's COMMIT does not checkpoint (`wal_autocheckpoint=0`); its writing
  session's own thread checkpoints the WAL off the commit path (owner approval,
  2026-11-07; [SQLite checkpoints](docs/architecture/engine/store/sqlite.md#checkpoints)).
- A PostgreSQL Store has no profile; its write setting is `synchronous_commit`
  (default `off`), recorded by `store.connect`.
- Each Workspace's daemon overlay is a separate MEMORY/OFF/EXCLUSIVE database
  file.

## LayerFS mental model

- **A Workspace is a complete mutable filesystem view** over an immutable
  committed root, including `.git` and its index, ignored files,
  dependencies, symlinks, caches and output; Git ignore rules do not filter
  LayerFS state.
- **Granularity is free.** Per-tool-call orchestration is the common case and
  needs fast bootstrap; per-task works too. A Workspace can serve many
  sequential or concurrent calls, last long and Commit incrementally.
  Workspace granularity, Exec duration and Commit cadence are independent.
  Ordinary Bash has no automatic timeout, command-specific restore/install,
  implicit Commit or automatic unmount; only explicit terminal unmount closes
  local ownership.
- **Filesystem access needs no Exec.** Any process with mount visibility and
  permission, including Bash launched by an external executor, gets the same
  Workspace semantics through ordinary filesystem calls. The Sandbox or the
  external executor owns command launch, streams, exit status and
  cancellation; the filesystem daemon supervises no commands. See
  [execution ownership](engine/AGENTS.md#filesystem-access-and-execution-ownership).
- **Mount is cheap.** Initial full-root acquisition is explicit. Mount binds
  the complete root with no whole-tree scan, copy, materialization or
  dependency restore; the only file it creates is the Workspace's overlay
  database, copied from a template built before readiness and deleted at
  terminal unmount. Demand-loaded metadata and content still pay real I/O.
- **Workspaces share nothing mutable.** A Workspace's metadata, payload and
  operation records live only in its overlay file; its open-file,
  directory-position and kernel-reference state lives in daemon memory.
  Workspaces share no overlay connection, writer, page cache or memory budget;
  the daemon shares only committed immutable content and the global Store.
  One lock guards a Workspace's connection: hold it for one short atomic job,
  never across Store I/O, a whole Exec or a whole Commit.
- **Commit captures the published frontier**, which may hold several calls'
  changes however their processes were launched; never filter by Exec
  registration or completion. Retain stable input without a bulk overlay copy.
  A known install advances the base while preserving later mutations and the
  live view. UpToDate need not create a new Commit; invocation auditing is
  separate.
- **Outcomes are not inferred.** A lost reply does not remove a published
  mutation. Bash exit does not prove that descendants, descriptors, dirty
  mappings or requests are finished.
- **FUSE** preserves permissions, stable identity, coherent caches and exact
  request/open/lookup ownership; qualify both frequent fresh mounts and
  sustained same-mount calls and Commits. Kernel writeback stays off.
- **No artificial caps.** Large files, huge namespaces and mutation bursts use
  indexed or backed state, bounded windows, streaming and backpressure;
  resident containers and transport frames impose no total file, edit,
  Workspace, Commit, flow or time cap. Physical capacity, platform and format
  limits and explicit admission remain. The one resident exception is a
  Workspace's open state, which grows with open files and the files the
  kernel references.
- **Reclamation is automatic and bounded**, after last-owner release, during
  activity and idle time; a Commit's own reclamation follows its answer
  (owner approval, 2026-11-07). Terminal unmount drains requests, closes the
  overlay connection and deletes its file. Neither deletes global history.
- **History is not a checkpoint of everything.** Immutable objects support
  reuse and distribution; authority, reference closure, mutable history,
  durability and safe GC still need exact contracts. Filesystem history is not
  a process, memory, socket or stdout/stderr checkpoint.

## Where calls run

Per-call routes are in
[Where each call runs](docs/architecture/control/README.md#where-each-call-runs),
machines in [Platforms](docs/architecture/control/machines/platforms.md) and
workers in [Workers and reach](docs/architecture/control/machines/workers-and-reach.md).

- **Store calls never run in the daemon.** Every daemon opens the global Store
  directly and reads, saves and publishes in-process (owner, 2026-10-07;
  revised 2026-10-18). A sqlite Store's calls run on the Store's kernel: in
  the caller on a local `native` machine, else in that machine's worker.
  PostgreSQL Store calls and bucket (s3) I/O run in the caller's process on
  any OS (owner, 2026-11-02, #533 G2). Workspace calls (mount, Exec, Commit,
  status, unmount) go to the daemon. Project Init into an existing Store is a
  live add (2.0, #533).
- **One kernel, local filesystem.** A sqlite Store's openers share one kernel
  and a local ext4, xfs or btrfs (Linux) or apfs or hfs (macOS) filesystem
  (`kernel_guard`, #427); another kernel reaches it only through that
  machine's worker. Anywhere else sqlite Store calls fail `Unsupported`, with
  no fallback.
- **Routes come from the home's records** (machines, owner 2026-11-02, #533
  G2; `native` on macOS, 2026-11-01, #527 D5; `native` on Windows through a
  WSL2 worker, 2026-10-22). Both Store kinds work on every machine (owner,
  2026-11-03, #535 D9): a sqlite Store is on exactly one machine, and a
  PostgreSQL Store is usable from the machines it is attached to
  (`store.attach`, each checked from that machine; supersedes #535 G3) where
  the support table (`layerfs-platform` `support.rs`, `control/SUPPORT.json`)
  allows, and refused elsewhere with the table's reason.
- **Workers** listen on loopback only, behind a secret; one attempt per call,
  no retry and no replay; an uncertain outcome is reported, never resent.
- **Secrets:** a PostgreSQL password is resolved only in the caller and travels
  only in one worker request and the Sandbox's private daemon setup file.
- **Plugin-neutral** (owner, 2026-10-24): LayerFS reads `LAYERFS_HOME` and the
  trace settings only; no adapter name, integration check or code that names a
  plugin. An integration maps its own prefixed names (such as `DSH_*`).
- **Removed stays removed** (owner, 2026-11-08): Firecracker, Microsandbox and
  SSH machines are gone with their code; no setting or flag brings one back
  without a new owner decision and source change.
- **One database per Store, one writer, short holds** (owner, 2026-10-18). A
  Store's committed state lives in one database (a SQLite file, or one
  PostgreSQL database, #506): no sharding, second database, concurrent-writer
  engine or objects outside it. Writes go one at a time through SQLite's
  single-writer rule, the Linux directory lock across writer processes, the
  in-process admission queue and the publication lane (head check, merge,
  publish as one step). These locks stay; they replace `Busy` failures under
  the no-retry rule. Heavy work happens before the lane, only the atomic
  publish holds it, and reads never take it. Concurrent Commits are expected
  to be uncommon: optimize the uncontended Commit and keep contention correct
  and bounded. Work inside the lane that grows with tree size, history or
  Branch count is a defect. See the
  [Store concurrency model](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/83-store-concurrency-model.md).
- Design references: the
  [integration contract](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/06-cluster-one-integration.md),
  [direct Store access](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/352/DIRECT_STORE.md),
  [#533](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/README.md),
  [#535](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/535/README.md)
  and the [proposal](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/host-platform-connections.md).

## Read first

| Task | Read |
| --- | --- |
| Any implementation | the two cluster-one handbooks and the [architecture guide](docs/architecture/README.md): the layer page and every topic the change touches |
| Cluster two | also [Workspace](docs/architecture/engine/workspace/README.md), [Overlay](docs/architecture/engine/overlay/README.md), [Commit](docs/architecture/engine/commit/README.md), [Kernel routes](docs/architecture/engine/kernel-routes/README.md), [Daemon](docs/architecture/engine/daemon/README.md), [Bridge](docs/architecture/engine/bridge/README.md) |
| Documentation or a release | [documentation policy](docs/general/documentation-policy.md), [release policy](docs/general/release-policy.md) |
| A pull request or issue | [code review and triage policy](docs/policy/code-review.md) |
| A measurement | [measurement workflow](docs/general/agent-measurement-policy.md), [benchmark rules](docs/general/benchmark_rules.md), [report template](docs/general/benchmark_agent_report.md), the owning harness contract |

## Measurements

The [measurement workflow](docs/general/agent-measurement-policy.md) owns these
rules under the same numbers; moving them out of this file does not weaken
them. In short:

1. **A warm cache never credits a measured phase.** Each phase pays for its
   own work from an equally declared and enforced cache state; unknown or
   mismatched cache state is INCOMPLETE/INELIGIBLE, not PASS.
2. **Reuse setup, never measurement.** Use `--setup clone` for
   post-initialization cases where supported; a clone is an independent
   writable copy, not a cold claim. Never reuse a mutated sample.
3. **Run once, pinned and bounded.** One sample per case and arm; fresh
   append-only outputs, keeping failed, ineligible and unrun selections; pinned
   identities; worktree-local locks and targets; a complete performance
   command within 15 s (declared exceptions up to 25 s, independent proofs
   under 10 s), with no timeout, workload or cache change to turn a miss into
   PASS. Export `LAYERFS_CONSTRUCTION_WORKERS=1`: one construction producer per
   Commit, capture or snapshot (Namespace Init keeps its parallel profile). A
   Commit's one producer runs on its own thread and hands objects through one
   bounded queue to the calling thread, which keeps the one Save and publishes
   (owner approval, 2026-11-07; [Commit construction](docs/architecture/engine/commit/construction.md)).
   Measure speed and storage; reject about 50% speed loss for about 5% storage.

The workflow's §5 keeps the cache-credit failure these rules came from.

## 4. Code, build and docs

- **One attempted operation.** No automatic retry, busy handler,
  refresh/reprepare or failed-operation replay; readiness waits are not
  retries. Preserve exact failure, uncertainty and custody: no guessed resend,
  deletion or success.
- **No test command runs longer than 2 minutes.** Build first (`--no-run`) so
  compilation is not mistaken for a slow test, then run each selection under
  an explicit wall timeout of at most 120 s, per package when the suite cannot
  fit. Never leave a test running, background it to wait it out or wrap it in
  a repeat loop. Reaching the ceiling is a FAILED hang: diagnose it from source
  and bounded output before any rerun, and report it. Write tests that cannot
  wait forever (bounded waits; a thread's exit must not depend on another
  thread finishing without panicking). The §3 budgets stay stricter.
- **CI reports; it never gates** (owner, 2026-10-23). `.github/workflows/ci.yml`
  runs fmt, guards, Clippy and the fast tests after a push or pull request,
  `nightly.yml` the slow and FUSE mount tests, `format.yml` the Ruff check;
  [tools/ci_tests.py](tools/ci_tests.py) picks the lanes. CI runs for
  `main`: pushes to it and pull requests into it (owner, 2026-11-16), plus the
  report-only `store-ext4` Store lane for pull requests into any base,
  including `layerfs_v2_0_0`, the 2.0 release base (owner, 2026-11-16);
  `layerfs_v2_0_0` and every other branch run no other lane, stacked pull
  requests included. Nothing runs before a push and nothing is required: do not
  wait for CI or treat a green run as verification. Verify the changed scope with
  the [product checks](engine/AGENTS.md#checks-and-completion) and report
  checks and gaps. `tools/preflight.sh` stays retired; restore no equivalent
  wrapper. An empty guard scan proves no implementation.
- **Refactors delete tests rather than nurse them** (owner, 2026-10-18). A test
  tricky to adapt (it drives a removed route, internal type or retired model,
  or needs scaffolding the new design lacks) is deleted, not patched; keep or
  move a test only when it checks surviving behaviour cheaply. Where a deleted
  test guarded a fix or contract that still holds, cover it on the new code.
  List deleted tests in the commit message and the pull request or issue.
  Never weaken a surviving assertion to make it pass.
- **Dependencies.** No new dependency when an existing crate covers the need.
  Never patch, fork, vendor, replace or edit third-party code or registry
  packages; the one exception is the owner's
  [fuser Git fork](engine/AGENTS.md#fuser-git-source) at its pinned revision.
  Use locked builds; report an incompatible required dependency with evidence.
- **Build the current product.** Preserve unrelated work. Build the current
  implementation, not legacy aliases, source includes or error-driven
  substitutes; keep platform cfgs. Keep the
  [repository ARM64 build inputs](.cargo/config.toml): explicit RUSTFLAGS
  repeat the profile.
- **Claim only real guarantees.** Persistence profiles are scoped to their
  Store. No `fsync`/`fdatasync`/`sync_data`/`sync_all` on disposable Workspace
  backing and no space reserved ahead of use on it. Memory hints are not
  evidence of resident bounds.
- **Independent E2E runs** own their writable Stores, SDK homes, containers and
  traces; keep a test's intentional multi-daemon shared Store.
- **Source shape.** New core, engine, control and API production files obey
  the 999-line and 200-line `lib.rs`/`mod.rs` ceilings in the
  [engine guide](engine/AGENTS.md#production-file-size-and-responsibility).
- **Docs.** Update affected architecture and API documentation in the same
  commit, as the [architecture doc update policy](docs/policy/architecture-doc-update.md)
  sets out; keep proposal, research, implemented and measured claims distinct.
  Format changed Python tools with Ruff (`ruff.toml` names the version, scope
  and command).
- **Local tools are optional.** The
  [parallel worktree launcher](docs/general/parallel-worktrees.md) and
  [cleanup script](docs/general/cleanup.md) are local tools; disk thresholds,
  check cadence and cleanup authorization belong in the user's own agent
  instructions, not here.

### Production LOC comparison for every commit

Every commit records **production lines of code before, after and the signed
delta**. It is a source-size comparison, not a performance claim.

- **What counts:** nonblank, non-comment lines of first-party product
  implementation, including shipped runtime SQL and other shipped
  implementation outside Rust `src/`; imports, declarations and forwarding
  count, and a line with code and a comment counts once. Tests (including
  inline test modules and test-only branches), fixtures, mocks, examples,
  benchmark harnesses, tools, docs, manifests, lockfiles, third-party code and
  generated artifacts do not. Never use raw line totals or Git statistics.
- **What is compared:** the first parent against the committed tree (an empty
  tree for an initial commit; the first parent for a merge, declared). Prepare
  it from the parent and the final staged tree before committing, confirm the
  commit matches, and exclude unstaged and untracked work. Recompute after an
  amendment, rebase or any change to the staged source.
- **Same method both sides:** the same counter and version, scope, exclusions
  and handling of inline test code; record the command. Review classification
  when files move or new product paths appear; never drop code silently.
- **Honest migration:** while old and new implementations coexist, report each
  total and the combined one; include application-adapter code. Label
  relocation, duplication and retirement; a scope change or the reference's
  deletion is not an algorithmic simplification. Report the reference as
  `reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)`,
  never as a bare 0.
- **Report:** in the commit message and handoff, the headline
  `Production LOC: <before> -> <after> (delta <signed difference>)`, then one
  line for each of `core`, `engine`, `control` and `api` in that order, even
  when unchanged, so they sum to the headline; after a changed scope, each
  crate or package whose count moved with its delta. Then the reference line
  and the method. Scopes and crates are the ones
  `tools/production_loc.py --detail` prints. A move between scopes or crates
  is labelled relocation. Several commits give one block each, plus one for
  the range. A test or docs-only commit still reports the unchanged total and
  delta 0.

  ```text
  Production LOC: 163,645 -> 163,685 (delta +40)
    core     39,787 -> 39,795 (+8)   layerfs-history +8
    engine   74,997 -> 75,029 (+32)  layerfs-persistence +7, layerfs-store-service +25
    control  42,777 -> 42,777 (0)
    api       6,084 ->  6,084 (0)
    reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)
    Method: tools/production_loc.py --detail on parent 99ffd262a3 and the staged tree
  ```

Growth is allowed when the product change justifies it. Never remove required
validation, compress code into dense lines or move implementation out of scope
to improve the number, and never estimate it.

### File and folder layout listings

A plan, handoff, review or reply that shows a source layout uses one vertical
listing per crate in a fenced `diff` block:

```diff
  construction/                       [793]
    mod.rs                               4
    records.rs                         247
+   driver.rs                            —   new
    captured/                         [502]  (existing constructor, reused)
      owner.rs                         256
      scan.rs                          246
-   legacy.rs                           40   (removed: driver.rs replaces it)
+   scratch/                             —   cond.
+     records.rs                         —
```

- Column 1 is the marker: `+` a path that does not exist yet, `-` one this
  change removes, a space one that stays (unchanged, edited or moved in; say
  where from). A listing of the current tree has a space on every line.
- One path per line, two spaces per directory level; directories end in `/`
  and list their files before their subdirectories; no joined siblings or
  brace groups.
- Each existing file shows its production LOC from
  `tools/production_loc.py --files` at a stated commit, right-aligned; each
  directory shows the bracketed sum of the files listed beneath it, removed
  files included (say so when only part is shown). A file that does not exist
  shows `—` with `new` or `cond.` (added only if existing code cannot cover
  the need); never an estimate.
- Annotations are short, in parentheses, after the number. State the commit,
  counter and included crates above the listing.

A listing is a snapshot: it does not make a planned name a requirement, and
the source and `Cargo.toml` still decide what is built.
