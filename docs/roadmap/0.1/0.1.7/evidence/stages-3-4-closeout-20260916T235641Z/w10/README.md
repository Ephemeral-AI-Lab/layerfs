# W10 - limits boundary coverage (gate G19)

Every row below is a boundary the Stage 3-4 review listed as unqualified. Each was
driven through an ordinary public API with a bounded fixture - no theoretical
maximum was allocated, no disk was exhausted, no endurance campaign was started.
Where a boundary cannot be reached inside a packet-sized fixture it is written
**UNRUN** with the source proof and the measured coverage gap; nothing is recorded
as passing that was not observed.

Fixtures are built inside each case (deterministic xorshift `noise`, a
`Repeatments` byte source, or an in-process `MemoryStore`); no fixture file is read
from disk. Every number below is a **measured counter** unless it is explicitly
labelled *derived*. The raw stdout, exit codes and whole-command wall times are in
`w10-verify.log`; the blocks were recorded with `record.sh` and nothing in the file
was rewritten afterwards.

## 1. W10.1 - the frontier leak the ceiling case exposed (production fix)

The 4 096-edit ceiling case is the first input that holds *many* superseded drafts
at once. On the committed W3 frontier it failed on the deepest shape: pages built
inside `concat_inner`'s `split` recursion became unreachable as soon as the next
level replaced their parent, and nothing released them (286 leaked level-1 pages at
4 096 edits, peak still growing with the edit stream). `tree.rs` now counts the
parents that reference each draft (`parent_refs`) and tracks drafts no live draft
references (`detached`), so releasing a superseded node cascades to children that
no surviving draft names; `apply.rs` calls `EditObjects::settle(mapping)` after
each edit, which releases the drafts the finished edit disconnected.

Effect, both measured on the same 4 096-edit fixture: `peak_deferred_bytes` is now
`353 952` B on the small-base ceiling case (79 published pages) and `384 112` B on
the 8 MiB base (122 published pages), i.e. the frontier is the tree the operation
ends up with, not the edit stream. The bytes were verified by reading the result
back (`read_back(&consumer, root) == expected`). This extends G3's release rule;
it is recorded in the report as W10.1 rather than as a rewrite of W3.

## 2. Boundaries run

