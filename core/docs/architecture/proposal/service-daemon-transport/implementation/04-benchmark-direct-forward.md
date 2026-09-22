# Post-implementation benchmark and verification: direct versus forward

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Draft the benchmark now; implement its driver/cases **after the C3/pair 3 product
and telemetry specification is implemented and its prerequisite correctness and
deployment checks pass**. This is not a new registered benchmark family, executable
runner mode or performance result. Numeric workloads/gates and launch syntax are
NOT_FROZEN. Every benchmark/verification result here is NOT_RUN.

Read the [implementation plan](README.md), [product spec](01-product.md),
[telemetry spec](02-telemetry.md) and [implementation verification](03-verification.md).
Those establish the product that this later benchmark exercises. Existing C1/C2
measurements keep their own source, registry, timers, receipts and classifications.

## 1. Exactly two execution modes

| Execution mode | Actual path | Meaning |
| --- | --- | --- |
| `direct` | Host caller -> actual Service handler -> C1/C2 -> host SQLite | Same authorization/admission/input/output/semantic completion, without wire delivery |
| `forward` | Host caller -> stdin -> actual Linux Docker daemon -> bridge/network -> native macOS Service -> C1/C2 -> host SQLite -> network/stdout | Main intended deployment, including actual relay and delivery |

```text
 DIRECT
 host driver -> Service.handle -> C1 + C2 -> SQLite

 FORWARD
 host driver --stdin--> Docker daemon --network--> host Service.handle
 host driver <-stdout-- Docker daemon <-network---       |
                                                   C1 + C2 -> SQLite
```

There is no automatic forward-to-direct fallback. The direct adapter must enter
the same service body, not call C1/C2 underneath it. It uses trusted setup to supply
the same effective caller permissions. Forward additionally verifies its actual
connection authentication and protocol. A direct result cannot qualify that route.

Placement is separate configuration: initial forward placement is Linux Docker
daemon plus native macOS service. A future supported native daemon placement is
still `forward` with a different recorded placement, not a third execution mode.
Do not mix results from different placements or silently move SQLite into Docker.

Execution mode is also separate from telemetry configuration:

```text
execution.mode            = direct | forward
execution.daemon_placement = not_applicable | docker  (initial supported values)
telemetry.profile         = off | on:<frozen-profile-id>
telemetry.output          = forward | local | both
```

These are proposed selection fields, not existing runner flags/config grammar.
The main forward acceptance profile forwards telemetry to the host and creates
no container Workspace/payload/spool/result/telemetry files, no FUSE/device and no
bind-mounted host directory or shared-data volume. Local/both telemetry is an
explicit separate output-profile test; it does not change payload placement.

## 2. Relationship to existing benchmarks

The [Stage 6 harness](../../../../../benchmark/fs-bench-pro-storage-content/README.md)
measures public C1/C2 APIs directly under a frozen structural-complexity contract.
Its construction/edit/read/pipeline entry points and fixed timer boundaries are
not the C3 Service handler. Do not rename a Stage 6 result `direct`, alter its
registry rows or treat its PASS as service/network evidence.

The [legacy runner](../../../../../../benchmark/fs-bench-pro/shared/runner.py)
supports `host-store` with an old daemon/FUSE workflow. Its `--topology docker`
does not select this proposed forward mode; Docker-owned SQLite remains outside
the approved topology. Legacy product crates/binaries are reference only.

Reuse compatible preparation/clone manifests, identity seals, locks, independent
oracles and receipt/report conventions. Reuse fixture recipes only when they
describe the same logical request bytes/trees. Packed canonical-object artifacts
need a declared conversion recipe; they cannot become hidden handler bypasses.
Reuse native measurement mechanisms only after source/scope/availability review.

Recommended later home: a focused C3 benchmark driver under `core/benchmark/`,
with its case contract under `core/docs/benchmark/`. Freeze exact package/paths
before coding. Use one case definition and a concrete direct/forward selector
with two small delivery adapters; no generic backend registry or rewrite of the
Stage 6 framework. A new benchmark package is harness code, not a fourth product
component or a test-only production feature.

## 3. Readiness and prospective case freeze

Before benchmark-driver implementation:

