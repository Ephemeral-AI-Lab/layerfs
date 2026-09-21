# Report — #219 round 20, step 1: the 1 MiB pack limit, and the correction it forced

Pre-registration ([`pre-registration.md`](pre-registration.md)) and its pre-run
[`Amendment 1`](pre-registration.md), both written before the first edit and before the first run. Raw
receipts under [`raw/`](raw/). The arm's gate sample is one sample, one arm, `--verify full`, and it is
reported here in full, **including the reason its headline must not be read as a 1 s claim.** The
matched pair that decides the arm is
[`../issue219-ns20-pair-20260921T115000Z/report.md`](../issue219-ns20-pair-20260921T115000Z/report.md).

## The arm's own row

`benchmark-results/issue219/ns20-P1-packlimit-20260921T112200Z`, `PASS` **13/13 gates**, **14/14 pinned
counters**, one sample, `--verify full`, sealed clean tree at `c296194a1`, complete command 1,766,158,875 ns
inside the 15 s limit with no declared exception.

| | control `ns19-S1-indexdrop-20260921T103500Z` | this row | difference |
| --- | ---: | ---: | ---: |
| `pipeline.operation_work_ns` — the declared figure | 1,126,731,417 | **929,513,251** | **−197,218,166** |
| target | 1,000,000,000 | — | |
| `pipeline.accept_span_ns` — the inclusive closure | 1,208,264,083 | 1,159,981,334 | **−48,282,749** |
| `pipeline.teardown_ns` = `finish_drop_ns` | 81,532,666 | 230,468,083 | **+148,935,417** |
| `pipeline.packs_created` | 1,270 | **295** | −975 |
| `space.pack_bodies_bytes` | 332,922,880 | **309,329,920** | −23,592,960 |
| `space.after.page_count` | 81,987 | **76,162** | −5,825 |
| `pipeline.pack_appends` | 6,603 | **7,578** | +975 |
| `pipeline.diag_commit_total_ns` | 431,236,291 | 366,862,953 | −64,373,338 |
| pinned values | 13 counters + 1 digest | **identical** | 0 |

Taking the row at face value would read: the arm moved the declared figure 197.22 ms and the row is
**929.51 ms, under the owner's 1 s bar**. That reading is wrong, and the row's own counters are what
say so.

## Why the headline is not a 1 s claim

`pipeline.operation_work_ns = accept_span_ns − finish_drop_ns` (`src/ops/pipeline.rs:1260-1263`), and
`finish_drop_ns` is the save's owner being taken apart, 99.99 % of it the save's connection close
(`diag_release_connection_ns`, `cas/store.rs:600`). On this pair that excluded term grew
**+148,935,417 ns** while the inclusive closure fell only **48,282,749 ns**. So **75.5 % of the
headline is work that crossed the row's own declared boundary**, inside the same closure the runner
times:

```text
control     establishment 2,251,208 + figure 1,126,731,417 + teardown  81,532,666 = 1,210,515,291
this row    establishment 2,686,458 + figure   929,513,251 + teardown 230,468,083 = 1,162,667,792
```

which is the runner's own `phases.operation_ns` in both rows (1,210,556,500 and 1,162,697,125).

The row is also **not a matched pair**. Work `PACK_LIMIT` cannot reach moved in the same direction, and
none of it can see a Store at all:

| pack-free work | control | this row | difference |
| --- | ---: | ---: | ---: |
| `construct_ns` (untimed, before any Store exists) | 440,144,955 | 321,098,127 | **−27.0 %** |
| `construct_noise_ns` | 113,345,687 | 84,200,562 | −25.7 % |
| `span_build_ns` (tree build, writes no pack) | 89,652,833 | 68,377,625 | **−23.7 %** |
| `preparation_ns` | 680,399,000 | 502,372,375 | −26.2 % |

## What the matched pair then measured

