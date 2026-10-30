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
- The 0.1.3 Developer Preview is an explicit exception to the
  earlier blanket Store-format promise: it migrates schema 4 to schema 5.
  Canonical identity remains unchanged, but migration is one-way and runtime
  components must be version-matched. The [0.1.3 release contract](../release-notes/0.1.3/release-contract.md)
  defines the upgrade boundary; do not infer downgrade or mixed-version support.
- The owner-approved 0.1.4 Developer Preview is also an explicit Store-format exception: new Stores use schema 7, supported development schema 6 remains legacy, and released schema 5 is rejected without migration. Canonical identity is preserved. The [0.1.4 release contract](../release-notes/0.1.4/release-contract.md) defines this boundary.
- The owner-authorized v0.1.5 Developer Preview is the current scoped
  canonical/Store-format exception: new Stores use **ordinary schema 10**
  storage (4 KiB pages, exact CAS, compression, bounded whole-file small
  content below 128 KiB, bounded delta chains, large-file CDC/extents, compact
  scoped namespaces and pooled metadata), while supported schema-6/7/8/9 Stores
  open without promotion. **Explicit compaction is removed by the 2026-09-11
  owner decision** — there is no compaction API or binary in this release, and
  previously compacted Stores remain readable through the retained
  authenticated LFCNT1 read path. Canonical identity is preserved. The
  [v0.1.5 specification](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/0.1/0.1.5/spec.md) and the
  [v0.1.5 release contract](../release-notes/0.1.5/release-contract.md)
  define this boundary. Benchmark evidence below is single-sample campaign
  evidence, not a universal speed or storage guarantee.
- The owner-approved v0.1.7 Developer Preview is a broader exception: it
  replaces the v0.1.6 CLI and public Rust SDK with the JavaScript SDK, changes
  the daemon protocol, and moves new Stores to schema 16–18 or 22–24 with
  `layerfs/commit/v3` identity. Every older Store, including v0.1.6 schema-10
  Stores, is refused without migration. The owner released it as-is with
  recorded waivers; see the [v0.1.7 release record](../release-notes/0.1.7/README.md).
  After 0.1.7 the planned next release is 1.0.0; the 0.2 and 0.3 plans were
  discarded on 2026-10-15.
- A pre-1.0 minor release such as `0.2.0` may define a revised public or
  storage contract and must document its compatibility boundary explicitly.
- A 1.0-or-later major release follows ordinary stable semantic-versioning
  expectations for incompatible public changes.
- LayerFS 1.1.0 is an owner-waived exception (2026-10-24): it breaks 1.0.0
  JavaScript SDK code without a new major version. The waiver covers 1.1.0
  only; see the [1.1.0 release record](../release-notes/1.1.0/README.md#owner-waiver-of-the-100-major-version-promise).

Pre-1.0 releases may evolve quickly. Any compatibility promise must still be
stated explicitly in that release's manual; silence is not a promise.

## JavaScript package versions

The four `@layerfs/*` packages are versioned as one LayerFS release set. Choose
the next set version from the current release policy and the compatibility
impact of all package changes: use the next planned release version, and do not
ship a package independently at a different version. Keep `@layerfs/agent`,
`@layerfs/backend`, `@layerfs/runtime`, and `@layerfs/cli` on the same exact
version because backend/runtime depend on the agent's internal surface and the
CLI calls its public surface.

For a `1.x` set, a change is breaking if it removes or changes an exported
entrypoint, export path, method, argument/result shape, documented error or
observable behavior; changes CLI command names, arguments, output or exit
semantics; breaks backend/runtime compatibility with the matching agent; or
changes Node/platform/addon/worker requirements in a way existing supported
installations cannot satisfy. A compatible correction or additive export is a
minor release when it adds public surface, and a patch release when it fixes
behavior without changing the documented contract. Any incompatible change
requires the next major version, except an explicit owner waiver recorded with
the release. A new major may also revise the Store or daemon compatibility
boundary, which must be stated in that release's manual.

Package-specific compatibility boundaries are:

- `@layerfs/agent`: `.` and `/internal` exports, SDK methods and their
  arguments, results, errors, Store/daemon behavior, and native-addon/worker
  requirements. The internal export is deliberately unstable for external
  consumers, but changing it incompatibly still requires a coordinated set
  release because backend and runtime depend on it.
- `@layerfs/backend`: its root export, `LayerFSBackend.of`, and the `store` and
  `daemon` call groups, including each call's inputs and results.
- `@layerfs/runtime`: its root export, `LayerFSRuntime.open`, runtime settings,
  lifecycle methods, worker startup and platform behavior.
- `@layerfs/cli`: the `layerfs` binary, command names and arguments, JSON/text
  output, stdin/file handling, and exit codes.

The published `1.1.0` is the documented owner-waived exception above; it does
not change this rule for the next version.

## Evidence policy

Performance claims must come from the released source or an exact recorded
source seal, use public operations, keep comparison boundaries matched, retain
every valid preregistered sample, and disclose setup excluded from timing.
Exploratory measurements may guide engineering but are not release claims.

All benchmark specifications, execution, evidence, and reporting must satisfy
the [LayerFS benchmark rules](benchmark_rules.md). An authenticity, timing,
family-completeness, memory-attribution, custody, or claim-mapping failure
blocks publication independently of numerical performance.

The v0.1.5 release reuses the complete #120 campaign rather than re-running it.
Targets the owner waived for v0.1.5 (unrelated-history 500 < 15 s, cold Init
2.7 s, Stage2 K10, and the K32000 historical-family reporting target) are
recorded as waivers with their exact measured values, scope, reason and residual
risk in [docs/release-notes/0.1.5/waivers.md](../release-notes/0.1.5/waivers.md).
A waiver retires that target for this release only; it does not convert the
measured result into a pass, and it does not lower the target for any other case
or for a future release.

The [0.1.2 benchmark report](../release-notes/0.1.2/benchmark-results.md)
records complete SDK edit evidence and the release-source namespace/Store
refresh. Its explicitly approved tolerances and sampled-memory limitations are
part of the published claim, not strict-threshold passes. Earlier release
records remain immutable under `docs/release-notes/`.
