# E1 executable registration checkpoint

> **Status:** Implemented registration tooling; final43 focused tests and syntax
> compile pass in [integration receipts](checks/context-custody-e1-20261007/18-e1-final-tests.json). No sample, performance admission, E1 exit or S7/S9 completion.
> Prepared 2026-10-07, Asia/Singapore.

The [prospective roadmap specification](../../../../docs/roadmap/0.1/0.1.7/cluster-two-e1-registration-v1.md)
defines the scope under [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307)
and the [performance companion](PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md).
The existing [core runner](../../../benchmark/fs-bench-pro/runner.py) now exposes
read-only `register --family cluster-two-evidence`; it builds or samples nothing.
Its family definition, integer gates, registration validator and independent
fixture helper are under the existing `families/` and `shared/` ownership groups.
The [registry](../../../benchmark/fs-bench-pro/registry/cluster-two-evidence-v1.json)
keeps every one of the 27 proposals visible with current public routes, timing/
acknowledgement/cleanup boundaries, exact declared counts and numerical budget
selection, source-derived gates and explicit incomplete driver/observer inputs.

The two separately owned resume/speed notes were read and preserved. All retained
Init failures, history passes, #305 failures/ineligible/unrun selections and
their comparison identities remain unchanged. This checkpoint does not add a
new performance runner, algorithm, product dependency or sampled baseline.

## Read-only usage and schema

Run from the repository root. Creating a template prints JSON to stdout; save
it only into a fresh campaign-owned preparation directory when execution is
selected. It is an incomplete input, not a performance receipt.

```sh
python3 -B core/benchmark/fs-bench-pro/runner.py register \
  --family cluster-two-evidence --template --case E04-write-16m
python3 -B core/benchmark/fs-bench-pro/runner.py register \
  --family cluster-two-evidence --manifest <sealed-registration.json>
python3 -B core/benchmark/fs-bench-pro/runner.py register \
  --family cluster-two-evidence --manifest <sealed-registration.json> \
  --observations <retained-observations.json>
```

Admission prints all 27 rows. Selected incomplete rows return process exit 2;
omitted siblings stay NOT_RUN. Registration PASS never creates a sample or
proves a milestone. Unknown/non-numeric counters stay INCOMPLETE. Retained gate
FAIL is preserved even when another dimension is incomplete.

With `--observations`, the output uses `cluster-two-retained-numeric-v1`, per-row
`numeric_gate_status` and overall `retained_numeric_status`; generic evidence
admission/status PASS fields are absent. Qualification is NOT_EVALUATED because
actual completed outcomes, acknowledged timing, and bound oracle/cache/lifecycle
results are not yet checked. Even failed/unattempted receipts with passing scalars
cannot be described as qualified evidence. Its exit status concerns numeric
validation only, and never authorizes product/performance admission.

The template fixes schema, registry and boundary hashes, selected order and
owning budgets. Fill every selected row with checked artifact descriptors
`{"path": "repository-relative path", "sha256": "64 lowercase hex"}`.
Artifacts must exist inside the owning checkout and match their content hash.
The specification object identifies an already committed roadmap authority;
each row also supplies an already committed `cluster-two-gate-specification-v1`
JSON containing that case's exact `limits`, `service_classes`, `source_arm`,
prospective nonzero SHA-256-form `sample_id`, `attempt_contract` and
`resident_spread_contract`. The checker
compares it to the registration rather than accepting changed post-hoc numbers.

The identity requires exact source commit/tree, locked release build command and
root Cargo inputs, hash-linked product/compilation/dependency/harness/workload/
cache/observer manifests, all source artifacts and binary/image/kernel/topology.
`identity.environment.LAYERFS_CONSTRUCTION_WORKERS` is the string `1`, separately
from `identity.construction_profile`: ordinary routes have constructor limit 1;
Q02–Q06 native public Project Init has limit 4 and one Save producer, matching
the explicitly added `import/files.rs` pin. Observed started threads remain
distinct from limits/environment; native Init's actual four cannot be labeled one.
`driver.route_proof` is a sealed
`cluster-two-route-proof-v1` PASS receipt matched to route, public entrypoint,
source commit and binary, with integer-zero excluded/fallback counts. A missing
route driver or proof is INCOMPLETE; an excluded source vehicle is refused.
Selected route/observer pins must match current contents and declared source/
product inventories. Unimplemented observers with no source pins stay INCOMPLETE;
source changes need explicit registry-pin advancement. Both driver and independent
oracle commands must actually invoke their respective sealed binary.
Executable comparison resolves paths inside the owning root; aliases of the
exact sealed binary are accepted, while unrelated executable paths are refused.

