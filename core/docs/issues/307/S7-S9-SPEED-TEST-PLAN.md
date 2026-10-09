# S7/S9 speed, round-trip and resource test plan

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Requested by the owner in a side conversation on 2026-10-06.
> Draft v1. All cases are NOT_RUN; this document creates no measurement receipt.
> Scope: S7 engine qualification and S9 runtime/acquisition qualification before
> S8. It does not implement a runner, select new product algorithms, change
> persistence guarantees, advance a milestone or authorize later milestones.

## 1. Questions this plan must answer

1. What does a complete local operation cost in time, SQL, pages, bytes, copies,
   queueing, allocation and reclamation?
2. How much latency and traffic does the authenticated host boundary add?
3. Do real consumers use batching, immutable caches and serial ranges, or turn
   ordinary activity into many dependent host calls?
4. Can unrelated Workspaces progress under Save pressure and slow delivery?
5. Do memory, physical allocation and cleanup debt remain within derived bounds
   during finite sustained workloads and after last-owner release?
6. Does acquisition preserve the entire root without input-sized resident state?

The [S7 audit](S7-EXIT-AUDIT.md), [S9 audit](S9-EXIT-AUDIT.md) and
[current continuation](HANDOFF-S7-S9.md) own implementation and milestone status.
Current functional/count receipts do not qualify cold speed, phase RSS or
sustained rates. A passing row here qualifies only its declared operation.

## 2. Authentic global/local topology

```text
macOS host                                      Linux Docker
initialized SDK runtime                         public-API workload driver
  Storage / Reader / independent Saves  <------> native runtime client
  HistoryCatalog / authority               Bridge        |
  selected global SQLite profile                        Workspace
                                                         |
                                                daemon SQL owner
                                                         |
                                                one local Overlay DB
```

The global Store owns canonical objects, reference closure, construction policy,
serial allocation and mutable history. The local Overlay owns uncommitted
Workspace metadata/payload, generations, scratch, custody and reclamation.
Use the supported macOS global provider; Linux operates the local engine and
reaches global services through actual authenticated adapters. See the primary
[architecture](../303/01-architecture.md), [engine](../303/daemon-sqlite.md) and
[runtime integration](../303/06-cluster-one-integration.md) contracts.

Run the same selected cases separately for global Durable and Disposable profiles
when applicable. Record each profile's real synchronization and acknowledgement.
The local Overlay remains its selected disposable profile: no fsync/fdatasync,
sync_data/sync_all, crash-recovery claim or profile substitution. Count its entire
268435456-byte reservation: 128 MiB mutation plus 128 MiB cleanup capacity.
Reservation is physical backing, not a claim of 256 MiB RSS.

A driver under core/benchmark calls ordinary public production APIs. It cannot
reimplement mutations, include private source, use test-only product hooks or
revive excluded Server/SDK/Sandbox source as a candidate route. Native socket I/O
is required for runtime rows; an in-process provider is a separately labeled
component control, never a substitute for that row.

S8 mounted FUSE, mmap/coherence, kernel request ownership, ordinary Bash and native
teardown are deferred. S10 captured Workspace Commit construction and
P3/P6/P7/P13/P14 stay outside this plan. Direct Save/history adapter checks use
valid roots built through existing public libraries; they do not claim S10.

## 3. Registration and source requirements

This is a concrete prospective selection, not an executable benchmark family.
Before harness implementation or sampling, commit the owning specification and
link it to tracker #307 under the [benchmark rules](../../../../docs/general/benchmark_rules.md).
Resolve the exact public entrypoint, fixture manifest, timer/acknowledgement,
observer capability, independent verifier and all gates for every selected row.
Missing registration or a missing gate leaves the row INCOMPLETE or diagnostic.

Use the [core benchmark routing](../../../benchmark/fs-bench-pro/AGENTS.md).
Each new family has one canonical definition and one thin runner, reusing existing
supervision, setup and evidence support. Register new component/runtime cases
explicitly rather than silently changing a frozen Workspace or SDK family.
The retained SDK Project Init family continues through its sole init_namespace
runner and compatible source. The current direct Init families keep their own
contract. An unfinished SDK facade is not supplied by a harness-only replacement.

