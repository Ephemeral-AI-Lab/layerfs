# Report D — C2 storage crate complexity research

- Source commit: `5e45897dd9f56e7f563aada031a68070a438fb93` (tree clean at inspection; HEAD
  moved to `2c82bf2042ffe4c05ecffa5a0a3081603fecf3b2` during research — both intervening
  commits are docs-only and `git diff 5e45897 2c82bf2 -- core/crates/layerfs-storage/` is
  empty, so every citation below is valid at both commits).
- Method: static source reading of `core/crates/layerfs-storage/src/` only. No build, no
  tests, no measurements, no performance claims. Measured/reviewed facts are cited to
  existing receipts under `docs/roadmap/0.1/0.1.7/evidence/`.
- Paths are relative to the repository root. "receipt" citations are prior review
  documents, not new measurements.

## Complexity variables

| Variable | Meaning | Declared ceiling (path:line) |
| --- | --- | --- |
| `n` | objects offered to one save operation | none per operation; batched into waves |
| `w` | objects in one preparation wave | 512 objects / 512 KiB canonical (`policy.rs:65-67`), enforced at `batch.rs:50-58`; one oversized object accepted alone (`batch.rs:44-48`) — ENFORCED |
| `r` | records in one group | 8191 (`policy.rs:59`), enforced `assemble.rs:30` — ENFORCED |
| `g` | groups in one pack | 256 (`policy.rs:57`), enforced `layout.rs:275`, singleton lane 1 (`layout.rs:129-136`) — ENFORCED |
| `p` | packs touched by one operation/wave | no count bound; retained pack bytes bounded 4 MiB wholesale (`policy.rs:113`) |
| `d` | dependency chain depth (edges) | role cap 0..50 (`schema.sql` CHECK `BETWEEN 0 AND 50`; `layerfs-content/src/policy.rs:24` `MAXIMUM_DELTA_MAX_DEPTH = 50`), enforced on walk (`delta/read.rs:147-149`, `pool/read.rs:223-225`) — ENFORCED |
| `C` | canonical bytes of one object | 16 MiB (`policy.rs:61`), enforced `full.rs:111-117` — ENFORCED |
| `B` | one pack BLOB | 256 KiB ordinary/native/whole-file/pooled (`policy.rs:55`), singleton 16 MiB + 4 KiB (`policy.rs:88-90`) — ENFORCED at parse (`layout.rs:287-289`) but only *after* materialization (see §Round trips) |
| `v` | distinct pooled values in one leaf | ≤ leaf rows 100 (`policy.rs:97`) — ENFORCED by `PENDING_VALUES_LIMIT` (`owner.rs:36`) |
| `m` | members waiting in unfinished groups | ≤ 2 lanes × 8191 records (`policy.rs:59`); whole-file/pooled/singleton lanes seal immediately (`owner.rs:373-374,403-408`) — STRUCTURAL |
| `L` | rows in one pooled leaf | 100 (`policy.rs:97`) — ENFORCED at decode (`pool/read.rs:327-329`) |

Bound classes: **ENFORCED** = a runtime check fails the operation; **STRUCTURAL** = bounded
by construction or a constant without a dedicated runtime check; **UNKNOWN** = no bound
found. Peak-memory figures are the crate's *declared* ceilings; whether SQLite itself
honors them is not verifiable statically.

## 1. Algorithm inventory

Rows: name | path:line | time (variables defined above) | peak owned memory | I/O trips | class.

### Save path

