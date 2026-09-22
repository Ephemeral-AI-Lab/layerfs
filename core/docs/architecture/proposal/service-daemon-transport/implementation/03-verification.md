# C3 implementation verification and evidence

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Tests, deployments and performance qualification in this implementation
specification are **NOT_RUN** until linked evidence exists.

C3 here means pair 3 / [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181):
the service, bridge and daemon foundation, plus the single-crate telemetry
extension. It does not rename C1/C2 or qualify later Workspace/FUSE/history.
Read the [milestones](README.md), [product specification](01-product.md) and
[telemetry specification](02-telemetry.md). Those own implementation scope;
this file defines closure evidence. The [operation catalog](../07-public-operations.md)
and [resource plan](../04-resource-and-verification-plan.md) supply semantics and
accounting. Exact protocol/native feature choices must match the implementation.

## 1. Admission before executing acceptance work

At M0 freeze the source/dependency identities, selected network carrier and
credential/authorization policy, host/container route, supported Store/profile,
operation inputs, expected results, byte/count/replay limits and deadlines.
Freeze telemetry modes, collection capabilities, allocation/retention accounting
and selected numeric profile. Proposed numbers are not already verified defaults.
Name the affected Stage 7 findings and their resolution or explicit disposition.

Map the IDs below to external test files and actual launch/verification commands
as those files are implemented. Do not create empty test scaffolds or claim a
matrix row passed because an API compiles. No test-only core path, feature, fault
switch, private source inclusion or replacement algorithm is permitted.
Malformed peers, fixtures, process control and assertions live outside `src/`.

Keep three evidence classes distinct:

- **Deterministic correctness:** checked decoding, state transitions, pure typed
  aggregation, public API composition, serialization and capacity boundaries.
- **Deployment/platform evidence:** actual processes, network, native collectors,
  OS resource ownership, slow peers, cleanup and exit behavior.
- **Performance evidence:** a separately registered/frozen workload and measurement
  contract. Correctness and direct-call timings cannot substitute for this.

## 2. Product acceptance matrix

IDs in this document are local to this implementation specification. The M0 case
map must relate them to the parent resource-plan checklist; matching numbers do
not imply matching cases between documents.

Unless explicitly labelled secondary, execute these against the actual Linux
Docker daemon and separate native macOS service. The host driver streams stdin,
consumes stdout incrementally and separately drains bounded diagnostics. The
service executes real C1/C2 against its host SQLite Store. There is no FUSE device,
mount, host-directory bind mount, shared-data volume or container Workspace,
payload, spool or result file. Ordinary image/configuration files are allowed.

| ID | Exercise | Required observations |
| --- | --- | --- |
| V01 | Launch, authenticate and route a valid operation | Record both production processes, image/config identities, endpoint/observed peers and handler result; prove daemon/network delivery rather than a host direct call |
| V02 | `ConstructFile`: empty, accepted cutoff boundaries, multi-frame source | Exact root/length and readback under Store-derived policy; input EOF only after valid completion; no whole-input daemon buffer; result only after successful C2 finish |
| V03 | `ReadFile`: zero length, boundaries, out-of-range and large admitted range | Exact bytes/typed error, bounded sink, terminal success required; partial bytes never count as a successful read |
| V04 | `Inspect`: each frozen supported query, absent path/object and paged listing | Correct typed metadata/continuation, count/byte limits and combined identity/attrs where specified; preserve available absence/provider distinctions |
| V05 | `EditFile`: no-op, supported insert/delete/replace and invalid coordinates | Exact C1 current-result/order semantics, stable replay, old root readable; reject earlier-replacement overlap/overflow and over-cap input without hidden rewrite or spool |
| V06 | `UpdatePreparedFilesystem`: existing identities and retained references | Correct changed-name tree update, original root intact; reject unsupported allocation/scope or malformed order; no invented same-save provider |
| V07 | N file saves followed by bounded tree attachment | Correct final root, capped result-root/record retention and N+1 logical exchanges; earlier successful saves can remain on later failure; no false atomicity claim |
| V08 | Header/body fragmentation, truncation, illegal state/opcode/ID and arithmetic extremes | Bounded rejection/close before unchecked allocation or dispatch; input/result totals exact; zero-byte data-frame abuse and surplus frames refused |
| V09 | Denied principal/Store/operation and unsupported version/profile | No prohibited effect, credential/data leakage, fallback or retry; authority is not taken from a caller-asserted field |
| V10 | Slow upload, slow download and blocked driver stdout/stderr | Bounded application/socket/diagnostic ownership, pressure reaches source, no busy spin or full-output accumulator |
| V11 | Early host refusal with unread daemon stdin body | Both affected sessions close unless synchronized; no unbounded drain and no interpreting leftover bytes as another request |
| V12 | Disconnect/deadline before input completion and around save finish | Known versus unknown outcome follows available evidence; no automatic resend/reconnect-and-replay or guessed deletion; old successful roots preserved |
| V13 | One service, multiple real daemons, full C/A and one beyond | Response/principal/Store isolation, global admission and Q=0 immediate refusal; pending/closing resources count; no added writer/construction lane |
| V14 | Close/open Store and verify successful roots | Read exact old/new data through reopened public APIs; ordinary reopen does not prove crash/power-loss durability |
| V15 | Failure and resource cleanup | Preserve original error and cleanup failure; release established owned resources once; uncertain ownership is not cleaned on a guess |
| V16 | Inspect actual container configuration/artifacts | No prohibited mounts/shared volumes/application data files; all payloads follow driver -> daemon -> network -> service and return network -> stdout |
| V17 | Secondary direct parity on the same supported inputs | Same authorized handlers, canonical results and meaningful non-transport errors without framing; cannot replace V01–V16 route evidence |

