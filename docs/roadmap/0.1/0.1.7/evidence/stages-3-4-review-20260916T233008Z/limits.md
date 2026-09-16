# Stages 3–4 mandatory supported-envelope / limits audit (#168 / #169)

**Status:** independent reviewer output. Static derivation + retained-evidence audit.
No product source, issue, commit or file outside this evidence directory was changed.

**Pinned reviewed snapshot:** `91c3a0741fff64e8161d5c1b6e759f347ffbf757`, branch `main`.
`git status --porcelain` reports only the untracked evidence directory
`docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/`; no tracked file is
modified. Reviewed components: C1 `core/crates/layerfs-content`, C2 `core/crates/layerfs-storage`
(+ `sql/schema.sql`). `core/crates/layerfs-telemetry` has no limit dimension.

**What I did NOT run.** Per the assignment I ran no `cargo` command of any kind — no build, test,
clippy, fmt or example. Every boundary below is derived from source arithmetic, from the external
test oracles I read, from the retained receipts, or from the parent's frozen check log
(`checks.log` in this directory, which records the focused C1/C2 suites and the workspace suite at
this snapshot). Boundaries I could not execute are marked `not-yet-qualified` and listed in
§"Boundaries not run".

**Method note.** "A green test NAME is not proof": for every case used as evidence I opened the
body and checked what the oracle actually asserts. One oracle in this tree does **not** check the
property its name and comment claim (§"Oracle defects"). A test that asserts a *constant equals a
constant* is recorded as a constant check, not as an exercised boundary.

**Tags:** enforced = code rejects/limits; configurable = caller- or policy-settable inside a checked
range; derived-theoretical = arithmetic from format fields, not reachable; verified = actually
executed at or across the stated value, with the artifact named; environment-dependent = decided by
the host/engine, not by this repository; not-yet-qualified = no execution at that boundary;
outside-Stage-3-4-ownership = owning component is a later stage.

Short source prefixes used in the table: `C1` = `core/crates/layerfs-content/src/`, `C1t` =
`core/crates/layerfs-content/tests/`, `C2` = `core/crates/layerfs-storage/src/`, `C2t` =
`core/crates/layerfs-storage/tests/`, `SQL` = `core/crates/layerfs-storage/sql/schema.sql`.

---

## 1. Complete supported-envelope table

Each dimension is split into separate rows wherever the unit, owner or enforcement differs. A row's
"first limiting mechanism" is the constraint that binds **for that row's own unit**; no row takes a
minimum across unrelated byte/count/depth units.

