# #286 Family5 functional checkpoint —2026-09-30

**Status:15/15 registered functional selections covered (10 native,5 public SDK), command/oracle/cleanup PASS. Numeric cache eligibility INELIGIBLE. Not release admission. Families6/7 remain open, and earlier numeric qualification remains unresolved.**

Family5 changes are benchmark/example/external-test work only. Product behavior, codecs, workers, Budget/quota, durability, C2/C5 formats and ARMv8 flags did not change. Earlier-family product proofs are explicitly reused, including the entire Family2 group; zero earlier performance reruns. Native small master and unchanged daemon/image artifacts reuse recorded seals, new layout masters are prepared once, and every case gets an independent byte copy.

| Public SDK selection | Complete command s / bound | SDK Exec ms | SDK final Commit ms | Separate full verifier ms /9s | Functional/cleanup/cache |
| --- | ---: | ---: | ---: | ---: | --- |
| Move/replace3 descendants | 0.865639833 /15 | 76.019084 | 22.348958 | 201.762875 | PASS/PASS/INELIGIBLE |
| Move/replace67 descendants | 0.871133417 /15 | 70.527500 | 27.216208 | 215.061833 | PASS/PASS/INELIGIBLE |
| Inherited4097-byte component path | 0.874109709 /25 | 63.039000 | 30.505833 | 97.284666 | PASS/PASS/INELIGIBLE |
| 270-component walk | 2.805615834 /25 | 1157.090208 | 767.457958 | 201.375042 | PASS/PASS/INELIGIBLE |
| Retained G1 namespace + replacement | 0.897687500 /15 | 18.484375 | 17.011291 | 26.150500 | PASS/PASS/INELIGIBLE |

These are raw single-command observations from r066. SDK headline includes all lifecycle/Mount/Exec/Commit/Status/checked cleanup/logs/shutdown; independent proof is separate and never added to a speed comparison. Retained case also includes prelude Exec20.052833ms, Commit10.734083ms and Pin. Held G1 file is fully read and hashed (10B), lease release checked; independent old/new trees and prelude parent verified. Native companion covers full pinned namespace and live G1/G2.

| Native selection | Functional child command s /15s | Observation |
| --- | ---: | --- |
| workspace-namespace-uncached-descendants-3-native-v1 | 0.206780458 | PASS /checked cleanup |
| workspace-namespace-uncached-descendants-67-native-v1 | 1.341718916 | PASS /checked cleanup |
| workspace-namespace-resident-descendants-3-native-v1 | 0.206207709 | PASS /checked cleanup |
| workspace-namespace-resident-descendants-67-native-v1 | 1.410362000 | PASS /checked cleanup |
| workspace-namespace-unrelated-resident-256-native-v1 | 4.206949709 | PASS /checked cleanup |
| workspace-namespace-deep-4097-uncached-native-v1 | 0.500132042 | PASS /checked cleanup |
| workspace-namespace-deep-4097-resident-native-v1 | 0.483469292 | PASS /checked cleanup |
| workspace-namespace-retained-live-g1-g2-native-v1 | 0.215163125 | PASS /checked cleanup |
| workspace-namespace-rename-refund-native-v1 | 0.099988041 | PASS /checked cleanup |
| workspace-namespace-rename-refusal-native-v1 | 0.093970625 | PASS /checked cleanup |

Native commands include public Workspace work, in-child canonical/pinned oracles and clean-close; they are not SDK speed. Container/fixture setup and external teardown are separate recorded functional infrastructure. Actual ext4 backing; Linux Workspace + in-process Server is the declared native control topology. Public SDK/Server/SQLite runs on the host with Linux daemon/FUSE workload, preserving the product host boundary. Host operating-system measurements are diagnostic instruments, not product storage requirements.

