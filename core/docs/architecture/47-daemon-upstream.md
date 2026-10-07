# Provisioned Daemon upstream and operation-owned consumers

> **Status:** Current general guide. R4 consumer composition checkpoint; native mount and full S9 qualification remain separate exits.

The active Daemon [upstream owner](../../crates/layerfs-daemon/src/upstream/owner.rs)
composes the existing SDK Attachment/Calls and public Workspace/Content ports with
an already initialized OwnerClient. It opens no global Store, performs no initial full-root acquisition,
starts no host service and depends on no excluded predecessor or Server. Host callers
retain the initialized Runtime/Supervisor and provide authenticated Connections and
exact trusted assignment context. Local overlay initialization stays in Owner, once
per daemon. This is a library composition boundary, not native mount readiness.

## Exact assignment and original attachment

[ExpectedBinding](../../crates/layerfs-daemon/src/upstream/types.rs) carries the
oriented host-responder and local-initiator public keys, runtime/catalog/provider
incarnations, Workspace, complete coherent BranchSnapshot, root serial, exact
StoragePolicy and required SqlitePersistenceProfile. The complete snapshot comparison
includes Branch ID, stack, name, base Layer, optional head Commit/root, base/effective
root, scope and filesystem-profile identity. Matching one root hash does not excuse
a different expected history context.

PersistenceBootstrap is a separate trusted fact for that exact host/runtime/catalog/
provider context. The host obtains its profile from `Handles::profile().persistence`
before Runtime consumes Handles. Daemon uses the owning public
`layerfs_persistence::SqlitePersistenceProfile` type, preserving Durable/Disposable
semantics without defining a second contract. `snapshot.profile` is the filesystem
profile ObjectId and never supplies a durability/profile observation. This public
type edge introduces no Daemon provider opening or global SQL implementation.

`Upstream::attach` receives the original SDK Attachment, ExpectedBinding,
PersistenceBootstrap, OwnerClient and cache-byte allowance. It checks provisioned
nonzero/coherent context, actual authenticated responder, bootstrap authority context
and actual-versus-required persistence profile before any wire request. It requests
the original Binding once and compares the complete returned authority/snapshot/root
serial context. It then requests Policy once and compares its exact owning fields.
Neither operation refreshes a Branch, reconnects or retries a failed request.

Only after those replies pass does a fresh bootstrap RemoteObjects operation and
shared-cache CanonicalClient demand the actual BaseView root. Scope, supported root
format and actual root serial are checked before any local namespace Open. The one
existing `Command::Open` uses the exact Workspace bytes as its local incarnation and
the selected effective root. Workspace is bound only from known `Response::Opened`.
All provider/network work occurs outside that overlay-owner job.

AttachRefusal returns the original Attachment for explicit fence/join, original
ExpectedBinding/Bootstrap, stopping phase and exact cause. Received Binding/Policy
Messages retain their original credits. Original CallFailure retains its nested
received/partial/error custody. A bootstrap demand refusal retains its RemoteObjects
owner and original PortFailure/result. Unadmitted Open returns its original command
and cause as OwnerError::Unattempted. An attempted Open refusal returns the original
boxed Completion and credit. Lost Open delivery remains uncertain; no namespace
absence, replacement Open, guessed Close or rollback is inferred.

## One authority cache, fresh failure ownership

Each Upstream owns one immutable cache restricted to its provisioned host/peer,
runtime/catalog/provider, Workspace and original captured context. Independent
attachments have independent caches. Cache identity alone is not current authorization
or revocation evidence; this checkpoint supplies no dynamic revocation protocol and
permits no cross-peer/Workspace cache sharing. The original checked BaseView retains
its successful bootstrap client solely as its base binding. Upstream hides unscoped
Workspace reads, so that bootstrap provider does not serve ordinary operations.

[UpstreamOperation](../../crates/layerfs-daemon/src/upstream/operation.rs) creates
a fresh Arc<RemoteObjects>, lengths/serial ports and CanonicalClient using that cache,
then obtains `Workspace::scoped(client)`. It shares the current original checked base,
paired installs and reserved serial ranges; it does not copy a mutable filesystem
or give every operation another authoritative base. The [Workspace scope owner](48-shared-cache-operation-scopes.md)
keeps successful install metadata shared while preserving the original binding
provider, rather than promoting an operation's failed provider.

An operation exposes its scoped Workspace, existing OwnerClient OverlayRead/
OverlayJobs/OverlayFileRead ports, canonical client, saved lengths and serial allocator.
`run` performs caller-selected public operations and returns OperationSuccess or
OperationFailure, each retaining the operation owner with the original value/error.
Original ContentError/WorkspaceError is not replaced by a display diagnostic. The
operation's `failure()` keeps the original PortFailure and credited result accessible.
An initial scope-construction refusal similarly returns its already-created provider/
client and exact Workspace error.