Inspection baseline: local main a3b2ee13eb07569eb1be6875c6f281d4b882d35c;
runtime source 1b2580f2e and native-alias source 551f165ed. Consumer-port changes
were uncommitted at document creation. These are context pins, not measured arms.
Seal the actual final candidate before a qualified comparison; dirty source is
not compared as a sealed product. Retain exact relevant production fingerprints
when reusing unaffected evidence.

Historical old-core comparison candidate: Phase 4.5 plus Phase B at
7edddbdb8e8512627aed0ed42533ef099d802384. This is not root crates/v0.1.6 and
does not reopen its campaigns. Qualify the selected compatible public route and
build separately. Different public operations, acknowledgements, profiles,
fixtures or orchestration counts produce separate non-comparative rows. In
particular, old application-to-daemon SandboxHello and new daemon-to-host grants
are different paths. Do not subtract one from the other or fabricate a speedup.

## 4. Proposed case catalogue

IDs and workloads below are draft v1 proposals. Registration freezes their exact
manifest, seed-1 generator, schedule, profile and verifier before execution.
Every row starts with status NOT_RUN and sample_count=0. Performance rows use
one sample per case/arm, with an independently recorded proof.

### S7 local engine cases

| Case ID | Workload and public route | Required observation |
| --- | --- | --- |
| E01-startup | One fresh Overlay/daemon owner through create_observed/start_observed | Creation through readiness; profile/schema/index/accounting SQL, original outcome, complete reservation and high-water allocation |
| E02-bind-1 | BaseView::open, daemon Command::Open and Workspace::bind against one initialized owner and complete root | Public component bind through usable result; root demand, logical state and allocations; zero new Overlay databases and zero whole-tree scans |
| E03-bind-64 | 64 such binds, all distinct namespaces in that same owner | All bind costs; one database/reservation; no namespace leakage; no hidden per-Workspace bootstrap |
| E04-write-16m | 1000 public 4096-byte writes to a prepared dense 16 MiB base file | Complete write acknowledgements, exact SQL/cell/copy/allocation work, no base-payload demand or global Save/history caused by writes |
| E05-write-1g | The same 1000-write trace against a dense 1 GiB base file | Same write topology; account population-dependent seeks; reject whole-file copy-up and file-prefix amplification |
| E06-read-mixed | 64 public 65536-byte reads over a fixed 1 GiB base plus 16 local 4096-byte overwrites and a declared truncate/regrow frontier | Local/base/zero composition, actual global demand, EOF and custody; every required acquisition remains inside the read operation |
| E07-reclaim | Release the final declared owners of 1000 locally written 4096-byte ranges across four Workspaces | Logical close separately from automatic physical cleanup; bounded turns, debt trace, local rows/owners gone when eligible, global roots retained |

E02/E03 include immutable validation and the actual OwnerClient open job. They
use production owner ports; a direct-Overlay Workspace::open control would be a
different row. These component binds do not claim an implemented SDK Mount or
kernel attach. E03 pays its first acquisition and subsequent declared reuse inside
the same 64-bind trace, not 64 independently cold requests.

For E04/E05, base size B is a multiple of 4096. Write i, for i in [0,999], starts
at 4096*((104729*i) mod (B/4096)). Replacement buffers are prepared caller inputs;
the manifest pins their generator/hash. Dense base bytes are independently
generated and their actual physical shape is recorded. The base-size cohort uses
the same mutation size, count, public topology and acknowledgement. A sparse or
deduplicated logical 1 GiB fixture is a distinct structural case, not E05 evidence.

For E06, registration fixes all read offsets, inherited cutoffs, overwrite ranges
and expected bytes before sampling. Report its 64-read workload total and each
request interval; do not present a batch average as a per-request upper bound.
E07 must use the owner's normal automatic service, not a harness maintenance pump.
Its background SQL/I/O and quiescence costs remain visible after logical close.

### S9 runtime and consumer cases

