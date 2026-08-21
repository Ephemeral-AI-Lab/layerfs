# Local COW, mapping, and delta directions

Status: research only. This report makes no format, profile, schema, durability,
source, or implementation decision. Local code and sealed evidence are
controlling; external systems are precedent, not proof. Distributed machinery
is out of scope.

## Executive conclusion

The accepted radix/COW algorithm is already excellent for same-count local
edits and authenticated ranges. It is not the current full-create bottleneck.
The two real local COW defects are:

1. core tree mutation clones complete `BTreeMap`s along the ancestor spine and
   then rediscovers the already-known mutation through a second tree diff; and
2. fixed-ordinal durable leaves make early/middle count-changing edits rewrite
   a suffix, with `Theta(N)` worst case.

The first should be attacked with a small standard-library experiment. The
second may justify a bounded, history-independent prolly-tree format only if
count-changing workload evidence makes it important. Neither is a credible
primary lever for the remaining 100-MiB full-create gap.

## Observed current behavior

`TreeNode` wraps immutable node data in `Arc`, so unchanged descendants are
shared (`crates/layerfs-core/src/cow/tree.rs:31-50`, `:146-152`). However, one
add/remove/replace clones the complete containing `BTreeMap` before changing one
entry (`:155-203`), and recursion repeats that at every changed ancestor
(`crates/layerfs-core/src/cow/mutate.rs:141-236`). The direct test confirms that
unchanged siblings remain pointer-identical (`cow/mutate.rs:300-340`).

After applying an explicit `Mutation`, `apply_mutation` calls `Delta::between`
(`cow/mutate.rs:53-66`). Equal node identities prune unchanged subtrees, but a
changed directory still clones the union of old/new names into a `BTreeSet` and
probes both maps (`crates/layerfs-core/src/delta/mod.rs:109-180`). Delta order,
before-state checks, parent identity, and final child identity are semantic
(`delta/mod.rs:87-105`; `cow/mutate.rs:94-139`).

