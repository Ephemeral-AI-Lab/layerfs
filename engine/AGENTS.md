# Agent rules for engine

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. This guide adds what is
specific to `engine/`: who owns what, the boundaries between crates, and how
to check a change. Current behaviour is in the
[architecture guide](../docs/architecture/README.md); how the engine got here
is in the [archived guide history](../docs/research/history/agents-guides/README.md).

## Scope and ownership

| Crate | Owns | Architecture |
| --- | --- | --- |
| `layerfs-protocol` | the wire: SDK call shape and values (`sdk`), the daemon control wire (`daemon`), authenticated duplex channels (`duplex`), Store provisioning records (`provision`) | [Bridge](../docs/architecture/engine/bridge/README.md) |
| `layerfs-os` | what the kernel reports: the process facts the home and the Sandbox driver share (`process`), the macOS mount table and bounded host-name resolution (`network`), the Sandbox runtime layer (`runtime`) | [Bridge](../docs/architecture/engine/bridge/README.md) |
| `layerfs-daemon` | mounted Workspaces, their overlay connections and locks, mount/Ready/unmount, Commit composition, the direct Store adapter | [Daemon](../docs/architecture/engine/daemon/README.md) |
| `layerfs-fuse`, `layerfs-nfs` | the kernel routes: FUSE and NFS as peers over the shared mount ports in `layerfs-vfs` (#535 D3) | [Kernel routes](../docs/architecture/engine/kernel-routes/README.md) |
| `layerfs-overlay` | each Workspace's own overlay SQLite file | [Overlay](../docs/architecture/engine/overlay/README.md) |
| `layerfs-persistence` | global Store writes, SQLite and PostgreSQL (#535 D2) | [Store](../docs/architecture/engine/store/README.md) |
| `layerfs-project` | native import; Project Store Init and install (`layerfs-project::store`) | [Project Init](../docs/architecture/engine/project-init/README.md) |
| `layerfs-store-service` | the SDK Store calls (`src/methods/`) over `read`, `write` and `history` | [Store calls](../docs/architecture/engine/store-calls/README.md) |
| `layerfs-vfs` | the Workspace view, captured construction (`CapturedNamespace`) and the shared mount ports (`ports`) | [Workspace](../docs/architecture/engine/workspace/README.md), [Commit](../docs/architecture/engine/commit/README.md) |

- **No engine crate depends on an API or control crate.** Core owns content,
  history, storage and telemetry. Since #535 (owner, 2026-11-03, D1, D4)
  Sandbox and network code live in the [control layer](../control/AGENTS.md)
  (renamed from connect by layerfs-project#101, D-6): the Sandbox backends
  (`layerfs-sandbox` `backend/`), the machine layer (`layerfs-machine`
  `platform/`) and backup buckets (`layerfs-store` `backup/`).
- **Store service methods** (#536 H3): `src/methods/` holds the method table
  (`methods::call`), its argument reader and one file per `file.*`/`branch.*`
  call, and is the facade over `read`, `write` and `history`, which know no
  SDK method names. Their tests live in store-service's `tests/`
  (`concurrent_commits`, `lane_merge` and `merge_report`, which need a daemon,
  in `layerfs-daemon`'s). `layerfs-api` keeps its public re-exports of
  Project Store Init and install, and its SDK-facing daemon tests include
  daemon and Sandbox fixtures from this layer.
- **Retired code stays retired:** the seven `*-legacy` directories and
  `layerfs-server` (R7-retire), the v0.1.6 reference and `core/reference-tests`
  (R9c; see the [root guide](../AGENTS.md#retired-and-frozen)). The boundary
  guard refuses their return. Do not restore predecessor code to supply a
  feature: pinned read-only SDK views (303/08 O-10) are deferred, not dropped,
  and need a new design on the SQLite overlay. Add a crate only with a real
  product boundary and implementation.
- **Facts that shape every change:** each mounted Workspace has its own overlay
  file and connection behind its own lock, and the calling thread runs each
  short job; there is no SQL owner thread, queue, credit scheme or shared
  memory budget (#321). There is no daemon-wide graceful drain, and none is
  planned. Privileged command cancellation and client provenance are outside
  the owner's scope.
- **Where to start:** cluster-one work with the
  [handbook](../docs/general/cluster_one_handbook.md) and the
  [CAS/CDC/delta guide](../docs/general/cas_cdc_deltaencoding_handbook.md);
  cluster-two work with the engine topics it touches, the
  [flows](../docs/architecture/flows/README.md) and
  [Where each call runs](../docs/architecture/control/README.md#where-each-call-runs).
  Required corrections are real scope; research candidates are not
  implemented features or excuses for workload caps. Migration, source moves
  and retirement are explicit changes, never side effects of a component.

## Architecture and public contracts

- **Overlay naming:** `directory_entry` and `operation_record` (with their
  owned and indexed variants) throughout SQL and the public ports. Since #321
  (schema 34) each mounted Workspace has its own overlay file with the same
  tables, copied from a closed empty template at mount and deleted at terminal
  unmount; the current schema is on the [Overlay](../docs/architecture/engine/overlay/README.md)
  page. A naming change authorizes no physical table redesign.
- **Contracts:** public content, storage, history and persistence contracts
  govern integrated callers. Stable capture and input, Store-derived policy,
  backpressure, Save completion and atomic history publication are separate
  obligations. Branch Commit overwrites the head and keeps its captured parent
  ([owner supersession](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/BRANCH-OVERWRITE-DECISION-20261007.md)).
- **Project composition:** native import consumes SQLite-backed acquisition
  through its backend-neutral port. Init into an existing Store is a live add
  through the Store's own writer (2.0, #533); the Init that creates, seals and
  installs a Store file stays for Store creation and install. Project's
  owner-approved first-party edges (`layerfs-protocol` provisioning types, Persistence)
  stay bounded by the guard's allowlist; Project uses no third-party database
  driver directly.
- **Source pins:** `docs/architecture/` describes product source, not release
  qualification. Record source pins and advance them explicitly; never
  silently re-date evidence.

### Filesystem access and execution ownership

Owner clarification 2026-10-08; implementation and proof requirements, not a
claim that S8 or S10 is qualified.

- **FUSE serves any permitted process that can see the mount.** Request
  admission and Workspace mutation or capture never require an Exec identity,
  registered command or LayerFS-launched parent; external executors,
  interactive shells and Sandbox execution get the same permissions,
  semantics and ownership.
- **Commands belong to their launcher.** Sandbox setup or the external
  executor establishes command identity, mount visibility and protection of
  Store, Overlay and daemon credentials; execution owns standard I/O, exit
  status and caller-requested cancellation. `workspace.exec` checks the
  mounted directory and runs the command through the Sandbox runtime
  ([where each call runs](../docs/architecture/control/README.md#where-each-call-runs)).
  The daemon has no exec supervisor, launcher mode, per-Exec cgroups, command
  registration or Exec stream/status/cancellation protocol. The SDK is the
  call table in [layerfs-api](../api/crates/layerfs-api/src)
  ([the calls](../docs/versioned/2.0.0/calls.md)).
- **FUSE owns lifetimes:** exact request, open, lookup, capture and Commit
  lifetimes and complete filesystem and daemon-work drain. Zero registered
  commands or shell exit never proves released descriptors, mappings, kernel
  references or daemon jobs. Forced teardown kills no caller-owned process.
  Normal unmount uses a reversible kernel busy probe while service stays
  usable; force refuses active namespace control producers before any effect.
- **Commit** consumes the published frontier through the shared Content,
  Storage and Persistence pipeline whatever the launch route. stdout/stderr are
  not filesystem state unless written to a file in the Workspace. The old
  managed-daemon Exec selections are withdrawn; keep their IDs, pins, receipts
  and verdicts.
- **Proofs:** S8 verification includes a process launched outside the Exec API,
  with no Exec registration, reading and mutating the mount and holding a
  filesystem reference during an unmount attempt; S10 proves such changes
  survive Commit and a fresh mount. Use ordinary public filesystem access.
- **FUSE source ownership** (reviewed 2026-10-08): `layerfs-fuse` owns its
  native connection and request service (bounded dispatch, parking and
  resumption, kernel handlers and replies, coherence); the daemon assembles one
  shared FUSE service with its SQL and Store services and keeps the registry,
  Ready/unmount and Commit composition. Dependency is daemon → fuse →
  workspace, never fuse → daemon. Moving blocking OwnerClient calls behind a
  port does not establish deferred service.

### Shared construction and completion boundaries

Namespace Init and Workspace Commit share Content, Storage and Persistence
through their public ports, with distinct input adapters and publication
lifecycles (names and paths rechecked 2026-11-16):

| Input/operation | Route |
| --- | --- |
| Native Init file | Project import calls Content `construct_stream`; bounded output batches feed `Save::accept` |
| New captured file | Workspace `CapturedFileEdits` calls Content `construct_runs` over data/zero windows |
| Existing captured file | The same adapter calls `apply_indexed_edits_view_backed` over an authenticated retained `FileView` and normalized final edits |
| Initial namespace | Project streams acquired bindings/inodes through Content directory/table builders |
| Captured namespace | Workspace `CapturedNamespace` seals headers, typed values and fresh ranks into indexed records and calls Content `update_filesystem_streamed_backed`; names come from the retained captured reader |
| Captured symlink and metadata | The same producer calls Content `emit_symlink` for a fresh target and patches or builds the portable attribute tree; a stored symlink keeps its base content root |
| Object output | Content emits `FinalizedObject`; Storage `SaveSink` implements `FinalizedConsumer` by forwarding to `Save::accept` |

- **One encoder.** Canonical construction stays in Content, encoding and packing
  in Storage, Store writes in Persistence; Overlay owns mutable payload,
  metadata, operation records and construction scratch. Add no second encoder,
  and keep SQLite types and paths out of Content and the daemon's
  provider-independent `store/` adapter. Never materialize a native tree or
  invoke Project Init to implement a Workspace Commit.
- **Use the real policy.** Use the Store-derived construction policy and
  bounded capacities on both routes. An incremental file root need not equal a
  fresh reconstruction: retained chunk boundaries can differ.
- **The product Commit:** `BoundWorkspace::commit_captured` runs
  [`CapturedNamespace`](crates/layerfs-vfs/src/construction/driver.rs)
  inside the Commit driver. It acquires the captured reader and one operation
  owner, hands the driver the original cause on failure, and releases both
  only once the outcome is known, keeping them with the failure when it is
  not ([product Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/79-product-commit.md)).
- **Completion boundaries:** `Save::finish()` completes object storage while the
  Store stays open; Init then initializes history, and Commit calls
  `commit_branch` against its expected head before installing the base
  locally. Only the Init that creates a Store file calls
  `SqliteHandles::seal()`, which consumes the sole session, checkpoints, closes
  and hands off that file; its macOS unused-extent cleanup runs only there and
  is no Content/pack format change or live-daemon maintenance. A live daemon
  keeps the shared Store open across Saves and Commits.

## Optimization and performance debugging

Before changing a performance-sensitive path or diagnosing a performance
finding, read the [optimization guide](../docs/general/optimization-guide.md);
its FUSE, SQLite and complexity rules are mandatory. Reject quadratic or worse
scaling; use SQLite-backed mutable indexes and state, not custom duplicate
trees or graphs; account for request, SQL, I/O, copy, residency and cleanup
work. A SQLite claim needs both EXPLAIN and correlated runtime database
profiling. The [optimization handbook](../docs/general/optimization-handbook.md)
is the working procedure. A docs-only change needs no campaign.

## Product source and external verification

- **`src/` holds production code only** in core, engine, control and api,
  including included or generated implementation; shipped runtime SQL and
  other shipped formats follow the same rules.
- **Forbidden in product source:** inline tests or benches, test-only cfg or
  features, fixtures, mocks, fake clocks, fault injection, test or benchmark
  drivers, test-only public APIs, benchmark-selected algorithms and executable
  rustdoc test blocks. Platform cfgs, real features, production validation and
  actual telemetry are legitimate.
- **Where things go:** public-API tests and their fixtures in package `tests/`,
  examples in `examples/`, benchmarks in `benches/`, product-wide harnesses in
  [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark),
  development checks in `tools/`. Scaffold no empty placeholder directories.
- **Tests use the real library.** Do not include or recompile private source,
  redirect target paths into test directories, import helpers into production
  or expose an API only for a test. Test-only dependencies are
  dev-dependencies; keep component restrictions such as std-only telemetry.
- **Deterministic observations.** Prefer public behaviour; telemetry tests use
  completed reports and structural ordering, with no product fake clock or
  narrow sleep-based thresholds. Verify disabled behaviour through its real
  allocation and clock path.

## Production file size and responsibility

- **999 physical lines** at most per new production file, comments and blanks
  included; this covers shipped runtime SQL and other formats outside Rust
  `src/` (extend guard coverage for a new format). Moving code does not evade
  it, and no root `crates/` returns for product work.
- **200 physical lines** at most per product `lib.rs`/`mod.rs`: attributes,
  declarations, imports and re-exports, API docs and thin forwarding only.
  Definitions, impls, behaviour-encoding state, branching, loops, conversions,
  validation, formatting and I/O go in focused implementation files.
- **Split by responsibility** (lookup, transactions, reconstruction, placement,
  ownership) before the ceiling, not into numbered files or a renamed god
  module. Use ordinary functions and concrete types: no factory or interface
  per algorithm, no service locator. Keep real process and provider
  boundaries independently usable. Do not minify, expand macros or add
  includes to evade these rules.
- Physical lines differ from [production LOC](../AGENTS.md#production-loc-comparison-for-every-commit),
  which every commit reports.

## Failure, persistence and dependencies

- **One attempt** ([root rule](../AGENTS.md#4-code-build-and-docs)), including no
  error-driven algorithm, backend or reference substitution and no guessed
  recovery. Readiness waits and explicit cancellation have their own state
  before an attempt and never permit replay after a failed or unknown
  publication. Preserve definite-failure atomicity, original errors and exact
  uncertain custody: no resend, rollback, deletion or success claim from an
  unfenced read. An unsupported required platform or capability fails
  explicitly, never as a silent no-op or fallback.
- **Every SQLite Store opens Normal/WAL/NORMAL**, on macOS and Linux, tests
  and diagnostics included
  ([profile](../AGENTS.md#active-persistence-profile--owner-direction-2026-11-17));
  the Disposable/WAL/OFF profile was removed (owner, 2026-11-23). Select the
  profile explicitly, never by default argument. Claim process-crash survival
  only until Normal's safety gate is run. Open verifies WAL and never converts an existing
  Store; select profile and layout before open. WAL stays on throughout Init
  and Commit (owner supersession 2026-10-07): no private MEMORY import or
  journal promotion. Checkpointing is described on the
  [SQLite page](../docs/architecture/engine/store/sqlite.md#checkpoints)
  (owner approval 2026-11-07; Durable keeps `wal_autocheckpoint=1000`); a
  non-WAL journal was measured and refused
  because it broke concurrent sessions and other processes. The measured host
  Init regression is retained as FAIL and accepted as the baseline for that
  scope. A PostgreSQL Store has no profile; its write setting is
  `synchronous_commit` ([PostgreSQL](../docs/architecture/engine/store/postgresql.md)).
- **Seal** consumes sole ownership, checkpoints, closes and verifies one file.
  Its macOS completion releases unused extents beyond logical EOF through the
  existing safe nix API, keeping the file's bytes and identity; it restores no
  per-pack preallocation or allocation owner and never runs on a live shared
  daemon Store ([seal boundary](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/architecture/60-shared-store-foundation.md)).
- **Writers:** several processes open the same Store directly, with no host in
  the data path ([routes](../AGENTS.md#where-calls-run)). One Commit runs at a
  time per Workspace; Commits on different Workspaces may run concurrently
  (owner, 2026-10-12). Bounded before-effect admission queues SQL work within
  a Session and coordinates short writer jobs on the Store's parent-directory
  inode, on Linux and macOS (#647), so separate writers queue instead of
  meeting SQLite's Busy; only an uncoordinated raw SQLite process still meets
  the exact Busy refusal. Store files in one directory share this lane. It
  gates no whole Save or Commit and replays no failed SQL attempt. Never open
  and close a separate main, WAL or SHM descriptor for advisory ownership:
  closing a main-file descriptor can cancel SQLite's own POSIX locks. Keep the
  admission, lock and publication lane, and shorten what runs inside the lane
  ([one writer, short holds](../AGENTS.md#where-calls-run)). Do not extend the
  host-mediated SDK runtime, Bridge data codec or daemon upstream.
- **The daemon overlay is a separate profile:** one MEMORY/OFF/EXCLUSIVE
  database file per mounted Workspace, opened by one connection behind that
  Workspace's lock, sharing no connection, page cache or memory budget. It is
  not a distributed Store; reader-pool alternatives need their own proofs.
- **Durability claims:** WAL and sync are permitted for a declared global Store
  profile. Add no `fsync`/`fdatasync`/`sync_data`/`sync_all` on disposable
  Workspace backing; claim only actual completion and durability contracts.
- **Dependencies:** no third-party patch, fork, vendor, registry edit or
  replacement dependency except the [fuser Git fork](#fuser-git-source); use
  existing capabilities and locked builds, and report an incompatible required
  dependency with source evidence.

## Checks and completion

For product, API, manifest or runtime-source changes, run from the repository
root with the root manifest:

```sh
cargo +1.85.1 test --locked --all-targets --no-run
# Execute the compiled tests per package under an explicit timeout <=120 s.
cargo +1.85.1 clippy --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --all -- --check
python3 -B tools/check_product_boundary.py
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

- **With the [worktree launcher](../docs/general/parallel-worktrees.md)**, run
  the same checks per package through `tools/worktree.py cargo` (it refuses
  workspace aggregates): tests on Linux where they need a Linux Store or
  mount, Clippy on macOS when the Linux build image has none.
- **A bug fix** adds a test that fails on the parent commit and passes on the
  fix; run it once against the parent's source and report the result.
- **Run once, at final identity:** the required covering checks; diagnose a
  failure from its output and source before a justified repair and rerun. Do
  not repeat unchanged passing checks or run an unrelated legacy sweep. For
  tooling or harness changes, run the owning scoped tests. For docs-only
  changes, check links, anchors, status, claims and whitespace, and say why
  Rust checks do not apply.
- **The guard is not a proof.** It checks source text, line caps and common
  prohibited constructs (including markers in comments and strings), not Rust
  semantics or runtime. Review macros, helpers, dependency and target
  declarations and generated inputs; extend scanners for new shipped formats.
  A package left out of the workspace is not exercised by passing tests.
- **Test lanes:** [tools/ci_tests.py](../tools/ci_tests.py). Update its `SLOW`
  list when a test target is renamed, moved or measured at 5 s or more;
  Linux-only daemon targets and the named API mount targets go to the nightly
  mount lane. Do not disable discovery. Report exact checks, failures and
  gaps; a scan or proposal alone establishes no implementation.
- **Build flags are part of the check.** aarch64 binaries keep the repository
  ARMv8 AEAD inputs (`--cfg aes_armv8 --cfg polyval_armv8 --cfg
  chacha20_force_neon -C target-feature=+aes,+sha2`). The
  [root Cargo config](../.cargo/config.toml) is found from the working
  directory, whatever manifest is selected; builds outside the repository or
  with explicit RUSTFLAGS repeat the profile. Record flags and config in build
  identities; keep the transport's required-profile refusal. A docs build
  proves no binary profile.
- **Measurements** also follow the
  [measurement workflow](../docs/general/agent-measurement-policy.md) and the
  [owning harness guide](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/fs-bench-pro/AGENTS.md);
  frozen identities, cache states and budgets are evidence contracts, not
  product limits.

### Fuser Git source

Owner direction 2026-11-15 (superseding the crates.io-only and local-patch
rules): use [Ephemeral-AI-Lab/fuser](https://github.com/Ephemeral-AI-Lab/fuser)
directly as a Git dependency at revision
`7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4` (package 0.18.0), kept reachable by
the fork's tag `v0.18.0-layerfs.1`.

- The fork owns its timestamp correction, observable receive-loop lifecycle and
  negotiated receive buffers, maintained in that repository with its tests,
  never as a second copy or local patch here. Its rulesets forbid deleting or
  force-pushing `fuser-0.18.0` and moving or deleting `v*-layerfs.*` tags; tag
  every new pinned revision the same way.
- Keep the full `rev` in `layerfs-fuse/Cargo.toml` and the matching Git source
  and commit in the root lockfile. No floating branch or tag, registry
  fallback, `[patch]`, `[replace]` or local vendored fuser; fail explicitly when
  the source is unavailable. The boundary guard refuses any other source, an
  override and a lock entry that differs from the manifest; it does not check
  the fork's code.
- A revision change updates the manifest, lockfile and
  [FUSE architecture page](../docs/architecture/engine/kernel-routes/fuse.md#fuser-git-dependency)
  together, needs owner approval ([code review policy](../docs/policy/code-review.md))
  and a fresh `tools/worktree.py prepare` in each worktree.
- Earlier pins, decisions, Docker acceptance and the Linux fractional
  signed-minimum FAIL stay in
  [the historical records](https://github.com/Ephemeral-AI-Lab/layerfs/tree/8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5/engine/patches/fuser-0.18.0);
  switching the dependency does not relabel that evidence.

### Audited unsafe files

Each crate below uses `deny(unsafe_code)` with exactly one allowed module; the
guard and its tests enforce the path, and no other FFI or dependency change is
authorized.

| Crate | Allowed module | Why |
| --- | --- | --- |
| `layerfs-persistence` | `src/backend/sqlite/lifecycle/file_control.rs`, macOS only | sets and reads back `SQLITE_FCNTL_PERSIST_WAL` under sole seal ownership and on a live-Store snapshot's own new destination connection (#506); owner authorization 2026-10-07, "Approve the narrowly audited seal wrapper" ([decision](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/307/SEAL-PERSIST-WAL-DECISION-20261007.md)); no manual sidecar deletion |
| `layerfs-os` | `src/process/darwin.rs`, macOS only | `proc_pidinfo`, `proc_listallpids` and the `sysctl` boot id, each with its SAFETY comment; moved below the drivers by layerfs-project#112 (finding R10, accepted 2026-11-06, #101 M-3), into `layerfs-bridge` (now `layerfs-os`, #828) |
