# Hot/cold materialization directions: authenticated core first

Status: research direction, not an implementation or performance claim
Scope: local LayerFS only; native directory production is not Phase 4 storage

## 1. Directional conclusion

The largest plausible local materialization gains do **not** come from making
SQLite a little faster. They come from avoiding payload work under explicit
authority:

1. prove that an existing destination is exactly an authenticated parent root,
   then apply only the authenticated parent-to-child delta; and
2. for a new destination on the same APFS volume, clone already verified native
   file seeds instead of reconstructing and copying their bytes again.

Those are algorithmic changes for their qualifying workloads: payload work can
fall from `Theta(S)` to changed bytes, or from `Theta(S)` copying to `O(J)`
clone/namespace operations. They do not make the first-ever, no-seed
materialization sublinear. That path must still authenticate the logical graph,
produce every output byte, create the namespace, apply metadata, publish, and
meet the chosen destination-durability contract.

The proposed v2 single canonical chunk identity helps all read paths, but only
at the constant-factor level for a first materialization: after complete
canonical-object authentication, the current second raw-payload hash can go
away. The canonical ID is also the right key for compressed chunk
representations. It is **not** by itself a native-file cache key; a verified
seed needs an authenticated file-level identity.

No retained campaign measured native materialization, APFS clone hits, a
controlled cold page cache, or destination publication. Therefore this report
ranks directions and ceilings, not expected milliseconds.

## 2. Terms that must not be collapsed

| Term | Exact meaning here | What current evidence establishes |
|---|---|---|
| logical reconstruction | authenticated mapping traversal and CAS objects streamed as raw bytes | measured for the retained one-file S1-100 fixture |
| native materialization | logical reconstruction plus directories/files, mode application, destination custody/publication, durability, and declared verification | specified for later work; not implemented or measured |
| hot | the same source data was accessed previously without clearing the OS cache | must be declared by procedure |
| reopened / warm-or-unknown | process or SQLite reopened while OS-cache state was not controlled | exact label of the accepted F4-A rows |
| controlled cold | page-cache state was actually controlled | unavailable in retained evidence |
| empty destination | no destination entries exist before the operation | says nothing about source page-cache state |
| verified native seed | an authenticated, separately custodied native file eligible to be a clone source | hypothetical; not an OS page-cache hit |

The evaluation contract explicitly defines `cold`, `warm`, and `reopened`, and
says a SQLite reopen does not prove cold
(`implementation-detail/evaluation.md:303-314`). Phase 4 explicitly separates
CAS reconstruction from native materialization
(`implementation-detail/phase-4/algorithm/spec.md:887-910`). These distinctions
remain controlling.

## 3. Observed current behavior and evidence boundary

### 3.1 Logical and native contracts

- **Observed:** the intended user workflow is
  `open -> materialize -> modify ordinary directory -> capture or discard`
  (`implementation-detail/evaluation.md:5-13`).
- **Observed:** B1 is S2-tree into an empty destination, B2 is the same root
  into a matching destination, and B3 is parent-to-child with three changed
  paths (`implementation-detail/evaluation.md:95-119`). The S2 corpus is about
  100 MiB across 10,000 deterministic entries, unlike the accepted S1-100
  single-file evidence.
- **Observed:** a native correctness result must compare paths, kinds, lengths,
  bytes, and all LayerFS-defined metadata
  (`implementation-detail/evaluation.md:213-226`). Current logical `TreeNode`
  admits only file and directory kinds and its only metadata field is an
  arbitrary `u32 mode` (`crates/layerfs-core/src/cow/tree.rs:10-41`).
- **Observed:** current canonical names are UTF-8, exclude empty, `.`, `..`,
  NUL, slash, and backslash, but do not case-fold or Unicode-normalize
  (`crates/layerfs-core/src/format/path.rs:157-197`). Distinct canonical names
  can therefore collide on a case-insensitive or normalization-insensitive
  destination. Exact materialization needs a destination-volume admissibility
  rule or an exact collision rejection before mutation.
