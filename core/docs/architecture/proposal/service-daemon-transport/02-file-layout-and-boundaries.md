# Service, bridge and daemon: file layout and dependency boundaries

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is the proposed implementation layout for **pair 3, #181**. Pair 1 remains
projection/Workspace (#179), and pair 2 remains history (#180). The implementation
order is pair 3 → pair 1 → pair 2, as directed in the
[parent proposal](../04-boundary-and-trust.md). The initial target is the actual
Linux Docker daemon connected to the macOS host service. These directories,
modules and new API names are proposals; this document does not create them or
claim they compile.

This is one bridge library and one logical implementation of each executable,
not separate local/Docker/cloud variants. The bridge has client and server
endpoints with one selected native network carrier initially. Endpoint, credentials,
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

## 1. Three new crates, existing core unchanged

```text
core/
+-- Cargo.toml                         add real members when implemented
+-- Cargo.lock                         existing locked dependency resolution
+-- crates/
|   +-- layerfs-content/               existing C1
|   +-- layerfs-storage/               existing C2, native SQLite initially
|   +-- layerfs-telemetry/             existing operation timing
|   |
|   +-- layerfs-bridge/
|   |   +-- Cargo.toml
|   |   +-- src/
|   |   |   +-- lib.rs                 declarations/reexports only
|   |   |   +-- contract.rs            request/result, errors and validated limits
|   |   |   +-- frame.rs               one bounded encoder/decoder
|   |   |   +-- client.rs              concrete remote Client
|   |   |   `-- server.rs              stream loop calls supplied handler
|   |   `-- tests/                    external framing/outcome checks
|   |
|   +-- layerfs-service/
|   |   +-- Cargo.toml
|   |   +-- src/
|   |   |   +-- lib.rs                 declarations/reexports only
|   |   |   +-- main.rs                delegates to host startup
|   |   |   +-- host.rs                config, authentication, listener/lifecycle
|   |   |   +-- service.rs             authorize/dispatch, real C1/C2 read/save
|   |   |   `-- input.rs               stable bounded C1 input and edit replay
|   |   +-- examples/direct.rs         same Service body without the wire
|   |   `-- tests/                    operation and authorization checks
|   |
|   `-- layerfs-daemon/
|       +-- Cargo.toml
|       +-- Dockerfile                package actual Linux binary
|       +-- src/
|       |   +-- main.rs                delegates to run
|       |   `-- run.rs                 config, headless relay, process lifecycle
|       `-- tests/                    real process and Docker/host checks
`-- docs/architecture/proposal/service-daemon-transport/
    `-- ...                            design only
```

This is a responsibility map, not permission to create empty scaffolding. Add a
file with its real implementation; merge a tiny responsibility into its existing
owner where that remains clear. There is no separate protocol crate, generated
provider registry, generic application framework or parallel implementation of
C1/C2. The chosen stream carrier may need a small carrier-specific module once
its concrete dependencies and platform behavior are settled; do not pre-create
one for each possible transport.

Production files remain at most 999 physical lines. `lib.rs` and `mod.rs` remain
at most 200 and contain declarations/delegation only. Put types, validation,
branching and I/O in the named modules. All tests, synthetic peers, process
controllers and fixtures stay outside `src/`. The Docker integration driver may
have external helpers under `tests/support/` when shared work warrants them; no
test-only public entry point or fault switch belongs in the daemon.

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

## 3. File-level responsibilities and public seams

All names in this table are **proposed**, except the C1/C2 APIs in section 5.
Exact function signatures and protocol discriminants belong to the operation
specification; they have not been implemented or compiled.

| File/module | Initial responsibility | Boundary |
| --- | --- | --- |
| `bridge/contract.rs` | Shared request/result/error types and validated limits | No SQL, native paths, Workspace state or caller-created authority |
| `bridge/frame.rs` | One encoder/decoder reused by native link and headless submission | Validate lengths/counts before variable allocation; exact completion |
| `bridge/client.rs` | Concrete remote call and bounded result handling | No reconnect-and-replay, polling or guessed rollback |
| `bridge/server.rs` | Stream decode/dispatch/encode through supplied handler | No C1/C2 algorithms or Store opening |
| `service/host.rs` | Config, credentials/authentication, listener and process/connection lifecycle | Deployment details remain outside operation types |
| `service/service.rs` | Store lifetime, per-operation authorization/admission, dispatch and C1/C2 read/save composition | Direct and remote callers share one authoritative operation body |
| `service/input.rs` | Sequential input, bounded replay/record backing and cleanup | Actual C1 capabilities and aggregate resources, not unlimited buffering |
| `daemon/run.rs` | Native config, supported stdin/stdout submission and configured bridge client | Reuses framing; no separate control protocol, SQL or history engine |

Do not pre-split configuration, access checks, read/save dispatch, limits and
errors into one file per concern. Split a real implementation when its cohesion
or the production line ceiling requires it. Authentication setup belongs to the
host endpoint; authorization remains in the service for **every** operation,
including direct calls. Combining files must not bypass either check.

An operation context supplied to the service must be minted/validated by the
trusted endpoint or embedded caller setup; a wire field cannot assert its own
authorization. Authentication carrier choice and per-operation authorization are
specified in documents 01/03. Framing code has no independent authority to grant
Store access.

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

Consequently `service/input.rs` must make each operation's input capability
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

## 7. Later components and honest portability boundary

Pair 1 adds actual FUSE callbacks, Workspace lifecycle/overlays, file handles and
accumulated mutation scheduling on the execution side. Their public-operation
caller should depend on bridge interfaces; it must not acquire a dependency on
service SQL/pack modules. No empty Workspace/FUSE crate is added in this phase.
Pair 2 adds history, allocation ownership, stage/Commit identities and conditional
head publication outside C1/C2, with its own explicit integration/schema design.
The saved-root response remains distinct from those future operations.

The current service links C2/native SQLite; the default remote daemon and bridge
do not. Keep provider extraction, alternate carriers and target-specific builds
for a concrete future target. The dependency rule is enforceable now; managed
runtime support is not. See [architecture boundaries](01-architecture-and-portability.md)
and the [future integration proposal](06-future-fuse-and-cloud.md) for provider,
codec, scratch and lifecycle requirements. No provider registry or unused embedded
feature is added in this foundation.

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
