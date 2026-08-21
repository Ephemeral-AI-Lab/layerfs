# CAS + CDC + COW core pipeline

Status: direction-finding research only. No code, profile, format, identity,
worker, or SQLite change is authorized. Snapshot: 2026-08-20, accepted F2-v3
source and sealed F4-A/F4-A2 evidence.

## Conclusion

**Observed:** the accepted full-create core is already a bounded one-source-pass
pipeline. It does not retain all chunks, all objects, or a source-sized map.
Its large residual is three byte-complete BLAKE3 outputs—raw chunk identity,
whole-source/construction qualification, and canonical object identity—whose
measured intervals total 280.147 ms. CDC-exclusive work adds 128.723 ms. By
contrast, canonical/mapping encoding is 3.162 ms and proof/bookkeeping is
1.307 ms. Eliminating another small buffer or proof loop cannot close the
159.6-ms accepted-F2 gap.

**Direction:** under the current synchronous caller-thread contract, first
decide whether the 89.067-ms whole-source/CDC-sequence qualification lane is
product authority or redundant harness evidence. If it must remain, the only
compatible large gross component left is the exact-boundary FastCDC gear loop;
a later specialist should ask whether its implementation can become materially
faster without changing a single boundary. Canonical tee writers and fused
mapping buffers are valid cleanup candidates but have low measured ceilings.

**Disruptive directions:** a larger CDC profile may reduce gear work and object
count at the price of changed roots, deduplication and edit/range locality. A
versioned single canonical chunk identity is more strategically interesting:
it can remove the separate raw-ID hash on full create and the raw rehash after
canonical authentication on scrub/reconstruction, while shrinking each durable
file reference from 68 to 36 bytes. It changes the mapping format and every
root/transition, so it is not a transparent optimization.

## 1. Actual pass, buffer, and authority graph

### 1.1 Foreground data path

```text
File::read
  -> stack input window [u8; 32,768]
  -> FastCDC gear scan
  -> scanner-owned Vec<u8>, capacity 32,768
  -> borrowed complete chunk callback
       ├─ BLAKE3(domain || raw) -> raw ChunkId
       ├─ exact LFSO Bytes encode -> canonical Vec, at most 32,781 bytes
       ├─ BLAKE3(domain || canonical) -> canonical ObjectId
       ├─ immutable CAS insert / authenticated equal reuse
       │    -> transaction/open/mutation-bound PutEvidence
       ├─ whole-source hasher.update(raw)
       ├─ CDC-sequence hasher.update(length || raw ChunkId)
       └─ append FileReference { raw_id, raw_length, object_id } to K=64 leaf
             -> canonical leaf object
             -> F=64 child frontier / canonical branch objects
             -> canonical file root
             -> singleton workspace Directory root
             -> genesis transition
             -> consume one transaction-local construction proof
             -> stage complete visible head and receipt
             -> one COMMIT
```

**Observed code authority:**

- FastCDC uses an exact 8/16/32-KiB min/target/max profile, one 32-KiB input
  array, one reusable 32-KiB chunk `Vec`, a pending byte, and two-byte Gear
  judgments (`crates/layerfs-core/src/cdc/mod.rs:11-20`,
  `crates/layerfs-core/src/cdc/mod.rs:36-75`, and
  `crates/layerfs-core/src/cdc/mod.rs:77-179`). Fragmented input must reproduce identical boundaries
  (`crates/layerfs-core/src/cdc/mod.rs:214-255`).
- The callback computes raw ID, asks the store to canonically encode/hash/
  persist the chunk, observes proof authority, and appends one reference
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3755-3812`).
- Canonical Bytes construction owns one exact `raw_len + 13` buffer
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:820-834` and
  `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2195-2221`). The canonical object is the persistence handoff; rusqlite may
  copy it later, but that is outside this core analysis.
- `FileReference` is exactly `raw_id[32] || raw_length[4] || object_id[32]`, 68
  serialized bytes. Leaves and branch descriptors are canonically bounded
  arrays (`crates/layerfs-core/src/content/persistence.rs:20-93` and
  `crates/layerfs-core/src/content/persistence.rs:135-195`).
