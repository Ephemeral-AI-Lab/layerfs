# R7 candidates

> **Status:** Current planning checklist; no release candidate exists.

Seeded from optimization handbook section 9 at initial product `2b4dc28a6`.
Historical counts are hypotheses to diagnose at the authentic baseline, not new
timings. No candidate is accepted yet. Every kept change must reduce counted
work, pass affected correctness/count/scaling proofs, add no resident allowance
or input-sized state and add no logical/allocated Store/overlay bytes.

| ID | Candidate / group | Counter and historical cause | Risk / affected proofs | Authority / outcome |
| --- | --- | --- | --- | --- |
| H01 | Batched sealed-record read / H | 19076/21182 Commit jobs are record jobs; reduce point round trips to bounded windows | Sealed identity/custody; captured_namespace, product_commit_cost, mounted_commit | Owner 2026-10-09 permits interface changes; pending baseline |
| H02 | Group Content membership questions / H | About 16 is-new and 11 has-header questions/new inode | Replay/validation; filesystem_topology_backed, captured_namespace | Owner direction permits; pending |
| H03 | Producer sorted answer windows / H | Repeated membership questions; expected fewer point reads | Exact seals/windows; captured_namespace_cursor/custody | Pending; alternative to H02, not duplicate cache |
| H04 | Compare inode metadata before patch / H | Patch offered for every captured row | Metadata/identity; captured_namespace_metadata, mounted_install | Pending |
| H05 | Sorted-merge sibling reads / H | 70–86 inode pages/one-file update; grows with base | Canonical tree/reference semantics; Content and Init proofs | Pending; scaling before constants |
| H06 | Narrow territory gate / H | Unrelated mkdir opens moved-directory subtree traversal | Alias/cycle validation; filesystem_topology_model/backed | Pending |
| C01 | Local READ without Store reader / C | Every local READ takes lease | Read/capture custody; mounted_parking/coherence, filesystem_port | Pending |
| C02 | Combine source/facts/release jobs / C | About 5 jobs/request | Atomic permissions/source/custody; native_jobs/coherence | Pending |
| D01 | Status without refused Lifecycle observation / D | 297/310 engine observations refused during R6 Commit | Bounded read-only control and additive wire | Pending |
| D02 | Commit/filesystem admission ordering / D | 30327 jobs during one Commit | Fairness/no starvation; finite_service, mounted_concurrency | Pending |
| B01 | Negative entries/READDIRPLUS/FLUSH / B | Unmeasured at product arm | Immediate namespace/permission coherence; native_coherence/mutation | Owner permits with proof; no TTL/depth increase |
| B02 | Wider directory replies / B | One 64-name window/reply | Exact cookies/custody; native_directory/mounted_install | Pending; no buffer enlargement |
| I01 | Save transaction/batch/pack-search cost / I | #313 hypotheses, current counts needed | Save finish/publication failure; storage/persistence/history | Pending |
| G01 | Partial-cell copy and cache-hit cost / G,F | #313 hypotheses, current counts needed | Exact fragments/truncate/bytes; Workspace and storage | Pending |
| H07 | Parent pointer/ancestry evidence / H | Moved stored directory may list subtree | Changes canonical format and adds stored bytes | PROPOSED ONLY; forbidden in this run, counted benefit pending |

Candidate details use the handbook template: cause sentence with measured units,
counter start/floor/end, growing variables, actual EXPLAIN and runtime profile,
proof binaries/count tests, memory/disk gates and immutable receipts. An
unavailable instrument must be named before diagnosis is closed.
