# Report A — C1 file-content pipeline: complexity inventory (read-only source analysis)

- Tree: working tree clean of tracked modifications (`git status --porcelain` shows only this untracked
  evidence directory). Source read at `5e45897dd9f56e7f563aada031a68070a438fb93`; three docs-only
  commits (`ce2d738ff`, `2c82bf204`, `2d93edf0e`, all under `core/docs/` and `docs/roadmap/`) moved HEAD
  to `2d93edf0e4cebbcf2ea7eac5a3f069e44244e3ce` during the pass — `git diff 5e45897 2d93edf --
  core/crates/layerfs-content/` is empty, so every citation below is valid at both commits.
- Method: static reading of `core/crates/layerfs-content/src/file/` and `src/object/` plus `src/policy.rs`.
  No builds, no tests, no measurements, no performance claims. Measured facts are cited only from
  existing receipts under `docs/roadmap/0.1/0.1.7/evidence/`.
- Variables: **n** = logical file bytes; **c** = chunk count (c ≈ n/avg_chunk, avg in 8–32 KiB, target
  16 KiB); **h** = mapping-tree height (h ≤ 32, enforced, `types.rs:17`); **p** = mapping pages visited;
  **P** = mapping pages an edit publishes; **E** = edits per operation (E ≤ 4096, enforced,
  `edit/input.rs:22`); **R** = plan segments (retain/replace); **W** = compare windows.
