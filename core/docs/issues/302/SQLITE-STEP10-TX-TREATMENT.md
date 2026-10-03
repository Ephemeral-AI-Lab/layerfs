# Durable SQLite transaction treatment: four-tier qualification

> **Status:** Dated measurement checkpoint; all four revised Init rows FAIL.

Source75a77927f, locked release, one sample per required Init case/arm. Prospectivev2 complete-product clock includes fresh database creation/import/checkpoint/close. All exact roots match; sampled independent proof/content-page attestation/scratch cleanup PASS. Keep v1 rows, build failure and diagnostic receipts unchanged. No resampling or policy relaxation.

| Files | Product baseline/current ns | Exact10*current /11*baseline | Final allocation baseline/current B | Performance envelope baseline/current ns | Time /allocation /envelope |
| --- | --- | --- | --- | --- | --- |
| 100 | 41570583 / 95885000 | 958850000 / 457276413 | 7372800 / 5251072 | 63352166 / 931806583 | FAIL / PASS / PASS |
| 1000 | 130144458 / 227274084 | 2272740840 / 1431589038 | 23101440 / 20557824 | 322784209 / 427698000 | FAIL / PASS / PASS |
| 10000 | 1590900959 / 2614821875 | 26148218750 / 17499910549 | 314605568 / 314642432 | 3501107084 / 4518475291 | FAIL / FAIL / PASS |
| 100000 | 5518847875 / 9201830208 | 92018302080 / 60707326625 | 518029312 / 520343552 | 19301947333 / 21727073917 | FAIL / FAIL / FAIL |

| Current work | 100 | 1000 | 10000 | 100000 |
| --- | --- | --- | --- | --- |
| statements | 275 | 1283 | 14131 | 128523 |
| vm_steps | 37807 | 209947 | 2593948 | 15514080 |
| bound_bytes | 5104091 | 20469982 | 305503377 | 526841995 |
| statement_ns | 62899502 | 147966668 | 1509132400 | 4163628838 |
| commit_ns | 50726958 | 127525834 | 1207574352 | 2783380893 |
| transactions | 22 | 39 | 743 | 5747 |
| write_transactions | 11 | 20 | 135 | 402 |
| write_commits | 11 | 20 | 135 | 402 |
| commits | 22 | 39 | 743 | 5747 |
| rollbacks | 0 | 0 | 0 | 0 |
| transaction_ns | 60622293 | 149251707 | 1593161042 | 4994895345 |
| sealed_inserts | 32 | 99 | 1418 | 2287 |
| sealed_body_bytes | 5028727 | 20127016 | 301576625 | 506420473 |
| checkpoint_ns | 2831959 | 1503250 | 5047042 | 15695750 |

The treatment reduces acknowledged write work:15->11 and28->20 commits compared with the earlier100/1000 mechanism. It does not establish a latency improvement: this window has higher commit wall, and every actual speed gate FAIL. Commit/SQL/transaction/parallel construction spans overlap. Do not infer sync-system-call counts or exact CPU attribution from these spans.

10000 storage overage36,864B;1000002,314,240B. Do not waive these prospective no-growth gates.100000 complete envelopes19.302/21.727s exceed15s although product5.519/9.202s fits. Non-child preconditioning/collection terms13.774/12.513s need cause diagnosis. No deadline/workload/worker/cache change or moving mandatory work outside timers is allowed.

Namespace100000 actual retained capacities: entry15,385,064B, jobs/path25,737,184B, frontier330,768B, child-vector1,081,344B, serial808,008B, inode8,888,088B, directory-vector32,032B, change-vector5,160,960B. These exclude opaque allocator/BTree/PathName/DirEntry internals, worker/SQL/OS state and do not prove a total importer bound. Candidate per-child RSS146,800,640B versus reference137,887,744B covers whole compared child, not isolated Init-only heap.

100000 diagnostics show4597 individual payload reads/1,079,610,281B plus270,179,431 batched body B for500,000,000B source and506,420,473B inserted pack bodies. This indicates repeat dependency/reuse acquisition worth finer attribution, not permission to remove authentication or grow caches.10000 body reads126,603,056B, writes301,576,625B.

Post-performance EXPLAIN/programs and dbstat inventory captured from all eight retained owners with systemSQLite3.51.0; main hashes unchanged. This populated/warm diagnostic state never credits a later arm. EXPLAIN opcodes are not actual VM execution counts. Plans remain indexed; no object-table full scan is assumed. Baseline finer operational counts come from distinct labelled1000 diagnostic3542/323375VM versus current whole-lifecycle1307/210246VM, zero omissions. Counter scopes are explicitly different; neither diagnostic duration is a speed arm.

Further work: reduce required durable exchanges through bounded allocation/publication composition, attribute repeated payload/dependency reads, optimize cold-attestation mechanics without changing the cold contract, qualify/paginate count-growing collections where required, and bind actual17/53/157-state history operations/proof after the deadline ruling. All seven competitive goal remains ACTIVE. Histories remain NOT_RUN, not PASS.

Code treatment LOC137475->137501(+26), reference65417/core72084, active28040/inactive core reference44044, old191/new7630/rest64263. Exact snapshot method in75a commit. Evidence-only next commit unchanged+0. No CI/preflight/third-party patch/push/PR/merge.
