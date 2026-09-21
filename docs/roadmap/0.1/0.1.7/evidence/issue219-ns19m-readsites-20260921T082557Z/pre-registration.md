# Pre-registration — #219 round 12: which call site reads the inode table

Written **before** the first run of this arm and before any edit. This round changes **product
instrumentation only**: counters in `layerfs-content/src/filesystem/validate.rs`, no behaviour.

Control: **K1** (`ns19-K1-counts-20260921T082053Z`, `31660e990`) and **J1** before it:
`span_build_ns` 308.59/323.86 ms, `build_validate_ns` **221.70/222.15 ms**,
`validation_inode_pages_read` **27,662**, `validation_read_waves` **27,657**,
`validation_objects_read` = `inode_demands` 17,777 (a *charge*, memo hits included — corrected in
round 11's README §5), `validation_directory_pages_read` 0, `validation_entries_examined` 4,096,
`build_base_records_read` 6,066; 13/13 gates, 14/14 pins in both rows.

## Why this round exists, stated as the thing it decides

`validate` spends 222 ms and the physical counters say **27,662 authenticated page reads in 27,657
waves** — a ratio of **1.0002**. `lookup_many` charges one wave *per level* (`inode/read.rs:110`), so
a grouped prefetch of thousands of serials reads thousands of pages in one or two waves and **cannot**
produce a 1:1 ratio; `lookup` (singular, `inode/read.rs:41`) charges one wave *and* one page per
level. A 1.0002 ratio therefore says almost every wave is a single-serial descent.

But every call site that can read is *supposed* to be cheap:

| site | why it should be cheap |
| --- | --- |
| `prefetch` | groups every parent and every bound child of the batch into one `lookup_many` |
| the binding loop | asks about the serials `prefetch` just answered — memo hits |
| `check_parent_aliases` | one listing per candidate parent |
| the cycle walk | asks about the batch's own children, and `directory_pages_read` is 0 |

Two of this lane's prior readings of this phase have already been wrong — `build_accept_ns`
(predicted 15–30 ms, measured 0.05 ms) and the cycle-walk prior (predicted 12,000–40,000 entries,
measured 4,096) — so this round **measures the split instead of guessing a third time**.

## The one difference

**`ValidationWork` gains `inode_pages_by_site`**, six named counters — `allocation`, `prefetch`,
`bindings`, `aliases`, `cycles`, `reachability` — each charged as the delta of the existing
`inode_pages_read` across that site's region. The six sum to `inode_pages_read` exactly, and the
harness publishes them for this row.

Two reads are deliberately **not** in a site and are documented as such: `FilesystemTopology::load`
reads the base root once per update batch (`validate.rs:82`) and **charges no counter at all**, so
its site is 0 by construction rather than by measurement. Changing that charge would move an existing
published counter (`fs_build.validation_*`), which this round does not do.

## Prediction, in the instrument's own units

Stated as a partition so the row can refute it, not as a winner:

| site | predicted share of 27,662 pages | derivation |
| --- | ---: | --- |
| `prefetch` | **< 2 %** | `lookup_many` charges one wave per level; ~4,000 serials per batch over a shallow table is tens of pages, in 2–3 waves |
| `allocation` | **< 5 %** | `check_new_identities` groups 64 serials per wave over `new_inodes` only |
| `aliases` | **< 10 %** | one listing per candidate parent, and `directory_pages_read` is 0 |
| `cycles` | **≤ 25 %** | the walk's entry lookups are the batch's own children — memo hits |
| `bindings` | **≥ 60 %** | the only site that can ask a serial the prefetch did not cover, once per binding |
| `reachability` | **~0** | one build batch of three, with `directory_pages_read` 0 |

**The row is a partition and the refutation is a dominance test:** if no single site holds ≥ 60 % of
the 27,662 pages, the reading above is wrong and the reads are spread across the walks, and the next
round is about the walk's demand shape rather than about one call site.

## What would refute it

1. No site holds ≥ 60 % of `inode_pages_read` (the partition is diffuse and the attribution above is
   wrong). Reported as a missed prediction, not repaired by re-running.
2. The six sites do not sum to `inode_pages_read`: an instrument defect, diagnosed from the receipt.
3. Any of the 14 pinned counters moves, `pipeline.commits` != 284, the root digest changes, or the
   row is not PASS 13/13.
4. `build_validate_ns` outside 221.7 ± 40 ms, or `operation_work_ns` outside 1590.4 ± 250 ms:
   charging counters that already exist must be work-neutral, and 40 ms is the tightest band this
   instrument has been held to because the phase is stable across I1/J1/K1 (221.70 / 221.70 / 222.15).
5. Any product test fails: `cargo test -p layerfs-content` and the whole core workspace must be green,
   since this is the first round in this lane to touch product source since L68.

## What is not claimed

No fix and no movement. The output is a priced call site, and the treatment after it is registered
against this partition.
