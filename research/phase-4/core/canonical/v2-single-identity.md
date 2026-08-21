# Canonical core redesign: one chunk identity and one ordered commitment

Status: direction-finding only. This report does not authorize a new profile,
format, migration, implementation, benchmark campaign, or production change.
It distinguishes **Observed**, **Derived**, and **Hypothesis** literally.

## Result first

The combined redesign is technically coherent and has enough measured ceiling
to be the first deep optimization direction:

```text
current chunk occurrence
  raw ChunkId       = BLAKE3(object-domain || raw bytes)
  canonical ObjectId = BLAKE3(object-domain || LFSO Bytes framing || raw bytes)
  raw length

candidate v2 occurrence
  canonical chunk ObjectId
  raw length

candidate ordered commitment
  BLAKE3(ordered-commitment-domain || repeated(u32be(length) || ObjectId))
```

The canonical `ObjectId` remains the CAS locator and complete-byte
authenticator. The candidate deletes the separate raw-payload identity from new
mapping leaves and uses the canonical ID for CDC rejoin and ordered file
commitment.

The second proposed deletion is narrower than it first appears. The exact raw
fixture fingerprint is benchmark custody data and should remain available
outside product authority. The private F2 construction witness also hashes the
complete source inside durable capture; that inner whole-source digest is not a
canonical object or persisted mapping field. Its product necessity is not yet
established. The measured 89.067-ms construction lane combines this whole-file
hash with a small ordered-sequence hash, so source-only removable wall is
currently **Unavailable**.

**Derived optimistic ceiling:** subtracting the raw-ID lane and the entire
combined construction-hash lane row by row from sealed F4 evidence yields
427.084-454.849 ms, median 452.873 ms. This is enough for 200 MiB/s, but it is
an upper bound because the ordered replacement commitment remains mandatory.

## 1. Actual identity model

### 1.1 Core identities

- **Observed:** `ObjectId` is 32-byte BLAKE3 over
  `"layerfs/object\0" || complete_bytes`
  (`crates/layerfs-core/src/identity/digest.rs:5-25,39-65`;
  `crates/layerfs-core/src/identity/ids.rs:8-34`).
- **Observed:** `ChunkId` is a Rust type alias of `ObjectId`, using the same
  domain over raw chunk bytes
  (`crates/layerfs-core/src/identity/mod.rs:1-13`). It is not type-safe domain
  separation; only its preimage distinguishes the role.
- **Observed:** canonical chunk bytes are a Phase-1 Bytes object: `LFSO`, kind,
  payload length, raw length, raw payload (`object/codec.rs:11-55`). The
  canonical ID therefore already commits to kind, both framed lengths, and
  every raw byte.
- **Observed:** current durable file references serialize
  `raw_id[32] || raw_length[4] || object_id[32]`, exactly 68 bytes
  (`crates/layerfs-core/src/content/persistence.rs:20-65`). K64 full leaves are
  4,380 canonical bytes
  (`implementation-detail/phase-4/mapping/logical-persistence.md:669-680`).

### 1.2 Full create

The exact accepted F2 source is
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`, SHA-256
`c8ac86be...cc158` (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:21-38`).

- **Observed:** each CDC chunk is raw-hashed, canonically encoded,
  canonical-hashed, stored, and appended as the three-field reference (accepted
  source `:1518-1527,3755-3789`).
- **Observed:** transaction-local construction evidence binds the returned
  canonical `ObjectId`, kind, canonical length, open, transaction, authority,
  and mutation serial before folding the reference (`:3192-3367`).
- **Observed:** `source_hasher.update(raw)` and
  `sequence_hasher.update(length || raw_id)` are both finalized into the
  move-only construction proof (`:3192-3249,3332-3367,3542-3543`).
- **Observed:** proof consumption compares those digests, count, total, root,
  and transition to caller-supplied qualification (`:3664-3685`).

### 1.3 Rejoin and edits

