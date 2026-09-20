# #210 independent implementation review evidence

> **Status:** Research; informative and not a product contract.

Reviewed head: `92e56635ae4559d175fe3cd455f36f9fe6b5b498`.
Base: `a02168adbb1b02571941654919cefca12dbc1f42`.
Contract: `c85cf6b69b3809d860caaad764a09e86a54ece9a`.
Review date: 2026-09-21. Verdict: **not ready**.
The [remediation specification](../../../../../../core/docs/architecture/proposal/commit-history/remediation-20260921.md)
maps all R01–R15 findings to work packages and regression requirements.

## Provenance and interpretation

The reviewed worktree was clean at the recorded head before and after verification.
The reviewer ran the commands, inspected the governing contract and source, and
created independent counterexamples. Author handoff figures were inputs to check.
The original evidence directory was `/tmp/layerfs-pair2-review-92e56635-dczjaq`;
this directory preserves the selected raw artifacts and diagnostic sources so a
new implementation task does not depend on that temporary directory surviving.
Historical paths in logs identify where those commands actually ran.

An exported copy at `<original-evidence>/probe-tree` contained unchanged product
source and four additional external test files. A byte comparison of production
Rust/SQL against the reviewed tree found no differences. Probe databases were
disposable; the terminal-allocation and open-validation cases intentionally
prepared boundary/corruption states through an external SQLite connection.
No product fault hook, third-party modification or performance run was used.

**Probe PASS means reproduction, not acceptance.** The diagnostic assertions
confirm the defective baseline behavior. Remediation tests must assert the
required corrected result instead of preserving these diagnostic assertions.

## Observed checks

All Cargo dependency-resolving commands used toolchain 1.85.1, `--offline`,
`--locked` and the core workspace. Exact invocation forms appear in the
remediation specification's verification section.

| Check | Fresh observed result | Raw evidence |
| --- | --- | --- |
| Workspace tests | 580 passed; 97 executable invocations plus 6 doc-test invocations; no failures | [test log](cargo-test.log) |
| Examples / bins | Exit 0 | [examples](cargo-examples.log), [bins](cargo-bins.log) |
| Formatting | Exit 0, empty output | [fmt](cargo-fmt.log) |
| Clippy all targets, warnings denied | Exit 0 | [clippy](cargo-clippy.log) |
| History without default features | Exit 0; contract separation only | [no-native](cargo-no-native.log) |
| Boundary guard / Python self-tests | 193 files; 6 tests passed | [guard](boundary.log), [tests](python-tests.log) |
| Host production daemon/service route | 13 cases PASS, fresh output directory | [receipt](history-route.json), [output](history-route.log) |
| Initial reviewer probe build | Exit 101: E0753 from including a test file's inner doc comments; reviewer harness error, no product changes | [failed attempt](probes.log) |
| Corrected catalog/codec/service probes | 9 diagnostics reproduced their observations | [probe results](probes-run2.log) |
| Native delivery / oversized page | Both defects reproduced | [delivery results](delivery-probes.log) |
| Different Layer provenance | Integrity refusal, original row preserved | [provenance](provenance-probe.log) |
| Concurrent catalog Commit | One success, one Busy, exact losing stage preserved | [concurrency](concurrent-commit.log) |
| DDL constraint probes | Required unique/CHECK/composite-FK violations rejected | [SQL results](schema-probe.log) |
| Per-commit production LOC | All six first-parent comparisons matched | [snapshot totals](loc-summary.json) |

The original service binary SHA-256 for this review's rebuilt host run was
`f5d99f2514acb2e03e31b4e0fce4a3ea60c8466536f2291528b0b3dcc662b651`.
It is not an assertion about the author's older binary or an executable supplied
by this documentation commit. Fresh remediation receipts need their own identities.

## Diagnostic source and replay

- [Catalog probes](catalog-probes.rs): long-name pagination, live Commit/Layer
  growth, a recomputed cursor digest selecting sibling ancestry, terminal
  reservation admission, incomplete-schema/binding reopen, stale-base publication,
  immutable provenance and concurrent Commit admission.
- [Codec probes](codec-probes.rs): a 71-byte Branch record and an 85-byte genesis
  record encode, then fail decoding under the 120-byte count pre-check.
- [Delivery probes](delivery-probes.rs): an authenticated peer sends unexpected
  ResultData for legacy ConstructFile; another case accepts a 22,918-byte history page.
- [Service probe additions](service-probes.inc.rs): legal empty-child/interleaved
  manifests fail; GetBranch accepts a catalog root absent from C2.
- [SQL probe](schema_probe.py): exact input rows for declarative constraints.
  Its frozen absolute schema path identifies the reviewed worktree; change that
  path only in a scratch copy if replaying elsewhere.

To replay, export the reviewed commit into a fresh scratch directory and add
`catalog-probes.rs` as `core/crates/layerfs-history/tests/review_probes.rs`,
`codec-probes.rs` and `delivery-probes.rs` as the corresponding bridge test files.
For the service probe, concatenate that commit's existing external
`core/crates/layerfs-service/tests/history.rs` and `service-probes.inc.rs` into
`tests/review_service.rs`. The catalog uses that commit's existing `tests/support`
helper. No product source or dependency edit is required.

From the scratch `core/`, run the diagnostic test target(s) with
`cargo +1.85.1 test --offline --locked -p <package> --test <target> review_ -- --nocapture`.
Use the scratch tree's own Cargo target. Port useful cases to proper external
regressions on the remediation branch; do not add this entire snapshot as a
duplicate test suite or copy product implementation into a test.

## Gaps and qualifications

- H04 real service/C2 overlap and reverse-completion failure schedule was NOT_RUN.
  The catalog concurrency probe does not satisfy it.
- H06 independent-stack upload overlap and H08 actual boundary/identical-root
  failure schedules remain unverified. Source proves exact-finish-before-stage;
  C5 unknown-outcome handling separately has a confirmed source defect.
- Linux Docker history route and H14 component substitution were NOT_RUN. During
  review, Linux targets were installed for the pinned toolchain; no source-matched
  Linux image was established. A missing-target explanation is not current evidence.
- Read-only reopen and no-native compilation do not establish writable restart,
  crash recovery or Windows/WASM/cloud persistence.
- The original review did not inspect the remote PR body. During remediation
  planning, #210 and #212 were fetched: both remained open, PR #212 still named
  the reviewed head, and its earlier M1–M3 DONE/H-case coverage statements were
  not acceptance evidence against the counterexamples.
- No performance, release admission, historical-author-run or historical
  third-party registry-integrity claim is made by this evidence bundle.

Source anchors are preserved in [source-anchors.txt](source-anchors.txt).
[review-manifest.json](review-manifest.json) and
[revision-final.json](revision-final.json) retain the original identity records.
[sha256.json](sha256.json) hashes the copied artifacts; this README is explanatory
metadata added during remediation planning, not a relabeling of the raw receipts.
