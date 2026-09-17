# Report B-c1: Stage 5 filesystem engine — complexity, memory and I/O trips

- Source commit: `5e45897dd9f56e7f563aada031a68070a438fb93` (clean tree).
- Method: static source reading only. No builds, no runs, no measurements, no
  performance claims. No receipts are cited because this report states no
  measured facts; every claim below is a code citation.
- Scope: `core/crates/layerfs-content/src/filesystem/` excluding `attributes/`
  and `references/` (owned by other agents), plus `layerfs-telemetry`'s timer
  tree. `references/` appears only where the operation driver calls it.

## Variables used throughout

| Symbol | Meaning | Bound |
| --- | --- | --- |
| n | bindings (leaf rows) in one directory's tree | — |
| c | children rows of one branch page | dir 93–232 (derived), inode 64–127 ENFORCED |
| d | depth of one sorted tree | ≤ 31 ENFORCED (`limits.rs:24`, `format.rs:658`) |
| k | rows in one page | dir leaf ≤ 740, dir branch ≤ 232, inode leaf ≤ 100, inode branch ≤ 127 |
| B | bindings one operation names (`input.directories` changes) | — |
| D | directory updates one operation names | — |
| P | path components | ≤ 256 ENFORCED (`limits.rs:16`, `path.rs:171`) |
| S | bindings in one rebound directory's effective subtree | ≤ 4,096 per walk ENFORCED |
| E | entries one listing call returns | ≤ 64 in validate (`validate.rs:293`), caller-set in reads |

Derived directory-branch fanout: fill rule ≥ 3,277 B (`limits.rs:34`) and ≤ 8,192 B
(`limits.rs:10`) with ≥ 35-B rows (`format.rs:290-298`) give c ∈ [93, 232];
`DEFAULT_PAGE_ITEMS = 234` (`format.rs:48`) is only the reserved cell count.

## 1. Algorithm inventory