Fixtures compare exact bytes and expected canonical/public behavior, not just
successful return codes. External malformed peers/controllers may exercise the
real network endpoint when the daemon's valid encoder cannot emit bad frames;
label that coverage precisely. It is not evidence that a bypassed daemon path
was exercised. Include the actual daemon for its relay/session/error obligations.

N:1 client topology is not concurrent-write qualification. Keep one permitted
construction producer and the actual C2 ownership behavior. Successful requests,
admission refusals and active/closing limits all appear in the results.

## 3. Single-crate telemetry acceptance matrix

Extend only `layerfs-telemetry` as specified in [02](02-telemetry.md). Portable
reports/aggregation remain separate modules from optional native collection/I/O.
No second telemetry package, third-party patch, vendored dependency, registry
edit or benchmark-source import satisfies acceptance. If safe published APIs do
not expose a required native observation under the crate's safety contract, report
that capability blocked/unavailable; do not insert an unreviewed unsafe escape.

| ID | Exercise | Required observations |
| --- | --- | --- |
| T01 | Existing timer API and result/panic behavior | Existing public tests continue to pass; outer composition preserves original Result, child/attachment completeness and !Send/!Sync lifetime constraints |
| T02 | Master-off and independently disabled measurements | No telemetry observations, workers, buffers, files or exports; lazy/static labels; disabled differs from zero and capacity-skipped detail; required product deadline/enforcement work remains active |
| T03 | Identical product requests with telemetry on/off | Same results, roots and failure/unknown classes; expected telemetry/output failure cannot replace product error or skip required cleanup |
| T04 | Typed CPU/memory window arithmetic | Cumulative counter differences, discontinuities, unavailable fields, baseline/final/max and coverage correct; sampled maximum not mislabelled exact peak |
| T05 | Long windows, ring wrap, overlapping operations | Deterministic timestamped observations preserve constant-size active summaries beyond history expiry; gaps/boundary ages explicit; sibling CPU/RSS not attributed exclusively to one operation |
| T06 | Active/recording limits and guard cleanup | Release slots on success/error/unwind; no live-window age eviction, indefinite wait or output from Drop; excess optional detail does not reject product work |
| T07 | Timing retained capacity and conversion overlap | Count labels/Vec capacity, tree conversion/attachment, completed ownership and clones; structural node/depth/label limits are not accepted as proof of byte pools |
| T08 | Encoded report limit and carrier headroom | Valid bounded summary/projection, explicit omissions, required identity/outcome preserved; no truncated JSON, hidden full-ring serialization or changed mandatory operation result |
| T09 | Queue count/byte overflow and failed export | Both caps enforced, oldest eligible drop/incoming refusal per contract, fixed loss/gap counters, in-flight bytes charged; no recursive logging or replay queue |
| T10 | Forward/local/both and co-hosted roles | Shared sampler/recording; copies/references/current writes included; aggregate rates across sinks; producer+collector budget validated, not silently doubled |
| T11 | Producer registry churn and collector limits | Bounded connected/closing/incarnation state, no tombstone/history leak; rate/burst/queue arithmetic and explicit loss under oversubscription |
| T12 | Segment count/size, expiry and conflicting writers | Active segment included; owned oldest eligible segment retired before exceeding budget; one writer per namespace; unrelated files never removed |
| T13 | Denied path, full disk, rotation/deletion failure and open readers | Sink stops or records declared loss without extra segments/fallback; retained handles and staging charged; logical file-length cap not called a physical-disk guarantee |
| T14 | Benchmark evidence beside operational output | Rotation/drop cannot touch append-only receipts, failed attempts or historical evidence; exhausted evidence capacity refuses/marks new proof incomplete without deleting old data |
| T15 | Shutdown and a stalled writer | Stop new telemetry; best-effort drain within allowance, remaining loss accounted; two seconds not called guaranteed syscall/thread preemption or lossless delivery |
| T16 | Native/default feature and dependency graphs | One crate; portable default does not start or acquire native facilities; native I/O explicit and configured; no unsupported provider fallback or test-only product feature |