| dimension | owner/scope | unit | default | accepted configurable range | enforced hard/resource limit | format/theoretical ceiling + derivation | largest actually verified case + evidence | first limiting mechanism | at/over-limit behavior | source locations (file:line) | confidence/gap |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **1. File revisions** — retained immutable file roots, per Store | C2 Store (no root registry) | count of distinct stored roots | none | none | none found | 2^256 candidate identities (32-byte BLAKE3 id, `objects.object_id` PRIMARY KEY); no root/refcount column exists, so the real ceiling is SQLite row storage | 16 distinct file roots stored in **one** save operation, `C2t/core_pipeline.rs:189-223`; >256 objects in one save for a 5 MiB file, `C2t/pack_locator.rs:170,186-201` | pack-id ordinal exhaustion or DB size; no per-root cap exists to hit | ordinary insert; nothing rejects "too many revisions" because nothing counts them | `C2/cas/store.rs:106-247`, `C2/cas/save.rs:22-72`, `SQL:49-63` | high confidence that **no cap exists**; not-yet-qualified: no boundary test (and none possible without an endurance run) |
| **1. File revisions** — retained root + its dependency closure | C1 emits / C2 stores | objects reachable from a root | none | none | none | dependencies are implicit: references are validated to exist (`C2/cas/dependencies.rs:49-72`) but never counted or capped | old roots + all dependencies still read after later saves: `C2t/edit_pipeline.rs:59-91,151-192`, `C2t/cas_reuse.rs:33-58` | storage size | n/a (no cap) | `C1/object/output.rs`, `C2/cas/dependencies.rs:20-88` | high confidence |
| **1. File revisions** — computing a root vs retaining it | C1 | n/a | n/a | n/a | n/a | construction emits each object once (child-before-parent) and retains no whole-mapping state, so *computing* a root is O(height) memory, not O(revisions) | base + 8-step pooled chain + 10-step payload chain reconstructed and read back: `C2t/metadata_chain.rs:121-180`, `C2t/delta_chains.rs:102-112` | height and page capacity | n/a | `C1/file/mapping/build.rs:58-80,187-232` | high confidence |
| **1. File revisions** — history / branch revision service | outside scope | n/a | n/a | n/a | n/a | **no history, branch, checkpoint or revision entity exists in C1 or C2** — not a crate, not a table, not a type | n/a | n/a | n/a | `C1/lib.rs:7`, `C2/lib.rs:7`; `SQL:13-69` declares exactly four tables | outside-Stage-3-4-ownership |
| **1. File revisions** — successful-version rollback | C2 save lifecycle | n/a | n/a | n/a | no rollback of a *successful* version | cleanup authority is the baseline pack id captured under exclusive ownership; only packs with `pack_id > baseline` are deleted, after rollback | failed-save cleanup leaves the previously acknowledged file readable and adds no pack: `C2t/core_pipeline.rs:227-267`; `C2t/metadata_pool_index.rs:206-243` | ownership acquisition | definite failure → one cleanup pass; unproven outcome → quarantine (nothing deleted) | `C2/sqlite/cleanup.rs:30-76`, `C2/cas/owner.rs:856-889`, `C2/cas/finish.rs:12-24` | high confidence |
| **2. Delta depth** — WHOLE_FILE payload chain | C1 policy / C2 policy | edges (u8) | 8 | 0 .. 50 (0 disables prospective delta) | enforced at selection (base must have `depth < cap`) and at read (`chain.len() > role_depth` → error) | 50 (u8 field + `MAXIMUM_DELTA_MAX_DEPTH`); the depth field is the binding store-side bound, since a chain longer than 50 is refused as corrupt on read regardless of the field width | 16 edges built and every edge followed on read, `C2t/delta_chains.rs:102-112`; cap 2 stops at its own boundary with `trials == 0`, `ineligible_candidates == 1`, FULL stored, and the accepted 3-object chain read back with `max_depth == 2`, `C2t/delta_chains.rs:79-99` | depth cap, then the 512 KiB canonical / 256 KiB encoded chain budget (whichever binds first) | at the cap the candidate is ineligible → ordinary planned FULL in a new/append pack; over the cap on a **stored** chain → `Integrity("dependency chain depth")`; over the budget → `Integrity("dependency canonical/encoded work")` | `C1/policy.rs:20,24,84-105`; `C2/policy.rs:98-100,300-320`; `C2/encoding/delta/select.rs:313-331`; `C2/encoding/delta/read.rs:127-147,180-203` | verified at 16 and at cap 2; not-yet-qualified at 50 |
| **2. Delta depth** — CHUNK payload chain | C1 policy / C2 policy | edges (u8) | 4 | 0 .. 50 | same mechanisms as WHOLE_FILE, per-role via `delta_depth_for_role` | 50 | **not verified at its own boundary** — every test uses the default 4; only WHOLE_FILE is driven to a changed cap | depth cap / chain budget | same as WHOLE_FILE row | `C1/policy.rs:22`; `C2/policy.rs:315-320`; `C2/encoding/delta/read.rs:127` | not-yet-qualified (missing evidence) |
| **2. Delta depth** — pooled-metadata chain | C2 policy (separate field) | edges (u8) | 8 | 0 .. 50, persisted in its own column | enforced in `PoolReader::leaf_body` (`chain.len() > metadata_delta_max_depth`) and at base selection (`cost.depth >= depth_cap` → skip) | 50; separately bounded by 8 x 8,192 = 65,536 canonical and 17 x 8,193 = 139,281 encoded bytes per chain | depth 8 reached exactly, 9th leaf stored FULL with the 10th restarting the chain, `C2t/metadata_chain.rs:121-180`; largest admitted 100-row leaf records cut the chain by the **canonical budget** at 8 records before the depth cap, `C2t/metadata_chain.rs:182-231`; depth 12 and 50 persisted/accepted, `C2t/policy_capacity.rs:215-254` | canonical chain budget (8,144 B/record), then the depth cap | over budget → `pool.work_exceeded` and FULL by policy (never a read failure); over depth on read → `Integrity("pooled chain depth")` | `C2/policy.rs:104,106-108`; `C2/encoding/pool/read.rs:151-188`; `C2/cas/owner.rs:648-692` | verified |
| **2. Delta depth** — chain work budgets (role-separate, depth-independent) | C1/C2 policy | bytes | 512 KiB canonical + 256 KiB encoded (payload); 65,536 + 139,281 B (pooled); 32 MiB decoded work (pooled read) | none — fixed constants, deliberately not derived from cutoff or depth | enforced before a dependency is created and again on read | 512 KiB / 256 KiB are the binding numbers because a single chain larger than them cannot be reconstructed at all | refusal on budget exhaustion with every stored object still readable, `C2t/delta_chains.rs:115-163`; 63 refusals out of 512 pooled leaves with `full == work_exceeded + 1`, retained receipt `stages-3-4-timing-20260917T031000Z/e1c-pooled-512/stdout.log` | byte budget | refused dependency → stored FULL by policy (payload) / `work_exceeded` then FULL (pooled); on read → `Integrity` | `C2/policy.rs:95-118,303-310`; `C2/encoding/delta/read.rs:180-203`; `C2/encoding/pool/read.rs:82-87` | verified |
| **2. Delta depth** — "raising a depth does not raise a budget" | C2 | assertion | n/a | n/a | budgets are `const` and never appear in `StorageCapacities::from_policy` derivation from depth | derived-theoretical: no code path can couple them | **the shipped oracle checks the wrong axis**: `C2t/policy_capacity.rs:146-158` compares cutoff 128 KiB vs 1 MiB at *identical* depths (8/4) and never compares depth 4 vs 50; no oracle in this tree asserts depth-independence of the budgets | none in code | n/a | `C2/policy.rs:282-312`; `C2t/policy_capacity.rs:146-158` | not-yet-qualified — source-derived only |
| **3. File size** — empty / WHOLE_FILE routing threshold T | C1 policy (persisted by C2) | bytes (exclusive upper bound for WHOLE_FILE) | 131,072 | powers of two only, 131,072 .. 1,048,576 | enforced before any work; non-power-of-two, <128 KiB or >1 MiB rejected | 1 MiB is the format ceiling for this profile because the compact whole-file pack grammar and its reader are sized for it; the doc comment states a larger object "needs a capacity-aware pack grammar and a matching reader" | exact ±1 boundary (T−1, T, T+1) verified at T = 131,072 and T = 262,144, `C2t/policy_capacity.rs:161-185`; T = 1,048,576 persisted and reopened, `C2t/policy_capacity.rs:27-42`; rejected values 65,536 / 196,608 / 1,048,577, `C2t/policy_capacity.rs:54-69` | the power-of-two/1 MiB validation | `UnsupportedPolicy { field: "small_file_threshold_bytes" }` before the DB file exists | `C1/policy.rs:14-18,84-105,123-131`; `SQL:18-19`; `C2/policy.rs:166-188` | verified except the ±1 boundary **at 1 MiB** (not-yet-qualified) |
| **3. File size** — largest canonical WHOLE_FILE object | C1 encode / C2 accept | bytes | 131,094 (T−1+23) | derived from T: T−1+23 | `encode_whole_file` rejects `bytes.len() > T−1`; C2's `encoding.whole_file_raw` rejects `raw > whole_file_canonical_limit − 23`; the envelope caps the value field at 8 MiB | derived: T−1 raw + 10-byte value header + 13-byte envelope; at the max accepted T the ceiling is 1,048,575 raw / 1,048,598 canonical | 131,094 B stored and read back byte-for-byte, `C2t/memory_bounds.rs:143-181`; 1,048,575 raw = 1,048,598 canonical stored in the Singleton lane under T = 1 MiB, `C2t/policy_capacity.rs:110-143` | T-derived raw limit | `CapacityExceeded { what: "encoding.whole_file_raw" }` surfaced through the save; over-limit is a definite failure, never a silent CDC/RAW switch | `C1/file/content.rs:77-101`; `C2/encoding/full.rs:138-147`; `C2/policy.rs:67-75` | verified |
| **3. File size** — largest canonical object / value field, any role | C1 envelope | bytes | 16 MiB envelope, 8 MiB value field | none | enforced in `canonical_len` and `decode_bytes_object` | 16 MiB envelope (u32 payload length is far wider: 4,294,967,291 B, so the constant binds first); 8 MiB field binds before the envelope by construction | **only asserted as a constant equality**, `C1t/object_identity.rs:250-251`; no object of either size was encoded, stored or rejected at ±1 | the 8 MiB field then the 16 MiB envelope | `ObjectLimitExceeded { limit, actual }` | `C1/policy.rs:183-199`; `C1/object/codec.rs:20-44,81-127` | enforced but not-yet-qualified at the boundary |
| **3. File size** — CDC chunk payload | C1 CDC profile | bytes | target 16,384 | none (fixed profile) | min 8,192, max 32,768 enforced by the chunk role codec on encode and decode | 32,768 (frozen CDC grammar); canonical chunk = 21 + raw = 32,789 | 32,789-byte canonical chunk stored and read back, `C2t/memory_bounds.rs:143-181`; 32,769 rejected with `limit: 32_768`, `C2t/memory_bounds.rs:81-102` | 32,768 | `ObjectLimitExceeded { limit: 32_768 }` | `C1/file/cdc/gear.rs:13-17`; `C1/file/mapping/codec.rs:53-93`; `C2/policy.rs:71-73` | verified |
| **3. File size** — extent count / offsets per page | C1 mapping grammar | entries and bytes | 128 max, 64 min non-root | none | `validate` rejects >128 and (<64 when non-root); leaf `source_offset: u32`, `logical_length: u32` with checked `offset+len` | per-page format ceiling 128 entries; node canonical width 31 + 128x40 = 5,151 (leaf) / 31 + 128x48 = 6,175 (branch), +13 = 5,164 / 6,188, under the 8,192-byte node cap | occupancy checked at 63/64/127/128/129 and at the 192/193 streaming flush boundary, `C1t/edit_bounds.rs:130-180`; a real join of two full pages stays canonical, `C1t/edit_bounds.rs:182-212` | 128 entries/page | `NonCanonicalPagePartition` | `C1/file/mapping/types.rs:12-19,156-219`; `C1/file/mapping/codec.rs:96-115` | verified |
| **3. File size** — mapping tree height and fanout | C1 mapping grammar | levels / children | height 0 (leaf root) | none | `MAX_LEVEL = 31`; branch `level` must be 1..=31 and root branch needs >= 2 children; `emit_node` re-checks height | derived: max leaves = 128^31, max extents = 128^32 = 2^224; a tree at level h needs >= 64^h extents because every non-root page must hold >= 64 entries, so level 31 needs >= 2^186 extents | real height growth/collapse exercised at 192/193 extents and 40 vs 400 extents, `C1t/edit_bounds.rs:157-180,240-273` (height itself not asserted numerically there); multi-level interior cases in the oracle receipts | 128 entries/page | `MappingDepthExceeded` | `C1/file/mapping/types.rs:17,199-201`; `C1/file/mapping/build.rs:350-352`; `C1/file/mapping/codec.rs:262-266,314-316` | verified structurally; deep heights not-yet-qualified |
| **3. File size** — **effective chunked-file logical envelope** | C1 format + resources | bytes | n/a | none | **no enforced maximum for a chunked file** | derived-theoretical: `FileState.logical_len` is u64 and every total uses checked adds, so the format can express 2^64 − 1 = 18,446,744,073,709,551,615 B (~16 EiB). The extent-side capacity (2^224 extents x 32,768 B max readable extent = 2^239 B) does **not** bind first; the u64 logical length does. Reaching it needs ~2^49 extents, so this is a field-width ceiling, not a supported envelope | 24 MiB (25,165,824 B) with >= 700 extents and >= 8 mapping pages edited through real public C1 APIs, `C1t/edit_localized.rs:274-419,421-470,471-521`; 16 MiB append case, `C1t/edit_localized.rs:529-571`; 5 MiB constructed **and stored in SQLite**, `C2t/visibility.rs:54-70`, `C2t/pack_locator.rs:163-201`; 4 MiB end-to-end construction+storage+readback, `C2t/core_pipeline.rs:100-119` | u64 length field, then physical storage and wall time | no cap exists, so nothing rejects a large file: the first failure would be a resource error (`BoundedCapacityExceeded` or `Integrity("bounded Zstandard workspace unavailable")`) or disk exhaustion | `C1/file/mapping/types.rs:222-235`; `C1/object/codec.rs:26-44`; `C1/policy.rs:134-153` | verified to 24 MiB (C1) / 5 MiB (C2); envelope above that is derived-theoretical, not a promise |
| **3. File size** — whole-file read maximum | C1 read API | bytes | unbounded (`u64::MAX`) | caller-declared via `read_all_bounded` | none in `read_all`; the bounded form refuses `logical_len > maximum` | 2^64 − 1 | 5 MiB read back, `C2t/pack_locator.rs:186-201`; 4 MiB, `C2t/core_pipeline.rs:100-119` | caller's declared maximum, else resources | `BoundedCapacityExceeded { what: "read.logical_length" }` | `C1/file/read.rs:19-57` | verified at the sizes above only |
| **3. File size** — read wave (per-read working set) | C1 traversal | objects / bytes | 32 distinct payloads (declared 1 MiB) | none | flush at 32 distinct ids or 128 demands | derived: 32 x 32,768 = 1,048,576 B declared; real demand count can reach 128 entries before the demand flush | a long read never holds the whole file (wave released per flush) — structural, `C1/file/mapping/read.rs:54-133`; no oracle asserts a peak byte figure | 32 distinct payloads | wave flushed early; not an error | `C1/file/mapping/read.rs:19-22,65-81` | derived; **the byte figure is declared, not measured** (not-yet-qualified) |
| **3. File size** — edit working set (deferred unfinished nodes) | C1 edit | charged bytes | 8 MiB − 1 | none | `hold_node` charges canonical + 128 B per node and fails past the bound; nodes are never evicted, so a multi-edit stream accumulates | derived: with 8,192-byte pages the bound holds ~1,008 distinct unfinished nodes per operation | **not exercised**: no oracle drives an edit to this bound; the closest is 200 edits on a 20,000-byte file (`C1t/edit_batch.rs:157-178`) and 64 edits x 8 rounds on ~150 KB files (`C1t/edit_bounds.rs:214-238`) | 8 MiB charged bytes | `BoundedCapacityExceeded { what: "edit.deferred_nodes" }` (the operation fails; state is not dropped) | `C1/file/edit/tree.rs:26-33,105-139` | enforced but not-yet-qualified at the boundary |
| **4. Files in one operation** — streaming across supplied file roots | C2 SaveOperation | file roots per save | none | none | **no cap**; objects stream through a bounded batch and are written as they arrive | 2^256 roots in principle; the operation holds at most the pending batch plus one open group per lane | 16 file roots in one save, all read back byte-for-byte, `C2t/core_pipeline.rs:179-224`; 2 roots into shared packs, `C2t/cas_reuse.rs:228-260` | nothing — bounded by storage | n/a | `C2/cas/store.rs:259-309`, `C2/cas/save.rs:22-72` | verified to 16; no upper cap exists |
| **4. Files in one operation** — per-batch bound | C2 PendingBatch | objects / canonical bytes | 512 objects, 512 KiB | none | `push` drains when `len >= 512` or `bytes + len > 512 KiB` | derived: 512 x 16 MiB is *not* the real maximum, because a single object larger than the byte bound is still accepted into an empty batch (deliberate singleton path) | pending never exceeded the declared bounds across 200 accepted 6,000-byte objects, `C2t/memory_bounds.rs:104-140`; 16 files, `C2t/core_pipeline.rs:207-210`; >256 objects streamed through several waves in one save, `C2t/pack_locator.rs:163-201` | 512 objects / 512 KiB per wave | drain and flush; the save continues | `C2/policy.rs:57-59`; `C2/cas/batch.rs:43-75` | verified |
| **4. Files in one operation** — transaction bounds | C2 write transaction | rows / canonical bytes | 8,191 rows, 4 MiB − 1 | none | `maybe_commit` commits and re-acquires `BEGIN IMMEDIATE` when either bound is reached; the re-acquire is a second attempt of a *new* transaction, not a retry of a failed one | derived: a save may span an unbounded number of transactions, so this is a checkpoint bound, not an operation bound | 5 MiB file written across several early commits, `C2t/visibility.rs:54-70` (asserts `highest > ceiling` while the save is open) | row/byte bound | commit + lazy `BEGIN IMMEDIATE`; a **lost lock at that point** is a definite failure that proceeds to cleanup | `C2/policy.rs:61-63`; `C2/cas/owner.rs:772-790` | verified |
| **4. Files in one operation** — query / cleanup page widths | C2 SQL | ids per statement | 128 ids, 128 rows | none | fixed; the IN-list width is built from the actual page | derived: well under any engine variable limit | every locator read is paged; exercised throughout, e.g. `C2t/pack_locator.rs:130-201` | 128 | n/a | `C2/policy.rs:55,65`; `C2/sqlite/lookup.rs:36-38`; `C2/sqlite/cleanup.rs:34-43` | verified by construction |
| **4. Files in one operation** — reference bookkeeping | C2 Availability | ids per wave | n/a | none | no cap; one map/set entry per wave identity | derived: bounded by the wave (512 objects) times their distinct references | 5 MiB workload with >256 objects, `C2t/pack_locator.rs:170-201` | wave size | `MissingDependency { object, reference }` when a reference is not present | `C2/cas/dependencies.rs:20-88` | verified |
| **4. Files in one operation** — advisory predecessors | C1 object | ids per object | 4 | none | enforced at push | 4 | chain building uses exactly one predecessor per step, `C2t/delta_chains.rs:24-30` | 4 | `BoundedCapacityExceeded { what: "object.predecessors" }` (duplicates are absorbed, not counted) | `C1/object/predecessor.rs:13-14,60-74` | enforced; boundary not exercised |
| **5. Files per workspace** | **no owner exists in this tree** | — | — | — | **no workspace membership, count or enforcement exists in C1/C2** | not derivable | underlying verified scale instead: 16 file roots in one save (`C2t/core_pipeline.rs:179-224`); 1,312 distinct root objects persisted across 1,312 saves in one Store, `C2t/metadata_window.rs:130-166` (inode leaves, not file roots); 512 pooled leaves in one Store, retained receipt `e1c-pooled-512` | n/a — deferred | n/a | `core/Cargo.toml` (members are content, storage, telemetry only); `C1/lib.rs:7`; `C2/lib.rs:7`; `SQL:13-69` | outside-Stage-3-4-ownership — defer the workspace cap to Stages 5/7 |
| **6. Directory length/size** — name component | none in core | bytes / characters | none | none | **no code**: `inode_leaf.rs` implements no directory traversal, no inode allocation, no hardlink update and no filesystem-tree construction | not derivable from this tree; a pooling value or inode-leaf row cap does **not** establish it | none | n/a | n/a | `C1/object/inode_leaf.rs:1-8`; Stage 5 owns filesystem-tree algorithms (handoff §E) | outside-Stage-3-4-ownership — Stage 5/7 review required |
| **6. Directory length/size** — full path length | none in core | bytes | none | none | no code | not derivable; host `PATH_MAX` is **not** imported as a core contract anywhere | none | n/a | n/a | as above | outside-Stage-3-4-ownership |
| **6. Directory length/size** — nesting depth | none in core | levels | none | none | no code | not derivable | none | n/a | n/a | as above | outside-Stage-3-4-ownership |
| **6. Directory length/size** — entries per directory | none in core | count | none | none | no code; `MAXIMUM_LEAF_ROWS = 100` caps rows in one **inode leaf page**, and mapping-tree height/fanout caps **extent pages** — neither is a directory entry limit | 100 rows/leaf, 128 entries/extent page: both are page capacities, not directory limits | 100-row leaves admitted and pooled, `C2t/metadata_window.rs:139-160`, `C2t/metadata_chain.rs:182-231` | n/a | n/a | `C1/object/inode_leaf.rs:23-24`; `C1/file/mapping/types.rs:12-19` | outside-Stage-3-4-ownership — do **not** read these as directory limits |
| **7. Workspace size** — sum of current logical file lengths | none | bytes | none | none | no owner, no quota | 2^64 − 1 per file x unbounded roots; no aggregate is computed anywhere | 5 MiB single file in a real Store, `C2t/visibility.rs:54-70`; 24 MiB single file in C1, `C1t/edit_localized.rs:276` | n/a | n/a | no code | outside-Stage-3-4-ownership — the core cannot promise a workspace maximum |
| **7. Workspace size** — retained-history logical bytes | none | bytes | none | none | no owner; nothing tracks retained revisions | unbounded by construction | 512 pooled leaves / 1,312 leaves retained in one Store, retained receipts + `C2t/metadata_window.rs:130-166` | n/a | n/a | no code | outside-Stage-3-4-ownership |
| **7. Workspace size** — unique canonical bytes | C2 Store | bytes | none | none | no cap; dedup is exact-identity reuse, so this is <= sum of distinct canonical objects | bounded by SQLite row storage | 323,584-byte Store for 512 pooled leaves (4,169,728 canonical bytes unframed), retained receipt `e1c-pooled-512/stdout.log` | storage size | n/a | `C2/cas/membership.rs:30-47` | verified that dedup changes logical vs physical by ~13x in that fixture |
| **7. Workspace size** — physical encoded Store bytes | SQLite file | bytes | none | none | no cap in product code; the engine's database maximum applies | see the Store/database rows | 323,584 B, `e1c-pooled-512` receipt; ~3.5 MB and ~2.1 MB Stores recorded in the smoke README (fixture stores since removed) | SQLite database maximum | engine error | `C2/sqlite/connection.rs:17-45` | environment-dependent |
| **7. Workspace size** — quotas | none | bytes | none | none | **no quota mechanism exists** | n/a | none | n/a | n/a | no code | outside-Stage-3-4-ownership |
| **8. Store/database** — object identity space | C1/C2 | ids | 32-byte BLAKE3 | none | fixed width | 2^256 | n/a (width assertion only) | none | n/a | `C1/object/id.rs:12-13,25-31`; `SQL:50` | verified by construction |
| **8. Store/database** — pack id | C2 placement | i64 | next = baseline + 1 | none | `checked_add` on acquisition and on placement; `pack_id > 0` | 2^63 − 1 (i64) | monotonically increasing across all retained receipts; 1,024 packs for 512 pooled saves, `e1c-pooled-512` | i64 overflow | `Integrity("pack identifier overflow")` | `C2/cas/owner.rs:172-174`; `C2/pack/placement.rs:95-101`; `SQL:34-37` | derived-theoretical |
| **8. Store/database** — groups per pack, records per group | C2 pack grammar | counts | 256 groups / 8,191 records | none | enforced in assembly, in the header parser and in the schema | 256 (u32 header field is wider, the constant binds); records 8,191 (u16-width directory is wider, the constant binds) | 1,024 packs with 1 group each, `e1c-pooled-512`; locator stability across a full lane and multiple packs, `C2t/pack_locator.rs:62-128` | 256 / 8,191 | `Integrity("pack group count")` / directory bounds | `C2/policy.rs:49-51`; `C2/pack/layout.rs:99-137,224-275`; `SQL:61-62` | enforced |
| **8. Store/database** — assembled pack size | C2 pack assembly | bytes | 262,144 ordinary; 16,777,216 + 4,096 singleton | none | enforced before placement and re-checked by `parse_header` on every read | 262,144 for the four ordinary lanes; 16,781,312 for Singleton; derived from `CANONICAL_LIMIT` (16 MiB) + 4,096 framing slack | singleton pack limit asserted, `C2t/physical_formats.rs:77,98`; over-limit header rejected, `C2t/physical_formats.rs:70-80` | 262,144 / singleton limit | new pack selected by ordinary placement; over-limit header → `Integrity("pack length")` | `C2/policy.rs:47,78-80`; `C2/pack/layout.rs:107-113,265-267` | verified |
| **8. Store/database** — group body / frame sizes | C2 codec | bytes | 65,536 body, 66,560 frame; pooled 16,384 | none | enforced on compress and decompress, plus a frame-header cross-check of declared size, window size, dict id and checksum | 65,536 (u32 fields wider) | group bound enforced by the decompressor's declared-length check; pooled group bound 16,384 fixes 165 values/group, verified `C2t/metadata_pool.rs:136-176` | 65,536 / 16,384 | `Integrity("group body bounds")` / `("group body frame bounds")` | `C2/encoding/codec.rs:36-43,324-368,531-568`; `C2/policy.rs:85` | enforced |
| **8. Store/database** — metadata ordinals | C2 catalogue | count | starts at 1 | none | `checked_add` on assignment and on read; schema CHECK bounds | 4,294,967,295 first ordinals, and `first_ordinal + count <= 4,294,967,296`, i.e. **2^32 total ordinals** — this is the real exhaustion bound | 131,400 ordinals persisted through a real Store, `C2t/metadata_window.rs:130-241`; 51,200 rows resolved to 611 values in the 512-leaf receipt | 2^32 ordinals | `Integrity("metadata ordinal maximum")` on `next_ordinal`; the schema CHECK would refuse the row first | `SQL:40-46`; `C2/sqlite/pool.rs:38-49,52-71`; `C2/cas/owner.rs:553-563` | verified to 131,400; ceiling derived from the CHECK |
| **8. Store/database** — canonical length column | C2 schema | bytes | n/a | none | schema CHECK `> 0 AND <= 16777216`; code re-checks against `CANONICAL_LIMIT` | 16,777,216 | 1,048,598-byte whole-file record stored, `C2t/policy_capacity.rs:110-143` | 16 MiB | `Integrity("canonical length")` / CHECK constraint failure | `SQL:52-53`; `C2/encoding/decode.rs:35-38`; `C2/policy.rs:323-331` | enforced |
| **8. Store/database** — pack BLOB length | C2 schema | bytes | n/a | none | DDL declares only `length(data) >= 32` (a lower bound); the real upper bound is the assembly limit | 16,781,312 (singleton lane) | 1,048,598-byte record written as a singleton pack, `C2t/policy_capacity.rs:110-143` | assembly limit, not the DDL | assembly refusal before insert | `SQL:36`; `C2/encoding/full.rs:172-202` | enforced by assembly; the DDL is not the bound |
| **8. Store/database** — SQLite engine page/database/runtime limits | environment (system library) | bytes / pages | page_size not set; cache_size not set; max_page_count not set | none | **the product sets only** journal_mode MEMORY, synchronous OFF, temp_store MEMORY, foreign_keys ON, busy_timeout 0 — no `sqlite3_limit` call exists anywhere in C2 | engine defaults, i.e. **environment-dependent**: the crate enables rusqlite's `limits` feature but never uses it, and the build links a **system** `libsqlite3` (build-script output `cargo:rustc-link-lib=sqlite3` with no bundled compilation), so the effective defaults are the host library's | largest retained Store in evidence: 323,584 B; largest test Store: a 5 MiB file (`C2t/visibility.rs:54`) | engine defaults | engine error (e.g. SQLITE_FULL), or `OwnershipUnavailable` on a lost lock | `C2/sqlite/connection.rs:17-72`; `core/crates/layerfs-storage/Cargo.toml` (rusqlite 0.40.2 features cache,hooks,trace,limits,blob); `core/target/debug/build/libsqlite3-sys-*/output` | environment-dependent — **not qualified for this snapshot**; a DB file maximum is in any case **not** a maximum single file or workspace |
| **8. Store/database** — unimplemented cloud/remote providers | outside scope | n/a | n/a | n/a | not implemented, therefore unqualified | n/a | none | n/a | n/a | no code in C2 | outside-Stage-3-4-ownership |
| **9. Pool/index** — candidate window | C2 Store-owned ordered set | retained entries / bytes | `next = 1`, empty | none | `note_group`/`sync` clear the **whole** window when the next group would exceed 131,072 entries; declared live byte ceiling 32 MiB | 131,072 entries (= 2^17); derived byte size 131,072 x 16 = 2,097,152 B in the tested configuration, far under the declared 32 MiB | the cap itself is retained exactly and one value past it resets the window wholesale, `C2t/metadata_window.rs:76-127`; a real 1,312-leaf Store crosses the bound at group 1,311 and retains 200 entries, `C2t/metadata_window.rs:130-166` | 131,072 entries | eviction duplicates a physical value, **never** deletes a stored value; the evicted value is re-written and the original still reads back by identity | `C2/policy.rs:116-118`; `C2/encoding/pool/index.rs:72-97,124-149,224-246`; `C2t/metadata_window.rs:196-206` | verified |
| **9. Pool/index** — persisted metadata values (all, not the window) | C2 catalogue + packs | values / groups | n/a | none | none per Store; bounded only by the 2^32 ordinal ceiling | 2^32 ordinals; 165 values/group is the packing rule, not a total bound | 131,400 ordinals / 1,312 groups, `C2t/metadata_window.rs:157-160` | 2^32 ordinals | as the ordinal row | `C2/policy.rs:83`; `SQL:39-47` | verified to 131,400 |
| **9. Pool/index** — rows per pooled leaf, leaf width | C1 grammar / C2 lane | rows / bytes | 100 rows | none | enforced on encode and decode; C2 re-checks `INODE_LEAF_LIMIT = 8,192` on every pooled read | 100 rows (u16 count is wider, the constant binds); canonical leaf = 31 + 100x81 = 8,131, +13 = 8,144 | 100-row leaves pooled throughout, `C2t/metadata_chain.rs:182-231`, `C2t/metadata_window.rs:139-160` | 100 rows | `NonCanonicalPagePartition` / `Integrity("pooled row count")` | `C1/object/inode_leaf.rs:23-24,173-215`; `C2/policy.rs:110,114`; `C2/encoding/pool/read.rs:217-247,261-263` | verified |
| **9. Pool/index** — value-group cache in a read wave | C2 PoolReader | bytes | 512 KiB | none | whole-cache reset when full | 512 KiB; derived ~6,300 values at 81 B each | not exercised at the reset boundary; the largest observed group cache is bounded by the 165-value group width | 512 KiB | cache cleared and re-read; not an error | `C2/encoding/pool/read.rs:26-27,94-100` | enforced; not-yet-qualified at the boundary |
| **9. Pool/index** — fingerprint collisions | C2 index | n/a | low 8 digest bytes | none | fingerprint is a **candidate filter only**; full 73-byte comparison decides | 2^64 fingerprints; collisions are expected and handled | a real searched collision pair is proved to be a real collision and is never confused through the Store, `C2t/metadata_fingerprint_collision.rs:74-140`; retained receipt `stages-3-4-fingerprint-collision-20260917T021500Z/collision.json` | nothing — a collision costs an extra group read | correct value resolved; no error | `C2/encoding/pool/index.rs:160-215,248-254` | verified |
| **10. Edits/concurrency** — edits per operation | C1 EditStream | count | n/a | none | `MAXIMUM_EDITS_PER_OPERATION = 4,096`, enforced **before any work** | 4,096 (usize, the constant binds) | largest exercised stream is **200 edits** on a 20,000-byte file, `C1t/edit_batch.rs:157-178`; 64 edits x 8 rounds, `C1t/edit_bounds.rs:214-238` | 4,096 | `BoundedCapacityExceeded { what: "edit.stream" }`, rejected before mutation | `C1/file/edit/input.rs:21-22,99-106` | enforced; boundary not-yet-qualified |
| **10. Edits/concurrency** — maximum accepted edit shape | C1 EditStream | coordinates | n/a | none | inverted range, range beyond the current result, overlap with an earlier edit, and reaching into an earlier replacement are all rejected; adjacent edits stay separate; length accumulation is checked | u64 coordinates with checked add/sub | rejected cases asserted, `C1t/edit_single.rs:240-281`, `C1t/edit_batch.rs:180-215`; accepted adjacent and shifted cases, `C1t/edit_batch.rs:125-156` | the applicability rule (no reaching into an earlier replacement) | `InvalidEdit` with `"inverted range"` / `"range beyond current result"` / `"overlapping ranges"` / `"range inside an earlier replacement"`, all before any read or emit | `C1/file/edit/input.rs:97-165,319-384` | verified |
| **10. Edits/concurrency** — replacement length | C1 EditSource | bytes | n/a | caller-declared per replacement, u64 | no explicit cap; the declared length must equal what the source yields, and a below-T result materializes `final_len` bytes in one allocation under `try_reserve_exact` | 2^64 − 1 declared; the practical binding is the allocation failure for a below-T whole-object assembly | 2 MiB single replacement, `C1t/edit_noop.rs:152-183`; 1,048,576-byte replacement at the 1 MiB cutoff (smoke receipt, earlier commit identity) | allocation for whole-file results; streaming otherwise | short source → `ContentError::Io` (`C1t/edit_bounds.rs:275-317`); declared/actual mismatch → `InvalidEdit { what: "replacement length" }` checked before the builder runs (`C1/file/edit/apply.rs:244-251`); allocation refusal → `BoundedCapacityExceeded { what: "edit.assembly" }` | `C1/file/edit/input.rs:167-270`; `C1/file/edit/apply.rs:127-197,244-251` | verified to 2 MiB |
| **10. Edits/concurrency** — replay requirement | C1 | passes | 1 planned comparison pass + 1 construction pass | none | at most one no-op comparison pass, then at most one construction pass; both are charged; there is no retry | 2 passes over the declared edits | no-op detection incl. a 1 MiB equal prefix then an early-stopping mismatch, `C1t/edit_noop.rs:128-183`; comparison window 64 KiB | 64 KiB comparison window | a mismatch is not an error: construction proceeds; nothing is re-probed | `C1/file/edit/compare.rs:18-19,38-85`; `C1/file/edit/apply.rs:62-74` | verified |
| **10. Edits/concurrency** — finality / frontier | C1 EditObjects | nodes | n/a | none | unfinished nodes are held under the 8 MiB charge bound and published children-first; a node a later split overwrote is simply never reached | bounded by tree height, page capacity and the deferred bound — **not** by file bytes or edit count | localized work does not grow with file size (8 MiB vs 24 MiB demand the same payloads and at most +1 emitted page), `C1t/edit_localized.rs:471-521`; 80+100 → 90+90 repartition and unequal-height joins in the sealed oracle receipts; >700-extent edit demands <= 6 pages of >= 8, `C1t/edit_localized.rs:274-419` | 128 entries/page + height | `InvalidRecord("extent summary")` / `("left concat levels")` / `("right concat levels")`; over the deferred bound → `BoundedCapacityExceeded` | `C1/file/edit/tree.rs:15-33,90-202,384-608` | verified for work; the deferred byte bound itself not-yet-qualified |
| **10. Edits/concurrency** — simultaneous writers / Stores | C2 save | writers per Store | 1 | none | one `BEGIN IMMEDIATE` attempt, zero busy timeout; a lost lock is an immediate definite failure | 1 writer per Store process instance; a second Store object on the same path contends for the same SQLite file lock | a second save cannot acquire ownership while the first holds it, `C2t/persistence_failure.rs:27`; the save then proceeds normally afterwards | the SQLite write lock | `OwnershipUnavailable` | `C2/sqlite/write.rs:44-49`; `C2/sqlite/connection.rs:43,53-72` | verified |
| **10. Edits/concurrency** — simultaneous readers | C2 read | readers | unbounded | none | each read opens its own connection and captures the publication watermark once | unbounded; readers never block writers in the supported profile | same-save reads observe accepted objects while an unrelated reader does not, `C2t/pack_locator.rs:130-201`, `C2t/visibility.rs:48-120` | the watermark | `VisibilityCeiling { pack_id, ceiling }` rather than a missing-object error | `C2/cas/store.rs:202-246`; `C2/cas/read.rs:41-68` | verified |
| **10. Edits/concurrency** — error behaviour at a boundary | C2 | n/a | n/a | n/a | refused-before-mutation (validation), ordinary planned FULL/new-pack selection, resource error, or one late cleanup attempt | n/a | both failure classes exercised: definite failure keeps the original error and cleans up once, `C2t/core_pipeline.rs:227-267`; unknown outcome quarantines, `C2/cas/finish.rs:12-24` | the single-attempt contract | definite failure → terminal + one cleanup; unknown outcome → quarantine, nothing deleted or resent | `C2/cas/owner.rs:855-889`; `C2/sqlite/cleanup.rs:30-76` | verified |
| **10. Edits/concurrency** — fixed-worker operation limits | none in C1/C2 | n/a | n/a | n/a | **C1 and C2 contain no worker pool, thread or parallel lane at all** — a worker limit is therefore not a workspace-size limit here | n/a | n/a | n/a | n/a | grep: no thread pool or worker constant in either crate | recorded so it is not mistaken for a capacity |

