# #231: release-only four-tier SDK Init selection

> Frozen before changing the active benchmark build or collecting a release
> row. Owner direction on 2026-09-24 supersedes the earlier debug-only Core
> SDK Init profile. Historical debug receipts and frozen contracts keep their
> recorded identities and statuses; none is relabeled or resampled.
> **Verifier amendment:** the five-second limit below applies to this
> historical v3 cohort. The prospective [v4 verifier treatment](SDK-VERIFIER-PIPELINE-20260924.md)
> selects 9.5 s for new receipts; old five-second results remain unchanged.

This selection uses the four exact `core-sdk-init-fixture-v2` cases from the
[registry](../../../../benchmark/fs-bench-pro/families/init_namespace.py):
`namespace-100-compact-v3` (100 files, 5 MB),
`namespace-1000-compact-v3` (1,000, 20 MB), `namespace-10000`
(10,000, 300 MB), and `namespace-100000` (100,000, 500 MB), seed 1. Run
one explicit `--case` sample each, in that order, under fresh output,
Store and History paths. Prepared source masters may be reused only outside
the timer. Structured-text variants remain unselected. Keep the default
two-case `--family init_namespace` membership; the larger cases are
explicit unregistered diagnostics until a separate admission contract.

**Only locked Cargo release binaries are allowed for Core SDK Init
measurement from this selection onward.** The sole `runner.py` builds
`benchmark_init` and `verify_namespace` with
`cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release`
and archives executables from `target/release/examples/` under verified
SHA-256 identities. A cache record must state `build_profile=release`;
a debug artifact, an unmarked old cache or a path under `target/debug/`
cannot satisfy reuse. There is no debug selector or fallback. Mark new
case/run receipts with an append-only release-specific schema and profile;
the existing debug v2 receipts remain immutable.

The driver makes one public `Client::init_project` call. Its monotonic
timer begins immediately before the call and ends after the typed result;
source scan/read, C1/C2 Saves and C5 publication stay inside. Launch,
Store/History setup, preparation and independent verification stay outside
the operation timer but within their declared scopes. The verifier reopens
Store/History and checks all paths, portable metadata, logical bytes and
SHA-256 against the sealed manifest. Preserve one performance attempt,
separate verifier and all failure/timeout/cleanup evidence. Keep the
15-second complete-command, 5-second verifier and 30-second build limits;
do not change worker counts or enlarge a bound after a miss.

This release-profile selection retains
`source-cache-uncontrolled-v1`: it has no validated whole-input cold
contract and no approved numeric SDK latency gate. Even a functional PASS
is `admission_eligible=false` and performance `INELIGIBLE`; the raw time
is not a cold-source result or a matched comparison with v0.1.6 or the
historical daemon-host route. The 100,000-file 2.7-second cold Init target
remains an unmet separate gate. #231 closes only with four prospective
eligible performance PASS rows, full verifier, cleanup and custody PASS.
Do not promote this cohort or the earlier debug receipts to that status.
