# Report G — architectural round trips across core and reference

Research agent, 2026-09-17. Static source reading only: no build, no tests run, no
timings, no measurements. Every claim carries a path:line citation against the
frozen tree; UNKNOWN is written wherever source could not confirm a suspicion.

- HEAD: `5e45897dd9f56e7f563aada031a68070a438fb93` (`git rev-parse HEAD`, exit 0).
- Tree at session start: clean (`git status --porcelain` empty). At report time the
  workspace carries sibling-agent work I did not touch: staged
  `core/docs/architecture/12-attributes.md`, `13-physical-writing.md`, modified
  `core/docs/architecture/README.md`, and this untracked evidence directory. My
  only write is this file.
- Trees in scope: replacement core `core/crates/{layerfs-content,layerfs-storage,
  layerfs-telemetry}` (cited as `core/...`) and v0.1.6 reference `crates/` (cited
  by their `crates/...` paths). Baseline seam map:
  `docs/roadmap/0.1/0.1.7/component-decoupling/stages-1-5-review-20260917T230700Z.md`
  §8.2 (:1422-1443), cited below as "R2-review §8.2".

Cost classes: per-object, per-page, per-wave (one C1 provider call), per-operation,
per-pack, per-edit. Ordering within the register is **my judgment from structure**
(nothing was measured); the judgment and its basis are stated per row.

## 1. Round-trip register

Ordered by expected impact (judgment: frequency × bytes × seam-crossing weight).

