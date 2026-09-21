# Report — #219 round 20: the matched pair, and the arm it refutes

Pre-registration: [`pre-registration.md`](pre-registration.md), written before either arm was built or
run. Raw receipts under [`raw/`](raw/), copied out of `benchmark-results` because that tree is not
tracked by git. **Diagnostic pair, one sample per arm, both `PASS` 13/13, both `--verify full`.**

## The two arms

| | arm A, control | arm B, treatment |
| --- | --- | --- |
| source commit | `b7a0ab0a2` | `8c21b9f12` (product source `c296194a1`) |
| product difference | — | `PACK_LIMIT` 256 KiB → 1 MiB, `SCHEMA_VERSION` 10 → 11 |
| harness binary sha256 | `f387eeb387…` | `97b8a5a7e71f…` |
| run | `ns20-P2a-control-20260921T115000Z` | `ns20-P2b-treatment-20260921T115000Z` |
| gates / status | 13/13 `PASS` | 13/13 `PASS` |
| complete command | 1,658,319,583 ns | 1,731,185,625 ns |

Both binaries were built **in this worktree**, from clean checkouts at their own commits, sharing one
`target/` and one dependency cache; the arms were run back to back, control first. The harness's own
`src/` is byte-identical between the two commits.

**The pair is matched, which is the check the pre-registration registered.** The work `PACK_LIMIT`
cannot reach — no Store exists for any of it — agrees between the arms:

| pack-free work | arm A | arm B | difference |
| --- | ---: | ---: | ---: |
| `construct_ns` (untimed, before any Store) | 322,013,388 | 327,395,149 | **+1.67 %** |
| `construct_noise_ns` | 82,769,536 | 84,231,585 | +1.77 % |
| `span_build_ns` (tree build, writes no pack) | 67,551,417 | 67,759,917 | +0.31 % |
| `preparation_ns` | 509,974,958 | 507,773,000 | −0.43 % |

All four inside the registered 5 %.

## What the arm did, exactly as predicted

| quantity | arm A | arm B | registered | fired |
| --- | ---: | ---: | --- | --- |
| `pipeline.packs_created` | 1,270 | **295** | 280–320, point 295 | **exactly** |
| `space.pack_bodies_bytes` | 332,922,880 | **309,329,920** | −23 MB ± 4 MB | **−23,592,960** |
| `space.after.page_count` | 81,987 | **76,162** | −5,781 | **−5,825** |
| `pipeline.pack_appends` | 6,603 | **7,578** | ~7,578 | **exactly** |
| `pipeline.diag_commit_total_ns` | 410,536,288 | **360,876,709** | −30.7 ms | **−49,659,579** |
| `pipeline.pack_bytes_written` | 301,865,004 | 301,865,004 | unchanged | unchanged |
| `pipeline.commits` | 95 | 95 | unchanged | unchanged |
| `pipeline.statements` | 7,666 | 7,666 | unchanged | unchanged |
| the 13 pinned counters + the pinned digest | — | identical | no pin moves | **no pin moved** |

`309,329,920 = 295 × 1,048,576` exactly, and the 5,825 pages are the 5,760 pack pages the smaller
reserve accounts for plus 65 non-pack pages. **Every page-accounting prediction fired.**

## What it cost, and where the work went

`pipeline.operation_work_ns` is `accept_span_ns - finish_drop_ns`
(`src/ops/pipeline.rs:1260-1263`), and `finish_drop_ns` is the save's owner being taken apart —
**99.99 % of it `diag_release_connection_ns`, the save's connection close.**

| quantity | arm A | arm B | difference |
| --- | ---: | ---: | ---: |
| `operation_work_ns` — **the row's declared figure** | 943,318,416 | 936,571,167 | **−6,747,249 (−0.72 %)** |
| `accept_span_ns` — **the inclusive measured closure** | 1,046,366,750 | 1,125,224,833 | **+78,858,083 (+7.54 %)** |
| `teardown_ns` = `finish_drop_ns` | 103,048,334 | 188,653,666 | **+85,605,332** |
| `diag_release_connection_ns` | 102,993,041 | 188,585,125 | +85,592,084 |
| `establishment_ns` | 2,705,583 | 2,502,250 | −203,333 |
| closure = establishment + declared figure + teardown | 1,049,072,333 | 1,127,727,083 | **+78,654,750** |

