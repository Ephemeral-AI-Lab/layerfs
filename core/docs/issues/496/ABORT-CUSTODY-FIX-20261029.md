# Normal unmount abort custody

> **Status:** Dated implementation checkpoint; not release qualification.

PR #496 at `3a0564b6bb235dc7a0a62d732a5d8ec58e0ad865` ended a normal
unmount stalled at Join by attempting one abort and draining again. Its failed
abort return still carried the pre-abort `Undrained.forced`, and normal daemon
teardown supplied no abort facts to retained drain, Revoke or Close replies.

## Change

- Refresh the returned FUSE receipt from the session's recorded abort outcome.
- Report short/failed writes at `Abort`, with the original byte count/errno.
- Carry `Written` through a failed second drain, Revoke and Close. Normal
  admission supplies `commit: Absent`; its completed detach supplies `Detached`.
- Keep successful normal replies `Unmounted`. Keep control consumption, one
  attempt, drain predicates and custody ownership unchanged.
- Document that retained abort facts also apply to a stalled normal unmount.
  Update the SDK guidance: an unavailable or consumed control is not forceable.
- Record the expanded test's functional duration in the CI slow inventory while
  retaining its existing Linux mount lane.

No test was deleted. No dependency, wire encoding or persistence profile changed.

## Regression evidence

The new failed/short-write cases were run before the production fix. Both failed:
`stage: Join`, `forced: None`, generic drain detail. The existing success case
passed. The post-fix six-case binary passes:

1. Copied mount namespace: normal unmount completes, with a Written abort receipt.
2. Kernel-denied abort write: EIO and Failed remain in custody.
3. Zero-byte abort write: exact count and Short remain in custody.
4. An acknowledged write followed by another Join stop: Written remains in custody.
5. A failed owner job before Revoke: Written remains in Revoke custody.
6. An already-closed overlay: Revoke succeeds, Close refuses, Written remains in custody.

The write failures use a test-only thread-local seccomp filter on the exact
already-bound descriptor of the test's own connection. Case 4 uses seccomp user
notification to simulate a successful write without ending the connection; it
checks propagation after a second drain failure, not a naturally reproduced
kernel failure. No descriptor is replaced, no other connection is inspected,
and no test code writes fusectl. The copied namespace is released before final
assertions, and subsequent normal/forced replies preserve the stage, detail and
abort facts without another abort. Work counters remain live observations.

An initial Close fixture used a missing-object read and expected quarantine.
That expectation failed: definite missing-object reads do not quarantine the
reader. The fixture was replaced with a public owner Close to stage its exact
refusal, without weakening the expected custody assertion.

## Validation

Linux ARM64 Docker, Rust 1.85.1, locked dependencies, repository ARM64 flags;
Disposable Store explicitly selected by the fixture. Builds precede execution.
Every mounted invocation has a 100-second wall timeout; the FUSE/Bridge test
invocation has a 90-second execution timeout and tooling tests a 120-second cap.
These are functional tests, not a performance campaign.

- `stalled_unmount`: 6 passed (21.28 s); the strengthened success receipt assertion
  is additionally checked with only that case selected.
- `forced_unmount`: 3 passed; `forced_unavailable`: 1 passed;
  `fp21_detach_stages`: 2 passed.
- `layerfs-fuse` and `layerfs-bridge`, all targets: 33 tests passed.
- Warning-denying Clippy for daemon/FUSE/Bridge, all targets: passed; the final
  strengthened stalled-unmount test is checked separately.
- Product boundary: 956 production files passed; authorized fuser integrity
  passed; 155 tooling tests passed, followed by 6 CI-selection tests after the
  slow-inventory update.
- Rust formatting, diff whitespace and Ruff 0.17.2 formatting: passed.
  An optional Ruff lint invocation reported the pre-existing EXE001 on
  `tools/ci_tests.py` (the parent tree has a shebang and mode 100644).
- Durable: NOT_RUN — disabled by owner. No full SDK E2E or benchmark run.

Machine-local append-only receipts, including the original failures, are under
`~/.cache/layerfs-worktrees/layerfs-pr496-fix-8cc8b1c1603d/builds/`.
The pre-fix receipt is `pr496-mount-stalled_unmount-7f1b9e7200234dcda9fc2312db1e4b87`;
the complete passing six-case receipt is
`pr496-mount-stalled_unmount-1b8071b206174205b677283a4ff02124`.

## Production source size

Production LOC: 102777 -> 102816 (delta +39).
Core: 38395 -> 38395 (0); engine: 61723 -> 61762 (+39);
API: 2659 -> 2659 (0). Reference absent (retired R9c; last counted 65,417
at tree `498dd1917812`).

Method: `tools/production_loc.py` blob
`3341e4d0362c822b6d9a08419c432942b3a13800`, `--root <exact exported tree> --json`,
against the first parent and final staged tree. First-party production Rust,
runtime SQL and JavaScript only; tests, tooling, docs, generated and third-party
code excluded. Exact tree IDs and the confirmed comparison are in the commit
message. This is a source-size comparison, not a performance claim.
