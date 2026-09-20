# S1 — measured-operation span attribution

> Status: Research; informative and not a product contract.

## Scope and custody

This report owns the harness span decomposition only. No product source, policy, default, worker count or codec level was changed. The opt-in `LAYERFS_HISTORY_PHASES=1` records existing public filesystem phases and coarse harness scopes. Its 53-state limit prevents exceeding the telemetry 1,024-node bound. The default product route and policy remain the same, but every trace gains counters and the instrumentation introduces helper/timer overhead; no byte-identical default trace or zero-overhead claim is made.

The original `/tmp/history-stride10/` files were copied byte-for-byte into [archive-original](archive-original/). This is an archival copy, not a fresh measurement, not an identity-matched rerun and not release evidence. The source/binary identities and cache preconditioning of that original invocation cannot be reconstructed from these three files alone. Fresh detailed diagnostic: **NOT_RUN**, pending an explicit diagnostic wall-budget ruling; the known stride10 wall exceeds the current 15/25-second command ceiling. No new runtime attribution is claimed below.

| Archived file | SHA-256 |
| --- | --- |
| phases-perf.json | `6b36894ff419f0089b530bf881ea5875176b1f01ab8db8f3bc8a83375e761985` |
| timing.json | `b20825c181090c14bf90e294e85c9f1c92b1f1600100dd5bae92ebb23182f139` |
| trace.jsonl | `979c2cc703238f5d9c8bb79325adc1ceac04b18f530d8e1763a07a22274c43f3` |

## Existing coarse decomposition — raw archival evidence

The handed-over 94% gap described only the timing tree. The same trace already contains `input_ns`, `build_ns`, and `save_ns` for every state. Those disjoint elapsed counters reduce the unassigned operation time to **93,006,002 ns**. The `save_ns` envelope already includes `storage.begin` and `storage.finish`; adding those child durations again would double count.

| Mechanism / boundary | Sum, ns |
| --- | ---: |
| content | 1,115,152,085 |
| store.create | 2,469,708 |
| input | 65,188,166 |
| build | 23,520,347,667 |
| save | 7,769,904,041 |
| state.residual | 93,006,002 |
| **Operation sum** | **32,566,067,669** |

Exact equation: `32,566,067,669 = 1,115,152,085 + 2,469,708 + 65,188,166 + 23,520,347,667 + 7,769,904,041 + 93,006,002`. Arithmetic residual after explicitly retaining unassigned time is 0; mechanism attribution residual is **93,006,002 ns**.

The source receipt root is **44,831,509,917 ns**, so its root-minus-children is **12,265,442,248 ns**. The handoff wrote 44,832,509,917 ns, a 1,000,000 ns transcription difference. This report uses raw timing.json.

Ordinal is the selection-local state number in the trace (1–17); corpus checkpoint is the selected index, derived from the fixed stride10 selection. All timing columns below are exact integer nanoseconds.

| State | Checkpoint | Operation | Content | Store create | Input | Build/update | Save envelope | Residual |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 37022000 | 4696250 | 2469708 | 340291 | 531750 | 28739167 | 244834 |
| 2 | 11 | 194775084 | 18745917 | 0 | 1120167 | 73406916 | 99639833 | 1862251 |
| 3 | 21 | 277627000 | 25980000 | 0 | 1166583 | 125935458 | 122972584 | 1572375 |
| 4 | 31 | 438791750 | 27665958 | 0 | 1477084 | 271863750 | 135908917 | 1876041 |
| 5 | 41 | 903992167 | 50678667 | 0 | 2849417 | 556033125 | 291628791 | 2802167 |
| 6 | 51 | 942292000 | 46803125 | 0 | 2472416 | 616797625 | 273296958 | 2921876 |
| 7 | 61 | 1333462416 | 53088042 | 0 | 3323416 | 894872625 | 378592500 | 3585833 |
| 8 | 71 | 1351428917 | 38340708 | 0 | 2723667 | 1043704334 | 263737833 | 2922375 |
| 9 | 81 | 1564124750 | 56873792 | 0 | 3450750 | 1113079125 | 386441917 | 4279166 |
| 10 | 91 | 1694802708 | 51117375 | 0 | 3072750 | 1277410833 | 359594958 | 3606792 |
| 11 | 101 | 2919255834 | 83469250 | 0 | 5054042 | 2232000542 | 592539041 | 6192959 |
| 12 | 111 | 2931409834 | 91436792 | 0 | 5463250 | 2191169625 | 632330167 | 11010000 |
| 13 | 121 | 3243867708 | 90358875 | 0 | 5223250 | 2456608125 | 684358458 | 7319000 |
| 14 | 131 | 3971637542 | 122183917 | 0 | 7038583 | 2918463000 | 912738208 | 11213834 |
| 15 | 141 | 3906736584 | 131850417 | 0 | 8020666 | 2777986542 | 976061209 | 12817750 |
| 16 | 151 | 3284932750 | 108359000 | 0 | 5856750 | 2376118625 | 783523083 | 11075292 |
| 17 | 157 | 3569908625 | 113504000 | 0 | 6535084 | 2594365667 | 847800417 | 7703457 |

