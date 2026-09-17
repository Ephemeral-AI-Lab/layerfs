## 7. Limits — supported-envelope audit

Confidence key: **enforced** · **configurable** · **derived/theoretical** · **verified**
(executed here or in a retained PASS receipt) · **environment-dependent** · **not yet
qualified** · **outside Stage 3–4 ownership**.

| Dimension | Owner/scope | Unit | Default | Accepted configurable range | Enforced hard limit | Format/theoretical ceiling + derivation | Largest verified case | First limiting mechanism | At / over-limit behavior | Source |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| File revisions retained | none (concept absent) | — | — | — | **none; no owner** | 2²⁵⁶ distinct roots (32-byte BLAKE3 `ObjectId`) | no count measured | SQLite database size (no product cap) | n/a | `object/id.rs:13`; grep `revision` = 0 hits in both `src/` trees |
| History / branch revision service | none | — | — | — | absent | — | — | — | — | **outside Stage 3–4**; `content/src/lib.rs:7`, `storage/src/lib.rs:7` |
| Retire / GC a published root | C2 | — | — | — | no API exists | — | — | — | — | `sqlite/cleanup.rs:32–99` removes only a failed save's packs |
| WHOLE_FILE vs chunked cutoff | C1 (persisted by C2) | bytes | 131 072 (exclusive) | powers of two 131 072 … 1 048 576 (four values) | min 131 072 / max 1 048 576 | u64 length field | 128 KiB, 256 KiB, 512 KiB and 1 MiB all exercised | policy validation | non-power-of-two or out of range ⇒ `UnsupportedPolicy` before work | `content/policy.rs:81–102,120–128`; `sql/schema.sql:18–19` |
| Whole-file depth | C1+C2 | chain links | 8 | 0 … 50 | 50 | u8 (255) narrowed to 50 | 0 and 50 accepted; changed caps tested | fixed chain byte budgets | >50 rejected; candidate at or over the cap ⇒ FULL | `content/policy.rs:20,24,91–100` |
| Chunk depth | C1+C2 | chain links | 4 | 0 … 50 | 50 | u8 narrowed to 50 | a 50-link chunk chain admitted and read | `CHAIN_ENCODED_LIMIT` 256 KiB | as above; reader ⇒ `Integrity("dependency chain depth")` | `content/policy.rs:22`; `delta/read.rs:147–149` |
| Pooled-metadata depth | C2 | chain links | 8 | 0 … 50 | 50 | u8 narrowed to 50 | depth ≤ 50 accepted; the boundary is a depth decision, not a work refusal | `METADATA_CHAIN_CANONICAL_LIMIT` 65 536 | as above | `storage/policy.rs:124,204–208,329–335` |
| Empty routing | C1 | bytes | len == 0 ⇒ Empty | n/a | — | — | 0-byte file stored and read | — | defined empty representation | `content/policy.rs:120–122` |
| Whole-file canonical object | C1/C2 | bytes | ≤ 131 094 | raw = cutoff − 1; canonical = raw + 23 | cutoff − 1 ≤ 1 048 575 raw | 16 MiB envelope | 1 048 575-byte raw stored and read back | cutoff | `BoundedCapacityExceeded` | `content/policy.rs:132–141,183` |
| Chunk payload / canonical | C1 | bytes | 8 192 / 16 384 / 32 768 | **frozen, not configurable** | 32 768 raw; 32 789 canonical | u32 field | exercised by every chunked case | CDC maximum | `ObjectLimitExceeded` | `file/cdc/gear.rs:13,15,17` |
| Canonical object (any role) | C1/C2 | bytes | — | not configurable | 16 777 216 | 9 + 4 + 8 MiB value | 16.78 MiB singleton lane bounded; 1 MiB incompressible singleton verified | `MAX_CANONICAL_OBJECT_BYTES` and the SQL CHECK | `ObjectLimitExceeded` / `CapacityExceeded` | `content/policy.rs:190`; `storage/policy.rs:53` |
| Single value field | C1 | bytes | — | not configurable | 8 388 608 | u32 `value_len` | — | `MAX_OBJECT_FIELD_BYTES` | `ObjectLimitExceeded` | `content/policy.rs:196` |
| Mapping entries per page | C1 | entries | 128 | frozen | 128 (root may be short); 64 minimum non-root | u16 count field | 63/64/127/128/129 occupied | `MAX_MAPPING_ENTRIES` | `NonCanonicalPagePartition` | `file/mapping/types.rs:13,15` |
| Tree height | C1 | levels | ≤7 for a 2⁶⁴ file | frozen | 31 | 128^h children | 192/193 flush and height growth | `MAX_LEVEL` | `MappingDepthExceeded` | `file/mapping/types.rs:17` |
| Logical file length | C1 | bytes | — | frozen | u64 | **2⁶⁴ − 1 = 18 446 744 073 709 551 615 (16 EiB − 1)** | 2 097 151 bytes (largest physical test) | u64 field | `LengthOverflow` | `file/mapping/types.rs:181–185` |
| Extent offset / length | C1 | bytes | — | frozen | u32 each | u32 | — | chunk payload 32 768 | **not checked at decode**; read-time `InvalidRecord("extent outside payload")` (F-15) | `mapping/types.rs:36–38`; `mapping/read.rs:120–122` |
| Whole-file edit assembly | C1 | bytes | result < cutoff | — | ≤ 1 048 575 | cutoff | all transition cases | cutoff routes to streaming | `BoundedCapacityExceeded edit.assembly` | `file/edit/apply.rs:137–145` |
| Pending batch | C2 | objects / bytes | 512 / 524 288 | fixed | flush at 512 objects or 512 KiB | usize / u64 | the 512-object path is exercised | batch bound | wave flushed, **not rejected**; one oversized object admitted into an empty batch (escape untested at size) | `storage/policy.rs:57–59`; `cas/batch.rs:45–69` |
| Objects per save / per Store | C2 | objects | — | — | **none** | i64 pack ids; 2²⁵⁶ object ids | 60 objects / 6 packs (footprint row) | SQLite database size | resource-limited, explicitly not unlimited | `sql/schema.sql:34–37,49–63` |
| Transaction | C2 | rows / bytes | 8 191 / 4 194 303 | fixed | same | u64 | exercised by the 4 096-edit stream | transaction bound | COMMIT then BEGIN IMMEDIATE and continue; **limits checked after the write, so one group plus one pack may overshoot** (F-13) | `cas/owner.rs:794–811` |
| Lookup / locator page | C2 | IDs | 128 | fixed | 128 (129 bind parameters) | — | exercised | `LOOKUP_PAGE_IDS` | extra pages issued | `storage/policy.rs:55` |
| Pack size by lane | C2 | bytes | 262 144 ordinary/native/whole-file/pooled | fixed | 262 144; singleton 16 781 312 | u32 offsets | 259 777 largest observed pack; 1 MiB incompressible singleton | lane `pack_limit` | `CapacityExceeded` | `storage/policy.rs:47,81–83` |
| Group body | C2 | bytes | 65 536 ordinary+native; 16 384 pooled | fixed | as default | u32 | exercised | `GROUP_LIMIT` / `METADATA_GROUP_LIMIT` | `CapacityExceeded` | `storage/policy.rs:45,88` |
| Pack ID | C2 | id | 1 | monotonic | i64::MAX = 9 223 372 036 854 775 807 | i64; SQL CHECK > 0 | — | `checked_add` | `Integrity("pack identifier overflow")`; new pack refused | `pack/placement.rs:90–92`; `cas/owner.rs:173–175` |
| Group / record ordinals | C2 | ordinal | — | — | group 0…255; record 0…8 190 | 8 / 13 bits | exercised | SQL CHECK + `RECORD_COUNT_LIMIT` | `Integrity` | `sql/schema.sql:43,62` |
| **Pooled ordinals (total persisted values)** | C2 | ordinal | 1 | monotonic | **4 294 967 295** | SQL CHECK `first_ordinal ≤ 2³²−1`, `first+count ≤ 2³²` | 2 400 values (largest fixture) | schema CHECK | at the exact end the guard is defeated by a u32 truncation and fails as a *different* error (F-14) | `sqlite/pool.rs:41–52`; `sql/schema.sql:40,45` |
| Values per group | C2 | values | 165 | fixed | 165 | `4 + 99n ≤ 16 384 ⇒ n ≤ 165` | boundaries exercised | 16 KiB group body | `Integrity("value group count")` / SQL CHECK | `storage/policy.rs:86`; `pool/value_group.rs:35–37` |
| Pooled index window | C2 | entries | 131 072 | fixed | 131 072 (no separate byte ceiling) | usize; modelled 3.00 MiB live | exactly 131 072 filled in-process and reset wholesale | `METADATA_INDEX_VALUES` | whole window cleared (a duplicate physical value, never a lost one) | `storage/policy.rs:135–141`; `pool/index.rs:80–83` |
| Edits per operation | C1 | edits | — | fixed | **4 096** | usize | a real 4 096-edit stream | `MAXIMUM_EDITS_PER_OPERATION` | `BoundedCapacityExceeded edit.stream` before any work | `file/edit/input.rs:22,100–106` |
| Edit shape | C1 | — | — | — | `start ≤ end ≤ current length`; no overlap; no reorder | u64 coordinates | exercised, including overlap rejection | `EditStream::new` | `InvalidEdit` | `file/edit/input.rs:110–133` |
| Edit frontier (unfinished nodes) | C1 | bytes | — | fixed | **8 388 607** | usize | peak ≤64 KiB (16 edits), <512 KiB (4 096 edits) — the *refusal* itself is UNRUN | `EDIT_DEFERRED_LIMIT` | `BoundedCapacityExceeded edit.deferred_nodes` | `file/edit/tree.rs:31,320–335` |
| Simultaneous writers per Store | C2 | writers | 1 | — | 1 | — | — | `BEGIN IMMEDIATE`, `busy_timeout = 0` | `OwnershipUnavailable`, no retry | `sqlite/write.rs:45–49`; `sqlite/connection.rs:43` |
| Concurrent readers | C2 | readers | any | — | no busy handler | — | — | `busy_timeout = 0` | `Engine(DatabaseBusy/Locked)` — engine semantics | same; **environment-dependent / unqualified** |
| Delta trials | C2 | trials | — | fixed | exactly one per object | — | `trials 1` observed | `delta/select.rs` | losing trial stores FULL | measured, this review |
| SQLite maximum BLOB | C2 (provider) | bytes | ≤ 262 144 typical | — | product cap 16 781 312 (singleton lane) | system `SQLITE_MAX_LENGTH` (stock 10⁹) | 1 048 583-byte fixture in one 1 204 224-byte DB | product pack limit | — | **environment-dependent, unverified** |
| SQLite maximum database file | C2 (provider) | bytes | none | — | **none enforced** | `SQLITE_MAX_PAGE_COUNT × page_size`; host-dependent | 1 204 224-byte DB (footprint row) | host disk | — | **not yet qualified**; `libsqlite3-sys` links the system library (no `bundled`) |
| Files per workspace | **no owner** | — | — | — | **unimplemented** | — | not measured | — | — | **outside Stage 3–4**; Stages 5/7 |
| Directory name / path / nesting / entries | **no owner** | — | — | — | **unimplemented** | — | not measured | — | — | **outside Stage 3–4**; the 73-byte inode value has no name field (`object/inode_leaf.rs:70–79`) |
| Workspace size / quota | **no owner** | — | — | — | **unimplemented**; no aggregate API; `quota` = 0 hits | — | not measured | — | — | **outside Stage 3–4**; Stages 5/7 |