A failed RemoteObjects remains terminal within that operation. A subsequent uncached
demand is refused without another Calls exchange, preserving the first failure.
Another independently created operation can succeed on healthy Calls and reuse
successful immutable cache. A stopped/fenced Calls owner remains terminal; a fresh
operation does not reconnect or revive it. Same-Save pending objects are a separate
Commit visibility capability and are not inserted into this ordinary saved-base
cache. No factory per filesystem algorithm or daemon-wide failed-demand registry is
introduced.

## Lifecycle and costs

`Upstream::fence`/`try_join` delegate to the existing independent consumer socket fence.
They do not abort a Save, terminate Bash, unmount or close the local Workspace. The
caller separately owns exact local source/descriptor/capture/reply lifetimes and
explicit terminal Close. OwnerClient's existing automatic bounded reclamation remains
the local deletion owner. [R3 scope custody](46-runtime-custody.md) remains the host
serving-scope boundary, including retained known stages and terminal unknown outcomes.

For one attachment, identity/snapshot/policy comparison is bounded metadata work,
followed by the actual bounded Binding/Policy/root network demands and one local Open.
There is no namespace scan/copy/materialization, provider initialization or per-tool-
call database creation. Every operation allocates one bounded provider/client/view/
port owner and shares the original cache/base/serial ownership. An occupied cache may
evict or bypass a large immutable object; it does not cap file or total Workspace size.

Across M operations and B demanded canonical bytes, object/cache/hash/copy/framing work
follows the existing public demand and cache algorithms and actual misses, including
errors and held original results. Source/overlay reads use existing short owner jobs;
network waits never occupy the SQLite owner. RemoteObjects' first-failure mutex and
Calls' one-exchange mutex are scoped to their actual operation/connection, not a whole
Save/Commit or every attachment. Cache diagnostics expose aggregate logical charge
and hit/miss/acquisition counters. These are not allocator/kernel/socket/pager/cache
residency, physical I/O, rate, queue or latency qualification.

## Evidence boundary

The portable [upstream tests](../../crates/layerfs-daemon/tests/upstream.rs) use real
KK/native framing, SDK Attachment/Calls and initialized overlay Owner with explicitly
scripted external protocol fixtures. They cover oriented responder/profile mismatch
before wire/Open, same-root different history context, original Policy/Binding
Messages, failed operation terminality with no second exchange, a distinct healthy
operation, boxed attempted-Open completion credit and uncached reads after socket
fencing. Their fixture intentionally provides no global Store/root namespace and
establishes no real-host readiness or full-root qualification.

The [real macOS Store/Project/Runtime/Supervisor to Linux Docker proof](../issues/307/R4-UPSTREAM-CACHE-20261007.md)
passes through public APIs under both actual persistence profiles, with native
source removal before consumer access. Its independent oracle covers all thirteen
original paths and seven file names in the small fixture. It verifies original
operation refusal without replay, local mutations, live MaintenanceIdle, logical
terminal CleanupState::Gone and zero consumer receive credit at its fence. Host
workers join and Sessions fence succeeds; the proof does not inspect every host
AttachmentFence field or establish zero host-domain credit. There is no native
mount, physical deletion/shrink, resident bound or speed qualification. API-core/Sandbox activation, native FUSE, ordinary Exec, real Commit,
huge/dense>4GiB acquisition, E/Q numerical/resource acceptance and S10–S13 remain
separate work. Existing Init gate failures and history comparison verdicts remain
unchanged.

## Original successful attachment receipts

`Upstream::attach_with_receipts` performs the same single attempt and returns
`AttachSuccess`: the authority-bound Upstream, original known local Open
Completion and original validated Binding/Policy Messages. Their real owner and
receive credits remain charged until the caller releases those values. Socket
fencing does not consume caller-held successful replies. The older `attach`
delegates to this path and releases the known receipts as it did before; the
shared refusal path retains original phase/cause/messages/provider and any
attempted Open Completion. There is no second Open, refresh or inferred close.

The new public case proves one outstanding Open result and two held transport
Messages, including after the actual consumer fence, then zero credits after
their explicit release. The same four-case scope passes on host and Linux after
matching locked builds in the
[E04 prerequisite receipts](../issues/307/checks/e04-writes-20261007).
This enables actual bootstrap result accounting in the prospective original-job
collector. It supplies no private base-fact trace, per-job statement-family
inventory, whole-operation copy/physical-I/O/residency or milestone qualification.
