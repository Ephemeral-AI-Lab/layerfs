# #286 round 20260930-history-stride1-v3-r032

> **Status: PASS** on the explicit v3 stride1/157 hard storage, independent
> semantic, C5, command/verifier and cleanup gates. Together with selected
> r031 stride10/3 PASS at the same product/harness/binary seal, all three
> family-2 cells meet their scoped gates. The required one-time family-1
> regression check remains next. Numeric latency is INELIGIBLE under the
> uncontrolled source-cache contract.

The single run-only sample used clean source commit `1bbffd2f6`, the exact
locked-release binary SHA-256
`4cdaf99ea524cd2a3cb9205bdd8a8c633715ca2391e06cbaece8e74a160bda80`
and compilation seal
`c31f1b666c8b3b41b85b19bff4884e2edee1e10752350ce47a086e4d24d29c52`
from r031. `build.json` records exact-binary reuse and build wall0; it did
not replay a measurement or verifier. Exact command:
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v3 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v3-r032`.
The fixed SHA-checked corpus and separate independently sealed root/O3
ledger were unchanged. All157 C1/C2/C5 states were constructed and saved
inside the measured child; no prepared Store, clone, extra worker or external
physical watcher participated.

| Gate | Original-owner actual / unchanged bound | Result |
| --- | ---: | --- |
| Complete driver | **164,904,205,167 ns /170 s** | PASS |
| Separate verifier | **20,881,929,292 ns /30 s** | PASS |
| Exclusive C2+C5 allocated | **78,839,808 B /<83,947,520 B** | **PASS**, margin5,107,712 B |
| Applicable independent O3 | **871,337,620 canonical B /104,618 objects** | PASS |
| Whole state trees, selected bytes, expected roots | **904,143/904,143** path-states, **76,726** selected content path-states, **157** roots | PASS |
| Public C5, schema, no sidecars, cleanup | all required checks | PASS |

The closed original C2/C5 files allocate **78,643,200/196,608 B** and
have apparent lengths **75,712,512/180,224 B**. C2 still has2-KiB pages,
31 freelist pages,1,686 packs/11,690 groups and66,732,147 B pack bodies.
Both database SHA-256 values are **byte-identical** to the official failed
level9 r021 originals:
`ef1fba79a76add4c781c9bbcf1c5219a9548d3cb03b4483a8d84ded9059ca781`
and `8dcf12b01322afcaf9e1a758b05abbe68e1d3216c0a37eda9033090f9af93833`.
The smaller allocated count is therefore physical placement paid by the
product before pack INSERT, not a changed oracle, encoded representation,
copied/VACUUMed Store or reclassified historical receipt. Every native and
Python hard gate passed, and `runner.py verify --run` passed the retained
manifest/receipt check. r021's original FAIL and the negative r025/r029
attempts remain immutable.

[Exact PASS receipt, separate verifier, trace, C5 file and SHA-indexed raw
evidence](20260930-history-stride1-v3-r032/evidence-index.json) are
published. The large original C2 Store remains local at its indexed path.
This is a **family-2 candidate checkpoint**, contingent on the one-time
family-1 regression check required by the frozen order. It does not claim a
history speedup, release admission, families3–7, a pure-move Commit result or
the broader #256 scope. #285 remains draft and #286 open.

This report-only commit changes no production code: reference65,417→65,417,
Core70,213→70,213, combined135,630→135,630 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including runtime SQL and excluding tests,
examples, harness and docs (counter SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
