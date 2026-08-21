# Research prompts while WP4-M is running

Status: user-authorized concurrent research. These prompts prepare later Phase
4 decisions; they do not alter, interpret, or preempt the active WP4-M profile
campaign.

Active task: `codex://threads/01a01eb9-0d14-73c1-9a1f-e97687f9420b`
(`Complete WP4-M profile campaign`).

## Safety boundary

The active campaign froze the repository at commit
`d781173a08ab4092eb539c3a0870056e6c6a77ff` and classified `research/` as
unrelated user-owned dirt. Running these prompts is allowed to add only the six
assigned `report.md` files below. Every other repository path is read-only.

Each task must:

- treat partial WP4-M code, rows, and commentary as incomplete and
  non-authoritative;
- use `git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` or sealed
  custody copies when the live tracked file is dirty;
- avoid Cargo, Rust builds, tests, SQLite commands, compression tools,
  filesystem experiments, performance counters, and any command that writes
  under `target/`;
- avoid messaging, interrupting, steering, or waiting on the active task;
- label material claims `Observed`, `Derived`, `Hypothesis`, or `Unavailable`;
- use primary papers, specifications, and official implementations for
  external technical research;
- write only its assigned report and leave its prompt unchanged.

Do not launch a task while WP4-M is executing measured release rows. If a
campaign begins after a task starts, pause that task's local shell commands and
repository writes; web research and reasoning may continue. Resume only after
the measured rows are quiet or the active task is terminal.

## Prompt and output map

| Task | Prompt | Sole output | Purpose |
|---|---|---|---|
| A | [Profile-promotion order](task-a-profile-promotion-order/prompt.md) | `task-a-profile-promotion-order/report.md` | Decide whether canonical v2 must precede WP4-P promotion |
| B | [Whole-source witness authority](task-b-witness-authority/prompt.md) | `task-b-witness-authority/report.md` | Decide whether the inner whole-source digest is independently necessary |
| C | [Memory semantic ceiling](task-c-memory-semantic-ceiling/prompt.md) | `task-c-memory-semantic-ceiling/report.md` | Define the minimum Memory/SQLite semantic-parity boundary for WP5-WP9 |
| D | [300-MiB/s cost model](task-d-300-mibs-cost-model/prompt.md) | `task-d-300-mibs-cost-model/report.md` | Build a row-wise, non-promissory route from accepted F2 to the stretch target |
| E | [Canonical-hash execution](task-e-canonical-hash-execution/prompt.md) | `task-e-canonical-hash-execution/report.md` | Identify credible ways to reduce the remaining required canonical-hash wall |
| F | [SQLite page/overflow model](task-f-sqlite-page-overflow/prompt.md) | `task-f-sqlite-page-overflow/report.md` | Model 4/8/16-KiB physical profiles without opening or timing SQLite |

## Parallel launch order

Run A, B, and C together. When capacity is available, run D, E, and F. They
own disjoint reports, but their filesystem writes will still appear as
concurrent user-owned research drift to WP4-M.

No task may turn its report into an implementation specification. After WP4-M
closes, reconcile all six reports against its terminal profile evidence before
authorizing WP4-P, canonical v2, Memory parity, CDC/hash work, or SQLite page
experiments.
