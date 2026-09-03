# LayerFS release policy

> **Status:** Current release policy.

LayerFS uses semantic versioning for the workspace packages and publishes one
immutable manual under `docs/versioned/<release>/` for each release.

## Release requirements

A release candidate is eligible only when:

- the workspace builds and all tests pass;
- formatting, warning-denying Clippy, and `git diff --check` pass;
- the public SDK and CLI reference match their exported surface;
- the SQLite schema and static SQL manifest pass their structural tests;
- supported FUSE and container-runtime gates pass on a capable host;
- the versioned manual and limitations are complete;
- every published benchmark links to reproducible raw evidence and identifies
  the exact source;
- the release tag resolves to the reviewed source tree.

## Version meaning

- A patch release within `0.1.x` preserves the documented public API, CLI,
  daemon protocol, canonical identity, and Store-format contract while
  correcting behavior or documentation.
- A pre-1.0 minor release such as `0.2.0` may define a revised public or
  storage contract and must document its compatibility boundary explicitly.
- A 1.0-or-later major release follows ordinary stable semantic-versioning
  expectations for incompatible public changes.

Pre-1.0 releases may evolve quickly. Any compatibility promise must still be
stated explicitly in that release's manual; silence is not a promise.

## Evidence policy

Performance claims must come from the released source or an exact recorded
source seal, use public operations, keep comparison boundaries matched, retain
every valid preregistered sample, and disclose setup excluded from timing.
Exploratory measurements may guide engineering but are not release claims.

All benchmark specifications, execution, evidence, and reporting must satisfy
the [LayerFS benchmark rules](benchmark_rules.md). An authenticity, timing,
family-completeness, memory-attribution, custody, or claim-mapping failure
blocks publication independently of numerical performance.

The [0.1.2 benchmark report](../../release-notes/0.1.2/benchmark-results.md)
records complete SDK edit evidence and the release-source namespace/Store
refresh. Its explicitly approved tolerances and sampled-memory limitations are
part of the published claim, not strict-threshold passes. Earlier release
records remain immutable under `release-notes/`.
