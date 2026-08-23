# G5 history-scaling and verification contract

Status: **prospective invariant set reconciled to the frozen final G4 baseline;
the first H11 screen is REVISE on whole-harness Q**.

## Correctness and identity

For every successful revision `r`:

```text
reconstruct(root[r]) = expected_bytes[r]
digest(reconstruct(root[r])) = expected_digest[r]
current_head = the exact acknowledged revision
```

Required properties:

- immutable canonical objects never change after publication;
- a new revision cannot mutate a retained old revision;
- reverting to identical logical content reuses the promised content identity;
- duplicate content reuses eligible canonical objects;
- retained roots remain reconstructable before and after GC;
- branch histories share unchanged objects safely;
- identity-before-grammar and all typed errors remain exact;
- each protected write uses the required transaction and COMMIT count; and
- failure leaves the prior state, the exact requested state, or an explicit
  unresolved outcome—never a hybrid.

## Latency versus history length

Record checkpoint distributions and slopes for:

```text
T_edit(N)
T_head_lookup(N)
T_reopen(N)
T_range(N)
T_reconstruct(N)
T_materialize(N)
T_gc(live, unreachable)
```

Desired complexity:

| Operation | Required dependence |
|---|---|
| Current-head lookup | `O(1)` or logarithmic in retained namespace metadata |
| Same-size edit | Changed region/mapping, not revision count |
| Current-root range | Selected mapping/chunks, not obsolete history |
| Current-root reconstruction | Current reachable graph and logical size, not root age |
| Current-root materialization | Current reachable graph/logical size or qualified seed state, not root age |
| Historical-root read | Selected root graph, not its ordinal age |
| Count-changing edit candidate | Changed CDC region + localized mapping changes + tree height |
| GC | Explicit live + unreachable reachability work |

The preregistration must freeze an acceptable history-growth estimator. At
minimum report:

```text
latency_growth_ratio
  = p50(last checkpoint interval) / p50(first stable interval)

latency_tail_growth
  = p95(last checkpoint interval) / p95(first stable interval)
```

No threshold may be replaced after observing the result. An absolute target
does not erase a failed growth slope, and a noisy single micro-operation may
not support a relative claim.

## Storage and reuse

For each checkpoint, retain:

```text
canonical_new_bytes
mapping_new_or_rewritten_bytes
objects_new / reused / live / unreachable
database logical / apparent / allocated bytes
authority and metadata bytes
journal/temp high-water where supported
native seed/cache/destination allocated bytes
```

Required relationships:

```text
canonical_growth
  ~= unique changed canonical bytes
     + changed mapping/proof metadata

canonical_growth != revisions * complete_file_size

native_acceleration_allocated <= explicit capacity K
native_acceleration_allocated != revisions * complete_file_size
```

Alternating A/B and revert workloads must distinguish content reuse from
revision-shaped duplication. Logical/apparent/allocated observations remain
separate and are never relabeled physical I/O.

## CPU, memory, Q, descriptors, and queues

Report per-operation and aggregate:

- user/system CPU and context switches;
- RSS high-water and post-warmup drift;
- exact Q high-water and terminal value;
- every owned buffer/queue capacity and maximum simultaneous overlap;
- SQLite connections/statements, file descriptors, seed descriptors, locks,
  and temporary files; and
- foreground versus maintenance/rebuild resources.

Required form:

```text
aggregate_resource
  <= bounded_shared_state
     + admitted_operations * frozen_per_operation_bound
```

Hard semantic gates:

- terminal Q equals zero after every operation;
- descriptor, permit, lock, seed, temp, and journal residue returns to the
  frozen steady-state bound;
- no full-file application buffer;
- no unbounded queue, decoded history, or per-revision native duplicate; and
- cache fill, eviction, revalidation, repair, and rebuild are never hidden as
  free background work.

## Reopen and authority

Verify after 1/10/100/1,000 revisions:

- head discovery;
- first range and full read;
- first same-size and count-changing edit;
- first/full materialization;
- same-open seed invalidation after all trusted holders exit;
- substitution, rollback, replay, and writable-descriptor mutation; and
- broker/service restart behavior if such a candidate is introduced.

Path, inode, mode, length, timestamps, watcher state, receipt, root ID, and
clone lineage are not reopened-byte authority. A persistent acceleration
candidate must name its protection domain and restart/rebuild rule.

## Durability, cancellation, and recovery

Fault or cancel at:

- canonical traversal and object authentication;
- transaction/COMMIT dispatch, acknowledgement, and reconciliation;
- native write, data sync, metadata sync, rename, directory sync,
  reconciliation, verification, and cleanup;
- cache admission/eviction/rebuild; and
- reachability mark, sweep, and reclamation.

Verify exact first-error precedence, old-or-new publication, fresh
reconciliation, substitution-safe cleanup, resumable maintenance, and zero
unsafe residue. Wall time, allocation, Q, RSS, or a prior receipt never proves
durability or authority.

## Concurrency and GC

Focused small-fixture cases must cover:

- reader/reader;
- reader/writer;
- reader/materializer;
- writer/writer;
- materializer/materializer on same and different destinations;
- cache eviction with a pinned lease;
- GC with pinned historical readers;
- GC racing new publication; and
- cancellation during each operation family.

The contract must name linearization or serialization points and bound
aggregate concurrency. GC must preserve every reachable/pinned object, reclaim
only independently proved unreachable objects, survive interruption, and
reconstruct retained roots identically afterward.

## Cold and unsupported observations

Fresh process and reopen are `warm-or-unknown`, not cold. A controlled
host-buffer-cold approximation requires prospective exclusive-host custody,
closed operands, a successful cache-purge procedure, and an immediate
no-warmup row. Device/controller cache and physical stable-media byte counts
remain `Unavailable` unless a direct supported source exists.

## Compatibility if a profile changes

Any new mapping/canonical/physical profile additionally requires:

- old-profile read and new-profile creation;
- explicit old-reader rejection of unsupported new state;
- migration and interrupted-migration recovery;
- downgrade rejection after new-state publication;
- mixed-generation reader/GC safety;
- migration storage high-water; and
- exact logical equivalence plus profile-specific identity custody.

Performance alone cannot promote a new profile.

## G5-1 terminal amendment

The first H11 attempts are complete at revisions `1 / 10 / 100 / 1,000`; see [H11 result](../concurrency-history/h11-result.md) and [resource/history model](../concurrency-history/resource-history-model.md). V2 supplies internally consistent current-root and append-only storage diagnostics for one deterministic 1-MiB control, but it does not prove the resource contract because whole-harness Q is incomplete. It does not close the full concurrency/GC contract.

The controlling G5 latency rule is dual materiality: exact two-sample relative failure plus a sum delta of at least `2,000,000 ns`. All identity, work, resource, storage, cleanup, chronology, and analyzer-agreement conditions remain hard. For first edit after reopen, genesis N=1 is a distinct transition mechanism; history scaling compares exact non-genesis work at N=10/100/1,000.
