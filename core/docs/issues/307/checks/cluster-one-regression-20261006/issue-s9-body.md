S9 component checkpoint — restored Monolithic, full cluster-one-end regression (2026-10-06).

Owner-directed scoped withdrawal is implemented locally at `7878bbbb40b2d162e03dcb6e4e43da7b63d5b5e4`, tree `5d5d885bd8935041c5740d3c2e158f19fd6a6576`. Core product/SQL bytes match retained `9b74ac035`; acquisition batching, reservation correction, S7 overlay/daemon accounting and S9 authenticated/runtime/Bridge code are preserved. Payload schemas7/10 and their case are retired; prior designs, diagnostics and all FAIL/NOT_RUN receipts remain. No old Service/per-Init SQLite prototype was restored.

Main control: actual cluster-one-end **public Project Init** source `197d2fb7d0a141d7a9350852022febeec3255bf2`, tree `dbbe49212b26294024be28986e868c27f4de4825`. All eight original #302 candidate rows passed exact source/product/compilation/dependency/driver/verifier/helper/workload/profile/cache/root/proof/raw-closure audit and are reused as same-profile controls. Their original historical competitive FAIL/PASS labels remain. The older7edddb Service MEMORY/OFF split Store is separate context.

One fresh sample per case at the final restored source, all eight completed. Main gates:10*current product ns <=11*same-profile control ns, and final DB/WAL/SHM allocated B <=same-profile control total, with no storage allowance.

| Profile/files | Control ns | Current ns | Latency delta | Allocated B delta | Speed/storage/joint |
| --- | ---: | ---: | ---: | ---: | --- |
| durable/100 | 79759708 | 108304791 | +35.788851% | +49152 (+0.935308%) | FAIL/FAIL/FAIL |
| durable/1000 | 201566000 | 216069833 | +7.195575% | +376832 (+1.834131%) | PASS/FAIL/FAIL |
| durable/10000 | 2492429625 | 2690838208 | +7.960449% | +3244032 (+1.063373%) | PASS/FAIL/FAIL |
| durable/100000 | 7724523333 | 9777662541 | +26.579494% | +36405248 (+7.069454%) | FAIL/FAIL/FAIL |
| disposable/100 | 38747750 | 35765459 | -7.696682% | +49152 (+0.941176%) | PASS/FAIL/FAIL |
| disposable/1000 | 129258375 | 132802209 | +2.741667% | +360448 (+1.755086%) | PASS/FAIL/FAIL |
| disposable/10000 | 1645276292 | 1759129584 | +6.920010% | +3190784 (+1.045904%) | PASS/FAIL/FAIL |
| disposable/100000 | 5558569958 | 7008597625 | +26.086344% | +36372480 (+7.063428%) | FAIL/FAIL/FAIL |

Five latency screens pass, zero strict allocation/joint gates pass. Every root/sample proof, cold-content zero-residency attestation, cleanup and30s build/performance/19s separate-proof cap passes. Original5MB/20MB/300MB/500MB seed1 fixtures and four constructors/environment workers1 remain. Full closed manifests and independent raw/binary copies are verified; no resampling or best-of. Metadata residency is unobserved; lifetime CPU/RSS are not phase/system peaks.

Checks:260 host bodies,128 Linux portable bodies; no-run builds, host/Linux Clippy -Dwarnings, fmt,652-file boundary,40 tooling and16 harness tests PASS, each test invocation bounded<=120s. S7/S8/S9 remain incomplete; this component campaign does not qualify daemon/FUSE/full-root runtime, flow or residency. Closed S5/S6, owner notes, four unrelated containers, other checkouts and root reference stay intact.

Local production LOC: `4b43fa667` core95164->95164, reference65417->65417, combined160581->160581 (delta+0); `7878bbbb4` core95164->94499, reference65417->65417, combined160581->159916 (delta-665). Exact first-parent/final staged/committed trees, unchanged tools/production_loc.py c0fe7f36, shipped SQL included and test/inline-test/docs/tool exclusions unchanged. This is withdrawal of the newer format, not algorithmic simplification or legacy retirement. Final evidence is docs-only with unchanged94499/65417/159916 totals.

Report and append-only receipts are local under `core/docs/issues/307/CLUSTER-ONE-END-REGRESSION-RESULTS-20261006.md` and `checks/cluster-one-regression-20261006/`; separate S9 audit is updated. Stopping at the requested reviewable boundary; no further mechanism/layout, push, release or deployment.
