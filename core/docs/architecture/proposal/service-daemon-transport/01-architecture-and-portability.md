# Service, daemon and transport: architecture and portability

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This proposal develops [pair 3 / #181](../04-boundary-and-trust.md), the service,
bridge and Docker-daemon foundation. Pair numbers retain their established
meaning: pair 1 / #179 later adds FUSE and Workspace behavior; pair 2 / #180 later
adds history and publication. The implementation order remains
[3 -> 1 -> 2](../README.md#implementation-order-pair-3-then-pair-1-then-pair-2).
Preparing this foundation does not rename it pair 1.

Source basis: captured HEAD `795fb1a2f` plus the frozen working files in
`layerfs-transport-docs-4325awif`; 255-file manifest SHA-256
`f916d0015473dcbb1ba893f6327c47a1a821e5419d785017ab62f090b55ff44e`.
The capture includes concurrent working changes and is not a qualification seal.
Source descriptions below refer to that capture; Stage 6's final source must be
reconciled before implementation. No service, daemon, cloud provider, latency or
resource result is established by this document.

For the later mutable-state owner and cloud adaptations, see the
[Workspace/FUSE and cloud integration proposal](06-future-fuse-and-cloud.md).
Workspace owns overlays, generations and semantic handle lifetimes; the daemon
assembles it and the thin FUSE adapter. No replacement combined LiveOwner is needed.

Read alongside [file layout and boundaries](02-file-layout-and-boundaries.md),
[operations and transport](03-operations-and-transport.md), and
[resource and verification plan](04-resource-and-verification-plan.md).
The prior [SQLite portability study](../../../../../docs/roadmap/0.1/0.1.7/study/service-runtime-cloud-path/README.md)
supplies the broader source investigation and provider constraints.

## 1. Proposed architecture

Keep C1 and C2 together inside the operation service, close to SQLite. The daemon
supplies a complete logical request and bounded stable input. The service checks
authority and resource admission, runs the real C1/C2 pipeline, and returns the
operation's actual result. Internal object-provider calls stay local.

```text
 CLIENT / EXECUTION SIDE                       AUTHORITATIVE OPERATION SIDE
 +----------------------------+               +-------------------------------+
 | Initial: external driver   |               | layerfs-service               |
 | Later: FUSE + Workspace    |               |                               |
 |                            |               | principal + Store permission  |
 | layerfs-daemon             |               | version/input/bounds checks   |
 | stable input generation    |               | operation admission           |
 | bounded request/result I/O |               | logical operation handlers    |
 +-------------+--------------+               +---------------+---------------+
               |                                              |
               |  logical Store ID, root, request, byte stream |
               +======== layerfs-bridge ======================+
                    contract + endpoint adapters              |
                    no third bridge process                   v
                                                 +------------+-------------+
                                                 | C1 canonical content     |
                                                 | construction/edit/read   |
                                                 +------------+-------------+
                                                              |
                                          LOCAL canonical object handoff/read
                                                              |
                                                 +------------v-------------+
                                                 | C2 CAS/encoding/packs     |
                                                 | save/visibility/readback |
                                                 +------------+-------------+
                                                              |
                                                 native SQLite, initially
```

There is **one bridge library**, containing the shared logical contract, one
frame encoder/decoder and the client/server endpoint code. Its client runs in
the daemon; its server endpoint calls the service's concrete handler. These are
the two ends of one bridge, not independent bridge implementations or a third
process. A direct call reaches that same handler without invoking the wire codec;
it is not a second bridge. No mandatory dispatch trait or handler registry is
needed for this composition.

The bridge does not replace C1's canonical object grammar or C2's physical
format. A logical read may traverse many mapping pages and packs entirely inside the service; a logical
save may emit many canonical objects without sending a network message per
object. A streamed request can have many bounded frames without becoming many
independent mutation operations.

| Component | Owns | Depends on | Does not acquire |
| --- | --- | --- | --- |
| Daemon | Process/connection lifecycle and bounded input/result relay; later hosts and wires Workspace/FUSE | Bridge operation meanings and limits | SQL credentials, Store paths, physical pack structures |
| Bridge | Versioned request/result vocabulary, bounded delivery, correlation, flow control and connection failure classification | Stable semantic inputs/outcomes | Canonical construction, history decisions, automatic mutation replay |
| Service | Caller authorization, named Store routing, resource admission, operation execution and cleanup | Supported C1/C2 facade and provider profile | FUSE handles or a mirror of every client's pending overlay |
| C1 | Canonical objects, file construction/edits, filesystem changes, logical reads | Authenticated canonical provider and finalized consumer | Transport, principal, daemon or Workspace types |
| C2 | Exact reuse, physical representation, packing, Store ownership/publication and authenticated reads | Canonical contracts and concrete native SQLite today | Mount/session identity or branch/history policy |

“Owner” names the authority for a Store operation; it is not synonymous with a
particular binary, connection, thread, Workspace or tenant. A service can own more
than one configured Store only within declared aggregate bounds. A Store's
identity and its caller authorization must survive changing transport addresses;
that requirement does not itself provide restart recovery.

## 2. One implementation, main network acceptance and secondary direct parity

Keep one logical service implementation and one logical daemon implementation.
Native placements select endpoint, credentials, Store routing and resource limits
through configuration rather than separate host/cloud algorithm implementations.
Platform-specific builds and endpoint setup remain legitimate; this does not
promise the same executable on macOS, Linux and a managed runtime. A future
managed/serverless target requires the provider, execution and lifecycle
adaptations described below.

**Main acceptance uses two real production processes across the Docker/host
network:** the
Linux container daemon and the macOS native host service. The direct route is a
secondary semantic-parity check and cannot substitute for this acceptance.
The initial implementation selects one native network carrier. TCP is the
concrete candidate; endpoint and authentication/security details remain to be
frozen in [document 03](03-operations-and-transport.md). The diagrams do not
require additional IPC or cloud carriers to be implemented now.

### Native embedded: proposed parity route

```text
 ONE NATIVE PROCESS
 +---------------------------------------------------------------------+
 | caller -> trusted caller context -> common service handler           |
 |                                      |                              |
 |                                 C1 <-> C2 -> SQLite                  |
 +---------------------------------------------------------------------+
```

A direct call avoids framing, socket copies and transport setup. It still runs
operation/version/profile validation, authorization, capacity admission and the
same handler body. “In process” is a delivery property, not an authority grant.
A caller context must come from the selected local trust policy, not arbitrary
unverified caller fields. Direct results and errors must have the same semantic
meaning as streamed results; only transport-specific failures are absent.

This is the secondary semantic-parity route, useful also for native embedding.
It does not qualify the actual Docker/host route and does not promise a native
filesystem mount on every host.

### Host service with a native client: same operation boundary

```text
 NATIVE CLIENT PROCESS                    NATIVE SERVICE PROCESS
 +----------------------+                 +------------------------------+
 | logical caller       |                 | auth + named Store routing   |
 | bridge client        |--bounded stream->| bridge endpoint -> handler   |
 +----------------------+<--results-------| C1 <-> C2 -> local SQLite    |
                                          +------------------------------+
```

This shows the same selected network carrier between native processes. Changing
the configured endpoint does not select another filesystem algorithm. A native
remote service can reuse the same ownership shape with an explicitly qualified
remote trust profile. An IPC adapter is deferred until a real requirement exists;
no remote deployment is accepted by drawing this diagram.

### Immediate target: macOS host service + Linux Docker daemon

```text
 macOS HOST                                  LINUX DOCKER CONTAINER
 +-------------------------------------+     +----------------------------+
 | external operation driver           |---->| stdin -> layerfs-daemon    |
 | generate/supply/check bytes         |<----| stdout <- bounded relay    |
 |                                     |     |             |              |
 | configured principal/Store mapping  |     |       bridge client        |
 | layerfs-service                     |     | initiates host connection  |
 | bridge server -> shared handler     |<====| bounded network operation  |
 | C1 <-> C2 -> SQLite                 |====>| bounded network result     |
 |                                     |     |                            |
 +-------------------------------------+     +----------------------------+
          bridge crossing: selected authenticated bounded stream
```

The daemon initiates the connection. Record the real host endpoint, container
network route, host/container versions and credentials/trust setup before code.
A container loopback address is not assumed to be the host service. The host
driver must exercise the actual daemon stdin/stdout relay and its bridge client;
calling a client directly is not the main deployment test.

Pair 3 is mount-free at the LayerFS application boundary: no FUSE mount,
host-directory bind mount, shared-data volume, Workspace directory or container
payload/spool/output files. Input crosses stdin and the real network bridge;
readback returns through the bridge and stdout. The container retains its
ordinary image filesystem, executable and runtime configuration. SQLite and any
required service scratch remain host-owned; initial bounded edit replay uses
service memory. Pair 1 introduces actual Workspace backing and FUSE mounts later.

**Proposed first delivery:** one selected native bounded network transport, one
explicit trust profile and the common logical-operation handlers. Freeze its
actual socket and security mechanism in the [transport contract](03-operations-and-transport.md)
before implementation. Neither plaintext access outside its qualified trust
boundary nor a universal multi-transport framework is implied. Other host OSes
and network arrangements each require their own verification.

One service may accept N daemon connections within its declared connection cap,
but connected clients do not imply N active handlers or writers to one Store.
Every operation receives a trusted authenticated-caller context and checks that
caller's Store/operation permission; request fields cannot assert their own
identity or authority. Direct calls receive context from the trusted local setup
and run the same permission checks. The captured `Store::begin_save` acquires one
exclusive save owner. Initial admission has no waiting queue (`Q = 0`): refuse
when active capacity or required Store ownership is unavailable. Do not silently
retry or add workers to make contention disappear. Bound active reads and input
reception as well as saves, and verify multiple-daemon isolation before claiming
the N-daemon deployment works.

A shared `Store` handle and per-operation `StoreProvider` sessions are distinct:
the provider owns mutable read-session state and is not a shared concurrent
provider object. Source: [Store](../../../../crates/layerfs-storage/src/cas/store.rs),
[provider](../../../../crates/layerfs-storage/src/cas/provider.rs).

## 3. Later pair 1 replaces the driver input, not the service algorithms

```text
 PAIR 3 NOW                              PAIR 1 LATER
 external driver                        Linux FUSE callbacks
 frozen bytes / explicit edits                 |
 explicit immutable base                Workspace accumulator
          |                             pending overlay/read-your-write
          |                             frozen base + input generation
          +--------------------+----------------+
                               |
                      SAME daemon operation client
                               |
                      SAME bridge operation contract
                               |
                      SAME service C1/C2 handlers
```

The driver initially supplies stable bytes and known edits, plus roots that the
caller is allowed to access. Pair 1 later produces those inputs from a bounded
Workspace accumulator. It adds callback ordering, file-handle behavior, overlay
reads, mount lifecycle and flush ownership. It must not duplicate canonical
construction or serialize SQLite calls through a new path.

A request's base root must remain the root against which its bytes/edits were
prepared. A later generation cannot silently reuse an earlier request's result
or let its cleanup delete newer pending input. A generation/correlation value
identifies the input lifetime; it is not a durable deduplication or retry token.

The supported baseline pattern is published immutable base -> one C1 operation
-> C2 save completion -> returned root. Current filesystem updates keep newly
produced directory roots/typed inode rows locally and file edits retain their
unfinished mapping drafts; this pattern does not need a provider for newly
accepted but unpublished output. If pair 1 requires a second C1 operation or
logical read against an unfinished save's root, that is a separate explicit
capability: `SaveOperation::read_batch` exists, but `StoreProvider` sees published
Store state and there is no ready dual reader/sink same-save bridge. Resolve it
before using that path; do not insert artificial save/publication boundaries to
conceal the requirement. Source:
[filesystem update](../../../../crates/layerfs-content/src/filesystem/update.rs),
[file edit](../../../../crates/layerfs-content/src/file/edit/apply.rs),
[save handoff](../../../../crates/layerfs-storage/src/cas/store.rs).

Pair 3 returns a saved content/filesystem root after C2 `finish` succeeds. Pair 2
later defines logical Commit identity, stage state, allocator ownership and
conditional head publication. Those are new semantics added to the operation
contract, not aliases for `finish` or for a received byte frame. New-inode
requests need an explicit serial allocation/nonreuse agreement even before pair
2's full implementation; an existing-file slice can come first.

## 4. Two portability boundaries

```text
             PORTABILITY BOUNDARY A: LOGICAL OPERATIONS
 client  =================================================> service
         direct / native stream / future HTTP or WebSocket
         Store identity, roots, stable input, bounds, outcomes
                                                               |
                                                         C1 + C2 bodies
                                                               |
             PORTABILITY BOUNDARY B: PERSISTENCE CAPABILITIES    |
                 C2 storage logic ==============================+
                                   |
                        native SQLite today
                        managed SQLite provider later
```

Boundary A preserves behavior when a caller changes process or network location.
Its contract must not require a native path, file descriptor, Rust pointer,
SQLite handle, permanent TCP connection or permanently resident process.
Endpoint-specific addresses belong in daemon/service configuration; authorized
Store names belong in requests. Resolve names using service-owned configuration,
and check the principal's permission for that Store and operation. An arbitrary
Store name, digest or filesystem path is not an authorization credential.

Boundary B preserves storage semantics when the SQLite API/runtime changes.
It is a future extraction point, not an implemented portable provider interface.
Current C2 uses native `rusqlite::Connection`, SQL transaction commands, native
errors and fixed physical-format bounds across several modules. Keep that
implementation concrete for the initial host path. When a real second provider
is implemented, extract the smallest required boundary from those actual uses;
do not create a backend factory, registry or one trait per SQL statement now.

| Dependency that must remain explicit | Bridge/service contract | C2/provider contract |
| --- | --- | --- |
| Data | Logical root and typed change/byte streams; exact integer/byte encoding and limits | Canonical IDs/bytes, roles, bounded lookup/write groups and physical references |
| Semantics | Authorized operation, stable input generation, finish/cancel meaning | Whole-save ownership, atomic transaction work, visibility watermark, collision/dependency checks |
| Errors | Absence versus invalid input versus capacity/ownership/authorization failure; unknown mutation outcome | Engine errors translated without losing the failure/unknown-outcome boundary; cleanup errors retained |
| Resources | Admission, bounded queues and in-flight bytes; slow peer pressure | Codec/scratch/index/cache bounds, parameter limits, result/BLOB limits and transaction lifetime |
| Lifetime | Connection state is disposable; a disconnect does not prove no mutation | Reopen reads authoritative state; caches/index accelerators must not be the only record of success |

The shared operation contract should not expose `StorageError::Engine`'s native
engine type or require clients to match SQL error codes. Service diagnostics can
retain the original cause while the wire carries stable error classes. C1/C2
error taxonomy must not be collapsed into “not found” or “retry later.” See the
[operation contract](03-operations-and-transport.md) for exact completion and
error mapping decisions.

## 5. Future cloud-native paths

### First practical cloud route: native owner colocated with SQLite

```text
 host / container clients               CLOUD NATIVE SERVICE
 +-------------------------+            +--------------------------------+
 | same daemon/client      |===========>| authenticated network endpoint |
 | same logical operations |<===========| common service handlers        |
 +-------------------------+            | C1 <-> C2 -> local SQLite      |
                                        | qualified durable storage later|
                                        +--------------------------------+
```

This keeps the expensive read/compute/write loop next to its database. It changes
endpoint reachability, authentication and operational deployment; it still needs
real remote bounds/latency/restart qualification. It is not a reason to move
individual SQL statements or database pages over the bridge. SQLite's ordinary
WAL mode requires cooperating processes on one host; enabling WAL later would
not turn the database into a shared network-file solution.
[SQLite WAL documentation](https://sqlite.org/wal.html)

### Managed/serverless owner: future implementation and proof

```text
 SAME LOGICAL CLIENT CONTRACT              MANAGED PLATFORM, FUTURE
 +--------------------------+              +--------------------------------+
 | daemon/native/isolate    |--HTTP/WS----->| authorized named Store owner   |
 | endpoint adapter        |<--results----| service lifecycle/handlers     |
 +--------------------------+              | portable C1/C2 operation bodies|
                                           |             |                  |
                                           | managed SQLite capability API |
                                           +--------------------------------+
                                                  restart / eviction
                                                         |
                                             reconstruct from authority
```

HTTP/WebSocket delivery is a future adapter option, not a selected implementation
in this proposal. A managed SQLite API is not a drop-in `rusqlite::Connection`.
For example, Cloudflare exposes `sql.exec` and platform transaction methods;
cursors held across an `await` lack snapshot guarantees. Adapting that API needs
explicit transaction, integer/byte, cursor-lifetime and error rules.
[Cloudflare SQLite API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)

A managed owner may lose in-memory state during hibernation; constructors run
again when it wakes. Named routing cannot by itself preserve an active save,
read cursor or accepted upload. Restart reconstruction and a supported operation
lifetime must be proved on the real target.
[Cloudflare lifecycle](https://developers.cloudflare.com/durable-objects/concepts/durable-object-lifecycle/)

The current native codec dependency and optional ordinary-file ordering backing
also need actual target support. Provider parameter/BLOB limits can require new
bounded batches or physical formats. The portability study records these gaps;
none is solved merely by keeping the word SQLite. Native versus managed formats
may differ while canonical content stays equal, but that needs a declared
compatibility/import policy and tests. Remote-database-only placement is another
separate target, with its own grouped-I/O and ownership proof; it is not hidden
behind a connection-string option.

## 6. Data movement and minimum-round-trip design

The architectural target is one logical operation exchange, with bounded payload
streaming in both directions. Connection establishment/authentication and any
required admission exchange are separately visible costs. Frame count is not
round-trip count: a sender can fill the agreed bounded window without waiting
for a response after every frame. Exact flow control belongs in the transport
contract; this document does not claim a measured “one RTT” for a large save.

```text
 daemon                                    service
 frozen logical request -----------------> validate/authorize/admit
 bounded stable byte chunks -------------> input view -> C1
                                                        | local object transfer
                                                        v
                                                       C2 -> SQLite
 final input boundary -------------------> finish/check cleanup
 saved-root result <---------------------- acknowledged C2 completion

 range request --------------------------> C1 navigation -> local C2 reads
 bounded output chunks <------------------ logical byte sink
 terminal result <------------------------ final success/error
```

Avoid full-request aggregation merely to decode a message, full-file response
buffers, canonical-object RPC, repeated root acquisition by adapter glue and
reopening a read session for each mapping demand. Reuse one operation-local
`StoreProvider` and transfer owned finalized objects through `SaveHandoff`.
These preserve existing public pathways; they are not claims of zero copying.
Transport framing, read buffers, codec workspaces, pending object batches, packs
and output sinks all contribute to the resource budget.

Complete-file construction has a streaming C1 API. Known edits may reread
replacement ranges while comparing and constructing; a socket stream alone is
not a replayable `EditSource`. The operation plan must state where bounded stable
replacement input lives before accepting that operation. No unbounded generic
payload spool is introduced by the diagram. Native replay/scratch ownership and
maximum supported operations are defined in the sibling resource plan; future
managed targets must provide an equivalent accepted capability or refuse the
operation explicitly.

“Fastest” remains an optimization objective, subject to measured comparison.
First eliminate structural network work that is unnecessary: per-object hops,
stop-and-wait framing, duplicate construction, and duplicate whole-input copies.
Then measure the complete path with declared cache state, input ownership and
lifecycle costs. Bounded memory and low CPU are requirements to verify together;
more concurrent saves or unlimited buffering are not substitutes for that proof.

## 7. Independent changes and compatibility

| Change | Expected unchanged consumer | Required evidence / explicit consequence |
| --- | --- | --- |
| Internal C1 navigation/chunk-search optimization within the profile | Daemon request and service handler logic | Same canonical contracts, error/resource behavior and unchanged-consumer checks; caller and C2 select the same C1 crate instance |
| Internal C2 candidate/batching optimization | C1 and bridge logical contract | Canonical readback, ownership/publication, bounds, and format compatibility |
| Native direct versus selected native stream adapter | Logical service handler | Same validation, permission checks, inputs, semantic outcomes and typed error classes |
| New protocol operation/version | Existing supported operations only | Explicit version/capability agreement; unsupported requests rejected before effects |
| Canonical profile/cutoff change | No implicit root-identity promise | Explicit profile decision; identical logical bytes can have different roots |
| Schema/pack representation change | API stability alone is insufficient | Store reopen/import compatibility and format declaration; no silent migration |
| Managed SQLite or durable profile | Bridge semantics only if the stronger contract is implemented | Real provider/runtime, failure, recovery, resource and format qualification |

The captured native Store uses schema version 6 and validates its table/profile
identity. Its open path is not a migration mechanism. Stage 7's independent
C1-only, C2-only and paired substitutions remain a separate proof obligation;
service/daemon smoke tests do not discharge it. Source:
[policy](../../../../crates/layerfs-storage/src/policy.rs),
[schema validation](../../../../crates/layerfs-storage/src/sqlite/schema.rs),
[Stage 7 review](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/stage-7-architecture-review.md).

## 8. Restart and durability are later contracts

Today's native profile stays MEMORY journal, synchronous OFF, no WAL and no
added sync/durability operations. Saved-root success states that the current C2
operation completed; it is not a promise of survival after host power loss.
Losing the result after a mutation can leave an unknown outcome. Do not resend,
reflush, poll for a guessed answer or destructively roll back on that uncertainty.

Future durability must state a failure domain and provide the needed save/state
reconstruction, publication ordering, recovery, backup/restore and migration
proof. A durable SQLite provider is one dependency of that guarantee; the
service's input lifetime and eventual history publication are others. A new
operation-ID/outcome record or resume protocol needs its own explicit design;
this foundation does not reserve an unused framework for it.

## 9. Admission decisions and acceptance status

Before implementing the initial path, record:

1. The selected native transport, host/container route and authentication plus
   named Store authorization model.
2. Initial logical operations and exact version/identity/outcome semantics,
   including stable input and replacement replay ownership.
3. Numerical per-frame, per-operation, connection, queue and aggregate budgets,
   with pressure/cancellation behavior and the one-Store writer rule.
4. The finalized C1/C2 pin, schema/profile and relevant Stage 7 findings: coherent
   supplied-capacity validation, resource arithmetic, facade usage and any needed
   same-save behavior.
5. The actual Docker/host acceptance cases, direct/stream semantic parity and
   later pair 1 reuse route described in the sibling verification plan.

All topology execution, transport/security acceptance, latency/resource results,
cloud/provider execution and independent substitution checks in this proposal
are **NOT_RUN**. The deliverable here is the architecture and explicit boundary
contract to review. It preserves the existing algorithms until evidence calls for
a specific change; it neither creates speculative cloud components nor retires
the reference product.
