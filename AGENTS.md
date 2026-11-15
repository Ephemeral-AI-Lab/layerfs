# AGENTS.md

> **Status:** Current general guide.

Repository-wide instructions for LayerFS. Also read the applicable [core](core/AGENTS.md), [engine](engine/AGENTS.md),
[control](control/AGENTS.md) and [API](api/AGENTS.md) guides for implementation in those layers. Current owner instructions and documented
supersessions govern; a closed handoff or historical receipt is not a new assignment.
Each rule has one home: a layer guide adds only what differs in its layer and
links here for the rest. How the guides reached their current wording is in
the [archived guide history](docs/research/history/agents-guides/README.md).

## Rules you must not break

Each line is a summary; the linked section is the rule.

| Rule | Where |
| --- | --- |
| Disposable / WAL / synchronous=OFF is the only SQLite Store profile; Durable execution is `NOT_RUN` | [Active persistence profile](#active-persistence-profile--owner-direction-2026-10-07) |
| One attempted operation: no automatic retry, busy handler, refresh or replay | [§4](#4-code-build-and-docs) |
| Build tests with `--no-run` first; every test command stops at 120 s, and reaching it is a hang | [§4](#4-code-build-and-docs) |
| CI reports and never gates; verify your own change and report checks and gaps | [§4](#4-code-build-and-docs) |
| No new dependency an existing crate covers; no third-party patch, fork or vendor except the fuser Git fork | [§4](#4-code-build-and-docs) |
| Refactors delete tests that are tricky to adapt; never weaken a surviving assertion | [§4](#4-code-build-and-docs) |
| Architecture docs change in the same commit as the code | [§4](#4-code-build-and-docs) |
| Every commit records production LOC, split by core, engine, control and api | [Production LOC](#production-loc-comparison-for-every-commit) |
| A warm cache never credits a measured phase; one sample per case | [§1](#1-a-warm-cache-must-never-credit-a-measured-phase), [§3](#3-running-a-measurement) |
| fs-bench-pro is frozen and left broken; do not edit, build, run or repair it | [Read the current contract](#read-the-current-contract-for-the-task) |
| Retired stays retired: the v0.1.6 reference, `layerfs-server`, standalone HTML/TUI pages, the `dsh-layerfs-demo` bundle | [Layers and workstreams](#layers-and-workstreams), [Where calls run](#where-calls-run) |

## Layers and workstreams

| Layer | Crates (root manifest) | Guide |
| --- | --- | --- |
| core | `layerfs-content`, `layerfs-history`, `layerfs-storage`, `layerfs-telemetry` | [core](core/AGENTS.md) |
| engine | `layerfs-bridge`, `layerfs-daemon`, `layerfs-fuse`, `layerfs-mount-ports`, `layerfs-nfs`, `layerfs-overlay`, `layerfs-persistence`, `layerfs-project`, `layerfs-store-service`, `layerfs-workspace` | [engine](engine/AGENTS.md) |
| control | entities `layerfs-home`, `layerfs-machine`, `layerfs-store`, `layerfs-sandbox` (with its `workspace/` module); drivers `layerfs-platform`, `layerfs-bucket`, `layerfs-sandbox-backend`; and `layerfs-control` | [control](control/AGENTS.md) |
| api | the call surface `layerfs-api`, the `layerfs` command `layerfs-cli`, the Node binding `layerfs-node`, the PyO3 module `layerfs-python`; plus the JavaScript and Python packages under `api/` | [API](api/AGENTS.md) |

The dependency direction is `core < engine < control < api`, including dev
dependencies and test includes; nothing in engine depends on control.
Workstream names do not change this layering. The control layer
(`control/crates`, renamed from `connect/` by the owner 2026-11-06,
layerfs-project#101 D-6; #535 D4) holds the management code and the crates
that reach a machine or the network. All product layers follow the
[production source rules](engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](engine/AGENTS.md#checks-and-completion).

**Implement both workstreams through the core, engine, control and API layers.** The v0.1.6
reference implementation that lived under root `crates/` was retired on
2026-10-10 (R9c) and is absent at HEAD. It was never a dependency, source
include or fallback for core, and must not return as a second product. Its last
source is at local tag `reference-v0.1.6-final` (`d296981ac`, `crates` tree `498dd1917812`);
see the [R9c record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R9C-COMPLETION-20261010.md).
Historical evidence, receipts and verdicts are retained unchanged in Git
history: `core/docs/` (issue records, numbered papers, receipts) was removed
by #671 and stays readable at commit `af2ad92bce`. Link it there by pinned
URL; do not restore it to the tree. The
retirement is a legacy retirement in the source-size comparison, not a
simplification.

| Workstream | Current scope | Entry point |
| --- | --- | --- |
| Cluster one | Core Content/Storage/History/Telemetry algorithms; Engine Persistence and Project Init | [Cluster-one handbook](docs/general/cluster_one_handbook.md), [CAS/CDC/delta handbook](docs/general/cas_cdc_deltaencoding_handbook.md) |
| Cluster two | Engine Workspace/FUSE/daemon/Sandbox; API execution and Commit/runtime integration | [Architecture guide](docs/architecture/README.md): engine and control topics and the [flows](docs/architecture/flows/README.md) |
| Legacy reference | v0.1.6, retired from `main` by R9c; read at the recovery tag only | [R9c record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/R9C-COMPLETION-20261010.md) |

**The LayerFS UI is the dsh LayerFS plugin** (owner, 2026-10-19): the LayerFS tab in
DeepSeek Harness. It lives in its own repository,
[dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs) (moved out of `dsh/`
on 2026-10-23, #428), which pins the LayerFS commit it builds on; this repository
holds no dsh code.
Do not build or revive standalone HTML or TUI pages. **The `dsh-layerfs-demo`
bundle (the LayerFS mock model and its demo workspace) is disabled permanently**
(owner, 2026-10-21): it loads nothing, is not released and no guide installs it.

Cluster names are workstreams. C1/C2/C5 in older component documents mean
content/storage/history. Check [Cargo.toml](Cargo.toml) for active
members: a directory or old test does not establish a built replacement.
Source and public contracts establish implemented behavior. Owner requirements
and proposals establish target behavior; research establishes neither capability
nor qualification. Preserve source pins, limits, failures and open prerequisites.

## Active persistence profile — owner direction 2026-10-07

**Disposable / WAL / synchronous=OFF is the only permitted global Store
profile. Durable execution is disabled indefinitely, until the owner explicitly
allows it again.** This supersedes earlier instructions to exercise both profiles
or to resume Durable after Disposable development. A milestone, old benchmark
selection, default argument or passing build does not reauthorize Durable.

Select Disposable explicitly for every new Store, application run, test,
diagnostic and measurement. Do not execute Durable or fall back to it. Retained
Durable implementation may compile, but report its execution as
`NOT_RUN — disabled by owner until explicit reauthorization`. Preserve historical
Durable receipts and their original verdicts. This execution policy does not
claim that the Durable API has been removed. Each Workspace's daemon overlay
remains a separate MEMORY/OFF/EXCLUSIVE database file.
Disposable's COMMIT does not checkpoint (`wal_autocheckpoint=0`); its writing
session's own thread checkpoints the WAL off the commit path (owner approval,
2026-11-07; [SQLite checkpoints](docs/architecture/engine/store/sqlite.md#checkpoints)).

## LayerFS mental model

- A Workspace is a complete mutable filesystem view over an immutable committed
  root. Include `.git` and its index, ignored files, dependencies, symlinks,
  caches and output; Git ignore rules do not filter LayerFS state.
- Support both per-tool-call and per-task orchestration. Per-tool-call is the
  expected common case and needs fast bootstrap. A Workspace can serve multiple
  sequential/concurrent calls, last a long time and Commit incrementally.
- Workspace granularity, Exec duration and Commit cadence are independent.
  Commands may be short or long-lived in either mode. Ordinary Bash has no
  automatic runtime timeout, command-specific restore/install, implicit Commit
  or automatic unmount. Only explicit terminal unmount closes local ownership.
- Filesystem access does not require `workspace_api.exec` or an Exec registration.
  Any process with mount visibility and permission, including Bash launched by an
  external executor, uses the same Workspace semantics through ordinary filesystem
  calls. Sandbox or the external executor owns command launch, streams, exit
  status and explicit cancellation. The filesystem daemon supervises no commands;
  optional `WorkspaceApi.exec` only delegates ordinary Sandbox execution.
  Keep this boundary explicit in implementation
  and proofs; see [execution ownership](engine/AGENTS.md#filesystem-access-and-execution-ownership).
- Initial full-root acquisition is explicit. Repeated mount binds the complete
  root without a whole-tree scan/copy/materialization or dependency restoration.
  The only file it creates is the Workspace's own overlay database, copied from
  the template. Demand-loaded metadata/content still pay real I/O.
- Each mounted Workspace has its own local overlay SQLite file, copied at mount
  from a template built before readiness and deleted at terminal unmount. Its
  metadata, payload and operation records live only in that file; its open-file,
  directory-position and kernel-reference state lives in daemon memory.
  Workspaces share no overlay connection, writer, page cache or memory budget;
  the daemon shares only committed immutable content and the global Store. One
  lock guards a Workspace's connection: hold it for one short atomic job, never
  across Store I/O, a whole Exec or a whole Commit.
- Commit captures the shared locally published Workspace frontier, potentially
  including several calls' changes regardless of how their processes were
  launched. Do not filter captured changes by Exec registration or completion.
  Retain stable existing input without a bulk overlay copy. Construct/save/publish
  it; known install advances the base while preserving later active mutations and
  the effective live view.
- Namespace Init and captured Workspace construction reuse the same Content,
  Storage and Persistence libraries. Project adapts native files and metadata;
  Workspace adapts the captured immutable base plus Overlay changes. Keep
  canonical construction in Content, object encoding/packing in Storage and
  database writes in Persistence. Reuse these ports when completing daemon
  construction; do not add a second encoder or route it through host Init. See
  [shared construction boundaries](engine/AGENTS.md#shared-construction-and-completion-boundaries)
  for implemented entrypoints and the remaining S10 namespace integration.
- `Save::finish()` completes saved objects while the Store remains open;
  History publication follows. `SqliteHandles::seal()` consumes the sole Store
  session for host/offline handoff, checkpoints and closes it. Its macOS unused
  extent cleanup belongs to that lifecycle. A live daemon keeps the shared
  Store open across Saves and Commits; it does not seal it per file or Commit.
- A lost reply does not remove an already published mutation. Bash exit is not
  proof that descendants, descriptors, dirty mappings or requests are finished.
  UpToDate need not create a new Commit; invocation auditing is separate.
- FUSE must preserve permissions, stable identity, coherent caches and exact
  request/open/lookup ownership. Qualify both frequent fresh mounts and sustained
  same-mount calls/Commits. Kernel writeback remains off under the target contract.
- Large files, huge namespaces and mutation bursts require indexed/backed state,
  bounded windows, streaming and backpressure. Resident containers/transport
  frames must not impose artificial total file/edit/Workspace/Commit/flow/time
  caps. Physical capacity, platform/format limits and explicit admission remain.
  The one resident exception is a Workspace's open state, which grows with open
  files and with the files the kernel currently references.
- Automatic bounded local reclamation follows last-owner release, including
  during live activity and idle periods. A Commit's own reclamation (its
  captured generation's retired rows) follows its answer (owner approval,
  2026-11-07). Terminal unmount drains requests, closes the Workspace's
  overlay connection and deletes its file. Neither deletes global history.

## Where calls run

The route of every call, with its source, is in
[Where each call runs](docs/architecture/control/README.md#where-each-call-runs);
machines and platforms are in [Platforms](docs/architecture/control/machines/platforms.md)
and workers in [Workers and reach](docs/architecture/control/machines/workers-and-reach.md).
The rules behind them:

- **Store calls never run in the daemon.** Every daemon opens the global Store
  directly and reads, saves and publishes in-process (owner, 2026-10-07;
  revised 2026-10-18). A sqlite Store's calls run in a process on the Store's
  kernel: the caller on a local `native` machine, else that machine's worker.
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
  (`store.attach`) where the support table allows.
- **Workers**: loopback only, behind a secret; one attempt per call, no retry,
  no replay of a lost reply; an uncertain outcome is reported, never resent.
- **Secrets**: a PostgreSQL password is resolved only in the caller and travels
  only in one worker request and the Sandbox's private daemon setup file.
- **Plugin-neutral** (owner, 2026-10-24): LayerFS reads `LAYERFS_HOME` and the
  trace settings only; no adapter name, integration check or code that names
  a plugin. An integration maps its own prefixed names (such as `DSH_*`).
- **Removed stays removed** (owner, 2026-11-08): Firecracker, Microsandbox and
  SSH machines are gone with their code; no setting or flag brings one back
  without a new owner decision and source change. Do not revive
  `layerfs-server` or a host/server adapter in the data path.
- Design references: the
  [integration contract](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/06-cluster-one-integration.md),
  [direct Store access](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/352/DIRECT_STORE.md),
  [#533](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/README.md), [#535](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/535/README.md)
  and the [proposal](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/host-platform-connections.md).
  The route decisions as first written, including the dsh Live workers on
  macOS and Windows, are in the
  [archived guide history](docs/research/history/agents-guides/README.md#root-guide-store-routes-as-first-written).
- **One global database, one writer, short holds (owner decision 2026-10-18).**
  All of a Store's committed state lives in one database: one SQLite file,
  or one PostgreSQL database for a PostgreSQL Store (#506); no sharding,
  second database, concurrent-writer engine or objects outside it. Writes go one at a
  time: SQLite's single-writer rule, the Linux directory lock across writer
  processes, the in-process admission queue and the publication lane (head
  check, merge, publish as one step). These locks stay; they replace `Busy`
  failures under the no-retry rule. Performance comes from short holds: heavy
  work happens before the lane, only the atomic publish step holds it, reads
  never take it. Agents' tool calls are separated by model latency, so
  concurrent Commits are expected to be uncommon; optimize the uncontended
  Commit and keep contention correct and bounded. Work that grows with tree
  size, history or Branch count inside the lane is a defect. See the
  [Store concurrency model](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/83-store-concurrency-model.md)
  and the [global SQLite Store](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/84-global-sqlite-store.md)
  design reference.
  Immutable objects support reuse/distribution; authority, reference closure,
  mutable history, durability and safe GC still require exact contracts.
  Filesystem history is not a process, memory, socket or stdout/stderr
  checkpoint.

## Read the current contract for the task

| Task | Read first |
| --- | --- |
| Any implementation or integration | the [cluster-one handbook](docs/general/cluster_one_handbook.md), the [CAS/CDC/delta handbook](docs/general/cas_cdc_deltaencoding_handbook.md) and the [architecture guide](docs/architecture/README.md): the layer page and every topic the change touches |
| Cluster two | also [Workspace](docs/architecture/engine/workspace/README.md), [Overlay](docs/architecture/engine/overlay/README.md), [Commit](docs/architecture/engine/commit/README.md), [Kernel routes](docs/architecture/engine/kernel-routes/README.md), [Daemon](docs/architecture/engine/daemon/README.md) and [Bridge](docs/architecture/engine/bridge/README.md) |
| Documentation or a release | the [documentation policy](docs/general/documentation-policy.md) and [release policy](docs/general/release-policy.md) |
| Opening, reviewing or triaging a pull request or issue | the [code review and triage policy](docs/policy/code-review.md) |
| A measurement | the [agent measurement workflow](docs/general/agent-measurement-policy.md), [benchmark rules](docs/general/benchmark_rules.md), [report template](docs/general/benchmark_agent_report.md) and the owning harness/family contract |
| Benchmarks | the [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark) repository (moved out of `benchmark/` on 2026-10-23); routing is its [fs-bench-pro/AGENTS.md](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/fs-bench-pro/AGENTS.md) |

The former #303 contracts and the
[optimization investigation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/fuse-optimization-investigation.md)
are archived research; they do not silently select new algorithms, profiles or capabilities.
Completed/superseded Stage 0–6 prompts are historical references, not default
routing. Preserve canonical compatibility and evidence, but do not resume old
loops, reopen closed rows or run old campaigns unless the current task selects
that work. Do not choose authority by issue number alone.

**fs-bench-pro is frozen and left broken (owner, 2026-10-18).** Do not edit,
build, run or repair it, and leave it out of workspace builds and checks. Its
call sites broken by #352 F-020 (`ProjectApi` fork and commits,
`ForkRequest`, a `SandboxCreate` without `connect_host`; listed in the F-020
fix record) stay broken; a product change never adapts it. Only an
explicit owner instruction reopens it.
The root `benchmark/` harness that built and measured the retired reference was
removed from HEAD; read it at tag `reference-v0.1.6-final`. Released
fs-bench-pro results (v0.1.4–v0.1.6) are in layerfs-benchmark's
[data/fs-bench-pro](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/data/fs-bench-pro/README.md).
Older documents here still link to `benchmark/` paths; read those files in
layerfs-benchmark at the same path without the `benchmark/` prefix.

## 1. A warm cache must never credit a measured phase

Each measured phase pays for its own work from an equally declared/enforced
cache state. No setup/earlier-phase warmth, pre-touching, own-write cache credit,
lifetime-peak substitution or file-sized backing/page-cache growth disguised by a
bounded heap. Unknown or mismatched cache state is INCOMPLETE/INELIGIBLE, not PASS.
Preserve failures and historical verdicts. Full rules and the retained example:
[measurement workflow §1](docs/general/agent-measurement-policy.md#1-a-warm-cache-must-never-credit-a-measured-phase).

## 2. Reuse setup (`--setup clone`); never reuse measurement

Reuse closed prepared inputs, sealed builds/images and qualifying proof receipts
through the declared mechanisms. Use `--setup clone` for post-initialization
cases where supported; fresh is for initialization/fresh-output cases.
A clone is an independent writable byte copy, not a cold claim. Never reuse a
mutated sample or remove timed product work through setup. Record all reuse and
enforce residency for a cold claim. See
[measurement workflow §2](docs/general/agent-measurement-policy.md#2-reuse-setup---setup-clone-never-reuse-measurement).

## 3. Running a measurement

The [measurement workflow](docs/general/agent-measurement-policy.md#3-running-a-measurement)
owns detailed procedures, scoped SDK rules and budget exceptions. Its requirements
remain binding; moving them out of this file does not weaken them.

1. One sample per case/arm, no best-of or repeated unchanged treatment.
   Diagnose retained receipts or labeled count-driven causes. The separately
   scoped regression-screen exception is not treatment resampling.
2. Fresh append-only outputs; retain failed, ineligible and unrun selections.
3. Pin source/product/compilation/dependency/binary/image/harness/workload/cache
   identities. No dirty seal compared as a sealed arm.
4. Exploratory timing is diagnostic; verification is separate. At final identity,
   run covering proof once. Reuse unaffected earlier-family evidence; rerun only
   the necessary affected checkpoint, without relabeling old receipts.
5. Use worktree-local measurement locks and Cargo targets; no same-worktree
   overlap. Declare cross-worktree interference; never interrupt another owner.
6. Read the report template before each invocation; record exact metrics, limits,
   arithmetic, commands and all outcomes in the campaign-owned ledger/receipts.
7. Default complete performance command ≤15 s; declared exceptions up to 25 s;
   default independent proof <10 s. Frozen family/profile limits retain their
   declared scope. No timeout/workload/cache changes to turn a miss into PASS.
8. Commit/capture/snapshot have one construction producer in normal wiring and
   each measured operation; export `LAYERFS_CONSTRUCTION_WORKERS=1`.
   Namespace Init alone retains its supported parallel construction profile.
   A Commit's one producer runs on its own thread and hands objects through
   one bounded queue to the calling thread, which keeps the one Save and
   publishes (owner approval, 2026-11-07); see
   [Commit construction](docs/architecture/engine/commit/construction.md).
   No other helper/lane to pass a gate; FUSE dispatch is a separate concern.
9. Measure both speed and storage. Reject about 50% speed loss for about 5%
   storage benefit; report raw deltas and preserve frozen-gate failures.

## 4. Code, build and docs

- [The parallel worktree launcher](docs/general/parallel-worktrees.md) and
  [the cleanup script](docs/general/cleanup.md) are optional local tools. Disk
  thresholds, check cadence and standing cleanup authorization depend on the
  machine; they belong in the user's own agent instructions, not in this file.
- Independent E2E runs own their writable Stores, SDK homes, containers and
  traces. Preserve a test's intentional multi-daemon shared Store.
- Preserve unrelated work. Build the current core implementation, not legacy
  aliases, source includes or error-driven substitutes. Keep platform cfgs.
- No new dependency if an existing crate supplies the capability. Never patch,
  fork, vendor, replace or edit third-party code/registry packages. The explicit
  exception is the owner's selected [fuser Git fork](engine/AGENTS.md#fuser-git-source):
  use its pinned revision, not a local vendor, patch or registry substitute.
  Use locked builds and report an incompatible required dependency with evidence.
- One attempted operation; no automatic retry/busy handler, refresh/reprepare
  or failed-operation replay. Readiness waits are not retries. Preserve exact
  failure/uncertainty and custody; no guessed resend, deletion or success.
- **CI reports; it never gates (owner, 2026-10-23).** GitHub Actions runs a
  fast lane after each push or pull request (`.github/workflows/ci.yml`: fmt,
  guards, Clippy and the fast tests) and a nightly lane (`nightly.yml`: slow and
  FUSE mount tests); `format.yml` checks the Ruff formatting below.
  Neither `ci.yml` nor `format.yml` runs for a pull request into
  `layerfs_v2_0_0` (owner, 2026-11-07).
  [tools/ci_tests.py](tools/ci_tests.py) decides which tests go in
  which lane. None is a required check, and nothing runs before a push. Do
  not wait for CI, and do not treat a green run as
  verification of your change: verify the changed scope yourself with
  [product checks](engine/AGENTS.md#checks-and-completion) and report checks/gaps.
  There is still no pre-push gate: `tools/preflight.sh` stays retired; do not
  restore it or an equivalent wrapper. An empty guard scan proves no
  implementation.
- **Migrations and refactors delete tests rather than nurse them (owner,
  2026-10-18).** When code is moved, replaced or refactored, a test that is
  tricky to adapt (it drives a removed route, internal type or retired model,
  or needs scaffolding the new design does not have) is deleted, not patched.
  Keep or move a test only when it still checks surviving behaviour cheaply;
  where a deleted test guarded a fix or contract that still holds, cover that
  behaviour with a test on the new code. List deleted tests in the commit
  message and the fix record (the pull request or issue; `core/docs/issues/`
  no longer holds records). Never weaken a surviving test's assertion to
  make it pass.
- **No test command runs longer than 2 minutes.** Give every test invocation an
  explicit wall timeout of at most 120 s and stop it when it expires; never
  leave one running, background it to wait it out, or wrap it in a repeat loop.
  Build first (`--no-run`) so compilation is not mistaken for a slow test, and
  select by package/test when the whole suite cannot fit. A test that reaches
  the ceiling has FAILED as a hang: diagnose it from source and bounded output
  before any rerun, and report it. Write tests so they cannot wait forever:
  bounded waits, and a spawned thread's exit must not depend on another thread
  finishing without panicking. This is a ceiling, not a target; the scoped
  measurement and proof budgets in §3 stay stricter.
- Preserve [repository ARM64 build inputs](.cargo/config.toml); explicit
  RUSTFLAGS must repeat the profile. Details are in the core guide.
- Persistence profiles are scoped to the store. Claim only their guarantees;
  no `fsync`/`fdatasync`/`sync_data`/`sync_all` on disposable Workspace backing,
  and no space is reserved ahead of use on it.
  Memory hints are hints, not evidence of resident bounds.
- New core/engine/control/API production files obey the 999-line ceiling and 200-line
  declaration/delegation ceiling for `lib.rs`/`mod.rs`. Product-only source,
  external tests and guard coverage are detailed in the layer guides.
- Format changed Python tools with Ruff; `ruff.toml` names the pinned version,
  scope and command. `format.yml` reports formatting drift after a push;
  nothing runs before one.
- Update affected source architecture/API documentation with implementation
  changes, in the same commit, as the
  [architecture doc update policy](docs/policy/architecture-doc-update.md)
  sets out. Keep proposal, research, implemented and measured claims distinct.

### Production LOC comparison for every commit

Every Git commit must record **production source lines of code before, after,
and the signed delta**. Test, documentation and tooling changes do not contribute
to this number. This is a source-size comparison, not a performance claim.

- **Count production code only.** Count nonblank, non-comment source lines in
  first-party LayerFS product implementation, including required runtime SQL or
  other shipped implementation outside Rust src/ directories. Imports, declarations
  and forwarding code count. Exclude tests (including legacy inline test modules
  and test-only branches), fixtures, mocks, examples, benchmark harnesses, development
  tools, docs, manifests/lockfiles, third-party code and generated build artifacts.
  A line with both code and a comment counts once. Do not substitute raw file-line
  totals or Git insertion/deletion statistics for production LOC.
- **Compare the exact commit snapshots.** Before is the commit's first parent;
  after is the committed tree. Prepare the comparison from the parent and final
  staged tree before committing, then confirm the resulting commit matches it.
  Exclude unstaged/untracked work. Use an empty tree for an initial commit. For
  merges declare the first-parent comparison; recompute after amendments/rebases
  or any change to the staged source.
- **Keep counting reproducible.** Use the same counter/version, source scope,
  exclusions and handling of inline test code for both snapshots. Record the
  command/method with the comparison. Review source classification when files
  move or new product paths appear; do not silently drop code from the count.
  A counter that includes legacy inline tests does not satisfy this rule.
- **Report migration honestly.** While old and replacement implementations
  coexist, report their production totals separately as well as the combined
  total. Include application-adapter production code when introduced. Label
  relocation, duplication and legacy retirement; do not call a scope change or
  deletion of the reference an algorithmic simplification. Since R9c the
  reference scope is absent: report it as `reference absent (retired R9c; last
  counted 65,417 at tree 498dd1917812)`, never as a bare 0.
- **Put the result in the commit message and handoff.** Use
  `Production LOC: <before> -> <after> (delta <signed difference>)`, with scope
  and counting method, plus migration subtotals when applicable. For multiple
  commits, give a comparison for each. A test/docs-only commit still reports
  the unchanged production total and delta 0; it does not report a fictitious
  zero-sized product or add test/documentation LOC to the headline.
- **Split the result by scope.** Under the headline, give one line for each of
  `core`, `engine`, `control` and `api`, in that order:
  `<scope> <before> -> <after> (<signed delta>)`. List all four even when a
  scope is unchanged, so the four lines visibly sum to the headline. After a
  changed scope, name each crate or package whose count moved, with its
  signed delta; omit unchanged crates. Then give the `reference absent` line
  and the method. Scopes and crate assignment are the ones
  `tools/production_loc.py --detail` prints; do not regroup them by hand.
  When code moves between scopes or crates, label the matching minus and plus
  as a relocation, not as growth or simplification. A pull request or handoff
  with several commits gives this block for each commit, and one block for
  the whole range against its base. For example:

  ```text
  Production LOC: 163,645 -> 163,685 (delta +40)
    core     39,787 -> 39,795 (+8)   layerfs-history +8
    engine   74,997 -> 75,029 (+32)  layerfs-persistence +7, layerfs-store-service +25
    control  42,777 -> 42,777 (0)
    api       6,084 ->  6,084 (0)
    reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)
    Method: tools/production_loc.py --detail on parent 99ffd262a3 and the staged tree
  ```

LOC growth is allowed when justified by the product change; this rule does not
require every commit to shrink. Never remove required validation, compress code
into dense lines, or move implementation outside the declared scope to improve
the number. Complete the comparison before committing; do not invent estimates.

### File and folder layout listings

When a plan, handoff, review or reply shows a source layout, current or
planned, use one vertical listing per crate in a fenced `diff` block, so the
renderer highlights what changes:

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

- **Column 1 is the change marker, then one space, then the path.** `+` marks
  a file or directory that does not exist yet (green); `-` marks one this
  change removes (red); a space marks one that stays, whether unchanged,
  edited in place or moved in (say where from in its annotation). A listing of
  the current tree alone has a space on every line.
- **One path per line.** Indent two spaces per directory level. Directories end
  in `/`; list a directory's own files before its subdirectories. Do not join
  siblings on one line or use brace groups such as `{mod,cursor}.rs`.
- **Every existing file carries its production LOC**, right-aligned in one
  column, from the pinned `tools/production_loc.py --files` at a stated commit.
  This is the `prod` column, never physical lines or Git statistics.
- **Every directory carries its subtotal in brackets**, summing all listed
  production files beneath it as they are at the stated commit, removed files
  included. If only part of a directory is shown, say so in its annotation;
  the bracketed number then covers only the files shown.
- **A file that does not exist yet shows `—`**, followed by `new` (planned) or
  `cond.` (added only if existing code cannot cover the need). Never print an
  estimated LOC for it.
- **Annotations are short, in parentheses, after the number**: what changes in
  this step, or why the file is listed. Keep ownership rationale in prose.
- **State the scope above the listing**: the commit, the counter, and which
  crates or directories are included. Tests live outside `src/` and are not
  counted; name their directory in prose if it matters.

A listing is a snapshot. It does not make a planned name a scaffolding
requirement, and an existing source file or `Cargo.toml` still decides
what is built.

## 5. Why these rules exist (worked example)

The historical cache-credit failure and its immutable ledger remain linked in
[the measurement workflow](docs/general/agent-measurement-policy.md#5-why-these-rules-exist-worked-example).
Historical explanation is retained outside default implementation routing.

