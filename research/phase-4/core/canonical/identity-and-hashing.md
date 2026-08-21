# Phase 4 identity and hashing optimization directions

Status: research only; no implementation or profile authorization.  Local
full-create performance is the objective.  `Observed`, `Derived`, and
`Hypothesis` have their literal meanings below.

## Current behavior and evidence

### Canonical identity

- **Observed:** `ObjectId` is a 32-byte BLAKE3 digest over
  `"layerfs/object\0" || canonical_object_bytes`
  (`crates/layerfs-core/src/identity/digest.rs:5-25,39-65` and
  `crates/layerfs-core/src/identity/ids.rs:8-34`).  The canonical Bytes object
  is `LFSO`, kind, payload length, raw length, then raw bytes
  (`crates/layerfs-core/src/object/codec.rs:11-55`).
- **Observed:** `ChunkId` is only a Rust alias of `ObjectId`; it uses the same
  domain but hashes raw chunk bytes rather than canonical Bytes framing
  (`crates/layerfs-core/src/identity/mod.rs:1-13`).  The durable file reference
  nevertheless stores all three of raw `ChunkId`, raw length, and canonical
  chunk `ObjectId` (`implementation-detail/phase-4/algorithm/spec.md:309-335,
  440-450`).
- **Observed:** complete canonical bytes are authenticated before their
  structure or strong edges are trusted
  (`crates/layerfs-core/src/object/codec.rs:153-165` and
  `implementation-detail/phase-4/mapping/logical-persistence.md:631-663`).

These rules are product-format semantics.  Changing or substituting either ID
changes compatibility, mapping roots, deltas, receipts, and reopen behavior.

### Accepted F2 construction path

The exact accepted source is preserved at
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`, SHA-256
`c8ac86be...cc158` (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:21-38`).

- **Observed:** every emitted chunk is hashed once as raw `ChunkId`, encoded as
  a canonical Bytes object, and hashed again as canonical `ObjectId` (accepted
  source `:1518-1527,3755-3789`).
- **Observed:** the private F2 construction witness also maintains a whole-file
  `source_hasher` and an ordered `(raw_length, raw_id)` `sequence_hasher`
  (accepted source `:3192-3208,3223-3249,3332-3367,3542-3543`).
- **Observed:** the whole-source digest is part of the benchmark construction
  proof/qualification.  It is not a field in a canonical Bytes object, file
  leaf, root, or durable mapping.  The production algorithm specification
  requires raw and canonical identities, but does not independently define a
  whole-source digest as canonical identity
  (`implementation-detail/phase-4/algorithm/spec.md:309-335`).
- **Conclusion:** the whole-source hash is **retained harness/witness work in
  the present private benchmark**, not established production canonical
  identity.  A future specialist must decide whether production authority
  needs that exact byte digest or whether the already-authenticated ordered
  chunk commitment is sufficient.  It must not simply move a required product
  hash outside the measured interval.

### Measured bottleneck

**Observed:** on the sealed 100-MiB F4-A diagnostic, the component medians are
(`implementation-detail/phase-4/wp4m/f-series/f4/report.md:272-315`):

| Disjoint measured lane | Median | Mapping share |
|---|---:|---:|
| raw `ChunkId` BLAKE3 | 95.185 ms | 18.16% |
| construction source/sequence BLAKE3 | 89.067 ms | 16.99% |
| canonical `ObjectId` BLAKE3 | 96.068 ms | 18.33% |
| all hash intervals | **280.147 ms** | **53.45%** |

The construction row contains both the whole-source and small ordered-sequence
updates; the evidence does not isolate them.  Calling its complete 89.067 ms
removable would be unsupported.

**Observed:** accepted F2-v3 durable capture is 659.593 ms; F4-A is an
observer-heavy diagnostic at 636.837 ms and does not replace that checkpoint
(`implementation-detail/phase-4/wp4m/f-series/f4/report.md:343-358`).

## Amdahl bounds from same rows

