# Agent rules for core

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. This file adds implementation
boundaries for both current workstreams under `core/`; it does not repeat the
product mental model or campaign-specific benchmark procedures.

## Scope and ownership

- Implement cluster one and cluster two in `core/`. Root `crates/` is the
  v0.1.6 reference, not a dependency, source include or fallback. Remove that
  legacy tree after cluster two is complete, with explicit retirement/accounting;
  it is not part of routine early cleanup.
- Active cluster-one members are content, storage, history, persistence, project
  Init and telemetry. The initial cluster-two SQLite engine is `layerfs-overlay`;
  its active membership does not complete Workspace/FUSE/daemon integration.
  [core/Cargo.toml](Cargo.toml) is the membership authority.
  Workspace now exposes immutable base binding/read, effective source-qualified
  lookup/stat/paged name merge, prepared actor install and atomic ordinary
  namespace operations, bounded writes/truncate/regrow and composed payload
  reads. Independent file/captured/operation custody and bounded live reclamation
  are implemented through S6, including physical reservation/headroom and
  indexed automatic reclamation; native/runtime/integrated exits remain
  unfinished. See [S6 audit](docs/issues/307/S6-EXIT-AUDIT.md). Its temporarily relocated `layerfs-workspace-legacy` source remains
  excluded.
  Daemon exposes the fair SQL owner and a direct global Store adapter with a
  fixed read set with fair admission/quarantine and read-only bind snapshots,
  one immutable cache shared across Workspaces, bounded root
  binding and operation-owned exact failures. Concrete opening stays outside
  the provider-independent adapter; see the
  [direct Store boundary](docs/architecture/61-direct-store-adapter.md).
  The host-mediated SDK/upstream transport is retired; see
  [the retirement record](docs/issues/307/PRE-S8-F12-20261007.md).
  Completions carry exact per-job statement-family receipts under
  explicit stage charging and a fixed lane table; see the
  [completion checkpoint](docs/issues/307/S7-COMPLETION-OWNERSHIP-20261007.md). Overlay/Daemon also expose bounded indexed operation records
  in the same database, with guarded atomic changes and automatic last-owner
  cleanup. Content now exposes a backed file editor over these records and
  Workspace supplies an explicit operation/file adapter with first-original
  failure custody. Run-aware localized edits reuse the frozen sparse scanner
  and bounded positive immutable zero evidence. Streamed directory inputs share
  the canonical filesystem algorithm; backed serial state supplies new-parent
  membership, rebuilt roots and initial counts/final row streams. Remaining
  validation/reducer/release state stays under its existing resource limits.
  Captured local inode/run points retain exact reader root/floor and bounded
  forward metadata progress. Captured regular-file normalization now derives
  authenticated base facts, coalesces final changes into indexed records and
  consumes them once through the retained FileView/fallible edit source. Backed
  reference reduction/release uses fixed rows, sealed membership passes and
  guarded FIFO/cursor transitions. Namespace normalization, remaining validation/
  topology state and live namespace normalization remain unfinished. The Store
  half of Commit is implemented with exact original failure/unknown custody;
  see [Store Commit](docs/architecture/65-store-commit-composition.md).
  The external E01 example records original startup work, diagnostics and Stop
  without creating a Workspace route. It supplies diagnostic receipt consistency,
  not E1/E2 performance admission. Its relocated predecessor remains excluded;
  standalone native executable/Exec integration is unfinished.
  Pre-S8 direct-operation accounting now has actual registered macOS/Linux engine
  and Store-half Commit runs, independent family/count validation, and supported
  phase observations. Numerical latency/residency and exact continuous peaks
  remain unqualified; see the [accounting scope](docs/issues/307/PRE-S8-F14-20261007.md).
  SDK creates/imports the initial Store, publishes the first Branch and consumes
  seal, returning a closed file plus explicit provider/locator/profile metadata.
  It retains no Store handle or data service. Native install now streams the
  sealed file once, opens it in the daemon and returns both actual SQLite versions;
  see [F5 handoff](docs/issues/307/PRE-S8-F5-20261007.md). Authenticated mount
  binding, Store-half Commit, status, terminal close, fork and history now compose
  the direct ports; see [native control](docs/architecture/68-native-workspace-control.md).
  The prior client/runtime, daemon upstream, Bridge logical data framing and
  excluded API-core are retired. Historical receipts keep their original topology
  and verdicts; excluded predecessor crates and root reference remain intact.
  Project native import preserves opaque symlinks, complete membership and hard
  links, with indexed acquisition state. The accepted host WAL baseline retains
  its recorded speed FAIL; no new timing is implied by SDK composition.
  Bridge retains pinned authenticated KK channels, bounded native records and
  independent fences and bounded native install/control. FUSE/Exec qualification
  and complete kernel mount readiness remain S8.
  The replacement Sandbox is an active member with ordinary Engine execution,
  owned lifecycle and authenticated SDK/daemon startup. Optional privileged
  command cancellation/client provenance is outside the current owner scope.
  The dormant FUSE directory remains excluded integration source; its presence
  does not establish an implemented native replacement.
  Add members only with real product boundaries and implementation.
