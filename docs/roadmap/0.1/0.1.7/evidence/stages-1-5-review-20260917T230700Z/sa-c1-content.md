# C1 content review — canonical objects, file content, edits (independent audit)

**Reviewer:** sa-c1-content (independent; not the author of the reviewed code)
**Tree:** branch `main`, HEAD `f288d2af7ecdc7e00f7df153073398d333461aa3`, tracked tree clean
(`git status --porcelain` shows only pre-existing untracked review/handoff docs).
**Scope:** `core/crates/layerfs-content/src/{object,file,policy,error}.rs` + the public seam
(`src/lib.rs` and every module `pub use`). `src/filesystem/**` is another reviewer's scope and is
touched here only where a C1 file references it (called out explicitly).
**Method:** read-only inspection of the product source and the external tests; `rg` sweeps; no
product/test/fixture file was modified and no git state-changing command was run. No cargo command was
executed by this review (see "Evidence used" below).
**Contract status:** `canonical-objects.md`, `finalized-object-handoff.md`, `content-io.md` and
`content-io-memory-audit.md` all carry `> **Status:** Proposal`, so they are claims, not shipped
contracts. Every "the contract says" sentence below is a requirement statement; every "the code does"
sentence carries a `path:line` and a verbatim quote.

**Evidence used for test status (not re-run by me):** the recorded receipt
`docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z/cargo-test.log` (same commit, 23:09Z)
records 60 test targets `test result: ok ... 0 failed`, including `edit_bounds` (12 passed, 16.13 s),
`edit_reference` (2 passed, 55.89 s), `streaming`, `file_complete`, `file_read`, `edit_noop`,
`edit_transitions`, `object_identity`. I did **not** reproduce that run; where I rely on it I say
"recorded receipt".

---

## Verdict summary

| # | Required check | Verdict | One-line reason |
| --- | --- | --- | --- |
| 1 | Framing, identity, auth boundary, duplicate codec | **PASS (2 notes)** | One envelope codec, domain-separated digest, authentication delegated to the provider; role is caller-asserted and `ObjectId` is also used for non-digest keys |
| 2 | `FinalizedObject` / `FinalizedConsumer` | **PASS (1 seam defect)** | Owned move, synchronous bounded consumer, no payload staging; `into_parts` silently drops the predecessor hints |
| 3 | Construction: empty/small/large, stability, validation | **PARTIAL** | Stability and explicit failures are good; the whole-file encoder does **not** write once (its own doc says it does), one public encoder allocates before any bound check, and C1 entry points never validate the policy |
| 4 | CDC frozen profile, boundary determinism, cost/byte | **PASS (coverage gap)** | Table/masks/seed frozen and hashed into `profile_id`; pair state is carried across reads; O(1)/byte. No test drives adversarial read splits |
| 5 | Mapping partitions, build, read | **PARTIAL** | No whole-file materialization behind the batch API; the declared wave byte ceiling is not enforced and the navigation frontier has no declared bound |
| 6 | Stage-4 edits | **PARTIAL** | COW reuse, coordinates, old-root immutability and no-op return verified; the base root is read twice per chunked edit, the write path validates every page as a **root**, and the no-op return skips the policy/representation check |
| 7 | C1-only operation (no SQLite/storage) | **PASS** | `Cargo.toml` = blake3 + telemetry only; direction is C2 -> C1 |
| 8 | Memory ledger | **PARTIAL** | Enumerated below; three allocation sites size buffers from unvalidated caller inputs, and container/index overheads are uncharged |

---

## 1. Canonical object framing and identity — PASS (2 notes)

### 1.1 Exact byte layout (verified against encoder and decoder)

Envelope (`object/codec.rs`): `LFSO` | `kind` u8 | `payload_len` u32 BE | `value_len` u32 BE | value.

- `object/codec.rs:14` `pub const OBJECT_MAGIC: [u8; 4] = *b"LFSO";`
- `object/codec.rs:16` `pub const BYTES_KIND: u8 = 1;`
- `object/codec.rs:18` `pub const HEADER_LEN: usize = 9;` (magic + kind + payload_len)
- `object/codec.rs:62-66` writes magic, kind, `payload_len.to_be_bytes()`, `value_len.to_be_bytes()`, value.
- `object/codec.rs:52-57` `let payload_len = u32::try_from(value.len() + VALUE_LEN_BYTES)...` — so
  `payload_len == value_len + 4`: the 4-byte value length is counted inside the payload.
- `codec.rs:88-90` reject short input; `:91-93` wrong magic; `:98-110` over-limit lengths;
  `:117-125` require `encoded_value_len == payload_len && canonical.len() == total` else `TrailingBytes`.
  Total width is `9 + 4 + value_len`, bounded by `MAX_CANONICAL_OBJECT_BYTES = 16 MiB` (`policy.rs:203`)
  with the field ceiling `MAX_OBJECT_FIELD_BYTES = 8 MiB` (`policy.rs:209`).

Role grammars carried as values (each with its own magic, so the envelope's single `BYTES_KIND` does not
identify a role — `canonical-objects.md:58-59` warns of exactly this):

| Value | Layout | Code |
| --- | --- | --- |
| whole file | `LFS5SML\0`(8) + version u16 BE = 1 (2) + raw payload | `file/content.rs:24-26`, `:92-94` |
| chunk | `LFS4CHK\0`(8) + raw <= 32768 | `file/mapping/codec.rs:20`, `:80-81` |
| mapping page | `LFS4MAP\0`(8) + version u16 BE = 3 + role u8 (LEAF 0x08 / BRANCH 0x09) + level u8 + flags u8 = 0 + count u16 BE + `subtree_logical_bytes` u64 BE + `subtree_extent_count` u64 BE + rows | `mapping/codec.rs:18-31`, `:122-153` |
| leaf row (40 B) | payload ObjectId(32) + `source_offset` u32 BE + `logical_length` u32 BE | `mapping/codec.rs:139-145` |
| branch row (48 B) | `cumulative_logical_end` u64 + `cumulative_extent_count` u64 + child ObjectId(32) | `mapping/codec.rs:146-152` |
| file state (93 B) | `LFS4MAP\0` + version u16 + 0x0a + 0 + `logical_len` u64 + `extent_count` u64 + `tree_level` u8 + `profile_id`(32) + `mapping_root`(32) | `mapping/codec.rs:28`, `:272-282`, `:301-315` |

`NODE_HEADER_LEN = 31` (`mapping/codec.rs:25`) = 8+2+1+1+1+2+8+8, and the decoder re-derives the expected
width from the role tag before reading rows (`mapping/codec.rs:193-210`), so truncation and trailing
bytes are both refused.

### 1.2 Digest domain separation

- `object/id.rs:16` `pub const OBJECT_DOMAIN: &[u8] = b"layerfs/object/v2\0";`
- `object/id.rs:24-28` `pub fn for_bytes(canonical: &[u8]) -> Self { ... hasher.update(OBJECT_DOMAIN); hasher.update(canonical); ... }`
  — identity is BLAKE3 over domain **plus the canonical bytes including the envelope**, not a raw
  payload digest (matching `canonical-objects.md:120-121`).
