# T10: the §11 report and `store.status`

> Implementation note for the frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md)
> §10/§11. The spec wins; `§N` cites it. No contract is added here.

## Placement

- `engine/crates/layerfs-persistence/src/report.rs` — the backend-neutral
  `CompactReport` (with `CompactGeneration`, `CompactKeptWaste`,
  `CompactResidue`, `CompactZeroReason`), the `CompactReportSource` trait that
  projects a backend receipt into it, and `CompactStatus`, the rendered
  `store.status` projection of a run. The sidecar reader itself is T12.2's
  `compact_op_status`/`CompactOpStatus`; T10 extends that projection with the
  run's `started_ms` and owner (`pid`, kernel start, boot).
- `control/crates/layerfs-store/src/report.rs` — the one serializer:
  `compact_report` and `compact_status`. Every key is snake_case (§10; §11
  names are logical) and `CompactZeroReason::wire_field` names each zero field
  by its rendered key.
- `CompactReceipt` is unchanged: `From<&CompactReceipt>`/`From<CompactReceipt>`
  and the trait project it. The control renderer no longer builds a map itself.
- `layerfs_store::sqlite::transfer::step::StoreStep::CompactOp` reads only
  `<store>.compact-op`; it travels the same `store.file` frame to a worker, so
  a remote SQLite Store answers it without opening the database, scanning the
  directory or touching an artifact.

## Report keys

| §11 name | rendered key |
|---|---|
| `path` | `path` (`none`/`rebuild`) |
| `generation` | `generation {before, after}` |
| `deadBytesBound` / `deadBytesRefined` | `dead_bytes_bound` / `dead_bytes_refined` |
| `logicalDiscarded` | `logical_discarded` |
| `backendReusable` | `backend_reusable` |
| `physicalShrink` | `physical_shrink` |
| `keptWaste` | `kept_waste {dead_in_retained_packs_bytes, delta_bases {bytes, references}, inode_blocks_bytes}` |
| `peakTempBytes` | `peak_temp_bytes` |
| `residue` | `residue {bytes, artifacts, reason?}` |
| `estimate` | `estimate` |
| `zeroReason` per zero field | `zero_reasons [{field, reason}]` |

`kind` is `Compacted`. `residue.bytes` is what is **left**, not what was
removed; `clear_artifacts` now adds a size only when a removal left a file, so
a successful settle reports an empty residue of 0 bytes (T10 fixes the T8
accounting).

## `store.status`

While a compaction runs, or after a dead owner left one, the call adds:

- `op` — `{id, name, phase, started, uncertain}` from `<store>.compact-op`
  (SQLite) / the Store's `gc_op` (PostgreSQL, T9). `id` is the recorded
  16-byte operation id rendered as `op-<32 lowercase hex>`, the sidecar's
  `op_id`/`gc_op`'s `gc_intent.op_id` (or the `lfs_gc_<hex>` scratch name when
  the intent is released), never a row number or a new id;
- `state` — `busy` while the owner lives, `unresolved` once it is gone
  (otherwise the existing `idle`/`in_use`/`busy`/`down` state). `uncertain`
  is the engine projection's own fact and is true for **every** record that
  remains, live or leftover; `state` is what separates the two. Equating
  `uncertain == true` with `unresolved` is wrong.
- `phases` — the backend's ordered phases: `mark`, `build`, `verify`,
  `install` on SQLite (T9 adds `mark`, `build`, `swap`, `cleanup`).

No compaction record means `op: null` and `phases: null`, and a transfer
`op` still renders exactly as before. The sidecar now records the owner
`pid`, kernel start time and boot id; the status step compares them with
`layerfs_home::process::alive_as`, as leases do, so a reused pid cannot pass
as a live owner. The run writes `mark`, then `build`, `verify` and `install`
as it enters each phase, so the phase is visible while it runs (AC-51).

## PostgreSQL hook (T9)

`CompactReportSource` is the hook: SQLite's `CompactReceipt` implements it,
and T9 implements it for its PostgreSQL receipt. The report struct names no
backend-only field, so PostgreSQL reports its zeros through the same keys
(`backend_reusable = 0`, `dead_bytes_refined == dead_bytes_bound`) without a
wire change. `store.status` reads the Store-side `gc_op` at the same point in
`status.rs`; nothing is read from a file there.

## Metrics and SLO signals (T10.3, PostgreSQL T10.4)

`engine/crates/layerfs-persistence/src/compact_metrics.rs` publishes the
compaction observations from the real `store.compact` and `store.status`
paths:

- counters: `compact.runs`, `compact.operations{outcome}`,
  `compact.refusals{code}`, `compact.settles{kind}`,
  `compact.retries{reason}`, `compact.status{state}` and
  `compact.slo{objective,result}` (`residue_free`, `generation`);
- duration aggregates: `compact.phase.duration_ns{phase,statistic}` for
  `mark`/`build`/`verify`/`install` on SQLite and
  `pg_mark`/`pg_build`/`pg_swap`/`pg_cleanup` on PostgreSQL, and
  `compact.duration_ns{statistic}`, each with `count`, `sum` and `max`;
- the last successful run's §11 gauges: `compact.dead_bytes{kind}`,
  `compact.kept_waste{part}`, `compact.kept_waste_bytes`,
  `compact.residue_bytes` and `compact.generation{which}`.