- **Observed core behavior:** `InMemoryCas` is keyed by raw `ChunkId` and
  rehashes raw bytes on put, incumbent reuse, and get
  (`crates/layerfs-core/src/cas/mod.rs:15-76`). `LogicalFile` stores only raw
  `(ChunkId, length)` references, uses them for ranges, and compares complete
  references during two-chunk rejoin confirmation
  (`crates/layerfs-core/src/content/mod.rs:25-95,151-269,557-620`). A genuinely
  end-to-end single-identity profile must therefore change or adapt the Memory
  lane too; changing only the durable leaf codec would leave the duplicate
  identity model in the shared core.
- **Observed:** same-count edit rejoin scans bounded old/new windows into
  `(start, raw_id, raw_length)` records and requires two equal length+raw-ID
  confirmations (`accepted source:4869-4955,5020-5186`).
- **Observed:** after selecting replacement chunks, `store_reference` computes
  raw ID again, then canonical encoding and canonical ID. The candidate can
  instead compute the canonical ID once during rejoin scanning and reuse it
  when storing.
- **Observed:** fixed-ordinal K64 mapping gives excellent same-count changed-
  spine behavior but count-changing edits may still rewrite a suffix. Identity
  collapse does not fix that topology
  (`implementation-detail/phase-4/algorithm/complexity-analysis.md:418-498`).

Canonical IDs can replace raw IDs in the rejoin predicate because every v1
reference already stores its canonical `object_id`. A new writer can compare a
newly computed canonical ID with either a v1 reference's `object_id` or a v2
reference's sole ID without fetching the old payload.

### 1.4 Reconstruction, scrub, and ranges

- **Observed:** lookup follows `reference.object_id`; storage authenticates the
  complete canonical BLOB against that ID. The consumer then decodes raw bytes,
  checks raw length, and hashes the raw payload again against `raw_id`
  (`accepted source:6608-6682,6820-6873,7563-7613`).
- **Observed:** changed-spine qualification likewise authenticates a changed
  canonical object, then raw-hashes its decoded payload (`:6971-7053`).
- **Consequence:** v2 removes a second full raw-payload hash from cold scrub,
  reconstruction, changed-subtree verification, and selected range chunks.
  Their exact wall saving is unmeasured; it is additional lifecycle upside, not
  part of the durable-create ceiling below.

### 1.5 Deltas, receipts, and publication

- **Observed:** deltas store parent/child root IDs and ordered tree operations,
  not raw chunk IDs (`crates/layerfs-core/src/delta/mod.rs:10-105`). New mapping
  descendants change file/workspace roots, so resulting transitions and delta
  IDs change even when logical bytes are equal.
- **Observed:** `ValidatedSnapshotReceiptV1` binds child root, transition, and
  `mapping_profile_id`, not source or CDC digests
  (`crates/layerfs-core/src/validation.rs:7-59,62-130`).
- **Consequence:** identity collapse is naturally a new mapping profile. It
  does not require adding source fingerprints to receipts, but every receipt
  and golden rooted in old mapping IDs remains profile-specific.

## 2. Benchmark custody versus product authority

There are three different commitments that must not be conflated.

| Commitment | Current location | Role | Candidate treatment |
|---|---|---|---|
| raw fixture BLAKE3 | fixture manifest, expectations sidecar, JSON row | campaign custody and unchanged input | retain outside product authority and timing |
| CDC sequence fingerprint over `length || raw_id` | fixture preflight plus F2 construction proof | boundary/sequence golden and same-capture proof | version it; product proof uses `length || canonical_id`; old raw sequence may remain an external benchmark oracle |
| whole-source BLAKE3 inside `ConstructionState` | private F2 transaction-local proof | exact source equality during capture | remove only if ordered canonical commitment plus put evidence formally supplies the needed authority |

**Observed:** fixture preparation independently computes raw source and CDC
sequence fingerprints before measured rows (`accepted source:4341-4445,
8350-8775`). Those values are retained in manifests and reports even if the
product witness changes.

**Observed:** the canonical mapping specification requires raw/canonical IDs
but does not make a whole-source digest a serialized mapping identity
(`implementation-detail/phase-4/algorithm/spec.md:309-335,440-450`).