1. **Batch flush trigger** | `cas/batch.rs:48-69`, limits `policy.rs:65-67` | O(1) per accept; drains wave when `len >= 512 || bytes + next > 512 KiB` (`batch.rs:50-52`) | wave holds ≤512 objects / ≤512 KiB canonical (ENFORCED) | flush is synchronous inside `accept` (`store.rs:295-299`) | ENFORCED. Receipt `stages-1-5-review-20260917T160000Z/sa5-c2-timing.md:19` records 512 obj/512 KiB as one wave and one COMMIT per save in the ordinary case.
2. **Wave membership resolve** | `cas/save.rs:26-31` | one paged `locations` query per wave (`save.rs:27`), pages of 128 ids (`lookup.rs:36-38`, `policy.rs:63`) → ceil(w/128) statements | wave-local `BTreeMap` | ceil(w/128) SELECTs, `prepare_cached` per page width (`lookup.rs:68`) | ENFORCED (page width).
3. **In-wave duplicate rule** | `cas/save.rs:41-49` | O(w) `prepared` map lookups; byte compare per duplicate | wave map | none | STRUCTURAL.
4. **Exact-reuse membership check (re-hash + byte compare)** | `cas/membership.rs:30-47`, `:15-24` | per occurrence: identity check O(1) (`:35`), length check O(1) (`:38`), chain reconstruction O(d) edges × per-record O(g + r) directory scans + one zstd group decompress per record (`delta/read.rs:109-183`, `decode.rs:39-55`), full re-hash O(C) (`membership.rs:20`), full byte compare O(C) (`:42`) | owner `pack_cache` ≤4 MiB wholesale (`policy.rs:113`), decompression arena ≤1 MiB lazy (`codec.rs:51,411-436`) | per edge 1 locator SELECT (`delta/read.rs:150`); pack BLOB read once per pack (cached, `delta/read.rs:246-266`) | ENFORCED (collision fails save; capacity budgets enforced `delta/read.rs:192-218`).
5. **Admission dependency checks** | `cas/dependencies.rs:47-70` | per object: O(refs) unresolved scan (`:72-85`) + one paged `present` query when refs missing (`:58`); `pending` closure scans all lanes' members O(m) per ref (`owner.rs:258-262`) | wave `Availability` set | 1 paged SELECT per object with unresolved refs | STRUCTURAL (no per-wave cap on total presence statements beyond wave size).
6. **Pending-member sealing on demand** | `cas/save.rs:60-68`, `owner.rs:289-310` | `seal_pending` scans ids × 5 lanes × members (`owner.rs:291-301`) | sealed group's framed bytes (lane pack limit) | 1 locator re-query after seal (`save.rs:62`) | STRUCTURAL.
7. **Delta candidate selection (trials, depth cache)** | `encoding/delta/select.rs:196-323` | per object: FULL prepare = one zstd compress O(C) (`:229`); CHUNK probes only first advisory (`:247-253`); whole-file probes advisory in order then signature cache (`:254-266`, `candidates.rs:139-165`, O(sig) per lookup); each probe = 1 locator SELECT + depth walk (`select.rs:356-374`); depth walk O(d) locator SELECTs, memoized in `DepthCache` ≤4096 entries, wholesale clear when full (`select.rs:27,94-144,147-153`) | depth cache ≤4096 × (~34 B + node share); candidate cache fixed 128 KiB (`candidates.rs:15-17`, asserted `:35-44`) | per probe ≥1 SELECT; per acquired base a full chain read (`:376-390`) | ENFORCED (exactly one trial per object, `:277`; chain budgets enforced `:282-298`).
8. **Pooled-leaf admission** | `owner.rs:465-567` | per leaf: index sync once per save (`:591-611`); distinct-value batch query via `PoolIndex::find` (`pool/index.rs:165-215`) — fingerprint range over window entries + per-candidate ordinal group seek; ordinal assignment O(1) after first `next_ordinal` SELECT (`owner.rs:614-624`, `sqlite/pool.rs:44-67` O(log n) windowed MAX); value-group build per 165-value chunk: frame + digest hash + one zstd compress (`value_group.rs:31-65`); one base acquisition (`owner.rs:714-764`) then one COPY/INSERT trial with 128 KiB match budget (`policy.rs:133`, `pool/delta.rs:125-225`) | `pending_values` ≤100 entries ≈12 KiB (`owner.rs:26-36`); pool reader caches ≤512 KiB values + ≤4 MiB packs (`policy.rs:121,113`) | per new group: 1 pack write + 1 catalogue INSERT (`owner.rs:662-671`); per trial a **fresh** `PoolReader` re-reads base pack bodies (`owner.rs:748`, caches not shared with owner's `pack_cache`) | ENFORCED (work budgets `pool/read.rs:135-140,246-252`; `work_exceeded` counters).
9. **Pack placement (`append_fits`, group counting)** | `pack/layout.rs:199-213`, `pack/placement.rs:66-132` | `append_fits` calls `assembled_length` which re-sums all g bodies (`layout.rs:216-234`) → O(g) per placement, O(g²) per pack; g ≤256 → ≤65,536 iterations per pack | open tail retains framed groups ≤ pack limit per lane × 5 lanes (`placement.rs:44-46`, `owner.rs:107-108`) | none (pure arithmetic) | STRUCTURAL (g² bounded by ENFORCED g).
10. **Pack assembly and BLOB write pattern** | `placement.rs:135-161`, `pack/assemble.rs:194-228`, `sqlite/write.rs:76-97` | every write assembles the whole pack from its groups (`assemble` borrows the whole open tail, `placement.rs:150-154`); a pack that stays open is re-assembled and **rewritten whole on every append** (`write.rs:88-97` `UPDATE object_packs SET data = ?2`); a pack with k appends ships Σ(i×B) bytes ≈ O(k²·B) through SQLite in the worst case | assembled pack buffer B + retained tail B (live together by design, `assemble.rs:188-193`); closing path releases bodies as copied (`assemble_consuming`, `assemble.rs:215-228`) | 1 INSERT (create, `write.rs:76-85`) or 1 UPDATE (append, `:88-97`) whole-BLOB statement per write | ENFORCED (pack length), quadratic shipping is STRUCTURAL. Verified against receipt `stages-1-5-review-20260917T230700Z/sa-adapters-limits.md:504` ("every append rewrites the whole pack BLOB").
11. **Object-row inserts** | `sqlite/write.rs:100-123`, loop `owner.rs:803-825` | one prepared INSERT per member | none | 1 statement per object | ENFORCED (cardinality check `:119-122`).
12. **Transaction accumulation + `maybe_commit`** | `owner.rs:845-862`, state `owner.rs:217`, limits `policy.rs:69-71` | O(1) per sealed group (counters + compare) | none (counters only) | trigger at ≥8191 rows or ≥4 MiB−1 canonical bytes → COMMIT + `BEGIN IMMEDIATE` restart (`owner.rs:850-858`); rows reset to 1 | ENFORCED trigger; receipt `sa5-c2-timing.md:19` records one COMMIT per save in the ordinary case.
13. **Save finish / watermark** | `owner.rs:888-925` | seals ≤5 lanes | — | watermark UPDATE + COMMIT (`:920-921`); empty write → ROLLBACK (`:902-904`) | ENFORCED.

### Read path

14. **Read wave (locations, ceiling, materialization, dependencies)** | `store.rs:202-233`, `cas/read.rs:42-101` | per wave: fresh connection (`store.rs:209`) + ceiling SELECT (`:213`, `schema.rs:254-267`) + ceil(w/128) locator pages with `i64::MAX` then per-location ceiling refusal (`read.rs:53,61-66`) + per object a full chain resolve (`:73-84`) + one re-hash O(C) per returned object (`:91`) | wave `packs` map ≤4 MiB wholesale (`read.rs:69`, `delta/read.rs:254-258`); decode arena ≤1 MiB lazy; returned `Vec<Vec<u8>>` is count-bounded (4096, `policy.rs:80`, `store.rs:259-268`) but **not byte-bounded** (receipt `sa-c2-storage.md:481`) | connection open + ~5-6 pragma statements per wave (`connection.rs:29-45`) + ceiling + pages + per-edge SELECTs + BLOB reads | ENFORCED (demand ceiling `store.rs:259-268`); result bytes UNKNOWN.
15. **Delta reconstruction (iterative chains, per-edge work)** | `encoding/delta/read.rs:109-183` | chain walk O(d) with 1 locator SELECT per edge (`:150`), chronology enforced (`:155-157`); decode loop per record: budget checks O(1) (`:192-218`), `pack_of` (cached whole-BLOB read, `:246-266`), `record_width` O(g) directory scan (`:282-297` via `layout.rs:325-389`), `decode_canonical` second O(g) `group_view` + **one zstd decompression of the whole group body per record** (`decode.rs:39-55`) | chain vector d+1; pack cache ≤4 MiB wholesale; decode arena ≤1 MiB | per edge 1 SELECT; per pack 1 BLOB read (cached); per record 2 × O(g) directory validation + 1 group decompress | ENFORCED (chain canonical 512 KiB / encoded 256 KiB, `policy.rs:102-104`, checked `read.rs:192-218`).
16. **Pooled read (leaf body + ordinals)** | `pool/read.rs:202-281,316-365` | leaf chain walk O(d) locator SELECTs (`:226-240`); per record whole group decompress (`:301-304`); ordinal resolve: covering-group memo changes group only when rows cross a group boundary (`:334-349`), per group 1 catalogue seek (`sqlite/pool.rs:92-121`, index seek O(log n)) + decode cached per wave | decoded values ≤512 KiB wholesale (`policy.rs:121`, `pool/read.rs:147-151`); packs ≤4 MiB wholesale (`:186-199`) | per chain step 1 SELECT; per group 1 seek + 1 BLOB read + 1 decompress | ENFORCED (`METADATA_DECODED_WORK_LIMIT` 32 MiB `policy.rs:123`, checked `pool/read.rs:135-140`; chain budgets `:246-252`).
17. **Pooled-value window/index mechanics** | `pool/index.rs:25-256` | `sync` once per save: cold start replays the whole-group eviction recurrence over the **entire catalogue** streamed (`retained_start`, `:224-248`, `sqlite/pool.rs:154-168`) then reads window groups forward (O(window/165) groups); warm start reads only new groups (`:124-147`); window ≤131,072 entries, wholesale reset (`policy.rs:142`, `index.rs:80-83,131-134`); `find` per leaf: per value a BLAKE3 fingerprint (`:251-256`) + BTreeSet range (`:185-190`) + per candidate ordinal a group seek/read, `values.contains` linear O(v) per candidate (`:207`) | window ≤131,072 × 12 B charge (`index.rs:151-158`) | cold start: 1 streamed catalogue scan + O(window groups) BLOB reads + decompressions | ENFORCED entry bound; cold-start catalogue scan is STRUCTURAL O(total groups).
18. **Membership presence (`contains`)** | `store.rs:236-251` | paged IN query, no locators | none | fresh connection + ceiling + ceil(w/128) SELECTs | ENFORCED (demand ceiling).

### Cross-cutting

19. **Per-call connection opens** | `store.rs:118,135,187,209,247` + `connection.rs:17-45` | every public call opens, configures and drops its own connection: 1 open + `journal_mode` query/set + 2 `execute_batch` + `foreign_keys` query + `busy_timeout` set ≈ 5-6 statement round trips before real work | none | 1 connection per wave/save/contains | STRUCTURAL (no pool; receipt `sa-adapters-limits.md:299` "per-call connection, no pool").
20. **zstd codec calls** | `codec.rs:196-250,258-334,341-385,444-493,501-564,567-604` | O(input) per call; frame header validated before decode (`:616-643`) | encode workspace 2 MiB **eager** per owner (`codec.rs:49,174-188`); decode arena 1 MiB lazy per reader (`:51,411-436`) | none (in-process FFI) | ENFORCED (bounds checked before every call).
21. **Rolling-hash signature** | `candidates.rs:57-81` | O(raw) per FULL whole-file insert and per lookup; 8-slot ordered insert O(8²) worst per window | fixed 128 KiB cache | none | STRUCTURAL.
22. **Pooled COPY/INSERT delta build** | `pool/delta.rs:125-225` | seed table build O(base/16) + match loop O(target/16 × slots) with byte budget 128 KiB (`:227-235`); BLAKE3 per 16-byte seed (`:237-240`) | table 4096×4×u32 = 64 KiB **allocated per trial** (`:142`) | none | ENFORCED (budget; `PROGRAM_LIMIT` 64 KiB `:18`).
23. **Cleanup (baseline-scoped deletes)** | `sqlite/cleanup.rs:35-149` | two paged loops: SELECT `LIMIT 128` newest-first then DELETE `IN (...)` → 2 statements per 128 rows; O(owned/128) pages; bounded transactions re-BEGIN at 8191 rows / 4 MiB−1 (`:87-94,142-146`) | page of 128 rows + placeholder strings | 2 statements per page × both passes; metadata rows removed by FK `ON DELETE CASCADE` (`schema.sql:42`) | ENFORCED (page budget 1,000,000 `:82-84,137-139`).
24. **Same-save read** | `store.rs:341-403` | pending scan O(w×ids) + remaining filter O(ids×pending) (`:351-363`), merge pass O(ids) (`:387-397`) | copies of pending canonical bytes | seals holding groups, then in-transaction read with `i64::MAX` ceiling (`owner.rs:313-322`) | STRUCTURAL (bounded by 512 × 4096).

## 2. Opportunity register

Format: id | cost today | proposal | class | risk.

- **O1 Whole-BLOB pack rewrite on append** | every append re-assembles the open tail and rewrites the full BLOB (`placement.rs:150-154`, `write.rs:88-97`); O(k²·B) bytes shipped per pack with k appends; receipt `sa-adapters-limits.md:504` | append-only or chunked BLOBs: SQLite incremental blob I/O (`sqlite3_blob_write`) writing at a growing offset, or `object_packs(pack_id, chunk_index, data)` rows | **REDUNDANT** (mechanism exists) | **FORMAT CHANGE**: chunked rows alter `sqlite_master` text — the schema identity check pins table shapes (`schema.rs:22-58`, `schema.sql`), and the objects FK references `object_packs(pack_id)` (`schema.sql:60`); incremental blob I/O keeps the schema text identical but requires pre-sizing the BLOB (zeroblob) or an UPDATE anyway when the pack grows past its reservation; pack bytes themselves would be unchanged. Also note the in-memory `assemble` still builds the whole pack buffer, so memory would not improve without further work.
- **O2 Membership re-hash on every reuse offer** | `stored_canonical` re-hashes the full canonical bytes (`membership.rs:20`) that the resolver just authenticated per record (`delta/read.rs:176-178`) | trust the resolver's per-record authentication and keep only the length check (`membership.rs:38`) + byte compare (`:42`): if `object.id() == location.object_id` (`:35`) and bytes compare equal, identity holds transitively | **REDUNDANT** (small: one BLAKE3 over ≤16 MiB per reuse occurrence; the dominant cost is the chain read itself) | none to persisted bytes; weakens defense-in-depth if resolver authentication ever changes.
- **O3 Per-object dependency presence queries** | `Availability::validate` issues one paged `present` per object with unresolved refs (`dependencies.rs:58`), called per object from `offer` (`owner.rs:357`) | collect all unresolved references of the wave before the offer loop and issue one wave-level paged presence query | **REDUNDANT** (O(n) → O(refs/128) statements per wave) | none; ordering of error reporting changes slightly.
- **O4 Connection per read wave** | fresh open + ~5-6 pragma round trips per `read_batch`/`contains`/`begin_save` (`store.rs:187,209,247`, `connection.rs:29-45`) | hold one connection in `Store` behind the existing `Mutex` pattern (`store.rs:104`), re-`configure` on acquire | **REDUNDANT** | none to persisted bytes; changes failure timing (an open can now fail at Store construction instead of per call); `prepare_cached` statement caches would persist (behavioral, not format).
- **O5 Per-record group decompression** | the ordinary resolver decompresses the same zstd group body once per record (`decode.rs:48-50` called per `decode_at`, `delta/read.rs:223`); k records in one group cost k full decompressions; `PoolReader` already caches decoded groups (`pool/read.rs:131`) | cache the decoded group body per (pack_id, group_number) in the wave, bounded like the pooled cache | **REDUNDANT** | none; adds a bounded cache (≤64 KiB per entry, `policy.rs:45`).
- **O6 Whole pack BLOB materialized before width check** | `pack_bytes` reads the whole BLOB with no length check (`lookup.rs:152-163`); the pack-limit check happens afterwards in `parse_header` (`layout.rs:287-289`); receipt `sa-c2-storage.md:480,509` (F3) | check `length(data)` (and `substr(data,1,16)` for the header) before selecting the body | **REDUNDANT** for corrupt/foreign rows, **IRREDUCIBLE** for product-written rows (the body is genuinely needed) | none; adds one statement when the check passes.
- **O7 `append_fits` O(g) re-summation** | each placement re-sums all bodies via `assembled_length` (`layout.rs:216-234`) | keep a running total in `OpenPack` (`placement.rs:15-18`) | **REDUNDANT** (O(g²) → O(g) per pack; g ≤256 so ≤65,536 iterations) | none; pure in-memory bookkeeping.
- **O8 Per-edge locator SELECTs in chain walks** | 1 statement per dependency edge (`delta/read.rs:150`, `pool/read.rs:226`) | for a single chain this is **IRREDUCIBLE** (each locator names the next base); across a wave, memoize locators in a wave-level map since bases repeat | **IRREDUCIBLE** per chain / **REDUNDANT** across wave | none.
- **O9 Fresh `PoolReader` per pooled base trial** | each `pool_base` candidate trial allocates a reader with empty pack/value caches (`owner.rs:748`), re-reading the same base pack bodies per trial | share one reader per save beside `pack_cache` | **REDUNDANT** (bounded by trial count × chain bodies) | none.
- **O10 Cold-start catalogue replay** | `retained_start` streams the entire value-group catalogue to recompute the eviction recurrence (`index.rs:224-248`) | persist the retained start (e.g. a policy column) or compute it backward from the tail | **REDUNDANT** in catalogue size n | **FORMAT CHANGE**: a persisted cursor alters the schema text pinned by `schema.rs:22-58`; a backward computation changes eviction semantics (must reproduce the reference recurrence exactly, `index.rs:220-223`).
- **O11 `seal_pending`/`pending_member` scans** | O(ids × lanes × members) per call (`owner.rs:291-301`, `:258-262`), invoked per unresolved reference per object (`dependencies.rs:80`) | one pending-id set per owner | **REDUNDANT** (bounded: 512 × ~16 K members worst case) | none.
- **O12 `DepthCache` wholesale clear** | the whole 4096-entry cache is dropped when full (`select.rs:147-153`) | LRU eviction | **STRUCTURAL** (bound is enforced either way; clear is O(1) amortized but loses all memoization) | determinism tradeoff; no format risk.
- **O13 Same-save read pending scans** | O(ids × w) linear scans (`store.rs:351-363`, `batch.rs:81-86`) | index the pending batch by id | **REDUNDANT** (bounded 4096 × 512) | none.
- **O14 Cleanup SELECT-then-DELETE pairs** | 2 statements per 128-row page (`cleanup.rs:46-73,101-128`) | single `DELETE ... LIMIT` with `RETURNING` | **IRREDUCIBLE** in practice (needs lengths for byte accounting; SQLite compile options unknown) | none if engine supports it; UNKNOWN here.
- **O15 Batched locator pages of 128** | ceil(w/128) statements per wave (`policy.rs:63`) | wider pages (e.g. 512) | **IRREDUCIBLE** as chosen (SQLite parameter limit ~999 default; 128 is conservative) | SQL text is rebuilt per width and cached per width (`lookup.rs:40-45,68`); no format risk.

## 3. Round trips

- **Per read wave** (the C1 provider path, `provider.rs:85-92` → `store.rs:202-233`): 1 connection open + ~5-6 pragma statements (`connection.rs:29-45`) + 1 ceiling SELECT + ceil(w/128) locator pages + per object: d locator SELECTs + per pack 1 whole-BLOB read (cached per wave, ≤4 MiB wholesale) + per record 2 × O(g) directory validations (`record_width` `delta/read.rs:210` then `decode_canonical` `decode.rs:40`) + 1 group decompress per record + 1 re-hash per returned object (`read.rs:91`). Receipt `sa-c1-content.md:608` records that each C1 provider call is a fresh connection + decode workspace; C1 batches mapping reads into waves of 32 (`layerfs-content/src/file/mapping/read.rs:24`), and point reads are 1-ID batches (`layerfs-content/src/object/access.rs:55`), so a point read pays the whole per-wave fixed cost.
- **Per new object (save)**: wave locator page (1/128 of a statement) + presence query when refs unresolved (1 per object, `dependencies.rs:58`) + probe locator + depth-walk SELECTs (cached) + base chain read (d SELECTs + BLOB reads) + 1 FULL zstd compress (`select.rs:229`) + optional 1 prefix compress (`:301`) + group frame + group zstd compress (`assemble.rs:163-184`) + whole-pack assembly + 1 whole-BLOB INSERT/UPDATE (`write.rs:76-97`) + 1 object INSERT (`write.rs:100-123`).
- **Repeated BLOB materializations**: pack BLOB read materializes the whole pack before any width check (`lookup.rs:152-163` → `delta/read.rs:205-211`; receipt `sa-c2-storage.md:480,509`); `pool_base` trials re-materialize base packs per trial via a throwaway reader (`owner.rs:748`); the dependency cache releases wholesale at 4 MiB so a later edge re-reads released bodies (`delta/read.rs:254-258`) — a released body is read again by design (`policy.rs:107-112`).
- **Re-hashes**: membership re-hash duplicates resolver authentication (O2); read-wave re-hash is the wave's own authentication (`read.rs:91`, necessary); value-group digest re-authentication (`value_group.rs:68-76`) is necessary (digest authenticates the decoded body, `pool/read.rs:141`); BLAKE3 fingerprint per value per `find` call (`index.rs:251-256`) could be memoized per leaf (minor).
- **Re-frames**: every append re-frames the whole pack from its retained groups (`placement.rs:150-154`, `assemble.rs:194-205`); whole-file compact records drop 8 framing bytes at assembly and re-derive them at parse (`layout.rs:27`, `record.rs:124-127`) — by design, cited as stable format.
- **Cross-layer trips per object**: C1 finalized-object handoff is in-process (`store.rs:539-549`); the read direction is the only cross-layer trip: C1 demand → provider → wave (fresh connection each time, O4).

## 4. Honesty notes

- **UNKNOWN — SQLite query plans**: index *existence* is verified from `schema.sql:70-72`
  (`objects_locations` UNIQUE (pack_id, group_number, record_number), `objects_bases` partial
  on base_object_id) and `objects` is `WITHOUT ROWID` keyed by object_id (`schema.sql:56-67`),
  so locator/presence queries and cleanup ordering *should* seek; no EXPLAIN was run (static
  reading only), so actual plans and statement-cache hit rates are UNKNOWN.
- **UNKNOWN — pack id interleaving**: whether pooled-metadata packs and ordinary packs share
  the id sequence (they do — one `next_pack_id` cursor, `owner.rs:103,192-194,653`) means
  appends to a pooled pack and an ordinary pack alternate ids; the append-rewrite frequency
  per pack depends on workload mix, which is not statically determinable.
- **UNKNOWN — result-vector bytes**: one read wave's returned bytes are count-bounded (4096)
  but not byte-bounded; receipt `sa-c2-storage.md:481` records the same gap. No code ceiling
  exists.
- **Transient cache overshoot**: the 4 MiB dependency pack cache is released wholesale
  *before* inserting the next body (`delta/read.rs:254-258`, `pool/read.rs:188-194`), so a
  single new body — up to a ~16 MiB singleton pack — can exceed the declared 4 MiB retained
  ceiling on its own. The declared bound is "retained before the next insert", not a hard
  peak. Same pattern for the 512 KiB pooled value cache (`pool/read.rs:147-151`).
- **Eager vs lazy workspaces**: the 2 MiB encode workspace is allocated at owner acquisition
  (`codec.rs:49,174-188` via `owner.rs:195`); the 1 MiB decode arena is lazy per reader
  (`codec.rs:402-436`). A save that compresses nothing still pays the 2 MiB.
- **Code-vs-doc contradiction (prior receipt)**: the roadmap bound "Pack INSERT statement |
  Up to 1 MiB of BLOB data" is recorded as deviating from the code, which binds a whole
  singleton pack up to ~16 MiB in one statement (`sa-c2-storage.md:470`, code
  `write.rs:76-85`, `policy.rs:88-90`).
- **`present` ceiling asymmetry**: `Store::contains` and read waves enforce the 4096 demand
  ceiling *before* opening a connection (`store.rs:246-247,259-268`), but the per-object
  presence queries inside a save have no aggregate statement bound beyond wave size
  (`dependencies.rs:58`).
- **Counted pages**: `ReadCounters.pages` counts `ids.len().div_ceil(128)` (`read.rs:56`)
  — a static projection, not a measured statement count.
- **Not checked**: rusqlite/zstd runtime behavior (statement cache capacity, sqlite3_blob
  API availability in the pinned build, zstd version), SQLite page-cache effects, and any
  consumer crate beyond `layerfs-content`'s provider calls — only `layerfs-content`,
  `layerfs-storage` and `layerfs-telemetry` exist under `core/crates/` today.
