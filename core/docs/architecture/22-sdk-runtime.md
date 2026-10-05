# SDK-owned scoped host runtime

> **Status:** Current general guide; implemented slice, not runtime qualification.

The #307 checkpoint after `042849109` embeds an initial runtime at the planned
[SDK composition path](../../crates/layerfs-api/sdk/src/runtime/mod.rs). The active
package is `layerfs-sdk`; its normal product dependency graph contains content,
storage, history, persistence and telemetry. It has no server dependency. Native
bridge transport, history adapters, five Workspace operations and full S9 exits
remain unfinished.

[Runtime](../../crates/layerfs-api/sdk/src/runtime/owner.rs) receives already-open
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

[Binding](../../crates/layerfs-api/sdk/src/runtime/binding.rs) ties an application-
authenticated peer, exact Workspace incarnation, Branch snapshot, catalog and
catalog/runtime incarnations. The application assigns a fresh nonzero runtime
incarnation and must never reuse it. Bind authorizes the peer/Workspace/Branch,
reads one coherent history snapshot and one authenticated canonical filesystem
root, and verifies scope/profile. It does no whole-tree scan or import. Every
operation rechecks authority before provider service, including exact object IDs
and derived references. This authorizes a transport-supplied peer identity; it
does not itself authenticate network bytes. No wire-capability constructor or
crypto/framing qualification is claimed from typed local Rust capabilities.

[Sessions](../../crates/layerfs-api/sdk/src/runtime/sessions.rs) indexes a fixed
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