Workload manifests use `cluster-two-workload-v1`, exact case and `parameters`.
Parameters must preserve every already frozen registry value and resolve every
null input. E04/E05 additionally provide `write_trace`, exactly equal to the
1,000 offset/length/input-hash rows generated by `shared.evidence_workload.write_trace`.
Fixture manifests use `cluster-two-fixture-v1`, case, seed 1, sealed independent
generator source, expected-state hash, `uses_candidate_result=false`, and actual
dense shape/byte counts where required. Schedules use `cluster-two-schedule-v1`,
case, horizon and finite `arrivals` with `at_ns`, `class`, `workspace`, `connection`
and `bytes`. Per-class offered counts/bytes must equal those derived from arrivals.

Numerical limits have explicit units and either a structured
`cluster-two-numerical-authority-v1` owning artifact or an eligible control formula.
Its `limits[anchor]` contains `value`, `unit`, the closed `meaning` from the owning
limit definition, and `scope` containing exact case/profile/route/limit. The
referenced entry must exist and agree with the registration. A bare hashed file
or invented anchor cannot supply numerical authority. The accepted calibration
receipt has `kind=control`, sample_count 1, admission_eligible true, cache/
correctness/resource/cleanup/status PASS, matched profile/route/workload/cache/
observer identities. Each numeric metric descriptor carries the same value/unit/
closed meaning/target scope; unrelated integer or byte observations cannot supply
a latency limit. The registered formula supplies integer
numerator/denominator/offset. No default millisecond, RSS, throughput or relative
tolerance is invented by the checker.

Retained observation JSON has schema `cluster-two-observations-v1`, exact registry
and complete-registration hashes, and `rows[case]` records. Each record uses
`cluster-two-row-observations-v1`; it binds case, full registration and individual
row hashes, registry, boundary, source commit/tree, workload/observer and binary
identities, with `metrics`, `resources.host/linux` and a hashed `raw` artifact.
The record excluding `raw` must equal that artifact's JSON exactly. Service
metrics are named `service.<class>.<metric>`.
Each row records `source_arm`, its frozen `sample_id`, sample_count 1, and an
ordered `original_attempts` list of `{attempt_id, receipt}`. The attempt contract
freezes scope `declared-public-operation-attempts` and exact count. Each receipt
uses `cluster-two-original-attempt-v1` and binds case/sample/arm/source/tree/binary,
attempt ID, operation index and single sample/attempt counts. IDs and receipt
hashes must be unique, including across selected rows. Known public-call counts
are registry-derived; mixed-load receipts cover the entire offered trace.

Resource witnesses declare phase scope, sampled or continuous peak, baseline/
peak/final bytes, first/opened/closed/last timestamps, sample count/interval,
largest gap, explicit clock identities/uncertainty and the complete ordered
`sample_timestamps_ns` and matching `sample_byte_values` inventories. First/last
bytes must equal baseline/final; the maximum derives the checked peak, which must
equal the gated host/Linux peak scalar. At least three consistent samples must cover
both boundaries and a guaranteed interior point under the recorded uncertainty;
largest-gap admission includes uncertainty. Different clocks require exact
`source_timestamps_ns` conversion through an attested
`cluster-two-clock-calibration-v1` PASS receipt with source/target clocks,
offset and bounded maximum error, supplied as `calibration_artifact` and matching
`clock_calibration`. Lifetime-only, uncovered or unrelated phase witnesses remain
INCOMPLETE. This validates observed evidence shape and gates; it does not provide
the still missing physical/residency observers.

`resident_spread_contract` freezes the exact cohort/domain operands and
`max(sum(owner phase peaks)) - min(sum(owner phase peaks))`. E04/E05 and
Q02–Q05 are their respective size cohorts; other rows use explicit singletons
with no cross-size inference. All operands require sealed original observations,
checked phases and matched arm/profile/source/product/binary/cache/observer
identities. The gated spread scalar must equal the computed value. Sums concern
separately observed owner peaks; sampled values stay sampled, and no simultaneous
aggregate peak is claimed. Every operand's registration_status must be PASS;
valid numeric limits cannot bypass a route/cache refusal. Missing or incompletely
registered operands remain INCOMPLETE.

