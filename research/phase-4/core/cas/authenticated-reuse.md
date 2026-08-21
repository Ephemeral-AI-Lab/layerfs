# CAS and authenticated reuse

Status: research only; no identity, authority, storage, or implementation
change is authorized. Snapshot: 2026-08-20, accepted F2-v3 plus sealed F4-A
and F4-A2 evidence.

## Direction

**Observed:** LayerFS's CAS semantics are already sound: exact canonical bytes
define an immutable ID, a key is only a locator, an incumbent is fully
authenticated before reuse, and one complete head is published atomically. The
retained fresh 100-MiB row creates 5,372 objects and reuses zero, so a faster
reuse lookup, Bloom filter, or verified-object cache has zero first-order
upside on the headline full-create row.

**Observed:** the dominant CAS-adjacent cost is instead three BLAKE3 streams:
raw `ChunkId` 95.185 ms, construction source/sequence 89.067 ms, and canonical
`ObjectId` 96.068 ms. Their exact analyzer subtotal is 280.147 ms, 53.45% of
the 524.112-ms diagnostic mapping phase
(`target/wp4m-f4a-residual-attribution-k64-20260820-v1/FINAL-REPORT.md:30-58`).

**Recommendation:** under the active synchronous caller-thread/no-worker
contract, first audit whether the whole-source/CDC-sequence digest is an
independently required product authority or a redundant campaign witness once
exact root, construction proof, and fresh verification agree. A small
single-thread hash-shape benchmark may then test streaming the same canonical
bytes into their existing hasher without changing outputs. A bounded multicore
pipeline is potentially larger, but it is disruptive research that requires a
new execution-profile authorization. Reuse caching belongs to edit/read work,
not fresh create.

## Current authority and evidence

- **Observed:** `ObjectId = BLAKE3("layerfs/object\0" || exact bytes)` in slice
  and reader forms (`crates/layerfs-core/src/identity/digest.rs:6-65`).
- **Observed:** `ChunkId` uses that domain over raw chunk bytes; a canonical
  chunk-object ID uses it over the complete `LFSO` Bytes encoding. They are
  distinct commitments (`crates/layerfs-core/src/identity/mod.rs:8-13` and
  `crates/layerfs-core/src/object/codec.rs:22-37`).
- **Observed:** the builder computes raw ID, canonical encoding and canonical
  ID, then stores `(raw_id, raw_length, object_id)` in order
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3755-3785`).
- **Observed:** admission is cached `INSERT ... ON CONFLICT DO NOTHING`. A
  conflict triggers a complete row read plus ID, kind, length and byte-equality
  checks before reuse
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2259-2345`).
- **Observed:** F2's transaction-local `PutEvidence` is issued only after insert
  or complete incumbent authentication and is bound to open, transaction,
  authority and mutation serials
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2209-2257`).
  It removed the old 5,373-object pre-COMMIT replay while fresh post-COMMIT
  verification remained independent
  (`implementation-detail/phase-4/wp4m/f-series/f2/report.md:64-125`).
- **Observed:** ordinary SQLite reads borrow the row BLOB but still validate the
  complete ID before semantic use
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2386-2410`).
- **Observed:** F4-A2 found only 3.702 ms median removable scanner-owned chunk
  materialization, 0/5 at the 33-ms gate. Copy avoidance is not the strategic
  lever (`target/wp4m-f4a2-cdc-materialization-k64-20260820-v1/FINAL-REPORT.md:85-117`).