1. Complete C3's relevant product/telemetry functional and platform acceptance,
   including real Docker delivery, safe disabled telemetry and bounded failure
   behavior. The implementation plan's correctness checks do not depend on this
   later performance campaign; no circular performance prerequisite is implied.
2. Reconcile the exact C1/C2/service/daemon/telemetry source tree and relevant
   qualification, lockfiles, protocol/profile and actual launch commands.
3. Commit the new case contract and link its issue as required by the
   [measurement rules](../../../../../../docs/general/benchmark_rules.md).
   This draft alone is not a registered frozen contract.
4. Freeze the full case list, exact request variants/sizes/counts/seeds, source
   and sink behavior, independent expected results, setup/cache/connection policy,
   operation boundaries, resource gates and mode/profile selection matrix.
5. Freeze one sample per declared case/arm unless an explicit owner-approved
   campaign changes that rule. Record ordering prospectively; no timing-based
   reruns, best-of selection or dropping inconvenient rows.

No arbitrary byte sizes, response thresholds or performance targets are approved
by the family outline below. Derive them from the intended workload and supported
implementation. Unfitting required work stays visible; never shrink it or lift a
timeout/resource cap to manufacture a passing mode.

## 4. Proposed matched case families

| Family | Exact variants to freeze | Shared verification |
| --- | --- | --- |
| Construct | Empty input, supported cutoff boundaries, admitted multi-frame sequential payload | Same canonical root/length, exact readback and terminal success |
| Read | Same retained root; zero, boundary and admitted large ranges | Exact ordered bytes, returned length and public error semantics |
| Inspect | Every implemented supported query and bounded listing continuation | Same typed metadata/page results; logical absence versus missing-object/provider distinctions |
| Edit | Same immutable base, no-op and supported insert/delete/replace sequences | Same new root/content; original root readable; actual C1 coordinate semantics |
| Prepared filesystem update | Same base, existing identities, sorted changed-name/inode records, retained roots | Same root and public filesystem observations; original root intact |
| Files then tree | N existing file operations followed by one bounded tree attachment | N+1 operations, bounded mapping/records, same result and explicit partial-success behavior |

The last row is a workflow, not a sixth public opcode or atomic composite save.
Creation/fixtures for existing inode identities happen in declared setup through
real public APIs. No Workspace, allocation engine, FUSE or history is introduced.

Share a semantic fixture specification and expected results across both modes.
Give mutation samples independent writable Store copies; never run forward on a
Store mutated by direct. Matching policies must produce matching roots, not just
matching returned byte lengths. Deterministic generation/source acquisition must
be charged consistently; one mode cannot receive preloaded input while the other
pays to read/generate it inside the claimed interval.

## 5. Phase boundaries and timing

| Phase | Required scope |
| --- | --- |
| Prepared master/build acquisition | Acquire and validate reusable immutable inputs and sealed binaries/images under existing rules; not a reused measurement |
| Per-sample setup | Independent closed-Store copy, native startup/open/configuration, container/connection/auth setup as declared, and recorded symmetric cache preconditioning |
| Caller operation | Begin before actual request input acquisition/consumption and encoding; end after terminal result plus declared bounded output consumption; forward includes stdin/network/stdout |
| Service handler | Same semantic handler scope in both modes, including authorization/admission, required input/replay work, C1/C2 and completion |
| Verification | Separate expected-root/byte/metadata/old-root checks; required bounded output consumption remains timed, while benchmark digest/oracle work is excluded from performance mode; hashing intrinsic to C1/C2 remains measured product work |
| Cleanup | Checked Store/process/container/output cleanup, with errors retained |
| Complete command | Actual performance selection invocation through its cleanup, including lifecycle/setup work performed by that command |

Service timing and daemon-local call duration are attribution fields; caller E2E
is measured on one caller clock. Do not subtract cross-host timestamps or add
inclusive/overlapping spans to reconstruct elapsed time. Record unattributed
time rather than hiding it in another phase.

Explicitly distinguish first-use connection setup from an established-session
operation. The initial contract must select its required connection profile;
do not automatically run a Cartesian campaign. Established authentication is
not permission to reuse warm Store/provider/file data or prior result buffers.
Do not probe measured roots during health checks or prime expected-result ranges.