| Algorithm | path:line | Time | Peak owned memory | I/O trips (waves) | Bound class |
| --- | --- | --- | --- | --- | --- |
| Initial construction merge (`apply_root`, base `None`) | `sorted/finish.rs:19-110` | O(n·k): each append pushes into a page whose full width sum is recomputed per append (`merge.rs:71-76`); splits O(k) each (`format.rs:582-600`) | one `Engine` budget ≤ scratch (4 MiB default `limits.rs:41`); spine 32 slots reserved once (`finish.rs:54-60`) | 0 reads (synthetic empty wire `finish.rs:37-50`); emits pages child-first | STRUCTURAL |
| Update merge, changed path (`edit` branch) | `sorted/merge.rs:234-297` | O(Σ over path pages of c): every child page of every branch page whose range holds a change is read, decoded and checked (`merge.rs:251-268`) | batch lease `chunk·(8192+assoc)+decode_scratch(8192)` (`page.rs:229-233`), shrunk to actual (`page.rs:246-255`) | ceil(c/32) waves per path page per level (`BATCH_CHILDREN=32` `page.rs:28`, `merge.rs:247`) | STRUCTURAL (grammar lacks child summaries) |
| Update merge, untouched subtree | `sorted/merge.rs:182-187` | O(1) after the child page was read: emits `Node::existing` from the already-decoded wire (`page.rs:118-129`) | wire retained until parent page done | 0 further waves below that child | STRUCTURAL |
| Update merge, unresolved neighbouring pages (`sibling`→`merge`) | `sorted/merge.rs:145-167`, `97-141` | O(children of the pair): `materialize` of a stored branch page reads each child to rebuild per-row summaries (`page.rs:444-502`) | both pages decoded simultaneously under the same budget | 1 wave **per child** (point `read`, `page.rs:180-189`, `470`) — unbatched | STRUCTURAL |
| Page partition / exact sizing (`push`, `fits`, `nearest_half`) | `sorted/merge.rs:52-93`, `format.rs:582-600`, `336-338` | O(k) width sum per append → O(k²) per page fill; O(k) split point | widths Vec O(k) (`merge.rs:80-84`) | none (pure arithmetic, `format.rs:9-10`) | ENFORCED by `fits` after each append (`merge.rs:77`) |
| Directory single-name lookup | `directory/read.rs:51-100` | O(d·log c + k) — leaf match is a linear `find` (`read.rs:72-76`) | one decoded page per level (no budget; `Wire` Vec) | d waves, 1 page each (`read.rs:298-300`) | STRUCTURAL |
| Directory batch lookup (`lookup_many`) | `directory/read.rs:108-179` | O(levels · (pages + names·log c)); results keep demand order with duplicates (`read.rs:139-149`) | frontier Vec + `BTreeMap` groups per level (`read.rs:152-171`) + answers Vec | 1 wave per level (`read.rs:126`) | depth ≤ 31 ENFORCED (`read.rs:122-124`) |
| Directory listing / pagination (`list_after`) | `directory/read.rs:187-259` | O(pages visited); skips any subtree whose max key ≤ cursor without reading (`read.rs:202-206`) | ≤ max_entries entries + DFS stack O(d) | 1 wave **per page** (`read.rs:208-210`, unbatched) | byte+count bounds ENFORCED, one row must fit (`read.rs:223-228`) |
| Inode single lookup | `inode/read.rs:38-82` | O(d·log c + k), k ≤ 100 | one page per level | d waves, 1 each (`inode/read.rs:51-53`) | STRUCTURAL |
| Inode batch lookup | `inode/read.rs:85-148` | O(levels · (pages + demands·log c)) | frontier + groups + answers | 1 wave per level (`inode/read.rs:108`) | depth ENFORCED (`inode/read.rs:104-106`) |
| Inode table merge (`apply_inode_changes`) | `sorted/finish.rs:165-172` → same engine | O(touched·k) as construction; consumes the reducer's change stream (`update.rs:346-356`) | same 4 MiB-class budget per engine | reads base table pages along changed paths (batched ≤ 32) | STRUCTURAL |
| Build reachability walk | `validate.rs:551-625` | O(B) over stated bindings, in memory, no reads (`validate.rs:555-563`) | `BTreeMap` edges + `BTreeSet` seen, O(D) | 0 | 4,096 entries **per walk** ENFORCED (`validate.rs:603-605`) |
| Parent-alias walk (whole base tree) | `validate.rs:241-368` | O(total base bindings × inode-depth) worst case: lists every directory in 64-entry pages (`validate.rs:289-296`) and does **one single-demand inode lookup per entry** (`validate.rs:331`, `323`) | `bound` map O(candidates), seen set O(dirs visited) | per directory: ceil(entries/64) listing calls · O(pages/call) + per-entry d-wave lookups | 4,096 entries **for the whole walk** ENFORCED (`validate.rs:302-304`) |
| Effective-cycle walk | `validate.rs:471-542` | O(S) per rebound directory binding; `effective_entries` lists the base directory fully and merges changes in O(entries+changes) (`validate.rs:638-696`) | `pending`/`seen` per walk + one full merged entries Vec per directory (O(S) rows, S ≤ 4,096) | listing pages 1 wave each + one single-demand lookup per entry (`validate.rs:529`) | 4,096 entries **per walk**, i.e. N rebinds may spend N×4,096 (`limits.rs:59-78`, `validate.rs:663-665`) ENFORCED |
| Root encode/decode | `root.rs:98-149` | O(1) fixed 116-B value (`root.rs:13`) | O(1) | 1 read per load (`read.rs:86`, `validate.rs:82`) | ENFORCED exact length (`root.rs:114-119`) |
| Sorted-page lifecycle (decode/retain/release) | `sorted/page.rs:180-212`, `520-542`, `budget.rs:32-110` | decode O(page); append amortized O(1) with doubling capped at `page_items` | reserve-before-alloc: `decode_scratch = 3·bytes + rows·48` (dir, `format.rs:304-306`) or `·88` (inode, `format.rs:523-525`); lease returns bytes on drop (`budget.rs:105-110`) | counted per point read only (see §4) | budget limit ENFORCED pre-alloc (`budget.rs:38-43`) |
| Operation driver phases | `update.rs:148-385` | see §3 | `contents` BTreeMap O(D); `unreachable` O(D) (`update.rs:394-417`) | 6 timed phases: validate, directories, references, inodes, cleanup, root.encode (`update.rs:160,179,337,354,369,374`) | phase count STRUCTURAL |
| Timing tree | `telemetry/timer/recording.rs:144-199`, `scope.rs:140-162` | insertion O(1) amortized; clip check O(1) (`recording.rs:203-206`); `mark_incomplete` O(depth) ≤ 32; collect O(nodes); `from_bounded` O(children) (`report.rs:195-217`) | slot arena ≤ 1,024 nodes | 2 clock reads per node when enabled (`recording.rs:155,161`); zero when disabled (`scope.rs:150-151`) | MAX_NODES=1,024, MAX_DEPTH=32, MAX_LABEL_BYTES=128 all ENFORCED (`recording.rs:14,17,20,203-206`) |

Driver detail (one `run_body`): validate → per-directory merge loop (`update.rs:180-275`, one `Engine`/budget per directory, `update.rs:239-248`) → contents value derivation (`update.rs:282-295`) → zero-count scan in `base_read_batch` waves (default 32, `update.rs:477`, `references/reduce.rs:27`) → release traversal → reference reduce → inode merge → root emit. `FilesystemObjects.read_batch` refuses waves > 4,096 ENFORCED (`objects.rs:20,90-95`); direct `read_canonical_batch` calls rely on the provider's declared capacity, default 4,096 (`layerfs-storage/src/policy.rs:80,328`, `cas/store.rs:256-265`) — UNKNOWN whether any caller lowers it.