**Boundary failure modes actually observed (traced against the single-attempt
contract).** Validation rejects before mutation for policy (`policy_capacity` 9/9), schema
identity, edit shape and stream size. Ineligible advisory delta candidates are ordinary
policy and select FULL — verified by three `delta_payload` cases that FAIL with
`ObjectMissing` without the fix. Corruption and codec/SQL failures fail the operation with
no FULL recovery and no retry — verified by `physical_formats`, `pack_locator`,
`cas_reuse`, `persistence_failure` and `visibility`. The 8 MiB−1 frontier refusal is
proved out of reach (a 408 MiB base floor) and **declared UNRUN**; the batch's own comment
says so. Nothing was clamped, retried, re-budgeted or algorithm-swapped to reach a limit
test.

### 7.1 Direct answers

* **File revisions.** There is **no revision-count cap in the reviewed C1/C2 code, because
  the concept has no owner**: grep for `revision` over both `src/` trees returns 0 hits,
  and `history` appears only in module docs that disclaim it. A revision is just another
  content-addressed root; C2 keys everything by 32-byte identity with no per-path or
  per-generation counter. Retained immutable roots are therefore unbounded in number, each
  up to 2⁶⁴−1 logical bytes, bounded only by the SQLite database file, which the product
  does not limit. **No successful-version rollback is introduced**: there is no API to
  retire a published root, and the only deletion path removes a *failed, unpublished*
  save's packs. A history or branch revision service is outside Stage 3–4 ownership. The
  batch's own phrase "file revisions" is informal — `EDIT_DEFERRED_LIMIT` bounds unfinished
  *nodes*, not retained roots.