Freeze input-source mode: if generated, charge its required generation consistently;
if file-backed, charge required reads and apply the declared cold/residency contract.
A prepared clone is setup reuse, not cold proof. Source, Store and transport-session
cache/lifetime states are separate receipt fields. Unsupported cold acquisition
or credited resident work is INELIGIBLE under the applicable measurement contract.

Apply current repository limits to this new family: complete performance command
<=15 seconds, or an explicitly declared exception <=25 seconds; verification
normally <15 seconds with a 60-second hard budget. Reuse matching permitted proof
or retain over-budget/unrun work with its reason; never move required timed work
outside the operation to fit. Build/preparation reuse follows seals, with any
cost actually incurred by the selection still reported.

The Stage 6-specific `declared_ns + 250 ms` allowance is **not inherited here**.
Do not replace actual C3 container/process lifecycle cost with that constant.
Preserve historical Stage 6 receipts and rulings under their own contract.

## 6. CPU, memory, transport and telemetry treatment

Record caller elapsed, service/daemon local durations, actual input/output bytes,
encoded/control bytes, frame counts, logical exchanges, connections/handshakes,
admission refusals and outcomes. Distinguish frames, application exchanges and
physical network packets; a streamed operation is not a literal one-packet RTT.

Process scopes differ: direct service work shares its caller process; forward
has a separate host service, daemon and driver. Record each enclosing scope and
its baseline/window. Never subtract their independent RSS peaks and call the
difference bridge memory, or attribute all shared process CPU to one operation.
Report container domains separately from daemon RSS and avoid overlapping sums.

Account source/sink, relay, codec, replay, record, core and telemetry allocations;
also report applicable socket/kernel/file-cache/backing and Store disk domains.
Lifetime peaks are not phase peaks. Phase/resource gates need qualified acquisition
and boundary coverage; missing required observations mean incomplete evidence,
not zero usage. No unbounded sampler/report history is permitted.

The operational 100 ms sampler is not automatic benchmark evidence for short
operations. Freeze external/phase-qualified measurements separately, including
sampling gaps and exact/sampled/accounted distinctions. Do not modify the product
algorithm, enable private hooks or add workers to make instrumentation easier.

For correctness, verify the supported representative requests in both execution
modes with telemetry off and on. For performance, freeze one matched telemetry
profile across the selected mode comparison. An additional on/off overhead study
is a separately declared selection, not an automatic four-way campaign or retry.
Telemetry `output=forward` is not execution `mode=forward`.

Master-off preserves mandatory operation behavior. Expected telemetry failures
preserve product results, but a row requiring missing telemetry cannot pass its
evidence gate. Operational retention must never delete benchmark receipts,
verification output or failed attempts; use distinct output namespaces.

Telemetry-off does not disable the benchmark's external caller timing. A field
intentionally absent under the registered off profile is not automatically missing
required evidence; freeze which observations remain mandatory for that treatment.

## 7. Verification matrix and applicability

Use the implementation [verification cases](03-verification.md) as prerequisites,
then map the frozen benchmark variants to this comparison matrix:

| ID | Case class | Direct | Forward |
| --- | --- | --- | --- |
| BV01 | Five operations and files-then-tree workflow | Exact service semantics and independent oracle | Same semantics through actual daemon/network path |
| BV02 | Authorization, malformed logical inputs, unsupported profile, capacity/replay limits | Same effective permissions; typed rejection | Same plus actual authenticated connection |
| BV03 | Wrong wire/version/state, truncated/oversized frames | NOT_APPLICABLE: no wire decoder | Real endpoint rejection; external malformed peer precisely labelled |
| BV04 | Slow source/sink, input/output failure and cleanup | Handler source/sink behavior | Additionally daemon relay/socket/stdout behavior and bounded pressure |
| BV05 | Early refusal with unread stdin; connection loss around completion | NOT_APPLICABLE for network-specific failure | Close unsynchronized input; actual known/unknown disposition, no replay |
| BV06 | Several daemons, global C/A full and one beyond | NOT_APPLICABLE for multi-daemon topology | Real caller/response/Store isolation, Q=0 refusal and actual writer ownership |
| BV07 | Telemetry off/on and diagnostic-sink failure | Same product result and declared omission | Same plus stdout/diagnostic separation and actual collector path |
| BV08 | Old roots, ordinary reopen and prepared-source integrity | Preserved | Preserved; no durability implication |
| BV09 | Process/image/network/mount evidence | Native direct identities; daemon/image fields N/A | Actual production binaries, peers/image; no shared-data or payload-file shortcut |

