# #286 round 20260930-history-stride1-v3-r029

> **Status: FAIL on original strict allocated storage only.** All independent
> canonical/root, complete semantic, C5, command/verifier and cleanup gates
> passed. Family 2 remains incomplete; families 3–7 remain NOT_RUN.

The separately registered stride1/157 case ran once at clean source commit
`2d4f537a5` with the same product/harness compilation seal and exact locked
release binary SHA-256
`0c1bdfd60413ac59463337bdf6233fd86117061e0a637f1e006806115ccd7996`
as selected r028; `build.json` records `exact-binary-reuse`, wall0. Command:
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v3 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v3-r029`.
The fixed corpus, independent O3/root ledger, InProcess/no-prepared-Store
policy, C5 retention and original170 s/30 s/strict storage limits were
unchanged. Corpus/source-cache time is INELIGIBLE for numeric admission.

| Gate | Actual / bound | Result |
| --- | ---: | --- |
| Complete driver | **163,757,785,917 ns /170 s** | PASS |
| Separate verifier | **20,911,970,958 ns /30 s** | PASS |
| Original exclusive C2+C5 allocation | **84,090,880 B /<83,947,520 B** | **FAIL**, excess143,360 B |
| Independent canonical objects | **871,337,620 B /104,618** | PASS, applicable v3 O3 pin |
| Complete state trees, selected bytes, roots | **904,143/904,143** paths, **76,726** selected content path-states, **157** independent roots | PASS |
| Public C5, schema, no sidecars, cleanup | all required checks | PASS |

The closed original C2/C5 files allocate **83,894,272/196,608 B** and have
apparent lengths **75,896,832/180,224 B**. C2 has2-KiB pages,
`auto_vacuum=2`,30 freelist pages,1,686 packs/11,690 groups and unchanged
66,732,147 B pack bodies. Relative to the prior level9 r021 original Store,
the new pointer-map/reclamation profile increased C2 apparent size by
**184,320 B** and actual allocated blocks by **4,096 B**; it reclaimed one
freelist page but did **not** reduce the APFS allocation step diagnosed in
r027. The synthetic one-page shrink mechanism therefore did not transfer to
this full Store. No copied file, synthetic result or `VACUUM` replaces the
at-run measurement. The old v0.1.6 O3 pins remain method-inapplicable
historical comparators; v1/v2 FAIL receipts are unchanged.

[Exact failure receipt, separate semantic verifier, native trace, C5 file and
SHA-indexed original raw evidence](20260930-history-stride1-v3-r029/evidence-index.json)
are retained. Original C2 SHA-256 is
`a9184fad1f482dbfa5016fe773364ef7a49df2c13cbb281e9f105d26b55a462c`.
`runner.py verify --run` refuses this nonpassing history receipt as designed;
that does not erase the independent semantic PASS within it. Selected r028
stride10/3 PASS receipts stay separate and cannot average away stride1.

The next product change will revert the incremental auto-vacuum treatment,
which made this strict physical gate4,096 B worse, while preserving r028/r029
and the calibrated r027 diagnostic. Further work must address the original
16-MiB allocation step or reduce persistent representation enough to cross a
real APFS block boundary without a file-sized spool, cache credit, changed
oracle, relaxed limit or replayed arm. #285 stays draft and #286 open.

This report-only commit changes no production code: reference65,417→65,417,
Core70,123→70,123, combined135,540→135,540 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including runtime SQL and excluding tests,
harness and docs (counter SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
