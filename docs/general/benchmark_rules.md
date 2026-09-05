# LayerFS benchmark rules

> **Status:** Current general guide.

**Permanent hosting rule:** benchmark SDK/coordinator, Workspace processing,
spool, and SQLite MUST run on the macOS host. Docker is only for Linux
daemon/FUSE/workloads. Docker-owned SQLite, prepared Store images, and
container-side benchmark coordinators MUST NOT be restored or executed.
Unsupported families require host migration, never a Docker fallback. This
rule supersedes older Docker-owned benchmark plans; historical results remain
evidence only for their recorded topology.


This document is the normative policy for LayerFS performance benchmarks.
`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative. A `MUST` or `MUST
NOT` violation makes the affected sample invalid and the campaign
admission-ineligible. Retain invalid and no-go evidence, but do not use it to
support a product or release claim.

Release-specific documents define exact families, cases, fixtures, and gates.
They may strengthen this policy but may not weaken it. See the
[release policy](release-policy.md), [0.1.x benchmark contract](../roadmap/0.1/benchmarking.md),
and [`fs-bench-pro` family format](../roadmap/0.1/0.1.2/fs-bench-pro-format.md).

The controlling principle is:

> A benchmark may be fast and still be invalid. The authentic product
> operation, a pure timing boundary, complete family evidence, attributable
> resources, honest statistics, and exact source custody are admission gates
> equal to the numerical target.

## 1. Specify the claim before implementing the benchmark

Every new or changed family MUST have a committed roadmap specification and a
GitHub issue before benchmark implementation or sample collection begins.
The specification MUST freeze:

- the question and exact claim the family is allowed to support;
- family and scenario IDs, versions, membership, and expected cardinalities;
- the approved public operation, entrypoint, projection, and acknowledgement;
- fixtures, exact byte sizes, seeds, schedules, cache state, and source arms;
- timing start/end events and every included or excluded action;
- raw metrics, units, formulas, required receipts, and applicability rules;
- baseline/candidate treatment and every permitted difference;
- absolute, comparative, correctness, resource, cleanup, and custody gates;
- verifier coverage and the independent oracle; and
- performance, verification, and result folder layouts.

Correctness and resource gates MUST be frozen before any measurement. A
performance target that requires an untouched baseline MAY be frozen after
that baseline, but it MUST be committed before candidate optimization or
candidate sampling.

A contract changed after observing results MUST receive a new scenario ID or
append-only schema version. Existing raw evidence MUST NOT be edited, relabeled,
pooled with the replacement, or made to disappear. Product code MUST NOT branch
on benchmark scenario IDs.

Missing or late specification makes every affected row exploratory with
`admission_eligible=false`.

## 2. Measure the authentic operation

Every scenario MUST declare:

```text
operation_contract_id
operation_surface
operation_entrypoint
orchestration_executor
mutation_executor
implementation_route
projection
acknowledgement_boundary
```

Product performance claims MUST exercise the public operation named by the
contract. A semantic operation MUST NOT be replaced by a shell reconstruction,
temporary-file algorithm, direct Store mutation, test hook, internal helper,
or environment-specific shortcut.

### LayerFS SDK file-edit invariant

All active LayerFS **SDK file-edit claims**, in performance and verification,
MUST perform their measured edit through one of:

```text
Client::edit_workspace_file_range
Client::edit_workspace_file_ranges
```

The shell runner may orchestrate the call, but it MUST NOT mutate the file.
The active edit families MUST NOT use container Exec, direct POSIX writes,
temporary-copy/rename, `copy_file_range`, reflink/clone, or a FUSE write as a
substitute. A real FUSE projection MAY be attached and read after the SDK call
to prove visibility, but edit-caused FUSE writes MUST be zero.

Historical results from a different operation surface remain archival evidence
only. They MUST NOT be registered in an active SDK edit family, paired with an
SDK result, admitted as SDK evidence, or used in an SDK performance headline.

Separately registered filesystem, tool, and Workspace workflow families MUST
exercise their declared ordinary POSIX/FUSE operations through public execution.
An editor save may genuinely write a temporary file and rename it; an import
must genuinely initialize from source files. These are different claims from
SDK range-edit latency. Their operation, timing, counters and schemas MUST name
the actual surface. Do not rewrite a tool's mutations as SDK edits, compare
unlike routes as paired arms, or weaken inherited SDK-only checks. A new
filesystem family does not revive archival temp-copy rows as SDK evidence.

Each row MUST report the observed public-call count and every forbidden or
fallback route count. An undeclared route, a nonzero forbidden/fallback count,
or an unprovable route is an authenticity failure, not a performance
regression. Reject the row and block admission.

## 3. Shell orchestrates; the product mutates

Shell runners SHOULD do only the following:

- validate arguments and family membership;
- prepare declared fixtures outside performance timing;
- launch and supervise the benchmark process and container;
- enforce timeouts and deterministic source-arm order;
- sample declared phase-local resources; and
- preserve raw output, receipts, custody, summaries, and manifests.

Shell, Python, and other utilities MAY construct fixtures and independent
oracles outside performance timing when the specification names and records
that work. They MUST NOT implement the measured mutation unless that exact
shell or POSIX interface is the preregistered product operation.

A shell wrapper or process launch MUST NOT be timed as operation latency unless
launch and parsing are part of the documented public contract. Bash access to
an SDK-only benchmark means Bash launches a compiled harness that calls the SDK
directly; it does not mean Bash recreates the operation with filesystem tools.

## 4. Do not add unrelated round trips

The measured path MUST contain only calls required by the declared operation
and acknowledgement boundary. A benchmark MUST NOT add a process launch,
shell invocation, container Exec, FUSE transfer, RPC, status poll, output read,
Store reconnect, digest, or metadata query per operation unless the contract
explicitly includes it.

If the claim is about a batch API, invoke that batch API. Do not emulate a
batch with repeated shell commands or repeated single-call processes. If the
claim is about one public call, attempted operations, completed operations,
public calls, transactions, and acknowledgements MUST match the declared
topology.

Every row MUST report expected and observed counts for relevant boundaries,
including:

```text
public_api_call_count
sdk_edit_member_count
exec_process_count
shell_process_count
daemon_request_count
fuse_read_request_count
fuse_write_request_count
store_transaction_count
visibility_query_count
output_poll_count
```

An unavoidable extra round trip MUST be included and named honestly or moved
to separately reported setup. It MUST NOT differ silently between baseline and
candidate. Unexpected or asymmetric orchestration invalidates the comparison.

## 5. Keep timing boundaries pure

Every timed metric MUST declare a stable `timing_boundary_id`, monotonic clock,
start event, end event, and inclusion/exclusion list.

Operation-local timing MUST begin immediately before the approved public call
and end immediately after its promised acknowledgement. The following work
MUST remain outside an operation-local timer unless the metric explicitly
names it:

- fixture, Store, Layer, Branch, Workspace, and container preparation;
- warm-up and preconditioning;
- argument and replacement-byte construction;
- process launch when launch is not the public operation;
- output formatting, transport, parsing, and receipt collection;
- monitoring snapshots and derived-counter calculation;
- digesting, payload/ObjectId enumeration, and oracle construction;
- reconnect, reopen, materialization, failure injection, and cleanup; and
- report generation.

Cheap validation MAY run immediately after timing and MUST still invalidate an
incorrect result. Performance mode MUST NOT execute added benchmark verification
work. A performance receipt MUST prove that benchmark verifier/digest/oracle,
reopen, materialization, and failure-injection counts are zero. Hashing intrinsic
to the declared operation, such as CAS identity/authentication or Git object
construction, remains measured product/tool work. Do not disable that hashing
or misclassify it as forbidden benchmark verification.

Complete-lifecycle metrics MUST include all named lifecycle phases. Ordered
timestamps and the sum of attributed/unattributed phases MUST balance against
the enclosing wall within a frozen clock-overhead allowance. External
supervisor wall time MUST be labeled external; it MUST NOT be substituted for
an inner operation metric.

## 6. Separate setup, performance, verification, and cleanup

Setup, performance, verification, cleanup, and external supervision MUST use
separate timing and resource scopes.

Fixture construction and LayerStack initialization are outside edit timing.
For file-size scaling, immutable base fixtures SHOULD be prepared once per tier
outside the measured child so preparation does not pollute operation latency or
process high-water memory.

Cache state MUST be declared and enforced equally. Cold and warm rows MUST NOT
be pooled. Preconditioning MUST be untimed, deterministic, and recorded.

### Reusable immutable input preparation

Benchmarks MAY persist initialized pristine input Stores across development
invocations and compatible source arms. This is preparation reuse, never an
optimization of the measured operation. They MUST NOT cache post-edit state,
Commit results, live Workspaces, reader caches, measured-process state, or
performance/verifier receipts. Expected-result data is verification-only and
MUST NOT be supplied to mutation as a shortcut or used to prime selected ranges.

A prepared-input cache MUST use a content/metadata/format compatibility key,
not merely a schema version or the whole candidate revision. Until explicit
compatibility is dependable, a conservative digest of preparation-related
source and configuration MUST invalidate relevant changes. Exact producing
revision and binary provenance MUST remain recorded. Unknown compatibility
fails closed; paired arms MUST use the identical qualified Store artifact.

Entries MUST be complete and quiescent before same-filesystem atomic
publication, with per-key coordination. Half-built entries, unexpected
sidecars, and corrupt manifests MUST NOT be consumed. Every mutation sample
MUST receive a fresh independent writable copy or real copy-on-write clone;
hard links to the immutable master are forbidden. Qualification that can open
a Store writable MUST also use a disposable clone. Normal sample cleanup MUST
NOT delete the shared prepared entry.

Validate the master once per acquisition/run and retain its sealed digest;
repeated source rehashing per sample is unnecessary. Admission clone identity
checks remain mandatory. Cache acquisition, validation, preparation, cloning,
qualification, and container setup time MUST be reported outside operation
timers. A lighter development-only policy requires an explicit non-admission
profile; it cannot fabricate an identity pass. APFS clones and hashed inputs
MUST NOT be called cold: ordinary OS-cache effects and actual clone/copy method
MUST be declared and treated consistently, and changed profiles MUST NOT be
pooled. Tests MUST cover invalidation, corruption, concurrent/interrupted
publication, cross-family reuse, and sample-to-master isolation.

Verification MUST use a separate mode, stream, schema, summary, and status. A
full digest, payload enumeration, root oracle, reconnect, reopen, or
materialization proof MUST NOT enter a performance distribution. Verification
work that is intentionally proportional to file size has its own resource
scope and MUST NOT be attributed to mutation.

A verifier failure preserves otherwise valid performance samples but makes
overall admission fail.

### Avoid repeated preparation

Resolve the requested case, tier, seed and source arm before preparation. A
selected run MUST NOT prepare all families or all four tiers. Reuse compatible
compiled binaries, workload helpers, runtime images, fixture bytes and pristine
Store entries; rebuild or regenerate only artifacts affected by the change.
Prepare larger fixtures lazily when the selected workload actually requires them.

Static oracle/manifest preparation MAY be cached by complete input and oracle
identity. Actual-output verification MUST still execute for each required
verification case. Reference shared controls and sealed workload rows from
multiple reports instead of collecting duplicate evidence. A report-only
change consumes existing raw evidence and MUST NOT trigger another product run.

Record first-use and cache-hit command wall, build/image reuse, cache hit/miss,
generation, qualification, clone validation and preparation resource peaks.
Optimizing preparation MUST NOT skip the measured initialization, file creation,
CDC scan or history construction. No post-operation Store or live Workspace may
be supplied from a cache. Keep preparation/verification concurrency bounded and
out of the measured product's resource window.


## 7. Define coherent and complete families

A family MUST have one coherent semantic axis and all preregistered siblings.
A release MUST NOT accept favorable members while moving unfavorable siblings
to a later release.

Each family MUST own exactly one canonical definition module and one thin
runner. Shared product lifecycle, process supervision, evidence, and reporting
code MUST be reused rather than copied into families.

The definition MUST freeze ordered IDs, fixtures, seeds, arms, roles, schedules,
byte equations, expected counters, and row/receipt cardinalities. A family name
MUST state the measured semantic axis; ambiguous words such as `same_count` or
`count_changing` are insufficient unless the counted quantity is explicit.

When a claim compares operations across file sizes, every comparable operation
MUST use the same exact tiers, fixture generator, seeds, local edit span,
position contract, public-call topology, and timing boundary. A partial scaling
cohort MUST NOT support a universal file-size claim.

Selected development mode MUST run one case, seed, and source arm and emit
`admission_eligible=false`. Full admission MUST require an explicit `--all` or
equivalent deliberate option. Missing, duplicate, cross-family, or unknown rows
fail cardinality checks.

A shared-path optimization requires rerunning every affected family member and
verifier. Run the smallest failing selected case during development; do not run
the full family repeatedly while diagnosing one failure.

## 8. Compare semantically identical baseline and candidate paths

A paired comparison MUST match on:

```text
family, scenario, and schema
operation surface, entrypoint, and call topology
fixture bytes and digest
seed and schedule
projection and environment
cache profile
round-trip counts
timing and acknowledgement boundary
verification contract
metric formulas and units
harness and workload behavior
```

For a product performance claim, the harness, workload, fixture, oracle,
report generator, schema, and runner artifacts MUST be byte-identical across
arms. The declared execution profile and controlled environment settings MUST
be contract-identical; run-specific container/process IDs, timestamps, and
observed ambient values remain distinct custody data. The sole intended
difference MUST be the exact product/source change under evaluation and MUST
be recorded as `treatment`.
Exact source, product, harness, workload, fixture, oracle, binary, image, and
report-generator seals MUST make that difference auditable. A harness or
workload experiment is an unpaired diagnostic and cannot support a LayerFS
product speedup claim.

An older baseline that lacks the new public operation cannot be paired as if
it had identical semantics. Different product surfaces or algorithms require
separate non-comparative rows. A POSIX rewrite and an SDK range edit MUST NOT be
combined into a speedup ratio.

Source-arm order MUST follow the preregistered alternating schedule. If either
pair member is invalid, rerun the complete pair. Do not rerun only the slower
arm, discard a valid outlier, or select the best attempt. A/A evidence is
repeatability evidence, not candidate improvement.

## 9. Report true metrics, units, and formulas

Raw durations MUST end in `_ns`, raw storage quantities in `_bytes`, counts in
`_count` or a documented plural, and rates in `_per_second`. Raw byte values
MUST be integers. Human reports MUST distinguish decimal MB from binary MiB.

Names MUST describe the measured interval. `edit_api_ns` contains only the SDK
edit call. `edit_through_visible_ns` ends at visible Commit. A process wall,
complete lifecycle, batch average, or external wall MUST say so.

Every derived value MUST retain its numerator, denominator, formula, unit, and
source fields. Throughput MUST name its byte basis, for example:

```text
supplied_bytes_per_second
copied_payload_bytes_per_second
scanned_bytes_per_second
durable_bytes_per_second
```

Do not call copied, scanned, logical, supplied, and durable bytes the same
thing. Do not headline MiB/s for a tiny edit when fixed latency is the actual
product question. Do not say every operation completed below a threshold when
only a batch-average interval was measured.

An unavailable or inapplicable value MUST be `null` with a status and reason,
never a fabricated zero. Summaries MUST identify whether they use a median,
range, percentile, ratio of medians, median of paired ratios, or another frozen
formula.

Published tables MUST show raw elapsed-time units, sample count, median, and
min-max range. Claims MUST remain within the measured file sizes, environment,
operation, and acknowledgement boundary. Complexity analysis is not measured
evidence, and measured rows MUST NOT be extrapolated to an unmeasured size.

A sparse, compressed, deduplicated, repeated-subtree, or otherwise synthetic
logical-size fixture is structural/complexity proof only. It MUST NOT extend
the empirical measured-size set or support latency, throughput, RSS, cgroup, or
"works at N bytes" headlines for its logical size.

## 10. Measure and attribute memory honestly

The exact-phase rules below apply when a report claims phase-local precision.
An explicitly approved, documented broader-window profile may instead use
causal start/finish acknowledgements, sampled category observations and native
lifetime bounds. Report gaps and unavailable exact attribution; never rename
these observations as exact phase peaks or continuous category maxima. The
v0.1.2 SDK campaign's approved `ack-window-v1` profile is one such exception.
Memory observation precision is not itself a product latency target.

A report MUST NOT collapse all resource domains into one field called
`memory`. It MUST distinguish:

```text
benchmark host-process RSS or physical footprint
measured child-process RSS
daemon/container anonymous memory
cgroup file cache, dirty/writeback, shmem, kernel/slab, and total
Workspace spool disk
Store durable disk
fixture and oracle disk
verification-only memory
```

Current RSS, process-lifetime high-water, phase-local peak, and incremental
phase peak are different measurements and MUST have different fields. Lifetime
`ru_maxrss` MUST NOT be labeled incremental RSS. Cgroup `memory.peak` MUST be
labeled lifetime total unless it was verifiably reset after setup.

A mutation-memory claim MUST use a post-setup baseline and a phase-local peak.
A cgroup phase peak MUST come from a fresh or resettable cgroup with verified
reset/readback, or it is unavailable for the gate. Sample and report relevant
`memory.stat` domains so anonymous memory, file cache, dirty/writeback pages,
shmem, kernel/slab, and sockets are not confused.

The family contract MUST freeze the native sampling method and maximum sample
interval. A phase-local receipt MUST include:

```text
sampling_interval_ns
sample_count
first_sample_ns
last_sample_ns
maximum_sample_gap_ns
rss_baseline_bytes
rss_phase_peak_bytes
rss_incremental_peak_bytes
rss_final_bytes
```

Sampling MUST cover both timed boundaries and contain at least one interior
observation. A missed boundary, an excessive gap, or otherwise incomplete
coverage makes the phase peak unavailable and the row admission-ineligible.
Different clock domains MUST NOT be compared as though their epochs were
identical. Calibrate offset and uncertainty outside operation timing, retain
all calibration operands and any bounded clock-rate allowance, and propagate
uncertainty into boundary selection and guaranteed-interior checks. A nominally
nearby sample on the wrong possible side of a boundary is not coverage. Sampled
maxima over an uncertainty-expanded interval MUST be labeled conservative;
uncertainty MUST NOT hide a possible phase spike or relax the boundary/gap gate.
Resettable cgroup `memory.peak` remains an exact total peak; polled cgroup
domains require the same coverage disclosure.

Physical spool is disk occupancy, not RSS. Fixture preparation and verifier
memory MUST NOT be charged to mutation. Conversely, bounded process heap does
not excuse file-size-proportional spool or cgroup page-cache growth.

Every file-size-insensitivity family MUST combine:

1. phase-local process and cgroup measurements at every declared size;
2. cross-size absolute and slope/spread gates;
3. direct counters proving no untouched-payload read, copy, scan, spool, or
   fallback proportional to the base file; and
4. fixed piece-tree, CDC, deferred-object, candidate, swap, and OOM gates.

An unavailable mandatory counter, unexplained or unattributed memory spike,
swap, OOM, or growth correlated with base-file size is a hard no-go. A latency
pass cannot override a memory failure.

## 11. Freeze statistics and gates before candidate measurement

The specification MUST define sample count, preconditioning, ordering,
aggregate, tolerance, outlier policy, invalid-run policy, and timeout before
candidate collection.

Retain every valid preregistered sample. Do not remove a slow valid sample or
repeat only a favorable arm. Invalid infrastructure samples remain retained
with an explicit failure class; rerun the complete affected pair or cell.

Small local phases MAY use a preregistered absolute noise allowance when a
percentage ratio would be misleading. That allowance does not relax absolute
latency, correctness, resource, cleanup, or custody ceilings.

Timeouts MUST be close enough to catch the wrong algorithm quickly. A timeout
must not be inflated merely to let a file-size-proportional implementation
finish. Performance and verifier timeouts MUST be separate and reported.

Targets and formulas MUST NOT be loosened after seeing candidate results
without a documented contract revision and new scenario/schema identity. A
valid no-go is useful evidence; it is not permission to change the question.

## 12. Require no-amplification and no-fallback receipts

An optimized semantic path MUST prove its mechanism, not merely finish quickly.
Applicable rows MUST report:

```text
old_payload_bytes_read
replacement_bytes_scanned
commit_payload_bytes_read
commit_cdc_bytes_scanned
fuse_read_bytes
fuse_write_bytes
spool_write_bytes
physical_spool_high_water_bytes
piece_count
piece_height
piece_allocation_bytes
candidate_object_count
candidate_bytes
inserted_object_count
reused_object_count
public_api_call_count
forbidden_route_count
fallback_count
```

The family specification defines exact or bounded expectations. Any forbidden
route or fallback is a hard-invalid sample even when time and memory happen to
pass. Missing required counters MUST NOT be converted to zero. If a numeric
route counter does not exist, use a sealed call-graph/manifest status plus
observable runtime tripwires; never fabricate zero.

### Mandatory core receipts

Every performance or verifier row MUST include, directly or through a sealed
campaign identity referenced by the row:

```text
schema
family_id
scenario_id
scenario_version
contract_commit
mode
admission_eligible
source_arm
treatment
product_identity
harness_identity
workload_identity
fixture_identity
oracle_identity
report_generator_identity
seed_or_repetition
attempted_operation_count
completed_operation_count
sdk_edit_member_count
public_api_call_count
operation_contract_id
operation_surface
operation_entrypoint
orchestration_executor
mutation_executor
implementation_route_status
timing_boundary_id
clock_id
start_event
end_event
all applicable duration, mechanism, and memory fields
field availability and provenance statuses
performance_status
verification_status
resource_status
cleanup_status
custody_status
row_status
failure_class
```

Every campaign MUST additionally report expected and observed family IDs,
scenario IDs, repetitions/seeds, source arms, performance rows, control rows,
and verifier receipts, plus:

```text
pairing_and_order_status
semantic_identity_status
timing_purity_status
receipt_completeness_status
claim_eligibility_status
overall_status
admission_eligible
evidence_manifest_sha256
```

Missing mandatory fields make the row or campaign invalid. They cannot be
interpreted as zero, unavailable, or not applicable without an explicit
schema-authorized status.

## 13. Preserve raw evidence and exact custody

Every run, including failures and no-go results, MUST use a unique
non-overwriting evidence directory containing:

- sanitized exact commands and arguments;
- stdout, stderr, exit status, timeout status, and failure context;
- every raw sample and product receipt;
- source commit, tree, index/worktree state, and dirty status;
- product, harness, workload, fixture, oracle, binary, and image digests;
- host, OS, CPU, runtime, container, projection, and cache identity;
- expected and observed cardinalities and execution order;
- derived summaries and the sealed report-generator identity; and
- a manifest hashing every retained file.

Raw evidence MUST NOT be edited or regenerated in place. Derived summaries
MUST be reproducible solely from retained raw evidence and the sealed report
generator. Secrets MAY be redacted before persistence without removing the
public command shape.

Release evidence MUST bind the exact candidate source, product, harness,
workload, fixture, environment, and report generator. Any relevant change
makes prior admission evidence stale and requires the affected exact-candidate
campaign to be rerun.

## 14. Bind every published claim to evidence

Every performance sentence, headline, and table MUST map to:

```text
claim_id
claim_kind
exact claim wording and scope
measured_fixture_sizes_bytes
family and scenario IDs
source arm or paired arms
metric, unit, and timing boundary
throughput byte basis when applicable
aggregate formula
gate and status
evidence directory and manifest
candidate source and product seal
```

`claim_kind` is one of:

```text
empirical-performance
structural-complexity
```

Only `empirical-performance` may carry non-empty
`measured_fixture_sizes_bytes` or support latency, throughput, RSS, cgroup, or
"works at N bytes" claims. `structural-complexity` may describe proved
algorithmic bounds but cannot present a synthetic logical size as measured
performance.

A report generator MUST fail if a claim lacks eligible evidence. A gate pass,
tolerated result, A/A repeatability result, diagnostic, or unpaired row MUST
NOT become an improvement headline. Incompatible byte bases or operation
surfaces MUST NOT be pooled.

Reports MUST link to raw evidence instead of copying unattested numbers through
multiple documents. Unsupported, inflated, ambiguous, or extrapolated claims
block publication even when machine gates otherwise pass.

## 15. Fast development loop and terminal admission

The normal development loop is:

```text
self-check
→ one selected case, seed, and arm
→ inspect phase/counter/resource evidence
→ fix the smallest shared root cause
→ rerun the selected case
→ expand to the matching size or sibling operation
```

Do not run the complete performance family or verifier suite on every edit.
Run full admission once the selected cases, size slope, operation parity, and
resource counters are green. A small optimization on a shared path still
requires one final run of every affected family member.

The selected loop SHOULD pair its performance case with the smallest relevant
correctness regression, not a full-tree verifier on every edit. Measure actual
command wall, including per-run cached acquisition, validation, cloning,
required runtime readiness, receipts and cleanup; report first-use build/setup
separately. A millisecond product operation MUST NOT be advertised as a
millisecond complete test.

Family specifications MUST distinguish ordinary selected runs, full performance
collection, independent verification and expensive/extended qualification.
Ordinary warm-prepared development should aim for a few seconds. Large-history,
resource-boundary, and sustained-endurance cases may take longer and MUST be
explicitly selectable. No default invocation may launch the whole release or
an endurance run. A complete release MUST still run every required extended
member; an ordinary-lane pass is not complete admission.

Freeze independent preparation, selected-case, performance and verification
budgets with their applicability and source. Numerical limits that need an
untouched baseline must be fixed before candidate optimization or collection.
Do not shorten a workload, hide preparation, raise a timeout after a valid miss,
or omit a correctness check merely to label a family fast.

Self-checks MUST be fast and product-free. They MUST validate unique IDs,
family cardinality, fixture/schedule/seed algebra, operation surface, timing
boundary, metric applicability, claim mapping, every mandatory core receipt
field, forbidden-route defaults, performance/verification separation,
selected-mode ineligibility, and the explicit complete-family option.

Runtime admission MUST independently validate observed call counts, route
counters, timing balance, family cardinality, memory scopes, cleanup, custody,
and claim eligibility. Static inspection alone does not prove that the measured
operation was authentic.

## 16. Failure classification

| Condition | Classification | Admission consequence |
| --- | --- | --- |
| Selected development or exploratory run | Valid diagnostic | `admission_eligible=false` |
| Frozen numerical target miss with valid evidence | Valid no-go | Retain evidence; admission fails |
| Frozen tolerance-band result | Tolerated only under the preregistered rule | No unsupported improvement claim |
| Correctness, verifier, memory, resource, or cleanup failure | Valid failure | Admission fails |
| Wrong operation, hidden fallback, extra round trip, contaminated timer, semantic mismatch, missing mandatory metric, incomplete family, or custody mismatch | Invalid evidence | No release claim; correct and rerun |
| Timeout, swap, OOM, or abnormal process/container exit | Hard failure | Retain artifacts; admission fails |
| Product, harness, workload, fixture, or report generator changed after evidence | Stale custody | Prior run becomes diagnostic; rerun exact candidate |
| Unsupported or misleading headline | Documentation failure | Publication blocked |

## Review checklist

Before accepting a benchmark or report, answer all of these with retained
evidence:

1. Did the benchmark call the exact approved public operation?
2. Did the shell only orchestrate, and are all forbidden mutation paths zero?
3. Does the timer contain only the work named by the metric?
4. Are baseline and candidate semantically and operationally identical except
   for the declared treatment?
5. Is every family member present with the required seeds and arms?
6. Are raw elapsed times, sample counts, medians, ranges, formulas, and units
   visible and reproducible?
7. Are process RSS, cgroup domains, file cache, spool disk, Store disk, and
   verifier resources separated?
8. Do size-scaling rows and mechanism counters reject hidden work proportional
   to untouched data?
9. Are correctness, resource, cleanup, custody, and claim eligibility separate
   hard statuses?
10. Does every published sentence stay within the exact measured scope?

If any required answer is no or unknown, the benchmark is not release
evidence.
