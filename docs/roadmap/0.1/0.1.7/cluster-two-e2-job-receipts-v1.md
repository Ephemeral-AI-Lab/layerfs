# Cluster-two E2 original-job receipt collector v1

> Status: prospective owning specification. Commit before implementing this
> benchmark extension. No driver, sample, observer calibration or E2 exit is
> established by this document. Issue: [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).

This extends [E1 registration](cluster-two-e1-registration-v1.md) with retained
original-operation observations. It follows [benchmark rules](../../../general/benchmark_rules.md),
[measurement workflow](../../../general/agent-measurement-policy.md) and the
[S7–S12 criteria](../../../../core/docs/issues/307/PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md).
Its consistency predicates establish accounting coverage only. They supply no
new deployment latency/RSS/rate/copy limit or qualified performance verdict.

## Membership and authentic routes

Family extension/schema: `cluster-two-job-receipts-v1`. Initial collector member:
`E01-startup`, exactly one public `Owner::start_observed` on one fresh owned local
database, actual configured-readiness outcome and explicit owner stop. Later members:
`E04-write-16m`, `E05-write-1g`, each exactly1000 writes through production
`Workspace::mutate`, existing OwnerClient and authenticated immutable ports. They
remain prospective/unimplemented until their complete fixture/route prerequisites
exist. No benchmark mutation implementation or scenario-selected product algorithm.

Compiled vehicle belongs in `core/crates/layerfs-daemon/examples/`; external
observation helpers implement existing Workspace ports and submit identical typed
Commands through the actual OwnerClient. Retained validator belongs in the owning
`core/benchmark/fs-bench-pro/shared/` family integration. No generic runner or
aggregate preflight. Existing package/provider/profile/architecture boundaries hold.

E01 input is one fresh absolute owned database path and fresh output directory,
default OwnerConfig and owning Overlay ProfileConfig, with exact readback recorded.
Failed startup retains its original report/error and artifact; never reopen, retry
or remove it to obtain success. `creation_reported=false` is unavailable creation
work, not zero. Optional post-ready Resources and diagnostics probes are distinct
original commands, outside the startup construction/readiness phase and fully
included in complete-command accounting. Their cost cannot be charged as startup
or silently removed from complete runtime.

E04/E05 use the exact frozen1000-write offsets and input hash in E1's
`evidence_workload.py`,4096000 input bytes/1000 aligned intersecting cells. Real
16MiB/1GiB dense saved fixtures and independent final-state oracles are prerequisites.
Use closed prepared inputs/`--setup clone` where supported; acquisition/output
initialization remain timed where the owning route requires them. Cache enforcement
must precede every measured phase; a clone is not a cold claim. Do not replace
unavailable native/global observations with local fixture counters.

## Original receipt ownership

Allocate one external receipt identity before each actual command admission,
linked to its public operation attempt. This identity is not the private scheduler
job ID. Record command kind, original route/source/publication identity and original
admission, pending and attempted-result disposition. One original command yields
one result/refusal; no replay, guessed completion or replacement failure.

The observing ports retain work before returning the ordinary value, covering
every Needs round, base fact/read, final publication, ReplyAttempted and source/
processing release. They preserve the existing unattempted command+cause or original
boxed Completion on failure. Successful adapter clones are actual copy work;
they cannot be credited as zero because the underlying SQL returned bytes once.
Writing an observational record does not replace original in-memory custody or
establish a crash-recovery resolver.

Emit append-only records through bounded synchronous windows/backpressure. Do not
retain all jobs, inputs, completed values or resource samples in a process-sized
Vec. Each successfully written record may release its observation buffer; actual
product result custody follows its real caller/consumer lifetime. Recorder failure
ends the driver once and retains the original operation and recorder outcome,
including any acknowledged prefix. No restart/truncation/retry of the same output.

## Fields and deterministic consistency predicates

Each record includes exact source/product tree, toolchain/config/dependencies,
binary/image, driver/schema, fixture/trace/cache and observer identities. A final
manifest enumerates actual records, byte counts, hashes and all failed/unrun phases.
No dirty working tree is a sealed arm. Startup and every original Completion retain:

- Actual StatementWork attempts/executions, VM/fullscan/sort/autoindex/reprepare,
  returned rows, direct/trigger changes and bound/SQL/returned BLOB/value bytes,
  with original statement-family totals.
- Actual PayloadWork copy/zero/input/cell observations and AllocationWork range
  attempts/requested volume, admissions/refusals, metadata observations and
  committed-freelist queries.
