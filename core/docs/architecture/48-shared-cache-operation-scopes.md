# Shared canonical cache and operation-scoped Workspace providers

> **Status:** Implemented additive R4 library boundary; scoped checks pending
> integration-owner execution. No mounted/runtime/resource qualification is
> claimed. Prepared 2026-10-07 after checkpoint `86f766750`.

This boundary reuses the existing immutable cache, checked BaseView, source/fact
planning and paired install. It changes ownership so a daemon consumer can create
a fresh provider for each operation while retaining one immutable byte allowance
and the original Workspace's installed root and reserved serial ranges. It opens
no Store, overlay database, namespace or additional mutable tree.

## Public surface and ownership

The additive APIs are exported by [Workspace](../../crates/layerfs-workspace/src/lib.rs):

```text
CanonicalCache::new(bytes: usize) -> CanonicalCache
CanonicalCache::diagnostics(&self) -> ContentResult<ClientWork>
CanonicalClient::with_cache(
    source: Arc<dyn AuthenticatedObjects + Send + Sync>,
    lengths: Option<Arc<dyn FileLengths + Send + Sync>>,
    cache: Arc<CanonicalCache>,
) -> CanonicalClient
BaseView::with_client(&self, client: Arc<CanonicalClient>) -> BaseView
Workspace::scoped(&self, client: Arc<CanonicalClient>) -> WorkspaceResult<Workspace>
```

Existing `CanonicalClient::new` and `with_lengths` retain their signatures and
behavior: each creates a new independent cache owner at its requested allowance.
The new `with_cache` retains an existing owner. ClientWork's public layout and
counter meanings are unchanged. Diagnostics on clients sharing that owner report
the same cumulative owner observations and current charge; simultaneous clients'
work is not automatically attributable to one operation through snapshot deltas.

[CanonicalCache](../../crates/layerfs-workspace/src/base/cache.rs) owns the existing
bounded immutable-object map/recency state and counters behind one mutex. Each
[CanonicalClient](../../crates/layerfs-workspace/src/base/client.rs) retains its own
source and optional length port plus an Arc to that owner. Cache lookup/recency
and hit/insert byte copies occur under its mutex; the mutex is released before
provider I/O, authentication and canonical traversal. Failed upstream
demand or failed authentication/window validation inserts no newly fetched values.
Earlier authenticated entries and original attempted counters remain; no retry,
provider substitution or guessed deletion occurs.

Provider errors and partial/native result custody remain with the supplied owning
port and caller. They are not shared through the cache. A fresh RemoteObjects port
can therefore retain its operation's first failure without poisoning the cache or
another fresh operation's provider. This does not reconnect or repair terminal
Calls/transport owners, resolve unknown outcomes, or reset a failed provider.
The library returns original Content/Workspace errors through the existing path.

Cache hits acquire no new authority. The application/daemon owner must authorize
each operation and restrict all sharing clients to its declared authorization
context, including the required peer/Workspace/Store/profile/scope/Save bindings.
The cache has no dynamic revocation or authority epoch mechanism. A new client or
cached object identity cannot grant another context access; owner changes require
their explicit authorization/custody policy, not an implicit cached success.

## Checked binding and original Workspace state

[BaseView::with_client](../../crates/layerfs-workspace/src/base/view.rs) copies only
the already checked filesystem-root identity, profile/scope/root metadata and the
new client Arc. It performs zero provider calls, scans, payload copies or namespace
materialization. Later reads still use the actual public content APIs, including
the operation's own optional length port and existing MissingLengthProvider
refusal. This metadata reuse is not an authority or saved-root proof.

[Workspace](../../crates/layerfs-workspace/src/workspace/state.rs) now retains one
Arc<RwLock<BaseView>> and one Arc<Mutex<Serials>> for its original binding and serial
state. `scoped` clones those owners, preserves the exact route and adds only an
operation client override. It checks the existing poisoned-binding refusal without
I/O. `base()` clones the original cell's current checked metadata under a short
read lock, releases the lock, and applies the override to that returned clone.
It does not freeze an old selected root into every future scoped operation.