---

## 2. Binding-constraint derivations (compatible units only)

**Largest chunked file (derived).** The format fields that can bind are, in compatible byte units:

1. `ExtentSlice.source_offset` and `logical_length` are `u32` — a single extent expresses at most
   4 GiB − 1, but a *readable* extent is additionally bounded by the chunk payload it points into:
   `encode_chunk_object` refuses raw > 32,768 (`C1/file/mapping/codec.rs:53-59`) and the read path
   requires the demanded slice to exist inside that payload (`C1/file/mapping/read.rs:120-122`), so the
   effective per-extent contribution is **<= 32,768 B**, not 4 GiB.
2. Extent count: `subtree_extent_count` / `extent_count` are `u64`, and the tree can hold at most
   128^32 extents = 2^224 (fanout 128 at every branch, height <= 31, 128 extents per leaf).
   2^224 x 32,768 B = 2^239 B.
3. Logical length: `FileState.logical_len` and every `subtree_logical_bytes` are `u64`, with checked
   adds on every accumulation path.

Since 2^239 B is far above 2^64 B, **(3) binds first**: the format ceiling is
**2^64 − 1 = 18,446,744,073,709,551,615 B ≈ 16 EiB**. This is a *field-width* ceiling, and reaching
it would need ~2^49 extents — it is not a supported envelope and this report does not present it as
one. The supported envelope is bounded by physical storage and the resource limits in the table.