Labels come only from closed enums: outcome, phase, refusal code, settle kind,
retry reason, status state, SLO objective/result, statistic and report key. A
Store name, path, operation id or artifact name is never a label, so metric
cardinality is fixed. A SQLite `none` run times only `mark` and a PostgreSQL
`none` run only `pg_mark`; a rebuild times all four phases its backend names;
a failure or unknown run records its outcome and total but publishes no report
gauge; a run that settles an earlier leftover adds `leftover` to
`compact.settles` and `compact.retries`. A `Busy`/`Unresolved` refusal that
stops a SQLite call before a run is counted once by the control path, and a
refusal a measured run itself produces (either backend) is counted by that
run.

PostgreSQL §8 emits the same observation from
`backend/postgresql/compact.rs`: its `RunMetrics` times Phase M/A/B/C as
`pg_mark`/`pg_build`/`pg_swap`/`pg_cleanup`. Those are distinct closed values,
not aliases of the SQLite phases, so a process total never blends a SQLite
phase with a PostgreSQL phase of another meaning. It records the same §11
gauges from the PostgreSQL receipt (where `dead_bytes_refined ==
dead_bytes_bound`, `backend_reusable = 0` and `residue` is empty) and maps
`Busy`/`BusyOp` to `compact.refusals{code="busy"}` and `Unresolved` to
`code="unresolved"`. A resumed `G + 1` leftover records Phase C, the receipt's
original `generation {before, after}` and the `leftover` settle/retry; a `G`
leftover is settled before the new round.

The registry follows the telemetry crate's rules (fixed low-cardinality
labels, recording never changes a result). `store.compact` has no new public
metrics call: `snapshot()` reads the process totals and
`CompactMetricsCapture` captures one thread, which the tests use.

## Tests

- `engine/crates/layerfs-persistence/tests/compact_metrics.rs`: a successful
  SQLite rebuild (all four phase durations, outcome,
  dead/kept-waste/residue/generation gauges and both SLO checks), the `none`
  path (only `mark` timed, generation kept), a stopped `AfterBuild` run
  followed by the leftover settle (`uncertain` outcome, then `rebuild` +
  `retries{leftover}` + `settles{leftover}`), and a definite failure with no
  report gauge.
- `engine/crates/layerfs-persistence/tests/postgresql_compact.rs`: the same
  capture on a real PostgreSQL Store: a successful rebuild (the four `pg_*`
  phases, outcome and the same report gauges and SLO checks), the `none` path
  (only `pg_mark`), a Busy swap refusal (`failed` +
  `refusals{code="busy"}`, phases M/A/B, no report gauge) and an uncertain
  leftover intent refusal (`failed` + `refusals{code="unresolved"}`, no phase,
  no report gauge).
- `engine/crates/layerfs-persistence/tests/support/metrics.rs`: the one
  closed label-key/value assertion both capture tests use.
- `engine/crates/layerfs-persistence/tests/sqlite_compact.rs`:
  `the_report_projects_every_section_11_field`,
  `the_empty_store_report_explains_its_zeros`,
  `a_leftover_op_reports_its_phase_and_state` (a real `AfterBuild` failpoint:
  phase `build`; `status(true)` is `busy`, `status(false)` is `unresolved`,
  both uncertain; ordered phases, then the next run settles the artifact and
  the record).
- `control/crates/layerfs-store/tests/compact_report_status.rs`: the report
  wire is the snake_case contract shape (including `zero_reasons` and the
  `delta_bases.references` hex), and `store.status` projects an idle Store,
  a live owner's record and a leftover one (`op`, `state`, `phases`), and the
  `compact.status{state}` metric of each read.
- `engine/crates/layerfs-persistence/tests/postgresql_status.rs` (T10.5): on a
  real PostgreSQL Store, an idle Store projects null; a live non-idle owner
  projects `busy` with `uncertain`; a gone owner projects `unresolved`; each
  `mark`/`build`/`swap` failpoint projects its recorded phase from the scratch
  name after `gc_intent` is released; and a completed run leaves no row.
- `control/crates/layerfs-store/tests/postgresql_status.rs` (T10.5): the same
  projection through the control `store.status` call on a real PostgreSQL
  Store: no row projects `idle` with `op`/`phases` null; a live
  `pg_stat_activity` owner projects `busy`; the gone owner projects
  `unresolved`; each of `mark`/`build`/`swap`/`cleanup` is projected from the
  row; the bounded `compact.status{state}` samples are emitted once each; and
  a Store whose row was removed is idle again.

## Gaps

- The compaction metrics are emitted into the process registry only; no SDK
  call or CLI command renders `snapshot()` yet.
- PostgreSQL compaction metrics cover the run and its admission/refusal paths.
  T10.5 supplies the PostgreSQL `store.status` source: `status.rs` reads the
  Store-side `gc_op` with its `gc_intent` on one status query (no Store open)
  and projects `busy`/`unresolved` and the `mark`/`build`/`swap`/`cleanup`
  phases, so `compact.status{state}` now has both sources.
- No control-level test joins `store.compact` and `store.status` in one run:
  the control case writes the rows a compaction leaves and checks the
  projection, while the persistence suites run the compactions themselves.
- The `{value, cap}` capped-field shape is not produced: neither backend caps
  a field in 2.0, so AC-52 does not claim it.