Venti provides primary precedent for the rule that content fingerprints create
write-once names, coalesce duplicates, and allow readers to verify returned
content; it is not LayerFS performance evidence
([Quinlan and Dorward, FAST 2002](https://www.usenix.org/conference/fast-02/venti-new-approach-archival-data-storage)).

## Invariants

Every current-contract direction below must preserve synchronous caller-thread
execution with no workers, exact canonical/raw identities, immutable
equal-only reuse, typed error precedence, ordered CDC occurrences and strong
edges, one writer and complete head publication, fresh ambiguity
reconciliation, bounded memory with terminal `Q=0`, and fresh scrub/
reconstruction/range authentication. A cached key is never byte authority.
The controlling receipt distinction is explicit at
`implementation-detail/phase-4/rollback/spec.md:188-214`.

## Ranked avenues

### Disruptive direction — bounded ordered hash pipeline

**Hypothesis:** under a separately authorized execution profile, use a fixed
number of local worker slots for independent
per-chunk raw/canonical hash work while one owner preserves ordinal proof
folding and SQLite write order. The caller still waits synchronously before the
single COMMIT. This is local parallelism, not distribution, but it breaks the
active Phase-4 caller-thread/no-worker invariant and is not a current-contract
candidate.

**Algorithmic effect:** work stays `Theta(B + N)`; with fixed `p`, hash span can
approach `Theta(B/p + N)`. Memory is `O(p*Cmax)` under fixed slots and maximum
canonical size. BLAKE3 explicitly supports tree/SIMD/multicore parallelism, but
its official Rust documentation warns that per-input Rayon hashing is often
slower below roughly 128 KiB. Average LayerFS chunks are about 20 KiB, so the
experiment must parallelize across chunks, not call `update_rayon` per chunk
([BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex),
[official API](https://docs.rs/blake3/latest/blake3/struct.Hasher.html#method.update_rayon)).

**Derived theoretical span bound:** the three observed intervals total
280.147 ms, but the 89.067-ms construction lane contains an ordered
whole-source/sequence stream and cannot be assumed to split like independent
per-chunk hashes. If scheduling could overlap all three unchanged lanes, their
hash-only span could not be lower than the largest observed lane, 96.068 ms,
for a purely theoretical overlap ceiling of about 184.079 ms. This is not a
perfect three-way speedup or an attainable Amdahl result: data dependencies,
CDC, proof order, queueing, CPU/cache contention, SQLite and COMMIT remain. No
realistic saving range is supported until a diagnostic exists.

**Decisive future question after execution-profile authorization:** can a
2/3/4-slot bounded replay of the sealed
chunk boundaries produce byte-identical IDs/order while saving at least 60 ms
median in 4/5 rows without increasing CPU over 10%? Stop below 33 ms or on any
unbounded reorder state, cleanup failure, or second hash after join.

### 1. Root-derived qualification audit — strongest current-contract question

**Hypothesis:** exact expected root plus per-occurrence construction proof and
fresh verification may already commit to the ordered `(raw length, raw ID,
canonical ID)` sequence, making the additional whole-source/sequence digest a
redundant witness for genesis create.

**Algorithmic effect:** total create remains `Theta(B + N)`, but one complete
`Theta(B)` validation stream is removed. The current specification calls this
stream required (`implementation-detail/phase-4/algorithm/complexity-analysis.md:1476-1517`),
so this needs a security/authority decision before code.

**Derived ceiling:** at most 89.067 ms, 13.99% of diagnostic durable time;
about 547.770 ms if removed perfectly. It cannot reach 500 ms alone.

**Decisive future question:** can a formal implication and adversarial shadow
proof show that every omission, duplication, reordering, wrong length/ID and
wrong topology caught by the digests is also caught by exact root/proof/fresh
verification? Kill the direction if either digest catches an independent fault
or is an interoperability output.

If the digest remains required, the next compatible probe is a
**single-thread hash-shape benchmark**: emit the exact canonical bytes to the
existing output and existing `ObjectId` hasher in one writer path, preserving
all bytes and domains. This changes no asymptotic work and has no supported
large ceiling—the observed encoding interval is only 3.162 ms—but it can decide
whether a second hot-buffer traversal is measurable without adding workers.

### 2. Verified-locator cache — edit/read direction only

**Hypothesis:** a fixed-capacity cache bound to store/open/epoch/generation,
object ID, immutable row/offset, length and kind can avoid repeated full hashes
within one proven scope. Invalidate on mutation, rollback, COMMIT, reopen,
authority change or eviction.

**Algorithmic effect:** worst case stays `Theta(A)`; repeated hot-object work
can fall from `r*Theta(b)` to `Theta(b)+O(r)` for fixed cache capacity.

**Impact:** **Derived zero** on retained fresh create (zero reuses). Edit/read
upside is **Unavailable** until a shadow hit/byte/wall study exists.

**Decisive future question:** does a no-skip shadow LRU predict at least 5% of a
protected edit/read operation in 4/5 rows with a small fixed capacity? Kill on
thrashing, unverifiable locator stability, or any fresh-create cost above 1%.

### 3. Hash-through-reference Bytes format — disruptive future format

**Hypothesis:** a versioned canonical descriptor could commit to raw payload ID
and length rather than inline-hash the payload again. Raw payload hashing stays
`Theta(B)`; additional canonical identity becomes `Theta(N)`. This follows the
transitive content-address pattern used by Merkle/CAS systems, but changes every
chunk object, root, transition, closure and migration rule.

**Derived ceiling:** the canonical-ID interval is 96.068 ms. Together with a
separately justified removal of construction qualification, the gross ceiling
is 185.135 ms (29.07% diagnostic durable). Replacement lookup and verification
must be subtracted.

**Decisive future question:** is byte-self-contained canonical `Object::Bytes`
a permanent product invariant? If yes, stop. If no, a specialist may compare a
research codec only after migration and transitive-authentication rules exist.

## Anti-recommendations

- **Bloom/quotient filter for fresh inserts:** SQLite still enforces uniqueness;
  no retained duplicates exist to save.
- **Key-only or page-checksum trust:** SQLite's checksum VFS detects random page
  bit flips but is not the cryptographic object identity and cannot replace
  incumbent authentication
  ([SQLite checksum VFS](https://www.sqlite.org/cksumvfs.html)).
- **Unbounded authenticated-object map:** violates the memory contract and hides
  a new authority source.
- **Restore packed CAS or add per-object files:** both are rejected historical
  directions; neither attacks the 280-ms hash subtotal.
- **Treat a pack/carrier as a hash optimization:** physical aggregation changes
  storage calls, not the required hash outputs.

## Recommendation

For fresh full-create under the active contract, send a future specialist the
root/qualification-authority question first, followed only by the compatible
single-thread hash-shape probe if the digest must remain. For edit/read, send
the verified-locator shadow question. Treat the bounded multicore pipeline and
hash-through-reference format as separately authorized disruptive research.
The current SQLite conflict/authentication path remains the rational default.
