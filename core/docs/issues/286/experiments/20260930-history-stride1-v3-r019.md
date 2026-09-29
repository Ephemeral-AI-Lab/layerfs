# #286 round 20260930-history-stride1-v3-r019

> **Status: FAIL only on the unchanged strict allocated-storage gate.** Independent roots/O3, full tree and selected content, C5 custody, command/verifier budgets and cleanup pass. Family 2 is not complete.

One explicit stride1 sample used the same clean product/harness/binary seal as r018: commit `c233e023017dfa227072324af905bc46319a970e`, tree `40744327fd9ac6b22283825da9b04bad66647ba6`, compilation seal `523e2bb8e0e9519f07f9c0411d38bffcecd7fa40ce93b362ab21551d12267430`, harness seal `b75a1d8971706d2b0a5db41acac4d8fcf39d8d0942630ede3c0ba3180d3ff625`, exact reused locked release binary SHA256 `6d6c80520657c96426cb716f957b6a955ffc6a09de8df7e38f79bfd040c00164` (build wall0). `LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v3 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v3-r019` ran once on a fresh output path. InProcess save/publish, fixed corpus and independently sealed roots, C5 public reopen, SHA-checked untimed count-ledger acquisition, separate verifier-local memo and competing-process inventory are in the receipt. Source-cache numeric time is **INELIGIBLE**.

| Gate | At-run observation / unchanged limit | Result |
| --- | --- | --- |
| Complete driver | **163,656,254,542 ns /170,000,000,000 ns** | PASS absolute bound |
| Independent verifier | **20,885,246,959 ns /30,000,000,000 ns** | PASS wall bound |
| Full semantic oracle | **904,143/904,143** listed path/kind/size states and **76,726** selected public content-digest path-states | PASS |
| Independent O3/root/C5 | **871,337,620 B/104,618 objects** vs same-producer independent pin, **157/157** roots, C5 public reopen all157 Layers/156 Commits | PASS |
| Exclusive at-run C2+C5 allocation | **84,246,528 B /<83,947,520 B** | **FAIL**, excess299,008 B |
| Cleanup/integrity | closed exclusive owners, expected schema, quick/foreign-key/sidecar checks | PASS |

C2/C5 allocated **83,963,904/282,624 B**; apparent total **76,052,480 B**. C2 has1,673 packs/11,690 groups,66,732,147 B bodies,886,616 B framing, zero unused pack capacity and63 freelist pages at2 KiB. Version22 retains159 pooled packs versus157 prior version12 packs. Against r017's prior source, the pooled cap reduces actual allocated C2+C5 by exactly **131,072 B**; the C2 freelist falls from127 to63 pages, also **64×2,048=131,072 B**. The added two pack directories increase framing by8,240 B. This source-backed page accounting bounds what this mechanism achieved; it cannot close the remaining299,008 B alone. No copied file, offline VACUUM or warm variant was credited. The old v0.1.6 O3 pin remains a historical, method-inapplicable comparator under the [prospective ruling](../HISTORY-O3-APPLICABILITY-RULING-20260930.md); its v1/v2 failures stay FAIL. R018's two PASS rows are distinct.

[Exact receipt, native trace, separate verifier, C5 file and SHA-indexed raw evidence](20260930-history-stride1-v3-r019/evidence-index.json) are published. The large original C2 Store remains at the indexed local path with SHA256 `6c4a082eaac927979790b4d28a27bbaf4d275db0ab2d2b1d052adfcb2322a652`. No Init or family3–7 case ran; #285 remains draft/unmerged and #286 open. This report-only commit records production LOC reference65,417→65,417 (+0), Core70,109→70,109 (+0), combined135,526→135,526 (+0), counted with `tools/production_loc.py --json --root <snapshot>` on exact first-parent/staged/committed archives (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
