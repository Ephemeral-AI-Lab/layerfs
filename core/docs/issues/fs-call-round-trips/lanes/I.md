# Lane I (Init) notes — #818

> **Status:** Working notes for lane I; measured values are one sample each.

Branch `rt818-lane-i`, base `54778271cd` (Phase 0 merge). PostgreSQL database
`rt818_i` in container `lfs818-rt-pg` (port 55818). Pins:
`engine/crates/layerfs-store-service/tests/init_round_trips.rs` and
`init_round_trips_10k.rs`. Counts are test profile; Init timing rows are
release builds (D15) and are run by the orchestrator per milestone.

Counts below are `[connects, round_trips, statements, transactions]` of the
Store's one session (SQLite `[0, 0, statements, transactions]`).

## I1 — logical size from the scan

### Prediction (written before coding)

Mechanism: delete Init's `logical_size` walk (tree open, inode-leaf windows,
one root open per regular file). Saves the per-file locates past the
4,096-entry locator cache at 10k, plus leaf and pooled-value reads.

| Shape | PG trips base → predicted | SQLite stmts base → predicted |
|---|---|---|
| `nested_1k` | 229 → ~205 | 308 → ~290 |
| `flat_1k` | 2,246 → ~2,225 | 2,284 → ~2,270 |
| `hardlink_window` | 705 → ~700 | 487 → ~482 |
| `dir_600` | 1,438 → ~1,425 | 1,428 → ~1,415 |
| `nested_10k` | 10,955 → ~606 | 12,164 → ~2,000 |
| `flat_10k` | 31,039 → ~20,700 | 31,881 → ~21,700 |

Roles: Init uses one Store session; all of the cut is on that session's reads.
Transactions unchanged (the walk's reads add no write transactions; PG reads
may be autocommit, so any change is a recorded finding). Logical, canonical
and root unchanged. Expect the 10k run-to-run spread to vanish (exact pins).

### Measured (test profile, one sample; load 5.7)

Receipts: `receipts/I1/init_round_trips*.log` (first run against the base
pins) and `receipts/I1/check/` (the check set after the pins moved).

| Shape | PG trips base → predicted → measured | SQLite stmts base → predicted → measured | Within 15% |
|---|---|---|---|
| `nested_1k` | 229 → ~205 → **209** | 308 → ~290 → **288** | yes |
| `flat_1k` | 2,246 → ~2,225 → **2,226** | 2,284 → ~2,270 → **2,264** | yes |
| `hardlink_window` | 705 → ~700 → **704** | 487 → ~482 → **486** | yes |
| `dir_600` | 1,438 → ~1,425 → **1,426** | 1,428 → ~1,415 → **1,416** | yes |
| `nested_10k` | 10,955 → ~606 → **624** | 12,164 → ~2,000 → **1,837** | yes (+3% / −8%) |
| `flat_10k` | 31,039 → ~20,700 → **20,720** | 31,881 → ~21,700 → **21,560** | yes |
| `mixed` (new) | — → **255** | — → **175** | new case |

- PG full rows: nested_1k `[0,209,98,33]`, flat_1k `[0,2226,2105,37]`,
  hardlink `[0,704,491,82]`, dir_600 `[0,1426,1305,37]`, mixed
  `[0,255,129,38]`, nested_10k `[0,624,335,120]`, flat_10k
  `[0,20720,20381,144]`. Transactions and prepares unchanged; every cut is
  one statement = one trip (autocommit reads on the one session).
- Logical, canonical bytes and roots identical to base on both backends;
  `logical == logical_size(reader, root)` holds in every case, including
  `mixed` (hard links within and across directories, a hard-linked empty
  file, tiny/whole/chunked files, three symlinks, empty directories:
  1,100,005 bytes = 5 + 2 × 100,000 + 3 × 300,000).
- **10k variance:** gone. The two runs (pin run and check run) gave identical
  10k counts on both backends, so the run-to-run spread was the walk
  (locator-cache hits depending on worker finish order), as inferred. The
  10k pins are exact (`SLACK` = 0).
- Exit (init.md I1): 10k nested 624 ≤ 650, cold; byte identity both
  backends; oracle passes.

Check set (once, `receipts/I1/check/`): init pins (1k, 10k), 
`commit_batched_base_files`, `commit_batched_reads`,
`commit_similar_tiny_files`, `postgresql_phase_attribution`,
`mount_round_trips` (all Init in setup), `sdk_round_trip_equivalence_seeded`
(incl. the 4,200-entry Init), `method_refusal_order`: pass.
`postgresql_round_trips` **fails** at `workspace_reader_open`
(`round_trips >= statements`: 11 trips, 12 statements). Not reached by I1:
that scenario runs on a fresh `Opened::open` (new connections, empty
caches) over Store rows I1 does not change (the walk was read-only); the
mount's pipelined admission (#768) sends more statements than trips.
Pre-existing, recorded for the orchestrator; not diagnosed further here.

No sweep: I1 adds no server message (it only removes reads).
Deleted tests: none. Production LOC: 227,595 -> 227,606 (delta +11), engine
layerfs-project +11.
