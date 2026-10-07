# SDK-owned scoped host runtime

> **Status:** Historical implementation record. The host-mediated transport
> described here was retired by F12 after `52e1f2e18`; use
> [the current retirement/Init guide](66-host-transport-retirement.md).
> Original source links below refer to their recorded Git revisions, not the
> current tree. Historical measurements and verdicts are unchanged.

The #307 checkpoint after `042849109` embeds an initial runtime at the planned
SDK composition path (`core/crates/layerfs-api/sdk/src/runtime/mod.rs` at local Git `52e1f2e18`). The active
package is `layerfs-sdk`; its normal product dependency graph contains content,
storage, history, persistence, telemetry and the native bridge. It has no server
dependency. Logical bridge service, history adapters, five Workspace operations
and complete S9 exits remain unfinished.

Runtime (`core/crates/layerfs-api/sdk/src/runtime/owner.rs` at local Git `52e1f2e18`) receives already-open
`Handles`, initializes one demand Storage plus configured independent Save Storage
handles once, and owns application authority. It neither creates/reopens the
provider per call nor starts another named service. The current owning provider
is macOS; Linux compilation does not establish that provider's availability there.

`Runtime::sessions` borrows those stable owners for a serving scope. The registry
and `Save<'_>` values sit outside their Storage owners with ordinary lifetimes;
there is no self-reference, unsafe lifetime extension or whole-Commit checkout.
The serving scope must stay alive across transport connections and tool calls.
Caller-controlled terminal scope drop ends local producer ownership; it does not
roll back acknowledged global objects/history. Cross-process disconnect/close
fences and restart receipt recovery remain integration work.

Binding (`core/crates/layerfs-api/sdk/src/runtime/binding.rs` at local Git `52e1f2e18`) ties an application-
authenticated peer, exact Workspace incarnation, Branch snapshot, catalog and
catalog/runtime incarnations. The application assigns a fresh nonzero runtime
incarnation and must never reuse it. Bind authorizes the peer/Workspace/Branch,
reads one coherent history snapshot and one authenticated canonical filesystem
root, and verifies scope/profile. It does no whole-tree scan or import. Every
operation rechecks authority before provider service, including exact object IDs
and derived references. The following bridge checkpoint requires a `VerifiedPeer`
from a completed native KK handshake at SDK bind; raw bytes cannot construct it.
See [native channels](23-native-bridge.md). Wire capabilities, logical codecs and
complete framed service remain unfinished; direct SDK proofs do not qualify them.

Sessions (`core/crates/layerfs-api/sdk/src/runtime/sessions.rs` at local Git `52e1f2e18`) indexes a fixed
configured set of live/retained processing slots, each with a monotonically burned
serial. Capabilities are private typed values, checked against the runtime and
the complete binding. Slot reuse and a new serving scope cannot reuse an exposed
serial. No total Save/file/flow counter or duration cap is imposed. An unavailable
slot returns explicit pre-attempt admission, never a replay loop.

Accept calls public canonical semantic admission under the selected Store policy
and scope, then authorizes the object plus derived refs and calls the owning Save
once. Local semantic refusal ends that producer with an Accept-phase receipt;
storage refusal preserves its original error in shared ownership. Saved and same-
Save demand use the storage `read_objects` methods directly so a typed storage
failure is not collapsed into a generic content-provider refusal. Demand windows
retain cluster one's 4,096-ID/32-MiB bounds; they do not cap the underlying file.
Objects are delivered in demand order through a borrowed sink after provider locks
are released. The transport must reserve its own delivery credits before copying
and avoid blocking network I/O on a shared service owner. Transport queues/fair
dispatch, aggregate delivery/cache/Save residency and service latency remain open.

Finish consumes a producer once and stores a borrowed exact completion. Inspecting
it after a lost reply does not repeat finish. A live or retained completion occupies
its slot until explicit release, so success is not inferred from reply delivery.
Explicit abort drops the producer without deleting acknowledged waves. Release
accepts only known terminal disposition: UnknownOutcome or CleanupFailed retains
the slot; no guessed resend/delete or uncertain resolver is supplied. History
stage/publish uncertainty remains a distinct unimplemented adapter boundary.

Work is O(S) for finding a free slot among configured concurrent S, O(1) capability
indexing, and actual bounded owning-library work for each object/demand/finish.
Bindings and slots contain no namespace mirror, whole-file buffer or Save-sized
ID collection. Canonical refs and demand results occupy per-object/per-window
temporary state. These source bounds are not a whole-system residency claim.
Original Store profiles/SQL/pack synchronization guarantees remain unchanged;
daemon disposable overlay is a separate owner and receives no sync call.

Public macOS tests use the real shared provider/catalog and this SDK API: two
interleaved producers, same-Save demand, independent saved demand, retained finish,
one-attempt refusal, explicit abort/release, ABA-safe slot/serving-scope reuse,
peer/Workspace isolation, permission revocation, object denial and demand-window
checks. Exact semantic and missing-dependency failures retain their original phase.
Uncertain/failed-cleanup custody has source handling but still needs actual owning-
platform integration proof; no failure injection or small fixture completes S9.