Existing `view_for_source` still checks the exact owner-issued route/root against
that current binding. SourceView, BaseFacts, reads and namespace jobs follow their
ordinary public paths; there is no scope-specific fake or alternate evaluator.
Existing plans retain their own immutable root/client. A later install changes
future base selection, while prior plans and owned source lifetimes keep their
original obligations and errors.

`next_serial` retains its existing external allocator port and bounded refill
algorithm. All scoped views consume the original reserved remainders through the
same mutex. They never clone, reset or recycle those ranges. Allocator I/O remains
outside Workspace locks; concurrent refill calls still require the owning scope's
unique authoritative ranges. This change makes no new reservation or crash-recovery
guarantee and adds no total inode/operation cap.

[Prepared install](../../crates/layerfs-workspace/src/workspace/install.rs) remains
one exact known-publication caller attempt. Preparation can demand the new root
through the operation's provider outside the binding lock. On install, the shared
original cell checks route/expected root and performs the existing overlay
transition. After known SQL success it publishes the prepared checked metadata
with the **original binding's provider owner**. An operation's failure-bearing
provider is not promoted into future original Workspace reads. No fallible content
or transport work follows SQL success; refusal returns original error/prepared
custody. Global Save/history knowledge and later active mutation preservation
remain governed by the unchanged owning contracts.

## Cost, copies and limits

R7 update, 2026-10-09: the cache owner also holds the bounded table of derived
child-directory counts, `DIRECTORY_COUNT_CAPACITY` entries keyed by directory
content root, outside the byte allowance described below and under the same
lock; see [effective view](29-effective-base-view.md). `ClientWork` gains
`directory_counts` and `directory_count_scans`.

For E admitted immutable entries and B demanded bytes, cache lookup/recency work
remains O(log E) per ID plus actual bytes and eviction work. A hit clones that
entry's canonical Vec into the caller result. Each successfully admitted distinct
miss clones its canonical bytes once into the retained cache; duplicate/already
retained IDs and oversized bypasses add no cache insertion copy. Returned fetch
vectors move through validation/result assembly. The upstream/native port's own
delivery copies are separate and remain its responsibility.

The one logical cache allowance charges each entry's canonical byte length plus
256 bytes of bookkeeping. Cache hit result copies, newly fetched windows, caller
held output, decoded FileViews, provider/native buffers and allocator overhead are
outside that charge. A mixed hit/miss request may hold both cached result copies
and the upstream fetched window before its final combined-window check. Existing
limits remain 4,096 demand IDs, 16 MiB per canonical value, and 32 MiB per logical
batch; upstream pre-return allocation/admission remains its owning contract.
Neither the allowance nor ClientWork proves total allocation, RSS, cache residency
or service rate. Exact get/insert copy counters remain E2 work; no fictitious zero
or new lifetime-as-phase number is introduced here.

Scoping, checked-client rebinding and current-base selection use fixed metadata
copies/Arc operations, independent of namespace/file size. Shared install and
serial ownership add no SQL and keep the existing operation/cumulative bounds.
Known SQL transition/allocation work remains real work under its original profile.
Source/source-view lifetimes and automatic local reclamation do not become global
content/history GC or a distributed retention lease.

## External verification and remaining acceptance

[operation_scope.rs](../../crates/layerfs-workspace/tests/operation_scope.rs) uses
public content constructors/readers and the existing actual Overlay/Workspace
mutation, source and prepared-install path. It checks one allowance across fresh
clients, independent returned bytes, eviction and aggregate counters, isolated
provider failure with retained original refusal, no-I/O checked metadata reuse,
the scoped length port, shared serial consumption, original install propagation,
the original provider surviving a failed scoped preparer, and old read bytes.
All test fixtures/providers are external to production source.

The author ran no build, test or measurement. The integration owner builds locked
`--no-run` first and runs the scoped test under an explicit wall stop <=120 seconds,
then performs the affected warning-denying Clippy/fmt/boundary checks and independent
review. Compilation/test outcomes must be appended at their actual identities.
This additive boundary does not complete R4/S7/S8/S9/S10: real owning daemon/runtime
composition, authority/custody, native requests, full root proof, resource/service/
cache observations and the retained Init performance failures remain separate.