Wire faults may require an external peer because the real daemon encoder should
not produce invalid frames. That proves endpoint behavior, not traversal through
the daemon. Retain separate actual-daemon relay cases; do not claim bypassed
paths were exercised. N/A has a reason and is not zero resource usage or PASS.

A claimed fault point needs evidence. An arbitrary connection kill does not prove
the after-save/before-response case. Keep the service alive for ordinary completed
root/readback checks; killing it during a save does not create a crash-durability
guarantee for the current no-sync/no-WAL profile.

Compare common successful/semantic outcomes, not impossible transport equivalence.
If the forward response is lost after a successful save, the service may know
success while its daemon knows only unknown; a direct caller can know success.
That difference is expected delivery semantics, not a parity defect to hide.

Verification checks exact output using an independent expected-data recipe; two
modes agreeing on the same bug is insufficient. Keep product outcome, cleanup,
telemetry completeness, cache eligibility and verification/performance status
separate. Unknown persistence state must not trigger guessed deletion or replay.

## 8. Receipt identity and reuse

Freeze a schema with at least:

- Contract revision, case/variant/seed, request schema and fixture/oracle hashes.
- Source tree/dirty state, core/service/daemon/telemetry identities, locks,
  compilation/dependency seals and actual binary hashes.
- Execution mode, daemon/service placement, process identities/incarnations,
  host/container versions, image and observed route/peers where applicable.
- Service security/profile/configuration and effective permissions without secrets;
  Store/canonical policy, capacities, construction-worker and concurrency settings.
- Connection/setup and source/Store cache profiles; copy/preparation/seal/reuse data.
- Telemetry enablement/output/collector/profile identity, observation scopes,
  boundaries/gaps, reported losses and timing/resource completeness.
- Every phase, full invocation wall, expected/actual outcome, oracle result,
  cleanup disposition, row applicability/status and all unrun/failed selections.

Mode, placement and telemetry-profile identity participate in proof reuse keys.
A direct PASS cannot supply forward proof, nor a Stage 6 core receipt service
proof. Accept reused verification only with verification PASS and cleanup PASS,
matching declared identities/required coverage, and compatible schema, hard-limit
and wall checks. Record reuse and explicit omissions. Fresh runs use fresh output
paths and samples; no overwriting/relabeling old receipts or replaying them as new.

Fresh process/container IDs and timestamps are run-custody fields, not values that
must literally match an older receipt's PID/time. Proof reuse compares the declared
semantic/build/configuration identities and exact coverage while retaining each
run's original custody separately.

## 9. Later implementation sequence and allowed claims

1. Finish prerequisite product/telemetry correctness and environment work.
2. Commit/freeze this new family's exact case contract and selection matrix.
3. Implement the shared request fixture/oracle plus direct delivery adapter.
4. Add actual daemon/bridge forward delivery and route verification without
   changing service logic or reinterpreting existing Stage 6 rows.
5. Verify semantic, failure, isolation, telemetry and resource cases, then execute
   the declared performance selection under its measurement lock.
6. Publish raw receipts, derived reports, reproduction commands and every gap.

Builds, verification and performance obey the per-worktree locking rules (owner
direction, 2026-09-21 — [isolation](../../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)); do not interrupt another owner's run,
and do not overlap two runs inside one worktree. No CI, aggregate preflight or
new all-purpose runner wrapper is introduced. Proposed CLI flags must not appear
as runnable commands until the actual parser and supported options exist.

Direct versus forward is an **execution-treatment comparison**, useful for
end-to-end delivery/resource attribution, not a measured core algorithm speedup.
The initial forward treatment includes daemon relay, process/platform placement,
authentication and network work; it is not a pure isolated socket-cost experiment.
Single-sample rows do not support percentiles. No cross-mode performance PASS
threshold or speed/memory reduction is established by this draft.
