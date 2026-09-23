# #237: external pack segment direction stopped

> **Status:** Owner stopped this direction on 2026-09-23: **pack payload must
> remain inside SQLite**. No segment-format product source was copied into
> the main #237 research branch, and no future segment treatment is selected.

After the [format feasibility analysis](segment-store-feasibility.md), an
isolated schema-11 prototype was built in
`/Users/yifanxu/.codex/worktrees/issue237-segment-prototype/layerfs` at
`abf22d567`. A separate BLOB control used the seven-COMMIT SQLite-pack
product at `343e4e029` with the same corrected benchmark harness. **One**
fresh control and **one** fresh candidate public 10k operation ran before the
owner's instruction to keep packs in SQLite; neither was repeated. The compact
[control](evidence/segment-cancelled/control/receipt.json) and
[candidate](evidence/segment-cancelled/candidate/receipt.json) receipts,
performance rows and pre-call cache sidecars are retained here. The original
Stores and complete run directories remain in their isolated worktrees; they
were not copied into Git.

| Exploratory arm | SQLite BLOB control | Segment candidate |
| --- | ---: | ---: |
| Raw public caller | 1.773005209 s | 1.168286875 s |
| Runner status | `DIAGNOSTIC` | **`INCOMPLETE`**, telemetry/stderr loss |
| Final source payload residency | 0 resident pages | 0 resident pages |
| In-run full verifier | `SKIPPED` | `SKIPPED` |
| Independent full reopened readback | `NOT_RUN` | `NOT_RUN` |

These rows used different physical Store formats, had unqualified metadata
cache state and lack independent readback; the candidate is incomplete. The
raw time difference is **not** a validated speedup or a reason to change the
owner's chosen format. This branch remains isolated as a canceled feasibility
experiment. The active v0.1.6/Core [comparison](v016-v017-common-source-results.md)
and all prospective optimizations keep pack bytes in SQLite BLOBs, use 4-KiB
database pages and retain the 128-KiB small-content cutoff.
