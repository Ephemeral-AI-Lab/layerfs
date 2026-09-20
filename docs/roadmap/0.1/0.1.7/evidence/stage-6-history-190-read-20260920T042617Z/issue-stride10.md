## #190 stride10 matched pair

One sample per arm, matched instrumented baseline (`baseline2`, SHA256 `371ee8c28edee240f3627e16935b1f259cf045f30ba63890cf1ee6caaadc4fdb`) versus candidate2 (SHA256 `2e1fd754961ea50bcdc85ac07c9a1131c1c9745c61665638d69015414b567089`), identical instrumentation, fresh outputs, both under the shared global flocks. Quiet preflight passed with no named competitor.

| quantity | baseline2 | candidate2 | delta |
|---|---:|---:|---:|
| **Operation ns** | 21,862,271,710 | **19,840,252,294** | **−2,022,019,416 (−9.248899%)** |
| Filesystem ns | 10,007,435,416 | 8,227,499,706 | −1,779,935,710 |
| Provider ns (nested) | 9,019,067,220 | 7,254,356,455 | −1,764,710,765 |
| Store begin+accept+finish ns | 9,863,299,041 | 9,649,021,081 | −214,277,960 |
| Content ns | 1,623,555,291 | 1,622,363,667 | −1,191,624 |
| Complete command ns | 40,220,718,250 | 38,101,144,250 | −2,119,574,000 |
| Whole-invocation CPU ns | 32,387,651,000 | 30,298,280,000 | −2,089,371,000 |

| work | baseline2 | candidate2 |
|---|---:|---:|
| catalogue statements | 278,927 | **65,337** |
| catalogue interval ns | 2,180,465,764 | 584,740,322 |
| pack fetches / bytes | 142,686 / 12,080,963,142 | unchanged |
| value-group decodes | 65,337 | unchanged |

The statement count is **exactly** the arithmetic prediction (one per distinct covering group). CPU is whole-invocation user+system, not codec or SQL CPU. The Store interval improves too because the save's own pooled dependency resolution goes through the same function.

**Rejected treatment retained with its samples.** A first shape — one catalogue statement per leaf ordinal span (`candidate`, SHA256 `9958e0d1a09e2e1b7148b1f76b4be2f5cd84c620c9022c5028ded3df38d7abe0`) — gave stride10 19,564,726,749 ns (**−2,297,544,961**, larger) but regressed stride3 (next comment), because a span statement costs 101,010 ns on stride10 and 259,906 ns on stride3 against 7,817 ns for a single-group statement. It was replaced, not combined.
