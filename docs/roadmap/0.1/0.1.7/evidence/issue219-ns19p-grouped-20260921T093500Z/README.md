# #219 round 15 — the whole-file lane's groups carry many records: `operation_work_ns` 1380.70 → 1283.16 ms

Pre-registration: `pre-registration.md`, with `raw/group-census.py` beside it (both written before the
edit and before any run). Rows:
`benchmark-results/issue219/ns19-P2-repin-20260921T102900Z`, **PASS, 13/13 gates, 14/14 pinned
counters** (`golden_matches: true`), `source_commit` `6a1b2d9f2`, clean, root digest unchanged — and
the **consequence run** `ns19-P1-grouped-20260921T102651Z`, **FAIL, one gate**,
`g1.o3-pinned-counters` reading `pipeline.commits 284 -> 285`. Both receipts are on disk and the red one
is reported as red. Landed as `a1faf957e` (the change) and `6a1b2d9f2` (the re-pin).
Control: **O1** `ns19-O1-stored-20260921T091015Z`, not re-run.

## 1. The movement

| instrument | O1 | **P2** (covering) | P1 (red, pre-re-pin) | movement |
| --- | ---: | ---: | ---: | ---: |
| **`operation_work_ns` (the row's formula)** | 1380.70 ms | **1283.16 ms** | 1258.81 ms | **−97.54 ms, −7.07 %** |
| **CPU user+system** | 1397.14 ms | **1297.49 ms** | — | **−99.66 ms, −7.13 %** |
| `operation_ns` (inclusive) | 1475.87 ms | 1402.71 ms | 1303.30 ms | −73.16 |
| complete command | 2312.15 ms | 2278.26 ms | — | −33.88 |
| **`statements`** | 16,590 | **7,666** | 7,666 | **−8,924** |
| **`pack_appends`** | 15,532 | **6,603** | 6,603 | **−8,929** |
| `packs_created` | 1,265 | 1,270 | 1,270 | +5 |
| **`diag_write_pack_total_ns`** | 204.88 ms | **123.24 ms** | 130.16 ms | **−81.64** |
| **`diag_insert_objects_ns`** | 133.33 ms | **146.36 ms** | 151.79 ms | **+13.04** |
| `pack_bytes_written` | 302,074,398 | **301,864,382** | 301,864,382 | **−210,016** |
| `commits` | 284 | **285** | 285 | +1, re-pinned |
| `stored_records` | 23,910 | 23,910 | 23,910 | 0 |

**Cumulative against this worktree's clean tree** (A0: ≤ 3487.3 ms of work): **−63.2 %**. Against the
handoff's 1473.8 ms: **−190.7 ms**. The serial floor — commit + pack writes + row inserts + wave +
collision check + begin, the work one writer must do alone — moves from **917.20 to 882.35 ms**.

The two runs agree to the byte on every count: `statements` 7,666, `pack_appends` 6,603,
`packs_created` 1,270, `pack_bytes_written` 301,864,382, `stored_records` 23,910, `commits` 285. They
disagree by 24 ms on the row formula, and the receipt says where: P2's `teardown_ns` is **117.43 ms
against P1's 40.50** and its `span_build_ns` 121.78 against 92.09. The teardown is the operating
system's price for the pages the process dirtied — this lane has measured it at 6.7 → 469.9 ms on
identical source — and it is excluded from the formula, but it moves the accept span it is subtracted
from. **The counts carry this round; the row is reported as it came out.**

## 2. The treatment

`pipeline.statements` is exactly the number of object groups: **16,590 for 25,245 objects**, of which
9,444 were whole-file records that each bought their own `INSERT` and their own `write_pack`. The lane
now groups them, and the group body carries what the single-record form derived from its own extent:
`[count u32][end_i u32 …][compact records]`. That is the native lane's framing — one count, one
four-byte end offset per record — so the new code is a second caller of `assemble::frame_group` and
`decode::framed_record`, not a new framing. The compact record inside is unchanged (tag, optional base
identity, frame) and the lane's directory stays starts-only with a 1,024-byte reserved region, so **no
other lane's geometry moves**. Whole-file moves 14 → 17; 11 and 14 still map to the same lane.

`raw/group-census.txt` projected **519** whole-file groups from the control row's own Store, replaying
the owner's seal rule; the row returned **519** (`statements` 7,666 = 519 + 7,125 native + 21 ordinary
+ 1).

### The wall this had to walk around

**Only a placed row can be a delta base.** The first attempt sealed the group by target alone and
`tests/delta_payload.rs::the_admitted_full_cache_supplies_a_candidate_within_one_save` failed
(`outcome.delta.trials` 0, expected 1): the first object was still waiting in an open group, so
`select`'s `eligible` asked storage for a row that did not exist and the edge the winner cache proposed
was silently dropped. That is invariant 2 of the handoff, verbatim, and it is why the pre-registration
said this change "must not touch" it.

The group is therefore **placed before selection is asked for a representation** for an object that
could name one of its members: the caller's advisory list first, then whatever the winner cache
proposes for this exact payload. The signature that question needs is a scan of the payload, so it is
computed once and handed down (`SelectInput::signature`) instead of being computed in both places —
`cas/selection.rs::pending_base_for`. The cost is one signature per whole-file offer, which the
selection already paid; the saving it protects is 8,929 seals.