**Largest whole-file object (derived).** `T − 1` raw + 10-byte value header + 13-byte envelope;
at the maximum accepted T = 1,048,576 that is 1,048,598 B. The 8 MiB field cap and the 16 MiB
envelope cap sit far above it and never bind for this profile.

**Pooled ordinal ceiling.** `metadata_value_groups.first_ordinal` is CHECKed to 1..4,294,967,295 and the
table additionally CHECKs `first_ordinal + count <= 4294967296`, so the binding count is **2^32
ordinals**, not the 165-values-per-group packing rule and not the 131,072-entry window.

**No workspace-size derivation is possible**: there is no owner of "a workspace" in this tree
(`core/Cargo.toml` members are content, storage and telemetry only).

---

## 3. How each boundary fails, traced against the single-attempt contract

- **Validation before mutation.** Cutoff/depth/profile (`C1/policy.rs:84-105`, `C2/policy.rs:166-188`),
  edit count and shape (`C1/file/edit/input.rs:99-133`), advisory predecessor count — all reject before a
  Store file, a read or an emit exists. Verified: `C2t/policy_capacity.rs:51-69` asserts
  `!path.exists()` after each rejection, and `C2t/memory_bounds.rs:236-250` the same.
- **Ordinary planned FULL / new-pack selection.** Depth-ineligible, absent, or losing candidates are
  *policy*, not failure: `select` returns the already-prepared FULL and counts `no_candidate` /
  `ineligible_candidates` / `full_losses` (`C2/encoding/delta/select.rs:232-292`); `plan_lane` starts a
  new pack or the singleton lane when the compact record does not fit
  (`C2/encoding/full.rs:172-202`). Verified by `C2t/delta_chains.rs:79-99` and
  `C2t/delta_payload.rs:159-202`.