- **Observed:** the native target is `Theta(A + S + J)` for a clean no-seed
  checkout and, only after proving the destination is the parent root,
  `O(changed paths + changed bytes + changed mapping paths + durability)` for
  incremental work
  (`implementation-detail/phase-4/algorithm/complexity-analysis.md:772-813`).

### 3.2 Production read/authentication path

- **Observed:** public `load_object` asks for the length, calls the range path
  for the complete object, then constructs an owned record
  (`crates/layerfs-engine/src/lib.rs:377-392`).
- **Observed:** `read_object_range_on_connection` obtains metadata,
  authenticates the complete canonical BLOB, then opens it again for the
  requested slice (`crates/layerfs-engine/src/lib.rs:912-965`).
- **Observed:** `authenticate_blob` opens and hashes the canonical BLOB for its
  `ObjectId`, then opens and parses the same BLOB again
  (`crates/layerfs-engine/src/lib.rs:968-1017`). Thus a production range can
  perform two complete passes plus a third range read. This is a constant-factor
  optimization target, not evidence that one-pass error semantics are already
  solved.
- **Observed:** a snapshot receipt never replaces canonical hashing for bytes
  actually fetched, proves object existence, or authorizes another store/root/
  epoch (`implementation-detail/phase-4/algorithm/spec.md:745-770`). Native
  destination authority is a separate problem.

### 3.3 Accepted F2/F4 logical evidence

