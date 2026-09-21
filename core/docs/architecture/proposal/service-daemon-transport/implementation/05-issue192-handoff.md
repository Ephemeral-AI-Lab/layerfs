# Issue #192 implementation agent handoff

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Use the following instructions as the implementation agent's task. This handoff
does not establish an implemented feature or a passing verification result.

## Objective and authority

Implement [#192](https://github.com/Ephemeral-AI-Lab/layerfs/issues/192),
the service/bridge/daemon foundation with bounded, single-crate telemetry, under
pair 3 / [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).
Carry implementation through M0-M6 and the required acceptance evidence; do not
stop at a plan or direct-only smoke test. Report concrete blockers and incomplete
proofs honestly. Routine choices within this scope need no repeated permission.

Read the live issue, repository and core AGENTS.md, then this
[implementation packet](README.md), [product contract](01-product.md),
[telemetry contract](02-telemetry.md) and [verification matrix](03-verification.md).
Follow their supporting links, especially the
[file map](../02-file-layout-and-boundaries.md),
[operation catalog](../07-public-operations.md) and
[telemetry bounds](../08-telemetry-and-retention.md).
These proposals require concrete M0 decisions; illustrative numbers are not
already qualified defaults. Preserve the user's agreed scope and repo rules.

[#193](https://github.com/Ephemeral-AI-Lab/layerfs/issues/193) owns the later
direct/forward benchmark driver and campaign. Its
[draft](04-benchmark-direct-forward.md) informs future compatibility but is not
implementation scope here. Do not rewrite Stage 6 evidence or claim performance
improvements without a separately qualified measurement.

## Workspace activation and specification custody

The following state was checked on 2026-09-20; recheck before changing anything:

- Source checkout: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main`.
- Prepared implementation worktree:
  `/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs`.
- Prepared worktree was clean and detached at
  `10b9d4a6cf9d88267d508cb010cc82950e080d77`.
- Another worker owns C1/C2 optimization, including the
  `history-parent-lookup` worktree. Preserve that work and the v0.1.6 control.

Inspect status, branches and worktrees first. Reuse the prepared worktree and
create `codex/pair3-foundation` there if it does not exist. If it exists or the
worktree has changed, inspect ownership and continue without resetting another
worker's work. Use the implementation worktree explicitly for every implementation
command; creating or selecting a worktree does not change another tool's cwd.

The latest specification was local, including untracked files, when the issue
was created. Before coding, copy and pin the reviewed packet from the source
checkout into the implementation branch. The copy allowlist is:

- `core/docs/architecture/proposal/service-daemon-transport/` including this file.
- If required to preserve its referenced context, inspect and selectively copy
  `core/docs/architecture/proposal/04-boundary-and-trust.md`,
  `core/docs/architecture/proposal/README.md` and
  `core/docs/architecture/proposal/01-projection-and-runtime.md`.

Compare source/destination and record the selected files and hashes. Detect
concurrent changes rather than silently taking an inconsistent snapshot. Do not
copy the whole dirty checkout, optimizer source, unrelated docs or evidence.
Keep original source-checkout files intact. Commit the specification snapshot on
the implementation branch with the required actual production LOC comparison.
Publish/pin it for review as required by M0; distinguish a local commit from a
published revision. Do not force-push or change another worker's branch.

## Architecture and implementation scope

```text
host test driver -> stdin -> Linux Docker daemon
                                   |
                             bridge client
                                   |
                         authenticated network
                                   |
                             bridge server
                                   |
                       native macOS service
                                   |
                      local C1 + C2 -> SQLite

results: service -> network -> daemon -> stdout -> driver
diagnostics: bounded independent output -> host collector by default
direct parity: caller -> same authorized service handler -> C1 + C2
```

Add `layerfs-bridge`, `layerfs-service` and `layerfs-daemon` under `core/crates/`.
Extend the existing `layerfs-telemetry`; do not add a second telemetry crate.
Bridge is a shared contract plus delivery adapters at the two endpoints, not
another relay process. Service owns authorization, admission, Store lifetime and
C1/C2 composition. Daemon owns bounded headless input/output and client lifecycle.
Use one initial native carrier. Keep future carriers/providers possible through
real boundaries without implementing speculative registries or empty adapters.

Implement all five operations: `ReadFile`, `Inspect`, `ConstructFile`, `EditFile`
and `UpdatePreparedFilesystem`, with the exact M0-frozen supported variants.
C1/C2 object access, SQL, begin-save/finish/abort stay local to the service.
Prepared filesystem updates use existing identities and retained content roots.
N file saves followed by a tree update remain N+1 operations, without an atomic
composite-save or logical Commit claim. Successful saved roots require complete
validated input and confirmed C2 finish. Partial read bytes remain provisional
until terminal success. Preserve known/unknown outcomes; never replay mutations
or guess rollback after a lost response.

Use bounded streaming/backpressure, one active operation per connection,
aggregate connection/operation caps and Q=0 immediate refusal. Count closing and
pending resources. Validate lengths/counts before allocation. No per-chunk
application ACK, in-band CANCEL, unbounded drain, hidden spool or extra
construction lane. A service may accept multiple daemons without promising
multiple simultaneous C2 writers.

Main acceptance must use real separate Docker/Linux daemon and macOS service
processes. No FUSE, `/dev/fuse`, bind-mounted data, shared-data volume, container
Workspace, payload/spool/result files or container telemetry files in forward
mode. Optional local/both telemetry output is a separately tested configuration.
Future Workspace/FUSE, history, inode allocation, cloud providers and durability
are outside this issue. Keep current C2 transaction/profile semantics unchanged.

## Telemetry obligations

Preserve `Timing`, its child/attachment behavior and original Result/panic
semantics. Add a minimal operation recorder and a generic process monitor shared
by hosted components. Portable reports remain independent of optional native
collection/output, inside the same crate, preserving the unsafe-code prohibition.

Master-off avoids telemetry-specific allocations, probes, workers, files and
exports. Required product clocks, security and resource limits still operate.
CPU/RSS observations belong to process windows and may include sibling work;
do not label them exclusive operation costs. Keep local clock domains and process
incarnations explicit. Long operations retain constant-size aggregates beyond
recent-ring eviction; active operations are not evicted because they are old.

Enforce configurable byte/count/rate limits on retained recordings, encoding,
queues, producer/collector state and log segments. Account for label/Vec capacity,
cloning/conversion overlap, in-flight ownership and encoded expansion. Valid
clipped reports must remain parseable. Implement forward/local/both, owned-file
retention, fixed loss counters and bounded best-effort shutdown. Operational GC
must never touch append-only benchmark evidence. Expected telemetry failures
cannot replace a known product result, block required cleanup or create a retry
spool. A lost product response retains its normal unknown-outcome semantics.

## Execution order and ownership

1. **M0:** Record exact source/lock/profile identities, carrier and security,
   schemas, caller-to-Store authority, workload/capacity/deadline bounds, native
   execution ownership, telemetry configuration and acceptance case mapping.
   Read the actual C1/C2 public APIs at the selected revision; an old source
   snapshot in the proposal is not a current qualification pin.
2. **M1:** Preserve timer compatibility/off behavior; close retained-byte gaps
   and implement minimal recorder composition.
3. **M2:** Prove real construct/read through bridge/service and direct parity.
4. **M3:** Prove the actual Docker daemon network route and bounded relay.
5. **M4:** Complete five operations, errors and multi-daemon isolation/admission.
6. **M5:** Qualify native CPU/memory, output/retention and failure isolation.
7. **M6:** Complete workspace/platform checks, evidence and actual LOC accounting.

Do not build a monitoring framework before the first real construct/read route.
Use focused files and ordinary concrete types/functions. All production files
must be <=999 physical lines; `lib.rs`/`mod.rs` <=200 with declarations/delegation
only. Tests, fixtures and fault peers stay external to `src/`; no test-only
product hooks, features or private-source inclusion.

You are not alone in this repository. Own the new crates, telemetry changes and
their external tests/docs. Preserve the other worker's C1/C2 algorithms and
tests. A required public-contract correction needs specific evidence and
coordination before changing their files. One integration owner controls
`core/Cargo.toml`, `core/Cargo.lock` and shared public types. If agents are used,
assign disjoint files and tell them not to revert others' changes. Integrate
committed optimizer changes at explicit checkpoints, then update source pins
and affected qualification; never import its uncommitted source implicitly.

## Cargo and resource isolation

Run core commands from the implementation worktree with an explicit local target:

```sh
cd /Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs
export CARGO_TARGET_DIR=/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs/core/target
export CARGO_BUILD_JOBS=2
```

Recheck effective environment/Cargo configuration first. One build/test/Clippy
owner uses this target sequentially. Build jobs are not product construction
workers. Reuse normal Cargo/Rustup dependency caches; do not delete lock files,
clear caches or run `cargo clean` to bypass contention. A separate target avoids
shared artifact-directory contention, not shared package-cache locks or host
CPU/RAM/disk interference. Serialize dependency updates/fetches and coordinate
resource-sensitive work with the measurement owner. Use `--offline` only when
the required dependencies are already available. Keep builds `--locked`;
intentional manifest/lock updates belong to the integration owner. No vendoring,
third-party patches, registry edits or fallback to legacy product binaries.

## Verification and completion report

Map every V01-V17, T01-T16 and ENV01-ENV06 case in the verification specification
to its actual external test/deployment evidence. Distinguish deterministic tests
from native OS/Docker proof. Record real commands, identities, cleanup and
PASS/FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN; do not call unavailable metrics zero.
Read the repository benchmark and release rules before any measurement work.
Observe the measurement lock, which is **per worktree** (owner direction,
2026-09-21 — [isolation](../../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)): builds and measurements in different worktrees
no longer exclude each other, two runs in one worktree still never overlap, and no
build may take a Cargo target directory outside its own worktree. A build that
overlaps a timed phase is recorded as declared interference on the row, not
prevented; do not re-introduce a machine-global lock. Preserve all append-only
evidence.

Run focused checks while implementing and the required final core checks:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Also run the actual native telemetry feature, Linux build and Docker/host launch
checks once their names/flags exist; default tests cannot qualify those paths.
Do not run retired `tools/preflight.sh`, add an aggregate gate or claim CI green.
Update affected architecture documents with source changes and explicit pins.

Planning estimate: **+3,900 to +6,700 production LOC**, comprising C3
**+2,100 to +3,500** including thin telemetry wiring, plus telemetry crate growth
**+1,800 to +3,200**. Tests/docs/tooling are excluded. This is an estimate, not
a measured delta or permission to remove validation to meet a target.

For every commit use the repository production counter and stable classification
on the exact first-parent and final staged/committed snapshots. Include new
runtime source, exclude tests/comments/blanks, and report legacy/core/adapter
subtotals and combined totals. Record `Production LOC: before -> after (delta
+/-N)` with method/scope in commit message and handoff. Docs-only commits record
the actual unchanged product total, not a fictitious zero-sized product.

Finish with branch/base/spec/core pins; implemented public APIs and configuration;
exact checks and deployment identities; case results and evidence locations;
production LOC per commit; unresolved limitations and readiness for #193.
Do not mark #192 complete while required product, telemetry or deployment proofs
remain missing. Separate implementation completion from later performance work.
