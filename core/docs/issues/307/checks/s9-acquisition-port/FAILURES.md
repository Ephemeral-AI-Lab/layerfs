# Acquisition port checkpoint (A3): retained failures

> **Status:** Historical receipt. Append-only; do not relabel as passes.

1. **`01-project-first`: exit 101.** `wide_nested_aliased_root_equals_the_whole_namespace_constructor`
   panicked at a new assertion, `again.namespace_work.read_units > 10 * work.read_units`.
   The assertions before it had passed: the root through 37-row read windows
   equals the root through 512-row windows, the largest window was 37 and the
   backing ended empty. The ratio was this test's own guess. Point reads and each
   stream's final empty window do not scale with the window size, so the real
   ratio is lower. It was a defect in the new test, diagnosed from that output,
   not a product failure. The bound is now 5 times. Cargo stopped at this test
   binary, so the later Project test binaries did not run in that invocation;
   `02-project-second` ran all of them once.
2. **`11-linux-clippy-failed`: exit 101.** `tests/init_sqlite.rs` imported
   `SqliteAcquisitionSchema` unconditionally; its only users are macOS-gated, so
   Linux Clippy refused the unused import. The import is now gated.
   `12-linux-clippy` passed.
3. **`15-linux-persistence-failed`: exit 101, not corrected.** The whole
   Persistence test package on Linux: `tests/history_allocation.rs` failed 3 of 3
   bodies at `tests/support/mod.rs:63` with
   `catalog creation: Integrity("persistence open")`, and Cargo stopped there.
   The global Store is macOS-only and 14 Persistence test files are not
   platform-gated. A3 changed neither that file nor the Store open path, but the
   command was **not** run at the parent commit, so that it also fails there is
   inferred from unchanged source, not demonstrated. A2's Linux receipt ran only
   the two acquisition test files. The acquisition test files execute 0 bodies
   on Linux.
4. **`05-harness-before` and `06-harness-after`: exit 1, not corrected.**
   `test_phase7*.py` discovery reports one error and one failure both before
   (32 tests) and after (34 tests) the harness edit:
   `test_phase7_cold_native` cannot create its scratch under an absent
   `target/phase7-agent`, and `test_phase7_history_vehicle` requires a release
   verifier that is not built in this checkout. Both are local prerequisites of
   tests this change does not touch. Every other test, including the two added
   here, passed.
5. **Product defect found by this checkpoint's profile, corrected.** Running
   Init over the provider showed the Session's automatic re-prepare count rising
   with the number of read windows (3 before A3, 18 and 36 for 100 and 1000
   files). `03-reprepare-probe-before` attributes it: every statement with a
   plainly bound `LIMIT` re-prepared once per execution, including with
   unchanged values, and the two removal statements once each. A2's profile
   had not counted re-prepares. With `LIMIT ?n+0`, `04-reprepare-probe-after`
   shows zero for every unit, the Init runs return to 3, and the plans are
   textually identical to A2's. The permanent profile test now asserts zero.
   The probes were temporary test files and are not in the tree; only their
   output is retained.
6. **`19-smoke-old-cli`: exit 1, expected.** The Init example given the previous
   argument form (with a scratch directory) refuses with
   `explicit durable/disposable profile required` before it creates a Store.

No test reached its explicit wall ceiling (at most 118 s on the host, 110 s
inside Docker).

## Limits of this evidence

- The cleanup, definite-failure and unknown-outcome tests inject a failure in
  the external memory backing. No capacity failure and no quarantined SQLite
  Session was induced through Init.
- The memory backing serves 512-row windows for every stream; the SQLite
  provider serves at most 62 rows where a row may carry a native path. Window
  resumption is covered on the memory backing at 37 rows and on the provider
  only at its own sizes.
- `17`/`18` are one functional smoke of the release example on a 305-entry tree
  with an uncontrolled cache. Their times are not measurements.
- `10-create-probe` is a temporary probe's output: 62 against 67 statements,
  72 against 90 full-scan steps and 13 against 15 sorts for creating a Store
  without and with the acquisition tables.
- Storage, SDK, Persistence and tooling ran before the last source edit, which
  gated one import in a Project test file they do not build. Host Clippy, the
  workspace test build, Project tests, fmt and the guard ran after it.
- No time, page, journal, synchronization, RSS or cold-cache figure exists for
  this change. The eight new Init case identities are registered and unsampled.
