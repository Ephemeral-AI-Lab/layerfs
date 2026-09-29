# #286 round 20260930-history-stride1-v2-r013

> **Status:** Dated **FAIL** on two independent hard gates: actual allocated C2+C5 storage and the original historical O3 canonical pins. Complete semantic and C5 custody proofs pass; family 2 remains incomplete.

One new changed-source candidate used committed [`290ddb180`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/290ddb18003aeed169c4ebe018107c41c392367a), tree `a29da274cc799683ae6acb1e48cbc4f98d80546f`, locked release binary SHA256 `abe80f23f6b40ea9f8b2905105bd0b5c922e0536d98e760cc27fec7a5df54ba7`. The source writes version18 whole-file packs with sixteen directory slots (version17 remains readable), and the independent verifier starts with an empty8 MiB cap on authenticated immutable C1 page identities. It preserves the fixed157 states, 170/30 s deadlines, C2/C5 work, original O3 pins and strict `<83,947,520 B` at-run allocated gate. No unchanged r012 arm was repeated.

| Gate | Observed / frozen limit | Result |
| --- | --- | --- |
| Complete driver | **165,410,986,167 ns /170,000,000,000 ns** | PASS absolute budget; numeric time INELIGIBLE (source cache uncontrolled) |
| Separate independent verifier | **19,951,435,791 ns /30,000,000,000 ns** | PASS wall budget |
| Complete corpus tree/kind/size and selected content | **904,143/904,143** listed path-states, **76,726** selected public content-digest path-states /271,450,660 logical bytes | native O4 PASS |
| Independent roots and C5 custody | **157/157** SHA-sealed v2 roots, public reopen all157 Layers/156 Commits, expected rows, no live stages | PASS |
| Actual exclusive C2+C5 allocation | **84,475,904 B /<83,947,520 B** | **FAIL**, excess528,384 B (native/Python agree) |
| Original canonical O3 | **871,337,620 B/104,618 objects** vs**871,588,115 B/104,705** | **FAIL**, −250,495 B/−87 objects |
| Cleanup/integrity | exclusive closed C2/C5, schema/quick/foreign-key/sidecar checks | PASS |

C2/C5 allocated **84,193,280/282,624 B**; apparent total **78,548,992 B**. C2 holds1,671 packs/11,690 groups, 69,576,603 B pack blob with66,732,147 B bodies,2,844,456 B framing and zero unused capacity. Compared with the changed-source r012 physical version17 Store, body bytes and pack counts are identical, and version18 removes exactly**961,920 B** of whole-file directory reservation. Apparent C2+C5 shrinks **958,464 B**, but actual at-run allocated C2+C5 shrinks only **45,056 B**. The at-run APFS block count, rather than a copied Store or an apparent-byte estimate, remains the hard metric; it misses by528,384 B. This is a physical-allocation finding, not permission to VACUUM a post-run copy, raise the bound or omit C5.

Native trace O1/O4/C5 and verifier budget all pass. The verifier memo reports **197,776** authenticated page reuses,**64,115** first reads and **8,388,608 B** peak canonical page bodies; it exists only in the separate verifier after the driver and does not affect the measured source cache/product read buffer. Its completed full proof means r012's verifier timeout is corrected. The original O3 failure is unchanged and the full candidate totals exactly match the independent corrected older-Core reference. Source inspection shows that the historical v0.1.6 pin came from the public Docker/FUSE importer and allocator, while this replacement component harness allocates serials from its own deterministic path chain; a source-aware ruling on that migration comparison is still needed before declaring all family-2 gates green. The old pins remain intact and their failing line is not relabelled.

[Exact original receipt, SHA-indexed raw native trace/phases, verifier, schema/pack allocation and C5 file](20260930-history-stride1-v2-r013/evidence-index.json) are published; the large original C2 Store remains at its indexed local path. Stride10 r008 remains a distinct PASS, stride3 r009 its O3 FAIL, and r010/r011/r012 their timeout or hard failures. No Init repeat or family3–7 case ran. PR #285 stays draft/unmerged, #286 open, and no release admission is claimed.

Product source commit `290ddb180` records exact production LOC reference65,417→65,417 (+0), Core70,045→70,060 (+15), combined135,462→135,477 (+15). This report-only commit records reference65,417→65,417 (+0), Core70,060→70,060 (+0), combined135,477→135,477 (+0). Both use `tools/production_loc.py --json --root <snapshot>` on exact first-parent/staged/committed snapshots (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