**Hypothesis:** a domain-separated transcript of ordered `(raw_length,
canonical_object_id)` plus exact transaction-local put evidence commits to the
same source bytes under BLAKE3 collision resistance:

1. canonical ID authenticates framing, declared raw length, and raw payload;
2. explicit length prevents concatenation ambiguity and routes ranges;
3. ordered framing detects omission, duplication, and reordering;
4. proof count/total/topology/root checks bind the transcript to the published
   graph.

This is not yet a proof. A future authority review must show that no failure
currently rejected by the whole-source digest becomes publishable.

## 3. Security properties gained and lost

### Gained or simplified

- One typed canonical chunk identity replaces an alias with two semantic
  meanings.
- A mapping cannot contain mutually inconsistent raw and canonical IDs.
- Complete canonical authentication already covers raw bytes and their framing;
  the second raw hash no longer repeats payload integrity work.
- An explicit ordered-commitment domain prevents the current unkeyed transcript
  from being confused with another BLAKE3 use.
- Mapping leaves and in-memory reference/rejoin state become smaller.

### Lost or changed

- Raw payload no longer has an identity independent of Phase-1 framing. This
  couples CDC/content APIs to the canonical Bytes-object contract.
- The independent cross-domain consistency check disappears. Security remains
  256-bit canonical-object collision resistance, but redundancy is reduced.
- `ChunkIdentityMismatch` can no longer mean “canonical object is authentic but
  its separate raw ID field is wrong” in v2 because that field does not exist.
  V1 readers must retain the old error.
- A wrong v2 canonical object reference is either detected as canonical
  `IdentityMismatch`, a separately checked length/role error, or represents a
  different valid file/root. Expected-root and receipt checks—not a second raw
  hash—distinguish the latter.

Noncryptographic hashes, truncated IDs, locator trust, or deriving the
canonical ID from an unauthenticated raw digest are not acceptable substitutes.

## 4. Honest row-by-row performance ceiling

