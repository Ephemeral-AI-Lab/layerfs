# Lean phase-3 gap closure

The lean phase-3 GC authority is done for the core (0 new tables, 4 `gc_intent`
columns, ordered `MachineRecord.incarnation` / `SandboxRecord.generation`,
machine-level tombstone, retained `gc_intent` `done` row audit, compact
`Copy::Done`, per-machine 2.0 conversion). This note closes the four residual
gaps, all with lean (no new Store table).

## G1. Shared-PostgreSQL cross-home proof

Today `control::layerfs-store::place_view::from_home` reads only the caller's
home; on a shared PostgreSQL Store, another machine's rows stay
conservative-live.

Lean closure: the Store record's `machines.<machine>` entry may record that
machine's own **`home` path**, an optional record field (no new table). The
control layer builds the PlaceView from the caller's home and every recorded
`home` it can read: each home's `MachineRecord.incarnation`,
`SandboxRecord.generation` and `machine/<machine>/sandbox-retired/*.json` are
merged into one bounded view, ordered by the ordered token and capped at 4096
places. An unreachable, relative or unrecorded home contributes no proof. The
worker/PG admission carries this merged view (the existing `place_view`
frame); no new table.

Acceptance: a shared-PG Store with two machine homes proves the older
incarnation dead across homes; a Store whose second home is offline stays
conservative-live; the bound (4096 places) holds.

## G2. Done-row audit retention

Today done rows accumulate and no archive exists (design row 20, Low).

Lean closure: add `store.repair_mount_holders` action `archive` that exports
the bounded done rows (default all, page-limited) to
`<home>/log/gc-audit.jsonl` and then, only with an explicit `--prune`,
deletes the exported rows. Default is export-only, no auto-delete. Optional
later: a `(scope_kind, scope_id, state)` partial index; not required now.

Acceptance: archive writes a JSONL and keeps the rows; `--prune` removes only
the exported pages; admission is unaffected.

## G3. 2.0 conversion

Withdrawn (pre-merge review): the Store keeps the base format exactly. A
live mount row stores its place tokens in the existing `gc_intent.evidence`
column (`place:v1;inc=<token>;gen=<token>`), so no schema version bump,
refusal or conversion tool exists, and `tools/store_gc_convert.py` was
removed.

## G4. Durable preview row

Today preview is call-log only (design row 8, Medium).

Lean closure: make `preview` write a retained `gc_intent` `done` row with
`action = "preview"`, `op = null`, the operator label and no owner columns,
reusing the release/done row path; no new table, no admission change.

Acceptance: preview leaves a done row readable by the bounded `gc_intent_done`
read; repeated previews are idempotent per call; admission still filters
`state <> 'done'`.

## Out of scope

The live 6 branches / 331 retired reclamation is an operator run (settle,
release, `branch.remove`, `store.compact`), not code. The shared-PG operators
still declare `--gone` machines explicitly.