For T05, public typed observation sequences can cover long durations without
sleeping for a minute or introducing a test clock into product timing. This
proves aggregation behavior, not native sampler scheduling. Short controlled
runtime runs separately verify actual cadence/gaps and resource ownership.
State any unrun default-duration stress proof rather than changing measurement
budgets or pretending synthetic observations are OS measurements.

## 4. Platform and physical evidence

| ID | Deployment check | Required evidence |
| --- | --- | --- |
| ENV01 | macOS host process collection | Actual supported safe source/API, units, process identity/incarnation, cumulative CPU/RSS coverage and unavailable fields; collection overhead recorded separately |
| ENV02 | Linux Docker daemon process and optional cgroup scope | Real process/container identity, source/version, anonymous/file/socket/kernel domains where available; process RSS and enclosing cgroup values not added as independent memory |
| ENV03 | Producer/collector forwarding | Actual bounded stderr/result/local-host path selected by contract, ordinary diagnostics separated, report identity/loss preserved; primary forward-mode container creates no telemetry files |
| ENV04 | Separately selected local/both mode | Owned paths/permissions, rotation, page-cache/disk costs and cleanup tested in that environment; explicitly outside the primary no-container-files forward-mode run |
| ENV05 | Allocation and concurrency envelope | Actual simultaneous producer pools, collector queue, encoding overlap, runtime stacks/OS buffers and co-hosted role totals; distinguish telemetry-owned targets from RSS |
| ENV06 | Real shutdown and process exit | Slow/broken outputs and actual cleanup disposition; report blocking-I/O limitations and lost telemetry without claiming preemptible disk calls |

Pure parsing/aggregation tests cannot qualify ENV01–ENV06. Process-wide samples
cannot establish per-operation exclusive CPU or verifiably reset phase peaks.
Native capability or counter absence is reported as unavailable, never zero.
Safe collection support and Cargo portability must be established for each real
target; successful macOS tests do not establish Linux/container support.

## 5. Commands by implemented scope

Run commands from the repository root. These are independent workspace/package
checks, not a new aggregate gate. Do not execute proposed-crate commands before
the actual members exist. Every build/test dependency resolution stays locked.

For the existing telemetry crate and its portable changes:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-telemetry
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-telemetry --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml -p layerfs-telemetry -- --check
```

After bridge/service/daemon are real core members, check the touched packages:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-service -p layerfs-daemon
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-service -p layerfs-daemon --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml -p layerfs-bridge -p layerfs-service -p layerfs-daemon -- --check
```

Once the optional native telemetry feature is declared, record and run its exact
implemented feature selection and target commands. Do not invent placeholder
feature names or claim a default-feature test covers native collection. Inspect
standalone portable and consuming-application dependency graphs: Cargo feature
unification can enable native dependencies in a shared application instance.
Record the exact Linux/container build and daemon/service launch commands when
implemented; guessed flags or direct-example runs are not deployment proof.

At final integration, run the replacement workspace checks and guard checks:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Formatting has no dependency-resolution `--locked` flag. Example compilation is
not execution: separately record the real direct example and deployment checks
with their actual supported arguments. Guard checks must cover any new production
paths/formats; maintain <=999 physical lines and <=200 declaration/delegation-only
`lib.rs`/`mod.rs`. Document-only work does not claim these runtime checks passed.

## 6. Results, measurement and release handoff

The [later direct/forward benchmark draft](04-benchmark-direct-forward.md) defines
how new service-level cases can compare the two execution treatments after this
implementation's prerequisite checks. It does not replace these correctness and
platform cases or reuse Stage 6 component receipts as service/network proof.


For every required ID record PASS, FAIL, INCOMPLETE, INELIGIBLE or NOT_RUN with
exact scope, source/dependency/configuration identities, command, expected/actual
result and cleanup disposition. Keep original operation outcome separate from
telemetry/evidence completeness. Attach relevant logs/receipts and all nonpassing
lines; do not rewrite or promote earlier evidence after source changes.

Performance remains NOT_RUN until its case contract is frozen and registered as
required by the [measurement rules](../../../../../../docs/general/benchmark_rules.md).
This specification creates no benchmark harness, registry or numerical performance
gate. Reuse prepared setup/builds/images only outside measured work, preserve
matched cache states, measure complete command/cleanup and keep fresh append-only
outputs. Respect the measurement lock, which is per worktree (owner direction,
2026-09-21 — [isolation](../../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)). Do not launch two runs inside one worktree
concurrently; a run in another worktree is not excluded, and the interference it
causes is recorded on the row rather than prevented.

No CI or aggregate pre-push/preflight gate is reinstated. Checks for the changed
workspace remain required, with exact omissions and reasons in the handoff. The
final milestone cannot close from direct parity, core-only qualification or a
small smoke test in place of the required actual Docker/network/OS evidence.
