# Resource bounds, round trips and verification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is the resource and evidence plan for the service/daemon/bridge foundation
under [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181). It does not
register a benchmark family, approve numerical performance gates, or report a
test result. The operation protocol is in [03](03-operations-and-transport.md);
component and file ownership are in [01](01-architecture-and-portability.md) and
[02](02-file-layout-and-boundaries.md). All limits below are proposed inputs to
the implementation contract. No transport code or cloud provider exists by virtue
of this document.

The [telemetry and retention proposal](08-telemetry-and-retention.md) defines
optional process observation, operation-report composition, disable behavior and
bounded output/cleanup. Its candidate 100 ms operational sampler is not a new
benchmark instrument contract or proof of a phase peak. Include enabled telemetry
in resource accounting and report unavailable/incomplete required evidence.

**Primary acceptance is network delivery between separate processes:** an actual
Linux Docker daemon connects to the actual native macOS host service. A bounded
host test driver supplies daemon stdin and consumes daemon stdout; inputs and
results cross the selected network carrier between daemon and service. This path
uses no FUSE mount/device, host-directory bind mount, shared-data volume, or
container Workspace/payload/spool/result files. The Store and any explicitly
owned service replay/scratch stay on the host. Direct invocation is a secondary
semantic-parity diagnostic using the same handlers; it cannot replace any
required daemon/network case.

Use one bridge library and one initially selected network carrier for this
acceptance. Carrier and authentication details remain decisions to freeze, not
permission to compare different routes as one treatment. One service can admit
multiple daemons within explicit connection/active-handler limits and Q=0;
multiple clients do not imply multiple C2 writers.

## 1. What "fastest" means here

The design objective is to remove unnecessary serialized exchanges, copies,
allocation and repeated work while retaining required validation. There is no
measured fastest implementation yet. Optimize only after a reproducible baseline
exists; never obtain speed by dropping authentication, checks, cold work or
failure reporting.

| Mechanism | Proposed property | How to falsify it |
| --- | --- | --- |
| Established bridge session | One logical request and one terminal response per read/construct/edit operation | Count application requests, terminal responses and serial dependencies; distinguish actual data frames and TCP acknowledgements |
| Upload | BEGIN and bounded body chunks can be pipelined; no stop-and-wait application ACK per chunk | Slow/high-latency external peer exposes frame-by-frame serialized waits |
| Read response | Stream bounded result chunks; do not collect a whole file merely to return it | Increase requested logical bytes while examining live buffers and required sink accounting |
| Core I/O | C1/C2 provider and consumer calls remain service-local | Object/pack/header RPC count on bridge must be absent from the designed route |
| Encoding | Binary bytes, reusable bounded buffers, no base64 or bridge recompression of file payload | Count encoded bytes, copy volume and CPU; retain required TLS/authentication and canonical hashing costs |
| Connection lifetime | Reuse an established authenticated session when the operation contract permits | Connection opens/handshakes do not silently occur once per operation |
| Secondary direct invocation | Same operation handler, no wire encoding or socket; semantic attribution only | Trace route and require the same validation/authorization; direct results cannot satisfy primary network acceptance |

One logical exchange is not necessarily one physical network RTT or one syscall.
Flow control, congestion, framing, TLS and streaming can require many packet
exchanges. For data of size U, at rate B, a rough explanation is latency floor
plus U/B plus the required core work; it is not a benchmark prediction. A small
window can limit throughput on a high-latency link. Size the window from the
supported environment and measured bandwidth-delay behavior without lifting its
memory bound to conceal a failure.

Constructing a new file and subsequently attaching its published root to a
prepared filesystem tree is **two logical operations** in the initial supported
composition. Do not advertise it as a one-exchange filesystem save. A combined
atomic unpublished-content/tree operation requires its own C1/C2 visibility proof and
cannot be assumed from the existence of one socket. N independent file
construction/edit operations followed by one bounded tree attachment cost N + 1
logical exchanges and O(N) caller result-root/record metadata under explicit caps.
Additional tree-update batches add exchanges and intermediate roots; no atomic
multi-file or constant-exchange claim follows from this initial operation set.
The [legacy comparison](05-v0.1.6-comparison.md) recommends evaluating one bounded
network submission with sequential internal saves separately from that atomic
variant. Fewer network waits do not imply fewer local save operations or SQL
transactions; one save operation can use multiple bounded SQL transactions.

