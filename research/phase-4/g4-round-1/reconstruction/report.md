# G4 Round 1 — reconstruction architecture and authenticated read path

## Disposition

**Lane disposition: `ROUND-1 RESEARCH COMPLETE / NO PROMOTION / G4 UNSTARTED`.**

The highest-value finding is a contract and path distinction, not a new data
format:

1. the retained logical-reconstruction benchmark performs three complete
   byte-work families after acquisition—canonical object authentication,
   benchmark closure commitment, and a raw-output fingerprint—but only the
   first is the durable byte authority of the Canonical-v2 Merkle graph;
2. G3's existing complete fallback proves the direct native-sink shape—one
   canonical-authenticated traversal into a private temp, with mapping checks
   and a raw digest—but it is a **diagnostic control**, not a promotable M0:
   it fetches each chunk with a separate query and omits the accepted logical
   closure/sequence folds;
3. consequently, G4 should measure that fallback first only to establish the
   missing native control, then compare it with a proof-preserving batched
   walker. The 88.483-ms closure budget must not be subtracted from the
   fallback wall because the fallback does not pay it, and the fallback must
   not be promoted by hiding its query/proof regression;
4. a protected seed's full logical read and an APFS clone are different
   operations. A full read remains `Theta(S)` returned bytes; a clone transfers
   no logical payload to the caller and must not be labeled 2–3 GiB/s;
5. the best system-level architecture is two representations: Canonical-v2 as
   durable truth, plus a capacity-bounded, content-addressed protected native
   seed plane. It can accelerate trusted full reads and clone materialization,
   but cache fill and cross-process authority are unresolved and cannot be
   hidden outside the claimed operation.

No experiment was executed. All repository and retained-evidence access was
read-only. No timing-sensitive probe was started; the lead was notified before
any such work, and the prospective probes below require fail-fast acquisition
of the repository benchmark lock.

## Evidence labels and scope

- **Observed** means directly present in the cited local source or sealed
  artifact.
- **Derived** means arithmetic or a structural consequence of observed facts;
  the equation is shown.
- **External** means a claim from a linked primary specification or official
  API document.
- **Hypothesis** means a falsifiable proposed mechanism.
- **Unavailable** means the current evidence cannot support the observation.
- **Speculative upper bound** is a gross ceiling, not a prediction and never an
  acceptance result.

This report analyzes reconstruction, authenticated ranges, trusted-seed reads,
and their connection to first native materialization. It does not promote a
candidate, change a profile, run G4 acceptance, or claim production
integration. `layerfs-vfs` and `layerfs-sdk` are five-line component stubs, so
there is no existing VFS read implementation to benchmark or optimize
([`crates/layerfs-vfs/src/lib.rs:1`](../../../../crates/layerfs-vfs/src/lib.rs),
[`crates/layerfs-sdk/src/lib.rs:1`](../../../../crates/layerfs-sdk/src/lib.rs)).

## Custody freeze

### Repository and host

| Item | Observed value |
|---|---|
| Working directory | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| HEAD | `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` |
| Initial status | only pre-existing untracked `implementation-detail/phase-4/experiments/g4-materialization-acceptance/`; no tracked diff |
| Inspection timestamp | `2026-08-22T06:27:05Z` |
| Rust | `rustc 1.96.0 (ac68faa20 2026-05-25)`, `aarch64-apple-darwin`, LLVM 22.1.2 |
| Cargo | `cargo 1.96.0 (30a34c682 2026-05-25)` |
| OS | macOS 26.4.1 (25E253), Darwin 25.4.0, arm64 |
| CPU / memory | Apple M3 Max; 14 logical CPUs; 38,654,705,664 B RAM |
| Filesystem | `/dev/disk3s5`, APFS, internal solid state, Apple Fabric; 994,662,584,320 B volume, 428,427,538,432 B free at inspection |
| Benchmark lock | no active repository-level `BENCHMARK_LOCK*` file found by the read-only scan; this is not proof that no external campaign exists |

The exact reusable fixtures were read-only (`0444`) and were hashed in place:

| Fixture | Size | Device/inode | SHA-256 |
|---|---:|---|---|
| `S1-1.source` | 1,048,576 | `16777232/734451393` | `4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a` |
| `S1-10.source` | 10,485,760 | `16777232/734451394` | `0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e` |
| `S1-100.source` | 104,857,600 | `16777232/734451395` | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |

Their path is
`target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/`.

### Controlling retained evidence

**Observed.** G3 is `PASS / G4 READY`; G4 remains planning-only. Controlling
custody is source-set
`3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`,
executable
`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`,
raw
`3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c`,
static closure
`cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531`,
67-entry manifest
`1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49`,
terminal
`1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e`,
and terminal verification
`a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6`.
The retained report's authority and exact-work description is at
[`G3-REPORT.md:67`](../../../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md)
through line 118.

## End-to-end read trace

### 1. Canonical identities and mapping authority

**Observed.** `ObjectId` is BLAKE3 over the domain-separated canonical object
bytes (`"layerfs/object\0" || canonical`), including a streaming reader form
([`identity/digest.rs:6`](../../../../crates/layerfs-core/src/identity/digest.rs),
[`identity/ids.rs:16`](../../../../crates/layerfs-core/src/identity/ids.rs)).
Canonical-v2 file leaf references are exactly `(u32 raw_length, ObjectId)`, 36
bytes per occurrence; no raw `ChunkId` remains in the selected mapping profile
([`canonical_v2.rs:15`](../../../../crates/layerfs-core/src/canonical_v2.rs),
[`canonical_v2.rs:72`](../../../../crates/layerfs-core/src/canonical_v2.rs)).
The file root commits total length, reference count, level, child object IDs,
and cumulative ends; leaf/branch validators enforce partition, ordering,
length, role, and topology.

**Derived authority chain.** If the namespace object, file-root object, every
visited mapping object, and every referenced Bytes object are each validated
against their expected `ObjectId`, then the namespace root transitively commits
the ordered logical byte stream. An additional hash of all canonical object
bytes can be a convenient evidence value, but it does not add a new stored
authority to that fully authenticated graph.

**Security constraint.** Current core helpers intentionally return
`IdentityMismatch` before decoding grammar: `validate_identity` and
`validate_bytes_identity` hash the complete canonical bytes first, then decode
([`object/codec.rs:153`](../../../../crates/layerfs-core/src/object/codec.rs)).
Any fused reader must preserve this exact error precedence. Streaming grammar
validation alone is not equivalent: `validate_object_from` may encounter a
parser error before the identity pass completes
([`object/codec.rs:173`](../../../../crates/layerfs-core/src/object/codec.rs)).

### 2. Selected Canonical-v2 SQLite access

**Observed.** The private selected-profile store uses one SQLite row per
canonical object. Mapping reads call `read_canonical`, borrow the row BLOB, and
copy it into a charged owned buffer before validating/decoding
([`phase4_create_edit_benchmark.rs:3005`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)).
Chunk reads use `Row::get_ref(...).as_blob()` and authenticate while the row is
alive, with no Rust-owned payload copy
([`phase4_create_edit_benchmark.rs:3040`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)).
Leaves batch up to the profile's 64 references in one generated `VALUES` query,
then consume rows in ordinal order
([`phase4_create_edit_benchmark.rs:3064`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)).