- Bound classes: **[E]** enforced (a declared constant refuses beyond it), **[S]** structural (the
  algorithm's shape), **[U]** unknown from source.
- Canonical-compatibility test set (named once, referenced as "the parity set"): `tests/edit_reference.rs`
  (nine sealed oracle cases: root, page partition, surviving leaf identities; header
  `edit_reference.rs:1-19`), `tests/fixture_seal.rs` + `tests/fixtures/filesystem.seal` (sealed fixture
  digests), `tests/object_identity.rs` (pinned identity hex, e.g. `DOMAIN_PROBE_ID`
  `object_identity.rs:20,55-56`), `tests/filesystem_reference.rs`.

## 1. Algorithm inventory

### Chunking and object codecs

| Algorithm | path:line | Time | Peak owned memory | I/O trips | Class |
| --- | --- | --- | --- | --- | --- |
| CDC scan + rolling GEAR hash | `cdc/gear.rs:105-123` (`scan_region`), `gear.rs:134-211` (`consume`) | O(n) per file: every byte once through the two-byte GEAR step; O(chunk ≤ 32 KiB) per chunk; hash incremental within a chunk, reset per chunk (`gear.rs:266` `self.hash = PROFILE_SEED`) | 32 KiB stack read buffer (`gear.rs:81` `[0_u8; MAXIMUM_CHUNK_BYTES]`) + 32 KiB owned chunk buffer (`gear.rs:128` `Vec::with_capacity(MAXIMUM_CHUNK_BYTES)`) | none (source is a caller `Read`); no provider calls | O(n)/file **[S]**; chunk caps 8/16/32 KiB **[E]** (`gear.rs:13,15,17`) |
| Chunk encode | `mapping/codec.rs:56-84` | O(chunk), payload copied exactly once into one allocation (`codec.rs:80-81`) | one canonical Vec ≤ 32 KiB + 53 B (`chunk_canonical_len`, `codec.rs:343-345`) | none | **[E]** payload > 32 KiB refused (`codec.rs:57-62`) |
| Whole-file encode | `file/content.rs:82-111` | O(n): value copy + canonical copy | **~2n + 23 B** (value Vec `content.rs:98-101`, canonical Vec `content.rs:102`); the single-allocation cut is explicitly NOT implemented (`content.rs:86-90`) | none | **[E]** raw ≤ cutoff−1 (`content.rs:91-97`), canonical ≤ raw+23 (`policy.rs:155,196`) |
| Canonical envelope encode | `object/codec.rs:50-75` | O(value), one pass into caller-sized buffer (`codec.rs:71-73`) | fresh-alloc form (`codec.rs:70-75`) holds value + canonical transiently | none | **[E]** 16 MiB object / 8 MiB field (`codec.rs:26-44`, `policy.rs:203,209`) |
| Canonical envelope decode | `object/codec.rs:81-127` | O(1) header/length checks; value borrowed, never copied (`codec.rs:126`) | none | none | **[E]** same ceilings (`codec.rs:82-110`) |
| Identity hashing | `object/id.rs:24-29` | O(canonical) BLAKE3 over frozen 17-byte domain + canonical bytes | 32-byte digest | none | **[S]**; domain frozen (`id.rs:16`) |
| Object finalization | `object/output.rs:98-109` | O(1) framing re-check (`codec::decode_bytes_object`) + one O(canonical) BLAKE3 | the canonical Vec itself | none | **[S]** |

### Mapping tree — build and read

| Algorithm | path:line | Time | Peak owned memory | I/O trips | Class |
| --- | --- | --- | --- | --- | --- |
| Streaming page build | `mapping/build.rs:200-221` (`flush_streaming`), `build.rs:256-299` (`emit_prefix`) | Amortized O(1) per extent: a flush removes `MAX_ENTRIES` and pushes 1 summary up (`build.rs:209-210`); O(c) per file | boundary levels only: level-0 ≤ 193 entries ×40 B ≈ 7.7 KiB (`build.rs:75-77`, `flush_at` = 192 = `MAX_MAPPING_ENTRIES+64`, `policy.rs:159,212,214`); each higher level ≤ 129 ×56 B (`build.rs:307`) | 1 consumer accept per page | page caps 64/128 entries **[E]** (`types.rs:13,15`, validate `types.rs:173`); node ≤ 8 KiB **[E]** (`codec.rs:121-126`, `types.rs:23`) |
| Node encode | `mapping/codec.rs:107-163` | O(entries): validate pass + value Vec + canonical Vec (2 allocations, `codec.rs:127,160`) | 2 transient buffers ≤ ~6.2 KiB each | none | **[E]** (as above) |
| Node decode | `mapping/codec.rs:171-270` | O(entries) ×2 passes: entry Vec build (`codec.rs:221-245`) then `validate` (`codec.rs:268`) | one entries Vec ≤ ~6.2 KiB | none | **[E]** (as above) |
| Multi-level finish | `mapping/build.rs:223-254` | O(c): half-partition of leftovers (`build.rs:244` `len / 2`); height = log₁₂₈(c) | as above | 1 accept per page | h ≤ 31 **[E]** (`build.rs:215-217`, `types.rs:17`) |
| Tree build (entry) | `mapping/build.rs:319-332`; chunked construction `file/content.rs:240-266` | O(n) scan + O(c) pages + O(c/128^h) upper pages | scanner 64 KiB + builder levels ≈ 7.7 KiB × (h+1) + one chunk canonical in flight — **independent of n** | 0 provider reads; 1 consumer accept per object (chunk, page, file state) | **[S]** |
| Mapping read: navigation | `mapping/read.rs:235-325` | O(p·entries) decode+validate per visited page; frontier per level | ≤ 32 pages × 8 KiB = 256 KiB per wave (`read.rs:40` `READ_NAVIGATION_WAVE`) + frontier entries | **ceil(p_l/32) provider calls per level** (`read.rs:257-260` `level.chunks(READ_NAVIGATION_WAVE)` → one `read_canonical_batch_scoped`) | wave width 32 **[S/E]**; h ≤ 32 **[E]** (`read.rs:251`) |
| Mapping read: payload waves | `mapping/read.rs:86-189` | O(range bytes) sink writes; per demand a linear `position()` over ≤32 distinct ids (`read.rs:167-171`) | ≤ 32 payloads ≤ 1 MiB **[E]** (`read.rs:24,33`; enforced `read.rs:104-106,142-147,151-156`); demands buffer ≤ 128 (`read.rs:114`) | 1 batch call per flush (≤32 distinct payloads) | **[E]** |
| Logical read (whole file, bounded) | `file/read.rs:19-47` | 1 root read + O(n) emission | root canonical + waves | 1 root call + navigation + payload waves | `maximum` refuses (does not stream) **[E]** (`read.rs:31-37`) |
| Logical read (range) | `file/read.rs:60-74`, `read.rs:76-126` | whole-file: zero-copy slice (`read.rs:101-108`); chunked: traversal above | as above | 1 root call (+ traversal for chunked) | **[S]** |

### Edit routes (`file/edit/`)

| Algorithm | path:line | Time | Peak owned memory | I/O trips | Class |
| --- | --- | --- | --- | --- | --- |
| Edit-stream validation + plan | `edit/input.rs:99-139`, `input.rs:306-390` | O(E) validation; O(1) per plan segment, one forward pass | the caller's edit list only | none | E ≤ 4096 **[E]** (`input.rs:22,100-106`) |
| No-op comparison | `edit/compare.rs:31-87` | only same-length replacements read: O(min(first-diff, Σ replaced bytes)); length-changing edits return `Differs` with zero reads (`compare.rs:46-48`) | 2 × 64 KiB windows **[E]** (`compare.rs:19`) | **W × (h nav calls + payload waves)** — one fresh root-down traversal per 64 KiB window (`compare.rs:56-63` → `view.rs:112-114` → `mapping/read.rs:242-248`) | **[S]** |
| Whole-file route: assemble | `edit/apply.rs:135-173` | O(final_len): `out` Vec reserved once (`apply.rs:143-150`) + retained reads | `out` (n) + value (n) + canonical (n) ⇒ **~3n peak** (`apply.rs:143-150` + `content.rs:98-102`) | R × traversal (see R4); replacements via 16 KiB buffer (`apply.rs:187`) | final_len < cutoff **[E]** (`apply.rs:63`, `policy.rs:120-128`) |
| Chunked route: split | `edit/tree.rs:552-635` | O(h·page): descent + prefix/suffix draft rebuild (`tree.rs:627-628`) | drafts charged | h point loads per split (stored pages); see R1/R2 | **[S]** |
| Chunked route: concat/join | `edit/tree.rs:658-805` | O(Δheight + boundary pages); equal-level join loads+merges both sides (`tree.rs:668-690`) | drafts charged | point loads incl. **discarded validation loads** (R1) | **[S]** |
| Chunked route: replacement scan | `edit/apply.rs:271-294` | O(replacement_len) through the same builder/CDC as construction | per-edit builder (7.7 KiB, `apply.rs:275`) + scanner 64 KiB | 1 consumer accept per chunk (immediate, `tree.rs:482-486`) | **[S]** |
| Chunked route: settle + commit | `edit/tree.rs:242-263`, `tree.rs:394-462` | settle: passes ≤ draft depth, O(passes × detached) (`tree.rs:243-248`); commit: O(P·page) encode+hash, children-first, BTreeSet dedupe | drafts ≤ **8 MiB charged [E]** (`tree.rs:31,332-347`; charge model `tree.rs:85-104`) | 1 accept per published node + file state | **[E]/[S]** |
| `rightmost_payload` hint walk | `edit/apply.rs:328-353` | O(h) loads down one path | none | h point loads **per edit**, including deletions (see R3) | **[S]** |
| Inode-leaf codec (canonical object layer) | `object/inode_leaf.rs:117-141,192-225,244-260` | value O(1) fixed 73 B; leaf O(rows) encode+decode (order check `windows(2)`, `inode_leaf.rs:249`) | rows ≤ 100 × 81 B **[E]** (`inode_leaf.rs:34,246-247`) | none | page ≤ 8 KiB **[E]** (`inode_leaf.rs:38,213-218`); pooled value exactly 94 B **[E]** (`inode_leaf.rs:48,156`) |

## 2. Opportunity register

Linear costs are classified **IRREDUCIBLE** (each byte/item must be touched once) or **REDUNDANT**.
Canonical risk names the parity set (§ header). "No canonical risk" = emitted roots/bytes byte-identical.

**O1 — Whole-file encode allocates twice (~2n → ~n).**
Current: value Vec then canonical Vec (`content.rs:98-102`); the code itself documents the missing cut
("the value is built in its own allocation and `encode_bytes_object` allocates the canonical object
around it", `content.rs:86-88`). REDUNDANT (allocation, not byte-touches). Mechanism: write envelope +
`WHOLE_MAGIC` + version + payload into one pre-sized Vec, exactly as `encode_chunk_object` already does
(`codec.rs:71-83`). Trade: none (fewer copies, same bytes). Risk: none — output bytes identical, parity
set unaffected.

**O2 — Whole-file edit route peaks at ~3n (out + value + canonical → ~n).**
Current: `assemble_final` holds `out` (`apply.rs:143-150`) while `encode_whole_file` builds value and
canonical (`apply.rs:95-97` → `content.rs:98-102`). REDUNDANT. Mechanism: assemble directly into the
canonical object's payload region (all widths known: envelope 13 B + value header 10 B, `policy.rs:196`).
Trade: none; bounded by cutoff anyway (≤ 1 MiB). Risk: none. Note: `content.rs` documents only the 2n
figure (`content.rs:77-81`); the 3n edit peak is derivable, not documented there (honesty note H4).

**O3 — Discarded validation loads in `concat_inner` (the seeded double demand).**
Current: the taller-side dismantle loads the boundary child and throws the result away —
`tree.rs:711` `objects.load_node(last, false)?;` (mirror `tree.rs:764`) — then the recursive
`concat_inner(objects, last, right, depth + 1)` (`tree.rs:713`/`765`) loads the **same** summary again at
`tree.rs:668-669` (equal level) or `tree.rs:699/752` (dismantle). `load_node` re-reads stored pages from
the provider on every call (`tree.rs:158-161`: drafts are cloned, stored nodes hit
`reader.read_canonical` — a batch-of-1 call, `access.rs:54-66`); nothing caches stored nodes (grep for
cache in `src/` is empty). When the boundary child is a stored non-root `ExtentLeaf` — the sibling page
adjacent to an edit whose sibling run has ≥2 members (`tree.rs:627` builds a draft branch over stored
siblings; `root_from_children` returns a single stored child unchanged, `tree.rs:831-833`, which is why
the double appears only on one side) — one edit demands that page **twice**. This is exactly the
phenomenon the 2026-09-18 verification pass recorded: "One `ExtentLeaf` mapping page is demanded twice"
(`stage-5-terminal-20260918T120000Z/verify-R2-F19-F20-F22-F26.md:205,365,535`). REDUNDANT. Mechanism:
delete the discarded load — the recursion's own `load_node` performs the identical summary validation
(`tree.rs:163-168`). Trade: none. Risk: none (read path only). Count per edit is 0–2 depending on tree
shape (prefix side and tail side each qualify independently); the receipt measured one.

**O4 — Comparison pass re-demands every mapping page the edit will re-demand.**
Current: `compare_replacements` runs before route dispatch for every non-empty stream (`apply.rs:64-77`)
and traverses root-down per window (R5); a differing chunked edit then re-reads the same root/branch/leaf
pages in the split descent (`apply.rs:236-249`). The base payloads compared are never used again (the
replaced subtree is discarded unread, `apply.rs:252`). REDUNDANT for differing edits; IRREDUCIBLE as the
price of exact no-op detection (it is how equality is proven, `compare.rs:3-9`). Mechanism: fuse
comparison into the split descent (compare at the leaf being split, before emitting halves), or share the
loaded pages between the two passes. Trade: a differing edit that would have stopped at the first
difference now pays partial split work; no-op result must stay bit-identical (return base root,
`apply.rs:71-77`). Risk: none if the Equal verdict is preserved; a changed verdict would change returned
roots and break `tests/edit_noop.rs`.

**O5 — `rightmost_payload` walk paid even by pure deletions.**
Current: the hint walk runs whenever `left` is `Some` (`apply.rs:257-262`) **before** the
`replacement_len == 0` check (`apply.rs:271`); for a deletion the hint is never consumed. O(h) point
loads per deletion edit, REDUNDANT. Mechanism: move the walk inside the `replacement_len > 0` branch.
Trade: none. Risk: none — the predecessor is advisory only (`build.rs:117-119`) and does not enter
canonical bytes or identity (`output.rs:88-94` stores it beside, not inside, the hashed bytes).

**O6 — `assemble_final` re-traverses the mapping once per retained segment (chunked base, small result).**
Current: each `Segment::Retain` issues its own `view.read_range` (`apply.rs:154-157`) → a fresh root-down
traversal (`view.rs:112-114` → `mapping/read.rs:242-248`). R segments ⇒ R × h navigation reads where one
ordered cursor would pay h once. REDUNDANT. Mechanism: since `Plan` yields segments in base order
(`input.rs:291-297`), stream the planned result through one traversal (the shape `PlanReader` already
gives the chunked-result route, `apply.rs:377-465`). Trade: none. Risk: none (assembled bytes identical;
the whole-file root depends only on them).

**O7 — `load_node` clones the entire decoded draft on every load.**
Current: `tree.rs:156` `Some(Draft::Node(node)) => node.clone()` — an O(page ≤ 128 entries) Vec copy per
load; split+concat+commit sequences load the same drafts several times. REDUNDANT (copying, not
algorithm). Mechanism: `Rc<ExtentNode>`/ownership passing, or a load-once memo per operation. Trade: one
pointer of indirection; refcount discipline with `hold_node`'s clone (`tree.rs:179`). Risk: none.

**O8 — Builder-produced branch pages are decoded 2–3×.**
Current: `hold_page` decodes each branch page to retain children (`tree.rs:208`), `commit_node` decodes
it again to publish children (`tree.rs:415`), and `release_draft_children` may decode it a third time
(`tree.rs:273`). REDUNDANT decode passes over the same bytes. Mechanism: store the decoded child-id list
with the `Draft::Page`. Trade: ~48 B × entries retained per held branch (inside the 8 MiB charge).
Risk: none.

**O9 — Compare windows: one full traversal per 64 KiB window.**
Current: `compare.rs:52-63` calls `view.read_range` per window; each call re-demands the root and path
pages (no cache, `mapping/read.rs:242-248`). For a same-length overwrite of L bytes: ceil(L/64 KiB) × h
extra navigation reads. REDUNDANT. Mechanism: one traversal per Replace segment with a comparing sink
(bounded 64 KiB in flight; early stop needs an error channel through the sink). Trade: sink contract
complexity. Risk: none (verdict unchanged).

**O10 — `coalesce` uses `Vec::remove` per merged pair.**
Current: `tree.rs:522-534`; each merge shifts the tail ⇒ O(k²) over ≤ 2×128 = 256 concatenated entries.
REDUNDANT. Mechanism: write-index compaction. Trade: none. Risk: none. Bounded by page caps **[E]**, so
structural in practice.

**O11 — `settle` is pass-wise over the detached set.**
Current: `tree.rs:249-262`; each pass scans the whole set, passes repeat while releases detach children.
O(passes × detached), passes ≤ draft depth (the code's own bound, `tree.rs:243-248`). Structural (bounded
by the 8 MiB charge and depth); a recursive release would make it O(detached). Risk: none.

**O12 — Per-page transient copies in the builder.**
Current: `emit_prefix` drains into a fresh Vec (`build.rs:265,275`) and `encode_node` builds value then
canonical (`codec.rs:127,160`) — ~3 transient copies per sealed page, plus decode's second validate pass
(`codec.rs:268`). REDUNDANT but bounded (≤ 128 entries ≈ 6 KiB); low value. Risk: none.

**O13 — `construct_stream` probe double-buffers the prefix.**
Current: up to `cutoff` bytes are buffered (`content.rs:210-224`), then re-read through
`Cursor::new(prefix).chain(source)` (`content.rs:225-230`) into the scanner's own buffers; a whole-file
outcome adds value+canonical on top ⇒ ~3×cutoff transient (see O1). Mechanism: a stateful peeking reader
that scans the probe bytes without copying. Trade: CDC state-machine complexity; memory saved ≤ 1 MiB.
Risk: none — the partition depends only on the byte stream (frozen profile, `gear.rs:1-6`).

**Not opportunities (IRREDUCIBLE, for the record):** per-byte CDC scan and per-object BLAKE3 identity
(definitionally O(n)/O(canonical), `id.rs:24-29`); per-chunk payload copy into canonical form (object
must own bytes, `codec.rs:80-81`); re-encode+re-hash of pages whose entries changed (identity is the
digest of canonical bytes, `output.rs:98-109`); one provider wave per 32 pages/payloads on reads
(already batched, `read.rs:257-260,124-126`); O(h) split descent loads (sequentially dependent:
parent must decode before child id is known). Deriving page identity without full re-hash would change
roots ⇒ breaks the whole parity set; likewise any CDC or partition change (`gear.rs:29-49`,
`mapping/codec.rs:31-50` digest every parameter into `profile_id`, so even "same semantics" retunes are
detectable).

## 3. Round trips (same bytes/state demanded more than once, or per-item boundary crossings)

- **R1 (the seed, placed precisely).** One non-root `ExtentLeaf` page demanded twice per chunked edit
  when the sibling run beside the edited child has ≥2 members: `tree.rs:711` discarded load +
  `tree.rs:713` → `tree.rs:668` re-load (taller-left), mirrored `tree.rs:764/765` (taller-right).
  Confirmed externally by `verify-R2-F19-F20-F22-F26.md:205,365,535`. See O3.
- **R2.** No stored-node memo in `EditObjects`: every `load_node` of a stored page is a fresh
  batch-of-1 provider call (`tree.rs:158-161`, `access.rs:54-66`); the two splits per edit both descend
  overlapping paths (second split descends the draft tail, but stored children along the way are
  re-read, `apply.rs:236-249`).
- **R3.** `rightmost_payload` walks h stored pages per edit even when its result is unused (deletions):
  `apply.rs:257-262` before the `replacement_len == 0` branch at `apply.rs:271`. See O5.
- **R4.** `assemble_final` (whole-file result over a chunked base) issues one root-down traversal per
  retained segment: `apply.rs:152-157`. See O6.
- **R5.** `compare_replacements` issues one root-down traversal per 64 KiB window: `compare.rs:52-63`.
  See O9. Its base payload reads are also one-way (replaced bytes are re-compared but never reused by
  construction, `apply.rs:252` discards the removed subtree unread).
- **R6.** Compare-then-construct: every mapping page on a replaced range's path is demanded once by
  compare and again by the split descent (`apply.rs:64-77` vs `apply.rs:236-249`). See O4.
- **R7.** Builder branch pages decoded at hold, again at commit, possibly again at release:
  `tree.rs:208`, `tree.rs:415`, `tree.rs:273`. See O8.
- **R8.** Per-object consumer accepts are point crossings (1 `accept` per chunk/page/file state,
  `output.rs:191-193`; `DeferredSink` routes each object individually, `tree.rs:477-490`). Whether the
  consumer (C2 bridge) batches downstream is outside C1 — **[U]**.
- **R9 (non-trip, verified).** The base root is read exactly once per chunked edit operation:
  `FileView::open` (`view.rs:28-44`) acquires and classifies once; `replace_chunked` reuses the decoded
  state (`apply.rs:111-115`, "One read of the base root per edit, not two"). The 2026-09-18 receipt's
  two-tree table confirms 1 demand at frozen HEAD vs 2 pre-fix
  (`verify-R2-F19-F20-F22-F26.md` §3.3, lines ~195-205).

## 4. Honesty notes

- **H1.** `view.rs:6-7` says "an extent walk visits one decoded leaf at a time" — the mapping read is
  wave-based (≤32 pages per navigation wave, ≤32 payloads per payload wave, `mapping/read.rs:24,40`),
  not single-leaf. Doc drift, not a bug.
- **H2.** `hold_page` decodes branch pages with `root=true` (`tree.rs:208`) although streaming flushes
  emit them as non-root pages ("it is never the tree's root page", `build.rs:207-209`). The laxer
  context accepts pages the strict non-root check would reject; harmless here because `emit_node`
  validated them as non-root at encode time (`build.rs:383` → `codec.rs:107,121`), but the flag is
  semantically inverted versus the page's real position.
- **H3.** `gear.rs:5-6` claims "one owned chunk buffer of exactly the maximum chunk size, no
  whole-input retention" — correct about retention, but `FastCdc::scan` additionally holds a 32 KiB
  **stack** read buffer (`gear.rs:81`) the doc omits.
- **H4.** The task brief attributes a "~3n edit peak" to `content.rs` comments; `content.rs` documents
  only the ~2n whole-file encode figure (`content.rs:77-81`). The ~3n edit peak is real but must be
  derived (`apply.rs:143-150` + `content.rs:98-102`); it is documented nowhere I found.
- **H5.** `mapping/read.rs:38-39` says the frontier retains "one 56-byte entry per mapping node";
  `Frontier` (`read.rs:69-75`: 32-byte id + bool + u8 + u64 + Option<(u64,u64)>) lays out closer to 64
  bytes. Immaterial, but the number is not the struct's size.
- **H6.** Could not determine from source: (a) actual allocator/copy behavior inside
  `Read::read_to_end` for the probe (`content.rs:234-238`); (b) whether any provider implementation
  deduplicates repeated demands (C1's contract treats each call as a demand, `access.rs:33-35`; the
  Store bridge is report D's scope); (c) BLAKE3 or GEAR constant-factor costs (no measurements
  permitted here); (d) the general-case count of R1 double-demands per edit (0–2 by tree shape; the
  receipt measured one scenario); (e) `std::io::Cursor::chain` copy behavior in
  `construct_stream` (`content.rs:227`).
- **H7.** The per-file bound on chunk count, page count and file size is **[S]** (log-shaped tree,
  checked arithmetic throughout) with no enforced file-size ceiling inside C1 itself; the enforced
  ceilings are per-object/per-page/per-wave (§1). A file-size ceiling, if any, lives in callers — **[U]**
  from this scope.
- **H8.** `content.rs:86-90` and `tree.rs:243-248` are examples of comments that explicitly disclaim an
  optimization or document a former quadratic — both match the code as read; no contradiction found
  between those two files' claims and their bodies.
- **H9.** All "provider call" counts are C1-abstraction counts (one `read_canonical_batch` = one call).
  A C2 provider pages each call into ≤128-id SQL queries (`LOOKUP_PAGE_IDS`,
  `docs/roadmap/0.1/0.1.7/component-decoupling/parallelism-and-batching-study-20260918.md` §2.3, the
  corrected F4 analysis); C1's ≤32-id waves fit inside one such query, so the counts here are also SQL
  round-trip counts for the Store bridge. The retracted F4 claim was about the 4096 demand ceiling, not
  about these C1 waves.