## 2. Opportunity register

R1. **Whole-tree walks are O(total bindings) per rebind** (`validate.rs:277-348`, `471-542`), ceiling 4,096 charged per walk so N rebinds → N×4,096 (`limits.rs:59-78`).
Alternative: a summary-based membership check. Directory pages already record `subtree_count`/`subtree_bytes` (`format.rs:56-57`) but **no ancestor-set information**; "is serial P anywhere under D" is not answerable without enumeration. A per-page digest (e.g. Bloom of member directory serials) would answer it in O(pages on D's path × digest), i.e. O(log n)-ish — but it changes the page grammar, so **every partition and root changes: canonical-compatibility BREAK against the sealed reference oracle**. Classification: IRREDUCIBLE under the sealed format; format-extension opportunity flagged. A cheaper non-format win: charge one shared 4,096 budget per operation instead of per walk — resource-contract change only, no canonical risk, but it *reduces* what large operations can do.

R2. **Page-group acquisition width ≤ 32** (`BATCH_CHILDREN`, `page.rs:28`). Each path page costs ceil(c/32) waves; c up to 232 → 8 waves where 1 would do. Raising the width toward the 4,096 demand ceiling (`objects.rs:20`) keeps every page read, authenticated, decoded and checked — only the wave count drops ~8×. Tradeoff: the pre-fetch reservation grows to `chunk·(8192+assoc)+decode` (`page.rs:229-233`), consuming more of the 4 MiB budget; the loop already narrows on budget pressure (`page.rs:229-235`). No canonical risk (I/O pattern only). REDUNDANT.

R3. **`page_from_wire` point reads one child per wave** (`page.rs:470`) — used by unresolved-neighbour merges and the root-collapse loop (`finish.rs:95-103`). This is the same acquisition `edit` does batched; batching it cuts a neighbour merge's child reads from (c1+c2) waves to ceil((c1+c2)/32). No canonical risk. REDUNDANT.

R4. **Per-binding single-demand inode lookups in validate**: one `lookup_optional` per binding (`validate.rs:175`), per alias-walk entry (`validate.rs:323,331`), per cycle-walk entry (`validate.rs:529`) — each a fresh root-to-leaf descent of d single-page waves (`inode/read.rs:101-147`). Batching per listing page (the `ALLOCATION_CHECK_BATCH=64` pattern already exists for `new_inodes`, `validate.rs:57,430-437`) turns O(B·d) waves into O(d + B/64·d). REDUNDANT, no canonical risk.

R5. **Parent inode records re-read across phases**: `lookup_one` in validate (`validate.rs:152`), `lookup_base` again in the directories phase (`update.rs:214`, `203`), and once more for contents values (`update.rs:288`). An operation-scoped record memo, or one batched lookup of all parents before the loop, removes 2 of 3. REDUNDANT, no canonical risk.

R6. **No page cache across validate walks and the merge**: the alias walk lists a directory's pages, the cycle walk lists the rebound subtree's pages again, and the merge reads the same directory's pages a third time — each through the provider with no memo (`objects.rs:38-42` holds no cache). A bounded operation-level page memo would cut this; read-only, no canonical risk. REDUNDANT.

R7. **`push` recomputes the whole page width sum per append** (`merge.rs:71-76`) → O(k²) per page fill, k ≤ 740. A running total in `Page` is O(1) per append and produces identical partitions. REDUNDANT, no canonical risk.

R8. **`batch_children` linear position scan** (`merge.rs:256-259`): O(chunk²) ≤ 1,024 comparisons per 32-child chunk. A small index map removes it. REDUNDANT, bounded constant.

R9. **Single-name lookup scans the leaf linearly** (`directory/read.rs:72-76`) while `lookup_many` uses `binary_search` (`read.rs:142`). Bounded by k ≤ 740. REDUNDANT.

R10. **`persist` re-encodes materialized-but-unchanged pages to detect reuse** (`page.rs:346-354`): identity is content-derived, so reuse of a *materialized* page can only be proven by encoding. Pages never materialized (`Node::existing`) skip this. A sound shortcut would need a fingerprint of (origin, row identities, split boundaries); unsafe in general because redistribution reorders rows. PARTIALLY IRREDUCIBLE.

R11. **Listing pagination re-descends from the root per continuation** (`read.rs:187-259`): each resumed call re-reads the O(d) branch pages on the path to the cursor (subtrees fully left of the cursor are skipped without reading, `read.rs:202-206`). A full listing of n entries costs ceil(n/64)·O(d) extra page reads in validate's walkers. An opaque cursor (page id + index) would avoid it but changes the public continuation contract ("the last delivered name", `read.rs:6-8`) and leaks identities; API change, not canonical. REDUNDANT with API tradeoff.

R12. **Listing DFS reads one page per wave** (`read.rs:208-210`): the stack's next level could be prefetched with `read_canonical_batch` (sibling leaves of one branch are known before popping). REDUNDANT, no canonical risk.

R13. **`nearest_half` and the split thresholds are pinned** (`format.rs:579-600`, `308-330`, `limits.rs:104-105`): any change to split-point selection, fill thresholds or row-width accounting changes canonical page partitions and therefore every root. IRREDUCIBLE — do not touch.

R14. **Store child summaries in branch rows** (count/bytes/size per child, ~16–24 B): would let untouched children stay unread, collapsing the changed-path cost from O(c) per level to O(1) — the largest structural win available — but it widens rows, changes every partition, and breaks the sealed oracle. IRREDUCIBLE under the sealed format; flagged as the format-level alternative to R2.

## 3. Round trips

- **Per-level waves that could share one call**: `edit`'s ≤32-child waves (R2); `page_from_wire`'s per-child point reads (R3); listing's per-page waves (R12). The provider accepts up to 4,096 demands per wave (`objects.rs:20`; storage default `policy.rs:80`).
- **Does an update touching several directories re-read shared ancestor *directory* pages?** No: each directory update merges that directory's own content tree from its own root (`update.rs:210-219`, `239-248`); two directories' page trees are disjoint — there is no common directory-page prefix to re-read. The **shared structure is the inode table**: every single-demand lookup (`validate.rs:175,323,331,529`; `update.rs:203,214,288`) re-descends from the table root (`inode/read.rs:47,101`), so the table's root and upper branch pages are re-read once per lookup, and a directory's *own* pages are re-read up to three times per operation (alias walk, cycle walk, merge — R6). The parent *record* is read 2–3 times across phases (R5).
- **`resolve`**: per component, one full directory descent (d waves, `read.rs:120-127`) **plus** one full inode-table descent (d waves, `read.rs:128,271-274`) — P components → P·d + (P+1)·d waves, inherently serial (each component needs its parent's `content_root`). The per-component inode lookup is not batchable across components, but multi-name entry points already batch (`read.rs:162-185`).
- **Repeated decodes**: every page is decoded exactly once per reader; no decode result is shared between validate, the merge and read paths within one operation (each builds its own `Wire`). `effective_entries` re-lists and re-decodes a rebound directory's base pages once per walk (R6).
- **Batched already**: directory/inode `lookup_many` (1 wave/level), `check_new_identities` (64/wave), `zero_count_serials` (`base_read_batch`/wave, default 32), and the sorted engine's child acquisition (≤32/wave).

## 4. Honesty notes

- **Counter doc-vs-code contradiction**: `SortedWork.pages_read` is documented "Pages read, including batched ones" (`page.rs:40-41`) but only the point-read path `Engine::read` increments it (`page.rs:185-186`); `batch_children` (`page.rs:220-259`) and `decode_page` (`page.rs:192-212`) never do, so a merge executed through batched acquisition reports `pages_read = 1` (the root) in `SortedWork` while the operation-level `ObjectWork` counts the waves (`objects.rs:103-107`). Consumers of `counters.directories.pages_read` (`update.rs:249-252`) see the undercount.
- The 4,096 per-walk ceiling is **per walk, not per operation** (`limits.rs:59-78`); its doc states the build boundary as 4,096 accepted / 4,097 refused — that is a documented measured fact from a prior verification probe, restated here from `limits.rs:80-86`, not re-measured.
- UNKNOWN: whether any deployed caller overrides the storage `read_objects` capacity below 4,096 (`policy.rs:328`); actual wave fanout at runtime; behaviour of `references/` and `attributes/` beyond the driver call sites (out of scope); the sealed oracle's own contents.
- `MAXIMUM_TREE_LEVEL = 31` (`limits.rs:24`) and telemetry `MAX_DEPTH = 32` (`recording.rs:17`) are unrelated constants with similar values; no coupling exists.
- Directory `filled()` ignores row count and level (byte rule only, `format.rs:332-334`) while inode `filled()` is count-based (`format.rs:544-551`) — deliberate profile asymmetry, not a bug.
- `DEFAULT_PAGE_ITEMS = 234` (`format.rs:48`) exceeds the 232 minimum-width directory branch rows an 8,192-B page can hold, so `fits` always refuses first; 234 is a reservation figure, not the effective bound.
- Scratch is budgeted **per engine** (per directory merge, `finish.rs:33`, `update.rs:239-248`), so operation peak is per-merge, not the sum; the driver records the max (`update.rs:269-272`).