Same case, same session, same machine, two binaries differing only by this commit, back to back
(`ns20-P2a-control-20260921T115000Z` → `ns20-P2b-treatment-20260921T115000Z`). The pair's pack-free
work agrees to **1.8 %**, so it is matched.

| quantity | arm A control | arm B treatment | difference |
| --- | ---: | ---: | ---: |
| `pipeline.operation_work_ns` — the declared figure | 943,318,416 | 936,571,167 | **−6,747,249 (−0.72 %)** |
| `pipeline.accept_span_ns` — the inclusive closure | 1,046,366,750 | 1,125,224,833 | **+78,858,083 (+7.54 %)** |
| `pipeline.teardown_ns` | 103,048,334 | 188,653,666 | +85,605,332 |
| `pipeline.diag_commit_total_ns` | 410,536,288 | 360,876,709 | −49,659,579 |
| flush-shaped total (commits + close) | 513,584,622 | 549,530,375 | **+35,945,753** |

**The registered page accounting fired exactly and the arm is still a loss.** The commit term really did
fall — 49.66 ms, better than the registered 30.7 ms — but the close rose 85.61 ms, the flush-shaped
total rose 35.95 ms, and the inclusive closure rose 78.86 ms. The declared figure's 0.72 % is bought
entirely from the far side of the row's own boundary.

## The registered prediction, scored line by line

| registered | outcome |
| --- | --- |
| `packs_created` 1,270 → 280–320, point 295 | **fired exactly, 295** |
| `pack_bodies_bytes` 332,922,880 → −23 MB ± 4 MB | **fired, −23,592,960** |
| pages 81,280 → ~75,499 (−5,781) | **fired, −5,825** |
| `diag_commit_total_ns` −30.7 ms | **fired at −49.66 ms on the matched pair** |
| `operation_work_ns` → ~1,096.0 ms | **missed**: 936.57 ms on the matched pair |
| `pack_appends` 6,603 → ~7,578 | **fired exactly** |
| the 14 pinned values do not move | **fired, none moved** |
| `encoding/full.rs` routing does not move *on this row* | **fired** — Amendment 1, and the row's 128 KiB cutoff reaches none of the moved range |
| refuted if `packs_created` > 400 | did not fire (295) |
| refuted if `pack_bodies` falls < 19 MB | did not fire (−23.59 MB) |
| refuted if commit falls < 15 ms | did not fire |

The refutation list as written did not fire, because it was written against the *page accounting*, and
the page accounting was right. What refutes the arm is the boundary reading, which the pre-registration
did not register as a refutation and should have: **a term the row excludes, that a change can grow,
is a way to move the number without moving the work.**

## Verdict and action

**Reverted** in `8efc798de`; the product tree is byte-identical to `944d43864^`. The two bound-derived
tests are kept because they state their premises in terms of the format's two bounds instead of one and
pass at 256 KiB as well.

Two things are handed on, both named rather than modelled:

1. **Why the close costs 85.6 ms more with fewer pages to write** — `NOT_MEASURED`. The round-19
   instrument already reads `CACHE_WRITE`, `CACHE_SPILL`, `page_count` and `freelist_count` and does not
   read them across a close.
2. **The row should not be allowed to exclude the connection close**, or a change that moves work there
   will keep looking like a win. The exclusion is pre-existing (`a35d9aa3a`) and declared; this round is
   the first arm to exploit it.

## Environment honesty

One sample per case per arm, no retuning, no receipt overwritten, `--verify full` on every row, every
row `PASS` 13/13. Both the arm and both pair arms ran under the per-worktree measurement lock, and no
other measurement ran in this worktree during them. **Not run:** any `PACK_LIMIT` other than 1 MiB, any
second sample of the arm beyond the matched pair's, and any instrument on the connection close.

Production LOC: **31683 → 31683 (delta 0)** across the arm and its revert. Method
`tools/production_loc.py --root <tree>`; the arm changed comments and two constant values, and the
revert restores them.
