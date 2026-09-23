# #231 first-pass Init benchmark and core benchmark substrate

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Written 2026-09-23. This is a specification and implementation plan, not a
> performance receipt. Freeze and commit the applicable case contract before
> building its driver or collecting its first sample.

Tracking: [migration #230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230),
[Init pilot #231](https://github.com/Ephemeral-AI-Lab/layerfs/issues/231), and
the [shared substrate prerequisite #235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235).
Read this with the [mode and timing map](../pipeline-and-modes.md),
[telemetry ingestion](../telemetry-ingestion-and-retention.md), and
[benchmark rules](../../../../../docs/general/benchmark_rules.md). The
[implementation handoff](HANDOFF.md) lists the required reading and work order.

## 1. Decision and evidence status

[#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179) is closed; its
bounded Workspace/FUSE route and small real-daemon two-Commit scenario passed.
Its closure explicitly defers load, throughput, cold-cache and maximum-size
qualification. Its native `InitLayerStack` is a bounded bootstrap using a
pre-described manifest and pre-saved file roots, **not** the v0.1.6 public
native-directory import. Therefore #179 closure unlocks substrate work but
does not make any `init_namespace` performance row runnable.

The first #231 collection cohort is **100, 1,000 and 10,000 files only**. Each
gets one raw performance sample per case and an independent verification.
Report every number with its source, route, cache state, resource
scope, result and gate status. `namespace-100000` stays in the registry as
`NOT_RUN`, with the missing public route/scale proof and the 2.7 s historical
cold target stated. This first pass neither turns 100,000 into a PASS nor
silently removes it from #230's original four-tier completion gate. A later
owner ruling may give the 100,000 tier a separate issue or revised admission
contract; do not infer that ruling from the three smaller measurements.

Every future sample must pin its own product/tree/build/image identities.
Neither this document's date nor the issue's closed state substitutes for that
seal.

## 2. First-pass case registry and operation

The source fixture counts below come from the v0.1.6
[`init_namespace` registry](../../../../../benchmark/fs-bench-pro/families/init_namespace/mod.rs).
MB here is decimal, as in that registry. Seed is **1**; do not run a sweep,
control/candidate arm or repeat a passing sample. The structured-text siblings
retain separate historical optional identities and are not part of this pass.

| Case ID | Regular files | Data directories | Logical bytes | First-pass disposition |
| --- | ---: | ---: | ---: | --- |
| `namespace-100-compact-v3` | 100 | 1 | 5,000,000 | Measure once after the real native import and contract freeze |
| `namespace-1000-compact-v3` | 1,000 | 10 | 20,000,000 | Same |
| `namespace-10000` | 10,000 | 100 | 300,000,000 | Same; report any capacity or complete-command miss |
| `namespace-100000` | 100,000 | 1,000 | 500,000,000 | `NOT_RUN` in this pass; retain the cold-source and 2.7 s target as unmet future work |

The measured operation must be a **public native-directory Init**: start the
caller timer immediately before the public import request reads the source
directory and stop after its confirmed final LayerStack/root response and
required bounded result consumption. Scanning names, reading file bytes,
constructing C1 objects/tree, accepting them into C2, and publishing the C5
identity/LayerStack record all remain inside that interval. Source fixture
generation, immutable-manifest validation, runtime startup and cache
preconditioning are outside it and reported separately. Init may use its
registered multi-worker construction path; Commit/capture/snapshot keep one
construction worker.

At this specification's baseline, there was **no equivalent public import in core**. #231 owns specifying
and implementing it before any row above can be measured as Init. Stream or
batch bounded input through the real Service/C1/C2/C5 route; do not pre-save
file roots, send a pathless 128-entry bootstrap, issue 10,000 mounted creates,
or hide multiple acknowledged Init operations behind one timer. Name any new
operation/schema and changed case identity before implementation. If that
route is still absent, emit `NOT_RUN`, not a time from an easier surrogate.

The first-pass public operation is `HistoryCommand::ImportNativeDirectory`
(history-command metadata tag 9, no request body). Its wire fields are a
16-byte authority stack body, bounded stack name, and 32-byte scope seed. The
authorized Service reads the exact operator-configured `LAYERFS_IMPORT_ROOT`
directory; the client cannot choose a host path. The Service refuses absent or
non-directory configuration, symlinks, unsupported file types, invalid portable
names/metadata. It has no default file or entry-count ceiling; allocation and
inode-serial representability remain checked. It scans and
reads source bytes **during this one request**, saves C1 file objects through
C2, builds the complete filesystem root, then publishes one C5 genesis stack.
The registered Init path has four file-construction workers and an eight-object
bounded handoff to the one C2 save owner. Per-file timing detail is grouped into
aggregate scan, file-construction and prerequisite spans, preserving complete
timing trees without changing the one caller sample.
The reply is the existing `StackCreated` result. The caller timer begins before
the daemon request frame and ends after decoding that reply. This operation has
fixture/route identity `core-native-directory-import-v1`; historical case IDs
remain selectors, but no v0.1.6 performance equivalence is inferred from the
matching counts and bytes. The 100,000 tier is explicitly `NOT_RUN` because it
is outside this first-pass collection cohort, not because of a product count cap.

The first three times are a **discovery cohort**: no new v0.1.7 latency target
or v0.1.6 speedup is approved here. Freeze correctness, resource, cache and
operation boundaries before collection; record `admission_eligible=false` and
`performance_gate=NOT_FROZEN` until an owner-approved numeric rule is committed.
One observed number is not a median or percentile. Do not promote discovery
receipts later by changing their labels; an admission campaign needs a new
prospectively frozen identity and fresh sample.

The source fixture profile for this cohort is `core-native-import-fixture-v1`
with seed 1. Paths are `dNNNN/fNNNNNN`, 100 files per directory, with indices
assigned in order: anchor, empty, tiny, small, medium. The 100/1,000/10,000
cases use class counts from the legacy registry respectively
`(1,1,78,15,5)`, `(1,10,789,150,50)`, and
`(1,100,7899,1500,500)`. Their one anchor is respectively 1, 5 and 100
decimal MB. Each positive non-anchor file starts with one byte; remaining
logical bytes after the anchor and these one-byte minima are apportioned with
weights tiny=1, small=64, medium=1024 by integer floor, then one extra byte
to the lexicographically earliest paths until the exact case byte total is
reached. Every content chunk of at most 1 MiB is the SHAKE-256 output of
`core-native-directory-import-v1|<case>|1|<path>|<chunk-index>` (UTF-8),
truncated to that chunk's length. File mode is `0640`, directory mode `0750`,
and all mtimes are exactly `1700000000` seconds. Fixture generation hashes
each file while writing it and seals one complete path/metadata/size/SHA-256
manifest without a subsequent content read. The changed profile avoids a
false claim that matching counts/bytes reproduce v0.1.6 content bytes.

## 3. Shared substrate prerequisite: minimal operations

The new prerequisite under #230 owns the reusable **benchmark harness**, not
the missing Init product API. Implement only the paths needed for one real
#179 daemon-host canary and the three #231 selections. The first runner has
one registered `daemon-host` route and no control/candidate arm, paired-run
scheduler, `--source-arm`, `--perf-samples`, sample verification mode or
unimplemented `storage-direct` adapter. Add another route only with its first
real case. Reuse Stage 6 and root harness mechanics where their identity and
cache rules match; do not copy their complete runners.

| Operation | Inputs and output | Why it exists |
| --- | --- | --- |
| `list` | Read-only case IDs, claim and `NOT_RUN` reason | Makes the deferred 100,000 row visible without build or setup. |
| `run --case ID --out NEW` | Lazily acquire/reuse only that case's sealed source, build/reuse exact product binaries, create fresh Init output, take **one** full public-operation sample, invoke a separate verifier child, parse telemetry, clean temporary files and write receipts | One command is the fast development loop. It refuses an existing output path, missing public route, identity mismatch, undeclared cache state or fallback. |
| `run --family init_namespace --out NEW` | Call the same selected-case path once for each of the three first-pass IDs, serially; build once | Gives one family report without three redundant builds or a hidden sample loop. The 100,000 row is emitted as `NOT_RUN`. |
| `verify --run RUN` | Re-derive existing performance and separate verifier receipts from retained raw evidence | Catches receipt/report drift without another product run or performance sample. The verifier child already run by `run` has a hard 5 s wall limit per case. |
| `report --run RUN` | Render a human table from retained evidence | Report-only edits never cause a new product run; failures and `NOT_RUN` remain visible. |

`run` owns case-scoped preparation automatically; a separate `prepare`,
`prune`, `calibrate`, `self-check` or mandatory `build` command adds no first-pass
capability. Focused unit tests provide self-checks; owned temporary files are
removed after their retained evidence is validated, while the three sealed
source masters remain for reuse. The first-pass harness takes **no
machine-global benchmark lock**. Each agent uses a different worktree; the
runner places its writable Cargo target, prepared source masters, sample
Stores and outputs inside that worktree. It holds one nonblocking OS `flock`
only while a run owns that **same worktree's** fixture/output namespace; two
runs in one worktree cannot mutate it concurrently. Builds take no benchmark
lock. Different worktrees never wait on each other's run lock. Give each run
unique dynamically bound ports, credentials, container names and telemetry
namespaces. Refuse a target outside its worktree, a symlink/path escape or an
existing output path. Record
observed competing builds/containers/measurements as possible timing
interference rather than claiming the host was quiet. The
substrate's real-daemon canary proves process placement, output capture,
teardown and receipt completeness using an existing #179 public operation;
it is diagnostic and cannot clear an Init case.

## 4. Expected implementation and result folders

These are expected homes, not empty directories to scaffold in advance. Use
Python orchestration around the **existing production daemon/Service binaries**
and public protocol, following the real route in
[`history_route.py`](../../../../../core/crates/layerfs-daemon/tests/history_route.py)
without treating its 128-entry bootstrap as Init. Do not create a second Cargo
workspace, lockfile or Rust benchmark binary unless the eventual public native
import cannot be driven through the production protocol. If a Rust driver is
actually necessary, add only that driver using the core lock and worktree
target, then update this structure before implementation.

```text
core/benchmark/fs-bench-pro/
  AGENTS.md                         existing agent rules
  runner.py                         list/run/verify/report, lazy one-case setup
  families/
    init_namespace.py               case declarations, public Init call,
                                    separate full-verifier child entrypoint
  shared/
    identity.py                     exact source/build/image/fixture seals
    fixture.py                      one-case source acquisition and expected map
    telemetry.py                    LFT1 validation and one-file ingestion
    evidence.py                     receipt, manifest and owned cleanup
  tests/
    test_init_namespace.py          case identities, fixture/oracle refusal checks
    test_substrate.py               identity/isolation/telemetry/receipt checks

benchmark-results/fs-bench-pro/      ignored, per-worktree owned output
  prepared/<compatibility-key>/      closed source fixture + sealed manifest
  <unique-run>/daemon-host/init_namespace/<case>/
    perf.jsonl                       one raw caller/phase observation
    telemetry.lft1                  exact LFT1 event lines when enabled
    receipt.json                    parsed stats, identity, cache and statuses
    verification.json               separate oracle and cleanup proof
  <run>/manifest.json               hashes of every retained run file
  <run>/report.txt                  derived, reproducible human table
```

No machine-global lockfile, shared mutable Cargo target, fixed network port, per-file
telemetry log, duplicate stderr log or local telemetry segment is
retained after successful ingestion. See section 7 for failure handling.
Prepared source fixtures are not measured output Stores; each Init selection
creates a fresh independent destination. Historical Stage 6 and v0.1.6
receipts stay under their original identities and are not copied into this
result tree as new observations.

`tests/test_init_namespace.py` checks the case IDs and sizes, deterministic
fixture manifest, and rejection of missing, altered or extra output. It does
not substitute for the separate full verifier child: that child reopens and
reads the actual persisted namespace for each benchmark case and writes
`verification.json`. `tests/test_substrate.py` checks selector, isolation,
identity, telemetry and receipt failure paths. The #179 canary exercises the
real daemon/Service deployment before Init collection.

## 5. LOC forecast and accounting

This document changes **0 production LOC**. The shared substrate prerequisite
should also add **0 production LOC**: it is benchmark harness, tests and docs
outside `core/crates/*/src`. The simplified Python-only first pass is expected
to add roughly **400–800 nonblank, noncomment benchmark/tooling lines**:
150–250 for the runner and Init family module together, 150–300 for reused/adapted
fixture/identity/evidence helpers, and 100–250 for telemetry parsing and
focused tests. These are planning ranges, not a budget to game by dropping
validation or compressing code. Add a Rust driver and its LOC only if the
public native import proves unreachable through the production protocol.

#231's missing **product importer** has no honest LOC estimate yet: its API,
streaming boundary and C5 publication design are unresolved. Record its actual
production LOC before/after and signed delta for each eventual commit, with
legacy/core subtotals, using the repository's per-commit counting rule. Do not
mistake benchmark LOC or documentation lines for production LOC.

The first-pass verifier should use the public protocol while it can meet the
full oracle and 5 s limit. If 10,000 per-file RPCs make that impossible, add
only a small verifier process over public C1 `FilesystemRead` and C2
`StoreProvider` to walk the persisted root directly. Build it through the
existing core lock and target, not a second harness workspace, and account for
its added **benchmark** LOC separately. This is a measured fallback design
decision, not permission to sample the oracle or change the measured Init path.

## 6. Expected one-sample stats and micro timing

`M1`–`M4` are report headings, **not** native telemetry field names. One
case has `sample_count=1` and one raw `operation_ns` from the caller's
monotonic timer. Setup, process startup, verification, cleanup and complete
command wall have separate fields. A case may emit many daemon and Service
`LFT1` operations inside that single sample; preserve their actual labels,
process/clock domain, invocation count, inclusive `elapsed_ns`, success and
incomplete flags. Do not add parent and child or subtract daemon and Service
durations to make a transport number.

| Report scope | Expected fields; `null` with reason if unavailable |
| --- | --- |
| M1 public native Init | `operation_ns`, source files/bytes scanned, C1/C2 objects and confirmed root; source scan/read and final acknowledgement remain inside the timer |
| M2 daemon-host delivery | daemon and Service operation counts and local `elapsed_ns` trees, actual input/output bytes and route identities; no isolated network latency from their subtraction |
| M3 C1/C2 | `service.begin_save`, construction/content children, storage read/finish and saved-object/byte counters; `service.construct` includes C2 handoff and is not pure C1 |
| M4 C5 | reservation, stage/publication outcome and named durations once instrumented; absent child spans remain `null`, not free work |
| Process resources | daemon and Service `cpu_shared_ns`, sampled maximum RSS, source, sample count, gaps, first/open/last/close timestamps; not exclusive CPU or exact phase peaks |
| External resources | host caller/service phase scopes, sandbox cgroup CPU and anonymous/file/kernel/total memory where applicable, OOM/swap, Store/history/spool disk separately |
| Iteration speed | `build_wall_ns` and build reuse, `preparation_wall_ns`, `operation_ns`, `verification_wall_ns`, `cleanup_wall_ns`, complete command wall and three-case family-cycle wall; keep every miss visible |
| Eligibility | cache state and residency evidence, telemetry expected/seen/loss, result/root oracle, performance, verification and cleanup statuses, exact source/build/image/harness/fixture seals |

The first-pass human table has one row for each of the three measured IDs and
one explicit `namespace-100000` `NOT_RUN` row. Columns are case,
files, logical bytes, cache claim, `sample_count=1`, raw `operation_ns`,
complete-command wall, admission eligibility, reason, and independent
telemetry/verification/cleanup statuses. No median, min-max range, p95 or
cross-version speedup is derived from one sample. A linked detail table may
show the raw M1–M4 micro events; it must keep process clocks separate.

## 7. Telemetry parse and garbage recycling

Use the existing native `Runtime` with `LAYERFS_TELEMETRY=forward`, one run ID
and distinct Service/daemon namespaces. Capture each process's stderr
separately from protocol stdout, then follow the
[ingestion contract](../telemetry-ingestion-and-retention.md): require `LFT1 `,
JSON `v=1`, known kind, matching run/role/namespace/PID, expected operation
labels/cardinality, complete required timing tree, run summary, zero reported
drop/failure/overflow and intact capture. `resource_status=sampled` alone does
not prove boundary coverage; inspect sample timestamps and gaps. Keep every
exact product event line, with producer byte ranges and hashes, in **one**
`telemetry.lft1` file per case. The parser version, interpreted values, missing
events and non-event diagnostics live in `receipt.json`; the run manifest hashes
retained files.

After all producers exit and streams reach EOF, validate and hash the retained
file, then remove temporary stderr captures and exclusively owned operational
telemetry segments. Record their removal in cleanup before final receipt and
manifest publication. If capture, parsing, hash or source mapping fails, keep
the original stderr in that attempt's fresh evidence directory and mark
telemetry `INCOMPLETE`. A failure or ineligible measurement still retains its
raw event file and receipt. Never remove another owner's logs, prepared
fixtures or historical evidence. There is no second
telemetry sampler, output protocol or permanent per-sample log tree.

## 8. Cache, verification, budgets and completion

The first three numbers must state the real source-cache condition. If the
source-reader domain has no verified cold contract, register it as
`source-cache-uncontrolled-v1`, retain any observed residency and report the
row as discovery-only `admission_eligible=false`; do not call it cold or PASS.
A future cold claim requires invalidation after setup writes, non-faulting
whole-input residency confirmation and any required device-read evidence in
the domain actually reading source bytes. The old
[`cold.py`](../../../../../benchmark/fs-bench-pro/shared/cold.py) applies only
to its original 100,000-file case and cannot be relabelled as proof for these
three. Cold and uncontrolled results are never pooled.

Prepare and seal the expected path/metadata/size/per-file SHA-256 manifest
**once** while acquiring the immutable fixture; do not rehash source files
before each run. Verification starts a separate process after Init, checks
the public returned root, reopens the persisted Store/history, walks every
directory through a public C1 reader and streams every file into SHA-256
against that sealed manifest. Use one reusable read buffer and bounded
pagination. Avoid 10,000 per-file daemon/Service round trips if they miss the
5 s budget; the alternative in section 5 is still a real public product read
path, not raw SQL or expected data passed to the mutator. Declare any mounted
readback as a separate, bounded coverage witness; do not label sampled mounted
paths an exhaustive proof. Verification has its own invocation/resource scope
and the exact performance identities. A skipped or partial full oracle is
`INCOMPLETE`.

The complete performance command retains the ordinary **15 s** budget, with
only a prospectively declared small exception up to **25 s**. The **entire
verification invocation is capped at 5 s per case**, including reopen,
readback and teardown; it may not use the older 60 s allowance. Preparation,
the three test runs, verification and cleanup should complete in **30 s per
family**; report the actual family-cycle wall and any miss. This 30 s family
number is a development-cycle recommendation, not permission to shrink a
workload or oracle. Do not increase a timeout, shrink 300 MB, move source
reads outside Init or raise worker counts to turn a miss into a pass. A tier
that cannot fit is `NOT_RUN` or a recorded miss with its measured wall and
reason. The 100,000 tier's 2.7 s cold target is neither revised nor tested by
this first pass.

Every invoked **Cargo build must finish within 30 s**, including first-use
and changed-product builds. Use only the exact changed production binaries,
`cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release`, a
worktree-owned `core/target`, compatible immutable binary/image seals
and no routine `cargo clean`. A matching artifact skips Cargo entirely for a
Python-only harness edit. Record first-use, edited-product and unchanged
builds separately, plus the compiled units/jobs and complete qualified build
wall. Historical root evidence measured a first native Cargo build above
40 s, so the new 30 s limit is **not already proven**. An over-30 s invocation
is `BUILD_SLOW` with its exact wall and cause, blocks the sample at that
identity and must be fixed in the build graph or reuse path; do not hide it
with a warm no-op, a stale executable or weaker `--locked` checks. Image build
and run-cycle walls are recorded separately.

The effective Cargo target comes from `cargo metadata`, not an assumed
default; reject a user config or `CARGO_TARGET_DIR` pointing into another
worktree or a shared directory. Reuse immutable executable and full Docker
image **IDs**, not mutable tags. Legacy BuildKit uses a globally locked Cargo
cache ID, and its tag/prune code can collide across worktrees. The successor
does not copy that global pruner or tag rule. Reuse a qualified image before
building; if distinct concurrent image builds contend on that cache, give
them separate worktree cache IDs and record the added cold-build cost. Never
change a writable Cargo cache to unsafe shared access just to remove a lock.

One sample per case at one exact source/fixture identity is evidence
cardinality, **not** one active run on the whole machine. A relevant source
edit creates a new identity and may get a new single diagnostic sample in
that agent's worktree; keep the earlier receipt. Dirty source can be hashed
and marked diagnostic. Admission requires a clean exact source/build seal.
An interference observation is a snapshot, not proof that an otherwise empty
host was quiet; an observed overlap stays in the receipt and makes an
admission performance claim ineligible.

The substrate prerequisite closes when its four CLI operations, exact
identity/isolation/reuse rules, parser/recycling checks, external resource scope,
report re-derivation and a real #179 route canary pass. It must also show the
30 s Cargo-build rule and 5 s verifier watchdog in evidence. An unresolved
`BUILD_SLOW` or verifier timeout at the accepted identity blocks completion;
a warm no-op alone does not qualify the build.
It does **not** claim Init performance. The #231 first pass is reviewable when
all three raw numbers and their independent proofs, nonpassing rows and the
100,000 `NOT_RUN` entry are retained. Record the 30 s family-cycle result
without claiming it passed if it missed. This first pass does **not** by
itself clear #230's existing four-tier pilot completion gate or authorize the
next migration cluster.