## Instrumentation boundaries

- `content`: existing C1 construction envelope, including harness signature generation and chunk predecessor reads. It is not pure codec CPU.
- `harness.predecessors`: prior-content construction when selected, advisory candidate selection, depth ranking and map maintenance.
- `harness.input`: `filesystem_input` assembly.
- `filesystem`: existing public build/update operation, with existing product children `validate`, `directories`, `references`, `inodes`, `cleanup`, `root.encode`.
- `storage.accept_loop`: consumer object clone, advisory attachment, and `SaveOperation.accept`. Includes encoding/index/transaction work performed by accept; no claim of pure codec time.
- `harness.index`: producer correspondence/signature index maintenance after accept.
- Existing `storage.begin`, `storage.finish`, and first-state `store.create` are unchanged.
- `state.residual` and `filesystem.residual`: exact parent minus immediate child sum, retained as unassigned work. No guessed redistribution.

`references` covers reducer.finish, while lazy FinalRows consumption occurs inside `inodes`; release traversal and some directory-value work are outside the six named filesystem phases. Therefore these labels cannot alone isolate total ordering or total read-back CPU. Source work counts accompany them: validation reads/entries, reference scans/spills/runs, sorted page reads, StoreProvider group decodes and connection opens, and SaveOutcome commits. Charged transaction bytes have no public outcome counter and remain unavailable.

No oracle read-back was added to the performance child. Base-tree read-back is intrinsic to build/update and appears in those inclusive phases; codec decode CPU is not independently timed.

## Reproduction and validation

The diagnostic switch is additional to all existing history switches, which the campaign must record individually. The harness rejects an enabled detailed recording beyond 53 states; stride1 is not part of this campaign. A 53-state detailed run uses at most 850 nodes (53 × 16 + root + first Store create), below 1,024.

`analyze_spans.py` reads one output directory and emits exact per-state decomposition; it never pools runs. Its self-check constructs nested spans and proves each nanosecond is counted once, then rejects impossible negative residuals.

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/squad-s1/analyze_spans.py --self-test
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/squad-s1/analyze_spans.py /absolute/new/diagnostic-output
```

Self-check: PASS. The first archive-analysis attempt rejected duplicate non-counter `note` keys; the reader was corrected to enforce uniqueness only for per-state keys it consumes, then successfully reconciled all 17 states. No measurement was performed during this script check. The campaign owner reports locked release build and harness Cargo tests PASS (logs in `../checks/`). Warning-denying Clippy FAILS at 24 sites, including existing `prior_roots`, `prior_unavailable`, `type_complexity` and `never_loop` findings; formatting check FAILS across the existing workspace. These are retained failures, not suppressed. No CI/preflight claim is made. Measurement lock/quiet status and any later fresh diagnostic are coordinated by the campaign owner.

## Unresolved attribution

The original trace supports a 23,520,347,667 ns **build/update envelope**, not the claim that validation or one named tree walk consumed that entire envelope. Detailed attribution and like-for-like causal deltas are NOT_MEASURED pending fresh diagnostic artifacts. Unknown original cache residency precludes treating the copied artifact as cold evidence.

## Review correction — instrumentation v2

The independent reviewer identified that the first instrumentation closure dropped
`prior_roots` and `prior_missing` when the predecessor phase returned, earlier
than the original outer state scope. V2 returns these maps alongside `bases` and
binds them in that outer scope, preserving their lifetime through the state body.
The original [harness-instrumentation.diff](harness-instrumentation.diff) remains
as reviewed evidence; [harness-instrumentation-v2.diff](harness-instrumentation-v2.diff)
records the corrected instrumentation. No product source changed. The earlier
build/test results above belong to v1; v2 rebuild and focused checks are pending
campaign-owner execution and must not be inferred from v1's results.

## Final v2 validation

The campaign owner subsequently completed the v2 checks: locked release build
**PASS**, measured build command wall **8,151,938,750 ns**; harness tests
**PASS, 116 tests**; warning-denying Clippy **FAIL, 22 findings**, whose source
statements are all present in HEAD. Logs are retained under `../checks/*-v2.*`;
`../built-artifact-v2.json` records the source and binary seals. This supersedes
the pending-v2-check status above without rewriting the v1 check history.
Formatting remains a recorded failure. Fresh detailed runtime attribution remains
**NOT_RUN**; successful compilation and tests are not a measurement.