## 3. Prediction scorecard: four hits, four misses, two clauses fired

| # | registered | measured | |
| --- | --- | --- | --- |
| 1 | `statements` 7,800–7,950 | **7,666** | missed low — my model added ~207 pooled-lane inserts that this counter does not count; the object-group count is the census's 7,665, exactly |
| 2 | `packs_created + pack_appends` 7,870–8,050 | **7,873** | hit |
| 3 | `diag_insert_objects_ns` 75–90 ms | **146.36** | **missed, wrong sign** |
| 4 | `diag_write_pack_total_ns` 135–175 ms | **123.24** | missed, better than registered |
| 5 | `pack_bytes_written` +37,776 ± 5,000 | **−210,016** | **missed, opposite sign** |
| 6 | `operation_work_ns` 1250–1330 ms | **1283.16** | hit |
| 7 | `stored_records` 23,910 | **23,910** | hit |
| 8 | `commits` 284 | **285** | missed; re-pinned, with the red receipt kept |

**Clause 2 fired**: it read *"`diag_write_pack_total_ns` >= 195 ms **or** `diag_insert_objects_ns` >=
125 ms … workstream B is refuted as a treatment"*, and 146.36 is above 125. It is recorded as fired.
Its stated rationale, though, is only half right and the half that is wrong is the useful half:
`write_pack` **did** move, by 81.64 ms, so that term is per-call dominated as the treatment assumed;
`insert_objects` moved the **other way**. Rows moved from single-row statements into 18-row ones and
the bucket rose 13.04 ms: **the per-row cost of the rows that moved went from 5.28 to 5.80 µs.** A
wider statement is more expensive per row than the single-row statements it replaced. The candidate
mechanisms — a varying statement width defeating `prepare_cached`'s one-entry-per-chunk-size shape
(`sqlite/write.rs::object_insert_sql`), and a wider `Value` vector per row — are named as hypotheses
and **are not measured here**. What is measured is the sign and the size.

**Clause 5 fired**: `pack_bytes_written` moved by −210,016 against a declared +37,776 ± 5,000. The
registration priced the end offsets the grouping **adds** (9,444 × 4 = 37,776 B) and not the control
area and directory entry each of the **8,929 seals it removes** stops writing (8,929 × 28 = 250,012 B).
The arithmetic closes: −250,012 + 37,776 + 519 × 8 = −207,920, against a measured −210,016. This is
round 8's lesson a third time, in a new direction: price the count **and** the width, both ways.

**Clause 4 fired once and only once, on the declared counter.** The consequence run P1 failed exactly
one gate, `pipeline.commits 284 -> 285`, and reproduced every other pinned counter. The count was read
from that receipt, re-pinned once in `6a1b2d9f2`, rebuilt, and covered by one run — which is the
established procedure and not a second sample. **The mechanism is read off the source**: a wave's seals
share the wave's transaction (`cas/owner.rs::with_wave` sets `wave_held`, and `maybe_commit` returns
without committing while it is set), so only groups still open when the last wave ends pay their own
COMMIT. The whole-file lane used to be empty at that point by construction; now one group is not, and
`finish_inner` seals it with `wave_held` false. Exactly +1, and +1 rather than more for the same reason.

Clauses 1, 3 and 6 did not fire: `statements` 7,666 < 9,000, `operation_work_ns` 1283.16 < 1330, and
`tests/cas_reuse.rs` passes (its two whole-file objects now share a group *and* a pack).

## 4. Checks as run, and what was not run

- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast` — **630 passed / 0 failed**.
- `-p layerfs-storage` alone: **224 passed / 0 failed** across 33 binaries, including `cas_reuse`,
  `delta_payload`, `pack_watermark`, `multi_writer`, `visibility`, `persistence_failure`,
  `pack_locator`, `physical_formats`, `stored_payloads` and the new grouping case.
- `clippy --locked --manifest-path core/Cargo.toml --all-targets` clean;
  `fmt --manifest-path core/Cargo.toml --all --check` clean;
  `core/tools/check_product_boundary.py` **PASS** (194 production files).
- Harness release build from the repository root before each run; `--verify full --no-build`; one
  sample per case per arm; fresh `--out`.
- **Not run:** the harness's own suite for this round (it is not touched: the only harness change is
  the golden pin), the reference `crates/` workspace, any other harness case or lane, and any further
  sample of this arm. The `registry_negative` repair of the handoff's §7 is not attempted.

## 5. One fixture trap, named where it bit

`support::noise` is a **single fixed stream with no seed**, so `noise(n)` and `noise(m)` share a long
common prefix. Two whole-file records built from it are near-identical, the winner cache legitimately
proposes one for the other, and the pre-seal above then places the group early: the new grouping case
first failed with `packs_created + pack_appends = 2` for that reason — correctly, since a delta edge
had been proposed and preserved. The case now seeds its own payloads (`distinct_noise`) and says why.

## 6. Production LOC

**31558 -> 31616 (delta +58)** for `a1faf957e`, and **31616 -> 31616 (delta 0)** for the re-pin.
Method `python3 tools/production_loc.py --root <tree>`, first parent against each committed tree;
scope first-party product implementation, with tests, docs and the harness excluded.
