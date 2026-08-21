# Compression and packing for the local LayerFS path

Status: **directional research; no implementation or promotion decision**
Date: 2026-08-20
Scope: local full-create, reopen, scrub, reconstruction, and range behavior. A
distributed protocol is out of scope.

## Executive direction

**Recommendation: reject foreground compression and Git-style delta packing as
the next optimization for the accepted 100-MiB K64/F64 full-create path.** The
retained source is already poorly compressible after CDC. The most favorable
locally relevant screen—adaptive, independent zstd level-1 encoding of canonical
chunk objects—would save only 4,529,478 bytes, or 4.32% of those objects and an
ideal 4.15% of the current database, while its exploratory encode pass took
about 148 ms. Even a deliberately generous linear scaling of the measured
database-write lanes assigns only about 3.0 ms of possible wall saving to those
bytes. Compression therefore adds a large foreground CPU stage to attack a
small fraction of the current cost.

Git's useful lesson is structural, not “compress everything.” Git computes an
object identity over the uncompressed logical object and places zlib/pack/delta
representations below that identity. LayerFS should preserve the same
separation if it ever adds a physical codec. Git's pack machinery also carries
base dependencies, indexes, repacking policy, and decode-depth costs; it is not
a free local-write acceleration mechanism.

The one direction worth **deferring**, rather than rejecting permanently, is an
adaptive per-object codec for a materially different, representative corpus
that proves a much larger ratio. Whole-store compression can remain an offline
export/archive idea. Neither belongs on the current full-create critical path.

## Evidence language and boundaries

- **Observed** means directly read from repository code or sealed evidence, or
  measured in the read-only exploratory screens described below.
- **Derived** means arithmetic from observed values; the equation is given.
- **Hypothesis** means a mechanism that still needs a prospective experiment.
- The compression screens are one warm-or-unknown-cache directional pass, not
  an acceptance campaign. Their CPU numbers are not production predictions.
- No source, database, fixture, implementation, or retained evidence was
  modified. Temporary compression and Git repositories were discarded.

## 1. Actual LayerFS representation comes first

### 1.1 Canonical bytes and identity