* **File size.** Three regimes. `len == 0` ⇒ the defined empty representation;
  `0 < len < cutoff` ⇒ one whole-file object (maximum raw `cutoff − 1`, i.e. 131 071 by
  default and 1 048 575 at the largest accepted cutoff); `len ≥ cutoff` ⇒ a chunked extent
  tree. The tree can address ≈2²³⁹ bytes, but the **binding ceiling is the u64
  logical-length field: 2⁶⁴ − 1 = 18 446 744 073 709 551 615 bytes (16 EiB − 1)**; one
  byte more ⇒ `LengthOverflow`. The largest canonical object any role can actually produce
  is 1 048 598 bytes (whole-file at the 1 MiB cutoff), far below the 16 MiB envelope
  ceiling. **T = 128 KiB or 1 MiB is a routing threshold, not a maximum file size.** The
  **largest physically tested file is 2 097 151 bytes** (the `small-to-large` pipeline
  example run by this review), and the largest payload-sized fixture anywhere is ~8 MiB of
  extents in `edit_localized`. `construct_bytes` takes a `&[u8]`, so the caller must
  already hold the whole file; there is no streaming *input* API for a multi-terabyte file,
  and none was tested.
* **Files per workspace.** **Not established.** There is no Workspace, filesystem or mount
  type in either reviewed crate; a Store is a path plus a policy and holds objects, not
  files. Stage 3–4 storage of independent roots does not implement workspace membership or
  a count. The underlying *verified* scale is 60 objects / 6 packs in the footprint row,
  and the 512-object / 512 KiB batch and 128-ID lookup pages are streaming and paging
  bounds, not a cap on how many files may exist. **The workspace cap is deferred to Stages
  5/7.**
