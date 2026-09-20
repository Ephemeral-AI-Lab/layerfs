# #190 — retained-history operation slowdown: evidence and unresolved cause

> Status: Research; informative and not a product contract.

**Disposition: retain the tripwire and keep the cause open.** The archived core operation spends **23,520,347,667 ns in build/update_filesystem**, which is the dominant measured envelope. The supplied “94% unnamed” statement overlooked counters already in the raw trace. The coarse decomposition leaves **93,006,002 ns unassigned**, not 30.6 seconds. This does not establish which tree algorithm or storage read mechanism caused the cost, and it does not causally allocate the cross-generation excess. No product fix is authorized or implemented.

This is an **archival audit and instrumentation handoff**, not a newly measured diagnosis. The requested fresh stride10 and stride3 diagnostics are **NOT_RUN pending a declared diagnostic wall ceiling**. The user was asked to resolve the conflict between the handoff's 15/25-second instruction and its explicit rerun of a known 45-second selection. The earlier lane specification lifts the generic limit but requires a prospective numeric ceiling. No ceiling was silently invented and no existing miss was converted to a pass. See [S5 protocol](squad-s5/PROTOCOL.md) and [all diagnostic dispositions](diagnostic-register.json).

## Evidence ownership and identities

| Squad | Authority used here | Output |
| --- | --- | --- |
| S1 | Exact core archival spans and harness instrumentation | [SPAN-ATTRIBUTION.md](squad-s1/SPAN-ATTRIBUTION.md) |
| S2 | Current product call graph, bounds and limitations | [TREE-UPDATE-PATH.md](squad-s2/TREE-UPDATE-PATH.md) |
| S3 | C2 levers, old recompression diagnostics, missing measurements | [C2-COST.md](squad-s3/C2-COST.md) |
| S4 | Original v0.1.6 receipts and source boundaries | [V016-COMMIT-COMPOSITION.md](squad-s4/V016-COMMIT-COMPOSITION.md) |
| S5 | Prospective protocol and unmatched comparisons | [PROTOCOL.md](squad-s5/PROTOCOL.md) |
| Independent reviewer | Separate re-derivation, objections and limits | [REVIEW.md](REVIEW.md) |

The campaign source is `9f35c49ad62956f131dc2676787f99d69659686e`. Product source is unchanged from `795fb1a2f792742a2b19fdb82d98e6fd8a8b0470` to HEAD (`git diff 795fb1a2f HEAD -- core/crates` is empty). That fact does **not** bind an unsealed historical binary to either source. The original core raw timing/trace/phases were copied unchanged from `/tmp/history-stride10` into S1's archive with SHA-256 hashes. They contain no contemporaneous source/binary identity receipt. They are retained evidence to audit the handoff, not a qualifying reused PASS or a replacement for the requested fresh run.

S4 retained compressed copies of the historical v0.1.6 receipts and their original source hashes. The host binary identifies `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`; the orchestrator identifies `7fab1027a0061e8b932345d4fcd6ac22a089b155`. Product and benchmark source diffs between them are empty. Source seal is `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a`, product seal `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd`. The historical cache declaration is `fresh-store-existing-os-cache-uncontrolled`, with `admission_eligible=false` in its own receipt. No original receipt was rewritten.