| # | Boundary | Value tested | API | At the limit | Over the limit | Validation before mutation | Command (log block) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | CHUNK delta depth, **changed cap** | cap 2 vs cap 3, same two-link chain | `StoragePolicy::new(.., chunk_depth)` + `Store::create` + `save_one` | cap 2: `delta.trials=0`, `ineligible_candidates=1`, `full_records=1`, `prefix_records=0`; cap 3: `trials=1`, `prefix_records=1` | - | eligibility is decided before a base is acquired; the ineligible base is never read (`trials=0`) | `delta_payload::the_chunk_lane_honours_its_own_depth_cap` |
| 2 | CHUNK depth 50 | 51 records, depth 50 at the deepest | same + `Store::read_batch` | 51 records built, `max_depth=50` on read, root identity re-checked | one level deeper: `trials=0`, `ineligible_candidates=1`, `work_exceeded=0` | the 51st save is refused as ineligible before any trial; depth is not a work budget | `delta_payload::a_fifty_link_chunk_chain_is_admitted_and_read` |
| 3 | Edit stream ceiling | 4 095, 4 096, 4 097 edits | `EditStream::new` | 4 095 and 4 096 accepted | 4 097: `BoundedCapacityExceeded { what: "edit.stream", limit: 4096, actual: 4097 }` | refused by `EditStream::new` itself - before a store, a scope or an object exists | `edit_bounds::the_edit_stream_ceiling_is_enforced_before_any_work` |
| 3b | Edit stream ceiling, real run | 4 096 edits applied over a 400 000 B base | `apply_edits` | `payloads_created=4096` (one per edit), `nodes_created=79`, `peak_deferred_bytes=353 952`, result bytes equal the model | - | every edit is validated by the stream before the frontier is touched | same case |
| 4 | 8 MiB - 1 edit deferred state | **UNRUN** - see §3 | `EDIT_DEFERRED_LIMIT` | largest in-budget shape: `peak_deferred_bytes=384 112` over 122 pages (3 148 B/page measured; 8 320 B/page is the derived maximum) | not induced; derived floor to cross is 408 MiB of base | `charge_bytes` refuses before inserting the draft | `edit_bounds::the_deferred_ceiling_charge_per_draft_stays_inside_its_derived_bound` |
| 5 | 8 MiB object field | 8 MiB, 8 MiB + 1 | `encode_bytes_object` / `decode_bytes_object` | accepted; canonical is field + `HEADER_LEN` + 4 | `ObjectLimitExceeded { limit: 8388608, actual: 8388609 }`, the **field** ceiling named, not the envelope | encoded length checked before the envelope buffer is built; decode refuses before materialising | `object_identity::the_field_ceiling_is_the_frozen_eight_mib_not_the_envelope` |
| 6 | 16 MiB envelope | 16 MiB, 16 MiB + 1 (declared total) | `decode_bytes_object` | at the ceiling the *field* check fires (`limit: 8388608`), proving the total check is not one byte early | +1 byte: `ObjectLimitExceeded { limit: 16777216, actual: 16777217 }` | header length is validated before any body is read | `object_identity::the_envelope_ceiling_is_enforced_at_plus_or_minus_one` |
| 7 | Pooled window, exact cap | 131 072 entries of distinct values, then +1 | `PoolIndex::note_group` | 131 072 retained exactly (cap retained, not evicted); `live_bytes` = key + ordinal for the single retained entry after the reset | the crossing group resets the window wholesale to exactly itself; a refused note leaves the window unchanged | chronology and empty groups are refused as `Integrity` without disturbing the window | `metadata_window::the_window_retains_exactly_the_cap_and_resets_whole_groups` |
| 7b | Pooled window at real scale + cold replay | 1 312 leaves / 131 200 distinct ordinals through a real `Store` | `save_one`, `open_store`, `read_objects` | one group per leaf; the crossing group is `(131001, 100)`; retained window 200 entries `<< 131 072`; evicted values rewritten not lost; re-saving a retained value reuses it | reopen: a fresh Store starts at 0 entries and replays the producer's window from the catalogue (300 then 400 entries), never rewinding to ordinal 1 | ordinals are assigned before the group is written; a refused/evicted value is written again | `metadata_window::crossing_the_window_evicts_early_ordinals_and_the_reopen_replays_the_window` |
| 8 | Read wave (C1) | 24 MiB + 1 file, ~768 payloads | `read_all_bounded` | `payload_batches_read > 1`; `max_payload_batch == READ_WAVE_OBJECTS (32)` measured | the byte window is the *derived* identity `READ_WAVE_BYTES = 32 x 32 768 = 1 MiB`, asserted as an identity, **not** as a second measurement | waves are released before the next; no wave exceeds the object bound | `file_read::a_large_read_acquires_payloads_in_bounded_batches` |
| 8b | Wave byte ceiling (C2) | 200 x 6 000 B accepted in one save = 1 200 000 B > 512 KiB | `SaveOperation::accept` / `pending` | peak pending asserted `<= batch_objects (512)` and `<= batch_bytes (524 288)` after **every** accept, so the wave splits by bytes rather than growing | - | the accepted object is already owned before the bound is read | `memory_bounds::pending_ownership_stays_inside_the_declared_batch_bounds` |
| 9 | `PoolReader` value cache | 200 leaves x 100 values = 1 460 000 B decoded | `PoolReader::leaf_canonical` | `peak_retained=518 300` B <= bound `524 288` B (71 groups) | more than twice the bound was decoded without the cache ever exceeding it, so a release is proven, not assumed | the ceiling is decided before the decoded-value cache is consulted | `metadata_pool::the_pooled_value_cache_releases_at_its_declared_bound` |
| 10 | File > 24 MiB (C1) | 24 MiB + 1 | `read_all_bounded` | full logical read, bytes equal | - | - | same case as #8 |
| 11 | File > 5 MiB (C2) | 6 MiB + 1 through a real `Store` | `construct_stream` + `SaveHandoff` + `read_batch` + reopen | spans 27 packs (`ceiling=27`), reads back equal bytes twice (live and reopened) | - | - | `core_pipeline::a_file_larger_than_five_mebibytes_survives_a_real_store` |
| 12 | SQLite engine maxima | the declared pragmas, and the host's own | `sqlite::connection::open`, `PRAGMA`s | asserted exactly: `journal_mode=memory`, `synchronous=0`, `temp_store=2`, `foreign_keys=1`, `busy_timeout=0` | **environment-dependent, unqualified**: measured host values `page_size=4096`, `cache_size=2000`, `mmap_size=0`, `sqlite_version=3.51.0`; nothing is asserted about their values, and `cache_size` is not presented as an RSS cap | - | `policy_capacity::the_connection_profile_is_declared_and_the_engine_maxima_are_the_hosts` |
| 13 | Directory / workspace dimensions | - | - | **NOT_APPLICABLE to Stages 3-4** (Stages 5/7): no directory, name, path or workspace-size value is modelled by C1/C2, and none is invented here | - | - | report §8 and the review's own §5.3 table |

