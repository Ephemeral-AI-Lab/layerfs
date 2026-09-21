# Pre-registration — #219 round 20, step 1: `PACK_LIMIT` 256 KiB → 1 MiB

Written **before** the first edit and before the first run. This round is a **treatment**: it changes
one product constant, registers its prediction and its refutation, and measures the row it is claimed
to move. It is commissioned by
[`issue219-ns19-packlimit-and-scaling-handoff.md`](../../issue219-ns19-packlimit-and-scaling-handoff.md)
section 3 and by ledger entry L79.

## What is being changed, and what is not

`core/crates/layerfs-storage/src/policy.rs:110`, `PACK_LIMIT` **256 KiB → 1 MiB**. Nothing else in the
framing moves: `GROUP_LIMIT` (65,536), `GROUP_TARGET` (48 KiB), `GROUP_COUNT_LIMIT` (256),
`RECORD_COUNT_LIMIT`, the five framing versions, the directory grammars and every stored byte stay
exactly as they are. `GROUP_LIMIT` deliberately does **not** move with it: L78 measured that raising it
*alone* makes the tail worse (half a bigger group per pack), so this arm carries one difference.

`SCHEMA_VERSION` moves **10 → 11**. It is not part of the prediction; it is the price of the change,
and it is paid because it is the mechanism that catches the incompatibility rather than a reader that
fails late: `pack/layout.rs:412` and `:525` refuse a pack longer than `lane.pack_limit()`, so a 1 MiB
pack is unreadable by a 256 KiB build. A version-10 Store is refused at open by the check below, before
any pack is read, exactly as every earlier version is. `PACK_LIMIT` is **not persisted** —
`store_policy` carries `format_profile`, `small_file_threshold_bytes`, the three `*_delta_max_depth`
columns, `max_concurrent_writes` and `publication_sequence` — so there is no policy migration to write.

## Why this is expected to move the row

L78 (round 19) settled that the row's commit term is a **page-write** term, and that the capacity a
pack row reserves is **flushed even where it holds no content**: a `zeroblob`'s overflow chain still
requires every page's next-page pointer to be written. `packs-zeroblob` and `packs-full` write the same
81,441 pages and cost 490.07 and 493.62 ms of transaction time — 0.7 % apart for 31 MB more payload.

The lever is the *count of packs*, not their contents. `pack/layout.rs:268-278`
(`pack_capacity`) gives every lane but Singleton its **full** `lane.pack_limit()` at row creation, and
`sqlite/write.rs:88-95` zero-fills the row to it with `zeroblob(?2)`. A pack is closed only when the
next group no longer fits (`pack/placement.rs:125-128`), so a pack's dead tail averages about half a
group — **24,455 B**, round 17's measurement — and the *total* tail is that average times the pack
count. Four times the pack holds four times the groups, so the same content needs roughly a quarter of
the packs and a quarter of the tail.

| | packs | reserved | content | tail |
| --- | ---: | ---: | ---: | ---: |
| today, `PACK_LIMIT` 256 KiB | 1,270 | 332,922,880 B | 301,865,004 B | **31,057,876 B** |
| `PACK_LIMIT` 1 MiB, same `GROUP_LIMIT` | ~295 | ~309.3 MB | 301,865,004 B | **~7.4 MB** |

## The prediction

Measured control: `benchmark-results/issue219/ns19-S1-indexdrop-20260921T103500Z`, one sample,
`--verify full`, sealed clean tree at `e3a46d74b`.

| quantity | control | predicted | as |
| --- | ---: | ---: | --- |
| `pipeline.packs_created` | 1,270 | **280–320**, point 295 | `counters` |
| `space.pack_bodies_bytes` | 332,922,880 | **−23 MB ± 4 MB**, point ~309.3 MB | `resources.space` |
| pages the packs reserve | 81,280 | **~75,499** (−5,781) | reserve ÷ 4,096 |
| `pipeline.diag_commit_total_ns` | 431,236,291 | **−30.7 ms**, point ~400.5 ms | pages × 5,306 ns |
| `pipeline.operation_work_ns` | 1,126,731,417 | **~1,096.0 ms** | the row |
| `pipeline.pack_appends` | 6,603 | **~7,578** (unpinned) | total groups − packs created |

**The 13 pinned counters and the 1 pinned digest do not move**, and that is a registered prediction
rather than an omission. Every pin is content-, construction- or tree-derived: `batches`, `bindings`,
`chain_objects`, `commits`, `content_bytes`, `content_objects`, `declared_*`, `inserted`,
`largest_batch_bindings`, `metadata_objects`, `objects_emitted`, `reused`, and `digest:filesystem_root`.
`pipeline.packs_created` and `pipeline.pack_appends` are **published but not pinned** on this row
(`tests/golden/expected.tsv:1379-1402`), and the commit cadence is bounded by
`TRANSACTION_ROW_LIMIT` / `TRANSACTION_CANONICAL_BYTES_LIMIT` at a step boundary
(`cas/lifecycle.rs:185`, `:278`), never by a pack boundary.

