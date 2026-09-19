# Later Workspace/FUSE and cloud integration

> **Status:** Proposal; target LayerFS v0.1.7 and later integration; not a released contract.

This document records how later consumers and deployment targets can use the
[transport foundation](README.md). It does not add their implementation to pair 3.
Pair 3 establishes bounded host-service/Docker-daemon delivery; pair 1 introduces
Workspace/FUSE; pair 2 introduces history. Cloud/serverless and durability remain
future work. All integration and performance checks below are **NOT_RUN**.

Pair 3's container has no Workspace or payload/spool/result files, no FUSE mount
or device, and no host-directory bind mount/shared-data volume. The host driver
feeds stdin and checks stdout; real content crosses the daemon/service network
connection and persists in host SQLite. Introducing local Workspace backing and
the Linux FUSE mount is an explicit pair 1 scope expansion, not a prerequisite
for transport acceptance.

Source basis and qualifications are the [packet's inspection manifest](README.md#source-basis-and-evidence-status),
the [legacy comparison](05-v0.1.6-comparison.md), and the existing
[SQLite portability investigation](../../../../../docs/roadmap/0.1/0.1.7/study/service-runtime-cloud-path/README.md).
Module and operation names below are proposed responsibilities, not existing APIs.

## 1. Keep two independent integration boundaries

```text
 CALLER SIDE                                   OPERATION OWNER SIDE

 Headless caller --+                           +-- Native host service
                  |                           |      C1 + C2 + SQLite
 FUSE -> Workspace+-- bridge client ---------->|
                  |    logical operations     +-- Future native cloud service
 Other real caller+    bounded input/results   |      C1 + C2 + SQLite
                                              |
                                              +-- Future managed service
                                                     qualified C1/C2 + provider
```

Adding FUSE changes the caller. Hosting the service elsewhere changes its endpoint
and deployment. Neither should put FUSE types, SQLite handles, native file paths
or mount/session state into generic framing. Preserving operation semantics does
not promise an unchanged binary, identical wire framing, or automatic compatibility
with every runtime.

The native service boxes are placements of one logical service implementation,
with platform-specific builds and configuration; the daemon likewise has one
logical implementation. They are not local/Docker/cloud application forks or
separate bridge libraries. A future managed runtime requires explicit carrier,
provider and lifecycle adaptation while preserving supported operation semantics.
The main pair 3 proof remains the separate Linux Docker daemon and native macOS
service over the selected network carrier; direct calls provide secondary parity.

The bridge owns delivery and transport failure. Service handlers own authorization
and C1/C2 operation outcomes. Workspace owns live mutable filesystem state. Keep
these owners distinct without creating an interface or crate for every helper.

## 2. FUSE: Workspace takes the useful LiveOwner responsibilities

```text
 LINUX DAEMON PROCESS
 +----------------------------------------------------------------+
 | Startup / native config / shutdown                              |
 |                                                                |
 | Application -> Linux kernel -> FUSE adapter                     |
 |                                |                               |
 |              callbacks / kernel identity / errno translation   |
 |                                v                               |
 | Workspace                                                      |
 |   namespace and inode identity                                 |
 |   open-file lifetime and pending changes                        |
 |   local read-your-writes and bounded backing/cache ownership    |
 |   stable submission generation and revision-safe reconciliation |
 |                                |                               |
 |                         bridge client                          |
 +--------------------------------+-------------------------------+
                                  |
                                  v
                 service -> C1 + C2 -> SQLite
```

Do not recreate the v0.1.6 combined `LiveOwner`. Borrow its behavioral requirements:
local writes, immutable-base reads, stable frozen input, inode/handle lifetime,
bounded caches/backing and completion that cannot discard newer writes. Exclude
its reverse snapshot-pull protocol, transport authentication, host-control framing
and automatic completion redelivery from Workspace.

The daemon creates and wires Workspace, the FUSE adapter and the bridge client.
It hosts Workspace; it does not duplicate its overlay or generation registry.
The FUSE adapter delegates filesystem behavior to Workspace and never bypasses
it with a remote request for each callback. Kernel handle-number translation can
live in the adapter; semantic file lifetime and revisions have one Workspace owner.

### Smallest plausible file placement

```text
 core/crates/layerfs-daemon/src/
 +-- main.rs                  delegates to startup
 +-- run.rs                   assembly, configuration and native lifecycle
 +-- workspace.rs             transport-independent mutable filesystem owner
 `-- fuse.rs                  Linux-only callback adapter -> Workspace

 Existing bridge/service crates keep their roles.
```

This is a future responsibility map. Start Workspace as a module; split focused
files when implementation or the 999-line ceiling requires it. A separate
Workspace crate becomes useful when a second real consumer or build boundary
needs it. No empty crate, generic runtime framework, or second `LiveOwner` wrapper
is introduced now. Platform-gate FUSE and its dependency; a headless build must
not require mounting capability. Module names do not waive the product-only source
and external-test rules in [core/AGENTS.md](../../../../AGENTS.md).

### Callback and operation mapping

| Activity | Workspace work | Possible service work |
| --- | --- | --- |
| Lookup/getattr/readlink | Overlay and immutable metadata first | One logical query returning needed identity/attributes; avoid mandatory resolve-then-stat calls |
| Read | Merge local changed bytes with immutable base ranges | Bounded range reads for actual misses; canonical traversal stays service-local |
| Write | Install readable local bytes under a declared pending-byte limit | Submission only at the defined boundary or pressure policy; not each ordinary callback |
| Readdir | Merge names and preserve valid cookies/generations | Incremental bounded entry-plus-attribute pages, without acquiring all pages merely to serve the first |
| Create/rename/unlink/setattr | Namespace, open-handle and allocation semantics | Bounded filesystem update once its required contract exists |
| Save/submission | Freeze stable input and apply a confirmed result | Existing file/tree operations, or an explicitly designed composite handler |
| Flush/fsync | Explicitly defined volatile filesystem semantics | No implicit logical Commit, durable acknowledgement or persistence guarantee |

New inode creation needs an allocation/nonreuse agreement with the appropriate
history/identity owner. A transport root or correlation ID cannot supply it.
Likewise, rename/open-unlinked-file behavior must be specified as filesystem
semantics, not inferred from the existence of an update opcode.

FUSE can overwrite bytes written earlier in the same dirty generation. Current
C1 [EditStream](../../../../crates/layerfs-content/src/file/edit/input.rs) rejects
edits that reach into earlier introduced replacement bytes. Therefore Workspace
must lower its final piece/namespace state to a supported stable operation input;
it cannot forward the chronological FUSE write list directly to `apply_edits`.
Specify this lowering and its canonical/profile consequences as a Workspace
algorithm. Do not sort/coalesce raw edits in the bridge or silently flatten them
to a complete file to evade a replay limit.

### Generation and failure rules

```text
 local writes -> freeze bounded generation G -> submit stable G
                       |                              |
                       | later writes remain newer    | service result
                       v                              v
               live successor state <---- reconcile only covered revisions
```

If writes are allowed during a submission, older completion must never erase
newer edits. State retained for G, its successor and any readers must all fit the
Workspace resource budget. Reject/block at a declared capacity boundary rather
than silently building an unlimited chain of frozen generations.

A failed or lost response does not authorize replay. Preserve uncertain state
and report unknown outcome where required. Recovery/reconciliation beyond a known
terminal response is a separate explicit contract. Root-save completion remains
distinct from later history publication and future crash durability.

## 3. What FUSE actually requires from the transport

Most additions are caller/operation work, not framing work:

- Keep exact input length, bounded streaming, backpressure and terminal outcomes.
  Workspace must retain stable bytes for the period the service operation needs.
- Refine metadata result shapes around actual FUSE needs. Preserve useful fused
  lookup/attribute results. Cache/prefetch policy belongs to caller/handler code,
  with measured work and explicit bounds, not a generic transport cache.
- Evaluate one bounded multi-file submission only when the real Workspace path
  warrants it. N individual file saves plus a tree update still cost N+1 network
  exchanges. One composite network request may orchestrate those saves locally;
  earlier saves can remain after a later failure. Atomic combined storage needs
  separate proof. Neither option adds a transport-level batch scheduler.
- Qualify read/control progress during bulk saves. Pair 3 has one active operation
  per connection and immediate admission refusal, not a FUSE fairness guarantee.
  A small fixed set of separately bounded connections may suffice. Additional
  sockets alone do not solve global handler or SQLite contention; qualify admission,
  execution ownership and permitted read progress together. Do not add construction
  workers to hide the problem.

Do not add FUSE opcodes, snapshot capture/completion commands, leases, remote file
handles, session recovery or a service-side Workspace registry without a concrete
semantic requirement. Mount/exec/stop remain execution-lifecycle concerns; choose
their actual control path when implementing them rather than folding Docker into
the storage handler.

## 4. Cloud path one: move the native service near SQLite

```text
 LOCAL OR CLOUD LINUX EXECUTOR                  NATIVE CLOUD HOST
 +-----------------------------+               +-----------------------------+
 | Workspace + optional FUSE    |               | Service                     |
 |             |               | authenticated |   common operation handlers |
 |        bridge client -------+---- network ->|   C1 + C2 + native SQLite   |
 +-----------------------------+               +-----------------------------+
```

This is the first cloud deployment to evaluate. Keep C1/C2 and native SQLite
together. Configure a real endpoint and an authenticated, confidentiality-protected
network route. Verify credentials, caller-to-Store authorization, process/resource
ownership and lifecycle on that target. A native cloud process does not imply
crash durability, replicated storage or unlimited horizontal writers.

Connection latency and bandwidth differ from host/Docker. Revisit finite windows
and deadlines with evidence; persistent sessions avoid repeated setup only while
the platform actually supports their lifetime. Transferring a large body can take
many packet RTTs even with one logical operation. Do not expose SQLite files or
individual SQL statements through the application bridge to obtain remote placement.

This route can initially keep native storage code, but its authentication,
deployment, failure and resource behavior still require real execution evidence.
No cloud infrastructure, alternate carrier or durability work is included in pair 3.

## 5. Cloud path two: managed/serverless execution later

```text
 Native/other supported caller
          |
          | versioned logical request + bounded bytes
          v
 Target-supported HTTP/WebSocket endpoint
          |
          v
 Authorized service operation owner
          |
          +--> target-compatible C1 execution / stable input / scratch
          |
          `--> C2 storage rules -> qualified managed SQLite provider
```

This needs two adaptations: delivery into the target runtime and C2 access to its
actual SQLite provider. It may also require changes to synchronous execution,
codecs and native scratch capabilities. A new transport or database connection
string alone is insufficient.

| Boundary | Required future decision/proof | Keep unchanged where supported |
| --- | --- | --- |
| Carrier | Map request/input/result/terminal semantics to actual HTTP/WS APIs; prove early refusal, message caps and slow-peer bounds | Logical operation meanings and exact completion rules |
| Authentication | Target identity and credential lifetime; authorize each Store/operation | A Store name/hash is not authority |
| Runtime lifetime | Reconstruct permitted service state after restart; define fate of interrupted input/core work | No assumption that a live connection preserves an active save |
| Core execution | Real target build and synchronous/async ownership model; compatible codec and integer/byte behavior | Canonical semantics and declared profiles, subject to qualification |
| Stable input/scratch | Replay and ordering backing that fits platform capabilities and admitted workloads | Finite byte/count/lifetime limits; explicit refusal if unsupported |
| SQLite provider | Grouped I/O, transactions, publication, limits/errors, physical-format compatibility | C2 logical guarantees actually implemented by that provider |
| Durability later | Failure domain, publication ordering, recovery, backup/restore and stronger acknowledgement | No silent upgrade of current no-sync/no-WAL success meaning |

The native `HELLO` handshake and byte-frame protocol need not be copied verbatim
into an HTTP body or a WebSocket message. Preserve the semantic contract and
version the actual mapping. HTTP/WS runtime buffering, half/full-duplex behavior
and request lifetime must be checked; a carrier that cannot meet required bounds
must refuse that operation or receive an explicit revised contract. Do not hide
a whole-request buffer behind the word streaming.

Do not freeze an async-core framework, generic SQLite provider registry, background
recovery agent, connection resumption or replay ledger now. Introduce the smallest
provider/lifecycle adaptation once a real target demonstrates its requirements.
Durability remains later work; target persistence by itself does not complete a
LayerFS durable-operation contract.

A serverless service does not need FUSE. A native Linux executor can mount FUSE
and contact that service. An isolate caller needs an appropriate logical or virtual
filesystem API; the native kernel mount interface is not a portable requirement.

## 6. Integration acceptance, all NOT_RUN

| Later scope | Evidence required |
| --- | --- |
| Workspace without FUSE | Local read-your-writes, stable generations, bounded backing/replay, revision-safe known completion and unknown-outcome handling through the real service path |
| Linux FUSE | Actual mounted read/write/metadata/namespace behavior, handle/cookie lifetimes, explicit allocation rules and flush/fsync contract |
| FUSE under pressure | Full resource accounting, slow upload/download, aggregate admission, concurrent read/control progress and disconnect behavior |
| Native cloud | Actual authenticated deployment, authorization isolation, same operation results, latency/resource accounting and declared process/storage failure semantics |
| Managed/serverless | Real target build/provider execution, request/lifecycle limits, codec/scratch support, format compatibility, interruption and restart handling |
| Future durability | Separately approved stronger contract and recovery/failure evidence; not inferred from any row above |

Direct/remote semantics must agree for supported operations, but acceptance in
one environment does not qualify another. Follow the existing resource/measurement
contract when making performance claims. These checks introduce neither a new
benchmark registry nor an aggregate preflight/CI gate.
