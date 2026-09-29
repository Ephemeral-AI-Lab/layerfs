# #286 round 20260930-history-stride1-v3-r017

> **Status: FAIL only on strict at-run allocated storage.** The independent O3, all157 roots, complete tree/selected-content, C5 custody, driver/verifier budget and cleanup checks pass. Family 2 remains incomplete.

The explicit run-only stride1 case sampled once on the same clean source as r016: commit `2280f580f9e9250c4a5f4b633608e3ee4e30428e`, tree `e8fc3b9727e0d79e0f6576ef4f07ba879eae13ba`, compilation seal `3afd58965c435352c25d3018bf350193e37075ddb14ca44b44fb19a827fb493f`, harness seal `b75a1d8971706d2b0a5db41acac4d8fcf39d8d0942630ede3c0ba3180d3ff625` and **exact reused release binary** SHA256 `ae03de1250e1c8bdba1eaa1c57735c90ccfcf972b699929ac991f6327e81fc34` (build wall0, no rebuilt arm). Command: `LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v3 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v3-r017`. InProcess setup, per-state C1/C2/C5 work, original independent roots, SHA-checked fixed corpus and verifier-local empty-start8 MiB authenticated C1 page memo remain as declared. Source-cache time is **INELIGIBLE** for a numeric claim; background processes are listed in the original receipt.

| Gate | At-run observation / unchanged limit | Result |
| --- | --- | --- |
| Complete driver | **165,223,743,542 ns /170,000,000,000 ns** | PASS absolute budget; numeric time INELIGIBLE |
| Separate independent verifier | **20,576,614,292 ns /30,000,000,000 ns** | PASS wall budget |
| Full semantic oracle | **904,143/904,143** listed path/kind/size states, **76,726** selected public content-digest path-states | PASS |
| Independent roots/C5 | **157/157** sealed roots, public reopen all157 Layers/156 Commits, required rows, no live stages | PASS |
| Same-producer O3 | **871,337,620 B/104,618 objects** vs independent corrected older-Core pin of exactly those values | PASS |
| Exclusive at-run C2+C5 allocation | **84,377,600 B /<83,947,520 B** | **FAIL**, excess430,080 B |
| Cleanup/integrity | closed exclusive owners, expected schemas, quick/foreign-key/sidecar checks | PASS |

C2/C5 allocated **84,094,976/282,624 B**; apparent total **76,189,696 B**. C2 uses2 KiB pages, with1,671 packs/11,690 groups,66,732,147 B pack bodies,878,376 B framing and zero unused pack capacity. The new page layout cuts385,024 B of apparent total from r015 but just40,960 B of original-owner `st_blocks*512`. No offline copy or VACUUM can stand in for the hard metric. The original v0.1.6 stride1 **871,588,115 B/104,705** remains a method-inapplicable historical comparator under the [published ruling](../HISTORY-O3-APPLICABILITY-RULING-20260930.md); v2's frozen FAIL persists. R016's separate stride10/3 PASS receipts are not pooled with this failed cell. The next treatment must reduce actual nonzero persisted bytes or explain the APFS allocation behavior with a count-driven diagnostic, under the same strict ceiling and time bounds. No threshold, worker, corpus or cache contract changes are authorized by this failure.

[Original receipt, separate verifier, native trace, C5 file and SHA-indexed raw evidence](20260930-history-stride1-v3-r017/evidence-index.json) are published. The large original C2 Store remains local with SHA256 `f4176d3563329a4522505eb6a7eddf83dbcc1be65073a827a5f8f1ad6c8283cf`. No Init or family3–7 case ran; #285 stays draft/unmerged and #286 open. This report-only commit records production LOC reference65,417→65,417 (+0), Core70,097→70,097 (+0), combined135,514→135,514 (+0), counted with `tools/production_loc.py --json --root <snapshot>` on exact first-parent/staged/committed Git archives (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