- **Resource error.** Codec workspace/estimate/bound failures (`C2/encoding/codec.rs:212-215,286-288`),
  batch/transaction/chain/deferred/node capacity failures all return a typed error. A codec or read
  failure is **never** turned into a FULL alternative (`C2/cas/owner.rs:320-329`); a corrupt base fails
  the save (`C2t/delta_payload.rs:223-245`).
- **Late failure with one cleanup attempt.** A definite failure rolls back and runs exactly one
  bounded deletion pass over packs above the captured baseline (`C2/cas/owner.rs:855-868`,
  `C2/sqlite/cleanup.rs:30-76`); an unproven outcome quarantines and deletes nothing
  (`C2/cas/finish.rs:12-24`). The retained file stays readable and the failed attempt adds no pack
  (`C2t/core_pipeline.rs:227-267`). No path retries, resends or swaps algorithms to satisfy a limit.

---

## 4. Oracle defects found while reading the evidence

1. **`C1t/edit_bounds.rs:240-273` (`the_retained_frontier_does_not_grow_with_the_file`) does not check the
   retained frontier.** The edit it applies is `Edit::delete(0, 0)` — a zero-length deletion with a
   zero-length replacement. `apply_edits` short-circuits it: the comparison pass finds a zero-length
   replacement equal to its base range and returns **the base root without running the builder at
   all** (`C1/file/edit/compare.rs:46-51`, `C1/file/edit/apply.rs:62-74`). `peaks` is then the *extent
   count of the unchanged base* (40 and 400), so the assertions `peaks[0] <= 192 && peaks[1] <= 192 * 32`
   are vacuous with respect to the builder's retained state. The comment "the builder is exercised
   through the same public edit path above" is not true for this case. **This is missing evidence,
   not a demonstrated defect**: no `peak_pending` / `peak_deferred_bytes` figure is asserted anywhere in the
   C1 suite.
