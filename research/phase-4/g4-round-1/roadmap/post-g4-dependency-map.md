# Post-G4 dependency map

Status: **planning only**. This map re-evaluates downstream ownership; it does
not execute G4, start G5, authorize production integration, or close Phase 4.

## Dependency graph

```text
Round 1 research package
  -> separate G4 preregistration review
  -> G4 reconstruction + native-materialization acceptance
       |-- PASS with honest unavailable cells
       |     -> freeze two scoreboards and exact mechanism custody
       |     -> G5-A reopen authority
       |     -> G5-B count-changing locality/mapping
       |     -> G5-C concurrency/endurance/history
       |     -> G5-D residual create/SQLite profile work
       |     -> G6 evidence closure
       |           -> later VFS/projection/SDK/application integration
       |
       |-- REVISE
       |     -> repair only the failed G4 mechanism or method
       |     -> repeat no passing lane until final proof
       |
       `-- BLOCKED
             -> freeze exact blocker and honest Unavailable cells
             -> do not substitute G5 or integration work

New profile/format research (independent branch of evidence)
  -> shadow model + migration/downgrade/security proof
  -> one-variable profile experiment
  -> separate versioned promotion campaign
  -> only then can it replace a G4/G5 control
```

## G5-A — reopen authority

### What G4 can establish

- warm/fresh-process authenticated reconstruction;
- same-open protected seed full reads and clone/patch behavior;
- first/full and fallback native publication; and
- the exact cost that appears when same-open authority is absent.

### What remains

The retained G3 permit and unlinked seed descriptor are operation-local. They
do not survive process loss and do not resist a malicious peer with the same
UID. A persistent file, inode/timestamp tuple, watcher hint, or prior receipt
is not byte authority.

G5-A must decide one of three explicit outcomes:

1. retain complete reauthentication after reopen;
2. introduce a bounded trusted broker/service that keeps descriptors and
   authority across client processes, with restart rebuilding and a stated
   same-UID threat model; or
3. define a new persistent authenticated native representation/profile with
   corruption, rollback, replay, migration, and downgrade proofs.

It must not hide seed creation or full reauthentication in setup. Cross-process
authority is a security/architecture decision, not a G4 latency repair.

## G5-B — count-changing locality and mapping

Canonical-v2 compacted each file occurrence from 68 to 36 bytes but retained
the current fixed-radix topology and its worst-case suffix work. G3 correctly
uses complete fallback when length/reference counts violate the qualified
same-size relation.

After G4 freezes the fallback cost, G5-B may test a different mapping only if
the proposal predicts a material win for measured early/middle count changes
and protects:

- 308.884052-ms full create;
- approximately 5–7-ms same-open edits;
- 2.279209-ms returned 1-MiB range;
- 2.088334-ms reopen/head;
- G4 reconstruction and first/full native materialization; and
- exact identities, ordered topology, typed errors, bounded Q, and one
  transaction/COMMIT.

Any prolly tree, history-independent tree, larger extent, or alternative CDC
profile is a new versioned profile. It needs shadow equivalence, migration,
downgrade rejection, and storage/history accounting before performance work.

## G5-C — concurrency, endurance, and history

The current production-shaped engine serializes one rusqlite `Connection`
behind a mutex; the accepted benchmark Store also uses one connection and one
transaction/COMMIT. G3 is a same-open, single-operation mechanism. None of
those facts establish multi-reader/writer behavior or bounded long-run cache,
seed, journal, history, and garbage-collection behavior.

G5-C owns:

- reader/writer and materializer/materializer conflicts;
- cancellation and deterministic first-error behavior;
- lost acknowledgement and reconciliation under concurrent namespace change;
- 10/100/1,000-revision logical/apparent/allocated growth;
- optional seed/native-cache capacity, LRU or equivalent eviction, pinned-item
  limits, corruption recovery, and rebuild;
- orphan/temp cleanup after crash and restart; and
- garbage collection reachability, race safety, and rollback.

A bounded native acceleration cache must be content-addressed, optional, and
capacity-accounted. It may hold selected roots; it may not become an implicit
full native duplicate for every revision.

## G5-D — residual create and SQLite work

G1 retained `cache_spill=2000`; full create is 308.884052 ms with 12.48 MiB
maximum RSS and an 8.35 MiB SQLite cache snapshot. G4 must not reopen page size,
journal, mmap, carrier, compression, worker, or payload layout while qualifying
materialization.

After G4, residual work may be reconsidered one variable at a time:

- page size/cache policy with byte-fixed memory;
- shipping-engine redundant BLOB pass removal;
- bounded SQLite-resident authenticated extent BLOBs, preserving one SQLite
  durable authority, before any external payload layout;
- only after a decisive SQLite-extent shortfall, SQLite catalog plus immutable
  external value segments with a full two-file crash/GC proof;
- optional macrosegment/native cache;
- query/VFS attribution where direct support exists; and
- compression only for a workload whose measured ratio and CPU budget justify
  it.

Page-cache counters and allocated blocks remain logical/resource observations,
not physical I/O. A catalog/value-log split must solve atomic reachability,
crash ordering, GC, orphan cleanup, authority, and downgrade; prior carrier
failures cannot be waived by a faster microbenchmark.

## VFS, projection, and application integration

`layerfs-vfs` and `layerfs-sdk` currently expose only component constants;
`layerfs-os` is a host-observation helper. There is no current production VFS
read/materialize API, mount, file-provider bridge, or application lifecycle.

Integration follows, rather than participates in, Phase-4 core acceptance. It
must specify:

- streaming/range API and backpressure;
- no-output-before-authentication semantics;
- platform-neutral first/full fallback;
- APFS descriptor-relative clone acceleration as an optional specialization;
- Windows/Linux clone/copy and durability fallbacks;
- cancellation, partial reads, error mapping, and resource limits;
- cache-service trust and process/UID/sandbox boundaries; and
- native-name, metadata, symlink, path traversal, atomic publication, and
  reconciliation semantics.

Direct VFS consumption of authenticated extents is promising only after the
same engine API also proves logical reconstruction, ranges, and native output.
It must not fork a weaker parallel authority path.

## G6 — final Phase-4 closure

G6 may start only after every G4 and G5 lane is accepted, explicitly deferred,
or recorded as an exact blocker. It introduces no new performance candidate.

G6 must freeze:

- final reconstruction and materialization scoreboards with honest cache-state
  and Unavailable cells;
- create/edit/range/reopen/concurrency/history/resource/storage guards;
- source, executable, profile, fixture, raw, analysis, cleanup, static closure,
  manifest, terminal, and independent-verification custody;
- production-integration status and limitations; and
- the exact WP5/integration handoff.

Phase 4 is not complete merely because G4 passes. Conversely, lack of a true
device-cold observation need not block closure if the host-buffer approximation
and its limitation are explicitly frozen and all required product semantics
are established.

## Decision table

| Downstream item | Earliest prerequisite | Phase owner | Round-1 disposition |
|---|---|---|---|
| honest reconstruction/native baselines | reviewed G4 preregistration | G4 | ready to preregister, not execute |
| same-open trusted seed acceptance | G4 exact authority/custody | G4 | ready to preregister |
| persistent/cross-process seed authority | G4 fallback and seed cost | G5-A or later integration | not a G4 repair |
| count-changing local mapping | G4 fallback baseline | G5-B | pending one-variable evidence |
| concurrency/endurance/history/GC | frozen G4 mechanism | G5-C | pending |
| page/storage/profile experiments | frozen G4 protected matrix | G5-D | pending |
| dual canonical/native representation | new-profile shadow proof and G4 costs | later architecture milestone | research candidate only |
| VFS/projection/SDK/application | Phase-4 core decision or explicit integration gate | post-Phase-4 | absent today |
| final evidence/limitations freeze | all G4/G5 dispositions | G6 | pending |