| Case ID | Workload and public route | Required observation |
| --- | --- | --- |
| R01-connect | One fresh real native handshake, authenticated attachment and original Binding retrieval | Cold connection/bootstrap boundary, actual grants/records, CPU/buffers and refusal scope; separate from steady-state calls |
| R02-objects-1 | One native object-demand request for one canonical whole-file object with 1024 payload bytes | Call through checked credited reply, canonical/authentication work, grant and final result |
| R03-objects-1000 | One native demand for 1000 distinct such objects, within ID/byte windows | One logical batch request; fragments/records/bytes separately counted; exact demand order and cardinality |
| R04-cache-trace | Ten CanonicalClient calls for the R03 set with an 8 MiB cache, starting empty; first fetch and all nine reuses inside one timed workload | Entire trace pays its initial acquisition; first call batches misses, later hits avoid host acquisition; cache/copy costs remain charged |
| R05-length-batch | One native Lengths operation for 1000 saved file roots | One logical batch; owning metadata I/O separately counted; no whole-file payload acquisition just to obtain length |
| R06-length-consumer | 1000 RemoteLengths::file_length calls for the same roots | Actual single-ID call count and grant latency; no claim that the batch endpoint automatically batches this consumer |
| R07-serial-consumer | 1000 sequential Workspace inode creations starting with no unused serials, using RemoteSerials | Exactly one 1024-serial reservation on this success trace; all other metadata/engine calls charged independently |
| R08-save-waves | Begin once, Accept 16 distinct valid whole-file objects with 65536 payload bytes each, then Finish once | Native grants/fragments, canonical admission/closure, physical encoding, final receipt and all retained credits; no Branch publication implied |
| R09-shared-call | Four caller lanes, each requesting 32 distinct small objects through one shared Calls owner | Actual connection serialization, queue wait and complete-call latency; no fabricated pipelining claim |
| R10-separate-calls | The identical 128 logical requests through four connections and four bound Workspaces | Fair ready service and connection costs; a topology-control diagnostic, not a product speedup ratio against R09 |
| R11-history | Two independently saved valid roots staged against the same original Branch expectations; conditional transitions attempted once in fixed order | Known winner and deciding conflict, exact stage/token/cause, no refresh/re-stage/automatic discard; global profile/transaction costs |

Connections in R02-R11 use the declared already-authenticated operation context.
Report connection/owner setup separately; R01 measures that work explicitly.
Do not carry reader/Store/input cache warmth from R01 or another sample into a
cold row. Root metadata required by an operation is measured even when the
surrounding owner was initialized before its call timer.

R04 is a complete reuse workload: no untimed warm-up and no speed claim for only
the final hit. Its subrequest hit counts are attribution, not independent warm
performance samples. R05 and R06 are different public surfaces and get separate
rows. R07's expectation applies to its sequential successful trace; overlapping
refills, refusals and unused consumed serials require their own accounted cases.

### Required scale/resource and failure companions

| Case ID | Selection | Scope |
| --- | --- | --- |
| Q01-mixed-load | 8 Workspaces/4 connections; fixed 10-second admission trace with timestamped Demand/Accept/Finish/Release units | One finite sustained sample; freeze offered work, byte mix, per-class service/debt budgets, admission and drain schedule first |
| Q02-root-1000 | Initial acquisition of 1000 entries with ignored/dependency/cache/output/.git state and exact symlink/hard-link identities | Public Init operation, complete source traversal and saved-root construction; fresh output, full membership oracle |
| Q03-root-10000 | Same generator at 10000 entries | Same defined operation, ownership and resource observations |
| Q04-root-100000 | Same generator at 100000 entries | Explicit large selection; original count remains visible even if NOT_RUN or over budget |
| Q05-root-1000000 | Same generator at 1000000 entries | Required huge-namespace resource/structural selection; default NOT_RUN until owning registration and feasible budgets exist |
| Q06-native-file-5g | Initial acquisition of one dense 5368709120-byte regular file | Native streaming, actual acquired bytes and bounded residency; sparse/compressed substitutes cannot supply this size claim |
| P01-slow-peer | Hold or stall one peer's credited output while another requests Demand/control service | Separate bounded functional/resource proof; verify unrelated progress and exact input/output ownership |
| P02-disconnect | Close real sockets at fixed header/partial-body/returned-operation checkpoints | Separate proof; retain unattempted, known and uncertain custody without replay |
| P03-local-full | One attempted local mutation on an owned limited backing device | Separate proof or identity-matched reuse of unchanged S6 evidence; global Store unaffected, original outcome retained |