| id | tree | operation | path:line (both sides of the trip) | what traverses twice | cost class | elimination proposal | trades | canonical/persistence risk |
|----|------|-----------|-----------------------------------|----------------------|-----------|---------------------|--------|---------------------------|
| RT-01 | core | any C1 read of the filesystem tree: directory listing, inode lookup, symlink read, stat, validation, attribute patch walk | point call `crates`→core: `core/.../filesystem/directory/read.rs:208,300`; `inode/read.rs:51`; `filesystem/read.rs:86,193`; `attributes/patch.rs:199`; `validate.rs:82` → provider `object/access.rs:54-69` (one-ID batch) → `layerfs-storage/src/cas/provider.rs:85-103` → `cas/store.rs:209` `connection::open` + `store.rs:214-216` `DecompressionWorkspace::new()` (1 MiB, `encoding/codec.rs:51` `DECODE_WORKSPACE_BYTES`); profile applied per open at `sqlite/connection.rs:29-45` (5 PRAGMA statements) | one provider call **per tree page**, each carrying a fresh SQLite connection, PRAGMA profile, publication-ceiling query (`sqlite/schema.rs` via `store.rs:213`) and a 1 MiB decode workspace | per-page, per-operation | level-batch these walks exactly as `attributes/read.rs:52-68` and `file/mapping/read.rs:257-269` already do (collect next-level ids, one `read_canonical_batch`); independently, pool/reuse one read connection per Store or per operation | churn in five read modules; a pooled connection must re-verify its profile (`connection.rs:54-68`) and re-capture the ceiling per wave (visibility semantics, see §3 P1) | none to canonical bytes; ceiling capture timing is a visibility contract, not a format |
| RT-02 | core | chunked edit over a chunked base (`apply_edits` → `replace_chunked`) | compare pass reads the replaced base range: `file/edit/compare.rs:56-63` → `view.rs:112-114` → `file/mapping/read.rs:259-260` (navigation wave reads the leaf covering the edit); then the edit route re-reads the **same stored leaf** via split: `file/edit/apply.rs:236-241` → `edit/tree.rs:574` → `tree.rs:159` `reader.read_canonical(summary.id)`; `rightmost_payload` re-walks the left subtree: `apply.rs:257-261,328-353` (stored pages point-read again) | the ExtentLeaf mapping page(s) containing the edit span are demanded, decoded and discarded twice — once by the no-op comparison's navigation, once by the split (verified independently in `stage-5-terminal-20260918T120000Z/verify-R2-F19-F20-F22-F26.md:204-208,443-450`: "one non-root identity — an `ExtentLeaf` mapping page — is demanded twice"); each `load_node` is a one-ID provider call (RT-01's per-call connection cost applies) | per-page, per-edit | (a) let the compare pass publish the decoded leaf pages it navigated and let `EditObjects` adopt them; or (b) batch-prefetch the leaf path covering the declared edit span before the split loop; or (c) key `EditObjects.drafts` (`tree.rs:112`) by stored id too, so a stored page read once in the operation is never re-read | (a)/(b) add bounded retained state to the edit route; (c) grows the deferred-node charge (`tree.rs:332-347`) — all bounded by pages the edit touches | none: the split validates the summary against the decoded node (`tree.rs:163-168`) either way |
| RT-03 | core | save: every group appended to a lane's open pack | retained tail assembled and re-written: `pack/placement.rs:66-132` keeps `OpenPack.groups` (:14-18); every append assembles the **whole pack** (`placement.rs:150-154` → `pack/assemble.rs:194-205` header+directory+all bodies) and `cas/owner.rs:830-842` `write_pack` calls `sqlite/write.rs:88-97` `UPDATE object_packs SET data = ?2` — the full BLOB, ≤ `PACK_LIMIT` 256 KiB (`storage/src/policy.rs:55`) | bytes already written by earlier appends of the same pack are re-assembled in memory and re-written through SQLite on every subsequent append (no read-back — the tail is retained; the trip is the rewrite) | per-pack, per-sealed-group | assemble each pack exactly once: buffer sealed groups per lane and write the pack only when it is full or at `finish` (placement already distinguishes closing: `placement.rs:134-161`) | same-save dependency reads need a locator **at seal time** (`owner.rs:278-310` `seal_pending`, `cas/save.rs:60-68`) — deferring the pack write breaks "a member of an unfinished group has no row yet" and requires serving pending members from memory, which the owner deliberately refuses as unbounded (`owner.rs:280-288`); larger `GROUP_TARGET` reduces append count instead, trading pack density | pack bytes are format-frozen (C-SQL verdict, R2-review §8.3 :1452); row shape change is a schema change, not allowed casually |
| RT-04 | core | save: exact-reuse of an already-stored identity (`flush_batch` → `reuse_or_collide`) | `cas/save.rs:55-59` → `cas/membership.rs:30-46` → `:41` `stored_canonical` → `:19` `owner.resolve_location` (`cas/owner.rs:325-339`) reconstructs the full stored chain (pack read + decompress + delta apply, hashed inside the walk at `encoding/delta/read.rs:176`), then `membership.rs:20` hashes the same canonical **again** (`ObjectId::for_bytes`), then `:42` compares full bytes | the stored object's bytes are reconstructed, hashed twice and byte-compared on every reuse offer, even though the offered object's identity was already computed when it was finalized and a length equality was already checked (`membership.rs:38`) | per-object (per reused occurrence) | trust the content address when `canonical_length` matches: skip reconstruction (or at least skip the second hash and the byte compare); the module's own doc states the invariant being traded (`membership.rs:1-6` "Membership says an identity is present; it never proves equality") | drops active collision detection — this is a stated product invariant, so the cut is an owner policy decision, not a refactor | none to persistence; weakens the collision proof to the digest's own strength |
| RT-05 | core | every C1 read wave / every `Store::contains` | `cas/store.rs:209` `connection::open(&self.path, false)` + `:213` `retained_pack_ceiling` + `:214-216` fresh `DecompressionWorkspace` (1 MiB); `store.rs:247` same for `contains`; profile statements per open at `sqlite/connection.rs:29-45` | a fresh connection + workspace per wave — the seam map already records it (R2-review §8.2 :1429 "each call opens a connection"); combined with RT-01/RT-02 the cost is per page, not just per wave | per-wave (per C1 provider call) | one open read connection per `Store` (Arc<Mutex>) or per operation, with profile re-verified on checkout and the ceiling still captured per wave | `Store` is `Clone + Send + Sync` shared state (`store.rs:94-105`); a shared read connection serializes concurrent readers (today a reader runs beside a save on a second connection, R2-review §8.3 :1449); the ceiling **must** stay per-wave or visibility changes | none to bytes; visibility (unpublished-pack hiding, `cas/read.rs:49-67`) must be preserved |
| RT-06 | core | every dependency-chain step in a read or base acquisition | `encoding/delta/read.rs:205-211` `record_width` (parses pack header + group directory, `:282-297` → `pack/layout.rs:246,313`) charges the budget, then `:219-223` `decode_canonical` re-parses the same header and directory (`encoding/decode.rs:39-40`); `pack_of` map lookups at `:206` and `:219` | the pack header + group directory of the same pack are parsed twice per chain step (the second `pack_of` is a cache hit — no re-read, but the parse is repeated) | per-record (per chain step) | have `record_width` return the parsed `GroupView` and pass it into `decode_canonical` (or decode first and charge from the record) | small API change in two files; no behavioral trade | none |
| RT-07 | core | dependency reads / base acquisition; pooled-leaf reads | wave-local pack cache: `cas/read.rs:69` fresh `BTreeMap` per wave, wholesale clear past 4 MiB (`delta/read.rs:246-266`, `:255-257`; `policy.rs:113`); pooled lane creates a **fresh `PoolReader` per pooled leaf** (`delta/read.rs:121`, `cas/owner.rs:748`), so its pack and value caches (`encoding/pool/read.rs:33-40,131-153,186-199`) never survive across leaves of one wave | pack bodies (up to 256 KiB each) re-read from SQLite when the 4 MiB cache clears mid-chain, when a new wave starts, or when the next pooled leaf rebuilds its own reader; value groups re-decoded per leaf | per-pack, per-wave, per-leaf | share one `PoolReader` (and one pack cache) across a wave the way `read_objects` shares `packs`; raise nothing — the bounds are policy | bounded-memory contract (`DEPENDENCY_PACK_CACHE_BYTES`, `POOLED_VALUE_CACHE_BYTES` `policy.rs:113,121`) is deliberate; a shared reader must keep the wholesale-clear discipline | none: caches are derivations, authentication is per use |
| RT-08 | core | every requested object in a read wave; every reuse offer | the requested object's canonical is hashed at the end of its chain walk (`delta/read.rs:176`) and again by the wave (`cas/read.rs:91`); a reuse offer adds a third (`membership.rs:20`, RT-04) | the same final canonical bytes hashed twice (or thrice) per operation | per-object | hash once and carry the verification result out of the resolver | minor plumbing across `Resolver`/`read_objects`/`membership` | none (authentication strength unchanged; one hash still happens) |
| RT-09 | core | ordering `consolidate()` (called by `touched_serials` and `finish`) | `filesystem/references/runs.rs:418-424`: the **first** source (newest tier, level 0 — `runs.rs:209-210`) is fully copied via `copy_run` (`runs.rs:566-582`: read every row, write every row) before any merge runs; stated rationale "so a merge never aliases its own input" (`runs.rs:565`) does not hold — `merge_runs` writes a fresh handle (`merge.rs:211` `backing.create_run()`, `backing.rs:244-267` `create_new`) and only borrows both inputs (`merge.rs:213-214`) | one full run (all its rows) is read and re-written once per consolidate, on top of the merges | per-consolidation, proportional to the newest run | move the first source into `combined` directly (`None => Some(run)`) and adjust `merge_input_bytes` accounting (`runs.rs:394-397,433`) so the accumulator's bytes stay counted | accounting-only change; the reserved-bytes ledger (`runs.rs:143-164`) must still cover the accumulator while it is an input to the next merge | none: rows are copied verbatim today; not copying them preserves bytes exactly |
| RT-10 | core | ordering spill cascade and consolidate merges (by design) | each merge re-reads both full inputs and writes a third (`merge.rs:200-260`, counters at `:213-217`); a spilled batch participates in ≤ log2(batches) merges (`runs.rs:11-12`, cascade loop `:265-289`); `consolidate` re-reads every live run again (`runs.rs:391-434`) | every run's rows are re-read and re-written once per merge it participates in | per-spill, per-merge (logarithmic per batch) | none proposed — this is the bounded-memory design; recorded so it is not rediscovered as a defect | raising `merge_buffer` only shifts the constant | none |
| RT-11 | core | ordering `find()` with non-ascending serial requests | `runs.rs:341-343`: a request below the tier cursor restarts the scan from offset 0 (`scan.reader.start(run, 0)`), re-reading every row from the front; ascending sweeps are amortized by design (`runs.rs:529-545`) | run rows between offset 0 and the target re-read per backward lookup | per-lookup (worst case O(run)) | binary search over sorted rows via `read_at` (`backing.rs:41,328-333` supports O(1) seek) for restarts | per-lookup O(log n) random reads vs one buffered pass; only wins for random-order request patterns — the reducer's demands are documented as ascending (`runs.rs:531-538`) | none |
| RT-12 | core | save: reference availability check per offered object | `cas/owner.rs:357` `availability.validate` → `cas/dependencies.rs:47-70` issues one paged `lookup::present` (128 ids/statement, `sqlite/lookup.rs:126-149`, `policy.rs:63`) per object whose references are unresolved; plus one point `lookup::locations` after a seal (`cas/save.rs:62`) | one SQL execution per offered object with references (the wave-level membership query at `save.rs:26-27` is already batched) | per-object (with references) | batch availability across the wave's offered objects — collect all unresolved references first, one `present` page set | reorders validation before admission decisions; `Availability` is already wave-scoped (`dependencies.rs:24-31`) | none |
| RT-R1 | reference | inode-table update in the sorted-tree engine | per-record encode→store→reread: `crates/layerfs-content/src/tree/batch.rs:1053-1063` `CompactInodes::leaf_value` — a branch entry's record is read back via `store.with_authenticated_canonical(id, decode_inode_record)`; the route `inode_table_apply_sorted*` (`batch.rs:1415-1460`) walks pages through `apply_budgeted` (`batch.rs:785`); the buffer re-hashes spilled bytes on read-back (`crates/layerfs-layerstack-store/src/objects.rs:3616-3635`, hash at `:3628`) | each inode record encoded into an object (hashed at `put_owned`) is read back, re-authenticated and decoded to be inlined into the leaf — the exact "encode into a temporary node object → store it → order by object id → read it back → decode → inline into the leaf" route the round-2 review recorded as BEFORE and confirmed cut in the core (R2-review :1030-1041); **the reference still has it** | per-inode-record, per-page-touched | the core's replacement: typed final values in the reducer row, one bounded ordering, shared leaf grammar (`core/.../references/runs.rs`, `object/inode_leaf.rs` — per R2-review :1036-1040) | core paid a format change (shared leaf grammar) to remove the objects entirely | reference-only; core is already clean |
| RT-R2 | reference | candidate delivery when construction spilled to disk | spool write: `crates/layerfs-layerstack-store/src/objects/spill.rs:6-20` `SpillObjects{writer,reader}` — objects + 144-byte hint frames written to a temp file; read-back: `spill.rs:548-616` `visit_ordered` seeks per frame and reads hints + canonical; delivery re-authenticates every object: `objects.rs:2931-2942` (`:2936` `AuthenticatedCanonicalObject::new(..., Some(id))` → `objects.rs:225-242`, `authenticate_identity` at `:233` re-hashes) after the encode-side hash (`objects.rs:807-813` counts `canonical_hash_calls`) | every spilled candidate's bytes make a full disk round trip (write then read-back) and are hashed twice (encode, delivery) — the write-then-read-back is the deferred-object design's declared cost (bounded memory), the second hash is not free either | per-object (when spilled) | none proposed for the spool itself (it is the memory bound); the delivery re-hash could trust the encode-side identity the frame already carries | dropping the delivery re-hash weakens the same collision-check invariant as RT-04 | none to persistence |
| RT-R3 | reference | workspace commit pulling frozen input from the sandbox spool | `crates/layerfs-workspace/src/snapshot_input.rs:174-189` `fetch` (one `SNAP_READ` wire request per bounded window) and `:193-230` `read` — commit streams back the bytes FUSE wrote into the sandbox backing, window by window, with a shared bounded window cache (`:200-229`) | the sandbox's own writes are read back over the transport during Commit — the #151 B2 pattern documented in `AGENTS.md` §5 | per-segment-window, per-commit | bounded-window fetch already exists; a pre-pull of referenced ranges is already done ("every piece in it is served by the same fetch", `snapshot_input.rs:203-207`) | transport traffic vs commit latency | none |
| RT-R4 | reference | "per-file flushes" (seed item) | **UNKNOWN / not confirmed**: construction flushes per bounded slab, not per file (`objects.rs:824-828`, slab bounds; `:772-805` `flush`); the FUSE `flush` handler is a gate no-op (`crates/layerfs-fuse/src/filesystem.rs:855-894`); I did not find a per-file flush in the paths examined (objects.rs, spill.rs, file_io.rs, filesystem.rs) — see §4 | — | — | — | — |