`MIN_ENTRIES`, `MAX_NODE_OBJECT_BYTES`, `MINIMUM_CHUNK_BYTES`, `EDIT_DEFERRED_LIMIT`,
`READ_WAVE_OBJECTS`, `MAXIMUM_EDITS_PER_OPERATION`, `MAX_OBJECT_FIELD_BYTES` and
`MAX_CANONICAL_OBJECT_BYTES` are the product's own declared values; the cases read
them rather than repeating them, so a change to a limit moves the case with it.

## 3. Row 4 - the 8 MiB - 1 deferred ceiling is UNRUN, with the proof

`EDIT_DEFERRED_LIMIT` (`file/edit/tree.rs`) is charged **live**: `charge_bytes`
adds the draft's own charge and `release` subtracts it, so crossing 8 MiB - 1 needs
roughly a thousand drafts alive at the same time.

* Each live draft charges at most one mapping node plus the fixed 128-byte
  overhead: `MAX_NODE_OBJECT_BYTES + 128 = 8 320` B. The ceiling therefore needs
  `ceil(8 388 607 / 8 320) = 1 009` live pages.
* A non-root mapping page holds at least `MIN_ENTRIES = 64` entries
  (`mapping/types.rs`), and the root is one page, so 1 009 live pages span at least
  `64 x 1 008 = 64 512` extents.
* An edit operation can only create `length / MINIMUM_CHUNK_BYTES + 3 x edits`
  extents: every extent is a chunk of at least 8 192 B except the at most three
  boundary pieces one edit leaves behind (left remainder, replacement, its
  sub-minimum tail). At the edit ceiling of 4 096 (`MAXIMUM_EDITS_PER_OPERATION`)
  that is 12 288 boundary extents, so the *base* alone must exceed
  `(64 512 - 12 288) x 8 192 = 427 819 008` B, about 408 MiB - and about 3.4 GiB
  with no edits at all.

That is far outside this packet's fixture budget, and the packet forbids allocating
a theoretical maximum, so the refusal is recorded as unrun. What *is* run is the
premise the derivation rests on, on the largest in-budget shape (8 MiB base, all
4 096 edits spread across it): measured `peak_deferred_bytes=384 112` over 122
published pages = 3 148 B per draft, inside the 8 320 B bound, with
`payloads_created=4096` proving the whole stream was applied. The refusal branch
itself has no coverage: if it ever fired, no case here would report it.