The five rename dimension controls report upstream/private/accounted counts: uncached3 and67 descendants both2 upstream calls and695B accounted delta; resident3 and67 both1 call and704B;256 unrelated resident files match the uncached control (2/695B). Physical private metadata reads/writes/new pages are0 for those operations. Internal resident scans, descendant visits and C1 page visits are UNAVAILABLE. These observations do not establish asymptotic independence or cold speed.

Deep native controls reach4097B, read by components, rename to4096B and move back. Public SDK4097B and270-component cases commit their final trees and verify by existing public C1 inode serial APIs. Moved serials persist, replaced files receive new serials, all complete modes/listings/bytes and history parents agree. Canonical full-path4096B/256-component bounds remain intact. Atomic rename refusal preserves revision/names at baseline+8192B quota, baseline and target clean; exact refund retains escrow851968B only until completion, then0.

Evidence: [frozen spec](FAMILY5-SPEC-20260930.md), [native r065](experiments/20260930-workspace-namespace-native-r065.md), [SDK + separate proof r066](experiments/20260930-workspace-namespace-sdk-r066.md), [owning checks](experiments/20260930-family5-checks.json). Raw paths and exact hashes/commands/seals are in the linked compact receipts. r064 compile typo and phantom pre-acquisition volume removal remain [historical INCOMPLETE/NOT_RUN](experiments/20260930-workspace-namespace-native-r064.md), not deleted or promoted. Post-collection standalone-shell import cycle is repaired and checked with no unchanged performance replay.

| Family | Current disposition | Family5 action |
| --- | --- | --- |
|1 Init | Existing release functional/command/verifier/cleanup proof, numeric INELIGIBLE; explicit large-tier registration preserved | Reused unchanged product proof |
|2 History retention | All3 strides independent-root/tree/content/storage/cleanup scoped PASS under owner-approved up-to10% storage deviation; time diagnostic | Entire group reused, no run |
|3 Workspace writes |9 functional/command/full-byte/cleanup profiles covered; numeric INELIGIBLE | Reused r063 affected dispersed4097 + earlier8 unchanged profiles |
|4 Workspace Commit |14 qualified functional/custody profiles;10240 explicitly SKIPPED/OWNER-DEFERRED under#276 | Reused scoped checkpoint, no run |
|5 Workspace namespace |10 native +5 SDK functional/command/full oracle/cleanup PASS; numeric INELIGIBLE | Current checkpoint |
|6 Workspace mutations | NOT_RUN /prospective mixed case specification next | Next family |
|7 Workspace shell/package | NOT_RUN /existing refresh plus broader#256 scope remains | No completion claimed |

Checks covering changed sources: Linux locked-release native external-test Clippy, host SDK/verifier example Clippy, fmt,357-file product boundary/9 self-tests and5 namespace Python checks all PASS. Initial generic-native3 Python checks also PASS. Standalone shell self-check and runner list PASS after import fix; these invoke no benchmark. Unchanged product-wide test/Clippy proof is reused from the [Family4 checkpoint](FAMILY4-CHECKPOINT-20260930.md); no CI/preflight or earlier passing unit sweep ran. Clippy dev checking does not substitute a debug measured binary: every actual benchmark/verifier executable is locked release.

Production LOC Family5 start→current: reference65417→65417, Core70221→70221, combined135638→135638 (**delta+0**). PhaseB start→current combined135439→135638 (**+199**), Core70022→70221, reference unchanged. Every commit uses exact first-parent/final staged production-only archive counting with `tools/production_loc.py --json`, excludes tests/examples/harness/docs and records its own before/after/delta. No code is relocated or removed to improve the count.

Next: freeze Family6 mixed ordinary mutation workloads, retained/live selections, failure custody and independent oracles, then execute through the same fast lane. Do not broaden this into the owner-deferred10240 streaming reconciliation architecture work. Preserve original failed/unrun receipts, unresolved cache eligibility, #256 scope, draft/unmerged PR285 and open issues. No claim that all seven families, PhaseB admission or release is complete.