- Cluster-one work starts with the [handbook](../cluster_one_handbook.md) and
  [CAS/CDC/delta guide](../cas_cdc_deltaencoding_handbook.md). Use public contracts,
  source pins and limitations; preserve canonical compatibility.
- Cluster-two work starts with the [current design index](docs/issues/303/README.md)
  and relevant primary operation/engine/FUSE contracts plus
  [runtime integration](docs/issues/303/06-cluster-one-integration.md).
  Required corrections are real engineering scope; research candidates are not
  implemented features or excuses for accumulated workload caps.
- Closed Stage 0–6 handoffs and prior server/backend plans are historical.
  Consult them for a selected compatibility/reference question; do not restart
  their loops or inherit their retired topology. Do not revive `layerfs-server`.
- Keep unrelated work intact. Migration, source moves and retirement are explicit
  changes, not side effects of implementing a component.

## Architecture and public contracts

Overlay schema19 uses `directory_entry` and `operation_record` terminology,
including owned/indexed variants, throughout SQL and the active public ports.
Workspaces still share physical tables partitioned by namespace. The naming
change authorizes no physical table redesign; generic temporary buffers and
historical receipts retain their distinct terms. See the
[terminology guide](docs/architecture/64-overlay-terminology.md).
Schema17 adds native mount/source/lookup/read associations, atomic Workspace
read observations and bounded revoked-lookup retirement. Schema18 adds native
file associations over the existing OpenFile owner and atomic open decisions; see
[native read custody](docs/architecture/73-native-read-custody.md). This component
does not establish Ready, kernel service or full drain. Schema19 adds native
directory ownership, exact name-boundary cookies, retained parent updates and
bounded live cleanup; see [directory custody](docs/architecture/74-native-directory-custody.md).

`docs/architecture/` describes product source, not release qualification.
A source change to boundaries, canonical/physical format, algorithm or named
bound updates the affected architecture/API document in the same change.
Record source pins and advance them explicitly; never silently re-date evidence.
Keep implementation description separate from target proposal and research.

Public content/storage/history/persistence contracts govern integrated callers.
Stable capture/input, Store-derived policy, backpressure, Save completion and
atomic history publication are separate obligations. Branch Commit overwrites the
head and keeps its captured parent; see the [owner supersession](docs/issues/307/BRANCH-OVERWRITE-DECISION-20261007.md). Immutable objects do
not replace authority, reference closure, mutable history or safe GC.

Engine-independent Project logic may consume SQLite-backed acquisition through
an owning backend-neutral port. Concrete SQL and database ownership belong in
Persistence/application composition along the permitted dependency direction.
A guard refusal of Project's direct engine dependency identifies wrong placement;
resolve that boundary before proceeding. It does not require replacing indexed
backing with a custom sorter. The port and its SQLite provider now exist as an
opt-in Store schema ([acquisition backing](docs/architecture/44-acquisition-backing.md)),
and Project's import runs on that port alone
([backed acquisition](docs/architecture/43-backed-initial-acquisition.md)). Do not
weaken the boundary guard, give Project an engine dependency or reintroduce a
second acquisition algorithm.
See the [S7/S9 remaining plan](docs/issues/307/IMPLEMENTATION-PLAN-S7-S9-20261006.md).

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
SDK public organization uses ProjectApi, WorkspaceApi and SandboxApi. Optional
WorkspaceApi.exec selects the mounted directory and delegates to Sandbox;
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

Reviewed target source ownership2026-10-08: the replacement `layerfs-fuse` owns
its complete native connection/request service, including bounded dispatch,
parking/resumption, kernel handlers/replies and coherence. Daemon assembles one
shared Fuse service with its existing SQL/Store services, retaining registry,
overall Ready/unmount and Commit composition. Dependency is daemon -> fuse ->
workspace, never Fuse -> daemon. This remains unimplemented R2–R5 work; see the
[reviewed ownership](docs/issues/307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md).
Moving blocking OwnerClient calls behind a port does not establish deferred service.

### Shared construction and completion boundaries

Source checked at `4156e9070`, guidance updated 2026-10-08. Namespace Init and
daemon construction share Content, Storage and Persistence through their public
ports. Their input adapters and publication lifecycles have distinct owners:

