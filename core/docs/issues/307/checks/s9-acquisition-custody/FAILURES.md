# Acquisition custody checkpoint: retained failures

> **Status:** Historical receipt. Append-only; do not relabel as passes.

1. **`01-custody`: exit 101.** `failed_scratch_release_reports_its_cause_and_retained_runs`
   panicked with `expected a cleanup failure, found
   Err(Content(ResourceUnavailable { what: "ordering run file" }))`. The test's
   watcher made the scratch directory unwritable as soon as it appeared, before
   the first run file existed. The first run was refused, the empty scratch was
   released and removed cleanly, and Init correctly returned its construction
   failure with no cleanup failure. This was a defect in the new test, diagnosed
   from that output and the source, not a product failure. The watcher now waits
   until the scratch holds a run file. `02-custody` ran the corrected test once.
   The other four bodies of `01-custody` passed.
2. **Harness launch error, no test ran.** The first Linux Project command passed
   the Docker argument string as one unsplit word (zsh does not split unquoted
   parameters); the wrapper raised `FileNotFoundError` before starting any
   process. Its log name `05` was consumed and is empty, so it is not retained
   here. `06-linux-project` is the first command that executed.

No test reached its explicit wall ceiling.

## Limits of this evidence

- The failed-release case depends on a second thread denying writes while
  acquisition runs. It waits for a run file and stops at its own deadline, but
  it is an induced race, not a deterministic fault: a host that finishes
  acquisition before the watcher observes the scratch fails the test.
- The partial-append case re-executes the test binary under a shell file-size
  limit. The limit admitted 8192 bytes on macOS and 4096 on Linux; the test
  asserts the charge equals the bytes on disk, not a fixed number.
- The earlier review probe expected Init to succeed with its scratch inside an
  empty source. The correction refuses that placement instead, because creating
  the scratch there changes the source directory before anything is read.