- `FileBuilder` holds only the current K-reference leaf, bounded F-child
  frontiers, totals, and proof frontiers. Full leaves/branches are encoded,
  persisted, folded into authority, and cleared
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3702-3812`,
  `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3869-3963`, and
  `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3983-4140`).
- Proof folding accepts only the next exact `PutEvidence`, checks reference/
  child totals and topology, and carries source/sequence digests plus the final
  file summary
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3305-3551`). Workspace and
  genesis-transition evidence are folded afterward
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3581-3614` and
  `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:5198-5205`).
- Proof consumption checks the live transaction, store/open/authority/epoch/
  profile/mutation scope and empty prior head, then returns the bound source,
  sequence, root and transition
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3618-3685`).
  COMMIT follows immediately after qualification
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:9841-9916`).

### 1.2 Authority cannot be reordered freely

```text
raw bytes --------------------------> raw ChunkId
canonical bytes --------------------> canonical ObjectId
CAS insert / full incumbent auth ---> PutEvidence
PutEvidence + exact reference ------> leaf proof
child proofs + cumulative totals ---> branch/file proof
file proof + workspace/transition --> full-create proof
live scoped proof consumption ------> publication qualification
complete head + one COMMIT ---------> visibility
```

**Derived:** canonical encoding and hashing may share a writer/buffer, but proof
authority cannot be issued before CAS insert or complete incumbent
authentication. Likewise the parent proof cannot precede its exact child
evidence. The existing pipeline already folds each result at the earliest safe
authority boundary.

### 1.3 Measured pass budget

**Observed five-row F4-A medians**
(`target/wp4m-f4a-residual-attribution-k64-20260820-v1/FINAL-REPORT.md:30-58`):

| Core/edge component | Median | Share of 524.112-ms mapping | Status |
|---|---:|---:|---|
| Source read | 16.468 ms | 3.14% | required |
| CDC-exclusive Gear scan | 128.723 ms | 24.56% | exact-boundary algorithm |
| Raw `ChunkId` hash | 95.185 ms | 18.16% | current identity |
| Construction source/sequence hash | 89.067 ms | 16.99% | current qualification contract |
| Canonical `ObjectId` hash | 96.068 ms | 18.33% | current identity |
| All three hash intervals | 280.147 ms | 53.45% | distinct outputs |
| Canonical + mapping encode | 3.162 ms | 0.60% | required bytes |
| Proof/bookkeeping | 1.307 ms | 0.25% | required authority |
| Persistence API parent | 74.336 ms | 14.18% | storage handoff, not core-only |

F4-A2 independently found that replacing full scanner chunk materialization
with borrowed windows plus mandatory carry saved only 3.702 ms median, 0/5 at
the 33-ms gate
(`target/wp4m-f4a2-cdc-materialization-k64-20260820-v1/FINAL-REPORT.md:85-117`).

**Observed work:** 104,857,600 source bytes become 5,284 chunk references,
5,372 new canonical objects, 83 leaves, two branches, and 365,262 mapping bytes.
All objects are new in the retained fixture
(`target/wp4m-f4a-residual-attribution-k64-20260820-v1/FINAL-REPORT.md:88-101`).

**Derived:** average raw chunk size is
`104,857,600 / 5,284 = 19,844.4 bytes`. Mapping bytes are only 0.35% of source;
mapping encoding/proof micro-optimizations have little full-create headroom.

## 2. Ranked current-contract directions

### 1. Qualification-authority audit

**Hypothesis:** exact root and transition, transaction-local occurrence/edge
proof, and independent post-COMMIT verification may already imply the expected
ordered source content, making the whole-source and CDC-sequence digests
redundant campaign witnesses rather than product authority.

**Effect:** removes one byte-complete `Theta(B)` hash stream but leaves total
create `Theta(B+N)`. **Derived perfect-removal ceiling:** 89.067 ms, 13.99% of
the 636.837-ms diagnostic durable row; insufficient alone for 500 ms.

**Compatibility/risk:** caller-thread, bytes, CAS and storage stay unchanged;
assurance semantics may weaken. The decisive future question is whether an
omitted, duplicated, reordered, wrong-length or wrong-ID occurrence exists that
the source/sequence digests detect but exact root + construction proof + fresh
verification do not. If yes, stop. This is an authority proof task before a
benchmark task.

### 2. Exact-boundary FastCDC hot loop

**Hypothesis:** preserve every boundary and error while reducing scalar Gear
judgment overhead inside the accepted two-byte loop. F4-A2 shows that removing
chunk-buffer materialization is not enough; any useful result must improve the
approximately 121-129-ms boundary/Gear parent itself.

FastCDC's original paper explicitly trades CDC CPU, average chunk size, and
deduplication through minimum skipping and normalized masks
([Xia et al., USENIX ATC 2016](https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia)).
Google's official FastCDC implementation is useful code precedent but modifies
the judgment rule, so it cannot be copied under exact-boundary compatibility
([google/cdc-file-transfer](https://github.com/google/cdc-file-transfer/blob/main/fastcdc/fastcdc.h)).

**Effect:** same `Theta(B)` work and same profile; constant-factor loop change.
**Gross ceiling:** 128.723 ms, but no removable subcomponent is isolated.
The decisive question is whether an exact-boundary scalar implementation can
save at least 33 ms in 4/5 boundary-only rows. Stop if boundaries, short-read
behavior, pending-byte behavior, or fragmentation independence differ, or if
the gain is below 5% of durable capture.

### 3. Single canonical construction/hash writer

**Hypothesis:** prefeed the exact 13-byte `LFSO` Bytes header to the existing
canonical hasher and stream the same raw slice into both the exact canonical
buffer and hasher, instead of building the buffer and then traversing it. For
mapping nodes, write outer Bytes framing plus mapping grammar directly into one
exact canonical buffer rather than building an inner mapping `Vec` and wrapping
it in a second `Vec`.

**Effect:** identities and `Theta(B)` hash compression are unchanged. It removes
buffer traversals/allocations, not the 96.068-ms cryptographic computation.
BLAKE3 1.8.5 already uses NEON by default on AArch64; LayerFS does not have a
missing single-thread SIMD feature to switch on
(`Cargo.toml:19`; [official BLAKE3 Rust documentation](https://docs.rs/blake3/latest/blake3/)).

**Ceiling:** encoding is 3.162 ms and F4-A2 materialization is 3.702 ms; any
additional cache-read saving is **Unavailable**. The decisive question is
whether an exact-output single-thread microbenchmark saves 33 ms. Expect a stop,
not a major win, if only encode/copy counters move.

### 4. Proof/reference fusion cleanup

The builder and proof each maintain leaf totals and `flush_leaf` iterates the
same at-most-64 references again
(`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:3370-3401`
and `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:4099-4139`). A single checked summary could avoid those bounded scans,
but the independent recomputation catches builder/proof divergence.

**Gross ceiling:** proof/bookkeeping is 1.307 ms. Do not weaken the independent
check for this. The direction is rejected for performance unless a future
profile makes proof wall exceed 5%.

## 3. Versioned/profile directions

### 1. Larger FastCDC chunks

**Hypothesis:** a 16/32/64-KiB or 32/64/128-KiB profile skips a larger prefix of
Gear judgments and produces fewer chunks, references, CAS admissions, and radix
nodes. Byte-complete raw/canonical/source hashing and payload persistence remain
approximately unchanged.

FastCDC reports the governing conflict: larger average chunks reduce metadata
and may improve chunking speed, while smaller chunks generally improve
deduplication. Its minimum-cut skipping can reduce deduplication, which
normalized masks are designed to mitigate. Restic's official format uses much
larger 512-KiB–8-MiB chunks targeting 1 MiB, but that backup workload is only
proof that chunk size is workload-specific, not LayerFS evidence
([restic design](https://github.com/restic/restic/blob/master/doc/design.rst)).

**Derived shape model, not prediction:** if average size doubled while the
distribution stayed similar, references would move from 5,284 toward 2,642 and
K64 leaves from 83 toward 42. The 280-ms byte-hash subtotal would not halve.

**Compatibility/risk:** changes every boundary, chunk/raw/canonical ID sequence,
file root, transition, fixture and profile. Larger chunks increase new payload
bytes after a local edit and complete-object authentication for small ranges;
they may improve full scrub/materialization by reducing object/query count.
The decisive question is whether a new profile saves at least 60 ms durable
while preserving an acceptable dedup ratio and keeping same-count edit,
count-changing rejoin, small-range authentication bytes, Q/RSS and complete
lifecycle within prospective limits. Do not infer this from object count alone.

### 2. One canonical chunk identity

**Hypothesis, format-breaking:** durable references become
`{raw_length, canonical_object_id}`. The canonical Bytes ID already commits to
the exact fixed header, encoded raw length and raw bytes. Under the same
collision-resistance assumption, canonical-ID equality is therefore sufficient
for raw-chunk equality in a single codec/profile; the separately stored raw ID
can be removed.

**Direct structural effects:**

- serialized reference size falls from 68 to 36 bytes, saving exactly
  `32 * 5,284 = 169,088` leaf bytes on the retained sequence before changed
  headers/topology;
- full-create raw hashing can fall by the observed 104,857,600 bytes / 5,284
  hashes; its write-side gross wall is 95.185 ms;
- fresh scrub and reconstruction currently each authenticate about 105.29 MiB
  of canonical bytes and then hash all 104,857,600 raw bytes again. Those exact
  counters are observed in
  `target/wp4m-f4a-residual-attribution-k64-20260820-v1/rows/row-3.json`;
- small range verification currently raw-hashes 133,140 bytes across eight
  chunks after authenticating 174,891 canonical bytes in that row.

Read-side wall attributable only to the raw rehash is **Unavailable**; the
write-side 95.185-ms interval is a prior, not a scrub/reconstruction claim.

**CDC rejoin/COW consequence:** current rejoin scans `(raw_length, raw_id)` and
requires two matching chunks
(`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:4869-4955`). A canonical-only profile can
scan `(raw_length, canonical_id)` instead. The ID must be computed from exact
Bytes framing during the scan and carried into persistence so it is not hashed
again. Same-codec equality is preserved, but cross-version rejoin is not:
migration or mixed profiles require a full translation/rechunk policy. K/F COW
topology remains deterministic but every leaf/root identity changes.

**Algorithmic effect:** full-create payload hashing drops from three
byte-complete streams toward two; total remains `Theta(B+N)`. Mapping metadata
shrinks by `Theta(N)`. Full scrub/reconstruction remove one `Theta(B)` raw
verification pass because canonical authentication plus strict decode supplies
the raw bytes and length.

**Decisive question:** is the raw `ChunkId` an independently required public
identity, or only a redundant durable/rejoin witness for canonical Bytes? A
future specialist must prove equivalence for malformed length/framing,
collisions, rejoin, range reads, edit COW, mixed versions and migration before
measuring. Stop immediately if independent raw identity is a permanent format
contract.

### 3. Bounded multicore pipeline

BLAKE3 supports SIMD and multicore tree hashing, and independent chunks can be
processed in parallel
([BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex)).
A fixed-slot ordered pipeline could overlap per-chunk identity work and storage
handoff, but it breaks Phase 4's synchronous caller-thread/no-worker invariant.
It is a separately authorized execution profile, not a current optimization;
the active contract forbids hidden workers and queues
(`implementation-detail/phase-4/algorithm/spec.md:1140-1160`).
The ordered 89.067-ms source/sequence lane also cannot simply be assumed to
split like independent chunk hashes.

## 4. Hot and cold materialization constraints

**Observed medians:** fresh full scrub is 280.251 ms, reconstruction 438.070 ms,
range verification 0.726 ms, and complete lifecycle 1,357.131 ms
(`implementation-detail/phase-4/wp4m/f-series/f4/report.md:257-270`). Cache state
is warm-or-unknown; cold APFS behavior is **Unavailable**.

- Larger chunks reduce object/index traversal for full materialization but make
  each small range authenticate a larger complete object. They also enlarge the
  expected changed chunk after a local edit.
- A fused writer changes neither read format nor materialization.
- Removing only redundant qualification improves durable create but not fresh
  scrub or reconstruction.
- A canonical-only identity can benefit both create and reads because current
  reads already authenticate the complete canonical object before repeating raw
  hashing. It is the only proposed core format direction with a plausible
  two-sided create/materialization benefit.
- Any hot verified-locator cache must remain a separate bounded authority; it
  cannot be used to claim cold/reopen improvement.

## 5. Direction map

| Rank | Direction | Target | Observed gross ceiling | Contract class | Main interaction |
|---:|---|---|---:|---|---|
| 1 | Qualification-authority audit | fresh create | 89.067 ms | current execution; authority review | no read benefit |
| 2 | Exact-boundary FastCDC loop | fresh create/edit scan | 128.723 ms gross parent | current profile if boundaries exact | CDC only; no object-count change |
| 3 | Canonical construction/hash writer | fresh create | 96.068 ms hash is mandatory; 3.162-ms encode observed | current | likely low ceiling |
| 4 | Larger chunk profile | create, full materialization | CDC 128.723 ms plus per-object work; net unavailable | versioned profile | dedup/edit/range tradeoff |
| 5 | Single canonical chunk identity | create + scrub + reconstruction | 95.185-ms write raw hash; read raw wall unavailable | format/identity migration | rejoin and every root change |
| 6 | Bounded multicore pipeline | fresh create | 280.147-ms hash subtotal, not all parallel | disruptive execution | ordered qualification remains |
| Stop | Proof/reference micro-fusion | create | 1.307 ms | current | assurance loss not justified |

## Recommendation

The next current-contract specialist question is:

> Is whole-source/CDC-sequence qualification independently necessary, and if
> it is, can the exact accepted FastCDC boundary loop—not its already-refuted
> chunk materialization—save at least 33 ms without changing one boundary?

The next versioned-format question is:

> Can canonical Bytes identity replace raw `ChunkId` in durable references and
> rejoin, removing one byte-complete hash pass on both create and authenticated
> reads while preserving edit locality and a credible migration story?

Do not prioritize another buffer abstraction, proof refactor, statement group,
or SQLite setting until those core questions are answered.