Connection setup is reported separately from established-session operation
latency. Reusing a connection is not permission to reuse forbidden warm Store
data, decoded payloads or earlier measured output. Cache contracts still apply.

## 2. One vocabulary for limits

The implementation must validate every configured limit and all derived arithmetic
before allocating or performing an operation. A peer cannot raise service limits.
Use the lower applicable negotiated/service bound and explicitly refuse a request
that needs unsupported capabilities. Unsupported version/policy is a failure, not
an invitation to retry another path.

| Symbol | Meaning | Required ownership/enforcement |
| --- | --- | --- |
| F | Maximum DATA payload bytes in one frame | Decoder checks header length before allocating; fixed header is separately bounded |
| W | Per-direction, per-connection application byte window | Bound queued upload and download bytes, including bridges to synchronous C1/C2 handlers |
| C | Maximum simultaneous connections per process | Include unauthenticated/handshaking/closing connections until resources are released |
| A | Maximum globally active operation handlers | Enforce before creating handler-owned core/replay/scratch state; obey per-Store writer authority |
| Q = 0 | No waiting request descriptors in the initial design | Immediate refusal when active capacity is unavailable; nonzero queue/scheduler deferred |
| P | Maximum control/operation metadata bytes per admitted request | Parse with aggregate byte and record limits; bound nested strings/lists and decoded expansion |
| D | Operation deadline | Includes the declared operation scope; never extend automatically after progress or a timeout |
| R_request / R_response | Per-opcode total input and output byte ceilings | Distinct from F and W; a streaming operation may legitimately exceed the window |
| R_replay | Maximum stable replacement bytes retained for a replay-required edit | Charge separately from transport windows; explicit refusal before allocating beyond it |
| N_records | Per-opcode record/item/name/edit limit | Independent from encoded byte length; validate counts and checked products |
| S | Declared handler scratch/backing allowance | Covers required replay/order resources outside bridge buffers; not an unbounded temporary file |

Validate relationships such as positive F/W/C/A, F fitting the receive strategy,
non-overflowing sums/products, and policy-specific operation capacity. Reuse
Store-derived C1 policy/capacities. The Stage 7 malformed-capacity/resource findings
must be resolved or explicitly constrained before an adapter exposes those knobs
to untrusted requests. A controlled default-only prototype does not close them.

No supported total request size, deadline, auth mechanism or transport buffer
implementation is selected by the illustrative arithmetic below. Freeze these
before implementation/qualification, with the required operation/workload range.
An existing registered workload cannot be shrunk to make a new cap fit.

## 3. An illustrative memory calculation, not an admitted default

An initial candidate for discussing bridge buffers is:

```text
 F = 64 KiB       DATA frame payload ceiling
 W = 256 KiB      each direction's application window
 C = 8           live connections per endpoint process
 A = 1           active handler in the illustrative single-Store baseline
 Q = 0           no waiting-operation queue; fixed initial scope
 P = 32 KiB      operation/control metadata envelope
```

Q=0 is the selected simplification for the initial proposal, not a tuning knob.
The other numbers above remain illustrative rather than approved defaults.
A=1 is an explicitly limited starting profile, not a claim that all read-only
traffic must permanently serialize or that one connection equals one compute
worker. Any change to concurrency has to preserve the registered treatment and
the single-construction-worker rule; never add workers to turn a miss into PASS.

Suppose each connection owns at most two W-byte queues and one F-byte codec
buffer in each direction. Charge metadata separately:

```text
 Bridge payload/codec buffers per endpoint process
   <= C * (2*W + 2*F)
   = 8 * (512 KiB + 128 KiB)
   = 5 MiB

 Additional operation/control metadata allowance
   <= (C + Q) * P
   = 256 KiB

 One service process plus one daemon process at these illustrative caps:
                        payload/codec buffers <= 10 MiB
                        metadata allowance  <= 512 KiB
```

