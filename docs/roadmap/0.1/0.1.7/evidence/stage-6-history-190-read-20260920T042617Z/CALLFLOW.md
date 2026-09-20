# Pooled-read call flow: legacy against current, with the measured costs

> Status: Research; informative and not a product contract. Source facts, measured
> costs and hypotheses are labelled separately. References were resolved with
> `git show REV:path | nl -ba` against the revisions named below, not assumed from
> an earlier document's line numbers.

**Pins.** Legacy: `7fab1027a0061e8b932345d4fcd6ac22a089b155`, whose compiled identity
is `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`; its Store source is unchanged from
that pin to `HEAD` for the files cited. Current: `4049e28b6` (the retained commit
of this campaign).

## Legacy: one BLOB, one directory entry, one bounded pool per record group

```text
objects/read.rs:1690-1696  visit_locations
  sort physical locations by (pack, group, record) so a group is not scattered
        |
        v
objects/read.rs:1773-1795  Extraction::Metadata(pooled, entry, encoded)
  decode the demanded metadata record group
  let mut wave_pool = PoolRead::default();        <-- ONE bounded pool reader
  for (id, location) in ordered {                 <-- per RECORD GROUP, not per leaf
      metadata_record(&decoded, location.record)
      metadata_chain(id, location, pooled, record, None, &mut wave_pool)
  }
        |
        v
objects/read.rs:1091-1115  required-group extraction
  blob_open("main", "object_packs", "data", pack_id)
  blob.len()                       -> actual BLOB length
  read_at_exact(header, 0)         -> 16-byte control area
  versioned_header(&header, length)-> header validated against that length
  if group >= header.group_count   -> refuse
  read_at_exact(directory, 16+16*group)  <-- ONLY the selected directory entry
        |
        v
objects/read.rs:1131-1132
  read_at_exact(encoded, entry.range.start)   <-- ONLY the selected group range
```

**Source facts.** Legacy reads the control area and **one** directory entry, then
the selected group's byte range. It checks the selected group's ordinal against
the header's group count and validates the selected entry against the BLOB length.
It does **not** walk the complete directory: continuity, extents, codecs, final
coverage and lane are not all checked on this route. Current core validates all of
them (`pack/layout.rs:252` `parse_header`, `:319` `group_view`), and porting the
legacy partial check would be a **weakening**, not an optimisation.

**Source fact.** Legacy's reuse scope is one demanded metadata record group
(`wave_pool`, created inside the `Extraction::Metadata` arm). Current core creates
one `PoolReader` per resolved inode leaf (`encoding/delta/read.rs:143`).

## Current core: whole pack BLOB, complete directory, per-leaf reader

```text
cas/read.rs:147 ReadSession (one per operation)
  connection + decode workspace + GroupCache (decoded ORDINARY groups)
        |
        v
cas/read.rs:51 read_objects  (one wave)
  lookup::locations(page)  -> locators, ceiling applied
  let mut packs = BTreeMap::new();        <-- wave-level pack cache, 4 MiB bound
  for each requested id:
      Resolver::new(connection, ceiling, capacities, &mut packs, groups, ...)
        |
        +-- role == InodeLeaf ----------------------------------------------+
        |   encoding/delta/read.rs:143  PoolReader::new()                  |
        |     PRIVATE pack cache, PRIVATE value cache, fresh per leaf       |
        |     encoding/pool/read.rs:238 leaf_body_with_groups               |
        |       chain walk -> :340 record()                                 |
        |         :209 pack()   -> sqlite/lookup.rs:148 pack_bytes          |
        |                          SELECT data FROM object_packs ...  <-- FULL BLOB
        |         pack/layout.rs:252 parse_header  (control area)           |
        |         pack/layout.rs:319 group_view    (directory, extents)     |
        |         decode group body (Zstd), frame the record                |
        |     encoding/pool/read.rs:419 leaf_canonical_with_groups          |
        |       :443 resolve rows in ORDINAL order (retained change)        |
        |         sqlite/pool.rs:92 group_for   <-- one per distinct group  |
        |       :135 load_group -> :180 group_body -> :209 pack() (values)  |
        |       rebuild_leaf, authenticate canonical identity               |
        +-------------------------------------------------------------------+
        |
        +-- every other role -> decode_at -> pack_of(self.packs, ...) (wave cache)
```

## Where the measured stride10 nanoseconds are

From `baseline2-instrumented-stride10-analysis.json` (instrument v2, both arms).
The provider's own elapsed interval is 9,019,067,220 ns.

| Where it is spent | ns | Kind |
|---|---:|---|
| `lookup::pack_bytes` — SQLite row materialisation and the copy into a `Vec` | 3,928,606,618 | measured |
| `sqlite/pool.rs:92 group_for` — 278,927 statements | 2,180,465,764 | measured |
| object/locator SQL | 666,844,674 | measured |
| pooled value materialise + authenticate + decode + convert | 644,700,604 | measured |
| ordinary-lane group decompression | 177,929,427 | measured |
| control-area validation (`parse_header` + `group_view`) | 47,134,250 | measured |
| ordinary record decode call | 21,186,780 | measured |
| canonical leaf re-encode | 17,180,051 | measured |
| physical pooled body rebuild from the delta chain | 14,238,663 | measured |
| record framing | 13,122,420 | measured |
| physical pooled body decode into rows | 1,717,437 | measured |
| **not attributed** | **1,305,940,532** | unmeasured remainder |

**Source fact + measurement, the retained change.** `group_for` was called once per
change of covering group. The loop's comment claimed a leaf's rows are in ordinal
order; they are ordered by serial, and `decode_pooled_body` rejects a body whose
serials are not strictly increasing. Measured consequence: 278,927 statements for
677,234 rows and 65,337 distinct covering groups — 24.8 statements per leaf against
5.80 groups, at 7,817 ns per statement. The retained change visits the rows in
ordinal order, so the statement count becomes exactly one per distinct group
(65,337 measured, the arithmetic prediction).

## What is *not* established by this flow

- **Hypothesis, not measured.** That the 3,928,606,618 ns of pack acquisition is
  dominated by the copy rather than by SQLite's row location. The interval is
  measured as a whole; its internal split is not.
- **Hypothesis, not measured.** That a selected-group BLOB read would win. It would
  add a BLOB open and range reads per extraction and give up the reuse the pack
  cache already provides (67% of 436,068 demands are served without SQLite), and
  it must still validate the complete directory. Its screen is **NOT_RUN**.
- **Source fact, not a measurement.** Legacy's per-record-group pool scope and
  current core's per-leaf scope are different lifetimes. Whether reusing a pack
  cache across an operation is safe depends on the Store write path
  (`cas/placement.rs:175-202` invalidates on every append), which is why that
  direction remains a proposal with its own contract to prove.
- **Not comparable.** Legacy's timers bracket different work: its
  `namespace_ns = 342,355,542` excludes dirty-directory work charged to Content,
  and its Commit includes canonical construction and admission. Nothing in this
  table is subtracted from the historical 11,370,679,212 ns.
