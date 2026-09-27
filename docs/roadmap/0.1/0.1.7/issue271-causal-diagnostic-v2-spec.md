# Issue 271: atomic Commit cause record, one new diagnostic identity

> **Status:** Prospective specification, committed before the source and runner change.

The first [causal diagnostic](issue271-causal-diagnostic-spec.md) has a retained
[INCOMPLETE receipt](../../../../core/docs/issues/271/evidence/causal-v1/run/receipt.json)
at source `4bedea97bf8b612f4bf2ff7ff238ad110fb53113`. Its public route,
4,097 FUSE WRITE callbacks, full old/new-head oracle and cleanup passed.
The daemon's `LFS_COMMIT_SOURCE_CAUSE` line collided with a concurrent LFT1
line on stderr. The strict parser correctly refused that Commit source record.
The first row and its cache-ineligible timing remain unchanged and visible.

## One bounded correction

Give both Commit cause lines one assembled byte buffer and one write syscall
each to stderr. Each record is below Linux `PIPE_BUF`, so it cannot be
interleaved with the telemetry queue's 512-byte writes. A short/error write
leaves cause evidence missing and the row `INCOMPLETE`; it never changes the
Commit outcome. Do not alter source traversal, bytes, payloads, workers,
cache state or the Service protocol. Keep all Exec instrumentation as it is.
The runner must reject malformed or duplicate cause fields and require the
original counts and timing fields.

Name a separate `diagnostic4097causev2` selection and scenario
`issue271-separated-4097-cause-v2`. Keep the same closed 8,194-byte master,
independent writable byte-copy clone, static writer, 4,097 separated one-byte
`pwrite`s, public Mount → one Exec → explicit Commit route, 512-WRITE
snapshots, one construction worker, 30 s product Exec deadline, full
independent old/new-head and byte verifier, clean close, and four upstream
Service calls. Rebuild changed locked-release daemon and linked host examples,
validate the old master with the new verifier, and pin source, product,
harness, binary, image, workload and spec identities before the attempt.

The complete command timeout is 60 s only for this named count diagnostic;
the separate verifier remains 9 s. The original public gate remains 25 s and
its FAIL is not revisited. Cache is uncontrolled, so all new latency remains
`INELIGIBLE` and `admission_eligible=false`. Use one fresh output directory,
one attempt at this new source identity, and append-only evidence. The second
row exists to recover a missing causal field, not to select a speed number or
replace the first row. If this source record is unavailable again, retain the
second `INCOMPLETE` result and make the architecture decision from the
remaining counts and source audit.
