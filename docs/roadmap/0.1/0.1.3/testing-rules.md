# v0.1.3 benchmark testing rules

> **Status:** Current planning checklist; no v0.1.3 release candidate exists.
> These rules govern implementation and execution of the twelve family
> specifications. They extend the [general benchmark rules](../../../general/benchmark_rules.md).
> New workloads, selectors, fixtures, and result schemas are planned, not
> implemented commands or measured performance guarantees.

The premeasurement [execution contract](execution-contract.md) freezes the
baseline, safety deadlines, resource profile and evidence layout. The
[ordinary workload supplement](ordinary-execution-contract.md) and
[dedup/reliability supplement](dedup-reliability-execution-contract.md) bind
the remaining deterministic operation/oracle choices. The
[infrastructure reuse map](infrastructure-reuse.md) records the existing
components and qualification work required by #22.

## Stage 1: build and collect the initial baseline

Stage 1 is tracked by [#21](https://github.com/Ephemeral-AI-Lab/layerfs/issues/21);
shared infrastructure is [#22](https://github.com/Ephemeral-AI-Lab/layerfs/issues/22) and the
consolidated review is [#35](https://github.com/Ephemeral-AI-Lab/layerfs/issues/35).

Stage 1 implements and qualifies the benchmark, then records initial outcomes
on a pinned product baseline. Product performance/storage optimization belongs
to Stage 2, after the consolidated baseline review. Reusing and improving harness
preparation, selection and reporting is Stage 1 infrastructure work; changing
the measured product algorithm to make a new result faster is not.

Freeze the family specification and issue, input/oracle identities, authentic
route, correctness/resource gates and finite safety deadlines before collection.
Implement the complete family, run its selected diagnostic, then collect all
prescribed initial samples and independent proofs, including required extended
members. Correct benchmark defects before calling the resulting evidence ready.
Do not silently fix or optimize the product during that collection.

For the Phase 1 campaign, finish the scheduled performance collection across
families before running the complete verification/proof campaign. Implement
verifiers early, but run them rarely during development: only the smallest
check justified by a new execution route, a concrete correctness signal, or a
changed verifier. Do not run full-tree verification after every case or edit.
After a final-verifier defect, recollect only invalidated performance and rerun
affected verification; retain unaffected passes. The
[handoff prompt](phase-1-handoff.md) defines the coordinated execution loop.

A Stage 1 family issue may close when its benchmark is implemented and every
required case/subcase has an executed, reproducible, correctly classified
initial outcome with retained evidence. A product failure, timeout, unsupported
capability, or numerical miss is recorded as such and remains a failing release
gate. An unimplemented case, missing required observation, or unexecuted slot
is incomplete work, not a successful baseline. A supported-error oracle may
pass only for the exact documented capability contract.

Keep baseline collection completeness, harness validity, product correctness,
resources, cleanup and performance as separate statuses. Never turn Stage 1
issue completion into release admission or claim that a failed operation's
latency is eligible performance evidence. Preserve pre-fix failures; any later
product correction receives its own source identity and evidence.

The consolidated baseline report identifies measured bottlenecks, correctness
failures, preparation costs and coverage gaps. It proposes Stage 2 issues by
shared root cause and freezes performance targets before optimization. Do not
pre-create one optimization task per benchmark or tune fixtures/thresholds to
hide initial misses. The central release issue remains open beyond Stage 1.

## One implementation path

Use `benchmark/fs-bench-pro`, its existing binary, and its shared workload
helper. Each family has one Markdown specification, one canonical definition
module when implemented, and one thin runner or existing compatible family
runner. Do not create a second benchmark crate, framework, cache service,
container manager, or independent report pipeline.

Reuse and minimally extend:

| Existing component | Reuse |
| --- | --- |
| [`families/sdk_edit_common.rs`](../../../../benchmark/fs-bench-pro/families/sdk_edit_common.rs) | Four sizes/labels, bounded deterministic byte generation, existing payload hashes and identities where the fixture matches |
| [`lib-edit-sdk-runner.sh`](../../../../benchmark/fs-bench-pro/lib-edit-sdk-runner.sh) | Family selection, explicit selected/full modes, performance/verification stages, supervision, cleanup and receipts; isolate SDK-specific route checks |
| [`sdk-edit-custody.py`](../../../../benchmark/fs-bench-pro/sdk-edit-custody.py) | Build/source seals, preparation compatibility, cache publication, acquisition, independent clones and manifests |
| [`run-namespace.sh`](../../../../benchmark/fs-bench-pro/run-namespace.sh) | Existing namespace preparation/import lifecycle and inherited namespace controls |
| [`workload.rs`](../../../../benchmark/fs-bench-pro/workload.rs) | In-container ordinary filesystem workloads and bounded deterministic generators |
| Existing report generators and validators | Raw-data consumption, family completeness, provenance, phase summaries and independent verification |

A reusable part may move to a shared helper when a second family actually
needs it. Do not parameterize or copy the whole SDK editor into a generic
framework. Preserve all existing SDK scenario meanings and route checks.

SDK range-edit claims call the public singular or same-file batch API exactly
as declared. Ordinary filesystem/tool claims run the actual POSIX/FUSE workload
through public Exec. A same-file batch is not a multi-file API. No shell
reconstruction may substitute for an SDK edit; no SDK edit may silently
substitute for a tool's ordinary writes. Import claims perform actual public
LayerStack initialization. Report the observed route and call counts.

## Size, units, tiers and seeds

Use binary units consistent with the existing fixture infrastructure:

```text
tiers = [1, 10, 100, 500]
max_workload_file_bytes = 524288000                 # 500 MiB, inclusive
max_workload_total_bytes_exclusive = 1073741824     # 1 GiB, strict
seeds = layerfs-v0.1.3-seed-1
        layerfs-v0.1.3-seed-2
        layerfs-v0.1.3-seed-3
```

Apply the file and total caps at every initial, intermediate and final workload
state. Count logical lengths, sparse holes, temporary replacements, generated
outputs, Git internals, conservatively each hard-link pathname, and live unlinked
inodes until their last handle closes. Deduplication
or compression cannot reduce this accounting. Use a precomputed transient
bound and verify it at relevant operation boundaries, not just final cleanup.
A 500 MiB file cannot be appended to under this cap without reserving headroom.

The cap covers the workload filesystem. Physical Store, spool, fixture caches,
verifier artifacts, binaries and logs have separate measured disk budgets;
do not call the logical cap a total host-disk bound. For the history family,
also bound the sum of logical retained snapshot sizes; its 1 MiB initial tree
and at most 500 new Commits represent at most 501 MiB.

Each curve names one tier unit: bytes, paths, shards, requests, variants,
episodes, or Commit count. Never form a size × operation-count × history-depth
Cartesian matrix. Count-based schedules are nested prefixes of one deterministic
maximum schedule; fixture generation may construct only the selected prefix.
The fourth tier is 500, not an inferred geometric tier of 1,000.

New timed IDs have three fresh samples, one per seed. Fixed-payload cases reuse
their existing byte identity; seed labels identify schedules/repetitions rather
than silently changing released payloads. Proof-only recipes declare their own
cohorts/subcases and normally run once; do not multiply every fault by four tiers.
Shared identical controls are registered and executed once and referenced by
other curves. No reused evidence gets a second scenario identity. Independently
verify each distinct admitted fixture/schedule variant. Repetitions sharing
that exact variant may reference its source-bound verification receipt; never
use one seed's proof for a different seed's input or reuse a proof after its
product/oracle identity changes.

## Shared Workspace fixture

The new `workspace-shards-v1` profile reuses the existing bounded byte generator.
For `N` shards and file ordinal `j` in `0..199`:

| Ordinals | Files per shard | Bytes per file |
| --- | ---: | ---: |
| 0..127 | 128 | 1,024 |
| 128..191 | 64 | 8,192 |
| 192..199 | 8 | 49,152 |
| **Total** | **200** | **1,048,576 per shard** |

Thus tiers contain 200/2,000/20,000/100,000 files and 1/10/100/500 MiB.
Use zero-based shard index `s`; paths below are relative to the fixture root:

```text
j < 64:       wide/s{s:03}-f{j:03}.dat
64 <= j <199: regular/s{s:03}/f{j:03}.dat
j == 199:     spine/d001/d002/.../d128/s{s:03}.dat
```

Prepare an empty `dest/` directory. The spine uses the literal sequence of
128 short components and remains within the released path limits. The largest
wide directory has 32,000 entries, allowing repeated FUSE readdir-page evidence.
These paths replace, rather than add to, the 200-file shard population.
Additional family target files or directories must be explicitly accounted for.

Use file mode `0640`, directory mode `0750`, and mtime `1700000000.0`.
Set directory metadata after children are constructed. For each file, define
`frame(x) = len(UTF8(x))_le_u64 || UTF8(x)` and
`H = SHA256(frame(profile) || frame(seed_label) || s_le_u64 || j_le_u64)`;
use its first eight bytes as a little-endian state for
`sdk_edit_common::fixture_block`. Freeze all resulting hashes at fixture
admission; file identity must not depend on requested maximum N.
Stream in one reusable buffer of at most 1 MiB. Do not hash separately for
every output word or clone another shard's content to manufacture sharing.

For a 500-target operation schedule with declared domain `D`, rank indices
0 through 499 by `SHA256(frame(seed_label) || frame(D) || index_le_u64)`,
break ties by numerical index, and take the first N entries. Operation kinds
and size/depth cycles use scheduled ordinal; file identity retains the selected
master index. A family may declare a different explicit schedule when its
work unit is a shard, variant or Commit, but must preserve nested prefixes.

Small selected runs prepare only the required shards. Count curves needing a
fixed large background reuse that qualified background across compatible runs.
Metadata/content scans and Workspace-locality cases share the same fixture
artifact when their complete identity matches. Hashes of the flat SDK payload
are not hashes of this multi-file tree, even though the generator is shared.

## Make preparation reusable and lazy

1. Resolve the selected case/seed/arm before building or preparing anything.
   A selected tier must not silently build all four tiers or every family.
2. Reuse the compiled host binary, workload helper, and runtime image when
   their exact source/build identity is valid. Rebuild only affected artifacts;
   do not rebuild an image for a report-only edit.
3. Acquire immutable fixture bytes, manifests and independently constructed
   oracle data by content/metadata/generator/profile compatibility. Reuse across
   families and compatible source arms; family name alone is not a cache key.
4. Reuse a pristine initialized Store only when initialization is outside the
   measured operation. Measured import/creation starts with a fresh output and
   still reads, chunks, creates and publishes every required byte/path.
5. Use the existing per-key coordination and atomic publication. A missing,
   interrupted, corrupt or incompatible entry is rebuilt, not partially reused.
6. Validate the sealed master once per acquisition/run. Give every sample and
   writable qualification an independent copy or real COW clone, retaining its
   required identity check. Never hard-link a mutable sample to the master.
7. Close Store connections and quiesce artifacts before sharing them. Cache
   eviction is explicit maintenance; ordinary sample cleanup removes its own
   resources and leaves shared pristine entries intact.
8. Cache static expected manifests/chunk transcripts when their full oracle
   identity matches. Always verify actual candidate outputs; never cache a
   passing result, post-operation Store, live Workspace, reader cache or history.
9. Generate expected bytes by streaming; do not materialize full expected copies
   per sample when a streamed digest/manifest suffices. Prepare dedup files
   independently; input caching does not authorize reflink-manufactured sharing.
10. Keep preparation workers bounded and idle during product timing. Default
    performance concurrency is one. Parallel preparation/verification requires
    disjoint resources and a declared limit; it must not contaminate measured
    CPU, disk, cache, memory or container capacity.

Record acquisition/hit/miss, compatibility key, producing source, validation,
generation, oracle construction, Store initialization, clone/copy method and
wall, build/image reuse, temporary disk high water, and cleanup separately.
Report first-use and cache-hit command wall so preparation improvements remain
visible. Reuse does not make hashed or cloned inputs cold; preserve the declared
cache policy on both source arms.

## Fast selected loop and complete qualification

| Lane | What runs | Runtime policy |
| --- | --- | --- |
| Static self-check | IDs/counts, tier algebra, byte bounds, route/timing contracts, schema applicability | Product-free and small; no full fixtures or Docker |
| Selected development | One case, seed and arm; cheap status/count checks plus the focused regression check | Ordinary warm-prepared command aims for 1–5 seconds; diagnostic, never full-family admission |
| Family performance | Complete unique case set and prescribed samples; performance only | Ordinary families aim for tens of seconds; publish actual wall and classify expensive families explicitly |
| Family verification | Exact bytes/metadata, intermediate observations, reopen and resource proofs | Separate mode, receipts and deadlines; can take longer than performance |
| Extended qualification | Large history trajectories, spill/admission-boundary faults, sustained sessions | Explicitly selected; full release qualification must include required extended cases |

These are development objectives, not invented latency guarantees. Before any
measurement, freeze correctness/resource bounds and diagnostic timeouts; after
an untouched baseline, freeze numerical performance targets and final deadlines
before candidate optimization or sampling. Every family must bind selected,
full-performance and verifier deadlines independently. A selected timeout is
not a reason to discard a valid slow result or inflate an algorithmic bound.

Warm-prepared command wall includes acquisition/validation/cloning, required
fresh process/container readiness, workload, minimal receipts and cleanup.
One-time build/preparation is shown separately, not hidden in the user-visible
wall. A 500-Commit case cannot inherit a 1–5-second promise from a tiny edit.

The loop is: self-check → smallest selected failing case → inspect phase and
work counters → fix shared cause → rerun focused check and selected case →
expand only to the siblings needed to resolve remaining risk. Do not repeatedly
run already-passing families, all tiers, or the full verifier after each edit.
Run complete affected families when the candidate is ready, then one final
release qualification. A later relevant source change requires affected evidence
to be recollected; old results remain retained with their original source.

Use the existing explicit selected versus `--all` and performance versus
verification stages as the implementation starting point. New family command
names must be labeled planned until implemented. No default command may launch
all twelve families or an endurance run. An ordinary-lane pass cannot stand in
for complete admission if extended members remain outstanding.

## Pure timing, verification and result reuse

Performance invokes only the declared authentic workload, its acknowledgement,
and named lifecycle operations. Measure inner workload, aggregate SDK calls,
Commit, End and complete product lifecycle separately where applicable.
No added benchmark digest, tree oracle, object census, Store reconnect,
materialization or fault injection runs in the performance phase. Intrinsic
product/tool work, including CAS hashing/authentication and Git object hashing,
remains inside its declared timing boundary; the zero-verifier counters do not
mean disabling those operations. Cheap exit/status/count checks
may reject an incorrect result immediately. Actual verifier failures block
admission even when already-collected performance samples remain valid.

Verification uses a separate run/state and compares all paths against an
independent expected manifest: type, logical size, streamed bytes, mode, exact
controlled timestamps, link equivalence classes and symlink targets. Include
unchanged paths and intermediate observations when final operations cancel.
Reuse a common verifier; use failure-specific cohorts without weakening checks.
For tool-generated platform-dependent metadata, such as Git index inode/stat
cache fields, a family may explicitly combine independent semantic/format
expectations with complete pre-Commit-to-reopen byte/metadata custody. It must
identify those fields, keep source bytes and semantic outputs independently
specified, check every path, and label the custody receipt as persistence
evidence rather than an independent oracle. This exception does not allow
candidate-produced roots or receipts to define their own expected semantics.

Collect latency, work counters, memory, FUSE traffic and incremental receipts
from one workload execution. Let several analyses reference the same sealed
row. Expensive chunk/Store census belongs in separately attributed verification;
do not rerun the identical workload solely to populate another report. Never
count a shared control twice or treat a reused row as an independent sample.

Derive reports solely from sealed raw evidence. Do not rerun a successful
campaign because Markdown or a chart changed; recompute its report. A changed
workload, oracle, relevant product code, environment or timing definition does
require the appropriate new evidence or scenario version.

## Inherited evidence and release scope

The released three SDK edit families retain their meanings. Five 500 MiB
length-growing rows exceed the new result-size cap and need versioned bounded
replacements before future capped execution:

```text
insert/append/prepend/zero-extend 4 KiB: input 524283904, result 524288000
replace middle 2 KiB with 4 KiB:         input 524285952, result 524288000
```

Prepare shorter deterministic prefixes outside timing; retain the singular
public call. Version IDs, fixture hashes, plan hashes and family manifest;
never relabel the originals or claim an exact 500 MiB input. Other inherited
rows keep their own repetitions, schemas and disposition. The previous
POSIX/temp-copy edit families remain archival and do not return as SDK evidence.

Single-Branch retained-history storage growth is now v0.1.3 scope. Multi-Branch
sharing, fan-out, Add/promotion, competing heads and broader history-query
scaling remain v0.1.4. Count actual new retained Commits, not `UpToDate` attempts,
when computing represented-history bytes. Dedup ratios must distinguish
within-operation coalescing, preexisting chunk reuse, borrowed unchanged
references, canonical metadata, and actual SQLite footprint.

The [released storage profile](../../../versioned/0.1.0/storage-format.md)
guarantees live-process transaction semantics, not crash/power-loss recovery.
These benchmarks cannot upgrade that guarantee by testing orderly reopen.

## Admission checklist

- One canonical file/module/runner per family; shared infrastructure and input
  preparation reused; no duplicated framework or cached measured result.
- Explicit case/seed selection, complete-family opt-in, and separately selected
  expensive qualification; all required members eventually included.
- Size/transient bounds, exact fixture/schedule/oracle identities, public route,
  CPU/memory/disk caps, and independent phase deadlines fixed before admission.
- All prescribed samples/subcases retained; no retries to select favorable time.
- Independent full-state and relevant intermediate verification; source-bound
  correctness, resource, cleanup, custody and performance statuses all present.
- Reports show preparation and command wall as well as product timings; a fast
  inner call is not reported as a fast entire test.