**Observed.** A canonical object begins with `LFSO` and a 9-byte outer header.
A `Bytes` object then writes a four-byte big-endian value length followed by the
value, so a raw chunk's canonical representation has 13 framing bytes
([`crates/layerfs-core/src/object/codec.rs:11`](../../../crates/layerfs-core/src/object/codec.rs#L11),
[`codec.rs:22`](../../../crates/layerfs-core/src/object/codec.rs#L22),
[`codec.rs:30`](../../../crates/layerfs-core/src/object/codec.rs#L30), and
[`codec.rs:46`](../../../crates/layerfs-core/src/object/codec.rs#L46)).

**Observed.** `ObjectId` is BLAKE3 over `layerfs/object\0` followed by the
complete canonical bytes
([`crates/layerfs-core/src/identity/digest.rs:5`](../../../crates/layerfs-core/src/identity/digest.rs#L5)
and [`digest.rs:39`](../../../crates/layerfs-core/src/identity/digest.rs#L39)).
Compression is therefore not part of current canonical identity.

**Observed.** Mapping objects start with `LFS4MAP\0`, a version, and a tag, then
are wrapped as canonical `Bytes` objects
([`crates/layerfs-core/src/content/persistence.rs:11`](../../../crates/layerfs-core/src/content/persistence.rs#L11),
[`persistence.rs:95`](../../../crates/layerfs-core/src/content/persistence.rs#L95),
and [`persistence.rs:111`](../../../crates/layerfs-core/src/content/persistence.rs#L111)).
Each file reference is already a dense fixed 68 bytes: raw ID, raw length, and
canonical object ID ([`persistence.rs:20`](../../../crates/layerfs-core/src/content/persistence.rs#L20)
and [`persistence.rs:38`](../../../crates/layerfs-core/src/content/persistence.rs#L38)).

**Observed.** The accepted F2/F4 SQLite path stores `object_id`, kind, canonical
length, and the uncompressed canonical BLOB
([`phase4_create_edit_benchmark-f2-accepted.rs:1900`](../../../target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs#L1900)).
Insertion binds the canonical bytes directly
([`phase4_create_edit_benchmark-f2-accepted.rs:2259`](../../../target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs#L2259)).

**Required invariant.** A physical codec must remain below the existing
canonical identity:

```text
canonical bytes -> ObjectId
canonical bytes -> optional physical encoding -> durable carrier
durable carrier -> bounded decode -> canonical bytes -> authenticate ObjectId
```

Changing `ObjectId` to hash compressed bytes would make codec version, level,
dictionary, and encoder implementation identity-significant. That is a format
change, not a storage optimization. It would also fracture dedup across
different encodings of the same canonical object. Do not do it without an
explicitly versioned identity migration.

### 1.2 Retained workload and accepted cost

**Observed.** The exact source is
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/S1-100.source`:
104,857,600 bytes, SHA-256
`63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`.
Its frozen K64/F64 boundaries contain 5,284 chunks and have the accepted
sequence fingerprint
`5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994`.

**Observed.** F4 preserves 5,372 objects, 5,284 references, 105,291,554 total
canonical bytes, and 365,262 mapping bytes
([`implementation-detail/phase-4/wp4m/f-series/f4/report.md:187`](../../../implementation-detail/phase-4/wp4m/f-series/f4/report.md#L187)).
The chunk-object portion is exact:

```text
104,857,600 source bytes + 5,284 * 13 framing bytes
= 104,926,292 canonical chunk-object bytes

104,926,292 chunk-object bytes + 365,262 mapping-object bytes
= 105,291,554 total canonical bytes
```

**Observed.** The retained finalized SQLite database is 109,268,992 bytes.
**Derived:** its noncanonical/page overhead is
`109,268,992 - 105,291,554 = 3,977,438 bytes`, or 3.78% of canonical bytes.

**Observed.** Accepted medians are 524.438 ms mapping/construction, 112.324 ms
publication/COMMIT on the durable median row, and 636.837 ms durable create.
Fresh scrub is 280.251 ms and reconstruction is 438.070 ms
([`f4/report.md:220`](../../../implementation-detail/phase-4/wp4m/f-series/f4/report.md#L220)
through [`f4/report.md:255`](../../../implementation-detail/phase-4/wp4m/f-series/f4/report.md#L255)).
The remaining 500-ms target gap is 136.837 ms, or 21.49%.

**Observed.** The isolated component medians most relevant to storage bytes are
24.282 ms for mapping direct VFS, 48.194 ms for main-database writes, 42.818 ms
for the main-database FULL sync, and 93.031 ms for all direct COMMIT VFS
([`f4/report.md:291`](../../../implementation-detail/phase-4/wp4m/f-series/f4/report.md#L291)
and [`f4/report.md:322`](../../../implementation-detail/phase-4/wp4m/f-series/f4/report.md#L322)).
These are diagnostic medians from different component distributions and are not
an additive same-row decomposition.

## 2. Read-only exploratory screens

### 2.1 Method and tool custody

**Observed.** The local tools were Apple `gzip 479`, Git 2.47.1, zstd CLI and
library 1.5.7, and Python's zlib runtime 1.2.12. CLI timing used
`/usr/bin/time -p`; per-object zstd called the official 1.5.7 library through a
small Python `ctypes` driver. Chunk ranges came from the retained boundary TSV,
and each canonical chunk was reconstructed as the exact 13-byte LayerFS framing
plus its retained source slice. Mapping objects were read from a copy-free,
read-only SQLite query over the retained finalized database.

**Limitation.** Each cell below is a single screen. The CLI timer resolves only
to 0.01 seconds; Python loop/FFI overhead is included; caches are warm or
unknown; physical-device bytes and energy are unavailable. These numbers rank
ideas. They cannot qualify an implementation.

### 2.2 Whole raw source

| Codec | Output bytes | Output/input | Encode wall | Decode wall | Classification |
|---|---:|---:|---:|---:|---|
| gzip level 1 | 100,394,343 | 95.743% | 2.11 s | 0.04 s | Observed |
| gzip level 6 | 100,371,498 | 95.722% | 2.17 s | 0.04 s | Observed |
| gzip level 9 | 100,371,498 | 95.722% | 2.19 s | 0.04 s | Observed |
| zstd level 1, one thread | 100,369,102 | 95.719% | 0.06 s | 0.02 s | Observed |
| zstd level 3, one thread | 100,346,065 | 95.697% | 0.10 s | 0.02 s | Observed |
| zstd level 9, one thread | 100,312,155 | 95.665% | 0.11 s | 0.02 s | Observed |

**Derived.** Whole-stream zstd saves only 4.28–4.33% of this source. This is a
ratio upper bound for a single continuous frame, not a usable object layout:
ordinary zstd frames are sequential streams, and the base format does not
provide object-level random access. Adding seek tables is another physical
format and index.

### 2.3 Independent canonical chunks

| Codec | Level | Total bytes | Ratio to 104,926,292 | Encode wall | Decode wall |
|---|---:|---:|---:|---:|---:|
| zstd | 1 | 100,444,024 | 95.728% | 147.8 ms | 93.7 ms |
| zstd | 3 | 100,434,310 | 95.719% | 172.0 ms | 93.4 ms |
| zstd | 9 | 100,434,058 | 95.719% | 224.1 ms | 90.9 ms |
| zlib | 1 | 100,508,052 | 95.789% | 1.213 s | 57 ms |
| zlib | 6 | 100,489,990 | 95.772% | 1.300 s | 58 ms |

All cells are **Observed** exploratory screens. The apparently lower zlib
decode wall does not rescue it: its encode wall is roughly twice the complete
accepted durable create, and the one-shot timing methodology is not a decoder
comparison campaign.

The distribution is more informative than the aggregate:

- **Observed:** at zstd level 1 only 563 of 5,284 chunks became smaller.
- **Observed:** the median object ratio was 1.000521; most objects expanded by
  exactly ten bytes.
- **Observed:** always compressing saved 4,482,268 bytes, while adaptively
  storing compressed bytes only when smaller saved 4,529,478 bytes. Avoiding
  expanded encodings recovered 47,210 bytes.
- **Derived:** the best adaptive payload ratio is
  `(104,926,292 - 4,529,478) / 104,926,292 = 95.6832%`—a 4.3168% saving.
- **Derived:** before adding codec tags, compressed lengths, checksums, SQLite
  page effects, or alignment, this is only
  `4,529,478 / 109,268,992 = 4.1453%` of the current database.

This workload is bimodal: a small compressible minority provides almost all
savings and the majority should remain raw. Any future codec must therefore be
adaptive; mandatory per-object compression is an anti-recommendation.

### 2.4 Mapping objects

| Representation | Output bytes | Ratio | Result |
|---|---:|---:|---|
| independent zstd level 1 | 365,894 | 100.173% | expands 632 bytes |
| independent zstd level 3 | 365,871 | 100.167% | expands 609 bytes |
| independent zstd level 9 | 365,849 | 100.161% | expands 587 bytes |
| independent zlib level 1 | 366,021 | 100.208% | expands 759 bytes |
| one continuous zlib stream | 361,889 | 99.077% | saves only 0.923% |

All cells are **Observed**. Independent mapping compression loses. The tiny
continuous-stream saving would exchange direct mapping-object access for a
shared decode stream, yet remove just 3,373 bytes. **Reject it.** Dense IDs and
fixed-width lengths are already near incompressible on this fixture.

### 2.5 Dictionary and cross-object delta attempts

**Observed.** A 65,536-byte zstd dictionary trained on the first 1,000
canonical chunks (19,721,605 training bytes) took 475.7 ms to train. On the
held-out 4,284 chunks (85,204,687 canonical bytes), plain zstd level 1 produced
81,567,332 bytes in 113.2 ms. Dictionary compression produced 81,566,484 bytes
but required 263.1 ms; after retaining the dictionary, total storage was
81,632,020 bytes—worse than no dictionary. Decode took 102.5 ms. **Reject a
dictionary for this fixture.** It adds version/custody/recovery state while
recovering only 848 payload bytes before its own 65,536 bytes.

**Observed.** A temporary Git repository imported all 5,284 exact canonical
chunks as blobs, plus one tree and commit. `git pack-objects` used one thread
and zlib level 6:

| Pack mode | Pack bytes | Ratio to canonical chunks | Encode wall |
|---|---:|---:|---:|
| no deltas, window/depth zero | 100,630,807 | 95.906% | 0.34 s |
| window 10, depth 50 | 100,631,651 | 95.907% | 0.34 s |

The delta pack was 844 bytes larger. `verify-pack` found only eight blob deltas,
all at depth one. **Observed:** decode CPU was unavailable. **Conclusion:** this
fixture contains essentially no profitable cross-chunk delta locality after
CDC; Git's search/dependency machinery cannot manufacture similarity.

### 2.6 Whole finalized SQLite file

**Observed.** Whole-file zstd level 1 reduced the 109,268,992-byte finalized
database to 101,609,898 bytes (92.991%, 7.009% saved) in 0.14 seconds. gzip
level 1 produced 101,634,726 bytes in 2.22 seconds.

This is an **archival upper bound**, not a product-path candidate. It compresses
SQLite page slack and repeated structures unavailable to independent object
compression, but destroys normal SQLite page access. Using it live would
require a second durable artifact or a wholly different page/storage layer,
with new recovery, sync, and atomic-publication semantics. It must not be
reported as a 7% live-store win.

## 3. What Git actually contributes

### 3.1 Identity/representation separation: adopt the principle

**Observed from the official loose-object format.** Git names an object from
its uncompressed `type + size + NUL + content`, then zlib-compresses that full
logical object for loose storage. The object name is not a digest of the zlib
stream ([Git loose-object format](https://git-scm.com/docs/gitformat-loose.html)).

**Applicable local lesson.** LayerFS can retain its existing canonical
`ObjectId` while choosing raw or compressed physical bytes per object. This
preserves cross-codec dedup, deterministic identity, and the ability to replace
a physical encoding without changing logical roots.

### 3.2 Pack/delta machinery: understand, do not cargo-cult

**Observed from the official pack format.** A Git pack may store a complete
deflated object or a delta whose base is identified by an offset or object ID;
the delta is a sequence of copy/insert instructions. The pack has a checksum
and a separate index for object lookup
([Git pack format](https://git-scm.com/docs/gitformat-pack)).

**Observed from official implementation documentation.** `git pack-objects`
searches candidate bases in a window, limits dependency depth, exposes
compression level and big-file behavior, and can create thin packs whose
missing bases must later be fixed
([`git-pack-objects`](https://git-scm.com/docs/git-pack-objects) and
[Git pack heuristics](https://git-scm.com/docs/pack-heuristics)). These knobs
exist because packing trades write/search CPU and dependency complexity for
space and transfer savings.

**Derived local implication.** A delta object is not independently
materializable. A range read may need its base and a bounded chain; scrub must
authenticate reconstructed canonical bytes; deleting or compacting a base is
constrained by descendants; recovery needs an authenticated index or scan. The
current retained fixture's eight shallow, net-negative deltas provide no
benefit to pay for those costs.

**Hypothesis.** Cross-version cold objects in a real history may be much more
similar than adjacent CDC chunks from one new random-like file. That is a later
offline-compaction question, not evidence for compressing the fresh full-create
transaction.

## 4. Local performance and Amdahl ceiling

### 4.1 Foreground create

**Derived, intentionally generous upper bound.** Suppose the adaptive 4.1453%
database reduction translated linearly into both the 24.282-ms mapping VFS and
48.194-ms COMMIT database-write medians:

```text
(24.281657 + 48.194103) ms * 4,529,478 / 109,268,992
= 3.004 ms
```

That bound is already generous: component medians are not additive same-row
work, SQLite page writes are discrete, the physical encoding needs metadata,
and the 42.818-ms FULL sync is a durability fence rather than byte-proportional
copy work. Against it, the per-object zstd-1 screen spent 147.8 ms encoding.

**Derived:** 147.8 ms is 23.21% of the accepted 636.837-ms durable create. Even
the whole-stream zstd CLI's coarse 60-ms screen is roughly 9.42% and sacrifices
object independence. Compression cannot close the 136.837-ms target gap on
this fixture; its measured direction is net negative.

**Algorithmic classification.** Compression is still O(source bytes) and adds
another transform pass. It may reduce a constant in downstream physical bytes,
but it does not reduce the 5,284-object cardinality, identity hashes, SQLite
statements, pager bookkeeping, transaction count, or durability-fence count.
Cross-object delta search adds candidate comparisons and dependency management;
it increases algorithmic work unless performed out of band.

### 4.2 Read, scrub, reconstruction, and ranges

**Derived warning, not a production forecast.** A 93.7-ms full per-object zstd
decode screen is 33.43% of the 280.251-ms fresh scrub and 21.39% of the
438.070-ms reconstruction median. The operations would read at most about 4.5
MiB fewer chunk payload bytes but must decode and still authenticate all
canonical bytes. A warm page cache makes that trade even less attractive.

**Range behavior.** Independent chunk frames preserve chunk-granular lookup but
force full-chunk decompression for any byte within a chunk. Whole-file or
whole-pack streams lose that property unless a seek/index layer and bounded
frames are added. Delta chains further turn one logical read into base reads and
reconstruction. Hot objects would repeatedly pay decode CPU unless cached;
caching decompressed bytes raises memory and cache-coherence/accounting costs.

**Cold behavior.** Compression is most plausible for cold objects where space
or physical-read bandwidth dominates and latency is amortized. That condition
was not observed in the accepted warm-or-unknown local fixture. Cold-tier
classification, background repack, and eviction are new policies, not a small
F-series speed optimization.

## 5. Dedup, durability, indexes, and compaction

### Dedup interaction

- **Observed:** LayerFS dedup is keyed by canonical `ObjectId`; keeping codecs
  below it retains current dedup semantics.
- **Derived:** compression does not improve the logical dedup ratio. It only
  shrinks bytes that remain after dedup.
- **Risk:** hashing physical encodings, or including encoder choices in current
  IDs, would create multiple IDs for the same canonical content.
- **Risk:** delta compression couples independently deduplicated objects through
  base dependencies. Reuse of the target ID no longer proves its base remains
  locally materializable.

### Durability bytes

- **Hypothesis:** smaller independent payloads can reduce data/page bytes sent to
  the pager on compressible data.
- **Observed counterweight:** the current candidate still uses one FULL/DELETE
  transaction and one publication COMMIT; compression does not remove the sync
  fence, journal protocol, or visible-head publication.
- **Risk:** writing raw bytes and later replacing them with compressed bytes
  doubles write amplification and adds a second publication/recovery problem.
  Foreground adaptive encoding must choose the representation before the one
  authoritative write, or defer the whole feature to separately specified
  offline compaction.
- **Unavailable:** actual host physical I/O-byte reduction and SSD effects. Do
  not substitute SQLite requested bytes, apparent length, or allocation for
  physical I/O.

### Index and format consequences

An independent physical encoding minimally needs codec, encoded length, and
canonical length. The current schema already stores canonical length, but a
codec/tag or alternate BLOB convention would be a schema/format change. Every
decode must apply strict encoded/canonical bounds, detect truncation/trailing
bytes, and then authenticate the reconstructed canonical bytes before use.

A pack adds at least object-to-offset lookup, lengths/type, checksum coverage,
and crash-safe catalog publication. Cross-object deltas additionally need base
selection, maximum chain depth, cycle/impossible-base rejection, reachability
across bases, and a compactor that never drops a required base. None is free on
the accepted SQLite path.

The old append-only prototype is a warning: although carrier overhead was only
1.4018%, 5,363 lookups caused 55,240 index-page reads, and reopen read
427,887,475 bytes for a 106,327,544-byte carrier
([`first-implementation-findings.md:63`](../../../implementation-detail/phase-4/storage/append-only/first-implementation-findings.md#L63),
[`first-implementation-findings.md:161`](../../../implementation-detail/phase-4/storage/append-only/first-implementation-findings.md#L161),
and [`first-implementation-findings.md:183`](../../../implementation-detail/phase-4/storage/append-only/first-implementation-findings.md#L183)).
A compact carrier did not imply a fast index/reopen path.

The earlier packed in-memory CAS also failed to prove speed: its initial median
was 180.462 ms versus 169.912 ms, a corrected shared run was 241.743 ms versus
230.739 ms, and pre-sized clean lanes were only 0.09–0.94% faster—parity below
the 5% gate
([`implementation-detail/phase-2/opt-2-packed-cas.md:534`](../../../implementation-detail/phase-2/opt-2-packed-cas.md#L534)
through [`opt-2-packed-cas.md:595`](../../../implementation-detail/phase-2/opt-2-packed-cas.md#L595)).
Packing and compression must each earn their own cost; combining them would hide
attribution.

## 6. Ranked directions

| Rank | Direction | Disposition | Expected local effect | Maximum plausible current-fixture upside | Primary risk |
|---:|---|---|---|---|---|
| 1 | Leave accepted objects uncompressed and optimize measured hashing/SQLite lanes | **Recommend** | Avoids a new O(n) pass; attacks larger observed lanes | Compression-specific upside is zero; broader work owns the 136.8-ms gap | None from a new codec |
| 2 | Adaptive independent zstd for a future strongly compressible corpus | **Defer** | Preserves object access; raw fallback avoids expansion | Current fixture: only 4.15% DB bytes and a generous ~3.0-ms write-wall ceiling | Encode/decode CPU, schema, bounds, codec compatibility |
| 3 | Mandatory zstd per object | **Reject** | Shrinks only 563/5,284 chunks here | Less than adaptive; most objects expand | CPU and systematic space expansion |
| 4 | Foreground zlib/gzip | **Reject** | Similar ~4.2% ratio | Cannot recover 1.2–2.2 s encode wall | Gross create regression |
| 5 | Compress mapping objects | **Reject** | Independent objects expand | None | Work and format for negative space gain |
| 6 | Shared zstd dictionary | **Reject for retained fixture** | Saves 848 held-out payload bytes before dictionary | Net storage loss | Dictionary identity/custody/recovery and doubled encode wall |
| 7 | Git-style cross-object deltas in fresh create | **Reject** | Eight shallow deltas; pack became 844 bytes larger | None observed | Base chains, index/repack, range/read amplification |
| 8 | Cold, offline pack/delta compaction across versions | **Defer as separate storage research** | Could exploit historical similarity outside commit wall | Unavailable on current one-version fixture | New format, GC, recovery, background resource policy |
| 9 | Whole-stream or whole-SQLite live compression | **Reject** | 4.33% source or 7.01% archival DB saving | Archival only | Destroys page/object random access and atomic store semantics |

The strongest conventional option is therefore **no compression on this path**.
The strongest disruptive option is a separate cold-store compactor, but it is
not justified by current evidence and is orthogonal to the local full-create
target.

## 7. Non-negotiable gates for any later codec experiment

Before implementation, prospectively require all of the following:

1. The same canonical bytes, raw IDs, `ObjectId`s, root, transition, closure,
   CDC boundaries/sequence, exact errors, and visible-head semantics.
2. Codec selection below canonical identity; a raw representation is always
   legal and produces the same logical object.
3. Strict encoded-size and canonical-size limits before allocation; checked
   arithmetic; bounded streaming decode; rejection of truncation, trailing
   data, dictionary mismatch, decompression bombs, and output-length mismatch.
4. Authentication over the fully reconstructed canonical bytes before trust or
   reuse; no codec checksum substituted for LayerFS identity.
5. One caller-thread transaction, one COMMIT, FULL/DELETE durability, no second
   database/artifact, and no post-COMMIT rewrite on the foreground lane.
6. Exact Q/RSS accounting including compressor/decompressor context, buffers,
   dictionary, encoded bytes, decoded bytes, and any simultaneous raw fallback.
7. Exact materialization/range/scrub behavior; a bounded maximum delta depth of
   zero for the first independent-codec experiment.
8. A versioned physical-format reader/writer and crash/reopen/corruption proof.
   No silent codec-autodetection by trying decoders.
9. Same-fixture adjacent control/candidate evidence. Report CPU, wall, logical,
   apparent, allocated, and supported physical-I/O observations separately.

**Prospective success gate for reconsideration:** after existing dedup on a
representative production corpus, adaptive independent compression must reduce
physical store bytes by at least 20% while keeping encode overhead within 5% of
protected durable create and decode overhead within 5% of each protected read,
scrub, reconstruction, and range operation. It must not regress durability,
exact Q, or one-COMMIT behavior.

**Kill gate:** if either byte saving is below 20% or any protected wall/CPU/Q
gate exceeds 5%, stop. Do not compensate by adding dictionaries, delta chains,
workers, caches, or background compaction in the same experiment.

The 20% threshold is a **Hypothesis-driven screening bar**, not a measured
LayerFS requirement. It is intentionally well above the current 4.15% ideal
because a codec must repay its format, CPU, read, and maintenance costs.

## 8. Single decisive question for a future specialist

> After canonical dedup on a representative, versioned production corpus—not
> this one retained synthetic fixture—does adaptive, independent zstd level 1
> save at least 20% of durable physical bytes while adding no more than 5% to
> caller-thread create, hot/cold materialization, scrub, reconstruction, range,
> Q, and RSS?

If no, reject compression for active LayerFS storage. If yes, test only the
minimal below-identity raw-or-zstd object representation first. Do not begin
with packs, dictionaries, deltas, a cache, or a compactor.

## 9. Primary technical sources

- Git, [loose object format](https://git-scm.com/docs/gitformat-loose.html).
- Git, [pack format](https://git-scm.com/docs/gitformat-pack).
- Git, [`git-pack-objects`](https://git-scm.com/docs/git-pack-objects) and
  [pack heuristics](https://git-scm.com/docs/pack-heuristics).
- zlib, [official manual](https://www.zlib.net/manual.html); IETF,
  [RFC 1950](https://www.rfc-editor.org/rfc/rfc1950) and
  [RFC 1951](https://www.rfc-editor.org/rfc/rfc1951).
- Facebook/Meta, [official zstd API manual](https://github.com/facebook/zstd/blob/dev/doc/zstd_manual.html)
  and [dictionary API](https://github.com/facebook/zstd/blob/dev/lib/zdict.h);
  IETF, [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.html).
- Korn and Vo, [“Engineering a Differencing and Compression Data Format”](https://www.usenix.org/legacy/event/usenix02/full_papers/korn/korn_html/),
  USENIX 2002; IETF, [RFC 3284 VCDIFF](https://www.rfc-editor.org/rfc/rfc3284).

## Final disposition

**Observed:** current LayerFS canonical chunks and mapping objects do not offer
enough compressibility to justify foreground coding; Git-style cross-object
deltas were net negative; prior packed-CAS and append-only work shows compact
layout alone does not guarantee a faster complete path.

**Derived:** adaptive per-chunk zstd's ideal current-store saving is 4.15%, with
a generous byte-linear write-wall ceiling near 3.0 ms against an exploratory
147.8-ms encode pass. It cannot make a material contribution to the 136.8-ms
target gap.

**Hypothesis:** a future real corpus with at least 20% post-dedup compressibility
could justify a raw-or-zstd physical representation below canonical identity.
That question should be screened before any implementation.

**Direction:** recommend no compression/packing change for the active local
full-write optimization; defer independent adaptive zstd to corpus
qualification; reject foreground zlib, mandatory compression, mapping
compression, dictionaries, delta packs, and whole-store live compression on
the current evidence.