### A priced surface the handoff names that does **not** apply, by reading

The handoff prices `encoding/full.rs:274`, `:297` — ordinary-versus-singleton routing — as a surface
that will move and force a re-pin. Read against the policy the row actually runs, it cannot move here.
The routing admits a compact whole-file record to the `WholeFile` lane when
`HEADER_LEN + directory_capacity(WholeFile) + body <= capacities.pack_limit` — **24 + 1,024 + body**
against 262,144 today and 1,048,576 after. A whole-file object is bounded by the construction cutoff:
`StoragePolicy::frozen_default()` carries `small_file_threshold_bytes = 131,072`
(`layerfs-content/src/policy.rs:14`), so the largest canonical whole-file object is
`131,072 - 1 + 23 = 131,094` B, and its group contribution is at most ~132,200 B. **Every whole-file
record already fits a 256 KiB pack**, so no record sits between the two limits and the routing is
saturated at both. The prediction of a moved routing is therefore registered as **not fired**, and the
handoff's stated consequence is corrected by reading rather than by measurement.

## Refutation

The prediction is **refuted** if any of these holds on the round's own row:

1. `packs_created` stays above **400**;
2. `space.pack_bodies_bytes` does not fall by at least **19 MB**;
3. `diag_commit_total_ns` falls by less than **15 ms** (half the predicted saving);
4. any of the 14 pinned values moves — which would mean the change reached work it was not predicted
   to reach, and the pin is then a finding rather than a nuisance;
5. `operation_work_ns` rises, or the row is not PASS.

A result between the prediction and the refutation line is reported as **partial**, with the measured
number, and no mechanism is claimed for the difference.

## The command, and the checks

```
cd core/benchmark/fs-bench-pro-storage-content
python3 runner.py perf --case pipeline-namespace-10000 --verify full \
    --out benchmark-results/issue219/ns20-P1-packlimit-<UTC>Z
```

Fresh `--out`, one sample, one arm, no retuning, no second sample. The harness rebuilds the binary
(`--no-build` is deliberately **not** passed: the product source changed, so the previous binary is a
different identity). Before the measurement: `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked --no-fail-fast`, `clippy --all-targets`, `fmt --all --check`, and
`core/tools/check_product_boundary.py`. The whole-pipeline row is **not** a substitute for those
checks and no gate is claimed from it.

## What is not claimed

No claim about the read path's cost, about lanes the row does not exercise, about a `PACK_LIMIT` above
1 MiB, and no claim that the saving closes the 126.73 ms gap. The measurement is one sample on one
machine, and the page price it is multiplied by (5,306 ns) is L78's arithmetic agreement between two
instruments, not a paired measurement taken here.

## Amendment 1 — pre-run: the routing surface, scoped to the row's cutoff

Written after the code edit and **before any measurement run**.

The claim above — that `encoding/full.rs`'s ordinary-versus-singleton routing does not move — is true
**of this row** and is not true in general, and what decides that is the construction cutoff rather
than the pack limit. The routing admits a compact whole-file record while
`HEADER_LEN + directory_capacity(WholeFile) + body <= capacities.pack_limit`; with `HEADER_LEN` 24, the
whole-file lane's reserved directory region 1,024 (`4 * GROUP_COUNT_LIMIT`) and a stored compact
record's group body costing `raw + 9` plus its 8 bytes of group framing, the condition is exactly

```text
raw + 1,065 <= pack_limit
```

- at the row's cutoff — 128 KiB, so `whole_file_raw_limit = 131,071` — every whole-file record is
  admitted at 262,144 and at 1,048,576 alike, so **nothing on this row moves**, which is the
  prediction above;
- at the largest cutoff the policy admits — 1 MiB, so `whole_file_raw_limit = 1,048,575` — the
  condition is `raw <= 1,047,511` after the change against `raw <= 261,079` before it, so the top
  1,064 bytes of the whole-file range take the compact lane where they used to be refused it.

That second case is a real routing change and is a routing change the row does not reach. It is
recorded here because a test that pinned the old boundary as a written-down number caught it:
`stored_payloads.rs:216` (`a_payload_larger_than_a_compact_pack_is_stored_in_the_singleton_lane`)
built its payload as 400,000 B at a 1 MiB cutoff, which beat the old pack limit and no longer beats
this one. It now takes the construction's own ceiling (`capacities.whole_file_raw_limit`), so the case
states "a payload a compact pack cannot hold" in terms of both bounds rather than in terms of one.
Three tests carried the old limit and were updated with it, and one of them was not a literal:
`physical_formats.rs:151` (the lane table's pinned limits, 256 KiB → 1 MiB);
`pack_locator.rs:499`, whose probe body was `pack_limit / 4` — 256 KiB against the ordinary lane's
64 KiB body ceiling, which the lane refuses before any fit arithmetic runs, and which now uses the
lane's own body limit with the count of bodies that fit derived from both bounds; and
`stored_payloads.rs:216` above.

No part of the row-level prediction changes, and no refutation condition is relaxed.
