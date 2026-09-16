# Bounded-resource-use and memory-safety audit — Stages 3–4 (C1 `layerfs-content`, C2 `layerfs-storage`, `layerfs-telemetry`)

> **Role:** independent reviewer. **Snapshot:** `git HEAD 91c3a0741fff64e8161d5c1b6e759f347ffbf757`, branch `main`.
> `git status --porcelain` at review start printed exactly one line — `?? docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/` (this evidence directory, untracked).
> No product source was modified. No cargo / build / clippy / test / miri / sanitizer / benchmark command was run.
> Method: whole-file reads of the real path, `grep`/`read` only, plus read-only inspection of the retained evidence directories.
> Every claim below carries a `file:line` citation. Arithmetic on `size_of` is **derived from source constants** and labelled as such; nothing is measured unless it says *measured*.

**Headline.** The compiled codec workspace, the pack/group grammar, the telemetry tree and the admitted-FULL candidate cache are genuinely and provably bounded. Three owners in the real save path are **not**: the mapping builder's level ≥ 1 pending entries (`build.rs`), the operation-lifetime pack caches (`owner.rs:111`, `pool/read.rs:32`), and the cleanup transaction under a MEMORY journal (`cleanup.rs`). Separately, the design's declared allocation ledger disagrees with the code in three places (SQLite `cache_size`, `METADATA_INDEX_BYTES`, and the builder's "not by the file length" claim), and the tests that carry the names of the C1 bounds do not assert the properties they are named for.

---

## Part 1 — Bounded resource use

### 1.1 The real path, owner by owner

Format: **owner type | byte/count bound (file:line) | live multiplicity | capacity/transient overlap | lifetime | release event.**

All `size_of` figures are derived from the struct definitions, not measured.

#### C1 — construction and edit

| Owner | Bound (file:line) | Multiplicity | Overlap | Lifetime | Release |
| --- | --- | --- | --- | --- | --- |
| Caller input (`&[u8]` / `R: Read`) | none in core; caller's declared size (`content-io-memory-audit.md:122`) | 1 per operation | coexists with every phase | operation | caller |
| CDC scanner input | `[0u8; MAXIMUM_CHUNK_BYTES]` = 32 KiB **on the stack** (`cdc/gear.rs:81`, `MAXIMUM_CHUNK_BYTES = 32_768` at `gear.rs:17`) | 1 | with chunk buffer and consumer | one scan | stack frame exit |
| CDC chunk accumulator | `Vec::with_capacity(32_768)` (`gear.rs:128`) | 1 | with input buffer | one scan | `Scanner` drop |
| Threshold probe | `try_reserve_exact(cutoff)` = 128 KiB default, ≤ 1 MiB at the widest accepted cutoff (`file/content.rs:195-202`; `content/policy.rs:16-18`) | 1 | **retained for the whole scan** — moved into `Cursor::new(prefix).chain(source)` (`content.rs:209-214`) | whole construction | `construct_chunked` end |
| Whole-file canonical | `whole_file_raw_limit = cutoff − 1`; one allocation (`content.rs:88-92`, `policy.rs:135-144`) | 1 | with the caller's input | until `consumer.accept` | consumer takes ownership |
| `ExtentBuilder` level 0 | `Vec::with_capacity(flush_at + 1)`, `flush_at = 192` (`mapping/build.rs:73-75`, `policy.rs:149`, `policy.rs:199/201`) → 193 × 40 B ≈ 7.7 KiB derived | 1 | with all higher levels | whole build | `finish` consumes `self` |
| `ExtentBuilder` levels ≥ 1 | **NO BOUND — see F1** | 1 per level | with level 0 | whole build | `finish` |
| `EditObjects.deferred` | `EDIT_DEFERRED_LIMIT = 8 MiB − 1` charged, +128 B/object (`edit/tree.rs:31,33,105-139`) | 1 per edit operation | with `ExtentBuilder` of each replacement (`edit/apply.rs:256-267`) | whole edit operation | `EditObjects` drop |
| `EditObjects` `visited` set | `BTreeSet<ObjectId>` of committed node ids (`tree.rs:168,178`) | 1 | with `deferred` | `commit` call | `commit_node` return |
| `EditObjects::read` clone | one node copy per read, ≤ `MAX_NODE_OBJECT_BYTES` 8 KiB (`tree.rs:84`, `mapping/types.rs:19`) | 1 transient | with the map entry + the decoded `ExtentNode` | one `load_node` | after `decode_node_with_context` |
| `child_summaries` | `Vec::with_capacity(children.len())`, ≤ MAX_ENTRIES 128 → ≤ 7 KiB derived (`tree.rs:234-255`) | recursion-depth many (`split`/`concat_inner`, ≤ `MAX_LEVEL` 31, `types.rs:17`) | with `summaries[..i].to_vec()` / `[i+1..].to_vec()` copies (`tree.rs:361-362`) | one split/join step | step return |

#### C2 — save/pack lane

| Owner | Bound (file:line) | Multiplicity | Overlap | Lifetime | Release |
| --- | --- | --- | --- | --- | --- |
| Compression workspace | **2 MiB**, one aligned `Vec<u64>` (`encoding/codec.rs:33,123-133,158`) | **1 per `MutationOwner`** (`owner.rs:99,175`) | shared by every role/lane | whole save | owner drop |
| Decompression workspace | **1 MiB**, one aligned `Vec<u64>` (`codec.rs:35,388`) | **1 per owner** (`owner.rs:100,176`) | shared by resolution, pool reads, selection | whole save | owner drop |
| `Candidates` admitted-FULL cache | **≤ 128 KiB, compile-time asserted** (`delta/candidates.rs:15-44`) | 1 per owner | with selection | whole save | owner drop |
| `DepthCache` | `DEPTH_CACHE_ENTRIES = 4_096`, cleared **whole** on overflow (`delta/select.rs:27,143-149`); walk path ≤ `MAXIMUM_DELTA_MAX_DEPTH`+1 (`select.rs:99-111`) | 1 per owner | with chain acquisition | whole save | owner drop |
| **`pack_cache`** | **NO COUNT OR BYTE BOUND — see F2** (`owner.rs:111`, `delta/read.rs:216-229`) | 1 per owner | with everything | **whole save**, not one wave | owner drop |
| **`PoolReader.packs`** | **NO COUNT OR BYTE BOUND — see F2** (`pool/read.rs:32,129-137`); owner-held copy at `owner.rs:119` | 1 per owner + 1 per `Resolver::resolve_charged` for pooled leaves (`delta/read.rs:109`) | as above | whole save | owner drop |
| `PoolReader.groups` | `VALUE_CACHE_BYTES = 512 KiB`, cleared whole (`pool/read.rs:27,94-100`) | 1 per `PoolReader` | with pack cache | owner / resolve | clear or drop |
| Pool index | 131,072 entries, cleared whole (`pool/index.rs:80-83,131-134,236-239`); entry charge 24 B (`index.rs:151-158`); `METADATA_INDEX_BYTES` **unenforced** — see F6 | `Arc<Mutex<..>>`, shared by every `Store` clone (`store.rs:101`, `#[derive(Clone)]` at `store.rs:91`) | with every save of that Store | Store lifetime | `invalidate()` on failure (`owner.rs:877-879,886-888`) |
| Base acquisition | full canonical base `Vec<u8>` ≤ `CANONICAL_LIMIT` 16 MiB (`delta/select.rs:243,333-343`; `policy.rs:53`) | 1 live across the prefix trial | **with the FULL record and the PREFIX record** | one `select` | `select` return |
| FULL alternative | ≤ `CANONICAL_LIMIT` + framing (`encoding/full.rs:104-128,148-161`) | 1 | **held while the PREFIX trial runs** (`select.rs:195` … `269-270`) | one `select` | `select` return |
| PREFIX alternative | ≤ profile frame limit (`full.rs:88-95,148-153`) | 1 | with FULL + base + `compact` (`full.rs:183-192`) | one `select` | `select` return |
| Pending group (per lane) | `GROUP_TARGET`; sealed at `GROUP_TARGET` (`owner.rs:348-357`) | **5 lanes** (`owner.rs:96`, `PackLane::ALL`) | with the incoming record | until seal | `seal_group` `pending.clear()` (`owner.rs:754`) |
| Open pack tail (per lane) | retained framed groups; `≤ lane.pack_limit()` (`placement.rs:61-66`; `layout.rs:108-113`) | **5 lanes** | with pending group + assembled write | until save end | owner drop |
| Assembled write | `Vec::with_capacity(assembled_length)` ≤ `lane.pack_limit()` (`assemble.rs:179-187`) | one per `SelectedWrite`; `select_many` may return several | **with `open.groups` and with `open.groups.clone()` — see F3** | one `write_pack` | `write_pack` return |
| `LanePlacement.select_many` candidate clone | whole open tail + incoming group (`placement.rs:84-86`) | 1 per input group | as above | one loop iteration | iteration end |
| Pooled value groups | `VALUES_PER_GROUP` 165, `METADATA_GROUP_LIMIT` 16 KiB (`policy.rs:83,85`); `built` **plus** a clone of every group body (`owner.rs:575-590`) | ≤ `POOLED_LEAF_ROWS_LIMIT`/165 → 1 (`policy.rs:87`) | with assembled pack | one leaf | `write_value_groups` return |
| Pooled leaf body | ≤ `INODE_LEAF_LIMIT` 8 KiB (`owner.rs:441,483`; `policy.rs:114`) | 1 + base body ≤ 8 KiB (`owner.rs:488-501`) | with the delta program | one leaf | `select_pooled` return |
| `pending_values` | `BTreeMap<[u8;73],u32>` (`owner.rs:121`) — **bounded only indirectly** by the transaction row limit | 1 per owner | with `fresh` | whole save | owner drop |
| SQLite page cache | **library default, never set — see F7** | **N = live connections, unbounded** | with every Rust-side owner | connection | connection drop |
| SQLite MEMORY journal | **no byte bound; unbounded on the cleanup path — see F4** | 1 per open transaction | with the write | COMMIT/ROLLBACK | txn end |
| Telemetry `Recording` | `MAX_NODES = 1_024`, `MAX_DEPTH = 32`, `MAX_LABEL_BYTES = 128` (`telemetry/src/timer/recording.rs:14,17,20`) | 1 per measured operation | with the operation | `complete(self)` | returned by value |

### 1.2 Demonstrated bound gaps (source-provable)

**F1 — `ExtentBuilder` levels ≥ 1 grow with file length. HIGH.**
`flush_streaming` (`mapping/build.rs:187-199`) loops on a **fixed** `level` parameter and only ever pushes into `level + 1` (`build.rs:197`); it is called from exactly one place, `self.flush_streaming(consumer, 0)` (`build.rs:165`). No code path ever flushes level 1 during streaming — `finish_levels` (`build.rs:201-232`) only runs at the end, after the memory is already held.
Consequence: level-1 pending entries ≈ `(chunks − 193) / 128`, each a `NodeSummary` of 56 B derived (`types.rs:238-248`), i.e. ≈ 0.44 B per emitted chunk, with **no declared cap**. This directly contradicts the module doc at `build.rs:60-62`: *"It holds only boundary pages: a level is flushed as soon as it can no longer change, so the retained entry count is bounded by the height and the page capacity rather than by the file length or the number of edits."* It also fails the design ledger row (`content-io-memory-audit.md:125`) "Bound by declared fanout/height".
Instrumentation exists and is unasserted: `peak_pending()` / `pending_entries()` (`build.rs:103-110`) — `grep -rn 'peak_pending\|pending_entries' crates/*/tests/ crates/*/examples/` returns **0 hits**.

**F2 — `pack_cache` and `PoolReader.packs` are operation-lifetime with no eviction and no byte cap. HIGH.**
`owner.rs:111` declares `pack_cache: BTreeMap<i64, Vec<u8>>`, initialised at `owner.rs:211`, used at `owner.rs:310` and `owner.rs:393`. `pack_of` (`delta/read.rs:216-229`) inserts one **whole pack body** per distinct `pack_id` and never removes an entry. Nothing clears it — not `maybe_commit` (`owner.rs:773-790`), not `seal_group` (`owner.rs:700-756`), not `finish_inner` (`owner.rs:816-853`).
Per-entry bound is the lane cap (`layout.rs:108-113`): `PACK_LIMIT = 262_144` (`policy.rs:47`) or **`SINGLETON_PACK_LIMIT = 16 MiB + 4_096`** (`policy.rs:78-80`) — and `lookup::pack_bytes` (`sqlite/lookup.rs:152-158`) reads the entire `object_packs.data` BLOB regardless of lane. The entry **count** is bounded only by the number of distinct packs holding a delta base during one save; no declared limit exists.
The doc comment at `delta/select.rs:162-163` calls this *"Pack bodies already read inside this wave"*, which understates the lifetime: the map is owned by `MutationOwner` for the whole save.
The same pattern is duplicated at `pool/read.rs:32` (`packs: BTreeMap<i64, Vec<u8>`) with `fn pack` at `pool/read.rs:129-137`; `MutationOwner` holds one for the whole save (`owner.rs:119`) and `Resolver::resolve_charged` builds a fresh one per pooled-leaf read (`delta/read.rs:109`).

**F3 — placement clones the whole open pack for every incoming group. HIGH (transient), HIGH (CPU).**
`placement.rs:84-86`:
```rust
let mut candidate = open.groups.clone();
candidate.push(group.clone());
assembled_length(lane, &candidate).is_ok_and(|length| length <= PACK_LIMIT)
```
`EncodedGroup` owns `bytes: Vec<u8>` (`layout.rs:151-154`), so this deep-copies every group body in the open pack **plus** the incoming group, per input group. Bounded at ≤ 256 KiB for `Ordinary`/`Native`/`WholeFile`/`PooledMetadata` (`PACK_LIMIT`, `GROUP_COUNT_LIMIT = 256` at `policy.rs:49`), but the `Singleton` lane's limit is 16 MiB + 4,096 B (`layout.rs:110`) → a transient of the same order as the pack itself, while the pack body is already resident. It is also O(n²) in `assembled_length` calls.
Same shape at assembly: `assemble.rs:187` allocates the full pack `Vec::with_capacity(length)` while `open.groups` still holds every constituent group body → ≥ 2× the pack at the copy. Same shape again at `owner.rs:589-590`, which clones every pooled group body into `encoded` while `built` retains the originals.
The design ledger's requirement (`content-io-memory-audit.md:141`) "Placement first, assemble once. Count source plus destination while copying" is met only in the sense that the copies are counted nowhere.

**F4 — one unbounded transaction under a MEMORY journal (cleanup). HIGH.**
`sqlite/cleanup.rs:30-75`: a single `write::begin_immediate` (line 31), a loop deleting 128 rows per iteration (`CLEANUP_PAGE_ROWS = 128`, `policy.rs:65`; loop at `cleanup.rs:33-69`), then `DELETE FROM object_packs WHERE pack_id > ?1` (lines 70-73), then one `write::commit` (line 75). The only budget is `if report.pages > 1_000_000` (`cleanup.rs:66-68`) — up to **128,000,000 row deletions in one transaction**.
With `journal_mode = MEMORY` (`connection.rs:33`) SQLite keeps a pre-image of every modified page in RAM until COMMIT. The transaction row/byte limits exist (`TRANSACTION_ROW_LIMIT = 8_191`, `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 MiB − 1`, `policy.rs:61-63`) but are enforced at exactly one site, `owner.rs:774-777`, which this path does not call. This owner appears in no row of the design's allocation ledger.

**F5 — `retained_start` materialises the entire value-group catalogue. MEDIUM.**
On a cold start with `end > 1 + METADATA_INDEX_VALUES` (`pool/index.rs:116-123`) the index calls `retained_start` (`index.rs:224-246`), whose first statement is `pool::catalogue(connection, None)` (`index.rs:225`). `pool::catalogue` (`sqlite/pool.rs:123-151`) collects **every** `metadata_value_groups` row into a `Vec<ValueGroupRow>`. That `Vec` is bounded only by the number of value groups in the Store. The declaration being protected (`METADATA_INDEX_VALUES`) bounds the retained **set**, not this read. The comment at `index.rs:221-223` ("Only the catalogue is read, never the payload") is accurate about payload, but the catalogue itself is unbounded.

**F6 — `METADATA_INDEX_BYTES` is dead; the declared 32 MiB is not the real bound. MEDIUM.**
`policy.rs:118` declares `pub const METADATA_INDEX_BYTES: usize = 32 * 1024 * 1024;`. `grep -rn 'METADATA_INDEX_BYTES' crates/` returns **exactly one hit — the declaration**. Nothing enforces it. The real enforcement is the entry count: 131,072 (`index.rs:80-83,131-134,236-239`) with a charge of `size_of::<(i64,u32)>() + 8` = 24 B per entry (`index.rs:151-158`), so the derived live-byte ceiling is 131,072 × 24 = **3,145,728 B ≈ 3.0 MiB** — the declared constant over-states it by ~10× and is unreachable. The design ledger row (`content-io-memory-audit.md:146`) also carries the reference's "32-MiB SQLite file ceiling and requested 4-MiB cache", neither of which has a counterpart here.

**F7 — no `cache_size` and no `mmap_size` is set; the ledger declares 32 MiB + mmap0. MEDIUM.**
`connection::configure` (`connection.rs:29-45`) is the complete profile: `journal_mode = MEMORY` (33), `synchronous = OFF; temp_store = MEMORY` (37), `foreign_keys = ON` (38), `busy_timeout(0)` (43). `grep -rn 'cache_size\|mmap_size' core/crates/` returns **0 hits**.
`content-io-memory-audit.md:144` declares *"requested 32-MiB cache/connection, MEMORY journal/temp, mmap0"*. The replacement sets no cache size at all, so the effective page cache is the library default per connection; the design's own row is stale for this tree.

> **Explicit answer to the required question.** **No — `cache_size` is not presented as a total RSS cap anywhere in `core/`.** `grep` over the whole core tree returns 0 hits for `cache_size`, 0 for `mmap_size` and 0 for "memory cap"; the only two `RSS` hits are explicit disclaimers (`crates/layerfs-storage/README.md:129` — *"Evidence for memory is declared-limits and live-ownership accounting, not RSS"*; `tests/memory_bounds.rs:5-6` — *"OS RSS are deliberately not claimed"*). The requirement is therefore satisfied — **vacuously**, because no `cache_size` is configured and no such claim exists. The real gap is the mirror image: the per-connection page cache, the RAM journal and the `temp_store = MEMORY` b-trees are charged to no budget at all, and the connection multiplicity `N` is unbounded (`Store` is `#[derive(Clone, Debug)]` at `store.rs:91`; no semaphore/pool/size limit exists anywhere in `core/`).

**F8 — transaction limits are enforced after the write (soft bound). MEDIUM.**
`maybe_commit` (`owner.rs:773-790`) tests `>=` only *after* `seal_group` has added rows and canonical bytes (`owner.rs:751-752`, called at `owner.rs:755`) and `write_pack` has added the assembled pack length (`owner.rs:767-768`). One transaction can therefore overshoot by up to one group (`RECORD_COUNT_LIMIT = 8_191` rows, `policy.rs:51`) plus one pack (`≤ SINGLETON_PACK_LIMIT = 16,781,312 B`, `policy.rs:80`). The declared bounds are upper bounds on the *steady state*, not on the peak.

**F9 — the threshold probe buffer is retained for the whole scan. MEDIUM.**
`construct_stream` reserves the cutoff (128 KiB default, ≤ 1 MiB, `content.rs:195-202`) and then keeps it alive by moving it into `Cursor::new(prefix).chain(source)` (`content.rs:209-214`) for the entire chunked scan, coexisting with the scanner's 32 KiB stack input and 32 KiB chunk `Vec` (`gear.rs:81,128`). The doc at `content.rs:183-184` ("at most `cutoff` bytes are buffered before the scanner takes over") reads as if the buffer were released. The bound itself is honest and declared; the overlap is what the ledger should charge.

**F10 — `plan_lane` keeps the losing compact record alive while building the winner. LOW.**
`encoding/full.rs:183-192`: `compact` is built, then when it does not fit, `record::encode(PackLane::Singleton, …)` allocates a second full record while `compact` is still in scope (it is dropped only at function exit). `content-io-memory-audit.md:140` asks to "release losing frame promptly"; here the loser is not released before the winner is built. Bounded by `profile.frame_limit()` + framing, so a small absolute overlap on the oversized-singleton path only.

**F11 — `charged` in `EditObjects::hold_node` is a conservative (never under-) estimate. INFO — bound holds.**
`tree.rs:105-139` adds `canonical.len() + 128` to `self.charged` on **every** call, including calls where `self.deferred` already holds that id and no new allocation happens (`tree.rs:123-129`). `deferred` entries are never removed (there is no `remove` call anywhere in the file), so `charged` ≥ live bytes always. The direction is safe for a bound, but the consequence is that an operation holding the same node repeatedly can be rejected with `BoundedCapacityExceeded { what: "edit.deferred_nodes" }` while its real live footprint is well under 8 MiB.

**F12 — two unchecked arithmetic sites whose safety rests on `StorageCapacities` being unforgeable, but every field of it is public. LOW (hardening).**
(a) `codec.rs:71` — `raw_limit: capacities.whole_file_canonical_limit - OBJECT_ENVELOPE_OVERHEAD`, a plain subtraction in a `const fn` with no `checked_sub`. The two constants it couples are declared in two different crates (`codec.rs:98` `OBJECT_ENVELOPE_OVERHEAD = 23`; `content/policy.rs:183` `OBJECT_ENVELOPE_BYTES = 23`) with **no compile-time equality assertion**.
(b) `codec.rs:429` and `codec.rs:487` — `header.windowSize > (1_u64 << profile.window_log())` shifts by a policy-supplied **`i32`** (`codec.rs:55,73`; sourced from `storage/policy.rs:243`). A negative or ≥ 64 value panics in debug and masks in release, and this shift happens **before** the corresponding FFI call, so no C-side guard covers it.
Both are unreachable through `StorageCapacities::from_policy` (`policy.rs:282-312`): `whole_file_canonical_limit = raw + OBJECT_ENVELOPE_BYTES` (`content/policy.rs:135,144`), and a validated policy yields `window_log ∈ {18, 20}` (`content/policy.rs:30-32,156-162`, cutoff range enforced at `content/policy.rs:84-105`). **But `StorageCapacities` carries no `#[non_exhaustive]` (`grep -n 'non_exhaustive' storage/policy.rs` → 0 hits, verified) and every field is `pub` (`policy.rs:233-277`), so any crate may build one by struct literal and reach both sites.** The derived invariant `raw_limit ≤ 1 << window_log` holds for both accepted cutoffs but is asserted nowhere in `codec.rs`. No memory-unsafety follows (buffers are still sized from real slice lengths), but the invariant is "asserted by construction" only.

**F13 — a real heap allocation escapes the codec's "every allocation is charged" claim. LOW (doc/budget accuracy).**
`codec.rs:6-8` and `codec.rs:12-13` state that the static region means *"a codec call cannot grow an allocator-backed context and every allocation is charged before the call."* That is false on the prefix-decode path: `ZSTD_DCtx_refPrefix` (`codec.rs:503`) reaches `dctx->ddictLocal = ZSTD_createDDict_advanced(…)` at `zstd/lib/decompress/zstd_decompress.c:1707`, which performs `ZSTD_customMalloc(sizeof(ZSTD_DDict), customMem)` at `zstd/lib/decompress/zstd_ddict.c:152` — a genuine heap allocation outside the charged 1 MiB/2 MiB regions, once per prefix decode, freed by the clear at `codec.rs:521` or by the next reset. It is small and bounded to one live `DDict` per workspace, so this is not a resource gap; it is an inaccurate claim in the module doc, and it is also a failure-identity asymmetry (an allocation failure there returns `memory_allocation`, which the decode paths route through `checked` → `codec_failure()` rather than `resource()`).

### 1.3 Multiple files / edits / Stores, blocked output, failure paths

- **Multiple files, one operation.** `construct_chunked` (`content.rs:224-249`) creates a **fresh** `ExtentBuilder` per file, so F1's level-1 retention is released between files. The cross-file owners are the C2 ones: `pack_cache` (F2) and the five lane tails accumulate for the whole save.
- **Multiple edits, one operation.** `replace_chunked` (`edit/apply.rs:206-304`) keeps **one** `EditObjects` across every edit (`apply.rs:217`), so `deferred`/the 8 MiB charge is per-operation, not per-edit — correct. Each edit creates its own `ExtentBuilder` (`apply.rs:256`) → F1 applies per replacement. Edit count is bounded at 4,096 (`edit/input.rs:22,100-104`).
- **Multiple Stores.** `Store` is `Clone` and holds no connection (`store.rs:91-102`); `begin_save` opens a **new** connection each call (`store.rs:184`). `Store::clone` shares the `Arc<Mutex<PoolIndex>>`, so the 131,072-entry index is shared, correctly. There is **no bound on how many Stores or SaveOperations may be live**; `BEGIN IMMEDIATE` with `busy_timeout = 0` (`connection.rs:43`) makes a second *successful* writer impossible, but a failed attempt still opens and configures a connection. Total SQLite page cache = N × default; `N` is undeclared.
- **Blocked output.** `owner.rs:330-332` returns `StorageError::Aborted` once `terminal`. A rejecting consumer propagates through `consumer.accept` (`build.rs:364`, `tree.rs:146,200,643`) as a `ContentResult` — the whole operation unwinds by `?`, so all operation-owned buffers drop together. `edit_bounds.rs:320-361` exercises exactly this (`OutputRejected` returned once, `consumer.accepted == 1`) — a real, non-vacuous oracle. No producer/consumer back-pressure queue exists in core (the design's four-slot channel at `content-io-memory-audit.md:128` is a *reference* owner, not present here).
- **Failure paths.** One terminal boundary (`cas/finish.rs:12-25`): unknown outcome → `quarantine()` with **no ROLLBACK issued** (`owner.rs:883-889`); definite failure → `mark_terminal()` + exactly one `abandon()` guarded by `cleanup_attempted` (`owner.rs:856-868`). All owner state is freed by Rust drop (`store.rs:472-483`). The MEMORY journal is released at COMMIT/ROLLBACK or connection close. **Not verified:** whether SQLite rolls back the quarantined transaction at connection close in this configuration — documented SQLite behaviour, but no product code or test asserts it.

### 1.4 What is genuinely bounded (state it, because it is real)

- `Candidates` is the strongest proof in the tree: a `const _: () = { assert!(…) }` block (`delta/candidates.rs:35-44`) statically establishes that the slot array, reference table and struct fit inside `INDEX_BYTES = 128 * 1024`.
- Both codec workspaces are **single fixed allocations** (2 MiB + 1 MiB, `codec.rs:33,35`) built with `try_reserve_exact` + `resize` behind a fallible path (`codec.rs:123-143`), and their static contexts live *inside* that region, so the codec cannot grow an allocator-backed context.
- Chain work is explicitly budgeted and iterative: depth ≤ role depth, canonical ≤ 512 KiB, encoded ≤ 256 KiB (`delta/read.rs:127-147,180-203`; `policy.rs:98-100`). No recursion over chain length.
- Pack parsing validates the whole bounded directory before any body is touched, with `checked_add`/`get(..)` everywhere and explicit continuity (`layout.rs:224-275,303-367,369-426,429-466`).
- `PoolReader`'s decoded-value cache is hard-capped at 512 KiB with a wholesale clear (`pool/read.rs:27,94-100`).
- Telemetry is bounded on every construction path and retains nothing after the operation: `MAX_NODES`/`MAX_DEPTH`/`MAX_LABEL_BYTES` (`recording.rs:14,17,20`) enforced at `recording.rs:203-206` and `recording.rs:239-241` and `report.rs:107`; over-limit **clips and flags**, never panics or errors (`recording.rs:144-152`, `scope.rs:153-156`); zero `static`/`thread_local!`/`OnceLock` in the crate; JSON and text serialisers stream straight into the caller's writer with only a 2-bytes-per-level indent (bounded by `MAX_DEPTH`), no intermediate `String`/`Vec`.

---

## Part 2 — Memory safety

### 2.1 Unsafe inventory (complete)

```
grep -rn 'unsafe' core/crates --include='*.rs'
```
- `layerfs-content/src/lib.rs:16` — `#![forbid(unsafe_code)]`
- `layerfs-telemetry/src/lib.rs:13` — `#![forbid(unsafe_code)]`
- `layerfs-storage/src/lib.rs:19` — `#![deny(unsafe_op_in_unsafe_fn)]`
- **The only `unsafe` in `core/` is `layerfs-storage/src/encoding/codec.rs`** — 12 occurrences (lines 109, 117, 161, 191, 265, 330, 390, 425, 483, 541, 580, 588), 12 `// SAFETY` comments, and `unsafe impl` count = **0** (no hand-written `Send`/`Sync`).

The raw-pointer fields `CompressionWorkspace::context` (`codec.rs:152`) and `DecompressionWorkspace::context` (`codec.rs:382`) make both types automatically `!Send`/`!Sync`, so neither can migrate between threads while holding a pointer into its own `memory`. Both types implement `Drop` that only nulls the pointer (`codec.rs:371-377,571-575`) — correct, because a static context must never be passed to a C free.

FFI surface: `zstd-sys = "=2.0.16"` (zstd 1.5.7) with `features = ["experimental"]` (`core/crates/layerfs-storage/Cargo.toml`), used **directly** rather than through the safe `zstd` crate wrapper — which is what makes `ZSTD_initStaticCCtx`, `ZSTD_FrameHeader`, `ZSTD_getFrameHeader` and the reset/refPrefix calls reachable at all. No third-party patch, fork or vendoring is present (repository rule honoured).

### 2.2 Block-by-block: asserted invariant, is it checked, arithmetic risk, error path

**A. `codec.rs:107-121` — `checked` / `encode_checked` (`ZSTD_isError`, `ZSTD_getErrorCode`).**
Invariant asserted by the SAFETY comment: "this API only interprets the numeric return value". It is trivially true — a `usize` by value, no pointer. Sound.
One substantive point: `encode_checked` (`codec.rs:115-121`) maps only `ZSTD_error_memory_allocation` to `resource()` and defers everything else to `checked` → `codec_failure()`. Both are errors, so no failure is silently absorbed. No fallback decoder exists anywhere (`decode.rs:7-9` documents this deliberately).

**B. `codec.rs:159-170` / `codec.rs:389-399` — `ZSTD_initStaticCCtx` / `ZSTD_initStaticDCtx`.**
Invariant: the region is live, eight-byte aligned, owned by `memory`, and never reallocated. **Checked in three ways:** `memory` is a `Vec<u64>` allocated by `workspace()` (`codec.rs:123-133`), so alignment 8 holds; the size passed is `workspace_bytes(&memory)` = `size_of_val` (`codec.rs:145-147`), i.e. exactly the allocation; the returned pointer is null-checked (`codec.rs:167`, `396`). `memory` is never mutated after construction — only read — so the pointer cannot be invalidated by a reallocation. Arithmetic risk: none; `size.div_ceil(8)` at `codec.rs:129` is the only arithmetic and it is exact for the two constants in use (2,097,152 → 262,144 words; 1,048,576 → 131,072 words).
Lifetime risk: the pointer lives in the same struct as the buffer it points into, and `Vec` moves only move the pointer-to-heap, not the heap. Safe.

**C. `codec.rs:191-232` — `compress`.**
Invariants: (1) input nonempty and ≤ `profile.raw_limit()`; (2) the context is large enough for the parameter set that will be configured; (3) the frame bound fits the profile; (4) the reported length does not exceed the allocation.
**Checked:** (1) at `codec.rs:180-186`; (3) at `codec.rs:211-212` (`bound > profile.frame_limit()` → error); (4) at `codec.rs:223-225`; the output is allocated to exactly `bound` (`codec.rs:215`). Both pointers passed are a live `&[u8]` and a fresh `Vec`, so they cannot overlap.
**(2) is the one I could not close.** The size check uses
```rust
let estimate = checked(ZSTD_estimateCCtxSize_usingCParams(ZSTD_getCParams(3, raw.len() as u64, raw.len())))?;
```
(`codec.rs:206-210`), i.e. the estimate for the **default** `cParams` of that source size — but the context is then reconfigured with `ZSTD_c_windowLog = profile.window_log()` (`codec.rs:198`), which for `CodecProfile::native()` is a fixed **20** (`codec.rs:64`) irrespective of `raw.len()` (≤ 32,768). It is therefore not evident from the Rust side that `estimate` upper-bounds the configured parameter set.
**Verdict: RESOLVED as fail-closed, verified against the pinned C source (not merely assumed).** zstd refuses to resize a static context and returns a detectable error rather than writing past it:
```
zstd/lib/compress/zstd_compress.c:2168:
    RETURN_ERROR_IF(zc->staticSize, memory_allocation, "static cctx : no resize");
```
under `/Users/yifanxu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zstd-sys-2.0.16+zstd.1.5.7/` (read-only grep at review time; the pin is `zstd-sys = "=2.0.16"`, zstd 1.5.7). `memory_allocation` from `ZSTD_compress2` is mapped by `encode_checked` (`codec.rs:115-121`, used at `codec.rs:216` and `codec.rs:296`) to `resource()` (`codec.rs:99-101`) — an error, never an out-of-bounds write. The residual issue is therefore a **capacity-policy** one, not a memory-safety one: an undersized-but-accepted configuration surfaces as `Integrity("bounded Zstandard workspace unavailable")` at the codec call instead of being rejected before the work starts. No supported cutoff was found to trigger it, but I did not exercise the accepted range 128 KiB…1 MiB end to end (`content/policy.rs:16-18`).
Note the asymmetry: `compress_group` (`codec.rs:330-367`) performs **no** estimate check at all, and is defensible only because it derives its parameters from `ZSTD_getCParams(GROUP_LEVEL, raw.len(), 0)` itself (`codec.rs:331`) with `windowLog` clamped to 16 (`codec.rs:332`).

**D. `codec.rs:265-316` — `compress_prefix` (borrowed prefix lifetime).**
Invariant: the caller's `prefix` bytes must remain live and unmodified for exactly the duration of one `ZSTD_compress2` call, and must not be observable by any later call.
**Checked / enforced:** the borrow is a `&[u8]` parameter, so Rust's borrow checker keeps `prefix` alive across the call — the obligation is genuine and satisfied by construction, not by convention. `prefix` is bounds-checked (nonempty, ≤ `raw_limit`, `codec.rs:254-260`).
**Reset/error path — this is the part that is right and worth crediting:** the fallible body is wrapped in an immediately-invoked closure (`codec.rs:289-308`) whose error value is only propagated at `codec.rs:315`. Therefore `ZSTD_CCtx_refPrefix(context, ptr::null(), 0)` (`codec.rs:310`) and `ZSTD_CCtx_reset(…, ZSTD_reset_session_and_parameters)` (`codec.rs:311-314`) execute on the success path **and** on every failure path inside the closure. **One refinement I owe after re-reading the control flow:** if line 310's `?` returns an error, line 311 is **skipped**, so the reset does not run on that path — my first reading was too generous. The residual window is bounded because every public method resets first (`codec.rs:192-195, 266-269, 333-336, 435-438, 493-496, 551-554`), all with `ZSTD_reset_session_and_parameters`, which clears any dictionary/prefix before a header is even parsed; and the decode-side `ZSTD_DCtx_refPrefix` installs a **by-reference** `DDict` whose `dictContent` points into the caller's buffer, cleared by `ZSTD_clearDict` on the next reset. I found no path that dereferences the stale reference before that reset, so **I do not claim a use-after-free**; the defect is that the safety-critical clear is not attempted on a path that can still fail, while the module doc at `codec.rs:11-13` states the reference "is cleared on success and on failure" — true only when the clear itself succeeds. Error paths: any `?` before `codec.rs:289` returns without a prefix ever having been set — no cleanup needed and none attempted. No use-after-failure is reachable.

**E. `codec.rs:330-367` — `compress_group`.** Bounds checked at `codec.rs:325-327` and `347-349`; length cross-checked at `codec.rs:358-360`; `ZSTD_c_windowLog` clamped to `GROUP_WINDOW_LOG_MAX = 16` (`codec.rs:43,332`). Sound; note the absent estimate check (above) and that `ZSTD_CCtx_setFParams` is used here rather than the six `setParameter` calls used by the payload paths — a different parameter surface with the same `Vec<u64>` workspace.

**F. `codec.rs:425-456` — `decompress` (the length-mismatch case).**
This is the correct pattern and the answer to the decompression-length question. Before the FFI call:
- bounds: frame nonempty, ≤ `frame_limit`, `raw_length != 0`, ≤ `raw_limit` (`codec.rs:414-420`);
- header parsed and validated (`codec.rs:426-434`): Zstandard magic, reserved descriptor bits zero (`codec.rs:581-582`), `frameType == ZSTD_frame`, **`frameContentSize == raw_length`**, `windowSize <= 1 << window_log`, `dictID == 0`, `checksumFlag == 1`;
- destination allocated to **exactly the caller-declared `raw_length`** (`codec.rs:444`) — never to a frame-derived size;
- `ZSTD_d_windowLogMax` set (`codec.rs:439-443`);
  **correction to my first reading, verified against the C source:** that parameter is *not* the enforcement point. zstd reads `maxWindowSize` only in its **streaming** decoder (`zstd/lib/decompress/zstd_decompress.c:2231`); the single-pass path used here (`ZSTD_decompressDCtx` → `ZSTD_decompressMultiFrame` → `ZSTD_decompressFrame`, same file `:1197-1199, 953-1066`) never consults it. What actually bounds the window is **the crate's own header test** at `codec.rs:429` (and `codec.rs:487` for the prefix path, `codec.rs:545` for groups), reading `header.windowSize` from the struct parsed at `codec.rs:426/484/542`. The defence is real; it just lives in Rust, not in the FFI parameter;
- the returned length is compared against `raw_length` (`codec.rs:445-454`) and a mismatch is an `Integrity` error.
So a frame that *declares* a larger decompressed size than the locator's `canonical_length` is rejected at the header comparison **before** any decoding; a frame that *produces* more than `dstCapacity` makes `ZSTD_decompressDCtx` fail with `dstSize_tooSmall`, which `checked` turns into an error. Both the declared and the actual side are defended. On failure the partially-filled `raw` `Vec` is dropped. **No offset or length arithmetic is performed on frame-derived values anywhere in this function.**

**G. `codec.rs:483-527` — `decompress_prefix`.** Same header/bounds/length discipline as F, plus the prefix borrow. `ZSTD_DCtx_refPrefix(NULL, 0)` (`codec.rs:521`) and the reset (`codec.rs:522-525`) run unconditionally for the same closure reason as D (`codec.rs:502-520`). `ZSTD_d_windowLogMax` is set (`codec.rs:497-501`).

**H. `codec.rs:541-567` — `decompress_group`.** Same checks (`codec.rs:542-550`), with `header.windowSize > GROUP_LIMIT as u64` rather than a `d_windowLogMax` parameter. Note the asymmetry: this is the only decompressor that does **not** call `ZSTD_DCtx_setParameter`; the window is bounded by the explicit comparison instead. Defensible, but a reader should not assume the three decompressors are parameter-identical.

**I. `codec.rs:580-607` — `parse_frame_header` (the most delicate invariant).**
It is declared `unsafe fn` with `/// SAFETY: the caller guarantees `frame` is a live slice; this only reads it.` That obligation is **vacuously satisfiable by any `&[u8]`** — the marker conveys no real precondition, and all three call sites (`codec.rs:426,484,542`) sit in `unsafe` blocks they do not need for this reason.
The real risk is the `MaybeUninit`/`assume_init` pair:
```rust
let mut header = std::mem::MaybeUninit::<zstd_sys::ZSTD_FrameHeader>::uninit();
if checked(ZSTD_getFrameHeader(header.as_mut_ptr(), frame.as_ptr().cast::<c_void>(), frame.len()))? != 0 {
    return Err(StorageError::Integrity("Zstandard frame header"));
}
…
Ok(header.assume_init())
```
- The FFI call is checked for length first (`codec.rs:581`: `frame.get(..4)`) and magic second (`codec.rs:582`), so a skippable frame (whose magic differs) can never reach the call — which matters, because a skippable frame is the known case where some zstd builds populate only a subset of the struct.
- `assume_init()` (`codec.rs:605`) runs **only** on the exact-`0` return (`codec.rs:594-597`), the "complete header" return; a "need more input" positive return is rejected. So the partial-initialisation case is excluded.
- **Resolved, verified against the pinned C source (read-only, by me):** zstd zeroes the whole struct before filling it. `ZSTD_memset(zfhPtr, 0, sizeof(*zfhPtr))` appears at `zstd/lib/decompress/zstd_decompress.c:479` on the main path — before any `return 0` — and again at `:486` on the skippable path. Every byte `assume_init()` reads is therefore initialized, and the crate reads only six of the nine fields (`frameType`, `frameContentSize`, `windowSize`, `dictID`, `checksumFlag`; `codec.rs:427-431, 485-489, 543-547`), never the reserved ones.
- The struct layout is safe: `zstd-sys 2.0.16` is bindgen output over the C header, so the Rust struct matches `ZSTD_frameHeader`.
- **Verdict: SOUND under the pin, resting on an upstream `memset` rather than on a Rust-side guarantee.** A differential or fuzz test is still the right regression guard for it (see §2.4), but this is no longer an open question.
- Offset arithmetic: `frame.as_ptr()`/`frame.len()` only; `ZSTD_findFrameCompressedSize` is compared to `frame.len()` (`codec.rs:598-604`) so trailing bytes are rejected.

**J. Malformed record / bad pack offsets (outside the codec, safe Rust).** The parsers that consume untrusted stored bytes are all bounds-checked with `get(..)`, `checked_add`/`checked_sub` and explicit monotonicity: `parse_header` (`layout.rs:224-275`), `ordinary_group_view` (`layout.rs:303-367`, including `body_start != offset` continuity and `end > bytes.len()`), `whole_file_group_view` (`layout.rs:369-426`), `record_range` (`layout.rs:429-466`, `end <= previous || end > area_length` rejected and `previous != area_length` rejected), `framed_record` (`decode.rs:176-192`), `group_records` (`decode.rs:159-173`, with `count.min(RECORD_COUNT_LIMIT)` on the `with_capacity` — the correct guard against an attacker-declared count causing an abort). `decode_canonical` cross-checks the rebuilt canonical length against the locator on every lane (`decode.rs:60,81,101,126`). I found no unchecked offset arithmetic on untrusted input in the real path.

### 2.3 Public-input tests that exercise these paths

The task asks which tests use *real malformed inputs*; a green test NAME is not proof, and the storage/content suites must be read as samples, not sweeps.

**Non-vacuous (exact outcome pinned, real bytes):**
- `crates/layerfs-storage/tests/physical_formats.rs:69` — `declared_pack_and_group_bounds_are_enforced_on_the_bytes`; pins exact strings `Integrity("pack length")`, `Integrity("singleton pack group count")`, `Integrity("pack group count")`. Strongest pack-header oracle.
- `crates/layerfs-storage/tests/metadata_pool.rs:207` — `a_corrupt_value_group_is_rejected_once`; catalogue digest replaced by 32 × 0xAB via SQL, asserts `Integrity(what) if what.contains("value group")`.
- `crates/layerfs-storage/tests/cas_reuse.rs:203` — `a_stored_row_with_a_short_length_is_rejected_without_reading_it`; `canonical_length - 1` in SQL, pins `Collision(_)`.
- `crates/layerfs-storage/tests/cas_roundtrip.rs:110` — malformed `application_id`/`user_version`, pins `UnsupportedPolicy { field: "schema identity" }`.
- `crates/layerfs-storage/tests/metadata_fingerprint_collision.rs:97` — a **real, searched** BLAKE3-truncation fingerprint collision driven through the real Store with byte-exact readback; the strongest adversarial-input test in the tree.
- `crates/layerfs-content/tests/object_identity.rs:175` — `malformed_envelopes_are_rejected_exactly`; eight mutations of a real envelope with exact `ContentError` variants pinned.
- `crates/layerfs-storage/tests/visibility.rs:173` / `:220` — injected pack row and advanced watermark, exact struct fields / variant compared.
- `crates/layerfs-content/tests/edit_bounds.rs:320-361` — rejecting consumer, `OutputRejected` returned exactly once (the only blocked-output oracle).
- `crates/layerfs-content/tests/streaming.rs:88-99` — late source failure after early output, exact `ContentError::Io` plus child-before-parent ordering.

**Weak / union / bare oracles (they can pass for reasons other than the property named):**
- `crates/layerfs-storage/tests/delta_chains.rs:239-251` — accepts `StorageError::Integrity(_) | StorageError::Engine(_)`; `Engine` is a catch-all around *any* rusqlite error, so this can pass for reasons unrelated to the corrupt frame. Weakest corruption oracle in the tree.
- `crates/layerfs-storage/tests/delta_payload.rs:237-242` — bare `outcome.is_err()` / `read.is_err()`.
- `crates/layerfs-storage/tests/pack_locator.rs:202` (`Integrity(_) | Content(_)`), `:240` (blanket `Integrity(_)`); `cas_reuse.rs:181` (`Collision(_) | Integrity(_)`); `physical_formats.rs:44` (magic and truncated-header cases accept any `Integrity(_)`).
- `crates/layerfs-content/tests/inode_leaf.rs:120` `malformed_leaves_are_rejected` — of eight cases only three feed bytes to a decoder, and all eight oracles are bare `is_err()`; five are in-memory **encoder**-side refusals.
- `crates/layerfs-content/tests/inode_leaf.rs:165-173` — the `POOLED_PREFIX_BYTES - 1` case is an all-zero buffer one byte short: it can only be rejected for being short and exercises no partially-valid structure.
- `#[should_panic]` occurrences in `core/`: **0** (`grep -rn 'should_panic' crates/ | wc -l` → 0). No test in the tree encodes "must reject rather than panic".

**Coverage holes — verified by my own greps, not taken on trust:**
- `grep -rn 'DecompressionWorkspace\|decompress\|compress_prefix\|parse_frame_header\|ZSTD\|zstd' crates/*/tests/` → **0 hits**. Nothing in the tree ever feeds a byte string to the zstd decoder or to the prefix codec. The only codec test is encode-side (`physical_formats.rs:113`, an oversized-but-well-formed input).
- 0 hits each in `crates/*/tests/` for `group_view`, `framed_record`, `record_range`, `decode_canonical`, `encode_prefix`, `parse_native`, `record::parse`. The per-group directory rules and the in-group record directory rules are therefore untested against hostile bytes; only the header/group-count-vs-total-length rule is covered (`physical_formats.rs:61-65, 89-93`).
- No test constructs a zstd frame whose declared decompressed size exceeds its content; no test sweeps truncation lengths (every truncation in the tree is one fixed length); no test decodes a PREFIX record against a borrowed prefix that does not match — the nearest cases stop before the codec (`delta_chains.rs:166` substitutes an id with no catalogue row → `ObjectMissing` at `delta/read.rs:138-139`; `delta_chains.rs:201` is refused by the chronology check at `delta/read.rs:143-145`).
- No test drives `pool::group_for`'s range branch (`sqlite/pool.rs:105-111`), `pool/read.rs:269-272`, or `value_group.rs:81` with an inconsistent catalogue row; the only catalogue-damage test issues `DELETE FROM metadata_value_groups` (`metadata_pool.rs:244-246`), removing all rows.
- `decode_node` / `decode_node_with_context` / `decode_file_state` are called at every test site with real canonical bytes and `.expect(…)`; no malformed mapping-node or file-state test exists, despite that code performing offset arithmetic over extent entries.

**Also worth recording as a positive:** `decode.rs:150-155` does **not** compare the record's embedded base identity against the locator's `base_object_id` — the base bytes come from the locator chain (`delta/read.rs:151-168`) and `parsed.base`'s value is discarded. The only backstops for a mismatched prefix are the zstd content checksum and the whole-object identity check at `delta/read.rs:164-166`. The pooled reader **does** make this comparison (`pool/read.rs:200-206`), which shows the check was understood — it is simply absent on the payload path, and untested in both places.

### 2.4 Missing instrumentation (stated as a coverage limit)

- `cargo miri`: **not run**, and no miri configuration exists — `grep -rn 'miri\|sanitiz\|fuzz\|valgrind\|AddressSanitizer'` over `core/` returns **0 hits**; there is no `.cargo/config.toml` with miri flags, no `fuzz/` target, no `#[cfg(miri)]`. Miri would in any case be the wrong primary tool here: it cannot execute the zstd C calls, so it can only validate the Rust-side pointer/borrow discipline, not the FFI contracts.
- Sanitizers (ASan/UBSan) and `cargo-fuzz`: **not run**, no harness exists. The correct targets are `parse_frame_header` + `decode_canonical` + `group_view` + `record_range` + `decode_node_with_context` fed raw attacker bytes, plus ASan over the existing corrupt-pack tests (`pack_locator.rs:202`, `cas_reuse.rs:181`, `delta_chains.rs:239`, `delta_payload.rs:223`, `metadata_pool.rs:207`).
- No allocator instrumentation: `grep -rn 'global_allocator\|AllocCounter'` over `crates/` → **0 hits**. No test in the tree observes an allocation count or peak while decoding hostile bytes. `memory_bounds.rs` bounds only the accept/encode path, and says so (`memory_bounds.rs:1-6`).
- **Therefore: "safe Rust plus low RSS" does not establish dependency safety here.** `layerfs-content` and `layerfs-telemetry` are `forbid(unsafe_code)` and genuinely have no unsafe; `layerfs-storage` calls raw `zstd-sys` FFI in 12 places with **no sanitizer, no fuzzer and no differential test behind them**, and the zstd magic literal `0x28, 0xb5, 0x2f, 0xfd` occurs exactly once in the whole tree — in the implementation at `codec.rs:581`, never in a test.
- **Provenance of the C-source claims in §2.2:** the two upstream questions I had originally left open were settled by reading the *pinned vendored C source* at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zstd-sys-2.0.16+zstd.1.5.7/zstd/lib/` — a read-only grep, no build, consistent with the `zstd-sys = "=2.0.16"` pin. The two results are the static-context refusal (`compress/zstd_compress.c:2168`, → §2.2 C) and the header `memset` (`decompress/zstd_decompress.c:479,486`, → §2.2 I). Reading a pinned dependency's C source is **not** the same evidence class as executing a test: it establishes what the pinned version does, and nothing about a future version or about a code path the compiler chooses differently. It does not substitute for the fuzz/ASan harnesses above.

---

## Part 3 — Measured vs declared

### 3.1 Measured observation

- **C1 localized-edit latency and object/byte counts, matched reference pair** — retained at `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-matched-c1-20260917T050000Z/ledger.md`. Declared sample: reference 224,875 ns / candidate 186,000 ns; identical edited root `b6dca354…c65207b`; 5 vs 7 objects and 13,826 vs 47,357 bytes in the window. The ledger lists **every** observation, not the best one, and concludes the ordering reverses between runs so latency is unqualified. One sample per arm.
- **Pool index occupancy** — real product accessors `Store::pool_index_entries` / `pool_index_bytes` (`store.rs:169-177`). Measured in-test at `tests/metadata_window.rs:161-166` (200 retained entries after the crossing group, asserted `< METADATA_INDEX_VALUES`), `tests/metadata_window.rs:109-113` (`live_bytes() == size_of::<(i64,u32)>() + 8` for one entry), and `tests/metadata_pool_index.rs:266-272` (`entries >= 1_000`; `bytes <= 131_072 * 16 + 1_024`). These are in-test assertions at the reviewed commit, not retained receipts.
- **Pending batch and retained tail** — `tests/memory_bounds.rs:105-140` measures `operation.pending()` and `retained_tail_bytes()` against `limits.batch_objects`, `limits.batch_bytes` and `3 * limits.pack_limit` on every accepted object; `tests/core_pipeline.rs:210` repeats the tail bound. Real, non-vacuous.
- **Boundary probes** — `tests/metadata_window.rs:129-174` writes 1,312 value groups and asserts the exact crossing behaviour (`rows[1_310] == (131_001, 100)`, 200 retained entries, evicted values rewritten). This is a genuine window-boundary measurement.

### 3.2 Source-derived bound

Not measured — read off the code, with citations in §1.1: codec workspaces 2 MiB + 1 MiB (`codec.rs:33,35`); `GROUP_LIMIT` 65,536, `PACK_LIMIT` 262,144, `GROUP_COUNT_LIMIT` 256, `RECORD_COUNT_LIMIT` 8,191, `CANONICAL_LIMIT` 16 MiB (`policy.rs:45-53`); lane pack limits `layout.rs:108-113`; `Candidates` ≤ 128 KiB **compile-time asserted** (`candidates.rs:35-44`); `DepthCache` 4,096 entries (`select.rs:27,143-149`); chain budgets 512 KiB / 256 KiB (`policy.rs:98-100`); `VALUE_CACHE_BYTES` 512 KiB (`pool/read.rs:27`); `EDIT_DEFERRED_LIMIT` 8 MiB − 1 (`tree.rs:31,116`); builder level 0 ≤ 193 entries (`build.rs:73-75`); telemetry 1,024 / 32 / 128 (`recording.rs:14,17,20`); pool index 131,072 entries → derived ≤ 3,145,728 B (`index.rs:80,131,236` + `index.rs:151-158`).
**Unbounded by any declaration:** builder levels ≥ 1 (F1); `pack_cache` and `PoolReader.packs` entry count (F2); `pool::catalogue` (F5); `N` live connections (F7); the MEMORY journal on the cleanup path (F4).

### 3.3 Unproven expectation (asserted in documentation, not established by the code)

- `content-io-memory-audit.md:144` — "requested 32-MiB cache/connection, MEMORY journal/temp, mmap0". No `cache_size`/`mmap_size` pragma exists (F7).
- `content-io-memory-audit.md:146` — the 32-MiB index ceiling. `METADATA_INDEX_BYTES` is dead code (F6); the real derived ceiling is ~3.0 MiB.
- `mapping/build.rs:60-62` — "the retained entry count is bounded by the height and the page capacity rather than by the file length". False for level ≥ 1 (F1).
- `delta/select.rs:162-163` — "Pack bodies already read inside this wave". The cache is operation-lifetime (F2).
- `file/content.rs:183-184` — "at most `cutoff` bytes are buffered before the scanner takes over". The buffer is retained for the whole scan (F9).
- `content-io-memory-audit.md:140` — "release losing frame promptly". Not done in `plan_lane` (F10).
- `content-io-memory-audit.md:117` — "These are reference limits, not a measured total RSS bound"; `content-io-memory-audit.md:215` — "No absolute total C1/C2 RSS cap is established by this source review." **Both still hold after this review and should not be upgraded.** The design ledger's own honesty about this is correct.

### 3.4 Vacuous oracles carrying bound-related names (demonstrated, not missing evidence)

- `crates/layerfs-content/tests/streaming.rs:72-85` `many_files_do_not_retain_the_whole_workload`: `peak` is computed at line 80 and **discarded** — `let _ = peak;` at line 84. The test asserts `root_seen.len() == 24` and `consumer.objects() > 24`; nothing about retention.
- `crates/layerfs-content/tests/streaming.rs:30-38` `input_requests_stay_bounded_by_the_declared_chunk_window`: a `CountingSource` is constructed at line 33, but line 35 passes `bytes.as_slice()` (never the source), and line 37 is `let _ = source;`. The counter is never read. The only assertion (line 36) is a logical-length equality.
- `crates/layerfs-content/tests/edit_bounds.rs:241-273` `the_retained_frontier_does_not_grow_with_the_file`: the measured quantity is `support::extent_count(&result, root)` (line 266), which counts **extents in the result file** (`tests/support/mod.rs:419-439`), not retained frontier entries. The assertions are `peaks[1] > peaks[0]` (line 268) — i.e. it asserts the metric *does* grow with file size, the opposite of the name — and `peaks[0] <= 192 && peaks[1] <= 192 * 32` (line 272) on that same file-size metric. Additionally the edit is `Edit::delete(0, 0)` (line 249): with `len == 0` the comparator `continue`s (`file/edit/compare.rs:49-51`) and returns `Equal` (line 84), so `apply_edits` returns the base root at `file/edit/apply.rs:70-73` and the split/concat/builder path — the thing the test is named for — **does not run**. (The early-return inference is source-derived; I did not execute the test.)
- `EDIT_DEFERRED_LIMIT` has **zero** references outside `src/` (`grep -rn 'EDIT_DEFERRED_LIMIT' crates/` → 5 hits, all in `src/`): the one hard C1 edit-memory bound and its `edit.deferred_nodes` failure (`tree.rs:116-122`) are never exercised.
- `peak_pending` / `pending_entries` have **zero** references under `tests/` or `examples/`: F1's own metric is unasserted.

### 3.5 Not available — recorded as `null` with the reason

| Metric | Value | Reason |
| --- | --- | --- |
| Measured RSS / peak heap / allocation count for C1, C2 or telemetry | `null` | No allocator instrumentation exists in `core/` (`grep -rn 'global_allocator\|AllocCounter' crates/` → 0 hits). The matched-C1 ledger states it directly: "**Memory: not measured.** Neither arm instruments allocation or resident memory … #168's 'simultaneous index/codec/SQL memory' gate is therefore **unmeasured**, not passed." |
| Bytes held by the SQLite MEMORY journal for a representative transaction | `null` | Would require opening/writing a Store and measuring; this review must not run builds and is read-only. Only an analytic description is possible from `connection.rs:33` and the write pattern (whole-BLOB `UPDATE`, `sqlite/write.rs:82-91`). |
| `peak_pending` / `pending_entries` for any real file | `null` | The accessors exist (`build.rs:103-110`) but no test, receipt or example reports them. |
| Linked library's exact `SQLITE_DEFAULT_CACHE_SIZE`, page size, default `mmap_size` | `null` | `libsqlite3-sys` is not `bundled` (`grep -rn 'bundled' core/` → 0 hits), so these come from the system build. A read-only `python3 sqlite3` probe of the *system* library (sqlite 3.51.2) returned `cache_size = -2000`, `page_size = 4096`, `mmap_size = NULL` — evidence about the system library, **not** about the linked one, and not a product measurement. |
| Any measurement for a run with multiple simultaneous Stores, or with producer output blocked mid-save | `null` | No such case exists in the retained evidence; the reference four-slot channel and the generic candidate store of the design ledger are not present in `core/`. |
| Whether SQLite rolls back the quarantined transaction at connection close | `null` | Documented SQLite behaviour, but no product code or test asserts it; `owner.rs:883-889` issues no ROLLBACK. |
| `layerfs-telemetry` per-report overhead figure | `null` | The crate's own README defers overhead qualification; no figure exists at this commit. |

---

## What I could not verify

1. **Whether the fixed 2 MiB static `ZSTD_CCtx` workspace is large enough for the parameter set actually configured** — *narrowed, not eliminated.* I established from the pinned C source that an undersized static context is a **detected error**, not an out-of-bounds write (`zstd/compress/zstd_compress.c:2168`, → §2.2 C), so the memory-safety question is closed. What remains open is whether any **accepted** policy (cutoff 128 KiB…1 MiB, `content/policy.rs:16-18`) can produce that error at runtime — i.e. whether a supported configuration is silently unusable. That needs the end-to-end accepted-range campaign, which I did not run.
2. *Closed:* the `assume_init` soundness question (`codec.rs:605`) — settled by `zstd/decompress/zstd_decompress.c:479,486` (→ §2.2 I).
3. **Whether `ZSTD_CCtx_refPrefix(ctx, NULL, 0)` / `ZSTD_DCtx_refPrefix(ctx, NULL, 0)` is a valid clear on this zstd version** (`codec.rs:310`, `codec.rs:521`). I did **not** confirm this myself; the subagent that proposed the "NULL invalidates" reading cited `zstd/decompress/zstd_decompress.c:1699-1713`, which I did not independently open. The code `?`-propagates the return value, so a failure surfaces as an error — but on that path the following reset is skipped, which is exactly the residual window in §2.2 D.
4. **Any runtime behaviour at all.** No test, example, benchmark, miri run, sanitizer run or fuzz run was executed by me; F1's "level ≥ 1 is never flushed during streaming" and the `edit_bounds.rs` early-return inference in §3.4 are read off control flow, not observed.
5. **The exact live-byte figure for the three unbounded owners** (F1, F2, F5) — they are unbounded in the input, so only a formula is derivable, and I give the formula rather than a number.
6. **The reference tree's actual behaviour for the same paths** — `crates/` was read only to check the design ledger's citations, never to establish a candidate bound; no reference-oracle execution was performed.
7. **Absolute per-connection SQLite page cache of the *linked* library** — see §3.5.
8. **Whether the level-1 retention in F1 is reachable under the committed measurement cases** — the largest retained case is 3,300,000 bytes (`matched-c1/ledger.md`), roughly 200 chunks, which is below the 193-entry streaming threshold plus a few flushes. F1 is a scalability bound gap, not an observed failure on the committed fixture; do not restate it as a measured regression.