2. **`C2t/policy_capacity.rs:146-158` checks the wrong axis for budget independence.** It compares the
   128 KiB and 1 MiB cutoffs at identical depths and concludes that "the cutoff, the depths and the
   chain/work budgets move independently". The budgets are `const` and cannot move with depth in code,
   but no oracle compares depth 4 with depth 50 budgets.
3. **Constant-equality assertions are not boundary tests.** `C1t/object_identity.rs:250-251` asserts the
   8 MiB field and 16 MiB envelope constants; no object at either magnitude is encoded, stored, or
   rejected at ±1.

---

## 5. Boundaries NOT run (honest gaps)

- No `cargo` command was executed by me (assignment constraint). Test outcomes cited from
  `checks.log` are the parent agent's runs at this snapshot.
- Payload **CHUNK** depth boundary — never driven to a changed cap.
- Payload depth **50** — accepted and persisted, never exercised.
- Delta-depth vs budget independence — source-derived only (defect 2 above).
- `MAXIMUM_EDITS_PER_OPERATION = 4,096` — never reached; the largest exercised stream is 200 edits.
- `EDIT_DEFERRED_LIMIT = 8 MiB − 1` — never approached; also unobservable because no oracle asserts a peak.
- Read-wave byte ceiling (declared 1 MiB / 32 payloads) — declared, not measured.
- `PoolReader` 512 KiB value-cache reset — not exercised.
- 8 MiB value-field / 16 MiB envelope ceilings — constant assertions only.
- File-size envelope above 24 MiB (C1) and 5 MiB (C2) — **not run**; this audit deliberately did not
  allocate multi-terabyte maxima or start an endurance campaign.
