# C. LayerFS core storage: design and optimization history (paper notes)

Read-only research notes, 2026-11-14. Nothing was built, run or edited.

## Identities and citation conventions

- **Code identity (2.0):** `origin/layerfs_v2_0_0` @ `8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5`.
  Citations written `v2:<path>` are read with
  `git -C ~/Ephemeral-AI-Lab/layerfs show origin/layerfs_v2_0_0:<path>`.
- **main (v1.x):** working tree at `67310175eb`; merge base with 2.0 is
  `35d36575c1` (2026-10-29). Differences are listed in section 10.
- **Benchmark repo:** `~/Ephemeral-AI-Lab/layerfs-benchmark` @ `e628da3`. Citations
  written `bench:<path>`.
- **Issues:** `layerfs-project#N` = GitHub `Ephemeral-AI-Lab/layerfs-project` issue N
  (the #84 storage epic and its sub-issues #85 to #93, #99), read with `gh issue view`.
- **Doc reorganisation:** on 2.0 the old numbered `core/docs/architecture/NN-*.md`
  pages (02, 03, 05, 08, 09, 31, 45, 54, 56, 63, 65, 84, 85) are gone. Their content
  lives in `v2:docs/architecture/core/{objects,files,physical-storage,history}/` and
  `v2:docs/architecture/engine/store/`. The numbered pages still exist on main under
  `core/docs/architecture/`. These notes cite the 2.0 pages.
- **Labels:** **[measured]** means a number with a named source run or receipt.
  **[constant]** means a value in source code. **[illustrative]** means a worked example
  with no measured meaning. Wall times in benchmark reports are INELIGIBLE diagnostics,
  never speed claims.

---

## 1. Object model

### 1.1 Canonical envelope and ObjectId

- Every stored thing is an immutable **canonical object**: role-tagged value bytes
  inside a fixed envelope.
  - Layout: `"LFSO"` magic (4 B) | kind `0x01` (1 B) | `payload_len` (u32 BE) |
    `value_len` (u32 BE) | value.
  - `payload_len = value_len + 4`. The value starts with a role magic and version,
    for example `LFS5SML\0` (whole file), `LFS4CHK\0` (chunk), `LFS4MAP\0` v3 (extent
    pages and file state).
  - Sources: `v2:docs/architecture/core/objects/README.md` ("Identity and the
    envelope"); `v2:core/crates/layerfs-content/src/object/codec.rs`.
- **ObjectId** = `BLAKE3("layerfs/object/v2\0" || canonical_bytes)`, 32 bytes
  [constant].
  - `OBJECT_DOMAIN = b"layerfs/object/v2\0"`, `DIGEST_BYTES = 32`, and
    `ObjectId::for_bytes` hashes the domain and then the bytes.
  - `ObjectId::stream()` hashes the bytes incrementally, so a chunk is hashed while it
    is scanned.
  - Source: `v2:core/crates/layerfs-content/src/object/id.rs`.
  - The ID covers the envelope and role framing, so it is **not** a digest of the raw
    file bytes. The same payload under different roles gets different IDs.
- **Strict decoding.** The decoder checks, in order: ceilings, length, magic, kind,
  payload and value ceilings, truncation and trailing bytes.
  - Non-canonical input (padding, a foreign kind) is refused, never repaired, so one
    value has exactly one encoding.
  - Source: `v2:.../object/codec.rs`; invariants table in `objects/README.md`.
- **Physical form is outside the ID.**
  - FULL, STORED or PREFIX encoding, compression, the delta base, the pack and the
    SQLite/PostgreSQL row do not affect the ID.
  - Every read rebuilds the canonical bytes and re-hashes them against the ID.
  - Source: `v2:docs/general/cas_cdc_deltaencoding_handbook.md` §2.
- **Pack keys.** A pack is keyed separately by `ObjectKey` = SHA-256 of the sealed
  pack bytes (`v2:core/crates/layerfs-storage/src/port/pack.rs`).
- **Admission.** `FinalizedObject` holds the ID, role, owned canonical bytes, direct
  references derived from the bytes (never trusted from the caller) and up to four
  advisory predecessors.
  - The predecessors are hints outside the ID.
  - Source: `v2:.../object/output.rs`, `object/predecessor.rs`, `object/admission.rs`.

### 1.2 Roles (13 `ObjectRole` codes)

Source: role table in `v2:docs/architecture/core/objects/README.md`; codes in
`v2:core/crates/layerfs-content/src/object/output.rs`.

| Code | Role | Value magic | References |
| --- | --- | --- | --- |
| 1 | WholeFile (file below the cutoff) | `LFS5SML\0` v1 | none |
| 2 | Chunk (CDC payload, ≤ 32,768 B raw) | `LFS4CHK\0` | none |
| 3 | ExtentLeaf (mapping tree leaf: chunk ID, offset, length) | `LFS4MAP\0` v3, `0x08` | chunk IDs |
| 4 | ExtentBranch | `LFS4MAP\0` v3, `0x09` | child pages |
| 5 | FileState (93 B: length, extent count, level, CDC profile ID, mapping root) | `LFS4MAP\0` v3, `0x0a` | mapping root |
| 6 | InodeLeaf (≤ 100 rows of serial + 73 B inode value) | `LFS6INT\0` v1 | content and metadata roots |
| 7 | DirectoryLeaf (name → serial rows) | `LFS6NSP\0` v1 | none (serials are not IDs) |
| 8 | DirectoryBranch | `LFS6NSP\0` | child pages |
| 9 | InodeBranch | `LFS6INT\0` | child pages |
| 10 | FilesystemRoot | `LFS6FSR\0` | inode-table root |
| 11 | AttributeLeaf | `LFS4MET\0` | value roots |
| 12 | AttributeBranch | `LFS4MET\0` | child pages |
| 13 | Symlink | `LFS4LNK\0` | none |

- **Merkle structure.**
  - FilesystemRoot → inode table, a B+tree of InodeBranch pages over InodeLeaf pages
    keyed by serial.
  - Each inode row names a content root and a metadata root.
  - Content root:
    - a WholeFile, or
    - a FileState over a B+tree of Extent pages over Chunk objects, or
    - a DirectoryBranch/DirectoryLeaf tree, or
    - a Symlink.
  - Metadata root: an attribute tree, or mode and mtime **inline** in the inode value
    (#360 item 2, commit `8aed51d600`).
  - Tree pages are capped at 8,192 B (`MAX_NODE_OBJECT_BYTES`,
    `v2:.../file/mapping/types.rs`).
  - Commits, Branches and Layers are **history records**, not objects
    (`objects/README.md`).
  - Large-object ceilings: canonical object 16 MiB; produced objects are smaller
    (whole file < cutoff + 23 B, chunk 32,789 B, tree page 8,192 B).

### 1.3 Why the same bytes can yield different roots, and `same_file_bytes`

- **Fresh construction is deterministic.** Equal bytes under one policy give one root,
  whether they arrive as a slice, a stream or data/zero runs, because every route uses
  one builder (`files/README.md` invariants).
- **Localized edits break that equality.**
  - A chunked edit result keeps the base's untouched extents.
  - Extents can be *slices* of stored chunks (offset and length into an unchanged
    chunk).
  - Only the replacement bytes are re-chunked.
  - So the root depends on edit boundaries, and in general differs from fresh
    construction of the same bytes (`files/edits.md` §Plans; test `edit_model.rs`).
- **Filesystem roots depend on more than bytes.** They also depend on scope and serial
  assignment: directory leaves map names to serials, and inode leaves are keyed by
  serial (`objects/README.md`, row "Equal logical content").
- **Advisory predecessors never change bytes, IDs or partitions**
  (`object/predecessor.rs`).
- **`same_file_bytes`** (`v2:core/crates/layerfs-content/src/compare/bytes.rs`, #352 F-073):
  - Equal roots answer immediately; unequal lengths answer false.
  - Otherwise it compares bytes in 256 KiB windows, skipping extents that both files
    take from the same chunk at the same offset.
  - Commit diff and merge use it, so a file written away and back is not reported as a
    content change.
  - The publication lane uses `same_file_bytes_within` with a 1 MiB limit (commit
    `d06a2b81fa`) to keep the lane's hold bounded.
  - Motivating incident (F-073, `core/docs/issues/352/RESULTS.md` at `97a48a646b`):
    equal bytes had roots `ee055170` vs `40ba3671` and were misread as a change.

---

## 2. Content-defined chunking (CDC)

Source: `v2:core/crates/layerfs-content/src/file/cdc/gear.rs`; `files/README.md`
§Chunking.

- **Representation by length** (`v2:.../contract/policy.rs`):
  - empty → a FileState over an empty extent leaf;
  - 1 to cutoff − 1 → one WholeFile object;
  - cutoff and above → chunked.
  - The default cutoff is **131,072 B (128 KiB), exclusive**. A Store may record any
    power of two from 128 KiB to 1 MiB, persisted in `store_policy`.
- **Scanner:** a FastCDC-style, two-byte-per-step rolling **GEAR** hash [constant].
  - `MINIMUM_CHUNK_BYTES = 8_192`, `TARGET_CHUNK_BYTES = 16_384`,
    `MAXIMUM_CHUNK_BYTES = 32_768`.
  - `NORMALIZATION_SHIFT = 2`, `PROFILE_SEED = 0`.
  - Four fixed masks:
    - small `0x0000_d903_0353_7000`;
    - large `0x0000_d901_0353_0000`;
    - plus shifted variants.
  - A frozen 256-entry `GEAR` table.
  - Behaviour by chunk length:
    - below 8 KiB: bytes are copied with no hashing;
    - 8 to 16 KiB: strict (small) masks;
    - 16 to 32 KiB: looser (large) masks;
    - 32 KiB: forced cut.
  - The final EOF chunk may be shorter than 8 KiB.
  - Per byte pair `(a, b)`: `h = (h<<2) + (GEAR[a]<<1)`, test the shifted mask; then
    `h += GEAR[b]`, test the mask (`files/README.md` diagram).
  - Read-buffer boundaries never move a cut (`process_pending_pair`).
  - Memory: one 32 KiB chunk buffer and one 32 KiB read buffer.
- **Profile identity.** `profile_id()` is a digest under `layerfs/fastcdc-profile/v1`
  of the sizes, shift, seed, masks and GEAR table.
  - It is embedded in every FileState's mapping profile.
  - A changed profile fails old file states with `InvalidRecord("file state profile")`.
  - So the partition is frozen per Store format.
- **All-zero runs** (`v2:.../file/cdc/zeros.rs`, `file/construction/runs.rs`,
  `file/mapping/repeated.rs`):
  - A zero stream cuts into full 32 KiB chunks, so its period is 32 KiB.
  - For a hole of length L:
    - the first ≤ 32 KiB is scanned;
    - the interior is `n` identical 32 KiB chunks, handed over as **one repeat count**;
    - the tail is scanned.
  - The partition equals byte-by-byte scanning.
  - `ExtentBuilder::push_repeated_chunk` accepts the payload once and reuses complete
    identical mapping pages: "work depends on boundary pages and tree height, not the
    repeat count."
  - So construction work for a sparse or zero region is logarithmic in its length
    (subtree reuse).
- **Fresh construction entry points:** `construct_bytes`, `construct_stream`,
  `construct_runs` and their `*_with_predecessor` forms
  (`v2:.../file/construction/{bytes,runs,chunk_runs}.rs`).
  - Project Init uses `construct_stream`.
  - Commit uses `construct_runs`, its predecessor form, or an edit.

---

## 3. Exact reuse (CAS)

Source: `v2:core/crates/layerfs-storage/src/save/wave.rs`, `save/publication.rs`;
`physical-storage/README.md` §Saves.

- **Save pipeline.** Objects accumulate in a `PendingBatch`: 512 objects
  (`BATCH_OBJECT_LIMIT`) or `publication_bytes`, which is 4 MiB − 1 on SQLite and
  16 MiB − 1 on PostgreSQL.
- **A wave** locates IDs in slices of ≤ 4,096 (locator cache size) and then admits each
  object:
  1. **Duplicate within the wave.** Same ID seen earlier in this wave: compare
     canonical bytes **and** role byte for byte; any difference is
     `StorageError::Collision`.
  2. **Already stored.** A located object with the same role and canonical length is
     reused and **not read back**.
     - This is owner decision 2026-10-19, #352 record 034 A-1, commit `abc3eb7721`.
     - A role or length mismatch is `Collision`.
     - A corrupt stored copy fails at its first read, because every read authenticates.
     - Measured motivation: on pnpm-first the read-back check was 50,183 read-backs,
       396 MB and 1,543 ms of `Save::accept`'s 2.76 s [measured, commit message
       `abc3eb7721`].
  3. **Pending in an unfinished pack.** The pack is sealed early so the object can be
     resolved within the same Save.
  4. **New.** Every reference must be stored, pending or sealed, else
     `MissingDependency`. Then representation selection runs (section 4).
- **Publication arbitration** (first writer wins):
  - `object_location` is insert-if-absent; `publish` returns `Published { lost }`.
  - Each lost ID is read back and compared byte for byte with what this Save would
    have written; a difference is `Collision` (`save/publication.rs` ~L245–270).
  - Publications are reference-closed and topologically ordered, and split before
    8,191 rows or `publication_bytes`.
- **Stale doc warning.** The 2.0 handbook §4 still says storage "resolves its
  canonical bytes for exact comparison" for an already-located object. That is stale
  since `abc3eb7721`; `physical-storage/README.md` and the code are current. Cite the
  code.
- **Scope of reuse.** CAS reuses only whole canonical objects whose boundaries come
  from chunking and tree construction; it does not find duplicate substrings. Reuse
  spans the whole Store, so it crosses Branches and projects (section 8:
  H05 vs H06 ≈ 16.7×).

---

## 4. Delta encoding

Sources: handbook §5–6; `v2:docs/architecture/core/physical-storage/encoding.md`;
`v2:core/crates/layerfs-storage/src/encoding/{full.rs,codec.rs,delta/select.rs,delta/candidates.rs,delta/read.rs,delta/record.rs}`;
`store/policy.rs`.

### 4.1 Record forms

- **FULL, STORED and PREFIX** apply to payload roles (WholeFile, Chunk). Every
  ordinary (tree/metadata) role is FULL `[0][canonical]` or, for program pages, a
  COPY/INSERT program (section 5).
  - **FULL:** `[0][zstd frame of the raw payload]`.
  - **STORED:** `[2][raw]`.
  - **PREFIX:** `[1][base ObjectId 32 B][zstd frame compressed with the base's raw
    payload as a prefix]`, using `ZSTD_CCtx_refPrefix` / `ZSTD_DCtx_refPrefix`
    (`codec.rs` L11–39).
  - PREFIX is a zstd prefix delta, **not** a stored edit script.
- **Codec settings** [constant, `codec.rs`]:
  - payload records: zstd level 3, chunk window log 20;
  - whole-file window log 18 for cutoffs up to 256 KiB, else 20;
  - ordinary group bodies: zstd level 1, window log ≤ 16;
  - frames carry a checksum; no dictionaries;
  - a static, caller-owned ZSTD context (encode 16 MiB, decode 1 MiB region);
  - `codec.rs` is the only `unsafe` module.
  - The codec was frozen by the owner on 2026-11-06 (`bench:handoff-storage-optimization.md`).
- **Incompressibility probe** [constant, `codec.rs` L72–90; `full.rs`
  `encode_representation`]:
  - A payload over 1,024 B has its first 1,024 B compressed.
  - If that saves ≤ 1/32 (`STORED_PROBE_SAVING_NUM/DEN = 1/32`, chosen just above the
    ~1.3 % frame overhead on 1 KiB), the payload is STORED with no full compression.
  - Otherwise the payload is compressed; if `frame.len() >= raw.len()`, it is STORED
    anyway.
  - PREFIX payloads are never probed.

### 4.2 Selection: one trial, strictly smaller

Selection runs per payload object in `select.rs` (header comment and steps in
`encoding.md`):

1. Prepare the independent record (FULL or STORED) first.
2. If the role's depth cap is 0, keep the independent record.
3. **Candidate** (only one is ever tried):
   - **Chunk:** the first advisory predecessor only.
   - **Whole file:** the declared advisory list first; if it yields nothing, the
     similarity ring (`Candidates::find`/`propose`).
   - 3a. **Losing-base backoff, chunks only** (`LosingBases`, commits `82d22cdced`,
     `66b9a73c67`):
     - After `LOSING_BASE_TRIALS = 8` consecutive losses against one base in this Save,
       chunks offered that base are stored FULL without a trial.
     - The base is retried after gaps of 1, 2, 4, … offers, up to 2^16.
     - A win saving < 1/8 of FULL (`LOSING_BASE_MIN_SAVING = 8`) counts as a loss for
       the backoff, but the smaller record is still stored.
     - The memo holds ≤ 4,096 bases.
4. **Eligibility:** the candidate is stored and has the same role; otherwise the
   independent record is kept.
5. **Chain budget and re-base** (#89, `27f72e3823`):
   - Trigger: the candidate's chain is at the depth cap, **or** chain plus target
     exceeds a budget, judged from stored record sizes.
   - Then the base becomes the FULL root of that chain; the trial runs against the
     root, so the new object sits at depth 1.
   - If an independent base alone exceeds the budget, keep the independent record
     (#473 / `6ac996cd7d`).
6. **One trial:** acquire and authenticate the base chain, build one PREFIX frame, and
   re-check the chain plus object against the budgets.
7. **Compare:** keep PREFIX only if its **record length is strictly smaller** than the
   independent record; ties keep the independent record.
   - Corruption, provider failure or a failed acquisition after a candidate is chosen
     is an error, not a fallback.

- **No PREFIX can win** when the base ID (32 B) plus a minimal zstd frame (≥ 14 B) is
  not narrower than FULL (`prefix_cannot_win`, `delta/record.rs`). The ring proposal is
  then dropped and no seal is forced.

### 4.3 Candidate sources

| Source | Applies to | Mechanism | Citation |
| --- | --- | --- | --- |
| Whole-file edit (`OriginalBase`) | whole file | the edited base root | `v2:.../file/edit/apply.rs` |
| Localized edit **Aligner** (#352 F-005) | chunks | walks the base mapping forward once and re-anchors at every new chunk equal to an old payload; otherwise offers the old payload sharing most bytes with the chunk, else the successor of the nearest unchanged neighbour; window: 64 old extents ahead, 128 held | `v2:.../file/mapping/align.rs`; commit `48ad603b14` |
| **Rename-over predecessor** (#352 F-063) | chunks (F-063); whole files too since #88 | `rename()` of a regular file over a regular file records `(moved, replaced)` in the Workspace overlay (`fs_predecessor`); a chain of renames before one Commit resolves to the base file; Commit's `construct_runs_with_predecessor` offers each chunk the replaced file through the same Aligner | commit `2de43d0a02`; handbook §5.3 |
| **Recreate-at-path** (#88) | whole file and chunks | a fresh file with no recorded predecessor is offered the regular file the base held at its path (`rm f; write f`, as `git checkout` does); at most 65,536 lookups per Commit | commit `714de4a16f` |
| Small-file predecessor (#88) | whole file | a whole-file result with a predecessor declares the base root as an explicit advisory predecessor without reading it | `714de4a16f` |
| `construct_bytes_with_predecessor` | chunks | positional overlap with a caller's base; ≤ 4,096 base extents | `v2:.../file/mapping/predecessor.rs` |
| **Similarity ring** | whole file only, when no declared candidate is usable | see below | `v2:.../encoding/delta/candidates.rs` |

**Similarity ring details** [constant; sizing measured in the code comments of
`candidates.rs`]:

- **Signature:** the eight smallest distinct mixed rolling hashes over 16-byte windows
  (polynomial base 257), each folded to its low 32 bits; 32 B persisted. This is a
  bottom-k / min-hash resemblance sketch.
- **Index:** 8,192 slots (68 B each) plus a 65,536-entry u16 reference table;
  `INDEX_BYTES = 704 KiB`.
  - Persisted in `content_signature` (slot 0–8191, stamp), upserted inside the
    publication transaction.
  - Lifetime: the Store, not the Save.
- **Measured sizing** (W2 evidence, #188d, commit `792715f00a`, quoted in the code
  comment):
  - The retained-history lane's 44,148 whole-file objects fill the ring to 7,614 of
    8,192 slots, and coverage is 36,544 objects at 8,192, 16,384 and 32,768 slots
    alike.
  - 32-bit folding gives byte-identical coverage to 64-bit hashes.
  - References at 16,384 covered 29,706 objects. Doubling to 32,768 and then to 65,536
    added only +492 and then +184 (the recall knee).
- **Ring admission:**
  - Independent (FULL) winners are admitted.
  - Since #88 (`a634941b63`, `a3077b8c1a`), PREFIX winners whose ring-proposed base was
    admitted **in an earlier Save** are also admitted, at most `ADMITTED_PER_SAVE = 1,024`.
  - A Save is never proposed its own winner; the earlier-Save base is substituted, to
    avoid early seals.
  - A tie in overlap prefers the newer entry.
  - Objects stored against a declared base never enter the ring.
  - Note: `encoding.md` mentions only FULL winners; the handbook and code include the
    #88 PREFIX admission.

### 4.4 Depth caps and chain budgets [constants]

- **Depth caps** (`ConstructionPolicy`, `v2:.../contract/policy.rs`; `store_policy`
  columns):
  - whole file **16**, chunk **8**, metadata **8**;
  - accepted range 0–50; 0 disables deltas for the role;
  - recorded per Store at creation.
  - Doubled from 8/4 for new Stores by owner direction on 2026-10-17, commit
    `4ab6e3c7b4`.
- **Payload chain budgets** (`store/policy.rs`, `CHAIN_CANONICAL_CUTOFFS = 4`,
  `CHAIN_ENCODED_CUTOFFS = 2`; #472, `3b10fef9cb`):
  - canonical work ≤ **4 × cutoff** (512 KiB at 128 KiB; 4 MiB at 1 MiB);
  - encoded work ≤ **2 × cutoff** (256 KiB);
  - tested as `chain decoded + target length ≤ 4T` and `chain encoded + target length
    ≤ 2T`.
- **Metadata budgets:** 65,536 canonical bytes (`8 × 8_192`) and 139,281 encoded bytes
  (`17 × 8_193`).

### 4.5 Chain resolution and authentication

Source: `v2:.../encoding/delta/read.rs` `Resolver::resolve_charged`; `encoding.md`
§Decoding.

- **Walk.** Locator → read the record's base ID → locate the base → check role and
  depth → repeat, iteratively (no recursion).
- **Checks while walking:**
  - visibility ceiling (pack ID);
  - no repeated ID (cycle) and depth within the cap;
  - same role as the base;
  - canonical and encoded work within budgets.
- **Decode, base first:** each element is decompressed against the raw payload of its
  base, rebuilt to canonical form, and `ObjectId::for_bytes(...)` is checked against
  its locator ID.
  - Failure: `Integrity("dependency identity")`.
- The base ID inside the PREFIX record is the only stored copy of the dependency edge.
- **Whole-pack checks:** a whole pack crossing the port is length- and SHA-256-checked.
- **Selected-unit reads:** each object used is authenticated by ID; unread bytes in the
  pack are not audited.

---

## 5. Metadata encoding

Sources: `objects/README.md` §Inode leaves; `encoding.md` §pool;
`v2:core/crates/layerfs-storage/src/encoding/pool/{delta,leaf,branch,read}.rs`;
`save/pooled.rs`; handbook §5.4.

- **Inode value.** 73 bytes [constant]:
  - kind byte (`0x80` bit = inline metadata);
  - namespace reference count u64;
  - content root (32 B);
  - either an attribute root (32 B) or inline mode u32 / mtime s i64 / ns u32 plus
    16 zero bytes.
- **Inode leaf.** Envelope (13 B) + node header (31 B) + count × (serial u64 + 73 B
  value); 1–100 rows, ≤ 8,192 B.
- **Value pooling:**
  - The physical leaf body is the 44 B pooled prefix + count × (serial u64,
    **ordinal u32**).
  - Each distinct 73 B value is stored once under an ordinal. In 2.0 these live in
    `inode_value_block` rows (section 7).
  - Ordinals are reused for the same value within a base leaf or a Save (memo
    ≤ 131,072, keyed on the whole value since #91, commit `017507ccf2`).
  - Values are published before the leaves that name them.
  - Reading rebuilds the canonical leaf and checks its ID. Ordinals are physical only,
    never a substitute for authentication.
- **COPY/INSERT leaf and page delta grammar** (`pool/delta.rs`):
  - record `1 | base id | out_len | count | program`;
  - a 4,096-bucket table over 16-byte seeds;
  - 128 KiB match budget (`METADATA_MATCH_BUDGET_BYTES`); an exhausted budget yields
    no program, not an error;
  - program ≤ 64 KiB;
  - kept only if strictly smaller than FULL.
- **Roles that take programs:**
  - InodeLeaf (pooled leaf body);
  - InodeBranch, since #360 item 3, commit `7ac7343c7e`;
  - DirectoryLeaf, DirectoryBranch, ExtentLeaf and ExtentBranch, since #92, commit
    `1583b1d71f`, which bumped ordinary pack framing to version 26.
  - Candidate: the first advisory predecessor.
    - directory pages: the page replaced, or split from or merged into;
    - extent pages: the earlier version's page at the same level and logical position
      (`file/mapping/hints.rs`).
  - A directory program is kept only if it saves ≥ 25 % (`c5604ef662`).
  - The metadata depth cap is 8, with re-base onto the chain root (#89).
- **Field-packed value and hint blocks** (#91, `b973426ee0`, schema v2):
  - Blocks are immutable, append-only and ≤ 4,000 B, with implicit ordinals.
  - Per entry: tag byte, LEB128 reference count, content root, and either the attribute
    root or a per-block mode-table index plus zig-zag mtime delta.
  - Each block ends in a CRC32C.
  - No compression is used (codec frozen); decoding rebuilds the exact canonical bytes.
  - Binding hints get similar blocks.

---

## 6. Localized edits

Sources: `v2:docs/architecture/core/files/edits.md`;
`v2:core/crates/layerfs-content/src/file/edit/*`; `file/mapping/*`.

- **API.**
  - `apply_edits`, `apply_edits_backed` and `apply_indexed_edits_view_backed` all run
    one driver, `apply_view`.
  - The input is a base root plus an ordered `EditSequence` of
    `Edit { start, end, replacement_len }` in current-result coordinates, plus a
    replacement `EditSource`.
  - `read_run_at` may return `FileRun::Zero`, which makes the edits run-aware (zero
    runs use the repeated-chunk path).
- **Plan.**
  1. Open the base view (one root read).
  2. Compare pass: if every replacement equals its base range (64 KiB windows, stop at
     the first difference), return the base root unchanged.
  3. Choose the route by result representation:
     - chunked base and chunked result: split stored nodes at the edit boundaries,
       discard the removed middle unread, CDC-scan only the replacement (each new chunk
       gets an Aligner hint), and concatenate, rebuilding pages only on the join paths;
     - whole-file result: kept ranges plus replacements go into one pre-sized object,
       hinted `OriginalBase`.
- **Extent tree.** A B+tree of 8,192 B extent pages.
  - Leaf entry: 40 B (chunk ID 32 B, u32 source offset, u32 length). Extents may be
    slices of stored chunks.
  - Branch entry: 48 B with a u64 cumulative logical end.
  - FileState: 93 B.
- **Cost model** (`edits.md` §Cost):
  - Mapping pages read grow with the root-to-boundary paths; payloads written grow with
    replacement bytes; pages emitted are the join-path pages.
  - Payload reads happen only in the compare pass for same-length replacements.
  - **[measured, test]** `edit_localized.rs`: one overwrite of three extents reads
    3 pages on an 8 MB file and 4 pages on a 24 MB file, with the same 3 payloads in
    both. **Edit cost is independent of file size.**
  - Exceptions: a whole-file result reads every kept byte (≤ cutoff); a whole-file base
    with a chunked result rescans the result.
- **Memory profiles:**
  - memory: drafts ≤ 8 MiB − 1;
  - backed: records ≤ 64 KiB in a caller backing, no total ceiling; Commit supplies
    the backing.
- **Rename-over:**
  - Editors, git (`index.lock` → `index`) and package managers write a new inode and
    rename it over the old file, so the edit route never sees a predecessor.
  - F-063 records the replaced file at rename time with no Store I/O.
  - Commit then constructs the fresh file with `construct_runs_with_predecessor`, which
    gives chunk delta bases through the Aligner (section 4.3).
  - This is not localized-edit structure: the file is fully re-chunked, but its chunks
    get delta bases.

---

## 7. Packs and the Store

### 7.1 Packs (`v2:docs/architecture/core/physical-storage/README.md` §Packs; `pack/layout.rs`)

- **Header and directory.**
  - Header: `"LFPACK\0\0"`, version, group count, used length, reserved word.
  - A directory follows, then group bodies.
  - Each group body: count | record end offsets | records.
- **Four lanes:**

| Lane | Contents | Framing version written | Group codec |
| --- | --- | --- | --- |
| Ordinary | tree and metadata roles, pooled leaves, program pages | 26 | raw or zstd level 1, kept only if smaller |
| Chunk | chunk payload records | 25 | raw (records are already compressed) |
| WholeFile | compact whole-file records, no length fields (raw = canonical − 23) | 23 | raw |
| Singleton | one whole-file record too large for a 256 KiB pack | 16 | raw |

- **Limits** [constant, `store/policy.rs`]:
  - `PACK_LIMIT` 256 KiB;
  - `GROUP_TARGET` 48 KiB / 512 KiB canonical;
  - ≤ 16 groups per written pack (256 readable);
  - ≤ 8,191 records per group;
  - singleton pack ≤ 16 MiB + 4,096.
  - All lanes share a 256 KiB / 512-row seal queue.
- **Immutability.** A sealed pack is never appended to. SQLite triggers refuse UPDATE
  and DELETE on `pack` and `pack_unit` (`v2:engine/crates/layerfs-persistence/sql/sqlite/objects*.sql`).
  - There is no GC or repack: published objects are never deleted
    (handbook §9 "No garbage-collection protocol"; #99 run-removal blocked on #42).

### 7.2 Store schema (SQLite) (`v2:docs/architecture/engine/store/sqlite.md`; `sql/sqlite/objects_units.sql`)

- **Physical tables:**
  - `pack` (id, domain, SHA-256 digest, length, and either the body or a control
    region in the group-row layouts);
  - `pack_unit` (one row per group: `pack_id`, `group_number`, `offset`, `body`);
  - `object_location` (`object_id` BLOB PK `WITHOUT ROWID` → role, canonical_length,
    pack_id, group_number, record_number);
  - `inode_value_block`;
  - `content_signature` (the ring);
  - `store_policy` (cutoff, three depths, `next_pack_id`, `next_ordinal`);
  - `store_totals`.
- **History tables:** `layer_stack`, `layer`, `"commit"`, branch, lineage,
  `file_digest`, `binding_hint_block`, and others.
- **Pack layouts:** Monolithic (`user_version` 36), GroupRows (37) and GroupRowsIndexed
  (38); 42–44 with Init acquisition tables. All tables are `STRICT`.
- **Disposable profile** (`sqlite-wal-off-v2`), the only profile permitted:
  - `journal_mode=wal`, `synchronous=OFF`, `wal_autocheckpoint=0` (a dedicated
    checkpoint thread runs PASSIVE checkpoints past 4 MiB of WAL);
  - `page_size=4096`, `busy_timeout=0`.
  - Durable execution: `NOT_RUN — disabled by owner until explicit reauthorization`
    (AGENTS.md).
- **Idle TRUNCATE checkpoint** (#86, `48168dd625`): a quiesced Store's WAL is 0 B.

### 7.3 One global database, one writer, publication lane

Sources: `v2:docs/architecture/engine/store/README.md`; `sqlite.md` §Lane; AGENTS.md
"One global database".

- All committed state lives in one SQLite file or one PostgreSQL schema.
- **One writer, in order:**
  1. a per-session FIFO admission turn;
  2. a cross-process lock: `flock` on the Store's parent directory (SQLite), or the
     `store_lane` row (PostgreSQL);
  3. `BEGIN IMMEDIATE` with `busy_timeout=0`.
- **Publication lane.** `publish_lane()` holds the turn and the lock across the
  Commit's head check, merge step and publish as one step. Heavy work (construction,
  encoding, pack sealing) happens before the lane; reads never take it.
- Work in the lane that grows with tree size, history or Branch count is defined as a
  defect.
- **Locator rule.** Locators are insert-only; the first publication wins and losers
  are compared (section 3).

### 7.4 PostgreSQL backend (2.0 only)

Source: `v2:docs/architecture/engine/store/postgresql.md`.

- One schema per Store, behind the cargo feature `postgresql`.
- **Pack bodies:** one `bytea` per pack, `STORAGE EXTERNAL` (uncompressed, so ranged
  `substring` reads work); no `pack_unit` table.
- **Narrow `object_location`** (#90, `79c911214b`):
  - no unique key on the ID; a B-tree on the first 8 bytes, with the full ID compared
    in the heap;
  - uniqueness is guaranteed by inserting only when absent under the lane.
- Short columns are `STORAGE PLAIN`, so those tables have no TOAST relation.
- `SCHEMA_VERSION = 2`.
- `publication_bytes` is 16 MiB − 1; `whole_pack_bytes` is 256 KiB.
- `synchronous_commit` is adjustable per write transaction, default off.
- No seal, snapshot or checkpoint operations apply.

### 7.5 Store overhead sources (beyond packs)

- **Definition** (`bench:result/2026-11-06/fs-storage-quick/7691ac20b0.../report.md`):
  "Store overhead" = stored ÷ Σ new_stored, covering database pages, indexes, free
  space, WAL and per-row headers.
- **Contributors:**
  - per-object `object_location` rows (≈ 32 B ID + 5 integers, plus B-tree);
  - per-file `file_digest` rows (#91 now keeps them only for chunked files:
    "D10: 15,524 of 15,535 rows were small", commit `54486095d6`);
  - inode values (#91 blocks: 40.3 B per entry, `bench:handoff-storage-optimization.md`
    table row #91);
  - binding hints (28.8 B per entry, same source);
  - ring rows, history rows;
  - SQLite WAL (up to 4 MiB before #86);
  - PostgreSQL tuple headers (~28 B per row), dead tuples from Init working tables
    (#87), and TOAST/heap page slack.
- **Final quick results** [measured, `bench:.../fs-storage-quick/7691ac20b0.../report.md`]:
  - SQLite overhead 1.01× (Q5) to 1.21× (Q4);
  - PostgreSQL 1.05× to 1.71×;
  - Q1 (2,000 tiny files): raw ÷ stored is only **0.90×** on SQLite and 0.80× on
    PostgreSQL, so the Store is *larger* than the raw bytes for tiny files.

---

## 8. Storage optimization history (measured)

Sources: commit messages on 2.0 (hashes given); `layerfs-project#84` lead summary
(2026-11-06) and sub-issue result comments; `bench:result/2026-11-06/...` reports.

### 8.1 Before #84 (chronological)

- **2026-09-08 `1aa40785a7`, bounded depth-one deltas and encoded-group selection.**
  Earliest delta implementation in this lineage; no byte number recorded here.
- **2026-09-20 #188d (W2), similarity ring sizing.** 8,192 slots, 65,536 references,
  32-bit folding, 704 KiB.
  - [measured, code comment in `candidates.rs`]: coverage 36,544 of 44,148 whole-file
    objects, flat above 8,192 slots; the previous 32,768 slots "held 24,576 slots
    nothing could ever use".
- **2026-09-21 #219, `a1faf957ed`, compact whole-file lane grouped.**
  - [measured, commit message]: whole-file lane 9,444 → 519 groups (8,925 fewer
    statements and pack writes) for 4 B per record.
  - #286 (`290ddb1800`) tightened whole-file pack directories to 16 slots.
- **2026-10-17 `4ab6e3c7b4`, default depths doubled** (chunk 4 → 8, whole file 8 → 16)
  for new Stores. No byte number in the commit.
- **2026-10-18 #352 F-005, `48ad603b14`, localized-edit Aligner.**
  - [measured, commit message; range-4 fork, 1 MiB file]:
    - insert near the start: 37,029 → 3,941 B stored (0/3 → 3/3 chunks as deltas);
    - middle edit: 25,649 → 4,785 B.
- **2026-10-18 #360 item 2, `8aed51d600`, inline mode/mtime in the inode value.** An
  mtime-only change is no longer a change. No byte number in the commit.
- **2026-10-19 #360 item 3, `7ac7343c7e`, InodeBranch COPY/INSERT deltas.**
  - [measured, fix record commit `040f517c85`; synthetic P3, 100 `fs.write` Commits,
    53,611 entries]:
    - InodeBranch 2,894 → 338 B per Commit (−88 %);
    - pack bytes 3,138 → 630 B per Commit (−80 %);
    - Store growth 15.4 → 12.3 KB per Commit (−20 %).
- **2026-10-19 #352 A-1, `abc3eb7721`, exact reuse trusts the content address.**
  Speed, not bytes: it removed 50,183 read-backs, 396 MB and 1,543 ms of 2.76 s on
  pnpm-first.
- **2026-10-20 #352 F-063, `2de43d0a02`, rename-over chunk predecessor.**
  - [measured, commit message; r42 sealed clone, steps 2–25]: stored
    1,463,988 → 297,095 B (−80 %); git index chunks 120 FULL → 10 FULL + 110 PREFIX.
  - [measured, `core/docs/issues/352/RESULTS.md` @ `97a48a646b` L910–918]: `.git`
    stored per replay range 13.1–23.1 MiB (r46–50) → 7.2–12.2 MiB (r51–60). This is a
    different-range comparison, not an A/B.
  - RCA (`534e9ae414`): before the fix, `.git` cost was index versions 32–43 %, loose
    objects 29–35 % and range packs 21–26 %.
- **2026-10-20 #352 F-073, `5b32a15773`/`366f23222b`/`d06a2b81fa`, `same_file_bytes`.**
  A correctness fix, not bytes.
- **2026-10-26 `6ac996cd7d` (#473) and 2026-10-28 `3b10fef9cb` (#472), budget fixes.**
  - #473: store FULL when a base alone exceeds the budget.
  - #472: budgets scale as 4T/2T, so large whole files at a raised cutoff keep deltas.
  - Default-cutoff Stores are unchanged. Correctness/coverage only; no byte number.

### 8.2 The #84 storage-efficiency epic (2026-11-06)

- **Phase 0 base:** `b4fa173e40`. **Phase 2 base:** `02133596ae` (after #86–#89).
  **Final integration:** `7691ac20b0`, tree equal to merged `3a44cc1b94`.
- The codec was frozen, so all savings come from base selection, programs and layout.

| Issue / commit | Change | Measured effect (exact) | Source |
| --- | --- | --- | --- |
| #86 `48168dd625` (+ fix `8a23d02d11`, PR #593) | SQLite `wal_checkpoint(TRUNCATE)` when a Store goes idle | idle/closed WAL 4.19 MB → 0; Q1 SQLite net 8,798,208 → 4,538,368 B (−48.42 %), but **late (closed) net only 4,796,416 → 4,538,368 (−5.38 %)**: most of the Q1 headline is WAL accounting | #84 summary; quick final report "Before/after against Phase 0" |
| #87 `c86b6bbe34` | PostgreSQL Init: TRUNCATE acquisition tables on last release | Q6 PostgreSQL net immediate 38,158,336 → 28,917,760 B (−24.2 %); late 31,358,976 → 28,917,760 (−7.8 %) | layerfs-project#87 result comment |
| #89 `27f72e3823` | Re-base onto the chain root at the depth cap or budget, instead of storing FULL (payload and inode branch) | Q2 edit-Commit payload 290,078 → 119,925 B (−58.7 %); Commit-9 spike 67,666 → 12,534 B; Q6 Σ new_metadata 52,017 → 41,936 B (−19.4 %); read amplification: average final decode +22.5 %, max 520,575 B ≤ 524,288 budget. Offline M03 diagnostic: 2,585,612 → 1,201,562 published pack bytes; average reconstruction 212,662 → 260,555 B | layerfs-project#89 result comment; handbook §6 |
| #88 `714de4a16f`, `a634941b63`, `a3077b8c1a` | Whole-file base on rename-over and recreate-at-path; ring admits cross-Save PREFIX winners; newest-entry tie-break | Q4 Σ payload 808,429 → 449,377 B (−44.4 %), within 3 % of Q2; Q3 payload equal to Q2; edit Commits 7.3–14.6 KB vs 24–57 KB before. Rejected first version inflated Q6 Init (~4× packs, +635 KB SQLite) | layerfs-project#88 comments; commit messages |
| #92 `1583b1d71f`, `c5604ef662` | COPY/INSERT programs for directory and extent pages; ordinary framing v26 | Q5 Σ new_metadata −40.06 %; Q6 −14 %; history Σ new_metadata −18.8 % (H01) to −41.4 % (H03) with payload unchanged | quick final report Observations; #84 summary; history final report |
| #91 `b973426ee0`, `54486095d6`, `017507ccf2` | Schema v2: append-only ≤ 4,000 B field-packed blocks for inode values and binding hints; `file_digest` only for chunked files | Q1 −5.5 % / −13.7 %; D10 PostgreSQL 70.4 → 61.0 MB | #84 summary table |
| #90 `79c911214b`, `55383742f7` | PostgreSQL narrow `object_location` with an 8-byte-prefix index; PLAIN short columns | D10 PostgreSQL 58.4 → 54.4 MB. Measured and *not taken*: 8,100 B PLAIN pack chunk rows (44.0 MB vs 43.2 MB with EXTERNAL) | #84 summary; commit message |
| #93 | zstd level 6 / dictionary | not done (codec frozen) | handoff |
| #99 | sorted-block runs for locations | not done (blocked on #42 run-removal) | handoff |

### 8.3 Post-#84 follow-ups (2026-11-07, speed with small byte cost)

- **`82d22cdced`, losing-base backoff.**
  - [measured, Linux release, 3 runs, median]: ADD500 (500 MiB random append) trials
    26,966 → 22; Commit 7,877 → 5,493 ms; stored bytes identical (525,796,005).
- **`66b9a73c67`, wins under 1/8 count as losses.**
  - [measured]: TXT 100 MiB log append Commit 1,540 → 977 ms (−37 %); stored
    23,430,713 → 23,675,507 B (+1.04 %). This is an explicit trade under AGENTS §3.9.

### 8.4 Phase 0 → final deltas (`b4fa173e40` → `7691ac20b0`, Docker, one sample per row)

**fs-storage-quick** [measured]
(`bench:result/2026-11-06/fs-storage-quick/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`):

| Case | Backend | Net Phase 0 (B) | Net final (B) | Δ % | Late net Δ % | Σ new_stored Δ % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Q1 2,000 tiny files | SQLite | 8,798,208 | 4,538,368 | −48.42 | −5.38 | +0.00 |
| Q1 | PostgreSQL | 6,111,232 | 5,136,384 | −15.95 | −16.22 | +0.00 |
| Q2 in-place 1 % edits ×12 | SQLite | 1,726,312 | 495,616 | −71.29 | −26.67 | −27.79 |
| Q2 | PostgreSQL | 942,080 | 688,128 | −26.96 | −26.96 | −27.79 |
| Q3 rename-over | SQLite | 2,253,672 | 503,808 | −77.65 | −45.09 | −45.61 |
| Q3 | PostgreSQL | 1,343,488 | 704,512 | −47.56 | −47.56 | −45.61 |
| Q4 new path per version | SQLite | 2,261,912 | 552,960 | −75.55 | −39.73 | −44.03 |
| Q4 | PostgreSQL | 1,343,488 | 778,240 | −42.07 | −42.07 | −44.03 |
| Q5 32 MiB random + 5 edits | SQLite | 37,605,376 | 35,037,184 | −6.83 | −0.18 | −0.16 |
| Q5 | PostgreSQL | 36,691,968 | 36,511,744 | −0.49 | −0.49 | −0.16 |
| Q6 DSH Init + 23 Commits | SQLite | 27,651,760 | 23,691,264 | −14.32 | −2.77 | −22.29 |
| Q6 | PostgreSQL | 38,158,336 | 25,640,960 | −32.80 | −18.23 | −22.29 |

- **Caveat:** SQLite "net" in Phase 0 included up to 4 MiB of uncheckpointed WAL. The
  **late (closed)** column is the like-for-like SQLite comparison.
- Q2 delta + zstd factor: 19.86× (Phase 0, `bench:.../b4fa173e40.../report.md` L15)
  → 27.51× (final).

**fs-storage-history** [measured]
(`bench:result/2026-11-06/fs-storage-history/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`):

- Net Store bytes: SQLite −2.93 % to −17.93 %; PostgreSQL −18.03 % to −53.26 %.
- Σ new_metadata −18.8 % (H01) to −41.4 % (H03); **payload identical**.
- PostgreSQL ÷ SQLite 1.07–1.10× (Phase 0: 1.11–2.22×).
- H01 (500 Commits) SQLite net 32,280,576 → 26,492,928 B (−17.93 %).
- H05 PostgreSQL 64,815,104 → 30,294,016 B (−53.26 %).
- H06 PostgreSQL 747,053,056 → 509,919,232 B (−31.74 %).

**Other suites** [measured, `layerfs-project#84` lead summary]:

- micro: SQLite −3.4 % to −73.1 %, PostgreSQL −0.4 % to −63.0 %.
- dsh D10: −16.0 % (SQLite) / −23.9 % (PostgreSQL).

**Absolute efficiency, final build** [measured, history report "References"]:

- H01, 500 Commits on a 65.0 MB / 9,404-file tree: raw Σ logical 32,477.4 MB, stored
  26.5 MB (SQLite), **1,225.9×**.
- Growth after Init: 5,939 B per Commit (SQLite); new_stored 2,990 B per Commit, of
  which payload 1,033 and metadata 1,957 (**metadata dominates per-Commit cost**).
- Comparators:
  - net ÷ Git pack + idx of the same states = 1.10–1.31× (SQLite), 1.19–1.40×
    (PostgreSQL), so LayerFS is 10–40 % larger than `git gc`-style packs;
  - net ÷ approximate CDC oracle 0.96–1.19× (SQLite), 1.06–1.27× (PostgreSQL).
- Cross-project sharing in one Store: Σ H06 ÷ H05 = 16.65× (SQLite), 16.83×
  (PostgreSQL).
- Per-operation diagnostic (least squares, not a measurement): delete costs about
  1,100 B of metadata; edit about 414 B of new_stored.

---

## 9. Standard vs LayerFS-specific (candid)

**Standard or well known, cite prior art:**

- **Content-addressed Merkle DAG of immutable objects with domain-separated hashes.**
  Git, Venti, IPFS, restic/borg, Nix. BLAKE3 is a stock choice.
- **CDC with GEAR / FastCDC normalized chunking** (Xia et al., USENIX ATC 2016).
  - The parameters 8/16/32 KiB and normalization level 2 are FastCDC's recommended
    shape (the spec itself calls it FastCDC; profile domain `fastcdc-profile/v1`).
  - Small-file whole objects below a cutoff are also common (restic, casync variants).
- **Delta compression against a base with depth limits.** Git packfile deltas with
  `--depth` (default 50) and window search; xdelta/vcdiff COPY/INSERT. The metadata
  COPY/INSERT grammar is a classic copy/insert delta.
- **zstd prefix/dictionary deltas.** `--patch-from` / `refPrefix` is a documented
  zstd feature.
- **Resemblance detection with min-hash / bottom-k sketches** (Broder 1997;
  "super-features" in Shilane et al. stream-informed delta, FAST 2012; Finesse). The
  8-hash bottom-k over 16-byte windows is standard.
- **Packing small objects into larger immutable packs with an index** (Git packs, Venti
  arenas). Storing packs as database rows has precedent in database-backed object
  stores.
- **Incompressibility probes** (sampling a prefix before compressing) are common in
  backup tools and ZFS-style early abort.
- **Single-writer SQLite with WAL.**

**Specific to LayerFS (defensible novelty, stated modestly):**

1. **Placement inside the per-Commit path.**
   - Representation selection runs inline in every Commit's Save, with bounded work and
     one trial.
   - Git, by contrast, stores loose objects and deltifies later in `gc`/`repack`
     window search.
   - Consequence: no background repack, no GC, append-only immutable packs. Deltas are
     chosen once and never improved.
   - This is a design choice (latency-bounded Commits for agent tool calls), not an
     algorithmic advance.
2. **Filesystem-event-sourced base selection.**
   - Bases come from FUSE overlay events rather than content search:
     - the rename-over predecessor recorded at `rename()` time (F-063);
     - the recreate-at-path lookup (#88);
     - the localized-edit Aligner over the base mapping (F-005);
     - positional extent-page hints (#92).
   - The similarity ring is only the fallback.
   - Git cannot see renames-over (it diffs trees); version-control filesystems rarely
     record them. This is the most distinctive idea, and it is measured (−80 % on git
     index churn; Q3/Q4 −45 %).
3. **Single trial, strictly smaller, with chain re-base at the root.**
   - One candidate and one PREFIX trial per object, kept only if its record is strictly
     shorter.
   - Chains bounded by depth *and* by work budgets that scale with the cutoff (4T
     canonical / 2T encoded).
   - At the limits, re-base onto the chain's FULL root (#89) instead of restarting
     FULL.
   - Losing-base backoff for chunks.
   - Each piece is simple; the combination as a bounded-read-work policy is
     LayerFS-specific. Re-basing onto an older root trades storage for longer average
     reconstruction (+22.5 % average decode, measured).
4. **Canonical framing independent of physical form, with authenticate-on-every-read.**
   - Identity covers envelope and role; FULL, STORED or PREFIX never change the ID.
   - Every decode re-hashes every chain element.
   - Exact reuse *trusts* the address on write (role and length only) and defers
     integrity to the read path. This is a deliberate trade, owner decision
     2026-10-19.
   - Similar in spirit to Git's object ID vs pack representation, but with a stricter
     single-encoding rule and an explicit read-time authentication contract.
5. **Metadata pooling and field-packed blocks.**
   - 73-byte inode values pooled by ordinal, with leaves storing (serial, ordinal);
     COPY/INSERT programs for inode/directory/extent pages.
   - Immutable CRC32C-checked field-packed value blocks (#91) under a frozen codec.
   - Engineering, not new theory, but it addresses per-Commit metadata dominance
     (1,957 of 2,990 B per Commit on H01).
6. **Same bytes, different roots, by design.**
   - Localized edits preserve old extent structure (slices of stored chunks), making
     edit cost independent of file size (3 vs 4 pages on 8 vs 24 MB).
   - This sacrifices canonical-root equality, which `same_file_bytes` restores
     semantically.
   - Similar to extent-based copy-on-write filesystems (btrfs/ZFS), here applied in a
     content-addressed Merkle setting.

**Candid weaknesses to state in the paper:**

- Tiny files: Store bytes exceed raw bytes (Q1 raw ÷ stored 0.90× SQLite, 0.80×
  PostgreSQL).
- 10–40 % larger than Git packs on history workloads.
- No GC, delete or repack: deltas chosen at Commit are permanent.
- PostgreSQL has a persistent 1.07–1.4× overhead over SQLite.
- Most #84 per-mechanism attributions are inferred from counters on one integrated
  build, one sample per row, on shared Docker Desktop.
- The codec was frozen, so zstd level and dictionary effects are unexplored.

---

## 10. main (v1.x) vs 2.0 differences relevant here

Present on 2.0, absent on main:

- PostgreSQL backend;
- #86–#92: re-base, cross-Save ring admission, directory/extent programs at framing
  v26, schema v2 value blocks;
- losing-base backoff;
- payload budgets scaled with the cutoff (#472, after the 2026-10-29 merge base);
- 2.0 docs reorganisation.

Shared:

- object model, CDC profile, depth caps 16/8/8 (from 2026-10-17);
- F-005, F-063, F-073;
- inode-branch deltas (#360);
- A-1 exact reuse.

The main handbook (`docs/general/cas_cdc_deltaencoding_handbook.md`) has 118 fewer
changed lines than 2.0 (`git diff --stat`) and lacks the #88/#89/#92 text. A Store
written by 2.0 (pack framing v26, schema v2) is not readable by main.

---

## Paper-ready storage design summary (~900 words)

LayerFS stores every committed filesystem state as a Merkle DAG of immutable,
content-addressed objects in a single database file (SQLite) or schema (PostgreSQL).

**Objects and identity.**

- Each object is a role-tagged value in a fixed envelope: magic `LFSO`, kind, two
  big-endian lengths, then role magic, version and body.
- Its identity is `BLAKE3("layerfs/object/v2\0" ‖ canonical bytes)`.
- Thirteen roles make up the tree:
  - a filesystem root names a B+tree inode table keyed by serial;
  - each 73-byte inode value names a content root and a metadata root, or carries mode
    and mtime inline;
  - content is a whole-file object, a file-state object over a B+tree of extent pages
    that reference chunks, a directory B+tree mapping names to serials, or a symlink.
- Decoders accept exactly one encoding per value.
- References are derived from bytes, never from callers.
- Physical representation (compression, delta base, pack location) is outside the
  identity. Every read rebuilds canonical bytes and re-hashes them.

**Chunking.**

- Files below a per-Store cutoff (default 128 KiB) are single whole-file objects.
- Larger files are split by a frozen FastCDC-style GEAR scanner: 8 KiB minimum, 16 KiB
  normalization point, 32 KiB maximum, normalization level 2, seed 0.
- The profile is hashed into every file state, so the partition cannot drift silently.
- Zero runs exploit the scanner's 32 KiB period: interior zero chunks are emitted as a
  repeat count, and identical mapping pages are reused, so construction work grows with
  tree height rather than hole length.

**Exact reuse.**

- Objects reach storage in waves. Duplicates within a wave are compared byte for byte.
- An object already located with the same role and length is reused without a
  read-back. Integrity is deferred to the read path, which authenticates every decoded
  object.
- Concurrent writers are arbitrated by insert-if-absent locators. A losing writer
  re-reads the winner and compares it byte for byte; a mismatch is a collision error.
- Reuse spans the whole Store, across Branches and projects.

**Representation selection.**

- Each new payload object (whole file or chunk) gets an independent record first:
  - zstd level 3, or
  - raw (STORED) when a 1 KiB probe saves ≤ 1/32 or compression does not shrink it.
- Selection then makes **at most one** delta trial: a zstd frame compressed with a
  single base payload as prefix.
- The delta is kept only if its full record, including the 32-byte base ID, is strictly
  smaller.
- **Bounded chains.** Depth caps are 16 (whole file), 8 (chunk) and 8 (metadata).
  Payload chains must also fit work budgets of 4× the cutoff in decoded bytes and 2× in
  encoded bytes.
- **Re-base at the limits.** A candidate whose chain reaches a limit is replaced by its
  chain's independent root, so the dependent restarts at depth one rather than storing
  a full copy.

**Choosing the one base: filesystem events first.**

- *Localized edits:* an aligner walks the old extent map and offers each new chunk the
  old chunk with the most overlap.
- *Rename-over:* the write-to-temp-then-rename used by git, editors and package
  managers is recorded in the Workspace overlay when `rename()` runs. The new inode is
  offered the replaced file as its base.
- *Recreate at a path:* a file recreated at a path is offered the file the base held
  there.
- *Fallback:* whole files with no declared base consult a bounded, persisted similarity
  index. It holds 8,192 entries of eight-hash bottom-k signatures over 16-byte windows
  (704 KiB in total), and prefers the newest similar version.

**Metadata.**

- Inode values are pooled: leaves store (serial, ordinal) pairs.
- Values live in immutable, field-packed, CRC-checked blocks of at most 4,000 bytes.
- Inode-table branches, directory pages and extent pages may be stored as COPY/INSERT
  programs against the page they replace, under the metadata depth cap.

**Localized edits.**

- An edit splits stored extents at its boundaries, discards the replaced middle unread,
  chunks only the replacement bytes, and rebuilds pages only on the join paths.
- Unchanged extents keep their identities, including slices of stored chunks.
- Edit cost is therefore independent of file size: 3 pages read on an 8 MB file and 4
  on a 24 MB file for the same overwrite.
- The price is that equal bytes can have different roots. Diff and merge therefore
  compare bytes, skipping shared extents, rather than roots.

**Packs and the Store.**

- New records are grouped by lane: ordinary (tree pages, with zstd-1 group
  compression), chunk, compact whole-file, and singleton.
- Groups are sealed into append-only packs of at most 256 KiB, keyed by SHA-256 and
  stored as database rows. Triggers forbid updating or deleting them.
- A locator row maps each object ID to (pack, group, record).
- **One writer.** All history and objects live in one database with exactly one
  writer:
  1. an in-process FIFO turn;
  2. a directory `flock` (SQLite) or a lane row (PostgreSQL);
  3. `BEGIN IMMEDIATE`.
- Construction, encoding and sealing run before the publication lane; only the head
  check, merge step and atomic publish hold it.
- SQLite runs WAL with `synchronous=OFF` (the only permitted profile) and truncates the
  WAL when the Store goes idle.
- There is no garbage collection or repacking: deltas chosen at Commit time are
  permanent.

**Effect.**

- On a 9,404-file, 65 MB repository history, 500 seeded Commits raise the Store by
  about 5.9 KB per Commit. 2,990 B of that is new packs, two thirds of it metadata.
- The Store holds 1,226× less than the sum of all snapshots.
- It is 1.16× the size of a Git pack of the same states (SQLite).
- A storage-efficiency round that changed only base selection, metadata programs and
  layout (the codec was frozen) cut net Store bytes:
  - by 3–18 % (SQLite) and 18–53 % (PostgreSQL) on history workloads;
  - by 27–78 % on small-file edit sequences;
  - with payload bytes unchanged and metadata down 19–41 %.

---

## Table: optimizations and measured effects

All numbers are exact as reported in the cited source. "One sample" applies to every
benchmark row. Wall times are INELIGIBLE diagnostics.

| # / commit | Date | Optimization | Measured effect | Source |
| --- | --- | --- | --- | --- |
| #188d (W2), ring sizing | 2026-09-20 | Similarity ring: 8,192 slots, 65,536 refs, 32-bit folding, 704 KiB | 36,544 of 44,148 objects covered, flat above 8,192 slots; 64 → 32-bit fold changed coverage by 0 | `v2:core/crates/layerfs-storage/src/encoding/delta/candidates.rs` comments |
| #219 `a1faf957ed` | 2026-09-21 | Grouped compact whole-file lane | whole-file lane groups 9,444 → 519 (−8,925 statements and pack writes), +4 B per record | commit message |
| `4ab6e3c7b4` | 2026-10-17 | Default depths: whole file 8 → 16, chunk 4 → 8 | none recorded | commit message |
| #352 F-005 `48ad603b14` | 2026-10-18 | Localized-edit Aligner: chunk bases by byte overlap | 1 MiB file: insert 37,029 → 3,941 B; middle edit 25,649 → 4,785 B | commit message |
| #360 item 3 `7ac7343c7e` | 2026-10-19 | InodeBranch COPY/INSERT deltas | InodeBranch 2,894 → 338 B per Commit (−88 %); packs 3,138 → 630 B per Commit (−80 %); Store growth 15.4 → 12.3 KB per Commit (−20 %) | `040f517c85` (fix record 030) |
| #352 A-1 `abc3eb7721` | 2026-10-19 | Exact reuse trusts the address (no read-back) | speed: −50,183 read-backs, −396 MB read, −1,543 ms of 2.76 s (pnpm-first) | commit message |
| #352 F-063 `2de43d0a02` | 2026-10-20 | Rename-over chunk predecessor | 1,463,988 → 297,095 B (−80 %); index chunks 120 FULL → 10 FULL + 110 PREFIX; `.git` per range 13.1–23.1 → 7.2–12.2 MiB (different ranges) | commit message; `RESULTS.md` @ `97a48a646b` |
| #473 `6ac996cd7d`, #472 `3b10fef9cb` | 2026-10-26/28 | FULL when a base alone exceeds the budget; budgets scale 4T/2T | correctness/coverage at raised cutoffs; default Stores unchanged | commit messages |
| #86 `48168dd625`, `8a23d02d11` | 2026-11-06 | Idle WAL TRUNCATE checkpoint (SQLite) | WAL 4.19 MB → 0; Q1 net −48.42 % but late −5.38 % | #84 summary; quick report |
| #87 `c86b6bbe34` | 2026-11-06 | PostgreSQL Init table TRUNCATE | Q6 PostgreSQL 38,158,336 → 28,917,760 B (−24.2 %) | layerfs-project#87 |
| #89 `27f72e3823` | 2026-11-06 | Chain-root re-base | Q2 edit payload 290,078 → 119,925 B (−58.7 %); Q6 metadata −19.4 %; average decode +22.5 % | layerfs-project#89 |
| #88 `714de4a16f`, `a634941b63`, `a3077b8c1a` | 2026-11-06 | Small-file base on rename-over and recreate; cross-Save ring admission | Q4 payload 808,429 → 449,377 B (−44.4 %); Q3 payload = Q2 | layerfs-project#88 |
| #92 `1583b1d71f`, `c5604ef662` | 2026-11-06 | Directory and extent page programs (framing v26) | Q5 metadata −40.06 %; Q6 −14 %; history metadata −18.8 to −41.4 % | quick and history reports |
| #91 `b973426ee0` + | 2026-11-06 | Schema v2 field-packed value and hint blocks; `file_digest` only for chunked files | Q1 −5.5 % / −13.7 %; D10 PostgreSQL 70.4 → 61.0 MB | #84 summary |
| #90 `79c911214b` | 2026-11-06 | PostgreSQL narrow locations, 8-byte-prefix index | D10 PostgreSQL 58.4 → 54.4 MB | #84 summary |
| `82d22cdced` | 2026-11-07 | Losing chunk-base backoff | ADD500 trials 26,966 → 22; stored unchanged | commit message |
| `66b9a73c67` | 2026-11-07 | Wins under 1/8 count as losses | TXT append Commit 1,540 → 977 ms; stored +1.04 % (+244,794 B) | commit message |
| **Phase 0 → final** | 2026-11-06 | #86–#92 combined | quick: Q1 SQLite 8,798,208 → 4,538,368 B; Q3 SQLite −77.65 % (late −45.09 %); Q6 PostgreSQL 38,158,336 → 25,640,960 B. History: Σ new_metadata −19 to −41 %, payload unchanged; net SQLite −2.9 to −17.9 %, PostgreSQL −18.0 to −53.3 %; PostgreSQL ÷ SQLite 1.07–1.10× (was 1.11–2.22×) | `bench:result/2026-11-06/fs-storage-{quick,history}/7691ac20b0.../report.md` |
