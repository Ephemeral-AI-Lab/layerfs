# #286 round 20260930-history-stride1-v4-r039

> **Status: explicit v4 stride1/157 PASS** on independent semantics, public
> C5 retention, 10%-maximum allocated storage, command/verifier bounds and
> cleanup. Alongside selected r038 stride10/3 PASS at the same product,
> compilation and harness seals, all three v4 family-2 cells meet their scoped
> gates. Family1's required current-source regression check is still due.
> Numeric latency remains INELIGIBLE under uncontrolled source-cache residency.

The one run-only sample used clean report source `0455a7e3c`, product/profile
source `60c7af879`, the exact r038 locked-release binary SHA-256
`65db587b41f1c6604f59da69d09f935ed7bfb5d7c1012331f562d5f559930849`,
compilation seal `00914f9020517febcec8a794b89f16e03ab8d05e8de5f40c5d89767608c4f72a`
and harness seal
`18fb89a51c9b60824b8726e5f11616ba3e66ec8b8ac0507a5bd58b1bce0dd9da`.
The build record declares exact-binary reuse and wall0. Exact command:
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v4 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v4-r039`.
All157 C1/C2/C5 states were constructed and saved inside the child; there was
no prepared Store, clone, external physical watcher or added worker.

| Gate | Original-owner observation / unchanged or v4 bound | Verdict |
| --- | ---: | --- |
| Complete driver | **144,769,277,500 ns /170,000,000,000 ns** | PASS, 25.231 s headroom |
| Separate verifier | **21,370,066,458 ns /30,000,000,000 ns** | PASS |
| Exclusive C2+C5 allocated | **86,179,840 B /<92,342,273 B** | **PASS**; +2,232,320 B (+2.659%) over original strict `<83,947,520 B`, within5% |
| Applicable independent O3 | **871,337,620 canonical B /104,618 objects** | PASS |
| Whole state trees, selected bytes, expected roots | **904,143/904,143** path-states, **76,726** selected content path-states, **157** independent roots | PASS |
| Public C5, schema, sidecars, cleanup | **157 Layers /157 Branches /156 Commits**, all required checks | PASS |

Closed C2/C5 owners allocate **85,983,232/196,608 B**. Native and Python
resource/semantic gates agree; `runner.py verify --run` rederives PASS from
the retained evidence. The internal timer tree clips later stride1 children,
so its partial `operation_ns` is not a whole-operation figure. The earlier
high-compression r032 complete command164.904 s and r036 timeout are separate
cache-uncontrolled observations, not paired controls or a quantitative speedup
claim. V1–v3 original thresholds and outcomes remain unchanged.

[Exact PASS receipt, C5 original file, trace and SHA-indexed raw evidence](20260930-history-stride1-v4-r039/evidence-index.json)
are append-only. The large original C2 Store remains at its indexed local
path. This is a **family-2 candidate checkpoint**, contingent on the one-time
Family1 regression check at the current product source. Families3–7 remain
NOT_RUN, PR#285 draft and #286 open. This report-only commit has production
LOC reference65,417→65,417, Core70,219→70,219,
combined135,636→135,636 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including shipped SQL and excluding tests,
benchmark harness and docs.
