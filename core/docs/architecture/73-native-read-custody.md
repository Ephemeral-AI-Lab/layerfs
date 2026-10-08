# Native read observation and lookup custody

> **Status:** Implemented R2 component on parent78374ced6,2026-10-08.
> Native Fuse activation, handles/directories, Ready and normal drain remain open.

Overlay schema17 adds one engine-minted NativeMount per Workspace and indexed
native_lookup, native_source and native_read associations in the existing Overlay.
The mount retains its authenticated root serial. Its implicit root lookup owner
is separate from explicit kernel nlookup; count-zero FORGET does not discard it.
Repeated positive lookups increment one checked count for the same inode and keep
one existing LookupOwner lease. Hard-link aliases therefore share inode custody.
There is no resident namespace/inode map or total lookup limit.

[Native sources](../../crates/layerfs-overlay/src/lifetime/native.rs) bind the
full engine/route/mount/request identity. Request keys preserve all64bits in an
eight-byte blob. Nonrecycled engine IDs occupy BaseSource kind2, distinct from
caller sources(kind0) and independent FileRead sources(kind1). Acquisition also
retains an independent FileReader lease on the lookup parent or getattr target.
That lease protects current metadata while immutable facts are demanded, even
when FORGET and unlink happen meanwhile. Exact source release removes both the
association and its lease. Copying a capability creates no ownership.

[Workspace NativeReadPlan](../../crates/layerfs-workspace/src/operations/native_read.rs)
uses the existing Eval/BaseFacts/Need semantics. Owner rounds perform bounded
current-row decisions and request missing immutable facts; Base rounds run those
demands outside SQL. Final positive lookup selects its inode, increments nlookup
and retains an independent FileRead in one atomic transaction. Getattr uses the
same observation without adding a kernel lookup. The existing source protects
removed metadata, so a previously received getattr can report nlink0. A final
negative/refusal is marked decided and acquires no FileRead. An original request
cannot perform a second final decision.

[NativeObservation](../../crates/layerfs-overlay/src/lifetime/native_observation.rs)
retains the original semantic decision separately from transaction completion.
A minted candidate is preserved even if association insertion or completion
fails. Only resultOk proves usable acquisition. A candidate from a failed or
uncertain attempt is evidence, never permission to adopt, release, replay or
send success. Retained-source/read point APIs observe original custody after
loss without resolving that uncertainty. Internal FileRead requests use negative
engine-owner keys; existing public positive request keys remain disjoint.

[NativeJob](../../crates/layerfs-daemon/src/overlay/native_job.rs) carries these
operations on the existing fair SQL owner. Observe returns the complete original
outcome in an Arc; input/facts/output capacity is charged with the original
completion. An asynchronous consumer must retain that Daemon Completion alongside
any projected Arc until output/consumer disposal. Owner execution performs no
provider I/O. NativeReadPlan::supply requires an already admitted immutable reader;
this component does not wire a native dispatcher or move blocking Store admission
onto its future worker pool.

After acknowledged detach and complete consumer drain, revoke_native_mount
refuses any remaining native source/read association before effect, then revokes
admission and schedules indexed retirement. Automatic maintenance removes at most
64lookup groups per turn, including the root group, using an ordered serial
cursor. Existing orphan and closed-Workspace cleanup follows exact last release.
The mount row itself fences whole-namespace cleanup until retirement ends. Open
file/directory owners and kernel detach/drain still need integration and cannot
be inferred from this source/read fence. Reply-send knowledge remains the
unavailable outcome selected in S8; no compensating FORGET is invented.

The [component receipts](../issues/307/checks/r2-native-lookup-20261008/35-results.md)
cover exact repeated counts, foreign/stale identity, independent held sources,
lookup races with unlink, hard-link identity, full-width request keys, rollback
decision retention, automatic bounded retirement and the real Daemon/Store route.
Actual indexed plans are paired with DatabaseWork; they do not establish native
kernel behavior, latency, cold cache eligibility, RSS bounds or full R2 acceptance.
