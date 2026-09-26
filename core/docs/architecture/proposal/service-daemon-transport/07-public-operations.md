# Pair 3 public operation catalog

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is the caller-facing catalog for the service/bridge/daemon foundation.
Names and entry-point shapes are proposed, not implemented Rust methods or frozen
wire opcodes. Source mappings use the [packet's recorded inspection basis](README.md#source-basis-and-evidence-status).
The [transport specification](03-operations-and-transport.md) owns framing,
versioning, input/completion states and failure handling; the
[resource plan](04-resource-and-verification-plan.md) owns limits and verification.
All runtime and performance checks for this catalog are **NOT_RUN**.

The [product implementation specification](implementation/01-product.md) assigns
these operations to implementation milestones; the [telemetry wiring spec](implementation/02-telemetry.md)
defines their optional recording wrapper without changing semantic outcomes.

## 1. Five logical operations

Every operation names an authorized logical Store. Store paths, SQLite handles
and physical object/pack operations remain service-local.

| Public operation | Caller supplies | Existing service-local C1/C2 composition | Successful result |
| --- | --- | --- | --- |
| `ReadFile` | Immutable file root, checked byte range and response allowance | C1 `read_range` with C2 `StoreProvider` | Ordered byte stream, exact returned length and terminal success |
| `Inspect` | Explicit file/filesystem root, supported typed query and any logical path/page bounds | Supported C1 inspection or `FilesystemRead` query with `StoreProvider` | Bounded typed metadata or listing page |
| `ConstructFile` | Exact declared length and stable sequential bytes; optional expected supported Store/profile identity | C1 `construct_stream` using Store-derived construction policy; C2 `SaveHandoff` and save completion | Saved file root, logical length and bounded save outcome |
| `EditFile` | Immutable base root/length, ordered supported edits and stable replacement parts | C1 `EditStream`/`EditSource`/`apply_edits`; local base provider and C2 save handoff | New saved file root/length and save outcome; old root remains readable |
| `UpdatePreparedFilesystem` | Prepared filesystem base, scope/root serial, sorted changed-name/inode records and retained content/metadata roots | C1 `FilesystemInput`/`FilesystemObjects`/`update_filesystem`; local C2 read/save composition | New saved filesystem root and save outcome |

These are semantic operations, not one RPC for every internal C1 or C2 call.
Their implementation bodies live in the service and are shared by network
delivery and direct parity invocation.

## 2. Common request and result rules

The request envelope identifies the operation and logical Store, carries a
connection-scoped correlation ID and caller input-generation label, and declares
the applicable input counts/lengths, response allowance and deadline budget.
Operation-specific roots and records follow the selected schema. Exact field
widths, query variants and numeric limits must be frozen before implementation.

The endpoint supplies a verified caller context; a request cannot claim its own
authority. The service authorizes each Store/operation, validates declared work
and admits it under the aggregate limits. Initial admission is Q=0: unavailable
capacity produces refusal, not an automatic queued retry. Correlation/generation
labels are not idempotency keys, Workspace sessions or conditional branch heads.

Input completion must match the declared totals. A mutation's root is successful
only after the required C2 save completion. Read result bytes remain provisional
until terminal success. Preserve known failure versus unknown outcome; closing a
connection or losing a response does not prove rollback and never authorizes
automatic replay. Saved-root success is not logical Commit/publication or crash
durability.

Expose the error distinctions available at each actual boundary. Bridge admission,
direct C1 errors and retained save failures can preserve their typed classes.
Current `StoreProvider` collapses several storage read causes into `ProviderFailure`;
the service must not reconstruct lost categories by parsing diagnostic strings.

## 3. Operation-specific boundaries

### ReadFile

Validate root role, range arithmetic and output allowance. Perform canonical
navigation and dependency reads locally through C1/C2, and stream logical bytes
through a bounded sink. A zero-byte result still needs terminal completion.
No full-file response accumulator or per-canonical-object network protocol is
introduced. See [C1 file reads](../../../../crates/layerfs-content/src/file/read.rs).

### Inspect

Use a closed, supported query set, not arbitrary method names, SQL or a generic
query language. Candidate filesystem queries include resolve/stat, bounded list,
readlink, portable metadata and attributes through
[FilesystemRead](../../../../crates/layerfs-content/src/filesystem/read.rs).
Finalize the precise file-inspection and filesystem-query variants separately.

Return related identity/attribute information together where the caller needs
it; do not require resolve followed by a second remote stat merely because C1 has
both methods. Listing needs count/byte limits and an explicit continuation.
This catalog does not freeze a cache, prefetch mechanism or FUSE directory cursor.

### ConstructFile

Adapt the bounded incoming stream to C1's sequential `Read` input, preserving
exact declared length and valid end-of-input before save completion. Obtain
construction policy from the selected Store; the caller cannot silently override
canonical policy. Empty construction uses the supported canonical representation.
See [C1 construction](../../../../crates/layerfs-content/src/file/content.rs).

### EditFile

Preserve C1's current-result coordinates and declared edit order. Its supported
edit stream rejects edits reaching into earlier introduced replacement bytes;
do not sort/coalesce records in the transport. The `Begin` frame declares the
descriptor count and replacement total; the descriptors ride the body stream as
its prefix, followed by the replacement bytes in edit order. The service parses
the prefix, validates the checked offset/length arithmetic (ordering, overlap,
accumulated final length inside the file ceiling) and C1 applicability, and
replays the replacement bytes from the bounded spool it took them into — a
resident window for small totals, one service-side file for large ones.

`EditSource` can reread replacement parts; the spool is that capability. The
retired 256-edit and 8 MiB replay caps are gone: the descriptor count is bounded
by the explicit per-operation edit budget shared with the builder (4,096), the
replacement total by the file ceiling, and both are refused before a stream byte
flows. See [edit input](../../../../crates/layerfs-content/src/file/edit/input.rs),
[edit application](../../../../crates/layerfs-content/src/file/edit/apply.rs)
and [the service's streaming edit input](../../../../crates/layerfs-server/src/service/save/edit_stream.rs).

### UpdatePreparedFilesystem

The initial public slice updates existing inode identities against a prepared
base. Directory records contain final bindings for **changed names**, not a copy
of every unchanged entry. Materialize finite checked records within their byte
and cardinality limits. New inode allocation requires a later explicit nonreuse
and ownership contract; supplying random serials is not a substitute.

Content/metadata roots must already be retained and readable through the ordinary
provider. The initial route does not assume a shared reader/consumer for new
unpublished content. See [filesystem input](../../../../crates/layerfs-content/src/filesystem/input.rs),
[object boundary](../../../../crates/layerfs-content/src/filesystem/objects.rs)
and [update](../../../../crates/layerfs-content/src/filesystem/update.rs).

Consequently, N file constructions/edits followed by one bounded tree attachment
cost N+1 logical exchanges today in the proposal. Earlier saves may remain
unreferenced if attachment fails. A future composite network operation could
orchestrate sequential existing saves with explicit partial-failure semantics;
an atomic combined save requires separate proof. Neither is an initial opcode.

## 4. Small public entry surface

| Component | Proposed public surface | Responsibility |
| --- | --- | --- |
| Bridge contract | Concrete request/result variants, semantic errors/outcomes andvalidated limits | Shared vocabulary, no native Store handles or caller-created authority |
| Bridge client | Connection lifecycle and one operation-call entry point with bounded input/output | Deliver a request and consume its result; no automatic replay |
| Bridge server | Serving entry point accepting a supplied handler | Decode/check delivery, invoke handler and encode/stream its result |
| Service | Concrete authorized operation handler | Compose C1/C2 and determine semantic completion |
| Daemon | Bounded stdin/stdout submission using the same bridge client | Real headless caller without a second operation framework/parser |

Conceptual call flow, **not frozen Rust signatures**:

```text
 Remote:
   Client.call(request, input, output)
       -> bridge delivery
       -> Service.handle(verified_context, request, input, output)

 Secondary direct parity:
   Service.handle(verified_context, request, input, output)
```

The main acceptance path is the actual Linux Docker daemon and native macOS host
service communicating over the selected authenticated network carrier. Direct
invocation exercises the same semantic body without wire encoding. No mandatory
`Operations` trait, registry, per-operation client class or separate bridge per
deployment is needed. Exact entry-point names and I/O ownership signatures remain
implementation decisions; convenience methods are not required to define the API.

## 5. Calls that stay inside the service

Store opening, `Store::policy/capacities`, `StoreProvider`, `Store::begin_save`,
`SaveHandoff`, `SaveOperation::finish/abort`, object reads and SQL remain local.
See [C2 Store/save facade](../../../../crates/layerfs-storage/src/cas/store.rs)
and [provider](../../../../crates/layerfs-storage/src/cas/provider.rs).

Do not expose remote begin-save/accept-object/finish-save commands. The service
owns one complete operation lifetime; the caller does not hold a network-visible
SQLite transaction or physical-object protocol. The bridge delivers results;
it cannot declare C2 success merely because the input was transferred.

Connection setup/close and deadline/disconnect handling are transport lifecycle,
not additional filesystem operations. Pair 3's container has no Workspace or
payload files; replay/storage resources belong to the host service as specified.

## 6. Later operation families

- Pair 1 may refine metadata results and add a bounded composite submission when
  actual Workspace needs justify it. FUSE callbacks still go through Workspace;
  each callback does not automatically become a remote operation.
- Pair 2 defines Commit, stage and conditional publication semantics before their
  operations are exposed.
- Mount/exec/stop belong to execution lifecycle and need their own explicit scope.
- Managed/serverless and durability work must preserve supported operation
  meanings or introduce an explicit versioned contract.

See [later FUSE/cloud integration](06-future-fuse-and-cloud.md). None of those
families expands the initial five-operation catalog by implication.