## Required next registration work

E1 is still incomplete. The integration owner must provide these concrete inputs
before any corresponding sample can be admitted:

| Input | Exact residual obligation |
| --- | --- |
| Public drivers | Compile actual E/R/Q/P route vehicles and obtain identity-matched route proofs; R1/R4 supervision/attachment owns runtime assembly |
| Workloads | Resolve E06 read/edit/frontier positions, E07 exact release ordering, Q full shape manifests and Q01/R09/R10/P01 finite arrival schedules, P disconnect/full-device checkpoints |
| Deployment constants | Freeze owning latency/service/queue/debt/residency/physical/I/O/copy limits, using source contracts or prior eligible untouched controls and committed formulas |
| Observation | Assemble E2 complete original-operation SQL/native/copy/service accounting and correlated EXPLAIN; E3 host/Linux phase/page/device observations and precision/overhead proofs |
| Independent proof | Compile route-specific full oracle, source-removal and custody/cleanup checks within separate owning budgets; huge/dense cases cannot inherit component sample scopes |
| Q05 budget | Register a feasible owning complete-command contract without importing unrelated long history/old #305 allowances or shrinking 1,000,000 entries |
| Native/Commit extension | S8 request/open/lookup/reply/cache/lifetime attribution and S10 actual captured Commit remain dependent qualification dimensions |

These are registration/implementation gaps, not external gates or permission
requests. They do not prevent independent engine, runtime or observation work.
The committed specification is required before actual sample selection under
benchmark rules §1; this checkpoint authorizes no measurement from a template.

## Scoped verification handoff

The authoring agent ran no tests, build or performance command; the integration
owner serializes checks in the primary checkout. The integration owner reported
the initial 19-test selection PASS and compilation PASS, then requested stronger
clock/interior-sample and retained-observation binding checks. Those source fixes
and additional tests require their affected verification; earlier receipt outcomes
stay intact. The focused test file is
[test_cluster_two_registration.py](../../../benchmark/fs-bench-pro/tests/test_cluster_two_registration.py).
It covers retained cardinality/verdicts, incomplete drivers/limits, sealed artifact
tampering, eligible matched-control arithmetic/refusals, dense cohort input identity,
unknown counters, bounded service admission and phase precision/lifetime rejection.
Independent review also prompted peak/spread operand binding, executable oracle
binding, structured numerical authority and calibration meaning, exact source-pin
coverage, original single-sample attempt custody, and native Init constructor
profile corrections. The first final 40-test invocation
[01-e1-tests.json](checks/context-custody-e1-20261007/01-e1-tests.json) FAILED with
1 error, exit 1, no timeout, and complete wall 403,402,667 ns under its 60-second
stop. [Original stderr](checks/context-custody-e1-20261007/01-e1-tests.stderr)
records `test_oracle_command_must_invoke_its_sealed_binary`: its positive path
under macOS `/var` resolved to `/private/var`, so lexical comparison incorrectly
refused the same sealed verifier. The other 39 tests passed in that invocation.
The raw failure receipt/stderr remain unchanged.

The repair resolves owning-root executable identity for both driver/oracle,
with an alias regression retaining `/usr/bin/true` refusal. Separate review found
that numeric limits could let an incompletely registered spread operand through;
every operand now requires registration PASS, with a route/cache-refusal regression.
Review also narrowed the evaluator to numeric results, with a failed/unattempted
receipt regression proving that passing scalars emit no qualified-evidence claim.
The focused selection now contains 43 authored tests; the affected rerun remains
owned by integration, with no author-run test/build/sample. No repaired PASS is
claimed before that run, and no earlier receipt is relabeled.
Use an explicit wall timeout no greater than 120 seconds for this invocation:

```sh
python3 -B -m unittest discover \
  -s core/benchmark/fs-bench-pro/tests -p 'test_cluster_two_registration.py'
```

The runner template/registration smoke checks are read-only and must retain the
expected INCOMPLETE/exit-2 result as such. After checking and committing, append
exact commands/outcomes and source/tree identities to the owning tracker receipt.
This tooling/docs-only checkpoint changes no production LOC; compute unchanged
core/reference/combined totals from exact parent/staged trees for the commit.