The old SDK is preserved byte-for-byte under `layerfs-sdk-legacy`, excluded, with
only the declared manifest package identity/relative dependency paths changed.
Its product LOC remain counted. S11 must retire this source once replacement
Init/control/transport has coverage; root reference remains until S13.

The following P5 slice adds authorized saved-file lengths through a borrowed
metadata sink. See [file-length facts](24-file-lengths.md) for the exact descriptor
trust and owning DB/canonical work, and remaining base/stat integration.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.

## S4 owning serial allocation

The checkpoint after `8d691ab8a` adds `Sessions::reserve_serials` and the
borrowed `serial_port`: one authorized, binding-checked reservation from the
Branch's allocation scope through the owning history catalog, at most 65,536
serials per call. Workspace consumes the range locally. A failed or unknown
call is never repeated for the same range; a consumed range is simply unused.
Logical transport, fair network service and disconnect fences remain S9.

S9 adds [bound history handlers/receipts](37-runtime-history-receipts.md) and
[bounded fair typed dispatch](38-authenticated-runtime-service.md). These current
local mechanisms preserve the initial public APIs and exact authority/custody;
logical transport, actual network/restart fences and full acceptance remain open.
Earlier limitations above retain their original checkpoint identity.

R1 composes these initialized owners through [host runtime supervision and consumer
attachment](45-runtime-supervision.md). Its host-thread Supervisor borrows Sessions,
dispatches bounded Service jobs and independently fences real socket workers. Provider
locks never span native I/O, original delivery/partial/result credits stay owned, and
Saves outlive connection detach. R2 contextual acceptance, R3 restart custody,
R4 integrated consumers and complete S7/S9 qualification remain open.

## R2 root-binding context

The S7–S13 continuation after `da331dfb607f5939b3e150c82480faabf6fb258c`
extends `Sessions::bind` (`core/crates/layerfs-api/sdk/src/runtime/sessions.rs` at local Git `52e1f2e18`)
through authorized root demands (`core/crates/layerfs-api/sdk/src/runtime/root_binding.rs` at local Git `52e1f2e18`).
The coherent Branch snapshot still fixes the expected root, scope and profile.
Binding now reads the actual root inode along its indexed canonical table path,
checks the root-directory/zero-reference invariant, decodes its directory root
page and reads the required portable mode/mtime using Content's public readers.
Every demanded identity is authorized before provider acquisition; the first
original authority or Storage error survives the narrower Content provider port.
No failure is retried and no binding is returned from partial validation.

Work follows one inode path, one directory page, two metadata paths and their
bounded 4/12-byte attribute-value mappings. It is independent of untouched
namespace population except for canonical tree depth. One initialized Reader
owns the operation's existing bounded caches. This adds real demand/read work
to binding; it is neither zero-I/O readiness nor a whole-tree closure proof.

The external `runtime_binding` target covers both Store profiles, invalid/missing
root-inode context, wrong content/metadata roles, scope disagreement, malformed
or absent portable fields and exact descendant authorization refusal. Full
contextual Save child meanings, saved closure and incremental alias/cycle evidence
remain R2/K2 work. This source change does not complete S9 or qualify native mount.

## Direct child-context admission continuation

The continuation after `a41d131f262c926015798d05096ebeaa01c70cdc` adds
[`FinalizedObject::validate_context`](../../crates/layerfs-content/src/object/output.rs)
before the original Save accepts an SDK object. Local admission re-derives exact
references; fresh authority checks cover every further child demand through the
authorized object adapter (`core/crates/layerfs-api/sdk/src/runtime/authorized_objects.rs` at local Git `52e1f2e18`).
The adapter calls that same Save's read method, so pending children are visible
without creating another provider or reopening a Store. Each Content demand uses
a one-ID window, with actual chunk slice, child fill/level/summary, inode placement,
kind-specific content and portable metadata checks. See [object context limits](02-objects.md).

The first authority refusal is returned exactly and does not end the producer.
An actual same-Save Storage read failure is terminal under its owning contract;
the returned error and retained Accept completion share the original Storage Arc.
A Content context refusal records its original Accept phase and prevents later
Accept/Finish. No prohibited parent is accepted after either terminal failure.
There is no retry, graph-sized buffer or namespace mirror. Validation pays real
canonical reads, copies and existing Storage cache work; bounded windows alone
are no speed or aggregate residency qualification.

Saved stage candidates now use the same bounded root validation and must retain
the captured root inode serial before history effects. Local serving-scope
completion returns original slot custody through [the explicit fence](46-runtime-custody.md).
Full saved closure/topology, incremental reverse bindings and provenance remain
R2/K2 obligations; process-crash receipt recovery remains R3/application work.