| Input/operation | Existing route |
| --- | --- |
| Native Init file | Project import calls Content `construct_stream`; bounded output batches feed `Save::accept` |
| New captured file | Workspace `CapturedFileEdits` calls Content `construct_runs` over data/zero windows |
| Existing captured file | The same adapter calls `apply_indexed_edits_view_backed` over an authenticated retained `FileView` and normalized final edits |
| Initial namespace | Project streams acquired bindings/inodes through Content directory/table builders |
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
links, metadata and changed file roots, remains S10 integration work. S8 FUSE/Exec
and direct-Content Store-half proofs do not establish that integration.
See [captured-file construction](crates/layerfs-workspace/src/construction/captured/owner.rs),
[the shared Save sink](crates/layerfs-storage/src/save/operation.rs) and
[Store Commit composition](docs/architecture/65-store-commit-composition.md).

Keep completion boundaries explicit: `Save::finish()` completes object storage;
Init then initializes history, while Commit calls `stage_and_commit` before
local base installation. SDK Init subsequently calls `Handles::seal()` to
checkpoint, close and hand off one Store file. The macOS allocation correction
runs only at this sole-owner Store seal; it is not a Content/pack format change
or live-daemon maintenance. Linux daemons keep their shared Store open. The
[seal proof](docs/issues/307/SEAL-ALLOCATION-STRIDE1-20261007.md) retains its exact
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

## Product source and external verification

`core/crates/<package>/src/` contains production code exclusively, including
included/generated implementation. Runtime SQL and other shipped implementation
have the same ownership rules.

Forbidden in product source: inline tests/benches, test-only cfg/features,
fixtures, mocks, fake clocks, fault injection, test/benchmark drivers, test-only
public APIs, benchmark-selected algorithms and executable rustdoc test blocks.
Platform cfgs, real features, production validation/invariants and actual telemetry
remain legitimate. Put API documentation with the API; examples outside src.

| Content | Location |
| --- | --- |
| Product implementation | `core/crates/<package>/src/` and required runtime source |
| Public-API tests and their helpers/fixtures | Package `tests/` |
| Runnable examples | Package `examples/` |
| Package benchmarks | Package `benches/` |
| Product-wide harness/diagnostics | `core/benchmark/`; inspect the actual harness manifest/entrypoint |
| Development checks | `core/tools/` |

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
moving implementation does not evade the rule. Legacy root source remains a
reference until retirement, not a place to put new product work.

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
  existing Store. Select profile/layout before open; Postgres stays unavailable.
  Owner supersession2026-10-07 keeps WAL throughout Init and Commit; no private
  MEMORY import or journal promotion remains. The measured host Init regression
  is retained as FAIL and accepted as the new baseline for that exact scope.
  Host seal consumes sole ownership, checkpoints, closes and verifies one file.
  Its macOS completion releases unused extents beyond logical EOF through the
  existing safe nix API, preserving the original file's bytes and identity.
  This narrow seal-only correction does not restore per-pack preallocation or
  the allocation owner, and is never invoked on a live shared daemon Store; see
  the [seal boundary](docs/architecture/60-shared-store-foundation.md).
  See the [foundation checkpoint](docs/issues/307/PRE-S8-F1-F4-20261007.md).
- Target (owner direction 2026-10-07): the same Store opened directly by every
  Linux daemon from a shared volume, several writer processes, no host in the
  data path, using only Disposable/WAL/OFF. A contended
  write is one exact before-effect refusal, never a wait or retry. Follow the
  [integration contract](docs/issues/303/06-cluster-one-integration.md) and its
  [deepest-file plan](docs/issues/307/SERVERLESS-STORE-PLAN-20261007.md); do not
  extend the host-mediated SDK runtime, Bridge data codec or daemon upstream.
- The daemon overlay is a separate owner/profile from global persistence:
  one local database per daemon, Workspace-prefixed state. Its MEMORY/OFF
  single-owner proposal is not an implemented distributed Store or a universal
  global-Store rule; reader-pool alternatives need their own proofs.
- WAL/sync are permitted for a declared global Store profile. Do not add
  fsync/fdatasync/sync_data/sync_all on disposable Workspace backing. Memory
  hints do not prove bounds. Claim only actual completion/durability contracts.
- No third-party patch, fork, vendor, registry edit or replacement dependency,
  except the owner's fuser 0.18.0 signed-timestamp correction below.
  Use existing capabilities and locked builds; report an incompatible required
  dependency with source evidence. Unsupported required platforms/capabilities
  fail explicitly, not via a silent no-op or fallback.

## Checks and completion