The following are **Derived upper bounds**, recomputed row-by-row from sealed
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/f4a.raw.jsonl`.  They do
not add independently selected medians.

| Hypothetical removal/acceleration | Five derived durable rows | Median | Meaning |
|---|---|---:|---|
| all construction source/sequence time | 525.935-550.034 ms | about 548 ms | upper bound; mandatory sequence replacement omitted |
| all raw-ChunkId time | 521.486-543.916 ms | about 542 ms | requires identity/profile change |
| both rows above | 427.084-454.849 ms | **452.873 ms** | 220.8 MiB/s upper bound; two semantic changes |
| 2x all three hash lanes | 475.431-499.028 ms | **496.842 ms** | all five rows under 500 ms; implementation plausibility unknown |

The useful ceiling is therefore large: one potentially redundant harness
commitment plus one redefinable format identity could theoretically close the
target.  Neither has yet been proved removable.  The lower bound is zero until
their authority replacements are proved.  The canonical `ObjectId` hash is not
freely removable: it is the current CAS authentication root.

## Primary-source precedents, not LayerFS evidence

- The [BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs)
  and [official implementation](https://github.com/BLAKE3-team/BLAKE3)
  describe a tree hash with SIMD and optional threading.  LayerFS pins
  `blake3 1.8.5` (`Cargo.toml:18-20`, `Cargo.lock:23-35`), and the current
  AArch64 release build records `blake3_neon` in
  `target/release/build/blake3-9a8053e7201992b5/output`.  Thus “enable SIMD” is
  not a new optimization hypothesis; NEON is already present.
- Git hashes one framed blob object (`blob <size>\0 || bytes`) and uses that
  object ID for both addressing and integrity; see the official
  [Git user manual](https://git-scm.com/docs/user-manual#_object_storage_format).
- The official [Xet hashing specification](https://github.com/huggingface/hub-docs/blob/main/docs/xet/hashing.md)
  computes one chunk hash from chunk data and derives file hashes from ordered
  chunk hashes.  This is a concrete precedent for avoiding an independent
  whole-file byte pass.

Git and Xet are design precedents only.  They do not prove that a LayerFS
identity change is safe or fast.

## Ranked directions

### 1. Separate harness qualification from product authority

- **Classification:** algorithmic pass elimination if valid; no canonical
  format change is necessarily required.
- **Hypothesis:** the ordered, framed `(length, authenticated chunk ID)`
  transcript already commits to the exact source, making the private
  construction `source_hasher.update(raw_bytes)` redundant.
- **Expected impact:** up to roughly 89 ms in the diagnostic; realistically
  slightly less because the ordered transcript remains.  This alone probably
  yields roughly 526-550 ms, short of 200 MiB/s.
- **Risk:** the source hash may bind an independently required caller claim
  that the sequence commitment cannot replace under the current authority
  model.
- **Decisive future question:** can a formal, domain-separated ordered
  commitment over `(length, canonical chunk ID)` prove exact source equality
  under the same failure model, while the benchmark keeps an independent
  post-commit oracle outside product authority?
- **Kill direction if:** authority analysis finds an exact-source property not
  implied by the transcript, or isolated source-hash removal saves less than
  33 ms in four of five full rows.

### 2. One canonical chunk identity in a new mapping profile

- **Classification:** disruptive algorithm/format change; full work remains
  `Theta(source bytes + references)`, but two full cryptographic passes can
  become one.
- **Hypothesis:** use the canonical Bytes `ObjectId` as the chunk identity for
  rejoin, deduplication, reconstruction, and ordered file commitment.  Store
  raw length plus canonical ID, not a second raw-payload ID.
- **Expected impact:** eliminating raw-ID hashing is about 95 ms.  Combining it
  with direction 1 has a row-derived upper-bound median of 452.873 ms, enough
  for the 200-MiB/s objective.
- **Risk:** all file leaves and roots change; old/new profile coexistence,
  migration, exact typed errors, rejoin semantics, and golden vectors need new
  authority.  Raw-ID removal also loses an independent check; security then
  rests on complete canonical-object authentication plus grammar/length checks.
- **Decisive future question:** does `(raw_length, canonical ObjectId)` preserve
  every current raw-ID use—especially edit rejoin and `ChunkIdentityMismatch`—
  without reading a payload that the old mapping could skip?
- **Kill direction if:** any current operation needs a raw-payload identity
  independent of canonical framing, or a full shadow profile fails to reach
  500 ms while preserving edit/storage gates.

### 3. Improve the exact BLAKE3 execution before changing identity

- **Classification:** constant-factor, identity preserving.
- **Hypothesis:** bounded chunk-size call overhead or current transcript
  scheduling leaves single-thread throughput below the available NEON backend.
- **Expected impact:** a true 2x improvement across all measured hash lanes
  would theoretically produce 475.431-499.028 ms.  Because NEON is already
  active and chunks average only about 19.8 KiB, that outcome is a stretch.
- **Decisive future question:** does an instrumentation-free, exact-output
  microbenchmark reproduce roughly 1.1 GB/s, and can a public supported BLAKE3
  call shape improve it by at least 33 ms end to end?
- **Kill direction if:** the optimized official API/backend is already within
  10% of the measured lane, or the full durable row improves less than 5%.

Do not reach directly for internal `hash_many`, custom cryptography, Rayon, or
a worker pipeline.  The public implementation already uses NEON; custom
multi-message code increases audit risk, and threading violates the current
synchronous caller-thread contract.

### 4. Make raw and canonical IDs distinct Rust newtypes

- **Classification:** correctness/type-safety only; zero expected throughput.
- **Benefit:** prevents accidental substitution that the current alias permits.
- **Recommendation:** worthwhile only with a nearby identity-format change;
  do not call it a performance milestone.

## Recommendations and anti-recommendations

1. First isolate whole-source versus sequence hash wall and resolve whether the
   whole-source digest is harness proof or required product authority.
2. If it is redundant, remove that pass as the lowest-format-risk direction.
3. In parallel design—not implementation—evaluate a versioned single-canonical-
   chunk-ID profile.  It is the only identity direction with enough measured
   ceiling to close 200 MiB/s without assuming miraculous hash-code speedup.
4. Benchmark the supported BLAKE3 backend once, but expect a constant-factor
   result, not an algorithmic breakthrough.

Do not replace BLAKE3 with a noncryptographic hash, derive an `ObjectId` from an
unauthenticated locator, trust raw bytes without complete canonical hashing, or
silently reinterpret old `ChunkId` fields.  Do not count benchmark-only source
qualification as production work without proving that authority relationship.
