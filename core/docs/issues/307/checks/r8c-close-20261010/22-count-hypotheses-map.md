# Count hypotheses H-1 to H-19: what existing tests assert

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Gate G08 under owner decision C-13: the count gate is the set of assertions
existing tests make on the quantities the
[proof plan](../../S8-PROOF-PLAN-20261008.md) names, mapped row by row. The
mapping was made by a read-only research pass at `307618496` and six citations
were checked against source by the lead. No test names an H row; the mapping
is a reading of the assertions. Every binary below passed in the final suites
at `307618496` ([host](11-final-host-suite.txt), [Linux](15-final-linux-suite.txt)),
except the 100,000-file case, which needs its prepared input and is a
registered precondition proof.

Paths are under `core/crates/`; `d/` is `layerfs-daemon/tests/`, `o/` is
`layerfs-overlay/tests/`, `w/` is `layerfs-workspace/tests/`, `f/` is
`layerfs-fuse/tests/`.

| H | Judgement | Asserting tests | Not asserted |
| --- | --- | --- | --- |
| H-1 | PARTIAL | `d/installed_store.rs:103`, `d/installed_qualification.rs:169`, `d/complete_installed_roots.rs:38` | Exact 10/11/12 (a 4 to 16 range is asserted); no enumeration counter; Store bind, not a kernel mount |
| H-2 | PARTIAL | `d/installed_store.rs:109`, `d/installed_qualification.rs:47,172`, `d/native_control.rs:97`, `d/mount_debt.rs:177` | Exactly one history read; creates no database |
| H-3 | PARTIAL | `d/fp2_negotiation_flags.rs` (threads equal one loop), `d/native_mount.rs:148,181,386` | System-call count; no base demand across Attach |
| H-4 | PARTIAL | `d/visit_port.rs:345,363,375,419`, `d/read_cost.rs:696`, `d/directory_cost.rs:679`, `w/native_visit_cost.rs:255` | Per-request owner-job count at a real mount |
| H-5 | PARTIAL (recorded baseline by its own text) | `d/read_cost.rs:696,730,753` | Object-cache hits per warm LOOKUP and GETATTR |
| H-6 | PARTIAL | `d/visit_port.rs:419,443,546`, `d/owner.rs:907,963`, `w/native_visit_cost.rs:255` | Not at a real mount; the ticket release is zero jobs normally, one only when a capture waited |
| H-7 | PARTIAL | `d/read_cost.rs:696,979`, `d/directory_cost.rs:679`, `w/native_visit_cost.rs:255` | Not at a real mount; OPEN of an unseen base file is pinned at two jobs |
| H-8 | COVERED | `d/mounted_drain.rs:1327,1360,1406,1455`, `d/native_custody.rs:145`, `d/fp21_detach_stages.rs:1250`, `o/native_revocation.rs:145` | A byte bound on queued FORGET units at a mount |
| H-9 | COVERED | `d/mounted_fusectl.rs:443,491,546,588` | Sampled, not a continuous maximum; `max_background` 1 only |
| H-10 | COVERED | `d/mounted_parking.rs:732,741,830`, `d/forced_unmount.rs:1090`, `f/dispatch.rs:120,190,265` | Receive-input bytes charged at a real mount |
| H-11 | PARTIAL | `d/installed_store.rs:135`, `d/store_read_service.rs:176` | Batch identities and bytes per demand window; duplicate acquisitions; a declared wait bound |
| H-12 | PARTIAL | `d/mounted_concurrency.rs:1459,1487`, `d/finite_service.rs:13` | A declared share; none exists |
| H-13 | NONE (recorded baseline by its own text) | none | No counter for work under the cache mutex or cache wait |
| H-14 | NONE (belongs to S10) | none | `git status` on a second fresh mount |
| H-15 | COVERED | `d/support/mounted.rs:127,155`, `d/mounted_drain.rs:442,510,532`, `d/native_mount.rs:278`, `d/forced_smoke.rs:88` | nothing |
| H-16 | COVERED | `d/owner.rs:116,128,180,1012`, `d/mounted_cycles.rs:505,531`, `d/mounted_drain.rs:510,1379` | A fixed step count; the bound is 64 rows per turn |
| H-17 | PARTIAL | `d/native_coherence.rs:166,530`, `d/fp2_negotiation_flags.rs`, `d/r5_9_dirty_mapping.rs:278` | A per-workload opcode range declared before the run |
| H-18 | COVERED | `d/native_coherence.rs:497,532` | LISTXATTR and REMOVEXATTR are asserted zero but never issued |
| H-19 | PARTIAL | `d/forced_smoke.rs:81,96`, `d/forced_producers.rs:813,827`, `d/forced_unavailable.rs:42`, `d/forced_unmount.rs:557,618` | An independent count of abort writes and unmount calls |

Six rows are asserted at a real mount (H-8, H-9, H-10, H-15, H-16, H-18). Ten
are asserted in part or at component scope. H-13 and H-14 have no assertion;
the first is a baseline to record by the plan's own text, the second is S10's.
None of these is reported as established beyond what its tests assert.

SQL plans: plan and runtime counters are asserted together at engine scope in
`o/engine.rs`, `o/compound.rs`, `o/native_lookup.rs`, `o/native_revocation.rs`,
`o/indexed_operation_record.rs`, `o/captured_namespace.rs`, `o/source.rs`,
`o/frontier.rs`, `o/reclaim_cost.rs` and `o/cleanup_observation.rs`. No test
correlates a plan with runtime statements in a mounted run, and no product
observation exists for pages, copies or waits.