- Parked turns, queue wait and final service span, each kept in its source scope.
  Queue wait includes parking; service overlaps SQL. Do not add overlapping spans
  as exclusive CPU/device/runnable-wait decomposition.
- Separate foreground and automatic-maintenance aggregates; admitted/completed
  jobs, per-class sums and retained credit current/peak at declared endpoints.
  Credit includes queued/executing/caller-held results and is not queue-only peak.
- Actual logical row/byte counts, shared pages/freelist/allocated/high-water/tail/
  cleanup-capacity/debt-upper observations. High-water remains lifetime-scoped;
  debt upper remains a bound, not exact eligible debt or phase residency.

Fresh output layout: `<campaign>/<case>/<profile>/<arm>/sample-0001/` for the one
eligible sample, `diagnostic/<receipt-id>/` for uncontrolled functional records,
and `verification/<proof-id>/` for the separately selected independent oracle.
Each contains `manifest.json`, `startup.jsonl`, `jobs.jsonl`, `probes.jsonl`,
`stdout.txt`, `stderr.txt` and `outcomes.json`; an explicitly unrun stream is named
in the manifest rather than silently omitted. Database/fixture artifacts have
separate owned paths, with exact identity and allocation inventory. Each execution
requires an unused output path; previous outcomes are never overwritten.

Validator predicates: exact membership/cardinality; unique original attempt/receipt
IDs; no missing Needs/publication/reply/release records; consistent source/arm/
route/window identities; monotonic complete counters; retained per-command sums
equal same-owner foreground deltas with diagnostic jobs included and maintenance
separate; final declared result/source ownership endpoints and retained credit.
E01 verifies actual268435456-byte reservation and134217728-byte cleanup capacity
where creation readback is available. E04/E05 additionally verify the exact frozen
write trace, bytes, publication/reply identities and independent complete final state.
Unavailable fields stay explicit null/UNAVAILABLE and refuse any predicate that
requires them. Failed/refused original operations remain observations, not PASS.

These predicates do not imply whole-operation copies, directly observed indexed
visited rows, statement-fingerprint/bind correlation, exclusive Workspace physical
allocation, queue-only occupancy, peak eligible debt, isolated resource wait or
host/Linux/kernel/socket/page-cache residency. Those require owning instrumentation,
exact paired EXPLAIN/execution and separately calibrated E3 observations. Do not
rename aggregate/source-limited quantities to fill missing fields.

## Modes, budgets and qualification boundary

Diagnostic/functional mode has uncontrolled cache and `admission_eligible=false`.
It reports `observation_consistency` only, with `qualification_status=NOT_EVALUATED`.
Compiled-driver smoke and negative-validator tests have explicit <=120s wall stops,
build first; timeout is FAILED. This does not transfer test budgets to measurement.

Eligible measurement remains one original sample per selected case/arm, complete
command<=15s (only prospectively declared exceptions<=25s), independent proof9s,
constructors1. E01 startup has no native Init construction; supported Namespace
Init profile remains separately scoped. Full outputs include startup, product
operation, observer/drain/stop and eventual cleanup outcomes; no phase warmth or
earlier/lifetime peak substitutes a measured phase. Record observer precision and
overhead before eligible collection. No unchanged treatment resampling or best-of.

Until actual driver/oracle identities, numerical authorities, Linux residency and
physical-I/O inventories, cache/topology seals and calibration are registered,
E01/E04/E05 remain NOT_RUN; all27 E1 proposals and prior verdicts remain unchanged.
Stale route/cache source pins need explicit later advancement with new checked
identity; this specification does not refresh them. Q05's full-million-entry budget
and other missing schedules/limits remain implementation registration work. This
specification changes no Init/history gate, chooses no longer-family allowance,
waives no failure and completes no S7/S9/S12 acceptance.

## Specification commit accounting

Production LOC: 162906 -> 162906 (delta +0).
Core97489 ->97489 (+0); reference65417 ->65417 (+0).
This documentation-only commit excludes the concurrently authored product changes.
[Exact comparison](../../../../core/docs/issues/307/checks/e2-spec-20261007/production-loc-source.json)
uses first parent4090cb9a2d5fa1cd899d63c7a44537ff3f23e3cd and unchanged staged
product roots, with `git archive TREE crates core/crates` and unchanged
`tools/production_loc.py --root ARCHIVE --json`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
The exact immutable before count is reused; final staged product is recounted.
Product Rust/runtimeSQL/excluded predecessors count; tests/inline tests/examples/
docs/tools/harness/third-party do not. No relocation/duplication/retirement.