The following is recomputed from the five measured rows in sealed
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/f4a.raw.jsonl`.
`construction` is the combined whole-source plus ordered-sequence interval.

| Row | Durable ms | Raw-ID ms | Construction ms | Durable - raw | Durable - construction | Durable - both |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 636.837 | 94.982 | 88.938 | 541.854 | 547.899 | 452.916 |
| 2 | 628.367 | 94.345 | 88.664 | 534.022 | 539.703 | 445.357 |
| 3 | 639.102 | 95.185 | 89.067 | 543.916 | 550.034 | 454.849 |
| 4 | 637.609 | 95.356 | 89.379 | 542.253 | 548.230 | 452.873 |
| 5 | 620.337 | 98.851 | 94.402 | 521.486 | 525.935 | 427.084 |
| **median** | **636.837** | **95.185** | **89.067** | **541.854** | **547.899** | **452.873** |

These are optimistic subtraction bounds, not forecasts:

- raw-ID work can plausibly disappear because the canonical ID is already
  computed; replacement bookkeeping still remains;
- only the whole-source sublane of `construction` is a removal candidate;
  ordered commitment work remains and its isolated wall is unavailable;
- a new format also changes mapping bytes, SQLite page layout, and edit paths,
  none of which is credited in the table;
- F4-A is an observer-heavy diagnostic, not a retained performance checkpoint.

Raw-ID deletion alone does not reach 500 ms in any row. The combined design is
the only variant whose measured upper bound clears 500 ms in all five rows.

## 5. Mapping and resource effect

For unchanged K64/F64 topology:

```text
v1 reference bytes                         68
v2 reference bytes                         36
bytes removed per occurrence               32
retained occurrences                    5,284
exact serialized mapping bytes removed 169,088
retained mapping bytes                   365,262
derived v2 mapping bytes                 196,174
retained total canonical bytes       105,291,554
derived v2 canonical bytes           105,122,466
```

A full 64-reference leaf falls from 4,380 to 2,332 canonical bytes. Object
count and topology remain 83 leaves, two branches, one file root, workspace,
and transition when K/F remain fixed. File/reference Q also falls by 32 bytes
per live slot, including the K64 frontier and bounded rejoin arrays. These are
logical/serialized byte equations, not physical-I/O or wall claims.

## 6. Hot and cold materialization consequences

- **Full create:** one 100-MiB raw-ID hash disappears; canonical encoding/hash,
  CDC scan, SQLite writes, and durability remain.
- **Same-count and count-changing edits:** rejoin can compare canonical IDs from
  old references and compute one canonical ID for each newly scanned chunk.
  The chosen ID must be handed to storage rather than recomputed. Fixed-ordinal
  suffix behavior is unchanged.
- **CAS reuse:** canonical ID already selects the incumbent and complete
  canonical bytes are authenticated/compared. No raw hash is needed for
  immutable equality.
- **Cold scrub/reconstruction:** complete canonical authentication remains;
  the subsequent raw-payload hash disappears. Length and Bytes role remain
  explicit checks.
- **Ranges/hot materialization:** every selected complete chunk still requires
  canonical authentication under the current whole-object ID. The second raw
  hash disappears, but v2 does not authorize unauthenticated partial reads or
  turn a receipt into byte authority.

## 7. Compatibility and migration blast radius

Affected surfaces include:

- `ChunkId`/`InMemoryCas` and `LogicalFile` core APIs;
- file-reference codec/version, profile ID, leaf goldens, K/F sizing, Q
  equations, raw-hash counters, and exact typed-error tests;
- full-create proof and qualification fields;
- same-count and count-changing rejoin state;
- scrub, reconstruction, ranges, changed-spine verification, and closure
  counters;
- workspace roots, transitions, deltas, receipts, visible heads, fixtures,
  manifests, and every root/transition/closure golden;
- Memory/SQLite parity and any production migration.

### Is dual-read/new-write conceivable?

**Yes, but not under the current single-profile open contract without new
authority.** V1 references already contain the canonical `object_id`, so a
compatibility reader can normalize either format to `(raw_length,
canonical_object_id)`. Existing canonical chunk BLOBs can be reused without
rewriting their bytes.

A conceivable transition is:

1. reader dispatches on authenticated mapping version/profile and retains v1
   raw-ID validation for old objects;
2. active writer emits only v2 compact references and v2 ordered commitment;
3. an edit may read a v1 parent and publish a v2 child, reusing canonical chunk
   objects;
4. transition/delta explicitly permits a v1 parent root and v2 child root;
5. the new receipt binds the active v2 profile while compatibility authority
   records how historical v1 roots are validated;
6. old objects remain reachable until retention/GC policy permits removal.

Current store metadata and receipts bind one exact `mapping_profile_id`
(`implementation-detail/phase-4/mapping/logical-persistence.md:881-903`;
`crates/layerfs-core/src/validation.rs:43-45,115-129`). A simple profile-ID flip
would make historical parents unreadable or cause mixed semantics.
Dual-read/new-write therefore needs an explicit compatibility set,
cross-profile delta rules, downgrade rejection, and migration tests. A full
eager rewrite is simpler semantically but rewrites all mapping/root/history
objects and forfeits lazy reuse.

Sealed v1 evidence must remain historical. Raw fixture fingerprints can remain
common custody values; v1 raw-ID CDC sequence and v2 canonical-ID sequence are
different named/versioned observations and may not be silently compared.

## 8. Primary design precedents

- Git uses one framed object hash (`blob <size>\0 || bytes`) for addressing and
  integrity; see the official
  [Git object-storage documentation](https://git-scm.com/docs/user-manual#_object_storage_format).
- The official
  [Xet hashing specification](https://github.com/huggingface/hub-docs/blob/main/docs/xet/hashing.md)
  computes one hash per chunk and derives file hashes from ordered chunk
  hashes. Xet also frames lengths in its internal-node commitment.
- The [BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs)
  provides a 256-bit cryptographic tree hash and explicit keyed/derive-key
  domain modes.
- [RFC 9162](https://www.rfc-editor.org/rfc/rfc9162.html) demonstrates explicit
  leaf/node domain separation and ordered Merkle commitments.

These are precedents for framing and composition. They are not evidence that
LayerFS migration, error semantics, or performance will pass.

## 9. Ranked direction variants

### 1. Witness-only ordered canonical commitment

- **Change:** keep the v1 three-field mapping, but isolate and, if justified,
  replace the private whole-source hash with a domain-separated transcript over
  `(length, canonical ObjectId)`.
- **Upside:** source-only portion of the 89.067-ms combined lane; exact wall
  unknown. No canonical mapping/root change is required.
- **Risk:** lowest, but the transaction proof must still catch every omission,
  reorder, duplicate, wrong ID/length, stale evidence, and wrong source claim.
- **Decisive question:** is the raw source digest product authority or only a
  benchmark oracle, and how many isolated milliseconds belong to it?

### 2. Compact v2 single identity plus ordered commitment — recommended design target

- **Change:** 36-byte `(length, canonical ID)` references and no inner
  whole-source byte digest; external fixture hash retained.
- **Upside:** raw-ID pass plus source-only construction sublane; 169,088 fewer
  mapping bytes; row-wise optimistic median 452.873 ms; fewer raw hashes during
  materialization.
- **Risk:** largest compatibility blast radius and new proof/error semantics.
- **Decisive question:** can a shadow model prove exact full-create, rejoin,
  edit, reconstruction, range, delta, receipt, and adversarial equivalence
  before any persisted v2 byte is written?

### 3. Same-width v2 bridge

- **Change:** retain the 68-byte leaf layout but define both 32-byte slots as
  the same canonical ID under mapping version/profile v2. This removes raw
  hashing while preserving offsets, K/F, and storage size.
- **Upside:** isolates the identity/hash effect with less codec/topology churn;
  dual reader is simpler.
- **Risk:** still a new incompatible semantic version, stores 32 redundant
  bytes per reference, and creates an extra bridge format to migrate later.
- **Disposition:** useful as a nonpersistent simulator or benchmark control;
  usually not worth promoting as a durable format.

### 4. Compact v2 while retaining the whole-source digest

- **Change:** remove raw ID only; keep exact whole-source qualification.
- **Upside:** approximately 95-ms gross raw-hash lane plus 169,088 mapping
  bytes; derived durable rows remain 521.486-543.916 ms.
- **Risk:** pays the full format/migration cost without reaching the target in
  the observed upper-bound arithmetic.
- **Disposition:** fallback only if authority proves the source digest
  independently mandatory.

## 10. Questions for future specialists

1. What exact production property does the inner whole-source digest establish
   beyond authenticated ordered canonical IDs, lengths, count, total, root,
   and transaction-local put evidence?
2. Can whole-source and ordered-sequence timer sublanes be isolated without
   changing their work, so removable milliseconds are measured rather than
   inferred?
3. Can every current `raw_id` consumer use canonical ID without fetching an old
   payload, especially two-chunk bounded rejoin across a v1 parent?
4. What is the exact v2 error mapping for wrong length, wrong role, canonical
   identity mismatch, wrong expected root, and legacy raw-ID mismatch?
5. Can a dual reader reject downgrade/profile confusion while one writer emits
   v2 and deltas cross a v1-parent/v2-child boundary?
6. Does v2 preserve same-count edit latency and reduce—rather than merely move—
   raw hashing in count-changing edits, scrub, reconstruction, and ranges?
7. Do the full end-to-end row and CPU improve enough after mandatory ordered
   commitment replacement, or was the combined construction ceiling mostly
   required work?

## Recommendation

Proceed conceptually in two steps, without writing a durable format yet:

1. isolate whole-source versus ordered-sequence proof cost and settle the
   benchmark-custody/product-authority distinction;
2. build a nonpersistent shadow model that normalizes v1 and proposed v2
   references to `(length, canonical ObjectId)` across full create, both edit
   classes, materialization, deltas, receipts, errors, and migration.

If both questions close, compact v2 is the highest-upside core redesign in the
current evidence. If either fails, retain canonical/raw dual identity and move
to lower-ceiling CDC or storage work. Do not implement a persistent bridge,
rewrite history, or relabel v1 evidence merely to obtain a favorable benchmark.