`establishment + operation_work + teardown` equals the runner's own `phases.operation_ns` in both arms
(1,049,072,333 against 1,049,102,250; 1,127,727,083 against 1,127,756,500), so the closure is the same
region the runner times and the boundary arithmetic is exact.

**So the declared figure improves 0.72 % because 85.61 ms of work crossed the row's own declared
boundary; on the inclusive boundary the arm is 78.86 ms worse.**

### The movement did not merely cross the boundary

If the close were only the commit's flush happening later, the sum would be flat. It is not:

| flush-shaped total | arm A | arm B | difference |
| --- | ---: | ---: | ---: |
| `diag_commit_total_ns + finish_drop_ns` (this pair) | 513,584,622 | 549,530,375 | **+35,945,753** |
| the same total on the earlier session's pair | 512,768,957 | 597,330,374 | **+84,561,417** |

Both sessions' pairs put the treatment's flush-shaped total above its control's, and the two control
values agree to **0.16 %** across five hours of session drift while the two treatment values differ by
8 %. The sign of this is not a session artefact.

### What is *not* claimed

`diag_write_pack_total_ns` rose 27,018,349 ns in this pair and **fell** 3,976,500 ns in the earlier
one, and `diag_insert_objects_ns` rose 4,285,136 and fell 23,060,694 in the same two. Both are
`NOT_MEASURED` and no mechanism is claimed for either.

**`NOT_MEASURED`: why a connection close with fewer pages to write costs 85.6 ms more.** That is the
question this round hands on, and the round-19 engine instrument
(`core/benchmark/fs-bench-pro-storage-content/tests/commit_page_price.rs`) is the tool for it: it
already reads `CACHE_WRITE`, `CACHE_SPILL`, `CACHE_USED`, `page_count` and `freelist_count`, and it
does not read them **across a close**. The candidate readings — dirty pages being written at
`sqlite3_close` rather than at `COMMIT`, and the pager's overflow list being larger for a 256-page
overflow chain — are both readable with that instrument and neither is tested here.

## Verdict

The arm is **refuted at the row level** and reverted in `8efc798de`. The page accounting it was
registered on is confirmed exactly; the time it was predicted to buy is not there, and the direction of
the boundary movement is against it. A different `PACK_LIMIT` is a different, unregistered treatment
and is `NOT_MEASURED`.

## The larger finding, which is not about this arm

Arm A is the round-19 control row's product source, re-measured today. Its declared figure is
**943,318,416 ns against the round-19 row's 1,126,731,417 ns** — 183,412,999 ns apart, with the same
product source. The session moved `construct_ns` from 440,144,955 to 322,013,388 (**−26.8 %**) and
`span_build_ns` from 89,652,833 to 67,551,417 (**−24.7 %**), on work no lever in this campaign can
touch.

Every lever this campaign has priced is 20–90 ms. **Session spread is larger than the effects being
chased**: two rows measured hours apart on the same machine, same product, same harness driver differ
by 183 ms on the row's declared figure. A row is comparable to another row from a different session
only with the pack-free work beside it, and no receipt in this campaign publishes that comparison.
This is filed as its own finding in the ledger rather than folded into the arm's verdict.

## Checks as run

Both arms `PASS` 13/13 with `--verify full`; arm A's complete command 1.658 s and arm B's 1.731 s,
both inside the 15 s limit with no declared exception. `packs_created`, `pack_bodies_bytes`,
`page_count` and `pack_appends` reproduce the registered values exactly in arm B. **Not run:** any
sample beyond one per arm, any `PACK_LIMIT` other than 1 MiB, and any instrument on the connection
close — that is the next round's.
