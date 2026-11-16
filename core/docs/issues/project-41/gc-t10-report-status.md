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
  (SQLite) / the Store's `gc_op` (PostgreSQL, T9);
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

## Metrics and SLO signals (T10.3)

`engine/crates/layerfs-persistence/src/compact_metrics.rs` publishes the
compaction observations from the real `store.compact` and `store.status`
paths:

- counters: `compact.runs`, `compact.operations{outcome}`,
  `compact.refusals{code}`, `compact.settles{kind}`,
  `compact.retries{reason}`, `compact.status{state}` and
  `compact.slo{objective,result}` (`residue_free`, `generation`);
- duration aggregates: `compact.phase.duration_ns{phase,statistic}` for
  `mark`/`build`/`verify`/`install` and `compact.duration_ns{statistic}`, each
  with `count`, `sum` and `max`;
- the last successful run's §11 gauges: `compact.dead_bytes{kind}`,
  `compact.kept_waste{part}`, `compact.kept_waste_bytes`,
  `compact.residue_bytes` and `compact.generation{which}`.

Labels come only from closed enums: outcome, phase, refusal code, settle kind,
retry reason, status state, SLO objective/result, statistic and report key. A
Store name, path, operation id or artifact name is never a label, so metric
cardinality is fixed. A `none` run times only `mark`; a rebuild times all four
phases; a failure or unknown run records its outcome and total but publishes
no report gauge; a run that settles an earlier leftover adds `leftover` to
`compact.settles` and `compact.retries`. A `Busy`/`Unresolved` refusal that
stops the call before a run is counted once by the control path, and a refusal
the measured run itself produces is counted by that run.

The registry follows the telemetry crate's rules (fixed low-cardinality
labels, recording never changes a result). `store.compact` has no new public
metrics call: `snapshot()` reads the process totals and
`CompactMetricsCapture` captures one thread, which the tests use.

## Tests

- `engine/crates/layerfs-persistence/tests/compact_metrics.rs`: a successful
  rebuild (all four phase durations, outcome, dead/kept-waste/residue/generation
  gauges and both SLO checks), the `none` path (only `mark` timed, generation
  kept), a stopped `AfterBuild` run followed by the leftover settle
  (`uncertain` outcome, then `rebuild` + `retries{leftover}` +
  `settles{leftover}`), and a definite failure with no report gauge. Every
  capture also asserts closed label keys and values.
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

## Gaps

- The compaction metrics are emitted into the process registry only; no SDK
  call or CLI command renders `snapshot()` yet.
- T9's PostgreSQL rebuild has no metrics of its own yet.
- T9's PostgreSQL rebuild and its `gc_op` status read are not implemented;
  the trait and the status comment are the only PostgreSQL pieces.
- The `{value, cap}` capped-field shape is not produced: the SQLite receipt
  has no cap today.