- The two `profile_id`s use their own domains and are **not** object digests: `cdc/gear.rs:33`
  `hasher.update(b"layerfs/fastcdc-profile/v1\0")` and `mapping/codec.rs:35`
  `bytes.extend_from_slice(b"layerfs/mapping-profile/bplus-extent/v3\0")`. Both hash every parameter that
  can change bytes (`gear.rs:35-46` includes all 256 table words; `mapping/codec.rs:40-47` includes
  `MIN_ENTRIES`, `MAX_ENTRIES`, `MAX_LEVEL`, the chunk ceiling and `cdc::profile_id()`), so moving a
  constant moves the recorded profile.

**Note 1a (type conflation, low):** the mapping profile digest is stored in an `ObjectId` field
(`mapping/types.rs:250` `pub profile_id: ObjectId,`, built by `ObjectId::from_bytes(...)` at
`mapping/codec.rs:48`), and the edit draft keys are synthesized `ObjectId`s from a tag plus a counter
(`file/edit/tree.rs:321-330`, tag at `tree.rs:62`). An `ObjectId` in C1 therefore does not always mean
"BLAKE3 digest of canonical bytes". Drafts are documented (`tree.rs:54-62`) and collision-safe, but the
type carries no provenance proof, and `tree.rs:155` (`self.drafts.get(&summary.id)`) resolves a draft
*before* asking the provider, so aliasing between the two key spaces would be silent. Not reachable
today; reported for completeness.

**Note 1b (role is caller-asserted, low-med):** `FinalizedObject::new` checks the envelope but not the
declared role: `object/output.rs:98-101` `pub fn new(role: ObjectRole, canonical: Vec<u8>) ... codec::decode_bytes_object(&canonical)?; let id = ObjectId::for_bytes(&canonical);`.
A caller can label a chunk object `ObjectRole::WholeFile`. `canonical-objects.md:81-82` says "A trusted
constructor must actually validate the fields it passes; caller-asserted metadata is not proof." In-crate
callers pass consistent roles; C2 receives the role as an assertion. The same applies to
`encode_whole_file_payload` (see 3.3).

### 1.3 Authentication boundary

- The trait states the provider contract and the one error that proves it: `object/access.rs:25-28`
  "Implementations must return the canonical object bytes whose identity is `ids[index]`... a provider
  that cannot establish identity must return [`ContentError::IdentityMismatch`] instead."
- C1 itself never verifies a returned object against the requested id. `file/read.rs:27-29` acquires
  `reader.read_canonical(root)` and hands the bytes to `content::classify`; `file/edit/tree.rs:902` does
  the same for the file state; neither recomputes `ObjectId::for_bytes`. `IdentityMismatch` is never
  constructed anywhere in C1's object/file/policy/error code — the only constructors in the crate are
  `filesystem/sorted/finish.rs:107` and `filesystem/sorted/page.rs:435` (out of scope) plus test
  providers.
- Cardinality *is* checked on every batch: `access.rs:52-57`, `mapping/read.rs:120-125`, `read.rs:236-241`.
  Authentication is therefore exactly one boundary — the caller's — as
  `canonical-objects.md:98-102` requires, and `README.md:73-75` says.

### 1.4 Duplicate codecs — none found in C1 (one stale prior finding)

- The envelope exists once: `layerfs-storage/src/encoding/full.rs:52` calls
  `layerfs_content::object::codec::decode_bytes_object(canonical)?` rather than re-implementing it; no
  second `LFSO` writer/reader exists under `core/crates` (rg sweep over `LFSO|LFS4MAP|LFS4CHK|LFS5SML|OBJECT_MAGIC`).
- The inode-leaf page grammar is single-sourced at this HEAD: `object/inode_leaf.rs:244`
  (`pub(crate) fn encode_leaf_value`) and `:282` (`pub(crate) fn decode_leaf_value`), with the sorted
  engine delegating at `filesystem/sorted/format.rs:376` and `:481`. The prior report row "two complete
  leaf-page codecs" (`stages-1-5-review-20260917T160000Z.md:1141`, id S1) is **stale** at this HEAD.
- The remaining three codecs (`object/codec.rs`, `file/mapping/codec.rs`, `object/inode_leaf.rs`) own
  disjoint grammars.
- Cosmetic aliasing hazard: role numbers exist twice — `ObjectRole::code()` gives
  `ExtentLeaf=3, ExtentBranch=4, FileState=5, DirectoryLeaf=7` (`object/output.rs:50-61`) while the
  mapping grammar uses `LEAF=0x08, BRANCH=0x09, FILE_STATE=0x0a` (`mapping/codec.rs:22-24`) and the
  inode leaf uses `const LEAF_ROLE: u8 = 7` (`object/inode_leaf.rs:467`, the same value as
  `ObjectRole::DirectoryLeaf`). Different grammars, so no live defect; the two numbering schemes are a
  reading hazard.

---

## 2. `FinalizedObject` / `FinalizedConsumer` — PASS (1 seam defect)

Exact signatures (`object/output.rs`):

- `output.rs:87-94` `pub struct FinalizedObject { id: ObjectId, role: ObjectRole, canonical: Vec<u8>, references: Vec<ObjectId>, predecessors: AdvisoryPredecessors }` — every field **owned**, all private.
- `output.rs:98` `pub fn new(role: ObjectRole, canonical: Vec<u8>) -> ContentResult<Self>` — takes ownership of the canonical allocation, computes the id once (`:101`), never trusts a caller-supplied id.
- `output.rs:112` `pub fn with_references(mut self, references: Vec<ObjectId>) -> Self` and `:118` `pub fn with_predecessors(...) -> Self` — builder moves, no clones.
- Borrowed views: `output.rs:134` `pub fn canonical(&self) -> &[u8]`, `:144` `references(&self) -> &[ObjectId]`, `:149` `predecessors(&self)`.
- `output.rs:154` `pub fn into_parts(self) -> (ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)`.
- `output.rs:163-166` `pub trait FinalizedConsumer { fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>; }` — one required method, owned argument, `&mut self`.

