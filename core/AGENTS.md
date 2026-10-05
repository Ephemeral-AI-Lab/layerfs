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
  Workspace now exposes initial immutable base binding/read interfaces. Its
  temporarily relocated `layerfs-workspace-legacy` source remains excluded.
  Daemon now exposes an initial fair SQL owner library; its relocated predecessor
  remains excluded, and native executable/control/Exec integration is unfinished.
  SDK now embeds an initial host object/Save runtime with scoped sessions. Its
  predecessor is preserved in excluded `layerfs-sdk-legacy`; authenticated
  transport/history/control and full runtime acceptance remain unfinished.
  Bridge now builds initial pinned native KK channels and authenticates typed
  peers for SDK binding. Its old protocol source is excluded at
  `layerfs-bridge-legacy`; logical framing/multiplexing and complete transport/
  runtime service qualification remain unfinished.
  FUSE/API-core/sandbox directories are presently excluded reference/
  integration source; existence is not an implemented replacement.
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

`docs/architecture/` describes product source, not release qualification.
A source change to boundaries, canonical/physical format, algorithm or named
bound updates the affected architecture/API document in the same change.
Record source pins and advance them explicitly; never silently re-date evidence.
Keep implementation description separate from target proposal and research.

Public content/storage/history/persistence contracts govern integrated callers.
Stable capture/input, Store-derived policy, backpressure, Save completion and
conditional history publication are separate obligations. Immutable objects do
not replace authority, reference closure, mutable history or safe GC.

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
- Current global persistence is host-local macOS SQLite. Durable is WAL/FULL
  with declared macOS synchronization; Disposable is MEMORY/OFF without crash
  survival. Select profiles/layouts before open; do not silently migrate or claim
  a guarantee from another profile. Postgres remains unavailable.
- The daemon overlay is a separate owner/profile from global persistence:
  one local database per daemon, Workspace-prefixed state. Its MEMORY/OFF
  single-owner proposal is not an implemented distributed Store or a universal
  global-Store rule; reader-pool alternatives need their own proofs.
- WAL/sync are permitted for a declared global Store profile. Do not add
  fsync/fdatasync/sync_data/sync_all on disposable Workspace backing. Memory
  hints do not prove bounds. Claim only actual completion/durability contracts.
- No third-party patch, fork, vendor, registry edit or replacement dependency.
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