For one service with N_daemons separate daemon processes, count the service once
and sum each daemon's actual bounded allocations: `M_total = M_service +
sum(M_daemon_i) + M_host_driver`, plus separately owned system domains. The
10 MiB two-process example is not the aggregate allowance for arbitrarily many
daemons. Freeze the tested daemon count and per-process/aggregate limits. Service
C and A apply across all its clients; opening another daemon cannot bypass them,
and each daemon's local source/sink and baseline memory still count.

This bounds only the stated application allocations. It is **not total RSS**.
An implementation with extra relay queues, vectors, retained frames or encoding
buffers must charge those too; calling them transport internals does not exempt
them. Metadata P must account for decoded representation, or the implementation
must derive and charge its separately bounded expansion before use.

```text
 M_service = M_base
           + M_bridge
           + M_socket_and_TLS
           + A * (M_core + M_replay + M_scratch + M_request_state)
           + M_store_indexes_and_caches

 M_daemon  = M_base
           + M_bridge
           + M_socket_and_TLS
           + M_control_input_and_output
           + M_source_and_sink

 Future pair 1 adds Workspace overlay, backing, FUSE and kernel-cache domains.
```

Measure socket/kernel memory instead of silently treating it as zero. The daemon
headless input/output route must propagate backpressure: a blocked stdout/control
consumer cannot cause an unbounded response accumulator. The test driver must
consume output in bounded chunks too, with its own memory reported separately.

## 4. Streaming versus replay

Complete-file construction has a streaming `Read` entry point. The bridge can
supply bounded input while preserving exact declared length and terminal-input
validation before `SaveOperation::finish`. C1/C2's own memory remains additional
and profile-dependent; transport streaming does not make those allocations free.

Known edits can read replacement input more than once through `EditSource`.
The initial proposal in [03](03-operations-and-transport.md) therefore retains a
bounded replacement body under R_replay. Its memory is O(retained replacement
bytes) up to that cap; it is not O(F) merely because the upload was framed.
Do not fetch each replay range back from the daemon: that would introduce an
input-dependent number of service-to-daemon round trips.

Oversized replay-required operations need an explicit additional design, such as
a qualified caller-owned backing capability, or are refused as unsupported.
There is no automatic disk-spool fallback. Any future backing must have declared
disk/residency bounds and include its preparation and required reads in the
proper operation phase. Full-file spool/page-cache growth is not excused by a
bounded Rust heap. Required benchmark cases that do not fit remain NOT_RUN or
failing with their reason; do not reduce them or move required work into setup.

## 5. CPU and scheduler bounds

Choose an explicit native execution owner for synchronous core work. An operation's
`StoreProvider` is `!Sync`; its `TimingScope` is `!Send`/`!Sync`. Keep their lifetime
on one owned execution context, with bounded adapters to full-duplex socket I/O;
do not freely migrate or share these values across generic worker tasks. Count
every adapter queue and execution thread/task. Pair 1 must additionally qualify
reads/control during bulk saves: the single-active-operation illustration is not
FUSE responsiveness evidence. A small fixed set of bounded connections is an
option to evaluate before introducing general multiplexing or a scheduler.

For a well-formed operation with U input bytes, V output bytes and n metadata
records, the proposed bridge parsing/framing work is O(U + V + n), excluding
core algorithms, cryptography and caller data generation. This is a design target
to verify, not a measured bound of code that has not been written.

- Validate fixed headers before variable allocation. Reject over-limit lengths,
  count/length arithmetic overflow and unsupported opcodes promptly.
- Bound total body bytes, metadata elements, connection count and handshake work;
  an unlimited stream of tiny frames must not yield unlimited admitted work.
- Require monotonic byte offsets/counters or an equivalent exact consumption rule;
  no duplicate chunk replay, sorting or whole-payload rescanning in the decoder.
- Wait on I/O readiness/backpressure; no busy-spin progress loops or status polls.
- Preserve edit order and segment boundaries. Coalescing edits can change C1's
  canonical output and is not a transport optimization.
- Charge core hashing, authentication, compression/decompression and physical
  dependency traversal as real operation work. Do not duplicate them merely for
  transport diagnostics; do not remove required integrity checks.
- Keep timing nodes and diagnostic counters bounded by operations/coarse phases,
  not one retained trace node per frame or object.

D is not a preemption mechanism for synchronous core work. If cancellation is
only observed at I/O/chunk boundaries, record that granularity. A timeout cannot
prove that a handler stopped or that a save did not publish. After a mutating
deadline/disconnect, preserve unknown outcome when completion was not established;
do not spawn a replacement operation or add a worker to hide the stalled one.

## 6. Verification matrix to turn into the implementation checks

This is a proposed correctness/resource checklist, not an implemented test suite
or a frozen performance registry. All results are **NOT_RUN** in this document.

| ID | Case | Required result/evidence |
| --- | --- | --- |
| V01 | Primary separate-process Docker-daemon/network/macOS-service path | Real host driver streams bounded stdin/stdout through the actual daemon; network endpoint receives the operation and the native host service executes C1/C2 against SQLite; no direct-client bypass or shared-file shortcut |
| V02 | Empty, cutoff-boundary and multi-frame construction | Exact result and framing; no whole-input bridge buffering; actual current profile |
| V03 | Range read, including empty/end/out-of-range cases | Exact bytes/error; bounded sink and correct terminal result |
| V04 | Supported known edits and no-op | Same public C1 semantics, old root preserved, replay cap and source reads accounted |
| V05 | Prepared filesystem update | Changed-name semantics and retained content roots; no invented allocator or same-save reader |
| V06 | One/N files then tree attachment, including intended N boundary | N + 1 exchanges for one bounded attachment, capped root/record metadata, correct final tree, no false atomic multi-file claim |
| V07 | Truncated/malformed/oversized/overflowing frames; host refusal with unread daemon stdin BODY | Typed refusal or bounded close on both framed sessions; no unchecked allocation, unbounded drain, residual-body command reuse or dispatch |
| V08 | Unsupported version/profile and denied access | Refused before prohibited effects; no fallback or retry |
| V09 | Slow uploader | Bounded receive/relay/socket accounting, admission held correctly, no busy loop |
| V10 | Slow reader/control consumer | Bounded response/relay/stdout buffers; pressure propagates to producer |
| V11 | Disconnect/local cancellation before complete input | No false success; correct definite/unknown disposition and cleanup ownership |
| V12 | Disconnect around save finish / partial result | Potentially completed mutation stays unknown; no resend or deletion on a guess |
| V13 | One service with multiple real daemons up to limits and one beyond | Distinct authorized callers/requests retain response and Store-access isolation; aggregate service C/A and Q=0 refusal hold, including handshake/closing resources; no extra writer/construction lane is inferred |
| V14 | Limit boundaries and extreme settings | Validated aggregate arithmetic, predictable refusal, no OOM/swap accepted as success |
| V15 | Normal close/open and old/new root readback | Existing logical data intact; not a process/power-loss durability claim |
| V16 | Cleanup and final resource release | Connections, buffers and operation-owned resources released; errors retained |
| V17 | Mount-free container and actual payload route | No FUSE mount/device, host-directory bind mount, shared-data volume or container Workspace/payload/spool/result files; bytes traverse driver stdin -> actual daemon -> network -> service and return via network/stdout |
| V18 | Secondary direct semantic parity for the same supported inputs | Same canonical roots, bytes and meaningful non-transport error classes from the same authorized handlers; zero wire encoding on the direct route; does not replace V01–V17 network-path evidence |

V01–V17 are scoped to the actual daemon/network/service deployment. Tests that
need malformed input or controlled disconnects use external peers/controllers at
the relevant real endpoint; record any case that cannot traverse the supported
daemon input instead of calling a handler directly and claiming route coverage.
V18 is separately labelled secondary parity evidence. Both routes share handler
bodies; do not introduce a second implementation or a new aggregate harness.

Use real public C1/C2 bodies and the daemon's normal headless/control entry path.
External tests/harnesses own malformed streams, slow peers and disconnects. No
test-only product branch, fake Store or test-only public API. A test client that
bypasses the daemon can test a handler, but does not satisfy Docker-daemon-path
acceptance. Later pair 1 adds kernel/FUSE/overlay evidence; pair 2 adds history.

For V01/V13/V17, record host service and container daemon process identities,
container launch/image identity, configured and observed peer addresses, the
selected carrier/authentication profile and bounded driver input/output route.
Demonstrate that the operation reaches the service through its network endpoint,
not through a host-only direct call. Record the actual container mount
configuration and inspect for application-created payload/backing artifacts. Ordinary image files and runtime
configuration are not Workspace data. Source fixtures/generators and result
verification belong to the host driver and must remain bounded; do not collect
all output merely to compare it. Construct/save/readback/edit checks use the real
host SQLite Store. Prepared-tree checks use host-stored logical roots without
creating a container directory tree. No mount-free acceptance is established by
this checklist alone.

## 7. Measuring the implementation without misleading speed claims

The [direct/forward benchmark-verification draft](implementation/04-benchmark-direct-forward.md)
turns these accounting requirements into a prospective comparison contract. Its
implementation follows product correctness/deployment work; workload membership,
numeric gates and actual runner flags still need a committed freeze.


Follow the repository [benchmark rules](../../../../../docs/general/benchmark_rules.md),
[benchmark-tree rules](../../../../../benchmark/AGENTS.md),
[runner mechanics](../../../../../benchmark/fs-bench-pro/QUICKSTART.md) and
[release policy](../../../../../docs/general/release-policy.md). Current root
AGENTS directions override older n3 language: one sample per case/arm unless the
owner approves a different campaign, one construction worker except namespace
initialization, 15 s complete selection or explicitly declared exceptions up to
25 s, verification normally under 15 s with a 60 s hard budget. Do not run the
retired preflight or introduce an aggregate replacement.

Before a new transport performance family/harness or measurement is implemented,
commit its operation-specific case contract and link its issue. Freeze byte sizes,
edits, required rows, timer boundaries, cache state, resource domains, identities,
statistics and checks. This proposal supplies the questions, not a numerical
latency/throughput target or permission to relax an existing family.

The coordinator, service, C1/C2 and SQLite stay on the macOS host for the current
benchmark topology. Docker owns the Linux daemon with bounded headless I/O.
Pair 3 uses no LayerFS/FUSE mount, host-directory bind mount or shared-data volume;
it creates no sandbox Workspace or payload/spool/result files. Host fixtures or
incrementally generated bytes enter through stdin, cross the network bridge to
the host service, and return through the bridge and stdout. Host-side service
scratch/replay and driver source/sink resources remain separately charged. Future
cloud/native deployment diagrams grant no current benchmark-topology exception.

Report separately:

- first-use setup/build/image/connection cost;
- established-session operation latency, including required request encoding,
  transfer, replay acquisition, core execution and terminal response delivery;
- time to first read byte versus time to a successful terminal read result;
- host service and daemon user/system CPU, admission refusals, I/O wait and syscall/frame counts;
- declared/observed application requests, responses, setup handshakes, admission
  controls, flow-control frames, connection opens and forbidden object RPCs;
- payload/control bytes sent/received, scanned/copied bytes and allocations;
- host process baseline/phase peak and Docker cgroup anonymous/file/socket/kernel
  domains, replay/scratch disk, source/sink and verification-only resources;
- independent oracle verification and cleanup with exact coverage and identities.

Use monotonic clocks within each process; do not subtract unsynchronized host and
container timestamps. Cross-process phase comparisons need declared clock mapping
and uncertainty. Prefer end-to-end duration measured at the initiating caller,
with service-local child timings attached rather than a telemetry polling RPC.

Direct invocation is secondary attribution/parity evidence. The primary measured
route is the separate-process network deployment above. Changing the
deployment/route changes the treatment and is not an identical-route product
speedup comparison. Never call the direct number a measured transport speedup.
Measure every promised environment separately, and compare algorithm candidates
only with matched operation/workload/harness/cache semantics and exact identities.

Reuse prepared pristine inputs/builds/images under the existing seals and clone
contract; use fresh outputs and samples. Reuse never moves measured work outside
its phase or credits resident bytes to a cold claim. Separate verifier readback
from operation timing. Respect the measurement lock, which is per worktree
(owner direction, 2026-09-21 — [isolation](../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)), and other owners' runs: two
runs in one worktree never overlap, while another worktree is not excluded.

Unavailable counters/phase peaks are unavailable, not zero. No meaningful
percentile follows from one sample. Missing required proof is INCOMPLETE/NOT_RUN,
unsupported cold acquisition is INELIGIBLE, and a valid missed limit is retained
as FAIL. A streaming design or a small source diff is not a measured performance
result.

## 8. Before implementation can be called ready

Freeze one initial network carrier and its exact transport/auth method,
opcode-specific byte/cardinality/replay limits and deadline/disconnect behavior. Initial admission is Q=0 and cancellation
closes the active connection; neither a waiting scheduler nor in-band cancellation
is part of this scope. Resolve exposed Stage 7 boundary findings. Then implement the smallest real path and its focused
external checks. Do not claim this proposed F/W/C/A/Q/P example, the ASCII
diagrams or a passing core-only suite proves production memory, CPU or latency.
