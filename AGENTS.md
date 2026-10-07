# AGENTS.md

> **Status:** Current general guide.

Repository-wide instructions for LayerFS. Also read [core/AGENTS.md](core/AGENTS.md)
for implementation under `core/`. Current owner instructions and documented
supersessions govern; a closed handoff or historical receipt is not a new assignment.

## Product scope and current workstreams

**Implement both cluster one and cluster two in `core/`.** Root `crates/` is the
v0.1.6 reference implementation, retained for inspection and explicitly selected
baseline comparisons. It is not a dependency, source include or fallback for core.
Owner direction: remove that legacy tree once cluster two is complete; do not
delete it early or preserve it as a second product. Retain historical evidence
and report retirement honestly in the source-size comparison.

| Workstream | Current scope | Entry point |
| --- | --- | --- |
| Cluster one | Implemented content, storage, history, persistence, project Init and telemetry libraries | [Cluster-one handbook](cluster_one_handbook.md), [CAS/CDC/delta handbook](cas_cdc_deltaencoding_handbook.md) |
| Cluster two | Workspace, FUSE, daemon, execution and Commit/runtime integration design and implementation | [Current design index](core/docs/issues/303/README.md) and its seven primary contracts |
| Legacy reference | v0.1.6 under root `crates/`; retire after cluster two completes | Read only when the task needs a reference/baseline |

Cluster names are workstreams. C1/C2/C5 in older component documents mean
content/storage/history. Check [core/Cargo.toml](core/Cargo.toml) for active
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
claim that the Durable API has been removed. The daemon overlay remains a
separate MEMORY/OFF/EXCLUSIVE database.

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
- Initial full-root acquisition is explicit. Repeated mount binds the complete
  root without a whole-tree scan/copy/materialization, dependency restoration or
  new database. Demand-loaded metadata/content still pay real I/O.
- One local overlay SQLite database belongs to each daemon and is initialized
  before readiness. Workspace metadata, physical payload, operation records and custody
  are namespaced within it. One SQLite writer does not serialize whole Execs or
  Commits; use short atomic jobs and bounded fair service.
- Commit captures the shared locally published Workspace frontier, potentially
  including several calls' changes. Retain stable existing input without a bulk
  overlay copy. Construct/save/publish it; known install advances the base while
  preserving later active mutations and the effective live view.
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
- Automatic bounded local reclamation follows last-owner release, including
  during live activity and idle periods. Terminal unmount includes close/cleanup;
  physical SQL deletion may finish later. It does not delete global history or
  imply that the shared database file shrinks.
- Target (owner direction 2026-10-07): every daemon opens the global Store
  directly from a volume the daemons share and reads, saves and publishes
  in-process. The host runs Project Init, installs one sealed Store file into
  that volume and is control-only afterwards (mount, Exec, Commit, status,
  unmount). Do not revive `layerfs-server` or build a host/server adapter in
  the data path; the host-mediated SDK runtime is retired, not extended. See
  the [integration contract](core/docs/issues/303/06-cluster-one-integration.md).
  Immutable objects support reuse/distribution; authority, reference closure,
  mutable history, durability and safe GC still require exact contracts.
  Filesystem history is not a process, memory, socket or stdout/stderr
  checkpoint.

## Read the current contract for the task

Before cluster-two implementation/integration, read both root handbooks and the
[current design index](core/docs/issues/303/README.md), then the relevant primary
operation, [engine](core/docs/issues/303/daemon-sqlite.md),
[FUSE](core/docs/issues/303/fuse.md) and
[runtime integration](core/docs/issues/303/06-cluster-one-integration.md) contracts.
The [optimization investigation](core/docs/issues/303/fuse-optimization-investigation.md)
is research; it does not silently select new algorithms, profiles or capabilities.

Completed/superseded Stage 0–6 prompts are historical references, not default
routing. Preserve canonical compatibility and evidence, but do not resume old
loops, reopen closed rows or run old campaigns unless the current task selects
that work. Do not choose authority by issue number alone.

For documentation/release work, read the
[documentation policy](docs/general/documentation-policy.md) and
[release policy](docs/general/release-policy.md).
For measurements, read the [agent measurement workflow](docs/general/agent-measurement-policy.md),
[benchmark rules](docs/general/benchmark_rules.md),
[report template](benchmark_agent_report.md) and owning harness/family contract.
Core routing is [core/benchmark/fs-bench-pro/AGENTS.md](core/benchmark/fs-bench-pro/AGENTS.md).
Root [benchmark/AGENTS.md](benchmark/AGENTS.md) and its
[Quickstart](benchmark/fs-bench-pro/QUICKSTART.md) apply to selected legacy/reference
work, not automatically to new core operations.

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
   No extra helper/lane to pass a gate; FUSE dispatch is a separate concern.
9. Measure both speed and storage. Reject about 50% speed loss for about 5%
   storage benefit; report raw deltas and preserve frozen-gate failures.

## 4. Code, build and docs

- Preserve unrelated work. Build the current core implementation, not legacy
  aliases, source includes or error-driven substitutes. Keep platform cfgs.
- No new dependency if an existing crate supplies the capability. Never patch,
  fork, vendor, replace or edit third-party code/registry packages, except the
  owner's explicitly authorized fuser 0.18.0 signed-timestamp patch described in
  [core provenance checks](core/AGENTS.md#fuser-provenance-and-authorized-patch-checks).
  Use locked builds and report an incompatible required dependency with evidence.
- One attempted operation; no automatic retry/busy handler, refresh/reprepare
  or failed-operation replay. Readiness waits are not retries. Preserve exact
  failure/uncertainty and custody; no guessed resend, deletion or success.
- There is no CI or aggregate pre-push gate. `tools/preflight.sh` is permanently
  retired; do not restore it or an equivalent wrapper. Verify the changed scope
  with [core checks](core/AGENTS.md#checks-and-completion), report checks/gaps,
  and never claim CI green or that an empty guard scan proves implementation.
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
  no `fsync`/`fdatasync`/`sync_data`/`sync_all` on disposable Workspace backing.
  Memory hints are hints, not evidence of resident bounds.
- New core/application production files obey the 999-line ceiling and 200-line
  declaration/delegation ceiling for `lib.rs`/`mod.rs`. Product-only source,
  external tests and guard coverage are detailed in the core guide.
- Update affected source architecture/API documentation with implementation
  changes. Keep proposal, research, implemented and measured claims distinct.

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
  deletion of the reference an algorithmic simplification.
- **Put the result in the commit message and handoff.** Use
  `Production LOC: <before> -> <after> (delta <signed difference>)`, with scope
  and counting method, plus migration subtotals when applicable. For multiple
  commits, give a comparison for each. A test/docs-only commit still reports
  the unchanged production total and delta 0; it does not report a fictitious
  zero-sized product or add test/documentation LOC to the headline.

LOC growth is allowed when justified by the product change; this rule does not
require every commit to shrink. Never remove required validation, compress code
into dense lines, or move implementation outside the declared scope to improve
the number. Complete the comparison before committing; do not invent estimates.

## 5. Why these rules exist (worked example)

The historical cache-credit failure and its immutable ledger remain linked in
[the measurement workflow](docs/general/agent-measurement-policy.md#5-why-these-rules-exist-worked-example).
Historical explanation is retained outside default implementation routing.