**External corroboration.** rusqlite documents `Row::get_ref` as reading row
data without copying, with validity limited to the row lifetime
([official rusqlite `Row` API](https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get_ref));
the exact vendored implementation says the same at
`rusqlite-0.40.2/src/row.rs:305-326`. SQLite's incremental BLOB API instead
opens a BLOB tied to a table/column/rowid, and `sqlite3_blob_read` copies the
requested byte subsection into the caller buffer
([official `sqlite3_blob_open`](https://www.sqlite.org/c3ref/blob_open.html),
[official `sqlite3_blob_read`](https://www.sqlite.org/c3ref/blob_read.html)).
These API facts do not establish physical I/O or page-cache residence.

### 3. Complete logical reconstruction (`reconstruct_file`)

The selected benchmark path is
`reconstruct_file -> verify_file_inner -> stream_file`
([`phase4_create_edit_benchmark.rs:9038`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs),
[`phase4_create_edit_benchmark.rs:9058`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs),
[`phase4_create_edit_benchmark.rs:8238`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)).

**Observed exact 100-MiB read work.** Sealed G2 records the following one-pass
shape ([`G2-REVISE-REPORT-v1.md:183`](../../../../implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md)):

| Counter | Value | Interpretation |
|---|---:|---|
| Logical source/output bytes | 104,857,600 | every logical payload byte reaches the fingerprint sink |
| Chunk references | 5,284 | ordered leaf occurrences |
| Authenticated objects | 5,371 | 5,284 chunks + 87 namespace/mapping objects |
| Canonical bytes authenticated | 105,122,401 | complete identity hashing of all fetched canonical bytes |
| Borrowed chunk BLOB rows/bytes | 5,284 / 104,926,292 | row-borrowed canonical chunk payloads |
| Leaf batch queries/references | 83 / 5,284 | at most 64 references per batch |
| SQL queries/rows | 170 / 5,371 | 87 singleton mapping/object queries + 83 leaf batch queries |
| Transaction/COMMIT | 0 / 0 | read-only path |
| Operation Q high-water/terminal | 32,195 / 0 B | bounded charged owned state; SQLite cache and RSS are separate |

**Observed pass inventory.** The logical path does not allocate a 100-MiB
application buffer, but it performs these folds:

| Stage | Scope | Reads/copies/allocations | Hash/fold | Required status |
|---|---|---|---|---|
| Namespace + mapping fetch | 87 complete canonical objects | 87 singleton SQL queries; each row BLOB copied to bounded `ChargedBytes`; decoded vectors/DFS frames bounded by profile | full canonical `ObjectId` authentication | required for fetched-byte identity, roles, topology and cycle/partition checks |
| Leaf chunk fetch | 5,284 complete canonical objects | 83 SQL queries, 5,284 returned rows; borrowed BLOB slices, no Rust-owned full chunk copy | full canonical `ObjectId` authentication | required under current object identity |
| Bytes decode | each chunk | header/length parse; raw is a borrowed subslice | no payload copy | required grammar/role/length check; the second decode measured only 0.141476 ms |
| Closure commitment | all 5,371 canonical objects | no extra DB read; rereads the in-memory/borrowed canonical slices | domain/tag + object ID + full canonical bytes | benchmark evidence/API result; redundant for byte authority after every graph edge/object was authenticated; product requirement uncertain |
| Occurrence commitment | 5,284 leaf references | 36 B per occurrence fed from decoded refs | raw length + canonical object ID | benchmark evidence; mapping already commits the same ordered occurrences; 0.408711-ms median |
| Logical fingerprint | all 104,857,600 raw bytes | raw borrowed slice; the hasher is the only logical sink in this benchmark | full raw BLAKE3 | exact benchmark oracle; a real reconstruction must still deliver/copy/write every byte, but need not necessarily return this digest |
| File-root parse | one already-owned canonical buffer | parsed twice in memory (`9067-9110`); no second SQL/BLOB read | none beyond prior auth | small local redundancy; no retained ceiling |

**Observed timing decomposition.** G2's components are disjoint within each
instrumented row, although component-wise medians may come from different rows
and must not be summed into a synthetic median
([`G2-REVISE-REPORT-v1.md:154`](../../../../implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md)):

| Family | Median | Contract classification for this research |
|---|---:|---|
| Canonical authentication | 94.816564 ms | required for cold/current-store fetched bytes |
| Closure commitment | 88.483070 ms | not required for transitive byte authority; benchmark/API evidence value |
| Source fingerprint/logical sink | 87.889943 ms | benchmark oracle; mandatory `Theta(S)` output delivery remains |
| SQLite/BLOB acquisition | 59.403771 ms | required with current SQLite representation; gross packing/access ceiling |
| Occurrence commitment | 0.408711 ms | redundant with authenticated mapping order unless explicitly exported |
| Mapping/topology validation | 0.199333 ms | required |
| Second Bytes decode/length | 0.141476 ms | removable but immaterial |
| Residual | 1.671903 ms | composite, ineligible as a candidate |

The control center was 328.897052 ms; instrumentation was 332.404990 ms
(+1.0666%), with all observer gates passing
([`G2-REVISE-REPORT-v1.md:140`](../../../../implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md)).

### 4. G3 complete native fallback is already a different path

**Observed.** `stream_root` resolves the namespace/file root, validates the
mapping graph, fetches and canonically authenticates every selected chunk,
decodes its raw borrowed slice, writes it to the supplied sink, and updates one
raw BLAKE3 digest
([`phase4_g3_materialization.rs:1087`](../../../../crates/layerfs-engine/src/bin/phase4_g3_materialization.rs)).
It does **not** compute the G2 closure commitment or occurrence commitment.
More importantly, its callback calls `with_borrowed_bytes` once for each chunk
rather than `for_each_leaf_bytes`; G3's counter adapter classifies each borrowed
row as one object SQL query (source lines 165-190). On the retained S1-100 graph,
the expected full-fallback shape is therefore approximately 5,371 queries/rows
(5,284 chunks + 87 namespace/mapping objects), versus the accepted batched
logical path's observed 170 queries/5,371 rows. This full-size G3 fallback count
is **derived**, not a measured G3 100-MiB row; the measured 1-MiB fallbacks show
the same one-query-per-object shape at 59 queries/rows/BLOBs.

When qualification or clone fails, `run_operation` creates a private temp and
calls `stream_root` directly into that file; it checks returned length,
reference count, and raw digest, then performs data sync, chmod, metadata sync,
rename, and directory sync
([`phase4_g3_materialization.rs:1954`](../../../../crates/layerfs-engine/src/bin/phase4_g3_materialization.rs),
especially lines 2055-2081 and 2125-2204).

This reconciliation changes the performance model:

- the 88.483-ms G2 closure family is a ceiling for replacing the logical
  benchmark's closure fold, not for G3 first/full materialization;
- the 87.890-ms raw-fingerprint family is the only known full hash fold in
  G3 fallback beyond canonical authentication, but dropping it is not a free
  87.890-ms materialization gain because G3 simultaneously writes and later
  syncs 100 MiB, costs that G2's hash-only logical sink did not measure;
- first/full 100-MiB fallback wall is **unavailable**. The retained 1-MiB
  fallback rows do not support linear extrapolation to 100 MiB or controlled
  cold state.

Therefore the first G4 native measurement should include the existing G3
fallback only as a diagnostic control, not as an accepted candidate. Any
promotable M0 must use the 64-reference batched leaf access (or improve its
exact counters), preserve canonical identity/topology/error authority, and
preserve the preregistered closure/sequence/output proof results unless the G4
contract explicitly versions and independently validates their removal. A
candidate that looks fast only because it regresses 170 queries to roughly
5,371 or omits proof outputs is ineligible.

### 5. Range reads

**Observed.** `read_file_range` authenticates the file-root and each selected
mapping node, routes by cumulative ends, authenticates each intersecting
canonical chunk in full, and copies only the requested raw subranges into an
exact-capacity charged `Vec`
([`phase4_create_edit_benchmark.rs:8370`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs),
[`phase4_create_edit_benchmark.rs:8446`](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)).
The retained 1-MiB returned range is 2.279209 ms / 438.749 MiB/s. The range
complexity is `O(H + J + R)` where `H` is mapping height/nodes, `J` is complete
canonical bytes of intersecting chunks, and `R` is returned bytes. It is not
`O(R)` under whole-object `ObjectId` authentication.

The G3 one-byte patch authenticated four objects/rows/queries and 22,551
canonical bytes; its 1-MiB patch authenticated 59 objects/rows/queries and
1,086,013 canonical bytes
([`G3-REPORT.md:79`](../../../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md),
[`G3-REPORT.md:175`](../../../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md)).
Those rows show that current chunk granularity already gives efficient partial
authentication. A large canonical extent profile would trade this protected
win for sequential-read savings.

### 6. Production engine access is not the selected benchmark path

**Observed.** The public engine remains an older schema/API and is not the
Canonical-v2 benchmark implementation. Its range read:

1. queries object rowid/kind/canonical length/BLOB length;
2. opens and reads the full SQLite BLOB to hash `ObjectId`;
3. reopens and reads the full BLOB to validate canonical grammar;
4. opens a third BLOB and copies the requested range
   ([`crates/layerfs-engine/src/lib.rs:912`](../../../../crates/layerfs-engine/src/lib.rs));
5. `load_object` uses that full range and then `ObjectRecord::new` validates it
   again ([`crates/layerfs-engine/src/lib.rs:377`](../../../../crates/layerfs-engine/src/lib.rs)).

For a canonical object of length `b` and requested range `r`, this is at least
`2b + r` SQLite BLOB bytes plus a later in-memory revalidation for `load_object`.
A one-row borrowed or one-owned-buffer path can preserve identity-first error
precedence with `b + copy(r)` byte work and one row acquisition. This is a real
format-preserving production repair, but it cannot explain or improve the
retained G2 selected-profile path because that path already borrows each chunk
row and hashes it once.

### 7. Seed construction, trusted seed reads, and clones

**Observed preparation cost.** G3 preparation is intentionally outside the
operation timer but is not free system work:

1. parent and target source files are created and hashed;
2. parent and target are built/published and scrubbed;
3. `prove_canonical_range` fully reconstructs/authenticates parent and target
   while byte-comparing each to stable no-follow source descriptors, then
   performs a complete parent/target exact-relation comparison
   ([`phase4_g3_materialization.rs:1146`](../../../../crates/layerfs-engine/src/bin/phase4_g3_materialization.rs));
4. the parent destination is fully materialized;
5. `create_verified_seed` fully reconstructs parent into a new seed, syncs and
   chmods it, closes the writer, reopens it read-only/no-follow, re-reads and
   hashes every byte, unlinks the name while retaining the descriptor, and
   validates regular-kind/link-count/length
   ([`phase4_g3_materialization.rs:1285`](../../../../crates/layerfs-engine/src/bin/phase4_g3_materialization.rs));
6. a single-use operation permit is minted and bound.

These repeated full reads prove the benchmark fixture and seed mechanism; they
must not be charged to every seed hit, but a production cache must report cache
fill and rebuild separately instead of hiding them.

**Trusted-seed full read.** A same-open read-only, unlinked, exact-verified seed
descriptor can be a scoped authority for later reads during that authority
lifetime. Reading it into a caller sink is still `Theta(S)` returned bytes and
at least one full file read/copy pass. At 100 MiB, the 50-ms objective requires
`100 MiB / 0.050 s = 2,000 MiB/s`; 35 ms requires 2,857 MiB/s. Those rates are
hypotheses and likely cache-sensitive. Current evidence does not identify page
residency, controlled-cold physical I/O, or whether an in-timer output digest
can coexist with the 50-ms target.

**Clone materialization.** The retained G3 clone copies the seed's filesystem
extent mapping into a private candidate, patches authenticated changed bytes,
and publishes it. The 10-MiB no-op was 0.993791 ms and the 100-MiB one-byte
patch was 3.414166 ms, but these are single G3 kill-screen rows, not G4
acceptance ([`G3-REPORT.md:217`](../../../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md)). A clone exposes a
100-MiB logical file without returning 100 MiB to the application; throughput
computed from logical size would be misleading.

**Authority boundary.** A 216-byte validation receipt binds store identity,
authority/epoch/generation, roots/transition, profile, and a keyed
authenticator, but does not authenticate later bytes fetched from an ordinary
mutable native pathname
([`validation.rs:7`](../../../../crates/layerfs-core/src/validation.rs)).
Receipt/root equality may select a seed cache key; it cannot replace protecting
or revalidating the seed bytes. Same-open descriptor authority does not by
itself establish persistent cross-process authority.

## Required authority versus evidence-only work

| Work | Reconstruction | Range | First native materialization | Scrub | Classification |
|---|---|---|---|---|---|
| Validate namespace/root object identity and role | yes | if namespace-root API used | yes | yes | required |
| Validate every traversed mapping object's identity, role, level, order, partition, cumulative ends, count and cycle freedom | all graph | routed graph | all graph | all graph | required |
| Validate every fetched chunk's complete canonical `ObjectId`, Bytes grammar and declared raw length | all chunks | intersecting chunks | all chunks | all chunks | required under current identity/storage threat model |
| Deliver exact logical bytes in order | yes, caller sink | requested range only | yes, private temp | no | required by operation semantics |
| Raw whole-output BLAKE3 compared to retained fixture digest | current benchmark | external expected comparison currently bytewise | current G3 fallback | no | benchmark oracle/defense-in-depth; product requirement uncertain |
| Ordered closure hash over full canonical bytes after each object was authenticated | current logical benchmark | no full closure | absent in G3 fallback | absent | benchmark evidence/API result; redundant for fetched-byte authority |
| Separate ordered `(length,ObjectId)` occurrence hash | current logical benchmark | absent | absent in G3 fallback | absent | redundant with authenticated mapping unless exported compatibility value |
| Seed full read-back at cache creation | n/a | n/a | current seed preparation | n/a | required for G3 seed's exact qualification under current stated mechanism; production frequency/threat model uncertain |
| Full parent/target source equality and exact patch-relation proof | n/a | proof preparation | G3 preparation | n/a | benchmark mechanism qualification; production must replace it with capture/transaction authority, not repeat it per operation |
| Destination exact comparison after ambiguous publication | n/a | n/a | conditional reconciliation | n/a | required for old/new resolution; G3 lost-ack paid 59 queries/BLOBs and a 1-MiB destination/source compare ([`G3-REPORT.md:102`](../../../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md)) |

The classification "redundant" is deliberately narrow: it means redundant for
authenticating the fetched logical bytes from a validated root. It does not
authorize deleting an API result, benchmark oracle, independently specified
audit digest, or error-precedence behavior without changing and versioning that
contract.

## Complexity and resource model

Let:

- `S` = logical file bytes;
- `C` = chunk occurrences (5,284 for retained S1-100);
- `M` = mapping/namespace objects (87 in the retained full read);
- `A` = authenticated canonical bytes (105,122,401 retained);
- `R` = returned range bytes;
- `J` = complete canonical bytes of chunks intersecting a range;
- `H` = routed mapping nodes;
- `E` = physical extent/segment terms;
- `B` = bounded stream buffer;
- `K` = protected seed-cache capacity in allocated bytes.

| Path | Current work | Minimum/target work | Peak application-owned state | Persistent state |
|---|---|---|---|---|
| Full cold/current-store reconstruction | `Theta(A)` auth + `Theta(A)` closure + `Theta(S)` output hash + SQL row acquisition | `Theta(A + S)`; cannot avoid authenticating current stored bytes or delivering output | `O(B + mapping-page)`; retained Q 32,195 B, separate SQLite cache/RSS | canonical store only |
| Full G3 fallback materialization | `Theta(A + S)` auth/hash/write + durability syscalls | still `Theta(A + S)` | bounded mappings/chunk row + filesystem buffers | one temp/destination; no seed required |
| Range | `O(H + J + R)` | same under whole-object IDs; proof-enabled sub-object format could approach `O(H + proof + R)` | `O(R + B)`; current API allocates exact `R` | none |
| Same-open trusted seed read | unavailable | `Theta(S)` read/copy; optional digest adds another CPU fold over same bytes | `O(B)` for streaming, or caller-owned `O(S)` if API returns a `Vec` | one capacity-accounted seed |
| Seed clone materialization | `O(1)`/filesystem metadata span plus `O(changed bytes)` patches and sync/publication; physical extent behavior unavailable | same | `O(B + changed-range)` | shared extents plus destination allocation semantics |
| Root-keyed native cache | fill `Theta(A + S)`, hits trusted read `Theta(S)` or clone metadata | same | `O(B)` plus bounded index | `O(K)`, not `O(revisions * S)` if eviction is enforced |

No current counter supports physical bytes read, device-cache residency,
stable-media completion, instruction count, or cycles. RSS includes allocator,
SQLite and runtime state and is not Q. SQLite cache snapshots are not page-cache
true high-water or host physical I/O.

## Candidate evaluations

### A. Measure the existing G3 fallback as a diagnostic native control

| Field | Required content |
|---|---|
| Mechanism | No architecture change. Exercise `run_operation`'s `complete-fallback` with empty destination and clone-unavailable/invalid-authority classifications. It overlaps canonical authentication, mapping validation, raw output hashing and native writes in one traversal, but performs one query per chunk and omits G2 closure/occurrence outputs. It is a diagnostic control only. |
| Target paths | First full native materialization, warm source, fresh process, controlled-cold if honestly obtainable, cross-volume/clone-failure fallback. Reconstruction and range unchanged. |
| Complexity | `Theta(A + S)` work, single-thread span, 5,371 object authentications and derived ~5,371 SQL queries for S1-100, plus `S` writes and sync/publication. Accepted batched logical shape is 170 queries for the same 5,371 rows. |
| Measured ceiling | Current first-full wall is unavailable. G2's 88.483-ms closure ceiling is **inapplicable** because fallback does not perform it. Canonical auth 94.817 ms, BLOB acquisition 59.404 ms, raw digest 87.890 ms are related gross logical-work budgets, but native write/sync overlap and costs are unmeasured. |
| Predicted gain | No gain claim; establishes a missing diagnostic baseline. Hypothesis: warm source can meet `<=400 ms`, but there is no valid retained equation for its wall. It is not promotable even if fast unless a candidate restores batching and the accepted proof outputs. |
| CPU | One core; full canonical hash plus full raw hash. Record user/system CPU and context switches. |
| Memory/Q | No full-file app buffer; current mapping/chunk bounds. Require process RSS `<=20 MiB`, exact Q high-water/terminal, SQLite cache snapshots and output buffer accounting. |
| Storage | Private full temp/destination `S`; no retained seed. Record logical/apparent/allocated pre/temp/post; cleanup residue zero. |
| Authority | Current canonical graph auth + expected raw digest/count/reference oracle; no weaker root/receipt trust. |
| Durability | Existing data sync, mode change, metadata sync, atomic no-follow rename, directory sync, and reconciliation. |
| Identity/format | None; selected Canonical-v2 benchmark-private path. |
| Cross-operation effect | None; diagnostic measurement only. Its scalar-query shape is a reconstruction regression and must not become the shared walker. Protect create 308.884 ms, edits 5–7 ms, range 2.279 ms and G3 rows. |
| Experiment | With fail-fast benchmark lock, run one 10-MiB existing frozen-binary `invalid-authority` fallback screen in `/tmp/layerfs-g4-r1-reconstruction-fallback-10`; retain only as a diagnostic if exact route/counters/Q/cleanup pass. A later 100-MiB row is admissible only paired with the batched proof-preserving candidate and cannot promote this control. Entire disposable screen must exit within 120 s. |
| Evidence | G3 source lines 1954-2251; retained fallback/error contract at G3 report lines 86-118. |
| Disposition | **DO NOW / G4 diagnostic control only; ineligible for promotion.** |

### B. Proof-preserving batched native sink first; evidence-policy split second

| Field | Required content |
|---|---|
| Mechanism | Define a format-preserving authenticated streaming primitive around the accepted `for_each_leaf_bytes` batching: validate every fetched canonical object and mapping invariant exactly once, fold the preregistered closure and ordered occurrence sequence, and emit raw bytes/digest to a caller/native sink. First M0 preserves all current proof results. Only a separately versioned policy A/B may move closure or fixture-oracle work outside product time. Reuse walker logic, not G3's scalar-query callback. |
| Target paths | Warm/fresh reconstruction, first/full materialization, scrub; range keeps routed traversal. VFS can later consume the same callback interface. |
| Complexity | Proof-preserving M0 remains `Theta(A auth + A closure + S fingerprint/delivery)` but overlaps all folds with one traversal and keeps 83 leaf batch queries/170 total. A later contract variant could be `Theta(A auth + S delivery)` with optional evidence folds separately attributed. |
| Measured ceiling | On the G2 parent only, removing closure while retaining raw digest gives speculative ceiling `328.897052 - 88.483070 = 240.413982 ms` (~416 MiB/s). Removing both hashes yields `152.524039 ms` **before adding a real output sink**, so it is not a reconstruction/materialization prediction. |
| Predicted gain | Proof-preserving M0 gain is unavailable: batching can reduce the G3 fallback's derived ~5,371 queries to the retained 170-query shape, while closure/sequence folds add CPU. The closure-free 240.414-ms number applies only to a later contract variant, not accepted M0. |
| CPU | Removes one complete canonical-byte hash fold from logical product path; optional raw digest remains. Single-thread baseline; parallel hashing is not assumed free. |
| Memory/Q | Same bounded traversal. Sink API must expose bounded buffer/queue and cancellation/error order; no `Vec<S>` requirement. |
| Storage | None for logical stream; native sink unchanged. External oracle artifacts compact only. |
| Authority | Root-to-object canonical authentication, mapping validation, ordered occurrence sequence, closure result, raw digest and identity-first error precedence retained for M0. Any later evidence-policy reduction requires an explicit contract version and independent exact oracle. |
| Durability | None for logical stream; native caller owns existing sync/publication contract. |
| Identity/format | No format/schema/profile change. API/result contract changes if callers require closure digest; version that API explicitly. |
| Cross-operation effect | Create/edit/range untouched if walker reuse is read-only. Scrub may share traversal without output. VFS benefits from direct streaming. |
| Experiment | Under the lock, first compare scalar G3 control with a copied-source/private proof-preserving batched sink at 10 MiB. Require identical root/output/digest/closure/sequence/errors, 83-per-100-MiB-equivalent leaf batching, and no Q/RSS regression. Only after that passes may a second one-variable closure-policy A/B be considered. Build + test + cleanup <=120 s per experiment. |
| Evidence | G2 lines 154-176 and code lines 8238-8300/9058-9177; G3 `stream_root` lines 1087-1117 is the existing proof of shape. |
| Disposition | **DO NOW / required proof-preserving batched M0 candidate; no promotion from scalar G3 control.** |

### C. Measure trusted-seed full logical reads separately from clone hits

| Field | Required content |
|---|---|
| Mechanism | From the already qualified read-only/unlinked same-open seed descriptor, seek/pread sequential bytes into a bounded sink. Score once without an in-timer digest (product read delivery) and once with digest (evidence cost), with an untimed independent exact oracle for both. Do not use `fclonefileat` for this row. |
| Target paths | Trusted hot full read and future VFS read; clone materialization remains a separate scoreboard. |
| Complexity | `Theta(S)` bytes read/delivered, `O(B)` memory. Digest variant adds `Theta(S)` CPU fold but not another filesystem read if fused. |
| Measured ceiling | No retained trusted-read wall. The objective requires 2,000 MiB/s at 50 ms or 2,857 MiB/s at 35 ms. G2's 87.890-ms raw-hash family warns that the current single-thread digest policy alone is incompatible with 50 ms, but paths differ and this is not a prediction. |
| Predicted gain | Hypothesis: cache-hot descriptor streaming without digest can meet <=50 ms on this host; controlled-cold is likely storage-bound and must be separately classified. Reject the 2–3 GiB/s claim if the exact 100-MiB read misses 50 ms or requires RSS >20 MiB/unbounded cache. |
| CPU | One read/copy pass; record user/system separately. No invented clone throughput. Optional digest CPU disclosed. |
| Memory/Q | Fixed 1-MiB or smaller buffer; caller sink accounted; RSS <=20 MiB. Seed itself is persistent/cache state, not RSS/Q. |
| Storage | Existing seed consumes up to `S` allocated bytes; a product cache needs explicit `K`, eviction and rebuild. No per-revision duplicate. |
| Authority | Only valid during protected descriptor/broker authority lifetime. Revalidate fstat identity/length/readonly/unlinked invariants. Ordinary path metadata is insufficient. |
| Durability | Read-only; seed fill qualification and corruption recovery separate. |
| Identity/format | No canonical format change; new cache/API lifecycle. |
| Cross-operation effect | Can serve hot reads and clone source. Seed fill adds full reconstruct + write + readback and may worsen first miss; account separately. |
| Experiment | After lock acquisition, use an existing G3-qualified 10-MiB seed in a copied/private harness; one bounded sequential descriptor read to a counting sink, then untimed SHA-256/BLAKE3 exact oracle; if direct counters and authority pass, one 100-MiB row. Each complete screen <=120 s. |
| Evidence | Seed construction lines 1303-1383; retained clone row lines 217-237; no existing trusted-read evidence. |
| Disposition | **DO NOW / separate G4 scoreboard.** |

### D. Fuse production engine object acquisition/authentication/range copy

| Field | Required content |
|---|---|
| Mechanism | Replace production `authenticate_blob`'s two full incremental-BLOB opens plus the range's third open with one borrowed row BLOB or one bounded owned canonical object buffer. Hash the whole bytes first, retain any parser error until identity succeeds, validate grammar/metadata, and copy only requested `r`. Avoid `load_object` revalidation. |
| Target paths | Public `load_object`, `read_object_range`, future production mapping/chunk traversal. Selected benchmark path already has this shape for chunks. |
| Complexity | Per object from `2b + r` BLOB bytes (+ later in-memory revalidation for load) to `b + r` hash/copy work, one query/row acquisition. |
| Measured ceiling | No production-path retained reconstruction wall. The selected path's 59.404-ms acquisition family is not attributable to these duplicate opens. |
| Predicted gain | At most nearly 2x object-byte acquisition/auth work on the old public API, but whole-operation gain unavailable until Canonical-v2 production integration. |
| CPU | One fewer full grammar/read pass and one fewer later validation; full identity hash retained. |
| Memory/Q | Borrowed row lifetime preferred for <=current object bounds; otherwise one bounded canonical buffer + range output. Charge exact capacities. |
| Storage | None. |
| Authority | Preserve identity-before-grammar errors, meta kind/length checks and immutable row authority. |
| Durability | Read-only. |
| Identity/format | Format-preserving; public implementation migration only. |
| Cross-operation effect | Read/range improve; writes unchanged. Borrowed-row callback constrains connection/statement reentrancy and cancellation. |
| Experiment | Unit-falsify first: corrupted identity + malformed grammar combinations must preserve exact typed-error precedence. Then one 10-MiB copied-DB range/load A/B under lock; kill if BLOB/query/copy counters do not fall exactly or any semantic test differs. <=120 s. |
| Evidence | Production code lines 912-1018 and 377-380; official SQLite/rusqlite APIs linked above. |
| Disposition | **LATER / production-integration repair, not a G4 performance candidate for the accepted path.** |

### E. Capacity-bounded content-addressed protected native seed plane

| Field | Required content |
|---|---|
| Mechanism | Keep Canonical-v2 authoritative. Add a derived cache keyed by `(profile, namespace/file root, length, raw digest)` whose entries are exact-verified read-only seeds. Enforce allocated-byte capacity `K`, LRU/clock eviction, corruption rebuild, and no per-revision reservation. A long-lived protected broker may retain unlinked descriptors and pass duplicated read-only descriptors to clients; persistence across broker restart requires full revalidation. |
| Target paths | Trusted full read, same-root clone materialization, incremental clone/patch, future VFS; cold/current-store miss still uses canonical traversal. |
| Complexity | Fill/rebuild `Theta(A + S)` plus `S` write/readback; read hit `Theta(S)`; clone hit metadata span + changed bytes; index `O(number of seeds)`. |
| Measured ceiling | G3 seed-hit walls 0.994 ms no-op and 3.414 ms one-byte show clone upside only. Trusted-read and seed-fill walls are unavailable. |
| Predicted gain | Clone hits retain G3 order-of-magnitude path. Trusted read may target <=50 ms. Cache miss cannot beat existing fallback and is worse if fill/readback is synchronously required. |
| CPU | Hits avoid SQLite/mapping/canonical hashes; full logical read still copies `S`; fill pays all auth plus raw hash/readback. Broker CPU/concurrency must be measured. |
| Memory/Q | Bounded stream buffer + bounded index; application RSS target <=20 MiB. Cache `K` is persistent allocated space, not Q. |
| Storage | Hard `K` cap, e.g. user-configured; steady state `<=K + index`, independent of 10/100/1,000 revision count. Eviction must consider shared clone extents and report apparent vs allocated bytes. |
| Authority | Same-open protected descriptor/broker state. Root/receipt is only lookup identity. Restart, rollback, same-UID mutation, descriptor substitution and replay need explicit treatment. |
| Durability | Cache is rebuildable and not canonical. Cache-index update can be best-effort only if stale/missing entries fail closed to rebuild. Native destination publication remains G3 contract. |
| Identity/format | Canonical format unchanged; major lifecycle/security/API architecture change. |
| Cross-operation effect | Hot reads/materialization improve; create/edit remain canonical. Fill may add latency/storage; evictions can destroy hit rate. VFS becomes natural consumer. |
| Experiment | Static first: simulate LRU allocated-byte occupancy for 10/100/1,000 roots with duplicate-root/reuse traces; falsify any claim of revision-independent space unless cap is enforced. Performance screen is Candidate C plus clone row. |
| Evidence | G3 authority/seed source and retained rows. The Nix store's official verifier recomputes content hashes to detect corruption, a precedent for treating cache verification as real work rather than trusting names ([Nix `store verify`](https://releases.nixos.org/nix/nix-2.34.8/manual/command-ref/nix-store/verify.html)). |
| Disposition | **LATER ARCHITECTURE / top disruptive system candidate.** |

### F. SQLite-resident immutable extent payload layout with object locators

| Field | Required content |
|---|---|
| Mechanism | Preserve canonical object IDs and mapping graph, but pack immutable canonical frames into bounded SQLite extent BLOB rows and add transactional `ObjectId -> (extent,row offset,length,kind)` locators. Batch sequential reconstruction by coalescing adjacent locators. Keep catalog and payload in one SQLite transaction, avoiding a second payload file. |
| Target paths | Full reconstruction/materialization and scrub; range uses locators for only selected chunks; create/edit append new objects; reopen/catalog validation changes. |
| Complexity | Full payload acquisitions from `C=5,284` BLOB row values toward `E` bounded extent ranges, while canonical hashing remains `Theta(A)`. Range remains `O(H + J + R)`. |
| Measured ceiling | Current selected path's **entire** SQLite/BLOB acquisition family is only 59.404 ms (17.87% of G2 instrumented parent), a gross maximum. Existing leaf batching already reduces query calls to 83 despite 5,284 rows, so query count alone is not a sufficient mechanism. |
| Predicted gain | Speculative upper bound <59.404 ms unless packing also improves cache locality/other folds. A 2x full-read claim requires independent evidence and cannot come from query-count arithmetic. |
| CPU | Per-object canonical hashes remain; frame splitting/locator decoding added. Compression excluded by current evidence. |
| Memory/Q | One bounded extent window (for example <=1 MiB) plus mapping page; must remain RSS <=20 MiB and exact Q bounded. |
| Storage | One physical canonical payload copy plus locator metadata; target metadata <=5%. Append holes/GC and SQLite vacuum/page fragmentation require 10/100/1,000-revision accounting. |
| Authority | Each extracted canonical frame still hashes to its `ObjectId`; extent checksum may detect container damage but cannot replace object identity. Locator and extent row commit atomically. |
| Durability | Single SQLite FULL+DELETE transaction avoids a catalog/external-value-log two-file commit, but extent compaction/GC crash ordering is new. |
| Identity/format | Schema/storage-profile version; canonical object and mapping IDs can remain stable. Migration and downgrade rejection mandatory. |
| Cross-operation effect | May improve sequential reads/scrub; creates buffer/append locators; edits append few objects; range could regress if an entire large extent must be copied/authenticated. Protect <=5% gates. |
| Experiment | Before format work, a <=120-s 10-MiB disposable SQLite microtrace compares current 64-row leaf query against one packed extent row with identical per-object hashes and exact row/BLOB/copy counters. Kill unless acquisition wall improves >=2x and 1-MiB range reads no more than 1.05x canonical bytes/current wall. |
| Evidence | G2 exact acquisition ceiling and current `for_each_leaf_bytes`. Xet's primary documentation describes grouping chunks into bounded xorbs and reconstructing files as ordered `(xorb hash, chunk range)` terms that can coalesce contiguous retrieval ([Xet hashing](https://github.com/huggingface/hub-docs/blob/main/docs/xet/hashing.md), [upload protocol](https://github.com/huggingface/hub-docs/blob/main/docs/xet/upload-protocol.md), [file reconstruction](https://github.com/huggingface/hub-docs/blob/main/docs/xet/file-reconstruction.md)). This is precedent, not local proof. |
| Disposition | **LATER VERSIONED STORAGE PROFILE; low ceiling unless falsifier is strong.** |

### G. New proof-carrying large segment profile (Bao-style)

| Field | Required content |
|---|---|
| Mechanism | Add an authenticated segment/extent tree or outboard proof representation so a selected range can be verified without hashing an entire large object; possibly keep small canonical chunks for COW and use a derived segment representation for sequential reads. |
| Target paths | Cold/warm full read, materialization, range, VFS; create/edit build/maintain proof metadata. |
| Complexity | Full remains `Theta(S)`; proof range could approach `O(R + log S)` authenticated bytes; two-representation fill remains `Theta(S)`. |
| Measured ceiling | Range is already 2.279 ms; no proof bottleneck. Full authentication is 94.817 ms, but large proofs do not eliminate hashing all returned full-read bytes. |
| Predicted gain | No defensible local gain yet. Metadata and build CPU can easily consume the protected range/create budget. |
| CPU | Extra tree construction and proof verification; potential parallelism not assumed. |
| Memory/Q | Bounded proof path possible; construction queues must be bounded. |
| Storage | Outboard/tree metadata and possibly duplicate segment payload. Must remain <=5% or be optional/capacity-bounded. |
| Authority | New root must commit proof encoding, segment ordering, lengths and profile. Ordinary BLAKE3 digest alone is not an exposed random-access proof. |
| Durability | New objects/profile participate in existing transaction; derived proof cache may rebuild. |
| Identity/format | Breaking canonical/profile change unless purely derived. Full migration/goldens/downgrade contract required. |
| Cross-operation effect | Risks create/edit/range regression to solve a full-read hash cost that remains linear. |
| Experiment | Static proof-size/build model first, then <=120-s 10-MiB proof build + 1-MiB verify versus current range. Kill on >5% range/create projected regression without >=2x full-read evidence. |
| Evidence | The BLAKE3 primary specification defines an internal 1-KiB tree, but a normal digest does not by itself encode range proofs ([BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex)). Bao explicitly requires combined or outboard tree encoding for verified streaming/slices ([Bao specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md)). |
| Disposition | **DEFER / no G4 role.** |

### H. Trust root/receipt/path metadata instead of authenticating fetched bytes

| Field | Required content |
|---|---|
| Mechanism | Skip canonical hashes or seed-byte protection because the expected root, receipt, inode, mode, mtime, watcher or filename matches. |
| Target paths | Would superficially speed reopen, read and materialization. |
| Complexity | Can remove `Theta(A)` work only by removing the byte authority. |
| Measured ceiling | 94.817-ms canonical-auth family, but it is required work on current-store bytes. |
| Predicted gain | Invalid because correctness/security contract changes. |
| CPU | Lower only by unauthenticated reads. |
| Memory/Q | Irrelevant. |
| Storage | Irrelevant. |
| Authority | None against later mutation/substitution/rollback; receipt authenticates metadata state, not mutable native bytes. |
| Durability | Cannot safely reconcile corrupted/substituted output. |
| Identity/format | Silent semantic downgrade. |
| Cross-operation effect | Reintroduces G3 Attempt-A failures and same-UID mutation risks. |
| Experiment | No performance experiment is admissible. Static adversarial byte mutation with unchanged or replayed metadata falsifies authority. |
| Evidence | Receipt fields in validation source; G3 v11 reaudit and retained G3 authority contract. |
| Disposition | **REJECT.** |

### I. Increase canonical chunk size to reduce object count

| Field | Required content |
|---|---|
| Mechanism | Select a new CDC/object profile with materially larger chunks, reducing `C`, row values and mapping objects. |
| Target paths | Full reconstruction/materialization/create; negatively affects edits/ranges/patch proof granularity. |
| Complexity | Full still `Theta(S)`; fewer objects/constants. Range authenticated bytes `J` and edit replacement amplification grow with chunk size. |
| Measured ceiling | BLOB acquisition 59.404 ms plus small mapping/occurrence budgets; canonical hashing 94.817 ms remains byte-linear. Current range/G3 patch rows are already strong. |
| Predicted gain | Insufficient for a 2x full-read claim and likely violates <=5% protected range/edit gates. |
| CPU | Fewer hash initializations/queries, same payload bytes. |
| Memory/Q | Larger maximum borrowed/owned object buffer; could exceed existing bounds. |
| Storage | Similar payload, smaller mapping; changed-object churn can grow. |
| Authority | Whole-object IDs remain sound. |
| Durability | No direct change. |
| Identity/format | New mapping/chunk profile and migration; content IDs/roots change. |
| Cross-operation effect | Range and incremental materialization authenticate/rewrite more bytes; COW locality worsens. |
| Experiment | 10-MiB boundary replay/model with 2x chunk parameters; kill if one-byte/1-MiB authenticated changed bytes or range bytes exceed 1.05x without >=2x BLOB acquisition improvement. |
| Evidence | Retained S1-100 has 5,284 chunks and 2.279-ms range; CDC/locality and pipeline reports in source appendix. |
| Disposition | **REJECT FOR G4; reopen only with a versioned profile and cross-operation proof.** |

## Predictions without double counting

### Logical reconstruction

Using the **G2 control center**, not the later 338.776-ms lifecycle row:

```text
closure-free but raw-digest-retaining gross ceiling
  = 328.897052 - 88.483070
  = 240.413982 ms
  = 415.95 MiB/s

closure-free and raw-hash-free computational floor before real delivery
  = 328.897052 - 88.483070 - 87.889943
  = 152.524039 ms
  = 655.63 MiB/s
```

The second value is a **speculative upper bound**, not a reconstruction result:
the current raw fingerprint is the logical benchmark's sink, so a real caller
buffer, VFS request, socket, or native file must add/overlap actual output
delivery. The 338.776-ms current retained row cannot be mixed with G2 component
medians without a new paired decomposition.

### First/full native materialization

No valid numeric prediction is available. Its minimum work is canonical
acquisition/authentication plus 100-MiB native output and durability. Existing
G3 fallback already excludes closure; subtracting 88.483 ms would double-count
an absent cost. Subtracting the 87.890-ms hash-only sink from an unmeasured
write+sync path would also be invalid. The honest G4 hypothesis is only:

```text
warm first-full objective:       <=400 ms (acceptance), <=333 ms (stretch)
controlled-cold first-full:      <=500 ms (acceptance), <=400 ms (stretch)
```

### Trusted-seed read

The bandwidth arithmetic is exact, the feasibility is not:

```text
100 MiB / 50 ms = 2,000 MiB/s
100 MiB / 35 ms = 2,857.14 MiB/s
```

Only an exact descriptor-read measurement with CPU/RSS/Q/block counters and a
cache-state label can answer it. Process restart does not create controlled
cold state.

## Smallest prospective falsifiers (all <=120 s)

These are experiment designs, not executed rows. Each requires the lead's
fail-fast benchmark lock before any timing starts. Each uses a disjoint
`/tmp/layerfs-g4-r1-reconstruction-*` namespace and must delete that exact
namespace after hashing any compact raw result.

| ID | One variable | Input | Direct counters / oracle | Retain rule | Kill rule |
|---|---|---|---|---|---|
| `R1-FALLBACK-10` | existing complete fallback only | frozen G3 executable + exact 10-MiB fixture/profile | route, SQL rows/queries/BLOBs, authenticated/source/write bytes, Q, RSS, CPU, sync/rename/cleanup, outside-timer exact output hash | retain as diagnostic only if exact shape and zero residue; never promotes scalar walker | timeout, counter mismatch, hidden preparation, wrong route, residue/unbounded memory, or labeling it accepted M0 |
| `R1-BATCHED-PROOF-10` | scalar G3 control vs proof-preserving batched sink | copied private selected-profile source, exact 10-MiB fixture/base | identical root/output/digest/closure/sequence/errors/auth; leaf batch/query/row/BLOB/Q counters | all proof outputs equal and query shape returns to accepted batching without >5% Q/RSS regression | any proof/error difference, per-chunk query remains, or timing improvement comes from omitted folds |
| `R1-CLOSURE-10` | only after batched M0 passes: closure fold enabled vs versioned external policy; same raw digest sink | copied private selected-profile source, exact 10-MiB fixture/base | identical root/output/errors/auth/SQL/Q; external exact oracle | semantic equality and >=noise-gate wall improvement under explicitly changed contract | any authority/error/counter difference, silent M0 proof deletion, or <gate improvement |
| `R1-SEEDREAD-10/100` | digest policy off/on on protected descriptor | G3-qualified exact seed; 10 MiB then at most one 100 MiB | descriptor identity, exact bytes, bytes read/delivered, buffer Q, user/system CPU, RSS, block ops, independent oracle | 100-MiB no-digest <=50 ms, bounded state, exactness | clone used, cache state mislabeled, >50 ms, RSS >20 MiB, unprotected pathname |
| `R1-PROD-BLOB-10` | current 2/3-open production read vs fused row acquisition | copied disposable SQLite DB | exact BLOB opens/bytes, queries/rows/copies, typed-error matrix | opens become one and byte work becomes `b+r` without semantic difference | selected benchmark cited as evidence, error precedence changes, or no exact counter reduction |

### Ledger-ready no-experiment block

```yaml
lane: reconstruction
experiment_count: 0
hypothesis: not_applicable
commands: []
utc_start: not_applicable
monotonic_start_ns: not_applicable
utc_end: not_applicable
monotonic_end_ns: not_applicable
wall_ns: 0
custody:
  repository_head: 5c342f0ae24ecc69f2bfc03da1c05d1074fe956a
  experimental_source_copies: []
  experimental_binaries: []
raw_results: []
cleanup:
  transient_namespaces_created: []
  retained_bytes: 0
  transient_peak_bytes: 0
resource_model: no experiment; read-only source/evidence inspection only
unsupported_observations:
  - trusted_seed_full_read_wall
  - first_full_100_mib_native_materialization_wall
  - controlled_cold_reconstruction_or_materialization
  - physical_io_bytes
  - stable_media_completion
reason: timing-sensitive probes require lead coordination and benchmark lock; the decisive first rows belong in the prospective G4 contract
```

## External architecture comparisons

- **Git:** Git stores framed content-addressed objects and builds higher-level
  trees from object names, a precedent for transitive graph identity rather
  than an extra whole-checkout digest. It does not prove LayerFS's local
  timings or authority policy ([Git user manual, object database](https://git-scm.com/docs/user-manual#the-object-database)).
- **BLAKE3/Bao:** BLAKE3's tree construction permits parallel/streaming hash
  computation, but its ordinary digest does not expose authenticated arbitrary
  slices; Bao supplies combined/outboard tree material for verified streaming.
  That extra representation is real storage/build work, not a free property of
  the current IDs ([BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex),
  [Bao specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md)).
- **Xet:** Xet's documented xorb/chunk-term reconstruction supports the idea of
  physical aggregation plus ordered range terms, but LayerFS already batches
  5,284 chunks into 83 leaf queries and has a strong 1-MiB range. A local
  acquisition falsifier is required before adopting that layout
  ([Xet file reconstruction](https://github.com/huggingface/hub-docs/blob/main/docs/xet/file-reconstruction.md)).
- **Nix:** Official store verification recomputes stored content hashes. This
  supports counting persistent seed/cache revalidation as genuine work and
  rejecting names/metadata as sufficient byte authority
  ([Nix `store verify`](https://releases.nixos.org/nix/nix-2.34.8/manual/command-ref/nix-store/verify.html)).

## Source-hash appendix

Every local file semantically inspected for this lane is listed below. Line
citations in the body identify the decisive passages; hashes bind the complete
files read. Generated target evidence is retained/read-only.

### Governing status and retained G2/G3 evidence

| SHA-256 | File |
|---|---|
| `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00` | `implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md` (pre-existing untracked, read-only) |
| `03ca46e7772c63a9f39eaa50275edd82a0e5ece50fc1c0aff00b4a21bd8db304` | `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md` |
| `a5dc635898e53939e34e135471bffc22d6361babeb7d90a48e38678f4a67c830` | `implementation-detail/phase-4/README.md` |
| `0cafb37d4d44659d226dae51d8ae7243612e628b4b3f943c540992393668d1de` | `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md` |
| `b94a638bc94be43f25d7e9b30248d93dcfc35d7170f6f85673389706f5695056` | `implementation-detail/phase-4/baseline/g3-incremental-materialization-baseline-v1.md` |
| `5748a36b9be0e2d21771483b1bc838804d47bc95801681df0863cb7c40caf462` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md` |
| `8226aacee217a58436b2c8405d953ee18882e5ad400662f1004368a91a26dae5` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md` |
| `13d7bd160b730285ba4457fcabc0107c8064ed6c63bdf9a1cfc84e275596e2c8` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/V12-PREEXEC-REVISE.md` |
| `39a081a185aa4560e60f5d6a862c47e0f13d9ac2d67ac769f6676a1238f8ecf8` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v12.md` |
| `70a8fedfa97a03ea56031cb06b033593d1595b7558c986ee625deab40ea33fee` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v13.md` |
| `8809034ee8fff0013eb622799a9c676e14c8a102ec5557172f121d7a0434fe58` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/COUNTER-DICTIONARY-v13.md` |
| `1aa960ce75bae2a69ae3f3f73b4e1b2cbe01baad841b5095714890118491e915` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/run_g3_v13.py` |
| `b1121f44b29d991f7212153e4a26c841db320045bab5a57e604919bafd677c33` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/analyze_g3_v13.py` |
| `146c73f6adc43c3de00c8a1d14ad77b7ec83732d858c0afb524df2a8a46fd6c5` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/recompute_g3_v13.py` |
| `b0c11b720e9d1aa56e66c4eded6ac37c5525ad7652b1f083c733d00dfe199006` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/finalize_g3_v13.py` |
| `558ee9b0d47d5653f47860cf717c23e9b96ae674fa5c449d60b17b3efd3fe6f5` | `implementation-detail/phase-4/experiments/g3-incremental-materialization/PREMEASUREMENT-FREEZE-v13.json` |
| `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/rows-v13/G3-V13-RAW.jsonl` |
| `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-PRIMARY-ANALYSIS-v13.json` |
| `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-INDEPENDENT-RECOMPUTATION-v13.json` |
| `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CLEANUP-v13.json` |
| `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json` |
| `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv` |
| `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json` |
| `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` | `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt` |
| `a85419e73f6aefa701028b2192cf49682ef14e403d6d914239b719f077e12cce` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/G2-REVISE-REPORT-v1.md` |
| `d778012b2d85006111eb31863ad4ea2c8e8fb1cf848a4d784a36130c317a00e6` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/PROSPECTIVE-G2-MATERIALIZATION-DECOMPOSITION-v5.md` |
| `bd1689dd26102f4bb67081141583cfb08d6e246f8aa5e3dcc541a34f653e8811` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/METHODOLOGY-MANIFEST-v5.tsv` |
| `54df9fc45dda98bbf085b6501780a22868c5bd61f121aa96389757bfd3b78958` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/run_g2_v5.py` |
| `e3a62da24eaa9020e5763ee48f433bf3fb302f9003e65712f91322181efa6d9c` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/analyze_g2_v5.py` |
| `5c3b37b381966af806b540bf0d06092ba37853cd01cd6cd6db30dbc68076507f` | `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/recompute_g2_v5.py` |
| `c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/rows-v5/G2-V5-RAW.jsonl` |
| `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-ANALYSIS.json` |
| `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-INDEPENDENT-RECOMPUTATION.json` |
| `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/PAYLOAD-MANIFEST-v5.tsv` |
| `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-v5.json` |
| `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` | `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-VERIFICATION-v5.txt` |

### Research, specifications and lifecycle contracts

| SHA-256 | File |
|---|---|
| `3cb890cc34cf3667944482294a41bad4120e8bd3e7c86ebfdd09385b26b22429` | `research/phase-4/handoffs/hot-cold-materialization.md` |
| `03f07d8337f346a411ed6138753dd8dc73781d191d8fdd9a35e0d8fc46341461` | `research/phase-4/assurance/verification-security-resources.md` |
| `c9a25b681fb5f15555adec5e356651fae06ce3cc8b075ebd617b7840a524c285` | `research/phase-4/foundations/invariant-matrix.md` |
| `62d385cd7a7245429326e7a9f6f6ba053c30fcbdf322b7fa0cabd10bfe9007a2` | `research/phase-4/foundations/benchmark-and-evidence.md` |
| `1d4b3bb83f9dbb43d66e10702b946cb8f8dddc39c6c1faae00187ea4e4b6c2f9` | `research/phase-4/foundations/hypothesis-ledger.md` |
| `8ddb236ff7d3cfa03257c9006d8b6f219b151f7433a331b4f2b9ea900c0c30fb` | `research/phase-4/decision-map.md` |
| `8b9b1fa13e56aed1b754da6b4b1dfe38d740199a0bded3b652fb3130ce824cd9` | `research/phase-4/core/canonical/canonical-v2-exploration-findings.md` |
| `36cbd3f973532768a44f6e11d9a9162c28898cad62c829d2a367da8ad14ae69e` | `research/phase-4/core/canonical/canonical-v2-exploration-preregistration.md` |
| `261ca204466438d69b0d2dfd96cb517c86145abff6440381cfcb749c9935f2bf` | `research/phase-4/core/canonical/h05-terminal-findings.md` |
| `ce947becfe9105a5df58888314ead2491f17ff1ca5842cd78f45302ab18efdb6` | `research/phase-4/core/canonical/identity-and-hashing.md` |
| `0857d7633bfa8f8d7831087be4cea30479a9092553f9e08058528be593ac3cd7` | `research/phase-4/core/canonical/v2-single-identity.md` |
| `daabf94a31a5613e1cf78fbaef1d46f3d8395fb3bc94c2fdbba6fdaf02a4be8d` | `research/phase-4/core/pipeline/full-create-pipeline.md` |
| `49c20e7404248f5dcc461271f3f829d0e2a97469c1b6fb97a0ef4c071630a6dd` | `research/phase-4/core/cas/authenticated-reuse.md` |
| `6e3935dae62b735c015f8feef09ddae49829f525bc6ce6a7e92e806f2cd13ba5` | `research/phase-4/core/cdc/locality-and-algorithms.md` |
| `b48facb78eb05cd5d11b330e990a6fcc11b88d595dbe34e9d5f4d9ed207ee2ca` | `research/phase-4/core/cow/mapping-and-deltas.md` |
| `d5160bc38e9fb24601ec936e1ec46a0a0c81d06ff6f803f26534ca67c16d2815` | `research/phase-4/storage/compression-and-packing.md` |
| `12053708d794fa9737b3c388d1ae74887e4267b0b1334d3b654430c9ea1b3a3e` | `research/phase-4/storage/sqlite/durability-and-layout.md` |
| `c27b6cb030aac3edaf4ed949498139c01a9ec94738f3f3c7b8d7d2041d356443` | `implementation-detail/phase-3.md` |
| `067f4107b886a504511475f0977b269016d233b6186a0de70b1a5681460c46c3` | `implementation-detail/evaluation.md` |
| `67202cac261e401e103fe74143f7346fda3f2250ec6ede7fcf3e54016dc74fbf` | `implementation-detail/phase-4/algorithm/spec.md` |
| `a8e65a188e4f5904c347f01d9bd65022c057c2348cf4d0350d8089f32a6e5fdf` | `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md` |
| `c6a44fda3286b2e7e38b905f0336757563aec815068a23745011f0ec9b1c550b` | `implementation-detail/phase-4/algorithm/complexity-analysis.md` |
| `a69569a36b76f2b5763991f11227d4e193dddbaeec9a828f7a0c922df672179a` | `implementation-detail/phase-4/mapping/logical-persistence.md` |
| `e05948b677a72cff9c2dda08016cb430ede5e1508cbf519af53ebb32bc8c7eee` | `implementation-detail/phase-4/wp4m/f-series/planning/retained-100-mib-lifecycle.md` |
| `256856fb1c0e0376abb56a83b229a71347ab5e0bd129f814c1c03dc0b4770bc9` | `implementation-detail/phase-4/storage/sqlite/spec.md` |
| `143ca5336169e8f7387a7e9075cdb11ac557eb0c8a5b067aeb690e1ba421effb` | `implementation-detail/phase-4/storage/sqlite/implementation-plan.md` |
| `8340011e0d9fe41834856a8e418c018a2911f25cd5a34e3788f0b58e87265c53` | `implementation-detail/phase-4/storage/sqlite/visible-head.md` |

### Implementation and dependency sources

| SHA-256 | File |
|---|---|
| `9475d9d32d2e59cdf7b8a5f9cc3e35ecf3c58e47152fcfbf96c7a8b896eeaadb` | `crates/layerfs-engine/src/lib.rs` |
| `c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb` | `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` |
| `f9ffe7058761c60e7d81c5da18ed3d7a9afdb5344f41b9a97dcb8c2b8a51f032` | `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs` |
| `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` | `crates/layerfs-core/src/canonical_v2.rs` |
| `53a4effd5ccafedb649ad9c151e6ee7115958f5b9b4e5128f8c835518d3dd319` | `crates/layerfs-core/src/cas/mod.rs` |
| `beb8637ea160f5b61401c0dec2b632927c81be0b491b443142973dc23108edb5` | `crates/layerfs-core/src/cdc/gear.rs` |
| `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` | `crates/layerfs-core/src/cdc/mod.rs` |
| `0969881a415f8bd4f4e1574170f8ee869b15145b215fad2c9a86dc0102ad6c9e` | `crates/layerfs-core/src/content/mod.rs` |
| `5b7831aa493e84aa77db274c1ac87db70b709a406e8241d7a665c6cefcf287fa` | `crates/layerfs-core/src/content/persistence.rs` |
| `4043d8390cb9b86d4584340dc8c9929bb07720a978e47ac688b72e502424d657` | `crates/layerfs-core/src/cow/mod.rs` |
| `59c22e102f235831e7ff5c12f119553c084044831199d015aaa53f57f88767fa` | `crates/layerfs-core/src/cow/mutate.rs` |
| `e2a25b67f7ee17a78a33aa0318bfcbcf020a5162b6670df8743941d282d65d56` | `crates/layerfs-core/src/cow/persistence.rs` |
| `de3171a54ac9eb4c16be834d51e0b1636009529316e04703a67def3a335e48c7` | `crates/layerfs-core/src/cow/tree.rs` |
| `e601dfcc561188d58d6cbb41d4ad0b606501995bce04e366afb601a7ba0f5c61` | `crates/layerfs-core/src/delta/codec.rs` |
| `c417e08dc2b6ecb39dc8371ccc5517780f948924425d33921b1036f725c46b1e` | `crates/layerfs-core/src/delta/mod.rs` |
| `8d22dbf8216da6cb2d88c3e067d41724d6dddaa0007a65cf5cbc5b9923151ce7` | `crates/layerfs-core/src/identity/digest.rs` |
| `4e6fe13f99abc20d0395c8e95de937614070f7d7bf7e3027d52259990927f54c` | `crates/layerfs-core/src/identity/ids.rs` |
| `bd43ccb083a0b4659fc5303469983e928fecfc5707b596cf592163ad50ba744f` | `crates/layerfs-core/src/identity/mod.rs` |
| `513596fffcd7dca5f63fd0d86a9df6376e6794ee350c137eb6d786bba2c74659` | `crates/layerfs-core/src/object/codec.rs` |
| `1566a7de1146962d6b189daf39fe1167282d0d22305cebe840183d1533228659` | `crates/layerfs-core/src/object/mod.rs` |
| `fe6cb9e79d3d9aa16cc82896015d3a0765fb542be5a333a2f5d74f47e42801ae` | `crates/layerfs-core/src/object/model.rs` |
| `f42eb13125cc19ecfc3e4567d35926b2871cd65b46d9f0af985c5a1782f02a5e` | `crates/layerfs-core/src/validation.rs` |
| `13866474b3b8387e06d9c501c533c3067100eb573654ed2b0912292847d94996` | `crates/layerfs-os/src/lib.rs` |
| `20de55cdbe636b2219d7eaa60bc703b126bb18b77f17d35c137ba0228ee75849` | `crates/layerfs-vfs/src/lib.rs` |
| `7bdcac0987a591841ce31d17134e040eef651335abc550ffec1b3d1971c01210` | `crates/layerfs-sdk/src/lib.rs` |
| `dbcb7eeb7672bdd5e8bb8ece8d238879e867b6f7f343ddfed50e20f807760621` | `Cargo.toml` |
| `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` | `Cargo.lock` |
| `7104453012be05e2e9c9baa870dfba01c1a8ca321ac9b628649926437032849c` | `crates/layerfs-core/Cargo.toml` |
| `35fd9c667575fdb3dd6ae720c4c43e6c654a9fd47da8b5dadc9f7672bd04498d` | `crates/layerfs-engine/Cargo.toml` |
| `ee7387a8858d3900792b424c77153a291983885a361a2c3e12128c5aa7cea21d` | `crates/layerfs-os/Cargo.toml` |
| `e6868b66f840e56c3614e7da13e6ea099b2b4a9de15e15c0d1d4d42708ffd27d` | `crates/layerfs-vfs/Cargo.toml` |
| `e3c94ac5a46873b7a3d3b91e123bf6950f8ba589ff333ea0b5928e153f818fdd` | `crates/layerfs-sdk/Cargo.toml` |
| `ca0d543ee7004db58d365bdf45708aa59d9c81ef20e20b4da40ab3eea6aaa492` | `/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/row.rs` |
| `11c83bb35ec8617b405c844d6f31f0bbd43c9f2de668a7cc65179ffbc6ff27c0` | `/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/blob/mod.rs` |
| `ba6a1f3d61ecb6bd79d8bf67193a6eaf8754673d5a5a01fbee82c949fb3c6a0f` | `/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/blob/pos_io.rs` |
| `7ccb0e379a067ded70ae6754209e3e824bc465cc260bca2f3e57245a65082562` | `/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/types/value_ref.rs` |

Cargo binds `rusqlite = 0.40.2` with `cache`, `blob`, and `hooks`, and BLAKE3
through the workspace. The selected benchmark's schema/profile is private and
production-shaped; it is not yet the public engine integration.

## Ranked recommendations

1. **DO NOW / G4:** Measure the existing G3 complete fallback only as a
   diagnostic native control, with explicit scalar-query/proof labels. Do not
   subtract the absent closure fold, and do not promote this path even if its
   wall is attractive.
2. **DO NOW / G4:** Build/measure the promotable format-preserving M0 as one
   canonical-authenticated **batched** traversal into a bounded native sink,
   preserving current closure, occurrence-sequence, raw-digest, error and
   topology results. It must retain the 83-leaf-batch/170-total-query class.
   Only a later explicit contract variant may separate optional closure/oracle
   timers after M0 equivalence is proven.
3. **DO NOW / G4:** Add a true trusted-seed **read** row distinct from clone
   materialization. Require exact returned bytes, <=20-MiB RSS, bounded Q, CPU,
   block counters and honest cache labels; 2–3 GiB/s remains a hypothesis.
4. **LATER PRODUCTION INTEGRATION:** Fuse the public engine's duplicate BLOB
   opens/validation passes while preserving identity-first errors. Do not cite
   this as a retained-path G4 speedup.
5. **LATER ARCHITECTURE (top disruptive recommendation):** Adopt Canonical-v2
   durable truth plus a capacity-bounded, root-keyed protected native seed
   plane serving reads, VFS, clones and patches. Promotion requires measured
   fill/rebuild cost, allocated-byte eviction, corruption recovery, and a real
   cross-process authority design.
6. **LATER VERSIONED STORAGE PROFILE:** Explore SQLite-resident immutable
   extent packing only if a 10-MiB falsifier demonstrates >=2x acquisition
   improvement without >5% range/create/edit degradation. Its current gross
   wall ceiling is only 59.404 ms.
7. **DEFER/REJECT:** Defer Bao-style proof extents until a real range/proof
   bottleneck appears; reject root/receipt/path metadata as later-byte
   authority; reject larger canonical chunks for G4 because they trade away
   already-fast ranges and incremental patches for a limited acquisition
   ceiling.

The cross-review question for the materialization/core lanes is: **does either
lane have evidence that a capacity-bounded seed broker can retain exact
same-open authority across client processes without a full seed revalidation
after broker restart, and if so, what primitive—not path metadata or a
receipt—authenticates the actual bytes?**
