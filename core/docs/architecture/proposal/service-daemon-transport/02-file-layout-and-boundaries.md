# Service, bridge and daemon: file layout and dependency boundaries

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is the proposed implementation layout for **pair 3, #181**. Pair 1 remains
projection/Workspace (#179), and pair 2 remains history (#180). The implementation
order is pair 3 → pair 1 → pair 2, as directed in the
[parent proposal](../04-boundary-and-trust.md). The initial target is the actual
Linux Docker daemon connected to the macOS host service. These directories,
modules and new API names are proposals; this document does not create them or
claim they compile.

Use one shared bridge contract with independently owned delivery-adapter folders,
and one logical implementation of each executable. Multiple bridge delivery
implementations can be added for real target requirements; local/Docker/native-cloud
placement can reuse the same adapter. The initial native adapter has client/server
endpoints over one selected carrier. Endpoint, credentials,
Store configuration and limits vary by deployment; platform-specific builds can
vary without forking operation behavior. Main acceptance uses networked separate
processes. The direct example is secondary parity through the same handler, not
another bridge crate or an embedded-daemon feature to implement now.

Existing-API statements were checked against the frozen source captured against
HEAD `795fb1a2f`, snapshot manifest SHA-256
`f916d0015473dcbb1ba893f6327c47a1a821e5419d785017ab62f090b55ff44e`.
The source includes captured working changes; the HEAD alone does not identify
it. Source links below resolve to the current checkout and must be reconciled
with the qualified baseline before implementation. This is not a Stage 6 result.

Read with [architecture and portability](01-architecture-and-portability.md),
[operations and transport](03-operations-and-transport.md), and
[resources and verification](04-resource-and-verification-plan.md). The operation
specification owns message semantics. The resource specification is the single
source of numerical limits and their configuration; this file defines ownership,
not another set of defaults.

## 1. Responsibility folders under three new crates

The user-directed structure separates independently changing responsibilities
rather than packing them into a few catch-all files. This remains a proposal;
create each production module with its implementation, not empty scaffolding.
C1, C2 and telemetry remain existing crates alongside the three new packages.

```text
core/crates/
+-- layerfs-content/                     existing C1
+-- layerfs-storage/                     existing C2; native SQLite today
+-- layerfs-telemetry/                   existing
|
+-- layerfs-bridge/
|   +-- Cargo.toml
|   +-- src/
|   |   +-- lib.rs                      declarations/reexports only
|   |   +-- contract/                   shared operation vocabulary
|   |   |   +-- mod.rs
|   |   |   +-- request.rs              envelope and five request variants
|   |   |   +-- response.rs             typed results and bounded metadata
|   |   |   +-- outcome.rs              semantic/delivery errors and certainty
|   |   |   `-- limits.rs               checked operation limits
|   |   `-- adapters/
|   |       +-- mod.rs
|   |       `-- native/                 initial selected network carrier
|   |           +-- mod.rs
|   |           +-- connection.rs       connect/accept, peer evidence, close
|   |           +-- client.rs           operation submission/result delivery
|   |           +-- server.rs           invoke supplied service handler
|   |           +-- payload.rs          bounded delivery sources/sinks
|   |           `-- protocol/
|   |               +-- mod.rs
|   |               +-- frame.rs        header/primitive framing and partial I/O
|   |               +-- metadata.rs     checked request/result field codec
|   |               `-- state.rs        grammar, counts, input/terminal states
|   `-- tests/                          external contract/framing/outcome tests
|
+-- layerfs-service/
|   +-- Cargo.toml
|   +-- src/
|   |   +-- lib.rs                      declarations/reexports only
|   |   +-- main.rs                     delegates to native startup
|   |   +-- owner.rs                    concrete Service and configured Stores
|   |   +-- operation/
|   |   |   +-- mod.rs
|   |   |   +-- dispatch.rs             logical request -> supported handler
|   |   |   +-- access.rs               per-Store/operation authorization
|   |   |   +-- admission.rs            aggregate active/resource limits, Q=0
|   |   |   +-- lifecycle.rs            save completion and safe failure cleanup
|   |   |   +-- read.rs                 ReadFile and Inspect composition
|   |   |   +-- write.rs                ConstructFile and EditFile composition
|   |   |   `-- filesystem.rs           prepared filesystem update composition
|   |   +-- input/
|   |   |   +-- mod.rs
|   |   |   +-- sequential.rs           exact-length construction input
|   |   |   +-- replay.rs               capped stable edit replacement source
|   |   |   `-- records.rs              bounded prepared filesystem records
|   |   `-- native/
|   |       +-- mod.rs
|   |       +-- config.rs               endpoint, credentials, Store paths, limits
|   |       `-- startup.rs              process/listener and handler assembly
|   +-- examples/direct.rs              secondary same-handler parity
|   `-- tests/                          operation/auth/input/cleanup behavior
|
`-- layerfs-daemon/
    +-- Cargo.toml
    +-- Dockerfile                      package the actual Linux executable
    +-- src/
    |   +-- main.rs                     delegates to run
    |   +-- run.rs                      assembly/startup/shutdown only
    |   +-- config.rs                   validated endpoint/credentials/limits
    |   `-- headless.rs                 stdin/stdout submission via bridge Client
    `-- tests/                          real daemon and Docker/host route
```

All `mod.rs` files shown above contain declarations/reexports/thin delegation,
not the state, types or implementations of their directory. Tests, fixtures,
synthetic peers and process drivers stay outside `src/`.

### File limits and size discipline

- **999 physical lines maximum** per production file, including comments/blanks.
- **200 physical lines maximum** for `lib.rs` and `mod.rs`, with the stricter
  declaration/delegation-only content rule.
- Keep each implementation focused and comfortably below the ceiling. The ceiling
  is not a target and file count is not a measure of simplicity. Split by real
  responsibility, not arbitrary numbered parts or one type/field per file.
- If `protocol/metadata.rs` grows, split request/result or filesystem-record codecs
  rather than weakening validation or creating a second parser. If `headless.rs`
  grows, replace it with `headless/{mod,session,relay}.rs`; do not keep parallel
  implementations. That session owns local submission lifetime, not another
  transport state machine.
- No placeholders for future adapters/providers, generated forwarding framework,
  test hooks, or implementation hidden in entry modules. New product formats must
  remain covered by the existing boundary checks.

This replaces the earlier flat file sketch as the proposed responsibility map.
It does not establish a new production LOC count: physical file limits and
nonblank/non-comment production LOC are different measures. The earlier total
LOC range remains an unmeasured planning estimate, to revisit after actual carrier,
security and schema choices; splitting folders does not prove a size or speed win.

## 2. Cargo dependency graph

Arrows below mean “depends on.” Solid edges are the proposed initial graph;
existing C2 → C1 and both core components → telemetry remain intact.

```text
layerfs-daemon [binary]
       |
       v
layerfs-bridge <---------------- layerfs-service [library + host binary]
       |                                  |             |
       |                                  v             v
       |                            layerfs-content <-- layerfs-storage
       |                                  |             |
       +-- no C1/C2/service dependency     +------v------+
                                           layerfs-telemetry

layerfs-service/examples/direct.rs
       +--> layerfs-service + layerfs-bridge

External Docker/host test driver
       +--> production daemon process + production host-service process
            (process control is not a daemon -> service Cargo dependency)
```

`layerfs-bridge` owns concrete request/result types and the remote `Client`.
The stream server accepts one supplied handler function/closure. The service
assembly supplies its concrete `Service` handler; the bridge never imports or
constructs that service. Direct calls enter the same service body with the same
logical input/result types and validation/authorization rules.

No mandatory `Operations` trait, dynamic registry, factory or per-op implementor
is needed for this composition. A future caller that really switches delivery
implementations can introduce the smallest interface it needs then. Sharing a
semantic contract does not require identical concrete caller types today.

The bridge contract uses logical Store identities, protocol root identities,
operation identifiers, validated byte/record inputs, outputs and error classes.
It does not import C1 solely to obtain every canonical type or expose C2 error
internals. The service translates these boundary values to C1's public types and
validates their semantics. This is transport representation, not a second
canonical-object codec. Do not serialize Rust memory layout or `usize` directly;
wire sizes and checked conversion rules belong to the operation specification.

The concrete native service may depend on selected carrier/credential libraries,
and the client may need the same carrier support. Reuse compatible dependencies
already present before proposing any new dependency; no third-party patching.
Native listener/connection setup must stay outside the semantic operation body.
Selecting a future HTTP/WebSocket carrier does not justify socket types in
the shared operation contract. A future async endpoint must preserve semantics and bounds; this
proposal does not claim synchronous native I/O is already a serverless runtime.

Direct invocation belongs in the service example and external parity tests.
Configurable placement does not require another launch framework, an optional
embedded-daemon feature or a daemon dependency on service/C2/native SQLite.

## 3. Dependency and responsibility rules inside the folders

The [public operation catalog](07-public-operations.md) owns the caller surface;
[document 03](03-operations-and-transport.md) owns native transport/lifecycle
behavior. Exact signatures, adapter selection and wire discriminants are still
proposal decisions, not compiled APIs.

```text
bridge/contract                 no carrier, C1/C2, SQL or runtime dependency
       ^
       |
bridge/adapters/native          native connection and protocol mechanics
       ^                  ^
       |                  |
daemon/headless        service/native/startup
                              |
                       supplied handler
                              v
                       service/owner
                              |
                    operation/access + admission + dispatch
                              |
                    read / write / filesystem
                              |
                      input adapters + C1/C2
```

| Owner | What it owns | What stays outside |
| --- | --- | --- |
| `bridge/contract/` | Logical requests/results/outcomes and checked common limits | Native sockets, HTTP objects, SQL, Workspace state and caller-created authority |
| `bridge/adapters/native/connection.rs` | Selected carrier setup and peer evidence; connection close | Store permissions, C1/C2 and deployment-specific service behavior |
| `bridge/adapters/native/protocol/` | Frame/field codec and legal wire transitions | Storage algorithms, replay backing and semantic success decisions |
| `bridge/adapters/native/{client,server,payload}.rs` | Bounded request/result delivery around the supplied handler | Unbounded queues, automatic replay and a second service implementation |
| `service/owner.rs` | Configured Store lifetimes and the concrete Service facade | Pending Workspace overlays or remote file handles |
| `service/operation/access.rs` | Verified identity -> Store/operation permission, including direct calls | Trust in an identity supplied by the request itself |
| `service/operation/admission.rs` | Active operation and handler resource accounting, immediate refusal | Waiting scheduler; hidden construction workers |
| `service/operation/lifecycle.rs` | Existing single-attempt C2 finish/abort/error retention | Transport grammar, durable recovery or completion redelivery |
| `service/operation/{read,write,filesystem}.rs` | Five logical operations composed from real C1/C2 APIs | Native listeners and per-canonical-object RPCs |
| `service/input/` | Sequential, replay and finite-record capabilities C1 actually requires | Generic collect-everything buffer or container spool fallback |
| `service/native/` | Config, credentials/verification policy, native process and bridge-server assembly | Operation algorithm forks for Docker or cloud placement |
| `daemon/{config,run,headless}.rs` | Validated settings, assembly, local submission and bounded result relay | Another frame parser, SQL, storage ownership or history engine |

Host setup supplies authentication policy to the selected endpoint and binds
verified caller context to the handler. The service authorizes every operation,
including direct calls. Connection admission resources are owned at the endpoint;
handler admission is owned at the service. They use declared aggregate limits,
not duplicate counters that each pretend to bound the whole process.

This applies SRP through separate reasons to change: operation schema, native
framing, carrier setup, authorization, storage composition and input lifetime.
The supplied handler maintains dependency direction without an interface per
file. Substitution requires matching semantics, errors and bounds through the
same external contract tests; a new adapter cannot silently weaken those rules.

## 4. A real daemon interface before FUSE exists

The proposed first production daemon supports a minimal **headless operation
submission mode using standard input and standard output**. It is useful for
scripted clients as well as the integration driver. Standard input carries the
bounded request/payload representation from document 03; standard output carries
results/payloads, and diagnostics use standard error. The daemon reads those
requests, invokes its configured bridge `Client`, forwards results and performs
normal shutdown/cleanup. Reuse the bridge frame decoder for this bounded local submission form; do not
create another parser, control endpoint or protocol negotiation stack. The local
stream does not mint authority: the configured client authenticates to the host.

Early host refusal can leave unread BODY frames on stdin. Close/terminate the
local submission session unless its input is known synchronized; never parse
leftover payload as the next command or drain an unbounded upload to reuse it.
The daemon must consume host responses concurrently with bounded input relay so
that refusal reaches a blocked upstream producer.

The external driver starts the actual host service, starts the actual Linux
container running the daemon binary with input/output attached, submits bytes
through this control interface, and checks the returned result. It must not
replace the daemon with a test program that calls `Client` directly. The driver
owns fixture generation, assertions, process termination and evidence recording;
the production executable owns real framing, connection, admission and cleanup.
No hidden “test mode,” alternate handler or mock storage is permitted.

The host driver supplies bytes incrementally from a generator or host-owned
fixtures, feeds daemon stdin, and checks streamed stdout. Pair 3 creates no
Workspace directory or payload, spool or result files inside the container,
including temporary file-backed payload staging. It requires no LayerFS/FUSE
mount, `/dev/fuse`, host-directory bind mount or shared-data volume. Docker's
ordinary image/runtime filesystem is outside this application-data restriction;
the daemon executable and configuration still exist normally.

Real persistence occurs in the host service's SQLite Store. The initial edit
replay buffer is capped service memory; any operation-required native scratch
uses its declared host-side owner and budget. Do not introduce container files
or an automatic spool fallback to accommodate a request beyond its supported
limits. A prepared filesystem root in the host Store is logical content, not a
mounted or materialized sandbox Workspace.

Pair 1 later uses **FUSE -> Workspace -> bridge Client**. FUSE delegates local
filesystem behavior to Workspace; Workspace owns the operation caller. The
headless adapter uses that Client directly. Neither FUSE nor Workspace must
encode stdin control frames. No FUSE callback or mount acceptance is claimed by
the pair 3 driver; see [later integration](06-future-fuse-and-cloud.md).

## 5. Reuse the existing C1/C2 public composition

| Service job | Existing public API | Source |
| --- | --- | --- |
| Open configured native storage and obtain its persisted policy | `Store::create`, `Store::open`, `Store::policy`, `Store::capacities` | [Store](../../../../crates/layerfs-storage/src/cas/store.rs) |
| Read authenticated canonical objects locally for C1 | `StoreProvider` implementing `AuthenticatedObjects` | [provider](../../../../crates/layerfs-storage/src/cas/provider.rs) |
| Construct a complete file from stable bytes | `construct_stream` or bounded `construct_bytes` | [construction](../../../../crates/layerfs-content/src/file/content.rs) |
| Apply known final edits | `apply_edits`, `EditRequest`, `EditStream`, `EditSource` | [apply](../../../../crates/layerfs-content/src/file/edit/apply.rs), [input](../../../../crates/layerfs-content/src/file/edit/input.rs) |
| Read logical file ranges or inspect paths | `read_range`, `FilesystemRead` | [file read](../../../../crates/layerfs-content/src/file/read.rs), [filesystem read](../../../../crates/layerfs-content/src/filesystem/read.rs) |
| Update a prepared filesystem | `FilesystemObjects`, `FilesystemInput`, `update_filesystem` | [objects](../../../../crates/layerfs-content/src/filesystem/objects.rs), [input](../../../../crates/layerfs-content/src/filesystem/input.rs), [update](../../../../crates/layerfs-content/src/filesystem/update.rs) |
| Persist finalized output and report completion | `Store::begin_save`, `SaveHandoff`, `SaveOperation::finish`, `SaveOperation::abort` | [Store/save and handoff](../../../../crates/layerfs-storage/src/cas/store.rs) |

For a save operation the service obtains construction policy from the Store,
creates a published-base `StoreProvider`, begins a save, and passes a
`SaveHandoff` consumer to C1. It retains the C1 result and the handoff's original
storage failure, then finishes or explicitly handles abort/cleanup as applicable.
Only a successfully finished save can produce the completed-save root result.
The service must not convert dropped response delivery into automatic replay,
guessed rollback or a logical Commit. Detailed state/error rules are in document
03; this outline is not executable code.

The prepared-filesystem request contains sorted, unique final bindings **for
changed names**, typed final inode values and explicit identities. It does not
require resending unchanged directory entries. New inode serials additionally
require the caller's never-reuse/allocation contract; the initial existing-inode
path avoids inventing that later Workspace/history responsibility.

Keep all `AuthenticatedObjects` and `FinalizedConsumer` callbacks inside the
service. `begin_save(&self)` returns a save handle, allowing a published-base
provider and a new output handoff to coexist. Reading a new unpublished root
through another C1 operation requires a separate explicit capability decision;
there is no ready same-save reader/consumer bridge to silently assume. Do not
force intermediate publication merely to work around borrowing or visibility.

## 6. Stable input is a real resource, not a wire promise

`construct_stream` accepts sequential `Read`, but this does not make every C1
operation consume an arbitrary one-pass network stream. Current `EditSource`
requires indexed `replacement_len`/`read_at`; `FilesystemInput` borrows finite slices, and its
validation/update code revisits records. Native ordering spill is available via
[OrderingBacking/FileBacking](../../../../crates/layerfs-content/src/filesystem/references/backing.rs).

Consequently `service/input/` must make each operation's input capability
explicit: bounded in-memory input, a declared stable random-access/replay backing,
or refusal when the capability is unavailable. The selected bounds and backing
policy come from document 04. An iterator over immutable prepared records can
avoid unnecessary cloning, but cannot erase a current API's borrowed-slice or
random-access requirement. Claims of large streamed edits must identify how
those required re-reads work.

Input retention, incomplete upload, C1 scratch, ordering spill and result output
have named owners and bounded lifetime. A bounded frame size alone does not
bound aggregate retained input. Temporary native paths are service-local choices
and never appear as client-requested write destinations. Managed runtimes may
lack those backing capabilities; they must refuse unsupported operations or
supply a qualified replacement, not silently collect unlimited data in memory.

## 7. Future adapters and database providers are different extension points

The shared operation contract can support several delivery implementations:

```text
layerfs-bridge/src/
+-- contract/                         shared meanings, limits and outcomes
`-- adapters/
    +-- native/                       initial implementation
    +-- http/                         FUTURE, only for a selected real target
    |   +-- mod.rs
    |   +-- client.rs
    |   +-- server.rs
    |   `-- mapping.rs                operation <-> HTTP representation
    `-- websocket/                    FUTURE, only when required
        +-- mod.rs
        +-- client.rs
        +-- server.rs
        `-- mapping.rs                messages, limits and terminal outcomes
```

The future folders are design locations, not files/features to create now.
Do not add local/Docker/cloud copies of the same adapter: those are usually
endpoint/configuration choices. Direct invocation is the existing service body,
not a delivery adapter that needs its own implementation.

HTTP/WebSocket may use different async I/O, buffering and lifetime APIs. Preserve
logical operation semantics without forcing every future target through native
`Read`/`Write`, HELLO or the same frame codec. A small operation-level caller
interface is justified when actual adapters need interchangeable callers; add
that boundary rather than exposing sockets to Workspace or creating a registry.
Native dependencies must not enter `contract/`. When a second target exists,
qualify feature/dependency isolation and use a separate adapter crate only if a
real build boundary warrants it. Folders alone do not prove target portability.

### Storage placement does not select a bridge implementation

A different database is a C2 persistence adaptation, independent of the delivery
adapter. Keep native SQLite for the initial pair. A later concrete provider may
justify this internal extraction:

```text
service operation handlers
         |
         v
C1 <-> C2 canonical/storage ownership and physical representation rules
         |
         v
future bounded persistence capability
         +--> native SQLite
         +--> managed SQLite           FUTURE
         `--> other qualified database FUTURE
```

Possible future C2 location, only when a real second provider is implemented:

```text
layerfs-storage/src/
+-- cas/                               Store/read/save ownership
+-- encoding/                          physical representation
+-- pack/                              pack algorithms
`-- persistence/                       FUTURE extraction from current sqlite/
    +-- mod.rs
    +-- contract.rs                    actual grouped I/O and save capabilities
    +-- native_sqlite/                 migrate current implementation once
    `-- selected_provider/             actual alternative, not a placeholder
```

Current `sqlite/` remains in place now. The future tree is a replacement/extraction
of provider-specific code, not permission to maintain duplicate native providers.
The contract must cover bounded grouped reads/writes, integrity/dependencies,
whole-save ownership, visibility, transactions, format/capacity rules, cleanup and
known/unknown outcomes. It must not become an interface for every SQL statement.
A different engine must qualify those semantics and declare format/import changes;
C1 identity and bridge compatibility are conditional on their actual guarantees.

Today's C2 still opens native paths, exposes native engine errors and links native
SQLite/codec dependencies. A future provider may need actual refactoring and build
isolation; the service is not already database-agnostic because this diagram exists.
No provider registry or database implementation is added in pair 3.

### Later Workspace, FUSE and history

Pair 1 introduces `daemon/workspace/` for local mutable state and `daemon/fuse/`
for a thin Linux projection. See the focused future tree in
[document 06](06-future-fuse-and-cloud.md#future-responsibility-folders).
Workspace depends on logical bridge calls; FUSE depends on Workspace. Neither
acquires service/C2/SQL dependencies or its own wire parser. Pair 2 adds history,
allocation ownership and conditional publication outside C1/C2 under its own
contract. Transport changes do not create a Workspace or a logical Commit.

### Telemetry integration is separate from transport and persistence

Use one `layerfs-telemetry` crate. Portable timing/report/operation modules stay
independent of optional native runtime/platform/output modules in that same crate.
Service/daemon assembly explicitly enables and configures native facilities;
Workspace/FUSE reuse the daemon's process monitor. See
[document 08](08-telemetry-and-retention.md) for feature boundaries, disabled
behavior, configuration and retention. No second telemetry package is proposed,
and compiling/importing modules does not start a sampler or exporter.

## 8. Layout acceptance

- The daemon and bridge dependency graph has no service/C2/native SQLite edge in
  the supported remote path, and the bridge never depends on the concrete Service supplied to its handler.
- Direct and remote routes enter the same authorized logical operation bodies.
  Both preserve C1/C2 error distinctions and explicit completion semantics.
- The Docker/host check launches both production processes and submits through
  the daemon's supported control path; a bridge-client-only driver is insufficient.
- Shared limits have one validated definition and are applied at both admission
  boundaries. Native Store paths, SQL and credentials cannot enter wire requests.
- Only implemented files/members are created. Product code, external tests,
  examples and deployment files remain separated under the core boundary rules.
- The implementation pins the reconciled core revision and records any required
  core contract correction rather than modifying C1/C2 covertly in this work.

This document changes documentation only. No product file, Cargo member, endpoint,
Docker image or runnable verification result is supplied by the proposed tree.