## 2. Seam map delta (baseline R2-review §8.2 :1422-1443)

Already batched (no per-item round trip at the seam today):

- producer → C2 `accept`: per-object crossing but wave-flushed at 512 objects /
  512 KiB (`cas/batch.rs:48-69`, `policy.rs:65 BATCH_OBJECT_LIMIT`); the seam map
  row (:1426) is unchanged and accurate.
- C1 ← provider, file-content mapping navigation: level-batched,
  `READ_NAVIGATION_WAVE = 32` pages per wave (`core/.../file/mapping/read.rs:40,
  257-269`, commit `ab17a6958` "batch mapping navigation per level").
- C1 ← provider, payload waves: 32 distinct payloads per grouped call
  (`mapping/read.rs:24, 102-118`), borrowed not cloned per demand (:166-185).
- C1 attribute reads: level-batched (`attributes/read.rs:52-68`).
- C1 sorted-tree update fetch: batched (`sorted/page.rs:241` `read_batch`,
  `update.rs:317-341` `base_read_batch`).
- C2 membership lookups per save wave: one paged query per wave
  (`cas/save.rs:26-27`, `sqlite/lookup.rs:52-85`, 128 ids/statement).
- Pooled-value index lookups: batched per leaf ("Asking per row cost one point
  query per row; the index is a batch API", `cas/owner.rs:473-502`).
- Save-side pack reads through one owner connection (`owner.rs:249-251,313-322`).

Still a round trip per item / per call:

- **C2 → C1 provider seam: each call opens a connection** (R2-review §8.2 :1429 —
  unchanged; RT-05). Under `StoreProvider` every C1 wave pays connection + PRAGMA
  profile + ceiling + 1 MiB workspace (`store.rs:209-216`).
- **Filesystem tree navigation reads one page per point call** (RT-01):
  `directory/read.rs:208,300`, `inode/read.rs:51`, `filesystem/read.rs:86,193`,
  `attributes/patch.rs:199`, `validate.rs:82`. The seam map's recipe claim "there
  is no per-node RPC left in C1 (the mapping navigation was batched in
  `ab17a6958`)" (R2-review §8.4 :1489-1491) is accurate for **file-content
  mapping** navigation only; directory/inode/attribute-patch/validate walks were
  not batched.
- **Edit-tree stored-node loads are one-ID provider calls** (RT-02):
  `edit/tree.rs:159` via `object/access.rs:54-69`.
- **Per offered object with references**: one availability `present` query
  (RT-12, `owner.rs:357` → `dependencies.rs:58`).
- **Per reuse offer**: full chain reconstruction + double hash + byte compare
  (RT-04, `membership.rs:19-42`).
- **Per dependency edge**: one `lookup::location` point query
  (`delta/read.rs:150`) — sequential by construction (base of base), so batching
  is limited to sibling edges, which the wave does not collect today.
- **Per pooled leaf**: fresh `PoolReader` (RT-07, `delta/read.rs:121`).

Telemetry (`layerfs-telemetry`): no per-item round trip found — recording is a
bounded arena (`timer/recording.rs`, 1,024 nodes / 32 levels per the seam map
:1432), and the per-accept node the seam map's store.rs comment describes was
already removed (`cas/store.rs:283-290` documents the fix). Not exhaustively
audited (§4).

## 3. Elimination proposals (top trips, concrete sketches)

**P1 — read-session provider (RT-05, enables RT-01/RT-02 relief).** What changes:
`Store` gains an explicit read session (or an internal `Arc<Mutex<Connection>>`
reader pool) so a C1 operation opens at most one read connection per operation
instead of one per wave; `StoreProvider` borrows the session. What stays
byte-identical: all bytes and all queries; `verify_profile`
(`sqlite/connection.rs:54-68`) runs on checkout, and **the publication ceiling is
still captured per wave** (`store.rs:210-213`, `cas/read.rs:49-67` — the ceiling
is the watermark of the last completed save and may move between waves; caching it
per operation would change what an in-flight save's early commits can leak).
Hard parts: `Store` is `Clone`/shared (`store.rs:94-105`) and today a concurrent
reader beside a save uses a second connection safely (R2-review §8.3 :1449) — a
single shared reader serializes them; no-retry semantics forbid reconnect-on-error
as silent recovery (a lost pooled connection must surface, not reopen).

**P2 — level-batch the filesystem tree reads (RT-01).** What changes: mirror
`attributes/read.rs:52-68` in `directory/read.rs` (`list_after`'s stack walk),
`inode/read.rs` (the per-level loop), `attributes/patch.rs:199` (pending-page
walk) and `validate.rs` — collect the next level's page ids, one
`read_canonical_batch` per level. What stays byte-identical: every page's bytes
and every decode check; only the demand shape changes. Hard parts: the walks are
cursor-driven (continuations, `directory/read.rs:188-200`) so a level's demand
set depends on the previous level's decode — same shape as the mapping frontier
(`mapping/read.rs:235-325`) which already solved it; files stay well under the
999-line ceiling.

**P3 — one read of the edit-span leaf (RT-02).** What changes: either the compare
pass hands its decoded leaf pages to `replace_chunked`, or `EditObjects` prefetches
the leaf path covering the declared span in one batch before the split loop, or
`drafts` (`tree.rs:112`) admits stored pages read this operation (keyed by real
id, charged against `EDIT_DEFERRED_LIMIT` `tree.rs:31,332-347`). What stays
byte-identical: the split still validates the node against its summary
(`tree.rs:163-168`); nothing published changes. Hard parts: the compare pass
currently owns its navigation through `mapping::read_range`'s wave (whose pages
are released per wave, `mapping/read.rs:1-10`), so handing pages over crosses a
module boundary deliberately built to release them; the draft-charge route must
not let a large edit pin the whole mapping (bound by pages the span touches).

**P4 — assemble each pack once (RT-03).** What changes: seal groups into the
lane's retained tail as today, but write the pack BLOB exactly once — when the
pack is full (closing, `placement.rs:87`) or at `finish` — instead of on every
append. What stays byte-identical: the pack bytes and the group/record ordinals
(`placement.rs:1-7` "existing group/record ordinals never move"). Hard parts:
`seal_pending` (`owner.rs:278-310`) exists precisely because a same-save dependent
needs a **row and readable bytes at seal time**; deferring the BLOB write forces
pending members to be served from retained memory, which the owner documents as
unbounded for compressible records (`owner.rs:280-288`). The narrower cut —
raising `GROUP_TARGET` or sealing only at transaction boundaries — trades pack
density and transaction size. Schema/BLOB shape itself is frozen by the C-SQL
verdict (R2-review §8.3 :1452 "a pack append rewriting the whole `BLOB`" is listed
as a core blocker for remote SQL, i.e. a known accepted cost).

**P5 — drop the redundant copy in consolidate (RT-09).** What changes: `None =>
Some(run)` instead of `copy_run` (`runs.rs:418-424`), with `merge_input_bytes`
(`runs.rs:394-397,433`) adjusted so the accumulator's bytes stay charged while it
feeds the next merge. What stays byte-identical: rows are byte-copies today; not
copying preserves them exactly, and `merge_runs` never writes into an input
(`merge.rs:211-214`). Hard parts: only the owned-bytes ledger (`runs.rs:143-164`)
must stay honest — the reserve before each output already exists
(`runs.rs:407-411`).

**P6 — hash once per resolution (RT-08, part of RT-04).** What changes:
`Resolver::resolve_charged` returns the verified identity alongside the canonical
(`delta/read.rs:163-182`) so `cas/read.rs:91` and `membership.rs:20` do not re-hash.
What stays byte-identical: one authentication hash still happens per object per
operation. Hard parts: none structural; it is plumbing, but it touches the
authentication pattern three call sites share.

## 4. Honesty notes — unknowns and unconfirmed suspicions

- **Per-file flushes in the reference (RT-R4): UNKNOWN.** The seed asked for it; I
  could not confirm one. Construction flushes per slab (`objects.rs:824-828`),
  the FUSE `flush` is a no-op gate (`crates/layerfs-fuse/src/filesystem.rs:855-894`),
  and I did not audit `layerfs-daemon`, `layerfs-monitor`, `layerfs-sdk`,
  `layerfs-materialization` or the remainder of the 7,099-line `objects.rs` and
  18,762-line workspace crate line-by-line.
- **Reference crate coverage is partial.** I verified the inode route (RT-R1), the
  candidate spool read-back (RT-R2) and the commit-time sandbox pull (RT-R3) in
  source; the reference's read side (`objects/read.rs`, `read/`), delta selection
  (`objects/delta.rs`) and pack writing (`objects/pack.rs`) were surveyed by grep
  only, not read completely.
- **Impact ordering is judgment, not measurement.** No benchmark, counter or timer
  was run (per the task's hard constraints). Frequency assumptions (e.g. "reuse
  offers are common") come from the code's own counters and docs
  (`owner.rs:264-267`, `membership.rs:1-6`), not from runs.
- **`layerfs-telemetry` was not exhaustively audited** — no per-item trip was
  found in the recording/report path, but I did not read all 1,059 lines of the
  timer implementation.
- **Whether `filesystem/objects.rs:72-79` point reads are hot** in the C1
  construction route: the boundary offers both point `read` and wave `read_batch`
  (`objects.rs:71-90`); I did not trace every caller to classify which dominate.
- **The staged sibling files** in `core/docs/architecture/` at report time are not
  mine; I neither read nor relied on them, and every citation above is against
  committed source at `5e45897d`.
- **SQLite-side effects (statement cache, page cache) are engine behavior**; this
  report cites only what the product source does (`prepare_cached` at
  `sqlite/lookup.rs:68,140` is per-connection, so a per-wave connection also pays
  statement re-preparation — inferred from `connection.rs:17-26` opening a new
  `Connection` per wave; not measured).
