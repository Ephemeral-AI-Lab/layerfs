# Pooled physical-group reuse diagnostic

> Status: Research; informative and not a product contract.

Follow #190 and the previous parent-lookup campaign protocol with the following prospectively frozen scope. Base: merged `9d82685f39460fabec6c810184d87adcccd53dc5`. Treatment: bounded reuse of pooled physical group decoding only. First freeze the same production work counters and harness in both arms. No signature, reference lookup, codec, worker, capacity or canonical-format changes.

One performance sample per case per arm; baseline stride10, candidate stride10, then baseline stride3 and candidate stride3 confirmation if actual decoded work falls and correctness holds. Baseline executable is archived before candidate source changes. No stride1. Complete-command diagnostic limits remain 120/240 seconds; verification hard deadline60 seconds and work targets10/20 seconds. No timeout enlargement or rerun for a better number. All failures and deferred preflight observations retained.

Fresh growing Store and output for every sample; reuse existing pinned corpus and incremental locked builds. LAYERFS_CONSTRUCTION_WORKERS=1, LAYERFS_HISTORY_PHASES=1; all eight history behavior switches unset. Cache residency uncontrolled, no priming/purge. All elapsed results INELIGIBLE for cold/performance admission; diagnostic effect only. Missing history O3 pins remain INCOMPLETE. No new v0.1.6 run or historical causal reallocation.

Use prior collect_diagnostic.py with only campaign directory redirected. It holds shared TMPDIR and /tmp flocks plus harness measurement lock, requires no named cargo/rustc/fs-bench competitor and >=70% CPU idle on the second observation. Root alone runs builds/tests/measurements serially under those locks. Desktop activity remains disclosed. Raw Store/binary files retained locally with hashes; traces, receipts and analysis publishable.

Mechanism: pooled physical_record_calls, actual physical_group_decodes and decoded bytes, cache hits, leaf requests/edges, value-group materializations and pack BLOB acquisitions/bytes. Counts describe successful read-wave results; do not call BLOB bytes physical disk traffic. No extra per-record clocks. Instrumentation overhead is NOT separately measured; identical instrumentation in both arms. Existing provider elapsed overlaps operation and cannot be added to it.

Correctness: focused public-provider FULL/chain tests, exact canonical equality, resource-refusal/corruption/visibility/cache-bound tests, identical saved roots/Store bytes, separate sampled verification. Core tests/examples/clippy/fmt plus boundary guard/self-tests. Retain known harness lint/format failures without relabeling. Production LOC baseline combined85582 (reference65417, core20165); instrumentation and optimization increments reported separately with same counter.