* **Directory length, path length, nesting depth, entries per directory.** **Not
  established by Stages 3–4.** All four are unimplemented: there is no name field, no path
  field, no entries-per-directory count and no nesting-depth limit, and the 73-byte inode
  value carries only kind, namespace ref-count and two roots — there is literally nowhere
  to put a name. A pooling value, inode leaf or mapping-tree height does **not** establish
  any of these. **Stage 5/7 review required**; no host `PATH_MAX`/`NAME_MAX` was imported
  as a core contract.
* **Workspace size.** **The core cannot yet promise a workspace maximum.** No aggregate
  API and no quota exists (`quota` = 0 hits). The sum of current logical file lengths and
  the retained-history logical bytes are undefined or unbounded; unique canonical bytes are
  bounded only by the 2²⁵⁶ identity space and in practice by the database; physical Store
  bytes are bounded per pack (262 144 ordinary, 16 781 312 singleton) but not in total.
  Deduplication makes logical size differ greatly from disk use, which is exactly why the
  two are reported separately. The only bounded "space" concepts are fixed in-flight
  working buffers, which bound *memory*, never *stored size*.
* **Store / database.** Object ids are 32-byte BLOBs; pack ids are i64 with a checked `+1`
  and a SQL CHECK; a pack BLOB is capped by the product at 16 781 312 bytes (typically
  262 144); row cardinalities are bounded per batch (512) and per transaction (8 191 rows /
  4 MiB−1). **SQLite's own limits are not determined by this repository**:
  `rusqlite = "=0.40.2"` is used without the `bundled` feature and `libsqlite3-sys 0.38.2`
  lists only `pkg-config`/`vcpkg`, so the **system libsqlite3** is linked; only a build-time
  floor of ≥ 3.34.1 is derivable, and no `sqlite3_limit` call exists in the product. **The
  maximum database file size and maximum single BLOB are therefore environment-dependent
  and unqualified.** A database file maximum is in any case not a maximum single file or
  workspace.
* **Pool / index.** The **131 072-entry window is a cache-eviction bound, not a cap on
  stored values**: eviction rewrites a whole group and never deletes a stored value, so a
  workspace may hold far more. The real persisted ceiling is the **ordinal space,
  4 294 967 295 distinct pooled values per Store** (SQL CHECK `first+count ≤ 2³²`), and the
  guard at that boundary is currently defeated by a u32 truncation (F-14). Values per group
  are 165, fixed by the 16 KiB group body. The window's live cost is modelled at 24 B/entry
  ⇒ 3.00 MiB at the cap, **not measured**.
* **Edits / concurrency.** At most **4 096** edits per operation, each a non-overlapping
  current-result-coordinate range with `start ≤ end` and no reordering; the unfinished
  frontier is charged against 8 388 607 bytes and fails closed. Exactly **one** writer per
  Store (`BEGIN IMMEDIATE`, zero busy timeout, no retry); readers are unbounded in number,
  and contending readers get an engine error rather than waiting — engine semantics this
  repository does not pin. "Simultaneous readers" and "number of Stores or connections" are
  **not** bounded by any constant.

---