## 4. What this packet does not prove

* No timing, throughput or memory-RSS claim. Every assertion is allocation,
  counter or byte-equality accounting; the wall times in the log are budget
  accounting for the commands, not product numbers.
* The 8 MiB - 1 deferred refusal is unrun (§3), the directory/workspace dimensions
  are out of scope (row 13), and the SQLite engine maxima are the host's, not the
  product's (row 12).
* The read-wave byte window is a derived identity from the object window and the
  largest chunk record (row 8); only the object window is measured.
* `edit_bounds` is one command of 16.0 s (12 cases, two of them 4 096-edit
  constructions). It is a verification run inside the 60 s verification budget and
  is declared as such: nothing in this packet derives a performance claim from it.

## 5. Commands, exits and budgets (`w10-verify.log`)

| Block | Exit | Wall |
| --- | --- | --- |
| rustfmt check (product workspace) | 0 | 0.44 s |
| content focused: `edit_bounds`, `file_read`, `streaming`, `object_identity` | 0 | 25.26 s (12 + 9 + 9 + 10 tests) |
| storage focused: `delta_payload`, `metadata_pool`, `policy_capacity`, `core_pipeline`, `metadata_window` | 0 | 15.50 s (15 + 15 + 9 + 6 + 2) |
| storage focused: `memory_bounds`, `visibility`, `pack_locator`, `physical_formats` | 0 | 5.30 s |
| storage focused: `delta_chains`, `metadata_pool_index`, `edit_pipeline` | 0 | 2.99 s |
| workspace suite (declared verification run) | 0 | 115.02 s - 43 targets, 272 tests, 0 failed |
| sealed reference parity `edit_reference` (declared verification run) | 0 | 56.90 s - 2 tests, nine sealed cases |
| clippy, all targets, `-D warnings` | 0 (after one unused binding was removed; both runs are in the log) | 1.64 s / 0.25 s |
| product boundary, both tool suites | 0 | 0.09 s / 0.14 s / 0.14 s |
| measured cases re-run with `--nocapture` for raw stdout | 0 | included above |

Earlier blocks in the same file record two command-level mistakes of mine (a root
workspace `fmt --all --check`, which also checks the untouched reference tree, and
a `--manifest-path` used from the wrong working directory). They are kept on disk
as recorded failures; the product-scoped rustfmt check and every later command exit
0.

## 6. Production LOC

`core 10983 -> 11058 (delta +75)` for the W10 commit: C1 `4388 -> 4463`,
C2 `5863 -> 5863`, telemetry `732 -> 732`; reference `65417` unchanged, combined
`76400 -> 76475`. All +75 are the frontier fix (`tree.rs`, `apply.rs`); the
`POOLED_VALUE_CACHE_BYTES` move is net zero (one declared constant line added in
`policy.rs`, one removed from `read.rs`, both counted as code).

## Control-log status (added 2026-09-17, D6)

This packet retains **no `*-fails-without-fix` control log**. W1–W6 each retain
one; W7, W8, W9 and W10 do not, so every margin this packet's oracles assert is
**source-derived** rather than demonstrated to fail without its fix. The
independent 2026-09-17 review recorded this as F-23 and counted the census in
`evidence/stages-3-4-review-20260917T022248Z/packet-control-audit.txt`.

They are labelled rather than repaired here. A control for these packets needs a
patch that reinstates the defect each oracle guards — for W7 the memory ledger's
charging path, for W10 the limits boundary list — and neither patch existed when
this round ran. Writing one and reporting it as a control after the fact would be a
receipt manufactured for the occasion, which the standing rules forbid; the honest
alternative the review allows is this label. Gate G7's claim that "each new case
fails without its fix" therefore holds for W1–W6 only, and the batch's own tracker
says so (closeout report §2, G7 is scoped to W4 there; §6 records the gap).
