# Service, daemon and transport design packet

> **Status:** Proposal for v0.1.7, prepared 2026-09-20. Not implemented,
> protocol-frozen, measured or qualified by this documentation.

This packet specifies the service/bridge/daemon foundation that **pair 1 will
consume for Workspace and FUSE**. Existing issue numbering remains unchanged:
[pair 3 / #181](../04-boundary-and-trust.md) owns this foundation,
[pair 1 / #179](../01-projection-and-runtime.md) adds Workspace/FUSE, and
[pair 2 / #180](../03-history.md) adds history. The implementation order is
**pair 3 -> pair 1 -> pair 2**. This packet covers the requested foundation and
its later pair 1 integration; it does not freeze the Workspace accumulator or
FUSE callback design.

The main pair 3 acceptance environment is **network delivery between separate
processes: an actual Linux Docker daemon and a native macOS host service**.
Direct same-process calls are secondary semantic-parity checks and cannot replace
this path. The architecture must also preserve a credible future cloud/serverless
SQLite path. Durability and a managed SQLite implementation come later.

The [implementation specification](implementation/README.md) turns this packet
into ordered product, telemetry and verification work. It uses C3 as the current
request's shorthand for pair 3/#181. The M0 source/security/schema/resource freeze
is still NOT_FROZEN; the specification itself is not implemented or qualified.

The [direct/forward benchmark-verification draft](implementation/04-benchmark-direct-forward.md)
is sequenced after the C3 product implementation and prerequisite checks. It
keeps the existing core benchmark unchanged and separates execution mode from
placement and telemetry-output selection; no new benchmark mode exists yet.

## Read in this order

| Document | What it settles in the proposal |
| --- | --- |
| [01 — Architecture and portability](01-architecture-and-portability.md) | ASCII ownership/dependency and deployment diagrams: embedded native, host processes, host plus Docker, native cloud, future managed SQLite, and later Workspace/FUSE integration. Identifies the real provider/lifecycle portability gaps. |
| [02 — File layout and boundaries](02-file-layout-and-boundaries.md) | Proposed three-crate tree, module responsibilities, dependency direction, production daemon entry before FUSE, and existing C1/C2 APIs to reuse. |
| [07 — Public operation catalog](07-public-operations.md) | Five caller-facing operations, inputs/results, C1/C2 mappings, minimal public entry points and internal-only storage calls. |
| [03 — Operations and transport](03-operations-and-transport.md) | Request/result sequences, C1 input constraints, framing, authorization, admission, backpressure and completion/failure states. |
| [04 — Resource and verification plan](04-resource-and-verification-plan.md) | Roundtrip accounting, buffer/replay/CPU bounds, illustrative memory arithmetic, correctness cases, real Docker/host evidence and honest performance measurement. |
| [05 — v0.1.6 comparison](05-v0.1.6-comparison.md) | Source-backed legacy flow, existing optimizations to preserve, exact exchange formulas, and the recommended minimal refinements before pair 1. |
| [06 — Later FUSE and cloud integration](06-future-fuse-and-cloud.md) | Workspace ownership, thin FUSE adapter, future native-cloud and managed/serverless adaptations, and the integration evidence each needs. |
| [08 — Telemetry and retention](08-telemetry-and-retention.md) | Disableable operation recording, independent CPU/memory observation, environment adapters, host/local output and bounded configurable cleanup. |

Document 07 owns the public operation catalog; document 03 owns wire and lifecycle
semantics; document 04 owns product-resource and verification profiles. Document 08
owns proposed telemetry observation/output budgets and retention; its candidate
operational defaults do not override benchmark contracts. The file map assigns responsibilities rather than creating
empty modules. Exact protocol widths, dependencies and deployed limits remain
implementation decisions to close below.

## Simplified initial scope

The further review keeps three crates and removes speculative machinery:

| Keep now | Cut or defer |
| --- | --- |
| Shared concrete request/result/error/limit types | Mandatory `Operations` trait, dynamic handler registry and per-op lifecycle classes |
| One frame encoder/decoder, concrete client, server calling a supplied handler | Independent daemon-control parser or additional local endpoint |
| Validated connection/active-operation and byte limits | Waiting-operation queue and scheduler; Q=0 means immediate refusal |
| Deadline/disconnect handling and explicit unknown outcome | In-band `CANCEL`, acknowledged rollback, resume and automatic replay |
| One selected authenticated native carrier; same-body direct parity | Carrier/provider matrix, generic cloud framework or unused embedded feature |
| Focused contract, adapter/protocol, service-operation/input and daemon folders | Catch-all implementation files, per-field wrappers and empty future adapter scaffolding |

These are proposal scope choices, not production changes. The five logical
operations remain service handlers used to exercise transport. Future composite
submission, Workspace/FUSE and managed SQLite stay in their own integration work.

Telemetry likewise stays in **one `layerfs-telemetry` crate**: existing timing
and portable reports plus optional native monitoring/output modules. The
[telemetry proposal](08-telemetry-and-retention.md) defines feature/runtime disable
boundaries; no separate telemetry-runtime crate is required.
Its [LOC baseline and estimate](08-telemetry-and-retention.md#production-loc-baseline-and-estimated-growth)
records 763 existing production LOC, an estimated +1,800 to +3,200 inside that
crate, and +100 to +300 for application wiring. Tests/examples are separate;
the new telemetry features are not implemented by these documents.

## One implementation, configurable placement

Build **one bridge library, one logical service implementation and one logical
daemon implementation**. Separate shared operation types from delivery-adapter
folders. The initial native adapter contains client/server endpoints over one
selected authenticated carrier; future HTTP/WebSocket adapters can implement the
same operation meanings without copying the service. Direct invocation calls
the same service body; it is not a second bridge implementation. Do not create
separate local, Docker and cloud bridge crates or environment-specific service
forks. Different native platforms can require different builds/configuration.

The [folder proposal](02-file-layout-and-boundaries.md) separates these two future
extension points: **delivery adapters under bridge; persistence providers under
C2**. Different databases do not require different bridge protocols. All production
files stay within 999 physical lines; `lib.rs`/`mod.rs` stay within 200 and contain
only declarations/reexports/thin delegation. Future locations are documented but
are created only with a real implementation.

| Placement | Delivery | Status in this scope |
| --- | --- | --- |
| Linux Docker daemon -> native macOS service | Selected authenticated network carrier, separate processes | Main acceptance target |
| Caller and service in one native process | Direct call to the same authorized handler | Secondary semantic parity |
| Other native host/process or cloud placement | Reuse the network implementation where the target supports it; configure endpoint/auth/limits | Future target qualification |
| Managed/serverless service | Target-specific carrier, execution and SQLite-provider adaptation | Future work; not an unchanged-native-binary promise |

Multiple daemons instantiate the same client code and connect independently to
the service. They need supported runtimes, network reachability and authorization;
"anywhere" does not bypass those conditions. Bound connections and active work;
Q=0 refuses excess operations rather than creating a waiting scheduler. Caller
isolation and C2 writer ownership still apply.

Service and daemon remain independent of each other's implementation and native
layout, while sharing operation meanings and supported protocol versions. The
service knows the verified caller context and authorizes its requests. It produces
the semantic result; the bridge delivers that result and reports delivery failures.
Neither a client-supplied identity nor successfully transferred bytes can establish
authorization or storage success.

## The proposed shape

```text
 NOW: macOS host                              NOW: Linux Docker
 +-------------------------------+            +-------------------------------+
 | External operation driver     |-- stdin -->| layerfs-daemon                |
 | Generate/supply/check bytes   |<- stdout --| headless input/result relay   |
 |                               |            |             |                 |
 | layerfs-service               |            |       bridge client           |
 | bridge server <---------------+-- network -+-------------+                 |
 | authorization + Store routing |            | bounded buffers only          |
 | common logical handlers       |            | no Workspace/payload files    |
 |        C1 <-> C2 -> SQLite     |            +-------------------------------+
 +-------------------------------+
 LATER: pair 1 replaces the supplied-input caller with Workspace + FUSE.
        It uses the same bridge client; core object calls stay service-local.

 Direct native caller -----------------------> same authorized handler
                                               (no wire encoding)

 Native cloud: move the service and its native SQLite together.
 Managed/serverless later: adapt C2 provider and service lifecycle, then qualify.
```

The external driver runs on the host and exercises the actual container daemon's
bounded stdin/stdout path. Calling a bridge client directly does not prove that
deployment. The bridge is shared contract and endpoint code, not a third process.
One service can serve multiple daemons under explicit admission limits; that
does not grant multiple concurrent C2 writers.

**Pair 3 is mount-free at the LayerFS application boundary:** no FUSE mount,
host-directory bind mount, shared-data volume, Workspace directory or container
payload/spool/output files. Input crosses stdin and then the real network bridge;
readback crosses the bridge and stdout. The container still has its ordinary
image filesystem, executable and runtime configuration. SQLite and any required
service-side scratch/backing remain host-owned; the initial bounded edit replay
is service memory. Actual Workspace backing and FUSE mounts arrive in pair 1.

## Data movement and the optimization contract

Send a complete logical operation with an explicit root and stable input. Keep
canonical construction, grouped object reads, membership checks, encoding and
SQLite work together in the service. The daemon does not request each object or
page across the network, and does not preflight every upload with `contains`.

On an established authorized connection, pipeline `BEGIN`, bounded body frames
and `END_INPUT` without a per-chunk application acknowledgement. Stream read
results under backpressure and finish with a typed terminal outcome. Direct
delivery invokes the same handler without serialization. One logical exchange
does not mean one physical packet RTT, and framing alone does not prove bounded
total memory or CPU.

The initial proposed operation set is `ReadFile`, `Inspect`, `ConstructFile`,
`EditFile` and `UpdatePreparedFilesystem`. These are proposed bridge names,
mapped to actual C1/C2 interfaces in the [operation catalog](07-public-operations.md).
Key limits are explicit:

- Complete-file construction can consume a sequential stream. Known edits use
  replayable replacement input under a separate retained-byte cap because the
  existing `EditSource` can reread it.
- Prepared filesystem updates use existing inode identities and already retained
  content/metadata roots. N file constructions/edits plus one bounded attachment
  cost N + 1 logical exchanges and capped O(N) caller metadata. This is not an
  atomic multi-file save; the initial surface has no same-save combined operation.
- Stream result bytes are provisional until terminal success. Saved-root success
  means C2 completion under its current profile, not Commit/head publication or
  crash durability. A lost mutation response can leave an unknown outcome.
- Frame, total bytes/records, connections, active handlers, queues, replay, scratch,
  socket/TLS and upstream/downstream buffers all need named owners and limits.
  No hidden resend, unbounded drain, disk-spool fallback or whole-read accumulator.

The illustrative F=64 KiB, W=256 KiB, C=8 profile in document 04 yields a
**5 MiB payload/codec application-buffer allowance per endpoint**, plus a
256 KiB metadata allowance under its stated assumptions. This is neither total
RSS nor an approved default. Core, replay, sockets, driver I/O and caches are
additional. No speed, memory or CPU improvement has been measured for this
proposed transport.

## Why the components can change independently

The daemon consumes logical operations; it does not link native SQLite or own
the service's Store handles. The bridge depends on neither service nor C1/C2.
The concrete service accepts the shared request/result contract, composes C1/C2
and owns authorization. The bridge server invokes its supplied handler; direct
calls enter the same service body. A mandatory shared trait or handler registry
is unnecessary. Transport adapters can therefore change without
reimplementing content/storage behavior or putting network types into core.

An internal C1/C2 optimization must still preserve the declared API, canonical
profile, storage format, errors and bounds. API compatibility alone does not
guarantee identical roots or that an old database can reopen. Independent
algorithm substitution remains part of the [Stage 7 review](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/stage-7-architecture-review.md).

Cloud portability needs more than moving SQLite away from the service. C2's
concrete connection/transaction/error APIs, native codecs, input replay/scratch,
operation lifetime, restart reconstruction and future durability must fit the
selected runtime. Document 01 maps those changes; no speculative provider
framework is added now. See also the [SQLite portability investigation](../../../../../docs/roadmap/0.1/0.1.7/study/service-runtime-cloud-path/README.md).

## Decisions to close before implementation

1. Select the native stream carrier and host/container address, authentication,
   credential lifetime and caller-to-Store authorization. Ordered TCP is the
   concrete candidate, not a frozen security/transport choice.
2. Freeze operation schemas, version negotiation, identity representation,
   exact-length framing and completion/error rules. Preserve edit order and
   current-result coordinates. Finalize the supported inspect queries/pages.
3. Set per-opcode total byte/record/replay limits, aggregate process budgets,
   active admission and deadlines for the intended workload, with Q=0 immediate
   refusal and disconnect-based cancellation. Resolve any
   required edit workload that cannot fit the proposed replay route.
4. Reconcile the final C1/C2 revision, schema/profile and exposed Stage 7 findings.
   Keep required core corrections explicit; a bridge test does not qualify core
   substitution or previously unverified capacity combinations.
5. Implement the smallest real host service, bridge and Docker daemon path; run
   the document 04 cases and record identities, results and remaining gaps.
   Register/freeze a performance case contract before creating its benchmark.

These are concrete remaining design choices, not requests to implement every
possible transport or cloud provider. Workspace/FUSE, history, managed SQLite,
durability and a combined multi-file save require their own later contracts.

The [legacy comparison](05-v0.1.6-comparison.md) recommends evaluating one bounded
multi-file submission for pair 1. A single network operation can use sequential
existing C2 saves, with explicit retained-object/partial-failure semantics; an
atomic combined save is a separate capability. It also identifies metadata-query,
read/control responsiveness, execution ownership and error-mapping requirements.
None is an implemented or newly frozen opcode.

## Source basis and evidence status

The source inspection captured 255 files with HEAD
`795fb1a2f792742a2b19fdb82d98e6fd8a8b0470` and the working files present at capture.
The retained [source-snapshot.json](source-snapshot.json) records file hashes and
no detected changes during capture. Its SHA-256 is
`f916d0015473dcbb1ba893f6327c47a1a821e5419d785017ab62f090b55ff44e`.
The commit alone does not identify captured working changes. This manifest is
an inspection reference, not a sealed build, dependency seal or qualification
receipt. Source links open the current checkout, which may evolve independently.

Parallel review covered architecture, file/dependency boundaries and operational
transport/resource semantics; an independent review checked their consistency.
Its clarifications are incorporated: unread daemon stdin must not be reused
after an unsynchronized refusal, and N-file composition costs and metadata
retention must be explicit.

All proposed deployment, security, protocol, resource, performance, cloud and
independent-substitution checks remain **NOT_RUN**. Documentation checks validate
this packet's links/formatting and source-reference consistency only. No runtime
code, crate skeleton, image, test, benchmark or issue update is delivered here.
