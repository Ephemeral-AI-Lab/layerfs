# Agent rules for engine

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. This file adds implementation
boundaries for the engine layer under `engine/`; it does not repeat the
product mental model or campaign-specific benchmark procedures.

## Scope and ownership

| Crate | Owns | Architecture |
| --- | --- | --- |
| `layerfs-bridge` | the process facts the home and the Sandbox driver share (`process`), wire and native records, native install and control channels | [Bridge](../docs/architecture/engine/bridge/README.md) |
| `layerfs-daemon` | mounted Workspaces, their overlay connections and locks, mount/Ready/unmount, Commit composition, the direct Store adapter | [Daemon](../docs/architecture/engine/daemon/README.md) |
| `layerfs-fuse`, `layerfs-nfs`, `layerfs-mount-ports` | the kernel routes: FUSE and NFS as peers over the shared mount ports (#535 D3) | [Kernel routes](../docs/architecture/engine/kernel-routes/README.md) |
| `layerfs-overlay` | each Workspace's own overlay SQLite file | [Overlay](../docs/architecture/engine/overlay/README.md) |
| `layerfs-persistence` | global Store writes, SQLite and PostgreSQL (#535 D2) | [Store](../docs/architecture/engine/store/README.md) |
| `layerfs-project` | native import; Project Store Init and install (`layerfs-project::store`) | [Project Init](../docs/architecture/engine/project-init/README.md) |
| `layerfs-store-service` | the SDK Store calls (`src/methods/`) over `read`, `write` and `history` | [Store calls](../docs/architecture/engine/store-calls/README.md) |
| `layerfs-workspace` | the Workspace view and captured construction (`CapturedNamespace`) | [Workspace](../docs/architecture/engine/workspace/README.md), [Commit](../docs/architecture/engine/commit/README.md) |

- Core owns content, history, storage and telemetry. The API crates
  (`layerfs-api`, `layerfs-cli`, `layerfs-node`, `layerfs-python`, under
  `api/crates`; see the [API guide](../api/AGENTS.md)) and the
  [control crates](../control/AGENTS.md) sit above the engine: **no engine
  crate may depend on an API or control crate.** Since #535 (owner,
  2026-11-03, D1, D4) Sandbox and network code live in the control layer
  (renamed from connect by layerfs-project#101, D-6): the Sandbox runtime
  (now `layerfs-sandbox-backend`), the machine layer (now `layerfs-platform`)
  and remote storage (now `layerfs-bucket`, with `layerfs-store-service`'s
  `remote/`) moved out in #535 phase 1.
- `layerfs-store-service`'s `src/methods/` (#536 H3) holds the method table
  (`methods::call`), its argument reader and one file per `file.*`/`branch.*`
  call; `read`, `write` and `history` know no SDK method names, and `methods/`
  is the facade over them. Their tests
  live in store-service's `tests/` (`concurrent_commits`, `lane_merge` and
  `merge_report`, which need a daemon, in `layerfs-daemon`'s).
- `layerfs-api` retains its public re-exports of Project Store Init and
  install. Its SDK-facing daemon integration tests include daemon and Sandbox
  test fixtures from this layer.
- [Cargo.toml](../Cargo.toml) decides membership. Add a member only with a
  real product boundary and implementation; a directory or an old test does
  not establish one.
- Retired code stays retired: the seven `*-legacy` directories and
  `layerfs-server` (R7-retire), and the v0.1.6 reference under root `crates/`
  and `core/reference-tests` (R9c, 2026-10-10; recover them at local tag
  `reference-v0.1.6-final`, `d296981ac`, `crates` tree `498dd1917812`). Neither
  was a dependency, source include or fallback. The boundary guard refuses
  their return. Do
  not restore predecessor code to supply a feature; pinned read-only SDK
  views (303/08 O-10), for example, are deferred, not dropped, and a kept
  feature needs a new design on the SQLite overlay.
- Each mounted Workspace has its own overlay file and connection behind its
  own lock, and the calling thread runs each short job; there is no SQL owner
  thread, queue, credit scheme or shared memory budget (#321). There is no
  daemon-wide graceful drain, and none is planned.
- Optional privileged command cancellation and client provenance are outside
  the current owner scope.
- How the engine reached this state (S6 to S10, R2 to R9c, the retired host
  runtime and its records) is in the [archived guide history](../docs/research/history/agents-guides/README.md). Current
  behaviour is in the [architecture guide](../docs/architecture/README.md).
- Cluster-one work starts with the [handbook](../docs/general/cluster_one_handbook.md) and
  [CAS/CDC/delta guide](../docs/general/cas_cdc_deltaencoding_handbook.md). Use public contracts,
  source pins and limitations; preserve canonical compatibility.
- Cluster-two work starts with the [architecture guide](../docs/architecture/README.md):
  the engine topics it touches, the [flows](../docs/architecture/flows/README.md) and
  [Where each call runs](../docs/architecture/control/README.md#where-each-call-runs).
  Required corrections are real engineering scope; research candidates are not
  implemented features or excuses for accumulated workload caps.
- Closed Stage 0–6 handoffs and prior server/backend plans are historical.
  Consult them for a selected compatibility/reference question; do not restart
  their loops or inherit their retired topology. Do not revive `layerfs-server`.
- Keep unrelated work intact. Migration, source moves and retirement are explicit
  changes, not side effects of implementing a component.

## Architecture and public contracts

Overlay SQL uses the `directory_entry` and `operation_record` terms,
including their owned and indexed variants, throughout SQL and the public
ports. Since #321 (schema 34) every mounted Workspace has its own overlay file
with the same tables, copied from a closed empty template at mount and deleted
at terminal unmount; the current schema is the
[Overlay](../docs/architecture/engine/overlay/README.md) page's. A naming
change authorizes no physical table redesign. The schema history (17 to 37)
is in the [archived guide history](../docs/research/history/agents-guides/README.md#overlay-schema-history-schemas-17-to-37).

`docs/architecture/` describes product source, not release qualification.
A source change to boundaries, canonical/physical format, algorithm or named
bound updates the affected architecture/API document in the same change.
Record source pins and advance them explicitly; never silently re-date evidence.
Keep implementation description separate from target proposal and research.

Public content/storage/history/persistence contracts govern integrated callers.
Stable capture/input, Store-derived policy, backpressure, Save completion and
atomic history publication are separate obligations. Branch Commit overwrites the
head and keeps its captured parent; see the [owner supersession](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/BRANCH-OVERWRITE-DECISION-20261007.md). Immutable objects do
not replace authority, reference closure, mutable history or safe GC.

Project's native import consumes SQLite-backed acquisition through its
backend-neutral port. Its Store composition lives in `layerfs-project::store`:
Init into an existing Store is a live add through the Store's own writer
(2.0, #533); the older Init that creates, seals and installs a Store file stays
for the Store's own creation and install. The
owner-approved first-party edges include Bridge provisioning types and
Persistence; the exact allowlist still bounds those edges, and Project still
cannot use a third-party database driver directly. Canonical construction stays
in Content, encoding/packing in Storage and database writes in Persistence.
The port and its provider remain the existing indexed acquisition algorithm.
See the [S7/S9 remaining plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/IMPLEMENTATION-PLAN-S7-S9-20261006.md).

### Filesystem access and execution ownership

Owner clarification 2026-10-08: these are implementation and proof requirements,
not a claim that S8 or S10 is qualified. FUSE serves any permitted process that
can see the mount. Request admission and Workspace mutation/capture must not
require an Exec identity, registered command or LayerFS-launched parent.
Apply the same permissions, filesystem semantics and ownership to external
executors, interactive shells and ordinary Sandbox execution.

Sandbox setup or the external executor establishes command identity, mount
visibility and protection of Store, Overlay and daemon credentials. Ordinary
runtime execution owns standard I/O, exit status and caller-requested cancellation.
The SDK is the one call table in
[layerfs-api/src](../api/crates/layerfs-api/src), one file per call under its
family folder ([the calls](../docs/versioned/2.0.0/calls.md)); see the
[API guide](../api/AGENTS.md). `workspace.exec` checks the mounted directory
and runs the command through the Sandbox runtime
([where each call runs](../docs/architecture/control/README.md#where-each-call-runs));
the daemon has no exec/supervisor, launcher mode, per-Exec cgroups, command
registration or custom Exec stream/status/cancellation protocol.

FUSE owns exact request/open/lookup/capture/Commit lifetimes and complete
filesystem/daemon-work drain. Zero registered commands or shell exit is never
evidence of released descriptors, mappings, kernel references or daemon jobs.
Forced filesystem teardown does not implicitly kill caller-owned processes.
Normal unmount uses a reversible kernel busy probe while ordinary service stays
usable. Force refuses active namespace control producers before any effect.

Commit consumes the Workspace's published frontier through the shared Content,
Storage and Persistence pipeline regardless of launch route. stdout/stderr are
not filesystem state unless the command writes them to a file in the Workspace.
The old proposed managed-daemon Exec selections are withdrawn prospectively;
retain their original IDs, source pins, receipts and verdicts. Ordinary runtime
stream/result correctness and filesystem-lifetime proofs keep their real owners.

S8 verification must include a process launched outside the Exec API, with no
Exec registration, reading and mutating the mount and retaining a filesystem
reference during an unmount attempt. S10 must prove that such changes survive
Commit and a fresh mount. Use ordinary public filesystem access in these proofs.

Source ownership (reviewed 2026-10-08): `layerfs-fuse` owns its complete
native connection and request service, including bounded dispatch, parking and
resumption, kernel handlers and replies, and coherence. The daemon assembles
one shared FUSE service with its SQL and Store services and keeps the
registry, overall Ready/unmount and Commit composition. Dependency is
daemon → fuse → workspace, never fuse → daemon. Moving blocking OwnerClient
calls behind a port does not establish deferred service. The R2 to R5 records
are in the [archived guide history](../docs/research/history/agents-guides/README.md#source-ownership-review-2026-10-08-r2-to-r5).

### Shared construction and completion boundaries

Guidance updated 2026-10-08; names and paths rechecked 2026-11-16. Namespace Init and
daemon construction share Content, Storage and Persistence through their public
ports. Their input adapters and publication lifecycles have distinct owners:

| Input/operation | Existing route |
| --- | --- |
| Native Init file | Project import calls Content `construct_stream`; bounded output batches feed `Save::accept` |
| New captured file | Workspace `CapturedFileEdits` calls Content `construct_runs` over data/zero windows |
| Existing captured file | The same adapter calls `apply_indexed_edits_view_backed` over an authenticated retained `FileView` and normalized final edits |
| Initial namespace | Project streams acquired bindings/inodes through Content directory/table builders |
| Captured namespace | Workspace `CapturedNamespace` seals headers, typed values and fresh ranks into indexed records and calls Content `update_filesystem_streamed_backed`; names come from the retained captured reader |
| Captured symlink and metadata | The same producer calls Content `emit_symlink` for a fresh target and patches or builds the portable attribute tree; a stored symlink keeps its base content root |
| Object output | Content emits `FinalizedObject`; Storage `SaveSink` implements `FinalizedConsumer` by forwarding to `Save::accept` |

Use the actual Store-derived construction policy and bounded capacities on
both routes. Fresh construction and incremental edits use appropriate public
entrypoints and common canonical formats; do not assume an incremental file root
must equal a fresh full reconstruction, since retained chunk boundaries can differ.
Overlay owns mutable payload, metadata, operation records and construction scratch
under Workspace prefixes. Content owns canonical algorithms; Storage owns
deduplication, encoding and packs; Persistence owns global Store writes. Keep
SQLite types and paths out of Content and the daemon's provider-independent
`store/` adapter. Do not materialize a native tree or invoke Project Init to
implement a Workspace Commit. Native Init acquisition backing remains a separate
host provisioning facility.

Current source has captured-file normalization and the Store half of Commit.
`BoundWorkspace::commit` captures once, begins a Save, invokes a caller-supplied
Content constructor, finishes the Save, publishes through History and installs
the known root locally. Full captured namespace assembly, including names,
links, metadata and changed file roots, is
[`CapturedNamespace`](crates/layerfs-workspace/src/construction/driver.rs).
The product constructor `BoundWorkspace::commit_captured` runs it inside that
driver: it acquires the captured reader and one operation owner, hands the
driver the original cause on failure, and releases both owners only after the
driver's outcome is known, keeping them with the failure when it is not
([product Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/79-product-commit.md)). A control Commit
uses it; `Service::execute` still takes a caller's constructor for driver tests.
See [captured-file construction](crates/layerfs-workspace/src/construction/captured/owner.rs),
[the shared Save sink](../core/crates/layerfs-storage/src/save/operation.rs) and
[Store Commit composition](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/65-store-commit-composition.md).

Keep completion boundaries explicit: `Save::finish()` completes object storage;
Init then initializes history, while Commit calls `commit_branch` against its
expected head before local base installation. Init into an existing Store is a
live add through the Store's own writer (2.0, #533); only the Init that creates
a Store file calls `SqliteHandles::seal()` to checkpoint, close and hand off
that file. The macOS allocation correction
runs only at this sole-owner Store seal; it is not a Content/pack format change
or live-daemon maintenance. Linux daemons keep their shared Store open. The
[seal proof](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/SEAL-ALLOCATION-STRIDE1-20261007.md) retains its exact
platform and benchmark scope; it does not qualify daemon allocation or S10.

## Optimization and performance debugging

Before changing a performance-sensitive path or diagnosing a performance finding,
read the [optimization guide](../docs/general/optimization-guide.md). Its FUSE,
SQLite and complexity rules are mandatory: reject quadratic or worse scaling,
use SQLite-backed mutable indexes/state rather than custom duplicate trees/graphs,
and account for request/SQL/IO/copy/residency/cleanup work. Preserve cluster one's
canonical formats and public APIs; use count-driven evidence and the existing
measurement policy. SQLite debugging/optimization claims require both database
EXPLAIN and correlated runtime database profiling; overall wall time or a plan
alone is insufficient. An unrelated/docs-only change does not require a campaign.
The [optimization handbook](../docs/general/optimization-handbook.md) is the
working procedure on top of those rules: the counters that already exist, the
diagnostic groups, the scaling checks and the current candidate list.

## Product source and external verification

The Rust `src/` directories in core, engine, control and api contain
production code exclusively, including
included/generated implementation. Runtime SQL and other shipped implementation
have the same ownership rules.

Forbidden in product source: inline tests/benches, test-only cfg/features,
fixtures, mocks, fake clocks, fault injection, test/benchmark drivers, test-only
public APIs, benchmark-selected algorithms and executable rustdoc test blocks.
Platform cfgs, real features, production validation/invariants and actual telemetry
remain legitimate. Put API documentation with the API; examples outside src.

| Content | Location |
| --- | --- |
| Product implementation | Core/engine/control crate `src/`, API Rust/JavaScript/Python `src/` and required runtime source |
| Public-API tests and their helpers/fixtures | Package `tests/` |
| Runnable examples | Package `examples/` |
| Package benchmarks | Package `benches/` |
| Product-wide harness/diagnostics | the [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark) repository; inspect the actual harness manifest/entrypoint |
| Development checks | `tools/` |

Tests exercise the production library that normal consumers use. Do not
include/recompile private source, redirect target paths into test directories,
import helpers into production or expose an API only to enable a test.
Test-only dependencies belong in dev-dependencies; retain component-specific
restrictions, including std-only telemetry.

Prefer deterministic public behavior and external observations. Telemetry tests
can use completed reports and structural ordering; do not add a product fake-clock
interface or narrow sleep-based wall thresholds. Verify disabled behavior through
its actual allocation/clock path. Scaffold no empty placeholder directories.

## Production file size and responsibility

All new replacement/application-adapter production files have at most **999
physical lines**, including comments/blanks. This includes required runtime SQL
or other shipped formats outside Rust src. Extend guard coverage for new formats;
moving implementation does not evade the rule. The legacy root source is
retired; do not recreate a root `crates/` for product work.

Every product `lib.rs`/`mod.rs` has at most **200 physical lines**. These files
contain attributes, declarations, imports/reexports, API docs and thin forwarding.
Definitions, impls, state/constants encoding behavior, branching, loops, conversions,
validation, formatting and I/O belong in focused named implementation files.

Split by responsibility before the ceiling: lookup, transactions, reconstruction,
placement and ownership, not numbered files or a renamed god module. Use ordinary
functions/concrete types; do not add a factory/interface per algorithm or a
service locator. Keep actual process/provider boundaries independently usable.
Do not minify, expand macros or add includes to evade size/ownership rules.

Physical line ceilings differ from production LOC. Follow the root
[per-commit LOC policy](../AGENTS.md#production-loc-comparison-for-every-commit)
using exact parent/staged/committed trees and migration subtotals. Docs, tests,
fixtures, examples and tools do not contribute to production LOC.

## Failure, persistence and dependencies

- One attempted operation; no automatic retry, busy handler, refresh/reprepare,
  error-driven algorithm/backend/reference substitution or guessed recovery.
  Readiness waits and explicit cancellation have their own state before an
  attempt; they do not permit replay after failed/unknown publication.
- Preserve definite-failure atomicity, original errors and exact uncertain
  custody. No resend, rollback/delete or success claim based on an unfenced read.
- Global Persistence uses **Disposable/WAL/OFF only**, on macOS and Linux,
  under the [owner's active-profile rule](../AGENTS.md#active-persistence-profile--owner-direction-2026-10-07).
  Durable execution is disabled indefinitely until explicit owner reauthorization;
  retained Durable source may compile but no test, diagnostic, measurement or
  application invocation may execute it. Select Disposable explicitly rather than
  relying on a default argument. Disposable claims process-crash survival only.
  Open verifies WAL and never converts an
  existing Store. Select profile/layout before open. A PostgreSQL Store (#506)
  has no profile; its write setting is `synchronous_commit` (default `off`),
  recorded by `store.connect`; see
  [PostgreSQL](../docs/architecture/engine/store/postgresql.md).
  Owner supersession2026-10-07 keeps WAL throughout Init and Commit; no private
  MEMORY import or journal promotion remains. Owner approval, 2026-11-07:
  Disposable runs `wal_autocheckpoint=0`; each writable Disposable session's
  own checkpoint thread copies a log past 4 MiB with
  `wal_checkpoint(PASSIVE)` on a second SQLite connection, off the commit
  path. Durable keeps `wal_autocheckpoint=1000`. A non-WAL journal was
  measured and refused: it broke concurrent sessions and other processes. The measured host Init regression
  is retained as FAIL and accepted as the new baseline for that exact scope.
  Host seal consumes sole ownership, checkpoints, closes and verifies one file.
  Its macOS completion releases unused extents beyond logical EOF through the
  existing safe nix API, preserving the original file's bytes and identity.
  This narrow seal-only correction does not restore per-pack preallocation or
  the allocation owner, and is never invoked on a live shared daemon Store; see
  the [seal boundary](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/60-shared-store-foundation.md).
  See the [foundation checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/PRE-S8-F1-F4-20261007.md).
- Several writer processes open the same Store directly, with no host in the
  data path (owner direction 2026-10-07; routes in the
  [root guide](../AGENTS.md#where-calls-run)). A contended write still has one
  SQL attempt and exact failure/unknown custody. Owner
  clarification2026-10-12 requires one ongoing Commit per Workspace and allows
  concurrent Commits on different Workspaces. Bounded before-effect admission
  now queues SQL work within a Session and coordinates short Linux writer jobs
  on the existing local Store parent-directory inode; multiple Store files in
  one directory share this coarse short-transaction lane. Do not open/close a
  separate main/WAL/SHM descriptor for advisory ownership: closing a main-file
  descriptor can cancel SQLite's own process POSIX locks. This admission does
  not gate whole Saves or Commits or replay
  a failed SQL attempt. macOS holds the same parent-directory lane as Linux,
  over the same short writer jobs and its writable opens (#647), so separate
  macOS writers queue instead of meeting SQLite's Busy; only a writer that
  bypasses the lane (an uncoordinated raw SQLite process) retains the exact
  Busy refusal. Follow the
  [integration contract](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/303/06-cluster-one-integration.md) and its
  [deepest-file plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/SERVERLESS-STORE-PLAN-20261007.md); do not
  extend the host-mediated SDK runtime, Bridge data codec or daemon upstream.
- Store concurrency (owner decision 2026-10-18) is the root rule
  [one global database, one writer, short holds](../AGENTS.md#where-calls-run):
  keep the admission, directory lock and publication lane, and shorten what
  runs inside the lane instead of adding writers or databases.
- The daemon overlay is a separate owner/profile from global persistence:
  one local database file per mounted Workspace, opened MEMORY/OFF/EXCLUSIVE by
  one connection behind that Workspace's lock. Workspaces share no overlay
  connection, page cache or memory budget. It is not an implemented distributed Store or a universal
  global-Store rule; reader-pool alternatives need their own proofs.
- WAL/sync are permitted for a declared global Store profile. Do not add
  fsync/fdatasync/sync_data/sync_all on disposable Workspace backing. Memory
  hints do not prove bounds. Claim only actual completion/durability contracts.
- No third-party patch, fork, vendor, registry edit or replacement dependency,
  except the owner's pinned [fuser Git fork](#fuser-git-source) below.
  Use existing capabilities and locked builds; report an incompatible required
  dependency with source evidence. Unsupported required platforms/capabilities
  fail explicitly, not via a silent no-op or fallback.

## Checks and completion

Run commands from the repository root with the root workspace manifest. For product,
API, manifest or runtime-source changes, cover tests/examples, warning-denying
Clippy and formatting, plus the boundary guard and its self-tests:

```sh
cargo +1.85.1 test --locked --all-targets --no-run
# Execute the compiled tests per package under an explicit timeout <=120 s.
cargo +1.85.1 clippy --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --all -- --check
python3 -B tools/check_product_boundary.py
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

Through the [parallel worktree launcher](../docs/general/parallel-worktrees.md),
run the same checks per package with `tools/worktree.py cargo` (it refuses
workspace aggregates): tests on Linux where they need a Linux Store or mount,
Clippy on macOS when the Linux build image has no Clippy. The 2-minute test
ceiling, the delete-rather-than-nurse rule for refactors and the frozen
fs-bench-pro are [root rules](../AGENTS.md#4-code-build-and-docs).

A bug fix adds a test that fails on the parent commit and passes on the fix;
run it once against the parent's source and report the result.

For docs-only changes, check links/anchors, status/claim accuracy and whitespace;
report why Rust/runtime checks are inapplicable. For tooling/harness changes run
the owning scoped tests. At final implementation identity run required covering
checks once; diagnose a failure from its output/source before a justified repair
and rerun. Do not repeat unchanged passing checks or run an unrelated legacy sweep.

The guard checks source text, line caps and common prohibited constructs,
including markers embedded in source comments/strings. It is not a Rust semantic
or runtime proof. Review ordinary macros, helpers, dependency/target declarations
and generated inputs; extend scanners for new shipped formats. An excluded
FUSE/daemon package is not exercised by passing active-workspace tests.

Test lane selection is [tools/ci_tests.py](../tools/ci_tests.py). When a test
target is renamed, moved or measured at 5 s or more, update its `SLOW` list;
Linux-only daemon targets and the named API mount targets go to the nightly
mount lane. Do not disable discovery. Report exact checks, failures and gaps;
a scan or proposal alone does not establish implementation.

### Build flags are part of the check

aarch64 binaries retain the repository ARMv8 AEAD build inputs:
`--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon
-C target-feature=+aes,+sha2`.
The [root Cargo config](../.cargo/config.toml) is discovered from the working
directory, independently of the selected manifest. Builds outside the repository or
with explicit RUSTFLAGS must repeat that profile. Record flags/config in build
identities; preserve the transport's required-profile refusal when integrating
it. Documentation builds do not prove the binary profile.

Measurement work additionally follows the
[agent measurement workflow](../docs/general/agent-measurement-policy.md) and
[owning core harness guide](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/fs-bench-pro/AGENTS.md). Frozen identities,
cache states and budgets are evidence contracts, not product runtime limits.

### Fuser Git source

Owner direction 2026-11-15 supersedes the earlier crates.io-only and local-patch
rules: use [Ephemeral-AI-Lab/fuser](https://github.com/Ephemeral-AI-Lab/fuser)
directly as a Git dependency. The selected revision is
`7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4` (package version 0.18.0), kept
reachable by the fork's tag `v0.18.0-layerfs.1`. Repository rulesets on the fork
forbid deleting or force-pushing `fuser-0.18.0` and moving or deleting
`v*-layerfs.*` tags; tag every new pinned revision the same way.
The fork owns its timestamp correction, observable receive-loop lifecycle and
negotiated receive buffers. These changes are independent of LayerFS; maintain
them in that repository, not through a second copy or local patch here.

Keep the full `rev` in `layerfs-fuse/Cargo.toml` and the matching Git source
and commit in the root lockfile. Do not use a floating branch/tag, a registry
fallback, `[patch]`, `[replace]` or a local vendored fuser. A revision change
updates the manifest, lockfile and the
[FUSE architecture page](../docs/architecture/engine/kernel-routes/fuse.md#fuser-git-dependency)
together, needs owner approval ([code review policy](../docs/policy/code-review.md)),
and needs a fresh `tools/worktree.py prepare` in each worktree before its offline
builds. Use locked builds; fail explicitly when the selected source is unavailable.

The fork's own repository maintains its code and tests; LayerFS uses ordinary
locked dependency builds and integration tests. The boundary guard
(`tools/check_product_boundary.py`) refuses a fuser source other than the fork at
a full `rev`, a `[patch]`/`[replace]`/`paths` override and a lock entry that
differs from the manifest; it does not check the fork's code. The earlier source pins, owner decisions,
Docker acceptance and Linux fractional signed-minimum FAIL remain unchanged in
[the historical records](https://github.com/Ephemeral-AI-Lab/layerfs/tree/8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5/engine/patches/fuser-0.18.0).
Switching the dependency does not relabel that evidence or extend its scope.

### Authorized Persistence seal file control

Owner authorization 2026-10-07: "Approve the narrowly audited seal wrapper".
The system macOS SQLite enables persistent WAL and otherwise leaves empty
sidecars after checked close. Persistence uses `deny(unsafe_code)` with one
allowed module, `src/backend/sqlite/lifecycle/file_control.rs`, compiled on macOS
only. It sets and reads back `SQLITE_FCNTL_PERSIST_WAL` under sole seal
ownership, and on a live-Store snapshot's own new destination connection (#506).
No other Persistence module may contain unsafe Rust; the guard and its tests
enforce the exact path. This does not authorize third-party patches, dependency
changes, manual sidecar deletion or other FFI. See the
[decision and diagnostic](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/SEAL-PERSIST-WAL-DECISION-20261007.md).

### Bridge process identity file

The bridge holds the process facts the home and the Sandbox driver share
(`layerfs_bridge::process`), moved below the drivers by layerfs-project#112
under review finding R10, which the owner accepted on 2026-11-06 (#101 M-3;
compute-plane-2.0 rule 5 and the §6.6 row: "move the process facts, with the
one audited macOS unsafe file and its guard entry, below the drivers"). The
bridge uses `deny(unsafe_code)` with one allowed module,
`src/process/darwin.rs`, compiled on macOS only: `proc_pidinfo`,
`proc_listallpids` and the `sysctl` boot id, each with its SAFETY comment, as
they were in the driver. No other bridge module may contain unsafe Rust; the
guard and its tests enforce the exact path. This authorizes no other FFI or
dependency change.