The accepted durable file map stores 68-byte references—raw `ChunkId`, raw
length, canonical `ObjectId`—in fixed K64 leaves, with cumulative-end/ObjectId
descriptors in F64 branches (`crates/layerfs-core/src/content/persistence.rs:20-21`,
`:38-92`, `:135-195`). Its builder retains one partial leaf and one branch per
active level and emits the smallest canonical height (accepted F2 source
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs:3702-3812`,
`:3869-4140`).

The accepted M4.5 changed-spine result reduced same-middle durable latency from
`440.023209 ms` to `9.134334 ms` (`-97.924124%`). A 100-MiB same-count edit
rewrites 7,098 mapping bytes in three objects rather than about 359 KiB of flat
mapping (`implementation-detail/phase-4/wp4m/progress.md:247-262`;
`implementation-detail/phase-4/algorithm/complexity-analysis.md:418-444`).

The known cliff is count change: fixed ordinal grouping can invalidate 365,211
mapping bytes/86 objects at 100 MiB, 1,876,516 bytes/433 objects at 512 MiB, and
373,777,332 bytes/85,889 objects in the retained-density 100-GiB projection
(`complexity-analysis.md:474-498`).

## Derived complexity and bottleneck

For source bytes `S`, references `N`, fixed leaf capacity `K`, fan-out `F`, and
height `H`, current full construction is:

```text
time       = Theta(S) CDC/hash/canonical work + Theta(N) mapping work
memory     = O(max_chunk + K + F*H)
map space  = Theta(N)
height     = O(log_F(N/K))
```

For a one-leaf same-count edit, mapping-object work is `O(log N)`. For a
count-changing edit that shifts `Z` suffix references, work/history is `O(Z)`,
worst-case `Theta(N)`.

For in-memory path depth `d` and ancestor sizes `E_i`, complete-map cloning and
provisional rehashing contribute `Theta(sum E_i)`. `Delta::between` adds another
changed-directory union/walk even though the mutation already knows the exact
before/after entry.

The sealed F4-A medians are 524.111750-ms mapping, 112.144334-ms standalone
COMMIT, and 636.836792-ms durable create. Yet canonical+mapping encoding is only
3.161540 ms; mapping bytes are only 365,262 of 105,291,554 canonical bytes
(`implementation-detail/phase-4/wp4m/f-series/f4/report.md:215-246`, `:278-315`,
`:371-374`).

Derived Amdahl bound:

```text
gap to 500 ms                         136.836792 ms = 21.49%
all canonical + mapping encoding        3.161540 ms = 0.50% durable
mapping/canonical byte ratio      365262/105291554 = 0.347%
```

Direct delta emission and in-memory map ownership are absent from the retained
genesis full-create path, so their full-create ceiling is effectively zero.
Even deleting the entire combined canonical+mapping encoding lane cannot close
the gap. COW redesign must be
judged on edits and namespace work, not sold as a 200-MiB/s write solution.

## Ranked directions

### 1. Emit delta while applying the explicit mutation — conventional

**Hypothesis:** the recursive mutator already observes every value needed for
the exact add/remove/replace/metadata delta (and rename's remove-then-add). It
can return that entry without running `Delta::between`.

- Target: wide/deep namespace edits.
- Improvement: constant-factor generally; removes a redundant `O(E)`
  rediscovery step at a changed wide directory.
- Full-create upside: none on the current genesis row.
- Risk: exact rename ordering, root metadata semantics, error parity.
- Kill question: does isolated `Delta::between` exceed 5% of real namespace-edit
  wall at 100,000 entries? If not, stop.

### 2. Consuming unique-owner mutation with `Arc::make_mut` — conventional

Rust's official [`Arc::make_mut`](https://doc.rust-lang.org/std/sync/struct.Arc.html#method.make_mut)
clones only when another owner exists. A separate consuming mutation path could
reuse uniquely owned nodes while preserving the existing borrowed persistent
API.

- Target: callers that discard the old in-memory root.
- Improvement: potentially changes unique-owner work from full-map cloning
  toward path/B-tree operations.
- Risk: mutating an observable parent or failing to recompute provisional IDs.
- Kill question: are real capture roots unique along the changed path? If not,
  `make_mut` just falls back to the existing clones.

### 3. Persistent ordered B-tree for core directories — conventional but larger

The Btrfs design and Rodeh's original work show bounded COW B-tree path copying
and root-switch publication
([Btrfs design](https://btrfs.readthedocs.io/en/stable/dev/dev-btrfs-design.html),
[B-trees, Shadowing, and Clones](https://static.usenix.org/event/lsf07/tech/rodeh.pdf)).
LayerFS already has immutable nodes and atomic root publication; only the
in-memory full-map owner is suspect.

- Target: repeated mutation in wide directories.
- Algorithmic aim: `Theta(sum E_i)` clone toward `O(d log E)` node path copying.
- Risk: a new collection implementation/dependency, canonical iteration and
  exact-error parity.
- Direction: attempt ranks 1 and 2 first; select this only if their measured
  ceiling remains large.

### 4. Hard-bounded prolly tree for reference sequences — disruptive

The original [Noms prolly-tree design](https://github.com/attic-labs/noms/blob/master/doc/intro.md)
uses content-dependent boundaries over ordered values, recursively, so equal
logical collections are history-independent and local insertions usually change
local chunks. This directly addresses fixed-ordinal suffix churn.

- Target: early/middle `+1/-1` edits at large `N`.
- Algorithmic aim: expected local nodes plus `O(log N)` ancestors instead of
  `Theta(N)` suffix rewrite.
- Full-create expectation: neutral or slower because boundary hashing is added;
  it cannot remove byte-linear capture.
- Format risk: very high—new canonical boundaries, roots, goldens, migration.
- Resource/security risk: average node size is insufficient. A deterministic
  min/max/forced boundary is mandatory; the official Noms discussion identifies
  adversarial oversized chunks
  ([issue 3878](https://github.com/attic-labs/noms/issues/3878)).
- Kill question: can a simulator cut 512-MiB early/middle rewritten bytes by at
  least 95% while keeping full build within 5%, hard maximum node size, and
  byte-identical roots across mutation histories?

### 5. Stream `DeltaRecord` identity instead of copying it — small conventional

Production `delta_identity` allocates `prefix || parent || child || payload`
and then hashes the copy (`crates/layerfs-engine/src/lib.rs:168-203`). The
existing `ObjectId::from_reader` can hash a chained reader with no payload-sized
temporary.

- Target: large delta creation/load and Q, not full create (genesis delta empty).
- Complexity: same `Theta(delta bytes)` time, auxiliary copy from `O(delta)`
  toward a fixed hash buffer.
- Kill question: are measured deltas large enough for this allocation/copy to
  matter?

## Alternatives to reject now

- B-epsilon trees buffer mutations for amortized writes, but the primary
  [USENIX description](https://www.usenix.org/publications/login/oct15/bender)
  implies pending-message/flush state. That adds recovery and compaction policy
  to a one-transaction immutable path and does not attack the measured hash
  majority.
- RRB trees provide logarithmic immutable split/insert
  ([original paper](https://hypirion.com/pdf/RMTrees.pdf)), but ordinary balance
  is history-dependent. They are an in-memory possibility, not a canonical
  durable format without a separate canonicalization proof.
- An unordered HAMT loses native lexical order/range behavior and still needs a
  canonical serialization.
- Do not sort/deduplicate delta entries, weaken before-state checks, add an
  unbounded visited map, or import packs/compaction/distributed manifests.

## Preserved invariants and decision

Every direction must preserve canonical path/name order, parent immutability,
unchanged sibling sharing, exact delta order and typed conflicts, checked
counts/offsets/depth, history-independent durable identity, complete incumbent
authentication, bounded Q, one transaction, one visible-head transition, one
COMMIT, and fresh ambiguous-COMMIT reconciliation.

The single decisive question for a future COW specialist is:

> Which measured workload still pays material `Theta(E)` or `Theta(N)` work:
> wide in-memory namespace mutation, or early/middle count-changing durable
> mapping—and how many milliseconds, allocations, and rewritten bytes does that
> cost relative to the whole operation?

Until that attribution exists, retain the accepted radix and changed-spine
algorithm. Start with direct delta emission, then unique-owner `Arc::make_mut`.
Authorize a prolly format only for a proven count-changing bottleneck, never as
a speculative full-create optimization.