Corpus manifest SHA-256 independently rechecked: `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`. Tip is `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. S4 confirms the historical selection against current harness pins: `1,11,21,31,41,51,61,71,81,91,101,111,121,131,141,151,157`. Same corpus selection does not make different implementations or timing boundaries a matched pair.

## Exact arithmetic, without overlapping spans

S1's table has every state's exact nanoseconds. Its top-level partition is:

| Named envelope | Archived core ns | Measurement and limit |
| --- | ---: | --- |
| Content construction | 1,115,152,085 | `content` child; includes harness signatures and predecessor reads; not pure codec CPU |
| Store creation | 2,469,708 | State 1 `store.create` child |
| Harness input assembly | 65,188,166 | Sum of per-state `input_ns`; includes complete current-tree path scan |
| Filesystem build/update | 23,520,347,667 | Sum of per-state `build_ns`; includes public tree work, provider reads and small harness setup |
| Save envelope | 7,769,904,041 | Sum of `save_ns`; includes begin, accept, producer index maintenance and finish |
| Unassigned state residual | 93,006,002 | Parent minus the five disjoint envelopes; no guessed attribution |
| **Total** | **32,566,067,669** | Equals the 17 state children and `phases-perf.operation_ns` |

`32,566,067,669 − (1,115,152,085 + 2,469,708 + 65,188,166 + 23,520,347,667 + 7,769,904,041 + 93,006,002) = 0`.

Arithmetic closes by retaining a residual; **mechanism attribution has not reached zero**. The entire 23,520,347,667-ns build envelope and much of the save envelope still need internal attribution. It would be misleading to call only the 93,006,002 ns the remaining root-cause uncertainty.

Within save, existing `storage.begin = 8,565,500 ns` and `storage.finish = 843,319,042 ns` are subsets. Their subtraction leaves `6,918,019,499 ns` for accept, producer index and intervening work. Do not add these three again to the top-level total.

S4's exact external Commit sum is **11,370,679,212 ns**, rather than the rounded 11.371 seconds. Consequently:

```text
historical excess = 32,566,067,669 − 11,370,679,212 = 21,195,388,457 ns
descriptive ratio = 32,566,067,669 / 11,370,679,212
```

This is an unmatched tripwire difference, not a matched estimate of product regression. The rounded denominator in the handoff produces a different excess by 320,788 ns; the raw denominator is used here. No averaging, smoothing or repeated sample selection is used.

Published squad disagreement: S2's closing caveat and one S3 paragraph retain **21,195,067,669 ns**, obtained from the rounded 11.371-second denominator. S4 and this synthesis use **21,195,388,457 ns** from the retained raw receipts. The independent reviewer identified the 320,788-ns discrepancy. The squad documents are retained as authored; their disagreement is not silently merged away.

The core raw root is **44,831,509,917 ns**, correcting the handoff's 44,832,509,917 ns transcription. Root minus operation is exactly **12,265,442,248 ns**. Invocation is 44,999,165,042 ns. CPU user/system are 30,614,677,000 / 9,042,045,000 ns over the broader instrumented work window; they cannot be assigned to tree/codec/SQL from this receipt. RSS 264,536,064 B is lifetime; heap peak incremental 75,411,887 B is the largest state figure. Neither is codec-local memory.

## Boundary reconciliation and hypothesis disposition

| Hypothesis | Supported observation | Status and missing evidence |
| --- | --- | --- |
| H1: base-tree walk | Build/update is 23,520,347,667 ns. Source shows sorted sibling acquisition before untouched-subtree checks, conditional validation walks and repeated base demands. | **Plausible, not causally resolved.** Source does not prove an unconditional full-base walk. Detailed fresh phases and counters are NOT_RUN. |
| H2: old Commit excluded content/tree | Historical receipt already records content 9,394,444,501 ns and namespace 342,355,542 ns inside Commit. | **That specific explanation is falsified.** Boundaries still differ: old content overlaps streaming C2 admission; new save is sequential. These numbers cannot be subtracted as like-for-like component deltas. |
| H3: codec level/workspace | Current source constants are PAYLOAD_LEVEL=3, GROUP_LEVEL=19. Archived group recompression logs exist. | **Live matched effect NOT_MEASURED.** GROUP_LEVEL is private; changing it would violate no-product-change. Historical recompression is not lane CPU. |
| H4: worker count | Core driver serially calls construction. Legacy predecessor path explicitly sets one construction producer. | **No quantified worker explanation.** Reference actual runtime count is absent; environment-only change has no treatment in core. Matched diagnostic NOT_RUN. |
| H5: persisted similarity index/chunk cursor | Save counters describe decisions; provider reads and candidate acquisition are real work. | **CPU effect NOT_MEASURED.** No index/codec CPU split or public index-disable lever. Changing producer declarations would not isolate persisted-index cost. |
| H6: save cadence/whole-pack accounting | `accept` can execute transactions; public SaveOutcome.commits is now collected by instrumentation. | **NOT_MEASURED in a fresh history run.** Original trace lacks commit count; charged transaction bytes are not public. “Every 28 objects” is not established for this lane. |

Further falsified explanations are narrow and numerical:

- The “94% cannot be decomposed from existing data” premise is false: existing counters give the six-part exact partition above.
- The harness's complete input-tree scan cannot directly account for the entire excess: its measured envelope is only 65,188,166 ns, smaller than 21,195,388,457 ns. This does not exonerate base reads inside product tree operations.
- Extra content construction alone cannot account for the entire excess: the core content envelope is 1,115,152,085 ns, and legacy Commit already includes content work.
- Treating 843,319,042 ns of `storage.finish` as total C2 save cost is false: the full save envelope is 7,769,904,041 ns. Codec/index/transactions execute in accept as well.

S4 reconstructs old boundaries from exact source. Fixture install and Exec are outside Commit; Commit includes capture/plan, content plus streamed admission, namespace, publication and checkpoint. New children include caller-side input/predecessor/index work and tree plus sequential save but exclude corpus disk reading. Old Commit has IPC/spool/snapshot work absent from core. **No measured additive composition correction has been established for the 21,195,388,457-ns excess.**

The old total wall is 98,009,764,333 ns; work wall is 96,444,055,041 ns. Contrary to the old report's prose, its outer wall includes setup/cleanup. Core invocation and old wall are different lifecycle scopes; retain both without promoting their ratio to a paired speedup claim.

Storage scope also differs: the old headline is retained directory bytes, 49,344,512 allocated / 49,315,940 apparent. Its database alone is 49,336,320 / 49,315,840. Core's archived database is 49,192,960 / 49,053,696. Core remains smaller on the database-only comparison, but the directory overhead must not be called codec savings. Historical canonical object totals also differ (+310 objects and +358,173 canonical bytes for core); same input history does not imply identical canonical object populations.

## Implemented diagnostic and validation

Only [the history harness](../../../../../../core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs) was edited. `LAYERFS_HISTORY_PHASES=1` opts into existing public `build_filesystem_timed`/`update_filesystem_timed`, six product phases and named predecessor/input/accept/index scopes. Additional per-state counters report validation reads, sorted pages, reference spills, StoreProvider connections/group decodes and SaveOutcome commits. No codec default, worker rule, source input or product implementation changed. No O4 oracle work was inserted into performance. The default trace gains counters and helper overhead, so “unchanged” refers to product route and policy values, not byte-identical output or timing.

The public `references` phase does not enclose all ordering work: lazy reference reads occur under `inodes`; release and setup remain in filesystem residual. Base read-back remains nested in tree phases, without independently measured read/decode CPU. These limitations remain visible even after a future detailed run. Opt-in recording is bounded to 53 states; stride1 is not selected.

The independent reviewer found that the first instrumentation closure shortened the lifetime of two predecessor maps. S1 corrected this before any measurement by returning both maps into the original state scope. Both harness diffs are retained; the final source and binary are sealed in [built-artifact-v2.json](checks/built-artifact-v2.json). Final binary SHA-256: `3cc3e1ad2ad4f663c4a7b1cdd5e4ab37090eb73b9a5a88535995a4337430a025`. The review objection and correction are not hidden.

Prospective commands and lock/cache conditions are in S5. All eight original history switches were unset in the starting environment, as was the worker variable; a future diagnostic explicitly exports `LAYERFS_CONSTRUCTION_WORKERS=1` and `LAYERFS_HISTORY_PHASES=1`. [Initial context](initial-context.json) and its [clarification](context-clarification.json) preserve this distinction. No fresh performance or verification process was started. Existing corpus preparation was reused; no fixtures were regenerated, no mutated Store supplied as setup, no caches purged or paths pre-touched for a timed run.

Checks, serial under both core and legacy measurement locks, retained under [checks](checks/):

| Check | Result |
| --- | --- |
| `cargo +1.85.1 build --release --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` | PASS v1, 12,331,748,417 ns; PASS final v2, 8,151,938,750 ns; incremental target/dependency reuse; final existing unused-mut warning retained |
| `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` | PASS, 116 tests in each required source revision; final v2 wall 20,511,384,791 ns |
| `cargo +1.85.1 clippy --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked --all-targets -- -D warnings` | FAIL v1 at 24 locations; FAIL final v2 at 22 locations, each statement present in HEAD (clippy-existing-sites-v2.json). V2 map bindings remove two unused-variable diagnostics; no suppressions added |
| `cargo +1.85.1 fmt --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --check` | FAIL; existing formatting differences across the harness |
| Python `test_history_corpus.py` | PASS |
| Product boundary guard and its Python self-tests | PASS |
| S1 attribution analyzer self-test | PASS |
| `git diff --check` | PASS at check time |
| Full core Cargo tests/Clippy/fmt | NOT_RUN: no product change; explicit independent harness workspace was checked |
| Fresh retained-history performance/verification | NOT_RUN; no admission claim |

No CI, aggregate preflight, commit, push or product change was performed. A per-commit production LOC comparison is not applicable because there is no commit; production source diff is empty. Unrelated architecture document edits present before the campaign were preserved.

## Recommendation and acceptance remaining

**Accept and record the archival corrections; do not accept the slowdown as explained, and do not re-rule the tripwire from these data.** The next useful experiment is the prepared detailed stride10 diagnostic, followed by stride3 confirmation, once the prospective command ceilings are explicit. Every run must retain fresh artifacts and identity, unknown cache status, machine activity and all non-passing lines. A future result diagnoses that exact run; it does not rewrite the old 32.566-second receipt.

If a particular product mechanism is then demonstrated, request a separate owner ruling for a product fix. Isolating private GROUP_LEVEL or adding missing charged-byte/codec-CPU instrumentation also requires product-change authorization; the present campaign does not provide it.

Acceptance achieved here: exact archival per-state partition, old Commit reconstruction, bounded source call graph, explicit hypothesis limits, harness-only diagnostic, retained checks and independent review. Acceptance still open: fresh detailed per-state measurements, codec/index/worker matched diagnostics, causal allocation of the historical excess, and independent reproduction of a **new** instrumented run. The missing evidence is named; no causal residual was invented away.
