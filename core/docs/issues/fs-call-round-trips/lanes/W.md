# Lane W (writes): notes

Branch `rt818-lane-w`, base `54778271cd` (Phase 0 merge). PostgreSQL database
`rt818_w` in `lfs818-rt-pg`. Pin file
`engine/crates/layerfs-store-service/tests/sdk_write_round_trips.rs`.
Counts are `[connects, round_trips, statements, transactions]` per role
(writer, control reader, Workspace reader); SQLite rows count statements.
Baseline = the Phase 0 pins (RECORD.md Phase 0a, W0 attribution
`receipts/P0/w0-*-phases.txt`).

## W1 — hidden-path check from the kept markers

Prediction (written before coding). The eager `Visibility::for_stack` +
`node_serial` + `TreeTable::open` in `write/files.rs` `change` run on the
control reader. With the bind's kept markers (`KnownRemovals`; both backends
keep them under a mount admission) the call builds `Visibility::from_markers`
and opens the base tree only on the refusal path. W0 attributes the control
reader's `files.change` work on every write case to exactly these reads.

| Case | PG control before → after | PG total before → after | SQLite control stmts before → after |
|---|---|---|---|
| every `.warm` | [0,10,6,2] → [0,4,2,1] | −6 (overwrite 22 → 16, rm.dir 148 → 142) | 14 → 6 (tx 4 → 2) |
| every `.cold` | [1,17,7,2] → [1,7,3,1] | −10 (its 4 prepares go too; overwrite 75 → 65) | 14 → 6 |

Writer and Workspace reader unchanged. Hand-off expectation: −5 on an
overwrite (4 verified + 1 inferred `TreeTable::open`); W0 shows the tree open
costs 2 (locate + span) on the uncached control reader, so −6 warm.

**Measured** (`receipts/W1/pins-measured.log`, then `sdk_write_round_trips.log`
green on the new pins). Exactly as predicted on every case and role, both
backends: PostgreSQL control [1,17,7,2] → [1,7,3,1] cold and [0,10,6,2] →
[0,4,2,1] warm; SQLite control 14 → 6 statements, 4 → 2 transactions. Writer
and Workspace reader unchanged.

| Case | PG trips before → predicted → measured | SQLite stmts before → measured |
|---|---|---|
| `write.overwrite.cold` / `.warm` | 75 / 22 → 65 / 16 → 65 / 16 | 148 / 62 → 140 / 54 |
| `write.new_file.cold` / `.warm` | 112 / 44 → 102 / 38 → 102 / 38 | 152 / 75 → 144 / 67 |
| `mkdir.cold` / `.warm` | 113 / 46 → 103 / 40 → 103 / 40 | 158 / 82 → 150 / 74 |
| `rm.dir_100_recursive.cold` / `.warm` | 207 / 148 → 197 / 142 → 197 / 142 | 251 / 177 → 243 / 169 |

Within 15%: yes (0% off). Against the hand-off's −5: −6, the tree open is 2
trips (locate + span), as W0 attributed.

- Decisions: D16 kept. Kept markers present → `Visibility::from_markers`, no
  marker or serial read, base tree opened lazily (control reader) only when
  `edit`/`cp`-from met `NoSuchPath`. Kept markers absent → today's eager
  `for_stack` + `node_serial` + `TreeTable::open`, unchanged. Narrowing on
  the kept path only: a transient failure of the base-tree open no longer
  refuses a change that never asks for it (the Workspace reads the same root
  itself); no answer or refusal on a healthy Store changes.
- Accessor `BoundWorkspace::kept_markers` (removal.rs) checks `gc_op`, the
  bound node and maps a node kept as absent (or one this Workspace
  published) to (0, no markers); `commit_removals` shares it.
- New test `tests/sdk_write_kept_markers.rs` (both backends): one kept marker
  and then 71 markers over `KEPT_REMOVAL_BYTES` (fallback); `edit` of the
  hidden path `Removed`, of an absent one `NoSuchPath`, a write commits; the
  fallback's control reader does strictly more (PG kept [13, 2] vs fallback
  [15, 6] control statements; SQLite [17, 6] vs [23, 16]).
- Checks (once): `sdk_round_trip_equivalence_seeded` (186 + 17 steps equal on
  both backends), `method_refusal_order`, `sdk_path_removal`,
  `sdk_path_whiteouts`, `sdk_path_reads`, `sdk_path_hidden_semantics`: green.
- No new or changed server message: no sweep. Deleted tests: none.
- Production LOC: 227,595 -> 227,620 (delta +25), engine
  layerfs-store-service +25.
