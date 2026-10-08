# fuser 0.18.0 authorized timestamp and lifecycle patches

> **Status:** Current general guide.

The owner explicitly requested: "use fuser 0.18.0 from crates io and apply patch".
This supersedes the earlier no-patch/registry-only ruling for this correction.

`core/vendor/fuser-0.18.0` is a complete copy of the official crates.io archive
originally qualified with only `src/time.rs` changed. Its correction and regression tests are copied
from upstream commit
[e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7).
No Git dependency is selected. The published package manifest, version and license
remain byte-identical. The original timestamp provenance below is immutable.
The later scoped lifecycle extension has separate provenance and qualification.

The input archive is
[fuser-0.18.0.crate](https://static.crates.io/crates/fuser/fuser-0.18.0.crate),
SHA-256 `b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`.
[provenance.json](provenance.json) pins all85 original files, the exact
[diff](signed-timestamps.patch) and the resulting time.rs. The focused
[integrity check](../../tools/check_fuser_integrity.py) verifies these pins and
both owning `[patch.crates-io]` declarations. Use it before native builds:

```sh
python3 -B core/tools/check_fuser_integrity.py
```

Cargo applies patches from the workspace root. The core workspace and independent
Linux prerequisite harness each select this exact path with `version = "=0.18.0"`.
The active core graph does not yet contain the replacement FUSE package; its patch
is recorded as unused. The prerequisite harness actually builds the patched crate.
The root reference workspace keeps its existing dependencies. A patched lock entry
has no registry source/checksum: the original archive checksum authenticates the
input, while the diff/file identities authenticate the changed dependency.

This checked-in copy and its patch survive fresh Cargo caches and clean checkouts.
Reconstruction is extraction of the pinned archive followed by one application of
`patch --batch --forward -p1` for signed-timestamps.patch, then one application of
the separately recorded session-lifecycle.patch inside the extracted package;
verify the result before using it. Builds do not mutate shared registry packages
or apply an unrecorded patch. No other dependency receives an exception.

Third-party source, including the corrected time.rs, is excluded from first-party
production LOC by the unchanged repository counter. It remains visible as a
dependency correction; it is not a first-party source-size reduction.

The [qualification record](../../docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md)
separates parser/reply correctness, actual mounted behavior, Linux timestamp
boundary canonicalization and the still-incomplete S8 product milestone. Retire
this patch only after an explicitly selected corrected release passes the owning
qualification; do not silently switch sources or weaken the timestamp contract.

## Receive-loop lifecycle extension, 2026-10-08

The owner [authorized this narrow extension](../../docs/issues/307/FUSER-LIFECYCLE-DECISION-20261008.md)
after inspection showed that the original public API could not report all-loop
startup or preserve all joins through its early error paths. The additive
`Session::into_runner()` API provides a `SessionRunner`, a `SessionMonitor` and
an exact `SessionOutcome`. Existing `Session::run()` behavior remains available;
new LayerFS native wiring must use the explicit runner for these lifecycle facts.

Each actual receive loop rendezvouses after buffer allocation and before reading.
Serving requires successful creation and entry of every configured loop, with
startup released by that same ordering. No ordinary kernel probe supplies the
entry witness. The first exit revokes Serving. The consuming runner retains and
joins every created thread, collecting original I/O errors and panic payloads;
cleanup runs after joins and retains its own panic independently. Observer
deadlines do not cancel, detach, retry or dispose the runner. A Stopping snapshot
is not complete join or daemon-drain evidence.

[lifecycle-provenance.json](lifecycle-provenance.json) pins the original archive,
unchanged timestamp provenance, [lifecycle diff](session-lifecycle.patch) and its
four exact changed/added files: session.rs, lib.rs, the new session/lifecycle.rs,
and the local changelog note. There are86 resulting package files. The focused
guard requires both exact provenance records and patches and rejects any other
changed, missing, redirected or added file. Its original timestamp constants
are unchanged. The [dependency proof receipts](../../docs/issues/307/checks/r2-fuser-lifecycle-20261008/18-proof-summary.json)
include public parser/loop fixtures, real resource-driven partial spawn failure
and one native mount/Ready/detach/join. Full LayerFS R2–R5 remains unfinished.