- SQLite engine maxima — **not qualified**: no `sqlite3_limit` / page-size / max-page-count setting
  exists, and the build links a system `libsqlite3` (build-script output
  `cargo:rustc-link-lib=sqlite3`, no bundled compilation), so the effective defaults belong to the
  host library and not to this repository.
- Workspace count/size, directory name/path/depth/entry limits — **do not exist in this tree**; deferred.
- Identity of retained receipts: the timing round is at `dfd54fd8e` and the smoke round at an earlier
  commit. Neither is this snapshot; they are cited as behaviour evidence for the same code paths,
  not as measurements of `91c3a0741`.

---

## 6. Direct plain answers

**File revisions.** No revision-count cap found in reviewed C1/C2 code; there is no history, branch,
checkpoint or revision entity at all, so a history/revision service is outside scope; the number of
retained immutable file roots grows until pack-identifier, ordinal or storage bounds. Retaining a
root and its dependencies is separate from computing it: computing is O(height) memory, retaining is
one object row per root with no counter. 16 file roots were verified stored in a single save
operation, and 1,312 distinct root objects were verified persisted across 1,312 saves in one Store
(inode leaves, not file roots — the file-root count itself was not measured beyond 16). No
successful-version rollback exists: cleanup only deletes packs above the save's own baseline.