Run commands from the repository root with the actual core manifest. For product,
API, manifest or runtime-source changes, cover tests/examples, warning-denying
Clippy and formatting, plus the boundary guard and its self-tests:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 -B core/tools/check_product_boundary.py
python3 -B -m unittest discover -s core/tools -p 'test_*.py'
```

Every test invocation obeys the repository's
[2-minute test ceiling](../AGENTS.md#4-code-build-and-docs): run it under an
explicit timeout of at most 120 s, per package when the whole suite cannot fit,
and treat reaching the ceiling as a hang to diagnose, not a run to wait for.

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

There is no CI or aggregate pre-push gate; `tools/preflight.sh` is retired.
Do not disable discovery or recreate an equivalent wrapper. Report exact checks,
failures and gaps. A scan or proposal alone does not establish implementation.

### Build flags are part of the check

aarch64 binaries retain the repository ARMv8 AEAD build inputs:
`--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon
-C target-feature=+aes,+sha2`.
The [root Cargo config](../.cargo/config.toml) is discovered from the working
directory, not by a manifest path into core. Builds outside the repository or
with explicit RUSTFLAGS must repeat that profile. Record flags/config in build
identities; preserve the transport's required-profile refusal when integrating
it. Documentation builds do not prove the binary profile.

Measurement work additionally follows the
[agent measurement workflow](../docs/general/agent-measurement-policy.md) and
[owning core harness guide](benchmark/fs-bench-pro/AGENTS.md). Frozen identities,
cache states and budgets are evidence contracts, not product runtime limits.

### Fuser provenance and authorized-patch checks

Additional owner authorization2026-10-08: the [scoped receive-loop lifecycle
extension](docs/issues/307/FUSER-LIFECYCLE-DECISION-20261008.md) may extend the
pinned fuser0.18.0 package beyond time.rs to establish loop entry/exit, retained
startup/join ownership and the proposed explicit lifecycle API. This narrowly
supersedes the timestamp-only file restriction below. Preserve the original
timestamp provenance unchanged; record and verify the lifecycle delta separately.
No other dependency change or general unsafe exception is authorized. Permission
does not establish implementation or native Ready/drain qualification.

Latest owner direction2026-10-06: "use fuser 0.18.0 from crates io and apply patch".
This supersedes the earlier registry-only/no-patch ruling for this one correction.
Keep the dependency version exactly0.18.0; use the checked-in copy of its official
crates.io archive with only `src/time.rs` changed by the recorded upstream timestamp
fix and regression tests. Both owning workspace roots use the exact path/version
`[patch.crates-io]` entry. Do not edit shared Cargo registry packages, adopt a Git
dependency, or extend this exception to unrelated source/dependency changes.

The original archive checksum, all85 published file hashes, exact diff and patched
time.rs hash are pinned in `patches/fuser-0.18.0/provenance.json` and checked by the
focused guard. A patched Cargo.lock entry has no registry checksum; the explicit
provenance record authenticates its modified bytes. See
[the authorized patch record](docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md).
Native qualification and the complete S8 contract remain separate requirements.

Subsequent owner direction2026-10-06: "docker verification is enough". Existing
native Docker proofs are accepted for verifying this fuser correction. Retain
Linux's fractional signed-minimum endpoint outcome as a platform limitation;
do not require QEMU or a custom-kernel campaign to verify the dependency fix.
This acceptance does not complete S8's product implementation or turn that
individual native FAIL into PASS. Continue the remaining S7–S9 work using Docker.

Before a native fuser build, run the focused provenance check:

```sh
python3 -B core/tools/check_fuser_integrity.py
```

For an installed registry source, also supply its archive, package directory and
checksum taken from its original registry lock or the pinned base-archive record
with `--archive`, `--package-directory` and `--locked-checksum`. This reads and
verifies package files without modifying
them. Run scoped self-tests when changing the check. It is not an aggregate
pre-push/CI wrapper or a timestamp-capability proof. Current evidence and the
historically rejected unmodified Git candidate are in
[the fuser verification record](docs/issues/307/FUSER-OFFICIAL-CANDIDATE-20261006.md).

### Authorized Persistence seal file control

Owner authorization 2026-10-07: "Approve the narrowly audited seal wrapper".
The system macOS SQLite enables persistent WAL and otherwise leaves empty
sidecars after checked close. Persistence uses `deny(unsafe_code)` with one
allowed module, `src/backend/sqlite/file_control.rs`, compiled on macOS only.
It sets and reads back `SQLITE_FCNTL_PERSIST_WAL` under sole seal ownership.
No other Persistence module may contain unsafe Rust; the guard and its tests
enforce the exact path. This does not authorize third-party patches, dependency
changes, manual sidecar deletion or other FFI. See the
[decision and diagnostic](docs/issues/307/SEAL-PERSIST-WAL-DECISION-20261007.md).
