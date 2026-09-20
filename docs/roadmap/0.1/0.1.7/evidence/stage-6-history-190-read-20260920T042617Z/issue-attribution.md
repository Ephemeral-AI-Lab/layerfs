## #190 attribution and treatment selection

Campaign: [`stage-6-history-190-read-20260920T042617Z`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/codex/history-data-access/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-read-20260920T042617Z) on base `9fe8eb290072d09594cde3ba67c3bbd1963d5e56` (clean isolated worktree, actual HEAD recorded per identity). Status: research, diagnostic evidence, not release admission.

**Attribution.** A bounded aggregate instrument (separate recorded patch, identical in both arms, no per-object map) decomposed the stride10 filesystem provider's own 9,019,067,220 ns:

| interval | ns |
|---|---:|
| Pack BLOB acquisition, both lanes (142,686 fetches, 12,080,963,142 B copied) | 3,928,606,618 |
| Metadata value-group catalogue SQL (278,927 statements) | 2,180,465,764 |
| Unattributed remainder | 1,339,076,683 |
| Object/locator SQL | 666,844,674 |
| Pooled value materialise/authenticate/decode/convert | 644,700,604 |
| Ordinary-lane group decompression | 177,929,427 |
| Control-area validation | 47,134,250 |
| Ordinary record decode call | 21,186,780 |
| Canonical leaf re-encode | 17,180,051 |
| Physical pooled body rebuild | 14,238,663 |
| Record framing | 13,122,420 |
| Physical pooled body decode | 1,717,437 |

Spans nest; the disjoint set closes against the provider elapsed. **NOT_MEASURED:** the ordinary lane's internal split, the per-wave `retained_pack_ceiling` read, the per-leaf identity hash, the per-row value-cache lookup, and the residual itself. Pack bytes are application-buffer acquisition, not physical disk traffic; exclusive SQL/codec/copy CPU is not claimed. Instrument v1 was superseded by a named defect (it charged the whole row-resolution loop to one container field); its sample is retained and is not the matched baseline.

**Treatment selection.** The pre-registered selected-group BLOB range read was **not** selected. 43.6% of the provider is pack acquisition, and the existing pack cache already serves 67% of 436,068 demands without touching SQLite, so a range route's removable share is not separable from the BLOB opens and range calls it adds without its own screen. Priority B's per-leaf pack-cache scope is a real 2.1–3.9 s target but changes a cache whose lifetime spans Store writes, so it needs its own invalidation contract and corruption tests: it stays a **proposal**. Priority C's pipeline work has no isolated measured effect: it stays a **proposal**.

The selected mechanism is the catalogue lookup, and the measured reason is a false premise in the code. `PoolReader::leaf_canonical_with_groups` memoised only the previous covering group, on the stated ground that a leaf's rows are in ordinal order. They are ordered by **serial** (`decode_pooled_body` enforces strictly increasing serial); the ordinal is a separate field. So the covering group changes about as often as the row does:

| quantity | stride10 | stride3 |
|---|---:|---:|
| leaf resolutions | 11,268 | 41,958 |
| distinct covering groups per leaf | 5.80 | 8.77 |
| catalogue statements per leaf | 24.8 | 31.8 |
| ns per statement | 7,817 | 7,827 |
| catalogue interval ns | 2,180,465,764 | 10,443,893,588 |

Treatment frozen before measuring: visit the rows in ascending **ordinal** order and write each value back at its own row's index, so the same memo answers one statement per distinct group. Same statement, same validation, no new SQL, no cache, no lifetime change, no format change, no bound change. Predicted statements: 65,337 / 368,074.