Q02-Q05 manifests use the same shape/seed and freeze exact regular-file, directory,
symlink and alias counts, payload bytes, depths and maximum fanout. Include broken,
external, cyclic and opaque-byte link targets. Validate saved roots after source
removal in the separate verifier. Native source stability is an input obligation;
no atomic host snapshot is inferred. Namespace Init retains its supported worker
profile while all runs export LAYERFS_CONSTRUCTION_WORKERS=1.

Q01 must freeze numerical arrival/service/debt and memory limits before sampling.
Its 10-second offered trace does not remove admission, results, drain or cleanup
from the complete-command budget. No unbounded repeat loop, automatic resubmit,
new construction lane or artificial total-flow cap is permitted. A finite workload
trace is not a product duration limit. If this declared case cannot finish within
its budget, preserve that outcome rather than shortening the offered trace.

P01-P03 do not inject failure into a pure speed timer or claim latency superiority.
Use actual peers/devices and public ownership boundaries, with no product test
hooks. An unfenced history read cannot resolve unknown publication. P10's resolver
and any dependent missing proof remain explicit prerequisites.

## 5. Timing and round-trip ledger

Freeze these separate intervals per case:

- setup/preparation and owner readiness;
- operation call start through its promised acknowledged, checked result;
- queue admission through dispatch and retained delivery;
- complete command, including its selected process/container lifecycle and cleanup;
- independent verification and its cleanup;
- automatic reclamation after logical release, with the original owner identified.

Intrinsic product hashing, SQL, encoding, authentication, socket delivery and
result decoding remain timed. Argument/oracle generation and report calculations
stay outside the operation timer. Never sum nested/overlapping SQL, dispatch,
transport and operation spans as exclusive time.

Every row reports logical public calls, logical RPCs, successful/failed native
attempts, request records, grant records, result records, fragments, transferred
bytes, copied/zeroed bytes, connection/handshake counts, provider opens, Branch
refreshes and replay counts. Cold tree reads also identify dependent demand waves.
One logical request can contain many records; records are not round trips.

The current body-bearing flow is header -> grant -> body -> result. Count the
grant as a real dependency before body transmission. Zero-body calls have grant
and final replies but no client body leg. Do not assign every operation two RTTs
from record count alone. Record actual ordered timestamps and dependency edges.
Calls currently holds one connection driver across that exchange; R09 exposes
this rather than claiming host fair dispatch implies client concurrency.

Store-open, handshake, status polling or extra verification added by the harness
per request is an authenticity failure unless the selected operation includes it.
Freeze expected counts before the run. Unexpected counts invalidate the stated
route or leave attribution INCOMPLETE; do not hide them in setup afterward.

## 6. S7 cost, storage and residency observations

Record per operation and cumulative workload:

- SQLite statement attempts/executions, VM/fullscan/sort/autoindex/reprepare,
  returned/direct/trigger rows, logical input/return bytes and query plans;
- page/dirty/index/overflow/journal activity and actual VFS/device reads/writes,
  distinguishing API requests from physical storage transfers;
- physical reservation, allocation calls/requested bytes, logical bytes, actual
  high-water allocation, freelist credit and allocation rounding/metadata;
- local payload versus canonical/transport copies and zero initialization;
- queue wait, parked turns, admissions/refusals, outstanding/caller-held results,
  native workers and shared credits;
- logical eligible debt, real reclamation service, retained owners and final
  quiescence/disposition, including failed attempts and rollback work.

Pair actual SQL EXPLAIN with correlated execution profiles. Approximate statement
memory samples are not RSS, pager usage or cumulative allocations. Range-call
requested bytes are not newly allocated bytes. A counter's unavailable scope is
null plus reason, never zero. Account the full reservation even when the database's
logical page count is small. Reclamation does not imply file shrink or global GC.
EXPLAIN, residency attestation and counter snapshots must not prime measured data.
Use their declared observation boundary; reset/attest affected cache state before
a cold arm. Any real SQL/I/O required to collect a product receipt remains charged.

