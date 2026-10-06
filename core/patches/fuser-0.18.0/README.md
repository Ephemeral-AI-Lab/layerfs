# fuser 0.18.0 signed-timestamp patch

> **Status:** Owner-authorized dependency correction, 2026-10-06.

The owner explicitly requested: "use fuser 0.18.0 from crates io and apply patch".
This supersedes the earlier no-patch/registry-only ruling for this correction.

`core/vendor/fuser-0.18.0` is a complete copy of the official crates.io archive
with only `src/time.rs` changed. Its correction and regression tests are copied
from upstream commit
[e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7).
No Git dependency is selected. The published package manifest, version, license,
public API and other84 files remain byte-identical to the archive.

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
`patch --batch --forward -p1` using the recorded diff inside the extracted package;
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
