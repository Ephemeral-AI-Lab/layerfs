# Pair 3 — portable service, bridge and Docker daemon

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Issue: [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181).
Parent: [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
Core contract review: [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172).
Sibling pairs: [projection/Workspace #179](01-projection-and-runtime.md) and
[history #180](03-history.md).

## 1. Owner scope and implementation order

Owner direction, 2026-09-20, sets two priorities:

1. **A flexible architecture that can later support cloud/serverless SQLite.**
   Keep operation contracts independent of transport, process lifetime and the
   concrete SQLite provider. Document actual portability gaps and the future
   changes needed; do not claim a cloud backend already exists.
2. **Implement and verify Linux Docker daemon + host service now.**
   The main acceptance target uses **network delivery between separate processes**:
   an actual Linux Docker daemon and the native macOS host service. Direct calls
   are secondary semantic-parity checks and cannot replace this route. Record the
   exact host/container setup, endpoint, authorization and limits used. Other host
   platforms require their own qualification.

Implementation remains **pair 3 → pair 1 → pair 2**, per the
[execution contract](README.md#implementation-order-pair-3-then-pair-1-then-pair-2).
Before implementing pair 3, agree only the initial operation inputs, results,
identity and completion semantics with the later pairs. Their full implementation
is not a prerequisite. Pair 3's scope is now specified. The detailed
[service/daemon/transport design packet](service-daemon-transport/README.md)
provides deployment diagrams, file boundaries, proposed operations and resource
accounting, including the later pair 1 integration. Protocol freeze,
implementation and acceptance evidence remain open.

## 2. Components and concrete topology

```text
 HOST (macOS initially)                    LINUX DOCKER CONTAINER
 +-----------------------------------+    +-----------------------------+
 | layerfs-service                   |    | layerfs-daemon              |
 |                                   |    |                             |
 | authorization / Store routing     |    | process/configuration       |
 | operation handlers                |    | connection and request path |
 | C1 + C2                           |    | local resource ownership    |
 | embedded SQLite                   |    |                             |
 |                 bridge endpoint   |<-->| bridge endpoint             |
 +-----------------------------------+    +--------------^--------------+
                                                        |
                                                   test driver

 Later, pair 1 adds FUSE and Workspace creation/state to the daemon.
 Later, pair 2 adds history, staging, Commit and conditional publication.
```

| Component | Owns | Must not own |
| --- | --- | --- |
| `layerfs-service` | Authorized logical operation execution, Store lifetime, C1/C2 composition and storage outcomes | FUSE callbacks, client file handles or pending Workspace overlays |
| `layerfs-daemon` | The execution-side process, configured service connection, bounded request/result handling and cleanup; later hosts pair 1 | SQLite credentials, SQL, physical pack logic or guessed history semantics |
| `layerfs-bridge` | Shared operation contract and endpoint adapters; versioned framing, bounded transfer, backpressure and connection failure reporting | Filesystem algorithms, history/publication decisions or automatic mutation replay |

The bridge is endpoint code, not a third process. A direct invocation can call the
same handler without wire serialization; the actual Docker/host path uses one
selected, authenticated transport. The daemon initiates the host connection so
service operation does not require inbound access to each container. Record the
real route; a container's localhost is not assumed to be the host endpoint.

There is one bridge library with client/server endpoints and one selected native
network carrier initially, plus direct invocation of the same service body. Keep
one logical service and daemon implementation across native placements, using
configuration and target builds rather than local/Docker/cloud forks. Managed
runtime support still needs the separately reviewed carrier/provider/lifecycle
adaptation; it is not a promise that the native binaries run unchanged anywhere.

Service handlers receive a verified caller context and the shared logical request.
They do not depend on daemon implementation types, Workspace internals or FUSE
callbacks. The service authorizes and executes the operation; the bridge delivers
its result and reports transport failure, without inventing storage success.

One service may serve multiple daemons. Connection multiplicity does not promise
concurrent C2 saves: obey the qualified Store ownership/admission rules and bound
all queues. Any 1:N claim needs an actual multiple-daemon check.

## 3. What to implement now

- A service that opens the configured native SQLite Store and executes real
  C1/C2 logical operations through their supported public interfaces.
- The minimal daemon lifecycle and bridge client needed to exercise that service
  from a Linux container. Workspace creation and FUSE arrive with pair 1.
- One concrete host/container transport and the required authorization checks.
  Settle the exact transport and trust profile before code; neither a universal
  provider registry nor every deployment combination is required.
- Bounded requests/results and byte streaming, including slow peers, disconnects,
  partial frames, malformed inputs and unsupported protocol/profile values.
- An external test driver supplying stable bytes, known edits and explicit roots.
  Use the same daemon client and service handlers that pair 1 will consume.

The pair 3 driver runs on the host and submits through the real daemon's bounded
stdin/stdout interface. No LayerFS/FUSE mount, `/dev/fuse`, host-directory bind
mount or shared-data volume is needed or used. The container creates no Workspace
directory or payload/spool/result files; it holds bounded transfer buffers.
SQLite, fixtures and any required service scratch remain host-owned, with initial
edit replay in capped service memory. Docker's ordinary image/runtime files are
not excluded by this application-data rule. Pair 1 introduces actual Workspace
backing and mounted filesystem behavior later.

The initial operation surface covers read/inspect, complete-file construction and
save, known file edits and save, and a filesystem update against a prepared base
where supported. C1 directory updates contain sorted final bindings for changed
names, not a resend of every unchanged directory entry. New-inode operations need
an explicit allocation/nonreuse contract; an existing-file path can be verified
without implementing history or a Workspace allocator.

```text
 test input -> Docker daemon -> bridge -> host handler -> C1 -> C2 -> SQLite
 result     <- Docker daemon <- bridge <- host handler <- save/read outcome
```

Keep C1/C2 together at the host. Their canonical provider/consumer calls remain
local; do not turn individual canonical-object reads or emissions into network
messages. Saving objects produces an explicit root and storage outcome, not a
logical Commit or branch-head update.

## 4. Portability requirements to establish now

### Operation and transport boundary

- Requests carry authorized logical Store identities, roots, stable input and
  operation generations, not native paths, process pointers or SQLite handles.
- Client, bridge and service handlers share an explicit semantic contract. Direct
  and stream delivery use the same handlers; remote transport does not introduce
  a second implementation of content/storage behavior.
- Define bounded payload delivery independently of native sockets. The contract
  must permit a future WebSocket/HTTP endpoint and must not require a permanently
  running process or permanently open connection for operation identity.
- Preserve missing-content, invalid-input, capacity, ownership and unknown-outcome
  distinctions. Peer authentication and operation authorization are independent
  of content hashes. A Store name is not itself an access grant.
- Input acceptance, saved objects, future history publication and future durable
  acknowledgement are distinct. Preserve the current no-resend rule.

### C2 and SQLite boundary

Cloud/serverless SQLite remains a future target, and durability is planned later.
Today's concrete implementation remains native SQLite under the current profile.
The future change is to separate C2's storage logic from provider-specific
connection, transaction, resource-limit and durability APIs; it is not to expose
SQL through the bridge or move authority into the daemon.

Produce a source-backed boundary map for native connection/error leaks, grouped
reads/writes, whole-save ownership, publication, supported formats and limits,
input replay/scratch needs, and restart reconstruction. State what a managed
SQLite provider would replace and what must remain unchanged for bridge callers.
Resolve/disposition any gap that blocks the present Docker/host path. Do not
introduce an unused managed backend or broadly rewrite C2 while Stage 6 finalizes.
A second provider still requires actual target builds, behavior/format checks and
resource qualification; document those unrun proofs explicitly.

Use the [SQLite service portability investigation](../../../../docs/roadmap/0.1/0.1.7/study/service-runtime-cloud-path/README.md)
and studies [#157](https://github.com/Ephemeral-AI-Lab/layerfs/issues/157),
[#158](https://github.com/Ephemeral-AI-Lab/layerfs/issues/158), and
[#173](https://github.com/Ephemeral-AI-Lab/layerfs/issues/173) as evidence, not as
permission to copy their retries, conflict semantics or unqualified adapters.

## 5. Decisions to close before implementation

- [ ] Enumerate the initial operations, typed results, error taxonomy and protocol
      version/compatibility rules; identify direct-call versus wire-only work.
- [ ] Record host/container versions, connection endpoint and initiation, process
      lifecycle, selected transport and trust/authorization mechanism.
- [ ] Define initial caller-to-Store permissions and tenant/identity direction;
      provide that direction before pair 2 freezes history schema.
- [ ] Declare frame, request, response, in-flight, queue and scratch/replay bounds;
      define who owns incomplete input and when pressure reaches the sender.
- [ ] Define save completion, definite failure and unknown outcome. Lost mutation
      acknowledgement must not trigger resend, reflush, polling or guessed rollback.
- [ ] Pin the C1/C2 source, dependency lock, schema/profile and affected Stage 7
      finding dispositions. Preserve unrelated Stage 6 work and its evidence.

## 6. Acceptance for this pair

### A. Architecture flexibility

- [ ] Publish the dependency and ownership map for service, daemon, bridge and
      C1/C2; no transport/FUSE/daemon types enter core algorithms.
- [ ] Use one real operation handler through direct and Docker/host delivery;
      show matching semantic results and error classes without duplicating C1/C2.
- [ ] Document the native-to-managed SQLite adaptation boundary, lifecycle and
      format constraints, proposed future durability semantics and all unrun cloud
      checks. No cloud or crash-durability acceptance follows from this document.
- [ ] State API/protocol, canonical-profile and persisted-format compatibility
      separately. Keep #172's independent algorithm-substitution proof explicit.

### B. Docker daemon + host execution

- [ ] Launch the actual host service and Linux container daemon; verify startup,
      configured connection, authorization and normal shutdown through the chosen
      route. Direct calls alone are not this acceptance.
- [ ] Send stable bytes from the daemon; construct with C1, save with C2, return
      a root only after successful completion, and read exact bytes/ranges back.
- [ ] Edit that root through the same path; verify new content and the continued
      readability of the original immutable version. Verify a prepared filesystem
      update without inventing Workspace/history behavior.
- [ ] Verify protocol compatibility, partial/malformed/truncated frames, denied
      access and capacity refusals; enforce bounds under slow upload/download.
- [ ] Exercise disconnects during input and around completion; report actual
      known failure versus unknown outcome, with no automatic replay or false
      success. Preserve established successful data and checked cleanup errors.
- [ ] If claiming 1:N, verify multiple daemons, response/permission isolation and
      Store contention behavior without adding workers or unbounded buffering.
- [ ] Record exact deployment/source identities, commands, results and unrun
      cases. Follow the measurement contract for performance/resource claims;
      do not overlap another owner's resource-sensitive run.

## 7. Scope boundaries

- Pair 1 owns actual FUSE mounts and Workspace generation, overlays, handles and
  lifecycle. Pair 3 tests the full service-operation path with supplied inputs.
- Pair 2 owns history, stage/Commit identity and conditional head publication.
  The initial bridge does not infer those meanings from a C2 save outcome.
- Cloud/serverless deployment, managed/remote SQLite implementation, object-store
  tiering and replication are future work. Future portability is a required
  architecture review deliverable, not an implemented-provider claim.
- Durability comes later. Current MEMORY journal / synchronous OFF / no sync/WAL
  behavior remains unchanged; this scope neither enables it nor forbids a future
  explicitly qualified durable SQLite profile.
- No automatic retries, fallback, dependency patches or speculative platform
  framework. Unsupported required capabilities fail explicitly.
- #171's core evidence is not Docker/host integration acceptance; #172 remains
  the separate architecture/replacement review. Do not close either scope by
  substituting a bridge smoke test for its required evidence.