**File size.** There is no enforced maximum file size. The routing threshold T is configurable only
as a power of two from 128 KiB to 1 MiB and is not a maximum: below it a file is one WHOLE_FILE
object capped at T − 1 raw bytes (1,048,575 raw / 1,048,598 canonical at the largest accepted T), at
or above it the file is a chunked extent tree whose only format ceiling is the u64 logical-length
field, 2^64 − 1 bytes ≈ 16 EiB (derived-theoretical, not reachable and not a promise). Largest
actually verified: 24 MiB (25,165,824 B) constructed and edited through C1 public APIs with >= 700
extents and >= 8 mapping pages; 5 MiB constructed, stored in SQLite and read back; 2,097,151 B
edited end-to-end through a real Store in the smoke round at an earlier commit. The boundary above
24 MiB was not run.

**Files per workspace.** No filesystem or Workspace owner exists in this tree, so Stage 3–4 stores
independent roots and implements no workspace membership or count enforcement; the workspace cap is
deferred to Stages 5/7 review. The underlying verified scale is 16 file roots in one save operation
and >256 objects streamed through one save for a 5 MiB file. The 512-object batch and the 128-ID
query page are wave/page widths and do not bound how many files may exist.

**Directory dimensions.** Directory name-component length (bytes or characters), full path length,
nesting depth and entries-per-directory limits are **not established by Stages 3–4**; no code in this
tree implements directory traversal, and the inode leaf module explicitly disclaims it. The 100-row
inode-leaf cap and the 128-entry mapping-page cap are page capacities, not directory limits. Host
`PATH_MAX` / `NAME_MAX` were not imported as core contracts. Stage 5/7 review required.

**Workspace size.** The core cannot promise a workspace maximum. No aggregate of current logical
lengths, retained-history bytes, unique canonical bytes or physical Store bytes is computed
anywhere, and no quota mechanism exists. The physical side is bounded only by the SQLite database
maximum, which is environment-dependent for this build (system `libsqlite3`, no page-size /
max-page-count / limit override) and is in any case not a maximum single-file or workspace figure.
Largest retained Store observed in evidence: 323,584 bytes.