Observe the host runtime/global SQLite and Linux workload/Overlay separately:
phase baseline/peak/final RSS or footprint; cgroup anonymous/file/socket categories;
local/global allocated storage and temporary backing; relevant OS/Store/pager
caches; credit capacities and allocator limitations. Lifetime cgroup/process peaks
are not phase peaks. Include payload-sized page-cache/backing growth even when
first-party heap counters are bounded. Attestation/sampling precision and gaps
must be explicit; a sampled maximum is not a continuous exact peak.

Name N metadata entries, M operations, B payload bytes, K requested IDs, Q admitted
jobs, C connections and S source depth. Derive worst-case, amortized and cumulative
work from the actual mechanism and compare it with observed visits/copies. Reject
quadratic work, whole-file prefixes and input-sized resident acquisition sets.
Credit ceilings alone do not establish whole-system residency or service rate.

## 7. Setup, cache, budgets and execution discipline

Follow the [measurement workflow](../../../../docs/general/agent-measurement-policy.md),
[optimization guide](../../../../docs/general/optimization-guide.md) and
[report template](../../../../benchmark_agent_report.md) before each invocation.

1. Prepare pristine compatible inputs once and seal their producer/manifest.
   Use --setup clone for post-initialization cases where the owning runner supports
   it, with independent writable byte copies. Initial acquisition/fresh outputs use
   fresh setup. Never reuse a mutated sample, hard-link a master, replay measured
   process state or reopen a disposable Overlay as a supported clone route.
2. Every measured phase pays its own I/O from its declared cache state. Attest
   relevant source, Store/pack/database and OS residency before a cold claim.
   Clone, fresh container, fresh process or advisory cache hint is insufficient.
   Preparation, hashing or the phase's own prior writes cannot supply hidden
   warmth. Unknown/asymmetric state is INELIGIBLE/INCOMPLETE. Cache-hit functional
   receipts cannot be promoted to cold speed evidence.
3. Freeze exact commit/tree, relevant source, manifest/lock, toolchain, release
   profile, compilation/dependency/harness/oracle seals, binary SHA-256, Docker
   image/kernel/architecture, corpus/seed/schedule and cache identity. Use locked
   release binaries for new speed rows. Preserve root ARM64 build inputs; explicit
   RUSTFLAGS repeats them. Test compilation uses --no-run first.
4. One sample per case/arm. Follow the registered arm order. Keep failed,
   ineligible, refused, uncertain and unrun rows in fresh append-only outputs.
   Diagnose retained evidence/counts; no best-of, repeat-until-pass or unchanged
   treatment resampling. Source/harness/workload changes get fresh identities.
5. Default complete performance command <=15 seconds. Draft exception selection:
   Q04 and Q06 may have <=25-second complete budgets if prospectively registered
   as those exact exceptions before sampling. Q05 is NOT_RUN pending a feasible
   owning contract; no guessed longer budget. Independent performance proof is
   <10 seconds, with its exact bound frozen. Other existing families retain their
   actual scoped limits, not these proposed ones.
6. Every functional/test command has an explicit wall stop <=120 seconds;
   Docker uses an inner stop and bounded kill/join fence as well. That ceiling
   never enlarges a performance/proof budget. Timeout is FAILED; retain output,
   diagnose source/custody before a justified rerun, and leave no background run.
7. Take a nonblocking worktree-local lock and use local Cargo targets/outputs.
   No same-worktree build/measurement overlap. Declare other host/worktree and
   container interference. Preserve the four unrelated containers; do not restart
   the Docker backend or invalidate another owner's caches. An unavailable
   isolation/cache mechanism is a recorded eligibility gap.
8. Set LAYERFS_CONSTRUCTION_WORKERS=1; retain only the existing Namespace Init
   parallelism exception. Transport workers are separately counted, not extra
   constructors. No third-party changes beyond the already authorized fuser fix.
