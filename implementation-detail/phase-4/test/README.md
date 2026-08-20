# Phase 4 fast test lane

This directory defines the small, repeatable test and performance loop used
while optimizing Phase 4. It does not contain generated fixtures, SQLite
databases, release executables, or benchmark results.

The governing rule is:

```text
make one small change
  -> run the affected correctness tests
  -> screen only the affected operation
  -> retain or revert
  -> run the larger checkpoint gate only for a winner
```

## Scope

The workflow lane covers:

- durable full-file write;
- fixed-size same-count edit;
- count-changing edit as a structural guard;
- warm materialization;
- fresh-process materialization;
- range read; and
- reopen.

Performance fixtures are deterministic 1, 10, and 100 MiB sources. Small
topology and malformed-input fixtures remain the authority for semantic
correctness. A large successful workflow does not replace boundary, identity,
tamper, transaction, reconciliation, or exact-Q tests.

## Contents

- [FAST_TEST_CONTRACT.md](FAST_TEST_CONTRACT.md) defines schedules, gates,
  resource limits, retention, and stop conditions.
- [run-phase4-fast.sh](run-phase4-fast.sh) is the initial deliberately narrow
  CP-0001 runner. It executes the accepted K64/F64 100-MiB full-write row, emits one
  strict JSON record per sample, enforces time and output checks, and deletes
  transient row state.
- [run-phase4-fast-v2.sh](run-phase4-fast-v2.sh) records capture-only 1/10/100-
  MiB durable writes plus one complete checkpoint round trip.
- [run-phase4-fast-v3.sh](run-phase4-fast-v3.sh) is retained with CP-0003's
  invalid 10-MiB same-count classification and must not be rerun.
- [run-phase4-fast-v4.sh](run-phase4-fast-v4.sh) records the corrected edit,
  materialization, range, and reopen baseline.

The active `--fast-*` CLI hard-binds K64/F64. The old exhaustive `--campaign`
and `--prepare-fixtures` entry points fail immediately. Archival campaign code
remains callable only through a deliberately hidden command plus explicit
environment override so the old evidence remains reproducible without making
the hard-grind path accidental. Do not add profile selection, artifact sealing,
or report generation to the fast runners.

Reports and raw JSONL belong in
[`../test-checkpoint-report/`](../test-checkpoint-report/).

## Non-negotiable boundaries

- Preserve canonical bytes, IDs, frozen FastCDC, COW, root, delta, closure,
  transaction, reconciliation, typed-error, and durability semantics.
- Use one SQLite transaction and one COMMIT for durable publication.
- Never report a same-count edit as whole-file throughput.
- `fresh-process` means a new process and SQLite connection with no LayerFS
  application cache. It does not claim that the operating-system page cache is
  cold.
- Successful transient databases are independently checked and then deleted.
- No ordinary screening run may retain a database, generated fixture, copied
  authority file, or release executable.
- No ordinary screening command may run longer than 120 seconds.
