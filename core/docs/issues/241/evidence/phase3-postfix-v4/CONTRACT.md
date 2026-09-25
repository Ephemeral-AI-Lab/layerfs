# #241 prospective Phase 3 position selection v4

Status: Prospective functional selection frozen before the first mounted case.
Source commit: 8aaf623835cf1384fbf5558e868b8d1b945830bd.
Plan: core/target/issue241-v4-position-plan.json, SHA-256 db2996e758637b00f9c2f1b5c31a3264ee96b03a50e81a7c128b57b31de519c9.

Run the unchanged 264-position manifest once at this source: 66 ordered cases at each of 1 MiB, 10 MiB, 100 MiB and capped 500 MiB. Use the locked release position binary and immutable Linux image in the plan, exact sealed SDK Init masters, and one independent writable byte copy per size. A size stops at its first failure, writes NOT_RUN receipts for the rest, and the other declared sizes still run once. Retain every output and cleanup outcome. One construction worker; original deadlines and cache policy. Cache is uncontrolled, so this is functional qualification only and provides no latency admission. The historical v3 229 PASS / 1 FAIL / 34 NOT_RUN remains unchanged. The four release Edit→Commit cases and separate verifier gates remain outside this selection.