9. Exploratory timing is diagnostic. At final source, run the independent covering
   proof once or reuse an exact qualifying receipt. Reuse unaffected S6 physical
   and earlier-family evidence at its original scope. No CI, aggregate pre-push
   wrapper, legacy sweep, push, release or deployment is part of this plan.

## 8. Gates and interpretation

Required gates are separate: authenticity; correctness/reference closure/authority;
known/refused/conflicted/uncertain custody; cache eligibility; command/proof budget;
numerical latency/rate; physical storage; residency; fairness/debt; cleanup.

No new millisecond target or universal throughput/RSS figure is invented here.
Before an eligible candidate arm, freeze numerical targets from the owning contract
or an untouched, eligible matched baseline. A baseline-derived target follows the
policy's permitted ordering and is committed before candidate optimization or
sampling. Unresolved numerical memory/rate/latency targets block an admission PASS;
retain raw diagnostic observations and identify the missing gate.

Both speed and storage matter. Report raw time/allocated-byte deltas and preserve
frozen misses. Do not accept roughly 50% speed loss for roughly 5% storage saving.
Do not claim faster cold behavior from reuse, compare batch average to per-call
latency, or compare unlike profiles/public surfaces as paired arms.

For a valid pair, elapsed change percent is 100*(candidate_ns-baseline_ns)/baseline_ns;
speedup factor is baseline_ns/candidate_ns. Throughput identifies its byte basis and
is bytes*1e9/operation_ns. Workload service rate is completed_units*1e9/measured_interval_ns
over its defined interval. Backlog snapshots use arrivals-completions-cancellations
with admitted/refused/attempted units kept distinct. There is one sample, not a
median or repeatability claim. Per-operation trace quantiles, if registered, describe
that finite trace only and cannot be labeled between-sample statistics.

## 9. Receipts and report

Use a new campaign-owned directory under checks/s7-s9-speed/<unique-run-id>/ when
execution is selected. Do not create result files or performance PASS placeholders
for this draft. Required immutable artifacts are registration/gates, source/build
seals, fixture/cache attestations, exact commands/timeouts, binary pre/post hashes,
raw events/counters, allocation/residency traces, verifier/cleanup receipts and
all outcome/custody causes. Numeric units are ns, bytes and explicitly named rates.

The report contains one row for every registered selection, with:

| Field | Required content |
| --- | --- |
| Identity | Case/version, arm, public API/entrypoint, profile, operation/call topology, source/build/harness/workload/cache seals |
| Time | Raw operation_ns, complete_command_ns/bound, queue/delivery attribution, independent verifier_ns/bound; sample_count |
| Communication | Logical calls/RPCs, grants, dependent turns, records/fragments, socket/crypto attempts, transferred/copied bytes |
| Engine/resources | SQL/VM/rows/pages/journal/I/O, full reservation/high-water/freelist/range work, host/Linux residency and backing |
| Service/debt | Offered/admitted/refused/completed/cancelled units, queue peaks, per-class progress, retained credits/owners, cleanup debt |
| Verdict | Every separate gate; actual PASS/FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN and original error/uncertainty/reason |
| Reuse | Exact earlier receipt and unchanged mechanism/compilation/binary scope; no new sample fabricated |

All 27 draft case IDs in section 4 are initially NOT_RUN. Execution must freeze
the actual selected cohort and retain omitted siblings as NOT_RUN; a smaller
development selection cannot claim full admission. S8/S10-dependent checks remain
explicitly deferred, not counted as passing pre-S8 evidence.

Each eventual commit records exact first-parent/final staged/committed production
LOC with core/reference/combined totals using unchanged tools/production_loc.py.
This plan is documentation only and does not change production source. The earlier
S5/S6 stopping record, side documents, root reference and raw failures remain intact.

## 10. Next-ready preparation

Freeze the candidate consumer/host boundary and source; map E/R/Q/P cases to real
public APIs; register only the owning families and observers that can support the
claims; establish equal cache/resource observation and prospective numerical gates.
Then collect each selected arm once, independently verify final identity and append
separate S7/S9 receipts. Required missing S8 or external capability is reported by
exact dependency while useful independent S7/S9 work continues. No whole-filesystem
load-bearing or milestone-completion claim follows from creating this document.
