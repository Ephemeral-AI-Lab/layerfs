### #190 correction: operation convention and the provider residual

Two numbers in the three comments above are corrected. The raw receipts were never modified; only the derived analysis and the reported figures.

**1. Operation and region convention.** `phases-perf.operation_ns` equals the sum over **all** selected states, including the first one, and that is the convention the retained L40 table uses. The first revision of this campaign's derived JSON summed states 2..N, which made its absolute figures about 43–48 ms too small and incomparable with L40. Corrected figures:

| quantity | baseline2 | candidate2 | reduction |
|---|---:|---:|---:|
| **stride10 operation ns** | 21,905,900,168 | **19,888,424,711** | **2,017,475,457 (9.209735%)** |
| stride10 filesystem ns | 10,008,188,499 | 8,228,260,706 | 1,779,927,793 |
| stride10 store begin+accept+finish ns | 9,894,760,665 | 9,683,862,043 | 210,898,622 |
| **stride3 operation ns** | 56,597,343,506 | **49,501,013,748** | **7,096,329,758 (12.538274%)** |
| stride3 filesystem ns | 32,620,645,790 | 25,336,780,793 | 7,283,864,997 |
| stride3 store begin+accept+finish ns | 20,206,239,840 | 20,384,813,375 | −178,573,535 |

The rejected span shape is correspondingly 19,607,302,749 ns on stride10 (**−2,298,597,419**) and 58,481,684,581 ns on stride3 (**+1,884,341,075**). The instrumented provider interval, the catalogue statement counts and interval, the pack counters, the Store bytes and every verification figure are **unchanged** by the convention, because the first state performs no pooled read. The reduction moves by 4,543,959 ns (stride10) and 2,234,459 ns (stride3); both remain far above the one-second criterion. The superseded derived files are kept and labelled rather than deleted or rewritten.

**2. Provider residual.** The stride10 provider's unattributed remainder is **1,305,940,532 ns**, not 1,339,076,683 ns. The disjoint set — pack acquisition 3,928,606,618 + catalogue 2,180,465,764 + locator 666,844,674 + value decode 644,700,604 + group decode 177,929,427 + control 47,134,250 + decode call 21,186,780 + leaf encode 17,180,051 + physical rebuild 14,238,663 + record framing 13,122,420 + body decode 1,717,437 — sums to 7,713,126,688 ns against the 9,019,067,220 ns provider elapsed.

**How the errors were found.** An independent `rederive.py` recomputes every headline figure, binary hash and custody hash from the raw receipts without importing the analysis scripts. Its first run failed on exactly these two points; its retained output now has an empty failure list. Both errors and their magnitude are recorded in the campaign's [REVIEW.md](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/history-data-access/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/REVIEW.md) and [SUPERSEDED-ANALYSIS.md](https://github.com/Ephemeral-AI-Lab/layerfs/blob/codex/history-data-access/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-read-20260920T042617Z/SUPERSEDED-ANALYSIS.md). The disposition is unchanged: **retain**, PR #202, commit `4049e28b6`, production LOC 85,722 → 85,725 (+3).
