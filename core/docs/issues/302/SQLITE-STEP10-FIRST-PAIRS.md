# First durable SQLite matched Init pairs

> **Status:** Dated measurement checkpoint; all-seven competitive qualification incomplete.

Source c80a26567; release/locked; one sample per100/1000 case/arm. Both canonical roots match; independent sampled namespace proof, content-page cold attestation and scratch cleanup PASS. Original e97 reference build failure remains NOT_RUN/sample_count0. No performance arm repeated.

| Files | External child baseline/current ns | Complete performance envelope baseline/current ns | Independent proof baseline/current ns | Final allocation baseline/current B | Frozen external time+allocation gate | Internal product baseline/current ns |
| --- | --- | --- | --- | --- | --- | --- |
| 100 | 905740083 / 626732000 | 916312667 / 648257667 (<=15s) | 33115291 / 526860000 (<=9.5s) | 7372800 / 5246976 | PASS | 52589125 / 75932541 |
| 1000 | 140899000 / 217724792 | 242834209 / 417605000 (<=15s) | 55150125 / 46699750 (<=9.5s) | 23101440 / 20557824 | FAIL | 134587333 / 208570875 |

The100 external gate passes, but its startup envelope is853,150,958ns reference versus550,799,459ns current. It masks a44.39% internal complete-product gap and44.07% Init gap. No similar-speed or terminal claim is made from that row. The1000 external gate FAIL is54.52% slower; internal product is54.97% slower. These spans are retained, not subtracted to rewrite the primary comparison.

| Step ns | 100 baseline/current | 1000 baseline/current |
| --- | --- | --- |
| bootstrap_ns | 12151292 / 14646208 | 5648042 / 7049334 |
| init_ns | 40305625 / 58066708 | 128834750 / 199424709 |
| close_ns | 131583 / 293542 | 104125 / 278666 |

| Current actual work | 100 | 1000 |
| --- | --- | --- |
| statements | 285 | 1308 |
| vm_steps | 37861 | 210182 |
| bound_bytes | 5103707 | 20469374 |
| statement_ns | 46682205 | 125218412 |
| commit_ns | 34774082 | 104273083 |
| transactions | 26 | 47 |
| write_transactions | 15 | 28 |
| write_commits | 15 | 28 |
| commits | 26 | 47 |
| rollbacks | 0 | 0 |
| transaction_ns | 44233749 | 126204585 |
| sealed_inserts | 31 | 97 |
| sealed_body_bytes | 5028447 | 20126440 |
| checkpoint_ns | 2852125 | 1787375 |

Statement/commit/transaction/save stages overlap. Commit wall34,774,082 /104,273,083ns is nested, not an added bucket; write commits15/28 are not observed fsync counts. Actual sealed INSERT bytes5,028,447 /20,126,440. Physical sync/write syscalls and VFS name UNAVAILABLE. Raw driver.stderr includes all nine C2 stage calls/walls, recent saves, profile/readback and namespace capacities.

Exact inventory matches:100346 objects/5,028,076 canonical B;10002003 objects/20,187,652B. Retained-db EXPLAIN and QUERY PLAN use systemSQLite3.51.0, matching the product runtime. Locator, pack and ordinal reads use primary-key searches; the reference scans only the one-row temporary read-scope, not the object table. Main database hashes remain unchanged. EXPLAIN programs are not actual execution VM counts. The diagnostic is explicitly populated/possibly warm after performance; these Stores never serve a later arm.

Promising mechanism: redundant transaction splitting charges the represented canonical bytes plus payload body bytes together. The old separate-body port did not count payload bytes there; the current shared Arc body owns no extra whole-body copy. The design requires separately explicit represented/physical charges under existing limits. This and unused reservations across saves merit a new bounded treatment after count attribution. WAL/FULL/fullfsync remain required; no timeout/worker/buffer relaxation is authorized.

Baseline actual statement/transaction/VM counts are UNAVAILABLE in these uninstrumented speed rows. Existing earlier Phase4.5 count diagnostics remain labelled by their original observer/source/fixture identities, and are not same-window time evidence. A new labelled count-driven reference diagnostic is still required for the finer disjoint mechanism decision; no unchanged-arm speed repeat is allowed.

| Remaining registered selection | Result |
| --- | --- |
| Init10000 | NOT_RUN |
| Init100000 | NOT_RUN |
| History stride10/17 states | NOT_RUN; budget/actual driver/proof pending |
| History stride3/53 | NOT_RUN; same |
| History stride1/157 | NOT_RUN; same |

All-seven goal remains ACTIVE. Source checkpoints carry exact LOC: bd9 143628->140936(-2692), b604140936->137448(-3488), e97 137448->137475(+27), c80137475->137475(+0). Reference65417 unchanged; current core72058, active28014/inactive reference44044. No push/PR/merge/CI/preflight/third-party patch or relabel.