The exact accepted benchmark source is
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`,
SHA-256
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
The sealed raw JSONL SHA-256 is
`5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f`.

- **Observed:** measured component medians were fresh head reopen `1.098750 ms`,
  full scrub `280.250583 ms`, reconstruction `438.069792 ms`, and range
  verification `0.725958 ms`
  (`implementation-detail/phase-4/wp4m/f-series/f4/report.md:220-270`).
- **Observed:** every measured row says
  `source_cache_state = warm_or_unknown_after_manifest_preflight`. It is not a
  cold result.
- **Observed:** each reconstruction row authenticated `105,291,489` canonical
  bytes in `5,371` objects, raw-hashed `104,857,600` payload bytes, used 83
  leaf-batch queries and 170 total SQL queries, and wrote only to a hashing/
  counting sink. The source authenticates each selected canonical object,
  decodes it, checks raw length and raw ID, and updates the reconstruction hash
  (`...phase4_create_edit_benchmark-f2-accepted.rs:6608-6682`).
- **Observed:** an exact range authenticates each selected complete canonical
  chunk and then hashes the complete raw chunk before slicing the requested
  bytes (`...phase4_create_edit_benchmark-f2-accepted.rs:6820-6873`). Full scrub
  performs the same canonical-plus-raw verification for all references
  (`...phase4_create_edit_benchmark-f2-accepted.rs:7563-7613`).

**Derived:** `438.069792 ms` is the gross ceiling for everything inside that
specific logical reconstruction row. A native cache hit cannot save more than
that logical phase on this fixture, and its final native wall still includes
namespace, metadata, clone/copy, publication, and durability. The raw rehash
sublane was not separately timed, so neither the create-time raw-hash median nor
the scrub median may be substituted for it.

## 4. Workload-specific lower bounds and opportunities

| Workload | Honest necessary work | Best qualifying direction | Algorithmic effect |
|---|---|---|---|
| cold first materialize, no seed | authenticate graph/objects, produce `S` bytes, create `J` entries, metadata, publish/durability | one-pass authenticated output; optional per-chunk storage compression | still `Theta(S + J)`; constants only |
| warm no-op, same destination | prove destination still equals root | protected destination authority, otherwise full exact verification | `O(1)` only under exclusive authoritative custody; ordinarily at least `O(J)`, and `Theta(S)` if bytes must be rehashed |
| warm incremental parent -> child | prove exact parent, authenticate delta/new objects, mutate changed paths | parent-root-gated delta application | target depends on changed paths/bytes, not all `S` |
| repeated root to new destination | create new namespace and metadata; establish output bytes | verified native seed plus APFS per-file clone | on seed hits, payload copying can disappear; `O(J)` clone/metadata work remains |
| exact logical range | authenticate selected mapping path and selected payload authority | current bounded chunks; possibly future authenticated slices for larger objects | already follows selected range/chunks, not `S` |

The scenario name “cold first materialize” should be split in evidence into
`destination_state=empty` and an independently proven `source_cache_state`.
Otherwise it is an empty-destination reopened/warm-or-unknown row.

## 5. Ranked directions

### Rank 1 — destination-authority-gated no-op and delta materialization

**Type:** algorithmic for B2/B3.
**Target:** same destination, especially parent-to-child small changes.
**Plausible ceiling:** all unchanged CAS payload reads, decodes, hashes, and
destination rewrites; exact wall is unavailable because native B2/B3 do not
exist.
**Risk:** high authority/invalidation risk, moderate implementation risk.

**Hypothesis:** when a destination is proven to be exactly parent root `P`, an
authenticated `P -> C` delta can drive deletes, creates, replacements, and mode
changes while untouched files and subtrees remain untouched. This realizes the
existing Phase 3 intent that work is changed leaf/path plus ancestor spine, with
no unchanged payload reconstruction
(`implementation-detail/phase-3.md:112-130`).

A signed/MACed receipt saying “this was P” proves only what was published then.
It does not prove no process changed the directory afterward. Size/mtime/inode
checks and FSEvents are useful invalidation hints, not adversarial byte
authority. Apple says dropped FSEvents require a recursive full scan
([FSEvents dropped-event contract](https://developer.apple.com/documentation/coreservices/kfseventstreameventflaguserdropped)).
Therefore a fast path needs one explicit trust mode:

- exclusive LayerFS custody with a protected monotonic mutation authority;
- a complete, gap-free change journal whose gap path falls back to exact scan;
  or
- non-adversarial destination trust, explicitly weaker than byte authentication.

For an ordinary user-editable directory, exact untrusted correctness eventually
requires content verification. Metadata can filter candidates but cannot mint
byte equality.

**Decisive question:** can a future `layerfs-os` maintain destination authority
through ordinary-directory edits and crashes without scanning all bytes, while
provably falling back after watcher gaps or external mutation? Kill the
algorithmic claim if exact parent qualification is `Theta(S)` on every B2/B3
operation or if any unchanged file is rewritten.

### Rank 2 — verified native file seeds plus APFS clonefile

**Type:** algorithmic for repeated new destinations; no benefit before a seed
exists.
**Target:** repeated checkout/materialization of the same or mostly shared file
roots to new destinations on the same APFS volume.
**Plausible ceiling:** on a hit, avoid the complete logical payload path (gross
ceiling `438.069792 ms` for the retained S1 row) and avoid allocating/copying new
data extents initially; final native wall is unmeasured.
**Risk:** medium implementation risk, high cache-authority/storage-policy risk.

**Hypothesis:** reconstruct a file once into a private verified seed cache,
publish the seed only after complete authentication and output verification,
then use descriptor-relative `fclonefileat`/`clonefileat` to create new
destination files. Apply the LayerFS mode separately so seed metadata is not
confused with target metadata.

Apple's Darwin contract says a clone shares data blocks with its source, has
private subsequent writes through copy-on-write, must be on the same filesystem,
and requires a nonexistent destination. Attributes and ACL/ownership behavior
have exceptions; directory cloning is discouraged
([Apple `clonefile(2)` source](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2)).
`copyfile(..., COPYFILE_CLONE)` can try a clone and fall back, while
`COPYFILE_CLONE_FORCE` exposes clone failure; ACL and metadata semantics remain
explicit
([Apple `copyfile(3)` source](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.3)).

The safe file-seed key is not an individual chunk ID. Candidate keys are:

1. the authenticated file-root `ObjectId` — simplest but mode changes may
   duplicate identical byte seeds; or
2. a version/profile-bound ordered `(length, canonical chunk ID)` commitment —
   content-only for the frozen CDC/profile, reusable across mode changes.

The second key is safe only if it becomes authenticated product state. The
private benchmark whole-source digest and expected CDC sequence remain harness
evidence and must not become native cache authority accidentally. A seed cache
also needs protected custody, bounded eviction, corruption revalidation, and an
accounting model for the seed's real allocated bytes. Read-only permissions or
hard links are not integrity; hard links are especially unsafe because a user
write can mutate the cache object. Nix is useful only as a precedent for this
distinction: its official verifier recomputes content hashes because store paths
can be altered by non-store tools, and notes that checking large contents is
slow
([Nix store verification](https://releases.nixos.org/nix/nix-2.34.8/manual/command-ref/nix-store/verify.html)).

**Decisive question:** on the 1/10/100-file and S2-10,000-file shapes, does a
verified same-volume seed cache make clone-hit materialization materially faster
after including seed lookup, namespace/mode application, publication,
durability, and cache storage? Kill it if clone hits are rare/cross-volume, if
small-file syscalls dominate, or if required seed revalidation rereads most
payload bytes per checkout.

### Rank 3 — one-pass authenticated output with canonical-only chunk identity

**Type:** constant-factor for first materialization, scrub, reconstruction, and
ranges.
**Target:** every CAS read, especially no-seed materialization.
**Plausible ceiling:** the unisolated raw-payload rehash portion of the
`438.069792-ms` reconstruction row; never the whole row.
**Risk:** low execution risk after, but only after, the high-risk v2 format and
migration decision.

**Hypothesis:** v2 mapping references store `(raw_length, canonical ObjectId)`.
Once the complete canonical Bytes object hashes to that ID and its framing/
length validates, its raw payload needs no second identity hash. Decode and
write it directly through a bounded chunk buffer, retaining exact output length
and destination error ordering.

Current chunks are at most 32 KiB raw, so authenticating a complete chunk before
exposing it is already bounded. Bao demonstrates BLAKE3-based authenticated
streaming and slice proofs, but its combined/outboard tree is a new format and
its own project warns that it is unaudited beta cryptographic software
([Bao specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md),
[Bao repository](https://github.com/oconnor663/bao)). Bao becomes interesting
only if a later design stores much larger file/pack objects or must authenticate
parts of an untrusted native seed. Adding it to 8/16/32-KiB chunks now would add
proof bytes and a second tree without removing SQLite/object crossings.

**Decisive question:** what wall interval is spent on the post-canonical raw
hash in scrub, reconstruction, and range reads? Kill a speed claim if removing
that exact pass is below the material threshold or changes typed error
precedence/output-before-error behavior.

### Rank 4 — defer compression below canonical identity; reject it for the retained fixture

**Type:** constant-factor/storage trade for a future, demonstrably compressible
corpus; measured negative direction for the retained fixture.
**Target:** compressible corpora where physical source I/O dominates.
**Plausible ceiling:** physical bytes saved by the measured compression ratio,
not logical length. Current evidence has no controlled-cold host-I/O result.
**Risk:** medium CPU/range complexity; low identity risk only if representation
is below identity.

Keep the canonical ID over the exact uncompressed canonical object. Store a
versioned compressed representation under that ID, decompress into a bounded
buffer, and authenticate the uncompressed canonical bytes. This preserves CAS
keys, roots, and v2 chunk references across compression levels. The Remote
Execution API is a useful design precedent: compressed blobs are named by the
uncompressed digest/size and clients must verify decompressed bytes
([official REAPI protocol](https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto)).

**Observed directional screen:** the exact retained fixture already rejects
foreground compression as the next local optimization. Adaptive independent
zstd level 1 shrank only 563 of 5,284 chunks and saved 4,529,478 bytes: 4.3168%
of canonical chunk-object bytes and an ideal 4.1453% of the finalized database.
The encode screen took about 147.8 ms. Even a deliberately generous byte-linear
application to the mapping-VFS and COMMIT main-write medians yields only a
`3.004 ms` gross storage-wall ceiling
(`research/phase-4/storage/compression-and-packing.md:8-31,94-130,300-346`). Its full
decode screen was about 93.7 ms; that is not a production forecast, but it makes
a hot-read win implausible on this corpus.

Compress chunks independently. Whole-pack compression would make random ranges,
reuse, and corruption containment depend on unrelated bytes. A compressed CAS
representation also cannot be an APFS clone source for an exact raw destination;
an uncompressed verified native seed is a separate cache with separate storage.

**Decisive question:** on a materially different representative corpus, does
post-dedup independent compression save at least 20% of physical store bytes
without more than 5% encode/decode/Q/RSS overhead under controlled cold,
reopened/unknown, and warm states? The 20%/5% values are screening hypotheses
from the compression report, not product requirements. Kill it on the current
fixture now; on a new corpus, kill it if either screen fails or range
amplification exceeds one bounded chunk.

### Rank 5 — sparse writes for authenticated zero extents

**Type:** niche constant-factor/allocation optimization.
**Target:** files with large block-aligned zero runs.
**Plausible ceiling:** only the authenticated zero bytes not physically written;
zero density is unmeasured in S1/S2.
**Risk:** low-to-medium portability and accounting risk.

Darwin `F_PUNCHHOLE` replaces an aligned region with a usually unallocated hole
whose reads return zeros
([Apple `fcntl(2)` source](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/fcntl.2)).
`COPYFILE_DATA_SPARSE` helps only when copying an already sparse source. Neither
mechanism removes CAS mapping/object authentication. Sparse output must still
have exact logical length and bytes, and apparent/allocated/physical bytes must
remain separate observations.

**Decisive question:** what fraction of authenticated destination bytes forms
large eligible zero extents? Kill the direction if representative density is
small, extent discovery costs more than it saves, or the destination filesystem
does not preserve the required behavior.

### Rank 6 — lazy hydration as a different product mode

**Type:** algorithmic time-to-first-use improvement, but not native full
materialization.
**Target:** very large trees with small working sets.
**Risk:** very high semantic/platform coupling.

Apple File Provider lets the system manage local copies and placeholders and
hydrate content on access
([File Provider overview](https://developer.apple.com/documentation/FileProvider),
[placeholder behavior](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/FileProvider.html)).
That can exploit LayerFS exact ranges and canonical chunk cache keys, but it
changes the ordinary-directory contract, introduces system-managed materialized
state, and is not an exact B1 result until every entry is hydrated and verified.
Keep it as an optional later projection, never as the first materializer.

## 6. V2 identity and hot/cold consequences

| Concern | Current dual identity | V2 single canonical identity | Materialization consequence |
|---|---|---|---|
| cold chunk read | canonical object auth plus raw-ID hash | canonical auth and length are sufficient | removes one raw hash pass; physical read unchanged absent compression |
| range | complete selected chunk auth plus complete raw hash | complete selected chunk auth | still reads/authenticates a complete bounded chunk |
| compressed CAS key | ambiguous temptation to key raw/compressed separately | uncompressed canonical ID remains stable | compression level is representation metadata, not identity |
| native chunk cache | raw/canonical keys can diverge | one stable chunk key | simpler decoded-chunk cache; no proof of a whole native file |
| native file seed | file root/sequence plus current chunk identities | file root or ordered canonical-ID commitment | mode-independent commitment can improve seed reuse if product-authenticated |
| APFS output | raw bytes must be reconstructed | raw bytes must be reconstructed once | later clone hits can reuse an uncompressed verified seed |

**Hypothesis:** a small decoded-chunk cache keyed by canonical ID may help
repeated ranges or repeated chunks inside one materialization, but fresh S1-100
created 5,284 references with no reported reuse advantage. It cannot be the
headline direction without a corpus showing repeated canonical IDs. OS page
cache and SQLite cache already provide byte caching at other layers, so another
cache must prove unique hits rather than rename warm state.

## 7. Exact output, publication, and path safety are gates

Every direction above is subordinate to these unresolved native contracts:

- preflight all canonical paths against destination case/normalization rules
  before mutating anything;
- traverse/create relative to retained directory descriptors, reject symlink or
  wrong-kind substitution, and avoid path check/use races. Apple's secure file
  guidance recommends descriptor-based operations and `O_NOFOLLOW`, and treats
  user-writable paths as untrusted
  ([Apple secure file operations](https://developer.apple.com/library/archive/documentation/Security/Conceptual/SecureCodingGuide/Articles/RaceConditions.html));
- define how the logical arbitrary `u32 mode` maps to native mode and reject
  unrepresentable/unsafe values rather than silently changing metadata;
- create replacement files under unique same-directory/same-volume temporary
  names, finish content and metadata, then publish atomically. Apple's
  replacement API recommends a temporary location on the destination volume
  ([FileManager replacement](https://developer.apple.com/documentation/foundation/filemanager/replaceitem%28at%3Awithitemat%3Abackupitemname%3Aoptions%3Aresultingitemurl%3A%29));
- define file, directory, and namespace durability separately; clone atomicity
  is not automatically workspace publication durability;
- state whether destination verification is inside the timer. Full independent
  byte verification is itself `Theta(S + J)` and can erase a nominal clone or
  delta speedup.

## 8. Recommendation and anti-recommendations

Recommended future-specialist order:

1. Specify destination identity/custody and exact native path/mode/publication
   behavior. Without this, B2/B3 speed has no safe authority.
2. Prototype parent-root-gated delta application on one file and 10,000-file
   trees, with forced external mutation and watcher-gap fallbacks.
3. Prototype a private verified per-file seed cache and per-file APFS clone on
   1/10/100/10,000-file shapes, including cross-volume/fallback results and real
   allocated bytes.
4. After the v2 format decision, isolate canonical-auth versus raw-rehash versus
   decode/write intervals on the accepted read path.
5. Do not implement compression for the retained fixture. Reopen only the
   below-identity per-chunk screen if a representative corpus first demonstrates
   materially better post-dedup compressibility.

Do not:

- call the accepted reconstruction row cold or native materialization;
- use a materialization receipt, mtime, inode, or FSEvents alone as byte
  authority for a user-editable destination;
- use hard links from a mutable workspace into a trusted cache;
- key storage identity by compressed bytes or compression settings;
- introduce whole-pack compression before proving range and edit locality;
- add Bao to the current small-chunk path merely because it supports verified
  streaming;
- claim sparse allocation, clone sharing, logical length, or OS cache warmth as
  physical-I/O evidence; or
- treat lazy hydration as a faster implementation of exact full materialization.

## 9. Decisive questions for the next specialists

1. What exact authority can prove that an ordinary native destination still
   equals root `P` after user edits, process death, reboot, event loss, and
   out-of-band mutation?
2. Is the authenticated file root sufficient as a native-seed key, or should a
   version/profile-bound ordered canonical-ID commitment become explicit product
   state so metadata-only roots share byte seeds?
3. On APFS, where is the crossover at which verified seed lookup plus per-file
   clone beats direct authenticated reconstruction for 1, 10, 100, and 10,000
   files after metadata, publication, durability, and verification?
4. How much of scrub/reconstruction/range wall is the raw rehash that v2 can
   remove, and how much is canonical authentication, SQL/object crossing,
   decode, and sink/output?
5. Can the native path reject every destination alias/collision and path race
   before visible mutation while preserving exact LayerFS bytes and mode?

Until questions 1 and 5 are answered, no warm no-op or incremental native
performance result is safe to promote. Until a separate native campaign exists,
`438.069792 ms` remains a logical warm-or-unknown reconstruction measurement,
not a checkout target.
