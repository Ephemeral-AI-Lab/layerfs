# Pre-registration — #219 round 15: the whole-file lane gets group boundaries

Written **before** any edit and before the first run of this arm. This is a **product** change in the
Store's pack grammar, the handoff's workstream B.

Control: **O1** `ns19-O1-stored-20260921T091015Z` (`4d8e6e2ab`, PASS, 13/13 gates, 14/14 pins):
`operation_work_ns` **1380.70 ms**, CPU 1397.14 ms, `diag_write_pack_total_ns` **204.88 ms**,
`diag_insert_objects_ns` **133.33 ms**, `diag_commit_total_ns` 455.52 ms, `statements` **16,590**,
`packs_created` 1,265, `pack_appends` 15,532, `stored_records` 23,910. This arm compares against that
row and **does not re-run it**.

## The one difference

**A whole-file group carries many records instead of exactly one**, and its body carries the record
boundaries the single-record form derived from the group's own extent.

*(The two counts below the line are the ones `raw/group-census.py` replays; its replay of the owner's
seal rule returns **519** whole-file groups where the first estimate in this document said 516, and
the predictions below are stated against the census. Both were produced before any edit, and the
census file is the artifact.)*

`cas::selection` seals the whole-file lane on every offer because `assemble::build_group` refuses
`records.len() != 1` for that lane, and the compact record can only be scanned if a group holds one of
them. The grouping counts, read out of the control row's own Store by
`raw/group-census.py`:

| lane | records | groups now | groups after |
| --- | ---: | ---: | ---: |
| whole-file | 9,444 | **9,444** | **519** |
| native | 14,466 | 7,125 | 7,125 |
| ordinary | 1,335 | 21 | 21 |
| total object groups | 25,245 | 16,590 | **7,665** |

`16,590` object groups **is** `pipeline.statements` 16,590, exactly: one seal is one `INSERT`
statement and one `write_pack` call. So the treatment takes **8,928 statements and 8,928 pack writes**
off the row — **8,925** of them — and it takes them off the **serial floor**, which is what a treatment has to do to lower
the ceiling rather than just the row.

**The grammar.** The whole-file lane's group body becomes `[count u32][end_i u32 …][record …]`, the
framing the native lane already uses (`assemble::frame_group`, read back by
`encoding::decode::framed_record`), with the compact record form unchanged inside it: each record is
still `[tag][base? 32][frame]`, its two length fields still dropped at assembly, because the group's
own end offsets now carry that information. A pack's directory for this lane is still starts-only, so
the reserved region stays 1,024 bytes and **no existing reader of any other lane changes at all**.

**Versioned, both directions.** Whole-file moves 14 → **17**. 11 (pre-stored-tag) and 14
(single-record stored groups) both still map to the same lane and are still read; a reader that
predates 17 refuses a new pack by version at `parse_header`, which is the same refusal the previous
round's move used. Native, ordinary, pooled and singleton are untouched.

## Price, count and width

| | count | width per unit | bytes |
| --- | ---: | ---: | ---: |
| today: whole-file seals | 9,444 | 1 record | 24.7 MB of writes |
| arm: whole-file seals | **519** | 18.2 records | 24.7 MB of writes |
| arm: group framing added | 9,444 records | **+4 B** (one end offset) | **+37,776 B** |

The width goes **up** by 4 bytes per record and the count goes down by 8,928 seals, twice over (8,925 of each). The
4 bytes are declared, not hidden: they are the group's end offset, and they are what makes a group
scannable without per-record lengths (8 bytes).

## Prediction, in the instruments' own units

| instrument | O1 | predicted | derivation |
| --- | ---: | ---: | --- |
| `statements` | 16,590 | **7,800–7,950** | 519 whole-file groups + 7,125 native + 21 ordinary + ~207 pooled groups (the pooled lane's are value groups, counted by the same counter's neighbour) |
| `packs_created` + `pack_appends` | 16,797 | **7,870–8,050** | one write per seal: 7,872 object groups + the pooled lane's |
| `diag_insert_objects_ns` | 133.33 ms | **75–90 ms** | the whole-file lane's 9,444 statements at 8.04 µs measured each, less the 9,444 rows that still have to be inserted |
| `diag_write_pack_total_ns` | 204.88 ms | **135–175 ms** | the whole-file lane's 9,444 writes at 12.2 µs measured each, less the 24.7 MB of bytes that still have to move |
| `pack_bytes_written` | 302,074,398 | **+37,776 ± 5,000** | the end offsets, nothing else |
| `operation_work_ns` | 1380.70 ms | **1250–1330 ms** | −55 to −127 ms of the store's two per-call terms |
| `stored_records` | 23,910 | 23,910 | unchanged: this change frames groups, not payloads |
| `commits` | 284 | 284 | the wave bound counts canonical bytes, not statements |
| all 14 pins + digest | — | unchanged | the change alters when a record is written, never what |

## What would refute it

1. `statements` >= 9,000 (the census says 7,872; a return above 9,000 is no grouping at all). The grouping did not happen, and the movement — whatever it is — is not the
   one claimed here.
2. `diag_write_pack_total_ns` >= 195 ms **or** `diag_insert_objects_ns` >= 125 ms. The two per-call
   terms do not move, so `write_pack` and `insert_objects` are per-byte and per-row respectively and
   **workstream B is refuted as a treatment** — the serial floor would then have to be attacked
   somewhere else entirely. Reported as a refutation, not repaired by re-running.
3. `operation_work_ns` >= 1330 ms with clauses 1 and 2 satisfied: the mechanism moved and the row did
   not, which is round 14's outcome a second time and would be reported as such.
4. Any of the 14 pinned counters moves, or `digest:filesystem_root` != `1d6fba29…3857847`, or the row
   is not PASS 13/13.
5. `pack_bytes_written` moves by more than the declared 37,776 B ± 5,000: something other than the
   group framing changed the bytes, which is not registered.
6. `tests/cas_reuse.rs` fails. It asserts that two whole-file objects saved in one operation **share a
   pack**; grouping must make that *more* true, never less.

## Declared risk, stated before the result

This is the handoff's "most bespoke part of the framing" and this round is not pretending otherwise:
`plan_lane`, `build_group`, `body_size`, the directory offsets, `decode_canonical` and
`delta::read::stored_base` all read the whole-file geometry, and three of the six are the read path.
The mitigation is that the grammar the group body adopts is *already implemented and already tested*
for the native lane - `frame_group`/`framed_record`, count plus end offsets - so the new code is a
second caller of a proven framer, not a new framing. `tests/stored_payloads.rs`,
`tests/physical_formats.rs` and a new grouped-whole-file case are the covering tests.
