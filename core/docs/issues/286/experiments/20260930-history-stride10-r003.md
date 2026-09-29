# #286 round 20260930-history-stride10-r003

> **Status:** Dated failed benchmark checkpoint; partial evidence, no admission.

First compound candidate selection at committed code/profile/pins **509668ea6b9bcd9318eb901ed3473f99cb0ef30e**, tree **02beecf740f03af5a9abdc4155ab51a96108724f**. Code fix/profile commit509668ea6 preceded the official sample. Selected only stride10/17 states, one invocation, no comparative arm. Stride3/stride1 remain NOT_RUN. Public C1/C2 construction with timed real C5 uses the [frozen compound profile](../HISTORY-PROFILE-V1-20260930.md).

| Case | Complete driver ns / limit | Product operation ns | Separate verification | Canonical / storage gate | Correctness / cleanup | Numeric time eligibility | Overall |
| --- | --- | --- | --- | --- | --- | --- | --- |
| history-retention-stride-10-total-storage-v1 | 6019876917 /60000000000, command budget PASS | unavailable: failed chain did not publish its timing tree | NOT_RUN: no complete17-state outcome | INCOMPLETE /INCOMPLETE | INCOMPLETE /INCOMPLETE | INELIGIBLE | FAIL |
| stride3 | NOT_RUN | unavailable | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |
| stride1 | NOT_RUN | unavailable | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |

The complete driver was6.019876917s; outer setup/build/collector command7.377486500s, exit0, no timeout. The native driver printed **INCOMPLETE** despite its exit0, and the adapter retained overall FAIL. Raw `phases-perf.json` emits operation0/timing bytes0 after the incomplete return: these are not evidence of zero product work or a zero-time operation. Six actual states had already been committed. `sample_count=null` records unavailable completed timing evidence; attempted invocation count1 is retained. No repeated arm or verifier ran to replace this partial attempt.

## Retained failure and actual prefix custody

Native trace: `g1.o1-chain-complete=INCOMPLETE`, **`product error: Integrity("dependency encoded work")`**. The backend's old O3 hook also emits INCOMPLETE because that historical table contains no history row. No independent full-chain verification was run: every mandatory17-state proof remains incomplete.

Read-only diagnosis from the closed partial at-run files found C2 `user_version10`, **13189120 apparent /13258752 allocated bytes**,8982 distinct objects/60671650 canonical bytes and6 known publication rows/watermark6. C5 has **86016 apparent/allocated bytes**, `history_meta1`, `layer_stacks1`, `branches6`, `layers6`, `commits5`, zero stages and zero scope allocator rows. Thus the observed partial compound allocation is **13344768 bytes**, but a6-state prefix cannot be compared to the17-state storage threshold as a PASS. Those owner readings are diagnostic prefix custody; the original collector's incomplete receipt is unchanged.

A second source-backed harness defect is preserved: the Python pack-space reader only recognizes legacy pack versions1/2/4/6/7 and refuses current **version17** with `pack 1: framing version 17 is not one of [1, 2, 4, 6, 7]`. Its original storage receipt therefore has no compound total. No missing field was set to zero and no archived-copy blocks were substituted.

## Cause and next concrete fix

All callers route through C2 `encoding/delta/read.rs::record_width`. It treats `PackLane::WholeFile` as an entire group extent. Current pack version17 contains multiple independently framed compact records per group, while actual reconstruction selects only `location.record_number` through the existing bounded framing decoder. Repeatedly charging unrelated sibling records to each chain dependency exhausts the unchanged256KiB encoded-work budget on otherwise valid persisted data. Native/ordinary Raw paths already use `framed_record`; remove the WholeFile whole-group exception so this shared function follows the actual record decoder. Retain ordinary compressed-group charging, identity checks and all existing canonical/depth/encoded limits. Add one external regression covering shared-group record charges and reopening a valid chain, then run only that new covering check.

Update the existing Python pack reader for the actual reserved-directory/declared-length grammar and grouped whole-file record boundaries, keeping legacy interpretation explicit. Do not accept unknown versions or charge every record the whole group. Source-aware parsing and the original fixed gates remain mandatory. Both changes are genuine product/harness identity changes before the next candidate invocation; this failed attempt stays append-only.

## Build, identity and reproduction

Locked incremental release build **306011375 ns**, PASS; same worktree's existing target, no foreign target. Archived binary SHA256 **1bbd2b1bb1ed7bc7a8f0bcfb53fed55ffe7f2626dfef59d148329e96b4e1b1d7**. Compilation seal **232078beb9cd15492c9c3670a2d206a04891ac274fce49c9cf22099aff319816**; dependency seal **576cfcbda6d8797d3f122b4d9d122648cf9a47290b35c63d9743c0424cca7aaf**; harness **0b10536330d5ae1254690490ecb69bc8b05217beec93e98c140e940161b6b5c6**; product/compilation-input legacy runner seal **9333b02e03ddb604b9013fcd3199d4f06b815234e338fe996088af1219147137**. Core Cargo lock and root ARMv8 configuration are unchanged. Immutable corpus/root pins and competing Docker background processes are in the receipt. No same-worktree build overlapped the child.

```sh
CARGO_BUILD_JOBS=8 LAYERFS_CONSTRUCTION_WORKERS=1 TMPDIR="$PWD/core/target/issue286-tmp" python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-10-total-storage-v1 --out benchmark-results/fs-bench-pro/issue286-history-stride10-r003
python3 core/benchmark/fs-bench-pro/runner.py report --run benchmark-results/fs-bench-pro/issue286-history-stride10-r003
```

The named output is already occupied; the next authorized changed-identity invocation requires a fresh path. Outer supervision175s includes untimed build/collector overhead and did not alter the60s driver or10s independent verifier budget. Receipt/report rederivation reads retained files only. [Exact published raw receipts and hash custody](20260930-history-stride10-r003/evidence-index.json), [original retained manifest](20260930-history-stride10-r003/manifest.json), [original candidate receipt](20260930-history-stride10-r003/history-retention-stride-10-total-storage-v1/receipt.json). Original partial Store/history remain local, with their hashes and actual allocations indexed. Logs are deterministic gzip representations of unchanged raw bytes.

Current family2 gate is **not complete**. Earlier-family checkpoint NOT_DUE; no unchanged Init or unit suite was repeated. Families3–7 remain NOT_RUN and continue after required history gates pass. The stride3 original canonical mismatch stays open; no pins, limits, fixture, policy, worker count or oracle were changed to pass this row. PR#285 remains draft/unmerged and all issues stay open.

Production LOC for this report-only commit: reference65417 ->65417 (delta +0); Core70022 ->70022 (delta +0); combined135439 ->135439 (delta +0). Exact first-parent/final staged/committed source snapshots via `python3 tools/production_loc.py --json --root <snapshot>`, counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`. Product Rust/nested API/runtime SQL included; docs/tests/harness/tools excluded. Report commit and issue comment are recorded after publication.