Backpressure: the handoff is a *synchronous* call, so the producer cannot run ahead of the consumer —
there is no queue and no unbounded buffer to bound. A refusal ends the operation with the consumer's
error and no resend (`output.rs:160-162`: "Returning an error ends construction: the caller receives
that error once and no retry, resend or alternative path is taken."), and callers propagate it
immediately (`file/content.rs:169-171`, `mapping/build.rs:132`, `edit/tree.rs:427`).

Release events: C1 keeps no payload copy after `accept` — `push_chunk` keeps only the id
(`mapping/build.rs:131-135`), `commit_node` keeps only the id (`edit/tree.rs:425-430`), and
`DiscardingConsumer` drops the object at the end of `accept` (`output.rs:206-213`). The one place C1
retains emitted objects is the edit frontier, deliberately (`edit/tree.rs:197-215`), charged against
`EDIT_DEFERRED_LIMIT` (`tree.rs:332-347`). There is no `Drop`-based release queue, so "release on
error/cancellation" is ordinary Rust drop of the returned/taken value; C1 has no cancellation token.

Direct emission without a whole-payload staging buffer: chunks are handed to the consumer from inside the
scanner callback while the scanner's own 32 KiB buffer is reused (`cdc/gear.rs:75-95`,
`mapping/build.rs:322-324`), and the canonical allocation *is* that chunk's final home
(`mapping/codec.rs:52-55`: "The envelope and value header are written into the final allocation once; the
payload is copied exactly once, from the scanner buffer into its final home."). The whole-file path is
the exception — see 3.2.

**Defect (seam, medium-low): `into_parts` discards the advisory predecessors.** `output.rs:154-156`
returns `(self.id, self.role, self.canonical, self.references)` and simply drops `self.predecessors`.
Every consumer in the repository except the production save path uses it
(`layerfs-storage/tests/support/mod.rs:127`, `filesystem_pipeline.rs:63`, `layerfs-content/tests/support/mod.rs:87`,
`examples/filesystem_timing_c1.rs:47`, `examples/filesystem_primitives_candidate.rs:51`,
`layerfs-storage/examples/measure_filesystem.rs:48`). The production path reads the hints directly
(`layerfs-storage/src/cas/save.rs:70` `let advisory: Vec<ObjectId> = object.predecessors().ids().collect();`),
so no shipped path loses a hint today; the API shape is still a trap that silently converts a
delta-hint-carrying object into a hint-free tuple, and there is no `into_predecessors()` alternative.

---

## 3. Construction: empty, small, large, stability, validation — PARTIAL

### 3.1 Routing is by logical length under one frozen selector

`policy.rs:120-128`: `if logical_len == 0 { Empty } else if logical_len < small_file_threshold_bytes { WholeFile } else { Chunked }`.

- Empty -> `content.rs:237-245`: no chunks, so the builder has no root and the defined empty leaf plus
  file state are emitted (`mapping/build.rs:332-344`, `:347-363`).
- `0 < n < T` -> `content.rs:160-177` builds one whole-file object and emits it immediately.
- `n >= T` -> `content.rs:228-253` streams through `build_streaming`.

### 3.2 Defect: the whole-file encoder does **not** write once into its final allocation

`file/content.rs:76-79` claims:

    /// Encodes the canonical object of a whole-file payload.
    ///
    /// The value is written into its final canonical allocation once; there is no
    /// inner allocation that the outer object then copies.

The body contradicts it:

    content.rs:91    let mut value = Vec::with_capacity(WHOLE_VALUE_HEADER + bytes.len());
    content.rs:92-94 value.extend_from_slice(WHOLE_MAGIC); ... value.extend_from_slice(bytes);
    content.rs:95    let canonical = encode_bytes_object(&value)?;

`encode_bytes_object` (`object/codec.rs:70-75`) allocates a *second* buffer and copies the value into it,
so two payload-sized allocations coexist (peak about 2n + 33) and the payload is copied twice. This is
exactly the cut the contract requires: `content-io.md:210` "| Whole-file construction | Encode directly
into final canonical allocation; remove inner-envelope allocation/copy |"; `canonical-objects.md:137`
"Small-file encoding ... builds an inner allocation then copies into the outer one | Write identical
bytes into the final canonical allocation"; `file-content.md:156-157` "For known final small results,
target one final canonical allocation instead of separate head, tail, replacement-probe and inner-envelope
allocations."

The prior review's memory row accepts the double allocation as fact
(`stages-1-2-review-20260916T185553Z.md:914`: "value plus canonical allocation ... `T + 23` and `T + 33`"),
so code and ledger agree with each other and the **in-code doc comment is false**.
`encode_chunk_object` shows the intended pattern (envelope and value header written straight into the
final `Vec` with a pre-validated capacity, `mapping/codec.rs:69-82`), and `encode_node` writes through
`encode_bytes_object_to` into a pre-sized allocation (`mapping/codec.rs:154-155`) — only the whole-file
path, the one the contract names, still copies. The same shape recurs at `content.rs:110-116`
(`encode_whole_file_payload`) and `mapping/codec.rs:272-282` (`encode_file_state`: value 93 B then
canonical 106 B — trivial in bytes, identical in form).

Edit consequence: `file/edit/apply.rs:87-96` assembles the result into `out` (`assemble_inner`,
`apply.rs:139-146` `try_reserve_exact(final_len)`), then calls `encode_whole_file(capacities, &bytes)`,
so a whole-file edit transiently holds the assembled bytes, the `value` buffer **and** the canonical
buffer — three payload-sized allocations, plus the base bytes held by `FileView` (`file/view.rs:20-24`).

### 3.3 Defect: one public encoder allocates before it checks any bound

`content.rs:110-116` `pub fn encode_whole_file_payload(raw: &[u8])`:

    content.rs:111  let mut value = Vec::with_capacity(WHOLE_VALUE_HEADER + raw.len());

There is no comparison against `capacities.whole_file_raw_limit` or `MAX_OBJECT_FIELD_BYTES` before the
allocation; the bound is applied only afterwards, inside `encode_bytes_object` -> `canonical_len(value.len())`
(`codec.rs:26-44`). The doc comment says the length "is already validated by the caller"
(`content.rs:107-109`), which is the caller-asserted-metadata pattern `canonical-objects.md:81-82` rules
out. The function is exported (`file/mod.rs:14`, `lib.rs:27`). `encode_whole_file` does it correctly (bound
checked at `content.rs:84-90` **before** the allocation at `:91`); this one does not.

### 3.4 Exact-length stability (same input -> same bytes regardless of route)

- `construct_stream` probes at most `T` bytes (`content.rs:198-209`) and only switches to the chunked
  builder when it *filled* the probe: `content.rs:210-212` `if (prefix.len() as u64) < cutoff { return construct_bytes_in(...) }`.
  A short source below the cutoff is a definitive whole-file result (test `streaming.rs:65-72`).
- The probe bytes are prepended to the source with `Cursor::new(prefix).chain(source)`
  (`content.rs:213-218`), so no byte is read twice and `len == T` is exactly the chunked route
  (`policy.rs:123-127`).
- The external test compares known-length and streamed construction object-for-object for
  `[0, 1, 8192, 131071, 131072, 131073, 300000]` (`tests/streaming.rs:140-160`, asserting equal `root`
  **and** equal emission order) and the streaming path against repeated runs (`streaming.rs:176-185`);
  `edit_transitions.rs:229` (`known_and_streamed_complete_lengths_agree`) repeats it across cutoffs.
  Recorded receipt: pass.

### 3.5 Validation on public entry points

Present and explicit: lengths/limits (`content.rs:84-90`, `:96-102`; `codec.rs:26-44`), range validation
(`file/read.rs:85-91`, `file/view.rs:92-98`, `mapping/read.rs:181-187`), edit-stream validation before any
work (`edit/input.rs:99-139` — inverted range, range beyond the current result, overlap, and
`MAXIMUM_EDITS_PER_OPERATION` at `:100-106`), declared-base-length check (`apply.rs:48-52`),
replacement-length check before construction (`apply.rs:256-260`), batch cardinality (`access.rs:52-57`,
`mapping/read.rs:120-125`, `:236-241`) and object limits (`codec.rs:82-87`, `:98-110`,
`mapping/codec.rs:57-62`, `:166-171`). No silent clamping was found; every failure path returns a typed
error.

**Gap 3a (medium): the construction policy is never validated inside C1.** `policy.rs:81-102` is the only
validator (`pub const fn validated(self) -> ContentResult<Self>`, returning `UnsupportedPolicy`), and it is
*not* called by `construct_bytes` (`content.rs:143`), `construct_stream` (`content.rs:190`) or `apply_edits`
(`apply.rs:38`). The only guard on the derivation path is a debug assertion that compiles out of release
builds: `policy.rs:142-145` `debug_assert!(self.small_file_threshold_bytes >= MINIMUM_SMALL_FILE_THRESHOLD_BYTES, "capacities() requires a validated cutoff")`.
`README.md:32-34` states "Any other value fails with `ContentError::UnsupportedPolicy` before work begins.
Nothing is silently clamped"; that is true of `validated()` and of the C2 bridge
(`layerfs-storage/src/sqlite/schema.rs:65`, `cas/store.rs:117`), but not of a direct C1 call. Two
consequences: (i) the cutoff that sizes the probe and the whole-file limits is a caller-declared number
when a caller skips `validated()` (see §8 rows A11/A16); (ii) an unvalidated policy silently produces
objects the frozen profile would reject (e.g. `ConstructionPolicy::new(4, 8, 4)` yields a 4-byte cutoff
that routes a 3-byte file to a whole-file object without error).

**Gap 3b (low): `Edit::overwrite` / `delete` can panic instead of erroring.** `edit/input.rs:43-45`
`pub const fn overwrite(start: u64, end: u64) -> Self { Self::new(start, end, end - start) }` and `:53-55`
(`delete`) subtract in a `const fn`: an inverted pair underflows (debug panic; release wrap to a huge
`replacement_len`). `EditStream::new` then rejects the pair (`input.rs:110-114`, "inverted range"), so no
invalid stream reaches construction, and `removed_len()` (`:73-75`) has the same shape. The public
constructors are not themselves checked.

### 3.6 Report-vs-code contradictions found in this area

- `README.md:76-77`: "A range read navigates the tree with one-ID batches and batches only payload
  acquisition; grouped node acquisition is a later change, not a silent default." At this HEAD navigation
  *is* grouped per level: `mapping/read.rs:232-235` `for chunk in level.chunks(READ_NAVIGATION_WAVE) { let ids ...; reader.read_canonical_batch_scoped(&ids, ...) }`,
  with the external test `file_read.rs:255-256` asserting `max_node_batch == leaves`. The README is stale
  (HEAD contains `ab17a6958 feat(content,storage): batch mapping navigation per level`).
- `README.md:64-68`: "unchanged mapping *pages* are re-encoded rather than reused from the stored tree, so
  a large-to-large edit does not reproduce a reference (v0.1.6) root in general." The code reuses stored
  subtrees by identity (see 6.1) and the sealed-oracle test asserts reference root **and** partition
  equality (`tests/edit_reference.rs:592-641`; recorded pass). The stale claim understates the
  implementation.

---

## 4. CDC: `file/cdc/gear.rs` — PASS (one coverage gap)

Frozen parameters (`gear.rs:13-26`): `MINIMUM_CHUNK_BYTES = 8_192`, `TARGET_CHUNK_BYTES = 16_384`,
`MAXIMUM_CHUNK_BYTES = 32_768`, `NORMALIZATION_SHIFT = 2`, `PROFILE_SEED = 0` and the four literal masks;
the 256-entry `GEAR` table is a frozen literal (`gear.rs:281-538`). Verified by arithmetic that the two
"shifted" masks are exactly the plain masks shifted left by one (`0x0000_d903_0353_7000 << 1 == 0x0001_b206_06a6_e000`,
`0x0000_d901_0353_0000 << 1 == 0x0001_b202_06a6_0000`), i.e. the names describe the code's actual two-byte
rolling construction. `profile_id()` hashes every parameter plus all 256 table words (`gear.rs:29-49`), so
the profile is self-identifying.

Boundary determinism (read, not executed):

- One 32 KiB input array and one 32 KiB accumulator per scan (`gear.rs:81`, `:128`), and the accumulator
  can only reach `MAXIMUM_CHUNK_BYTES`: the fill phase tops it up to `MINIMUM_CHUNK_BYTES`, after which
  every scan append is even-length and the code emits on `position == MAXIMUM_CHUNK_BYTES`
  (`gear.rs:146-155`, `:199-204`). No reallocation is possible.
- A read boundary that lands on an odd byte is carried in `Scanner.pending` (`gear.rs:205-207`,
  `:140-144`), and `process_pending_pair` (`:213-250`) reproduces exactly the two cut rules of
  `scan_region` (`:105-123`): first check against the shifted mask after adding `GEAR[first] << 1`, second
  against the plain mask after adding `GEAR[second]`, with the small/large mask chosen by the same
  `position < TARGET_CHUNK_BYTES` predicate (`:160-164` vs `:220`). Both paths agree on pair alignment and
  on which byte a cut precedes, so the partition is a function of the byte stream, not of the reader's
  buffer sizes.
- Cost per byte: one table lookup, one shift-add and two mask tests per byte in a two-byte loop
  (`scan_region`, `#[inline(never)]` at `gear.rs:104`), plus the copy into the chunk accumulator and the
  copy into the canonical object.

**Coverage gap (UNVERIFIED by execution):** no external test drives a source that splits its reads at
*odd, non-EOF* offsets — `CountingSource` forwards the caller's request size (`tests/support/mod.rs:338-346`)
and the probe+`chain` route always hands the scanner full 32 KiB windows, so only the final short read
exercises the pending byte. The mid-stream `process_pending_pair` path is asserted here by reading, not by
running.

---

## 5. Mapping: partitions, `build.rs`, `read.rs` — PARTIAL

Partitions and bounds: `MIN_ENTRIES = 64` and `MAX_ENTRIES = MAX_MAPPING_ENTRIES = 128`
(`mapping/types.rs:13,15`; `policy.rs:212`), `MAX_LEVEL = 31` (`types.rs:17`), `MINIMUM_ROOT_ENTRIES = 2`
(`types.rs:19`), `MAX_NODE_OBJECT_BYTES = 8_192` (`types.rs:23`). `ExtentNode::validate` enforces the
partition and the summary arithmetic in one place (`types.rs:171-237`), including the "two contiguous
slices of one payload are not canonical" rule (`:189-195`) that `coalesce_adjacent` maintains.

Build: one owned builder with flush threshold `max(capacities.stream_flush_entries, MAX_ENTRIES+1)`
(`build.rs:74-82`; default `192` via `policy.rs:159` `stream_flush_entries: MAX_MAPPING_ENTRIES + STREAM_FLUSH_HEADROOM`),
a lowest-first cascade (`build.rs:200-219`), and a final partitioner that half-splits only when a level
exceeds 128 (`build.rs:221-252`; the edit-side twin of that partitioner is `edit/tree.rs:808-841`), so a fresh page always has >= 64 entries. Pages are emitted child-before-parent
(`build.rs:295-296`, `:377-386`) and the file state last (`build.rs:347-363`).

Read: `read.rs:174-199` validates the range, walks the tree and then *proves* coverage
(`read.rs:195-197` `if wave.counters.payload_bytes_read != requested { return Err(ContentError::InvalidRecord("mapping coverage")) }`).
Navigation is grouped per level in waves of 32 pages (`read.rs:232-244`, `READ_NAVIGATION_WAVE = 32` at
`:33`), payloads in waves of 32 distinct objects (`read.rs:95-111`, `READ_WAVE_OBJECTS = 32` at `:24`),
and each wave is released before the next (`read.rs:161-163`). Bytes reach the sink in logical order
straight from borrowed payload slices (`read.rs:141-160`).

**Is there a whole-file materialization behind a batch API? No.** The only batch API is
`read_canonical_batch` / `read_canonical_batch_scoped` (`object/access.rs:31,40`), and C1 calls it with at
most `READ_WAVE_OBJECTS` distinct ids per call (`mapping/read.rs:97-99`, `:232`); a whole-file read
streams payload slices to the caller's sink (`file/read.rs:93-114`, `file/view.rs:103-111`).
`read_all_bounded` refuses a declared maximum smaller than the logical length (`file/read.rs:31-37`)
rather than materializing anything.

**Gap 5a (low-med): the declared wave byte ceiling is not enforced.** `read.rs:25-26`
`pub const READ_WAVE_BYTES: usize = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES;` with the comment at
`:27-32` presenting it as the wave's "declared byte ceiling". It is never read in product source; the only
reference outside the declaration is a test asserting the constant's *arithmetic*
(`tests/file_read.rs:294-298` `assert_eq!(READ_WAVE_BYTES, READ_WAVE_OBJECTS * 32_768, "the declared byte window is the object bound at the largest chunk")`).
The effective bound is 32 distinct payloads, each rejected by `decode_chunk_payload` only *after* it is
held (`mapping/read.rs:127-131`; the ceiling check is `mapping/codec.rs:91-96`), so a provider returning
oversized objects can make C1 hold up to 32 x the 16 MiB envelope ceiling before the first error. The
prior review already recorded this as "declared, never enforced"
(`stages-1-2-review-20260916T185553Z.md:251`, `:612`) and it is still true at this HEAD.

**Gap 5b (low): the navigation frontier has no declared bound.** `traverse` accumulates one `Frontier`
(`ObjectId` + bool + level + origin + `Option<(u64,u64)>`, `read.rs:62-68`) per node the range must still
reach at the next level (`read.rs:278-284`), while only 32 pages are held at a time. The doc comment
concedes the shape ("The frontier retains one 56-byte entry per mapping node the range still has to reach
at the next level", `read.rs:30-32`) but no ceiling exists; for a workspace-scale file the level frontier
is proportional to the page count of the range, not to a constant. This is the one C1 owner with no
declared capacity at all.

---

## 6. Stage-4 edits — PARTIAL

### 6.1 What is verified

- Stored-subtree copy-on-write: `split` returns a stored summary untouched when the cut is at its edge
  (`tree.rs:565-570`), and `commit_node` republishes a stored subtree by identity without re-encoding it
  (`tree.rs:400-408`: "Either a stored subtree, or a draft another reference already committed during this
  walk: both answer with their identity."). The external test measures demanded identities and asserts the
  read set is the affected path plus the replaced payloads (`edit_localized.rs:1-19`, `:300-357`);
  recorded pass.
- Current-result coordinates: the module contract is stated (`edit/input.rs:1-15`) and enforced by the
  lazy `Plan` cursor, which is a function of the stream and never materializes a second edit list
  (`input.rs:291-304`; `advance` at `:319-384`). Adjacent edits stay separate on purpose (`input.rs:120-127`).
- Old-root immutability: the base is only read (`tree.rs:159`, `:902`; `view.rs:33-36`), never written;
  the edit test asserts every base object is byte-identical afterwards (`edit_single.rs:179-206`, recorded
  pass).
- Bounded decoded frontier: `EDIT_DEFERRED_LIMIT = 8 * 1024 * 1024 - 1` (`tree.rs:31`) charged per draft
  with a 128-byte overhead (`tree.rs:85-104`; `charge_bytes` at `:332-347`), released when a draft is
  superseded (`release` at `:313-319`, `settle` at `:242-263`), with the peak reported
  (`EditCounters::peak_deferred_bytes` at `:345`). Tests assert the frontier does not grow with the edit
  count (`edit_bounds.rs:249-335`, `:672-763`) and that the per-draft charge stays inside its derived bound
  (`edit_bounds.rs:764`). Recorded pass.
- No-op handling: an empty stream returns the base root before any work (`apply.rs:53-60`), and a stream
  whose every replacement is byte-identical returns the base root after a bounded comparison
  (`apply.rs:63-76`; `compare.rs:31-87`, window `COMPARE_WINDOW_BYTES = 64 * 1024` at `:19`). A zero-length
  replacement over a zero-length base range is classified `Equal` by construction (`compare.rs:46-50`), so
  a degenerate edit still writes nothing. The comparison is a *second* pass over replacement and base
  bytes, which the contract explicitly sanctions and requires to be counted (`file-content.md:291-295`);
  the mismatch path replays the same stream (`compare.rs:6-9`).

### 6.2 Defect (medium): the immutable base root is acquired twice per chunked edit

`apply_edits` opens the base once (`apply.rs:47` `FileView::open(reader, request.root, edit.child("edit.base"))`,
which reads the root at `view.rs:33-36` and classifies it at `:37`), then decodes the file state from the
cached bytes to choose the route (`apply.rs:110-111` `match view.file_state()? { Some(_) => replace_chunked(...)`),
and then `replace_chunked` **reads the same root again from the provider**:

    apply.rs:216-218   let (state, mut summary) = edit
                           .child("edit.base_read")
                           .run(|_| crate::file::edit::tree::read_state(reader, view.root()))?;

`read_state` is a fresh acquisition: `tree.rs:902` `let canonical = reader.read_canonical(root)?;` followed
by `decode_file_state` (`:903`). The 93-byte state is therefore demanded twice and decoded three times,
contradicting `apply.rs:2-3` ("The base is opened once, the edit stream is validated once"), `view.rs:1-7`
("The base root is acquired and classified exactly once per operation") and `README.md:49`
("`file::FileView` one authenticated base, opened once per operation"). The fix is local: `replace_chunked`
already receives `&view`, `view.file_state()` returns the decoded state, and the summary is a pure function
of it (`tree.rs:905-912`).

Cost is not merely a function call: the production provider turns each C1 call into a Store read wave, and
`Store::read_batch` opens a fresh SQLite connection and a decompression workspace per call
(`layerfs-storage/src/cas/store.rs:207-219`: `check_read_demand(ids, self.capacities.read_objects)?; ... let connection = connection::open(&self.path, false)?; ... DecompressionWorkspace::new()`),
reached from `StoreProvider::read_canonical_batch_scoped` (`layerfs-storage/src/cas/provider.rs:60-67`).
The external test cannot see the duplicate because it collapses demanded ids into a set
(`edit_localized.rs:343` `let distinct: BTreeSet<ObjectId> = report.demanded.iter().copied().collect();`,
then `:353-357` asserts on `distinct.len()`), so "pages read 3/4" in that file's header counts distinct
identities.

### 6.3 Defect (medium, reachability UNVERIFIED): the write path validates every page as a **root**

    mapping/codec.rs:101-102   pub fn encode_node(node: &ExtentNode) -> ContentResult<Vec<u8>> {
                                   node.validate(true)?;

`validate(true)` skips the non-root occupancy rule (`types.rs:171-175`
`if count > MAX_ENTRIES || (!root && count < MIN_ENTRIES) { return Err(ContentError::NonCanonicalPagePartition) }`),
and every encoder path funnels through it: the builder (`mapping/build.rs:377`), the edit commit for
rebuilt nodes (`edit/tree.rs:449`), and the pages the replacement builder produces. The readers, by
contrast, apply the true context — `mapping/read.rs:246` `decode_node_with_context(canonical, node.root)?`,
`edit/tree.rs:157`, `edit/tree.rs:415`. A page with 1..63 entries that lands in a non-root position is
therefore *writable and publishable* and *unreadable by C1's own reader*. The contract requires the check
to be preserved: `file-content.md:188-189` "Preserve coalescing, root collapse, half partitioning,
fanout/height limits and root/non-root validation."

Two partial safety nets exist on the commit walk: a held *branch* page is re-decoded with its real context
(`edit/tree.rs:412-415`), and the edit-side page constructors never validate at all (`emit_leaf`
`tree.rs:844-856`, `emit_branch` `tree.rs:859-895`, `root_from_children` `:824-841`). A held *leaf* page is
deliberately not decoded ("a leaf page needs no decode", `tree.rs:412-414`), so an under-full leaf escapes
every write-side check.

Reachability: I could not construct a counterexample by reading. The edit merge discipline keeps non-root
pages >= 64 by construction — a split piece is always concatenated with a same-level stored sibling that
itself has >= 64 entries (`tree.rs:667-694` merges same-level branches by *unioning their children*, and
the mixed-height arms re-host the smaller side only when it is a collapsed single child:
`tree.rs:716-746`, `:767-804`), and the half-partition only triggers above 128 entries
(`tree.rs:808-841`), which yields halves >= 64. I nevertheless mark this **PARTIAL/UNVERIFIED** because the
write-side proof is absent and the test evidence stops well short of the interesting shapes: the
canonical-partition assertions in `edit_bounds.rs:80-99` cover 63/64/127/128/129 and 192/193 extents
(`edit_bounds.rs:131-180`) and the sealed oracle cases cover at most 400 extents
(`edit_reference.rs:198-310`), i.e. mappings of at most two node levels; a third level needs >= 128*128
extents, about 256 MiB of input, which the suite itself declares out of budget for the related cascade case
(`streaming.rs:238-241`). The one test that would catch such a page is the oracle walk
(`edit_reference.rs:313-351` decodes every page with its real `is_root`), and it only runs those nine cases.

### 6.4 Defect (low-med): the no-op return skips the policy/representation check

`file-content.md:257-260`: "An empty edit stream can retain the base root after checking declared length
and compatible profile/representation. ... do not promise unconditional root reuse alongside a conflicting
representation change". The code checks only the length: `apply.rs:48-52`
`if view.logical_len() != request.edits.base_len() { return Err(...) }`, then returns the base root for an
empty stream at `apply.rs:53-60` (before `representation` is computed at `:61`) and for an `Equal` verdict
at `apply.rs:63-76` (after computing `representation` at `:62` but never comparing it with the base's own
`FileContent`). With a policy whose cutoff disagrees with the stored representation (e.g. a chunked base
under a larger cutoff), the operation returns a root whose representation is not the one the passed policy
selects, silently. The shipped C2 path records one policy per workspace and compares it
(`layerfs-storage/src/sqlite/schema.rs:128` `if expected.validated()? != stored`), so the exposure is
direct-C1 callers; the edit no-op tests all run under one policy (`edit_noop.rs:19-105`).

### 6.5 Uncharged frontier index (ties to §8)

The draft index maps are not part of any charge: `drafts: BTreeMap<ObjectId, Draft>`,
`parent_refs: BTreeMap<ObjectId, u32>`, `detached: BTreeSet<ObjectId>` and
`committed: BTreeMap<ObjectId, ObjectId>` (`edit/tree.rs:112-120`) are all container-allocated, while only
the per-draft payload is charged (`tree.rs:180`, `:211`, `:332-347`). `committed` grows monotonically
across the operation (`:460` inserts on every committed draft; nothing removes entries). The concurrent
bound on the *charged* bytes is 8 MiB - 1 (`tree.rs:31`), which at the 128-byte minimum charge is about 65 000
drafts; the maps' own overhead on top of that is undeclared.

---

## 7. C1-only operation — PASS

- `core/crates/layerfs-content/Cargo.toml:9-11`: `[dependencies]` / `blake3 = { version = "=1.8.5", default-features = false }` / `layerfs-telemetry = { path = "../layerfs-telemetry" }`. No SQLite, storage, workspace, history, mount or runtime dependency.
- Direction is one-way: `core/crates/layerfs-storage/Cargo.toml:11` `layerfs-content = { path = "../layerfs-content" }`; nothing in `layerfs-content` names storage (rg for `layerfs_storage|rusqlite|sqlite` in C1 src: no matches).
- Construction completes with no database, pack or file: the consumer is the only output (`object/output.rs:163-166`), `DiscardingConsumer` is the non-persisting implementation (`output.rs:173-213`), and the external test runs complete and multi-file construction through it with nothing else open (`tests/streaming.rs:163-173`, `:97-123`).
- Reads work against a plain in-memory map in the tests (`tests/support/mod.rs:20` `MemoryStore`), and the production bridge is a separate crate (`layerfs-storage/src/cas/provider.rs:44`).
- Product rules: no inline tests, no `cfg(test)`, no retry/busy-handler/fallback and no fsync/fdatasync/sync_* anywhere in C1 src (rg sweeps empty; the only "retry"/"fallback" hits are the doc sentences `object/output.rs:162` and `file/content.rs:188`).

---

## 8. Memory ledger (each C1 allocation site)

Legend: **cap source** = what bounds the capacity; **Δ** = growth checked before the allocation;
**rel** = release on success / error / cancellation (all C1 releases are ordinary Rust drops: nothing in
C1 is `unsafe`, `static mut` or an owned background task).

| id | Owner / buffer | Capacity source | Max multiplicity & overlap | Δ | rel |
| --- | --- | --- | --- | --- | --- |
| A1 | `object/codec.rs:72` `encode_bytes_object`: `Vec::with_capacity(total)` | `canonical_len` (`codec.rs:26-44`), <= 16 MiB | 1 per call; overlaps the caller's value buffer (A10/A12) | yes | drop / drop / drop |
| A2 | `object/codec.rs:50-67` `encode_bytes_object_to` | caller's writer; no allocation | — | n/a | — |
| A3 | `object/output.rs:91` `FinalizedObject.canonical` | moved in from the encoder | 1 per emitted object, moved to the consumer at `accept` | by construction | moved / dropped by consumer |
| A4 | `object/output.rs:93,112` `references: Vec<ObjectId>` | **caller-supplied length**; in-crate producers pass <= `MAX_ENTRIES` (`mapping/types.rs:154-164`) | 1 per object | no | moved into the object |
| A5 | `object/predecessor.rs:48,61-74` `AdvisoryPredecessors.entries` | `MAXIMUM_ADVISORY_PREDECESSORS = 4` (`:14`), rejected in `push` | <= 1 per object | yes | with the object |
| A6 | `object/inode_leaf.rs:252` `encode_leaf_value`: `Vec::with_capacity(31 + count*81)` | count <= `MAXIMUM_LEAF_ROWS = 100` checked at `:245-247` | 1 per leaf | yes | returned |
| A7 | `object/inode_leaf.rs:375` `pooled_body` capacity | `pooled_physical_length` (`:358-367`), <= 44 + 100*12 | 1 | yes | returned |
| A8 | `object/inode_leaf.rs:406,428` `decode_pooled_body` rows + 44-byte prefix | width checks at `:395-405` | 1 | yes | returned |
| A9 | `object/inode_leaf.rs:446-455` `rebuild_leaf` row `collect` | `rows.len() == values.len()` (`:443-445`) | <= 100 rows | yes | dropped after `encode` |
| A10 | `file/content.rs:91` `encode_whole_file` `value` | `whole_file_raw_limit` checked at `:84-90` before the allocation | 1, **overlaps A1** (the canonical copy at `:95`) -> 2n+33 | yes | drop |
| A11 | `file/content.rs:199-206` `construct_stream` probe `prefix` | `policy.small_file_threshold_bytes()` (`:198`) — **caller-declared, unvalidated** (gap 3a); `try_reserve_exact` turns failure into `BoundedCapacityExceeded` | 1, overlaps the scanner | via `try_reserve` | dropped after the probe is chained |
| A12 | `file/content.rs:111` `encode_whole_file_payload` `value` | **none before the allocation** (3.3) | 1, overlaps A1 | **no** | drop |
| A13 | `file/view.rs:22` `FileView.canonical` | the root object from the provider (<= 16 MiB envelope ceiling; a whole-file root <= T+23) | 1 per operation | provider-side | with the view |
| A14 | `file/cdc/gear.rs:81` `input: [u8; 32768]` | constant, **stack** | 1 per scan | n/a | on return |
| A15 | `file/cdc/gear.rs:128` `Scanner.chunk` | `MAXIMUM_CHUNK_BYTES`, proven non-growing in §4 | 1 per scan; overlaps A1 (the emitted chunk) | yes | on scan return |
| A16 | `file/mapping/build.rs:77` `levels[0]: Vec<Pending::Extents>` | `flush_at + 1` where `flush_at = capacities.stream_flush_entries.max(129)` (`:75`) — **caller-declared, unvalidated** | 1 per builder | **no** | on builder drop |
| A17 | `file/mapping/build.rs:302-305` upper levels | `MAX_ENTRIES + 1` entries; depth bounded by `MAX_LEVEL` | <= 31 levels, 129 x 56 B each | yes | with the builder |
| A18 | `file/mapping/build.rs:262,272` `emit_prefix` drain+collect | count <= 128 | one extra copy of the sealed page, overlaps A16 | yes | dropped after encode |
| A19 | `file/mapping/codec.rs:121,154` `encode_node` value+canonical | `node.validate(true)` then `canonical_len` <= `MAX_NODE_OBJECT_BYTES` | 2 x <= 8 KiB per page | yes | drop |
| A20 | `file/mapping/build.rs:377` `node.references()` Vec | `MAX_ENTRIES` | 1 per page, moved into the object (A4) | by construction | moved |
| A21 | `file/mapping/read.rs:89-90` `Wave.demands` / `distinct` | literal 64 / 32 capacities | 1 wave | yes | cleared per wave (`:161-162`) |
| A22 | `file/mapping/read.rs:117-131` wave `values` (provider-owned) | **provider**; C1 checks cardinality, then the chunk ceiling per object | <= 32 objects held at once | no (gap 5a) | dropped after the wave |
| A23 | `file/mapping/read.rs:217-230,278-284` `level` / `next` frontier | **none declared** (gap 5b) | one entry per node the range still reaches, per level | no | dropped per level |
| A24 | `file/edit/input.rs:91` `EditStream.edits` | `MAXIMUM_EDITS_PER_OPERATION = 4096` (`:22`, checked at `:100-106`) | caller-owned Vec, moved in | yes | with the stream |
| A25 | `file/edit/input.rs:185` `Replacements.parts` | caller-supplied | caller-owned | no | with the source |
| A26 | `file/edit/compare.rs:40-41` two windows | `COMPARE_WINDOW_BYTES = 64 KiB` (`:19`) | 2 x 64 KiB, reused across windows | fixed | per call |
| A27 | `file/edit/tree.rs:112-120` `drafts` / `parent_refs` / `detached` / `committed` | **container overhead uncharged**; the per-draft payload is charged at `:332-347` | bounded in count by `EDIT_DEFERRED_LIMIT/128`; `committed` grows monotonically | partial | with the operation |
| A28 | `file/edit/tree.rs:179` `hold_node` `node.clone()` | the node being held (<= 128 entries) | transiently 2x the node | yes | draft released on release/settle |
| A29 | `file/edit/tree.rs:493-514` `child_summaries` | `children.len()` <= 128 | a few live during `concat` recursion | yes | per call |
| A30 | `file/edit/apply.rs:139-146,183` `out` + 16 KiB stack buffer | `try_reserve_exact(final_len)`; this route runs only when `final_len < T` | overlaps A10+A1 -> 3n peak | yes | dropped on return |
| A31 | `file/edit/apply.rs:264-283` per-edit replacement builder | `ExtentBuilder::new(capacities)` -> A16/A17/A15/A14 | one live builder per edit, concurrent with A27 | as A16 | dropped after the edit |

Reading of the ledger: no site grows a buffer without a check *except* A11/A16 (whose capacity comes from
an unvalidated policy/capacities field), A12 (no check at all) and A22/A23 (provider/frontier shapes with
no C1 ceiling). Every release is a drop, which is correct for success, error and cancellation, and no site
keeps a payload copy after the handoff. The note in `content-io-memory-audit.md:120-139` claims the
replacement core's owners are "stated where they are implemented, in `filesystem-tree.md` and
`content-io.md`"; for C1 that is not so — `content-io.md` contains no C1 numbers (rg for
`192|8 MiB|EDIT_DEFERRED|READ_WAVE` in that file: no matches), so the C1 rows above currently live only in
the code and in a prior review's table (`stages-1-2-review-20260916T185553Z.md:908-920`).

---

## 9. Public API with no caller anywhere (dead-surface sweep)

Method: extract every identifier in a `pub use` of `src/lib.rs` or any `src/**/mod.rs` (214 names), then
search every `.rs` file under `core/crates` (src, tests, examples, support modules) for each name,
excluding `pub use` / `use` lines, the item's own definition line and comment-only lines
(``//``, ``///``, ``//!``, ``*``). Names under `src/filesystem/**` are listed only for completeness — they
are the other reviewer's scope and I make no claim about them.

**C1 (my scope) — exactly one dead re-export:**

| Item | Declared | Re-exported | Callers |
| --- | --- | --- | --- |
| `slice_of` | `file/edit/split.rs:12` `pub fn slice_of(extent: ExtentSlice, origin: u64, from: u64, to: u64) -> ContentResult<ExtentSlice>` | `file/edit/mod.rs:19` `pub use split::slice_of;` | **none** — no reference in `core/crates` outside those two lines (repo-wide `rg -n 'slice_of'` finds only the definition, the re-export, an unrelated `slice_offset` in `crates/layerfs-fuse/src/live_wire.rs`, and two prior-review documents) |

The same item was already flagged by the Stage-4 review:
`docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/criteria-stage4.md:133` —
"file/edit/split.rs:12 slice_of is re-exported (edit/mod.rs:19) and **never called**; tree.rs:315-341
splits extents inline instead, without slice_of's coverage check." The corresponding inline split is still
at `file/edit/tree.rs:577-611`; note it *does* keep the equivalent coverage arithmetic (partitioning the
extent list with `logical + len <= offset`), so the dead function duplicates a live rule rather than
replacing a missing check. Recommendation: delete `split.rs` and its re-export, or route `tree.rs`'s leaf
split through it.

**C1 names with no caller outside their own declaring file (informational, not dead):**
`COMPARE_WINDOW_BYTES` (only `compare.rs:54`), `POOLED_VALUE_CANONICAL_BYTES` (only `inode_leaf.rs:156`),
`chunk_canonical_len` (only `mapping/codec.rs:69`), `emit_empty_leaf` / `emit_empty_representation` /
`compare_replacements` (used by other C1 files, no external caller), `DEFAULT_CHUNK_DELTA_MAX_DEPTH` and
`DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH` (only `policy.rs:54-55`). Each is used by product code, so none is
dead; several exist only because `lib.rs` re-exports the whole policy surface.

**Out of scope, zero-caller `pub use`s found by the same sweep (for the filesystem reviewer):**
`DirectoryListing` (`filesystem/mod.rs`), `apply_changes` (`filesystem/inode/mod.rs`),
`build_directory` (`filesystem/directory/mod.rs`), `build_table` (`filesystem/inode/mod.rs`). Reported as
sweep output, not adjudicated here.

---

## 10. Findings, ranked

1. **MEDIUM — `encode_whole_file` copies twice and its doc claims it does not** (`file/content.rs:76-79` vs `:91-95`; contract `content-io.md:210`, `canonical-objects.md:137`, `file-content.md:156-157`). The whole-file route is the one the contract singled out; peak 2n+33 on construction and 3n on a whole-file edit (`apply.rs:87-96`).
2. **MEDIUM — the chunked edit re-acquires the base root** (`apply.rs:216-218` -> `tree.rs:902`) after `FileView` already read and classified it (`view.rs:33-37`, `apply.rs:110-111`), contradicting the "opened once per operation" claim in three places; each C1 provider call is a fresh connection + decode workspace in C2 (`cas/store.rs:207-219`), and the edit tests cannot see it because their oracle is a set (`edit_localized.rs:343`).
3. **MEDIUM — every mapping page is encoded as if it were a root** (`mapping/codec.rs:102` `node.validate(true)?`), while readers validate by context (`mapping/read.rs:246`, `edit/tree.rs:157,415`); a non-canonical non-root page is therefore publishable but unreadable, and no test reaches a three-level mapping to show otherwise.
4. **MEDIUM — the construction policy is not validated inside C1** (`policy.rs:81` never called by `content.rs:143/190` or `apply.rs:38`; only `debug_assert!` at `policy.rs:142-145`), contradicting `README.md:32-34`; this is also what makes A11/A16 caller-sized.
5. **LOW-MED — `encode_whole_file_payload` allocates before checking any bound** (`content.rs:111`), on a caller-asserted length (`content.rs:107-109`), against `canonical-objects.md:81-82`.
6. **LOW-MED — the no-op returns the base root without a policy/representation check** (`apply.rs:48-60`, `:63-76` vs `file-content.md:257-260`).
7. **LOW-MED — `READ_WAVE_BYTES` is declared but never enforced** (`mapping/read.rs:25-26`; only asserted as arithmetic at `file_read.rs:294-298`); the navigation frontier has no declared bound either (`read.rs:217-230`).
8. **LOW-MED — `FinalizedObject::into_parts` silently drops the advisory predecessors** (`output.rs:154-156`), while the production save path reads them (`cas/save.rs:70`).
9. **LOW — `slice_of` is dead public surface** (`split.rs:12`, `edit/mod.rs:19`), unchanged since the Stage-4 review flagged it.
10. **LOW — draft/frontier index containers are uncharged** (`edit/tree.rs:112-120` vs the charge at `:332-347`; `committed` grows monotonically at `:460`).
11. **LOW — documentation drift in the crate README**: navigation *is* batch-grouped now (`README.md:76-77` vs `mapping/read.rs:232-235`), and stored mapping pages *are* reused (`README.md:64-68` vs `tree.rs:400-408` plus `edit_reference.rs:592-641`).
12. **LOW — `Edit::overwrite` / `delete` underflow on inverted input** (`input.rs:43-45`, `:53-55`; rejected later by `EditStream::new` at `:110-114`).
13. **INFO — `rebuild_leaf` checks only the pooled prefix's width** (`inode_leaf.rs:457-459`) and deliberately ignores its contents (`:460-463`); `decode_pooled_body` rewrites the header count/byte fields from the row count (`:431-433`), so a corrupt pooled header is not an error. Consumers are C2/filesystem-facing; no C1 file read/write path is affected.

## 11. What this review did not establish

- No execution: I did not build, run or re-derive any test; the pass claims rely on the recorded receipt
  named at the top (same commit) and on the code.
- The three-level mapping canonicality question (§6.3) is an analysis, not a demonstrated counterexample;
  the reachability of an under-full non-root page is **UNVERIFIED** either way. A test that builds a
  >= 16 385-extent base (about 256-512 MiB) and walks every emitted page with its real `is_root` would settle it.
- CDC determinism across adversarial read splits (§4) is read-verified only; `process_pending_pair` is not
  exercised by any test with an odd mid-stream read.
- All `src/filesystem/**` claims (including the zero-caller re-exports listed in §9 and the inode-leaf
  pooling consumers) are outside this review.
- No memory measurement exists for C1 in any document I read; every capacity in §8 is a declared bound,
  not an observed peak.
