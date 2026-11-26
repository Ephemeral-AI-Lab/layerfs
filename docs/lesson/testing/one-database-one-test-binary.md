# One database, one test binary at a time

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** testing · **Applies when:** running PostgreSQL-backed test binaries outside CI, especially several at once against one test database · **From:** [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822)

## Rule

Run PostgreSQL-backed test binaries that share one database one at a time, or give each its own
database. A failure from a parallel run against one database comes from an invalid setup: diagnose
it, then check that binary alone before you call it a product failure.

## What happened

PR #822's full Linux test pass built 354 test executables from `layerfs-persistence`,
`layerfs-store-service`, `layerfs-workspace` and `layerfs-overlay`. It ran them five at a time,
each binary in its own fresh container, all against one private PostgreSQL 17 database. Six
binaries failed (seven tests). Each was then run alone against the same server:

| Binary | Symptom in the parallel run | Cause | Alone |
| --- | --- | --- | --- |
| `commit_fault_race_kill` | `BusyOp(OpConflict { kind: Compaction, scope_kind: Store, ... })`; the test runs no compaction | lock interference | 4/4 pass |
| `writer_reopen` | `compact(2)` refused with the same Compaction `OpConflict`, held by another pid | lock interference | 9/9 pass |
| `tester768_stale_reader` (2 tests) | "no key held after the writer's backend ended" lists `ShareLock` keys of another backend; a leftover key count | lock interference | 5/5 pass |
| `kept_removals_bound` | remove refused `Busy` "removal at 3161…" on a project id, another pid | lock interference | 1/1 pass |
| `store_upgrade_postgresql_wire` | "cut 16: schema 3 with rows changed" | not established | 2/2 pass |
| `commit_similar_tiny_files` | `commit.overwrite` writer statements 15, pinned 16 | stale pin, real | fails, also at base `db54e9d75f` |

The four lock failures named Busy refusals and advisory keys held by a backend that the test did
not own. The receipt traces them to fixed ids shared across test rigs (the project id `3161…`)
and to PostgreSQL advisory locks, which are scoped to the database, not to a schema. A private
schema per test does not separate them. The exact colliding key was not traced.

The upgrade wire test restores its fixture into a private database per cut
(`engine/crates/layerfs-persistence/tests/support/pg_fixture.rs`,
`FixtureDb::restore`) and showed no lock symptom. It passed alone,
but the run was also under heavy host load (1-minute load 16.6 at the start), so its cause stays
open.

The sixth was real. `commit_similar_tiny_files` failed alone too, and failed the same way at the
PR's base `db54e9d75f`: its PostgreSQL pins had been stale since #816 Stage 2. Commit `207b4f63cc` updated
them, and the binary then passed 2/2.

An earlier attempt was discarded before any of this. It used the URL form of the PostgreSQL
address and had no source bind. `FixtureDb::restore` builds its database address as
`{admin} dbname={name}`, which only extends the key=value form, and the tests read `sql/` and
`fixtures/` from the source tree. Those failures were setup defects, not results.

## How to spot it

- Errors that name `Busy`, `OpConflict`, an advisory key, `ShareLock` or a pid the test did not
  start, such as a Compaction conflict in a test that never compacts.
- Failures in tests that the change cannot reach, in a run of many binaries at once.
- A test that passes alone and fails only in the parallel run.
- A runner that starts several PostgreSQL binaries at once against one `LAYERFS_TEST_POSTGRES_URL`.
  cargo-nextest runs binaries in parallel ([test speed-ups](../../general/test-speed.md#4-run-the-selection-cheaply)); CI's nextest lanes set no PostgreSQL URL.

## What to do

- **Run as CI does.** The only CI job with PostgreSQL is `store ext4 (--full, PostgreSQL 17)` in
  [store-ext4.yml](../../../.github/workflows/store-ext4.yml), scheduled or dispatched by hand.
  It runs [test_store_ext4.sh](../../../tools/ci/test_store_ext4.sh) `--full`: one `cargo test -p
  <package>` per package, in sequence, so cargo runs that package's binaries one after another.
  Tests inside one binary still run on parallel threads; the suites are written for that
  ([per-test bindings](../../architecture/engine/store/testing.md#compaction-tests-and-per-test-bindings)).
- **Or give each binary its own database.** Advisory locks do not cross databases. If each binary
  runs in a fresh container, do not name its database from the process id alone: pids repeat
  across containers.
- **Use the key=value form** that CI sets, for example
  `host=127.0.0.1 port=5432 user=postgres dbname=<db> sslmode=disable`
  ([testing.md](../../architecture/engine/store/testing.md#postgresql)). Bind the source tree
  into the runner.
- **Diagnose before you rerun.** Read the failure and the test source first. Then run that binary
  alone, once, as the valid check. Record both runs: the parallel failure stays in the receipt,
  labeled as a setup artifact, beside the run alone.
- Follow the test rules in [AGENTS.md §4](../../../AGENTS.md#4-code-build-and-docs): build with
  `--no-run` first, stop every test command at 120 s, no repeat loops, and never weaken an
  assertion to make it pass.

## Limits

- A failure that persists alone is real, as `commit_similar_tiny_files` was. Fix the cause or the
  stale pin with its own evidence.
- A pass alone removes both the shared database and the host load. It shows the failure needs the
  parallel run; it does not show which of the two caused it.
- Running alone is not a retry to pass. One run alone per failing binary, after diagnosis; a
  second unchanged run proves nothing more.
- Serial runs take longer. Split by package and `--test` so each command stays under the 120 s
  ceiling.

## Evidence

[PR #822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822). Receipts on the owner's machine:
`2026-11-26-commit-batching/full-tests-ec201d014c/` (`SUMMARY.md`, `results.tsv`, the single-binary
logs in `serial-reruns/`, and the discarded first attempt in `pass0-invalid-env/`). The pin fix is
commit `207b4f63cc`.
