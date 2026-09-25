# #232 implementation specification: SDK Exec/FUSE edit families

> **Status:** Current planning checklist; no release candidate exists.
> The 2026-09-24 scenario-v2 baseline has 56 receipts: 45 completed and
> verified, 11 product-side `FAIL`; see the
> [baseline report](exec-fuse-edit-v2-baseline.md). The new all-ioctl scenario
> is planning only and has no performance admission.

Tracking: [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).
Read with the [benchmark rules](../../../../docs/general/benchmark_rules.md),
[Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md), and
[Exec-to-edit guideline](../../benchmark/fs-bench-pro/exec2edit.md). The #232
owner amendment selects an **SDK-only Workspace Exec/FUSE edit** route; the
original direct SDK range-edit text and its 56 rows remain historical.

## Current owner route decision — 2026-09-25

The current prospective #232 route uses **one mounted ioctl range-replace
interface for all 56 cases**, reached by a cooperating program inside public
`WorkspaceApi::exec`, followed by public `WorkspaceApi::commit`. The
performance driver uses public SDK APIs for every product operation. The
command tool neither calls LayerFS private methods nor mutates a host path.
The ioctl adapter receives the operation from the opened file descriptor;
LayerFS does not parse shell text. An unmodified editor using ordinary POSIX
writes is a different product workflow and cannot supply one of these 56 rows.

The single semantic operation takes `(offset, delete_length,
replacement_stream)`, with literal and zero-run segments. It applies once to
the private Workspace and produces one revision. The current 4 KiB inline
ioctl remains a small transport; a new versioned staged transport must carry
the fixed 64 KiB benchmark replacements without turning them into sixteen
visible edits. Replacement length is bounded by the existing **8 MiB replay
contract**; the file/result length remains bounded by **4 GiB**. An ioctl
request over 8 MiB fails explicitly before mutation. A caller may separately
choose a normal POSIX file workflow, whose atomicity and I/O differ; the
adapter never falls back automatically. The largest of the 56 registered
replacements is 64 KiB, so all fit this semantic limit.

This decision supersedes the direct-SDK and mixed POSIX editor proposals in
sections 1–8 below, which remain the **historical scenario-v2 contract**.
Their fixture shapes, Edit→Commit boundary,
receipts and source identities stay unchanged; their old G2 targets do not
become matched Exec-route gates. Freeze a **new** immutable 56-case registry,
ABI proof, enforceable cache contract and per-case targets before candidate
implementation or sampling. The [unified implementation plan](UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md)
and [Phase 2 rollout](ROLLOUT_PHASE2.md) hold the file map and checkpoints.
The new scenario uses locked release builds, LFT1-only operation wall/CPU/RSS,
one performance sample per case, a ≤15 s complete command and a separate
identity-matched verifier under 10 s. No new performance admission follows
from this planning amendment.

> **Historical material below:** Sections 1–8 describe the frozen v2
> POSIX/FUSE route and its prior proposals. Their route, target, file map and
> implementation phases are not instructions for the new all-ioctl scenario.

## 1. Claim and timing boundary

The question is how long one public `WorkspaceApi::exec` edit plus its explicit
`WorkspaceApi::commit` takes on a previously mounted Branch Workspace. This
matches the **Edit → Commit timing boundary** used by the v0.1.6 SDK edit
campaign, but changes the edit operation from `Client::edit_workspace_file_range`
to a command whose POSIX writes enter through FUSE. Name this route
`sdk-exec-fuse-edit-commit-v1`; never label it a direct SDK range edit or a
matched v0.1.6 speedup/regression arm.

```text
first-use preparation, outside the selected command:
  sealed fixture → public SDK Project.init → closed prepared Store/history master

per-case setup, outside operation timer but reported:
  independent master byte-copy → layerfs-server Server.open → ProjectApi.fork
  → Sandbox.create → Workspace.mount → declared cache qualification

one layerfs-telemetry caller operation: edit_commit_ns
  start immediately before WorkspaceApi::exec(command)
    child edit_ns:   Exec returns typed exit/output after FUSE mutation
    child commit_ns: explicit Commit returns typed publication result
  stop immediately after the Commit result

outside operation timer, but reported:
  Workspace.status → Workspace.unmount → Sandbox.delete → command ends

separate verification command:
  independent reopen/oracle → verifier cleanup
```

The enclosing duration includes shell spawn, command execution/output drain,
FUSE callbacks, delivery, construction and publication. The caller checks
`exit_status == 0` and output truncation before Commit; its small branch/argument
work remains inside the enclosing timer. A failed Exec does not proceed to
Commit. An unknown Commit result is retained as uncertain and is not replayed.
Status, Unmount and Sandbox Delete still run as cleanup, with separately
reported outcomes and durations. The complete-command wall includes the fresh
Sandbox Create and its Delete, while the Edit→Commit operation timer does not.
No readback, assertion command, `docker exec`, fixture preparation or telemetry
publication goes between the measured calls. The caller's root duration is the
headline; child durations are inclusive substeps and are **not summed** to
reconstruct it.

## 2. Case membership and the semantic feasibility gate

The prospective matrix retains the *semantic shapes* and family names of the
historical 56 selections: `edit_length_preserving` (12),
`edit_length_changing` (32), and `edit_canonical_chunk_count` (12). These
names align with v0.1.6, but the new cases are separately registered
Workspace Exec/FUSE workflow claims under the [benchmark rule for POSIX/FUSE
workflows](../../../../docs/general/benchmark_rules.md#layerfs-sdk-file-edit-invariant).
Each case has one edit and one Commit. New scenario IDs append `-exec-v1` to
the historical selection ID; every receipt names the
`sdk-exec-fuse-edit-commit-v1` route and its operation surface. No new row can
inherit a direct-SDK `edit_*` PASS merely because its family name matches. The
receipt fixes `operation_entrypoint=WorkspaceApi::exec`,
`operation_surface=workspace-posix-fuse`,
`acknowledgement_boundary=WorkspaceApi::commit`, and `scenario_version=1`.
The original fixture size, edit offset, inserted/deleted length, replacement bytes
and input recipe seed 1 come
from the [v0.1.6 family definitions](../../../../benchmark/fs-bench-pro/families/).
Selection uses inherited `--repetition 1`. The five capped-v1 duplicates are
not added. The five `500mib-result-capped-v2` cases retain their smaller input
sizes: four at 524,283,904 bytes and one at 524,285,952 bytes. Thus there are
six reusable pristine fixture sizes, not 56 independently regenerated inputs.
Freeze a generated 56-row registry with exact command, declared editor
algorithm/allowed syscalls, payload/tool hashes, pre/post digests, final size
and chunk count before driver implementation. Observe actual kernel/FUSE call
counts rather than assuming an exact syscall sequence. Until that registry and
its hash are committed, all 56 rows are `NOT_RUN`.

The command is one sealed, release-built workload executable invoked by
`WorkspaceApi::exec` from the mounted Workspace; `/bin/sh` is part of the real
Exec implementation. Overwrite uses positional write, append/prepend/insert and
delete use the declared POSIX/FUSE algorithm, and truncate/extension use the
declared size operation. A temporary-file/rename save is allowed only if it is
the explicitly registered editor algorithm and is named as such; a hidden
full-copy rewrite cannot inherit an in-place range-edit target. Require the
existing product projection read/write/upstream counts and an independent
mutation proof. The current counters have no byte totals, omit size SETATTR
and rename, and are dropped by the daemon status wire. Before qualification,
extend the ordinary Workspace status route to expose bounded callback counts
through a public SDK `WorkspaceApi::status` call **after** the Edit→Commit
timer. Count size SETATTR and rename where cases require them. This is product
route evidence, not an alternate time/CPU/RSS sampler. The tool may neither call
LayerFS internals nor write a host-side path.

**Feasibility risk:** the current FUSE adapter has WRITE and size SETATTR but no
`fallocate` insert/collapse operation. The 20 middle/prepend/structural
length-changing cases may require shifting up to hundreds of MiB through FUSE,
whereas v0.1.6's SDK range edit did not. Do not shrink these cases, substitute
direct `EditFile`, skip them, or call their likely higher cost a regression.
Keep them visible as `NOT_RUN` until their authentic POSIX algorithm is frozen;
if it is implemented and misses the target, report `TARGET_MISS`. A product
FUSE capability needed to reach the target requires its own reviewed design and
LOC estimate, not a benchmark-only shortcut. The current sandbox fixes 512 MiB
container memory, a 1 GiB Workspace disk budget and a 16 MiB `/tmp`; a
500 MiB temporary copy may also exceed the real product budget. Record that
outcome instead of raising limits for a benchmark case.

## 3. Performance goal and comparison limit

The historical reference is the #152 **G2 candidate** at commit `8b5e0955e`,
not a fresh measurement of the final v0.1.6 tag. Its 56 direct-SDK
`edit_commit_ns` observations are in the
[#152 final report, rows 74–129](../../../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md#3-family--per-test).
The [v0.1.6 timer source](../../../../benchmark/fs-bench-pro/src/sdk_file_edit.rs)
starts before range Edit, stops after Commit, and times End separately.

| Prospective family | Cases | G2 observed range | Weak family envelope for context |
| --- | ---: | ---: | ---: |
| Length preserving | 12 | 4.72–10.01 ms | 10.01 ms |
| Length changing | 32 | 4.94–12.16 ms | 12.16 ms |
| Canonical chunk count | 12 | 6.02–13.54 ms | 13.54 ms |

The **per-case engineering target** is `#232 edit_commit_ns ≤ the candidate
edit_commit_ns printed for that same historical selection in the linked G2
report`, compared at the report's stated 0.01 ms precision. The exact 56 G2
values are the predeclared target table; the family maxima above are only
context and cannot turn a slower case into a match. These are aspirational
absolute targets, not paired comparisons or a claim that unlike operations do
equal work. Display each #232 case beside its historical G2 row, but never
compute a causal speedup ratio or reuse G2's PASS status. One eligible #232
sample at or below its own case target is `GOAL_MET`; above it is
`TARGET_MISS`. A fast row without route, cache, telemetry, independent oracle
and cleanup proof is never an admission PASS. G2 `--setup clone` did not prove
cold OS cache, so its cache state is **not** a matched cold comparator. An
unexpectedly high result stays on disk; do not rerun the arm to select a
better number.

The target is intentionally demanding. For length-changing cases, the
feasibility gate above may show that a public POSIX/FUSE edit performs different
amounts of work. Retain the target and report a miss; any future target revision
gets a new scenario version before new samples, without rewriting old rows.

## 4. SDK-only product route

All product operations, including setup and cleanup, use public `layerfs-sdk`:
Project Init, Branch fork, Sandbox Create, Workspace Mount, Exec, Commit,
Status, Unmount and Sandbox Delete. The
performance driver imports `layerfs_server` only for server assembly, `layerfs_sdk`
for product operations and `layerfs_telemetry` for observation. Python prepares
immutable fixtures,
launches the driver, checks cache eligibility and preserves receipts; it does
not edit the Workspace or call internal Service/Bridge/Store/Workspace APIs.
The verifier may use public C1/C2/C5 readers after the timed child exits.

Package boundary: `layerfs-sdk` has only `project`, `sandbox` and `workspace`
implementation modules (plus declaration-only `lib.rs`). Their public prefixes
are `ProjectApi`, `SandboxApi` and `WorkspaceApi`. Rename and extend the
**existing** `layerfs-service` package to `layerfs-server`; do not add a second
host/server package. Its existing `Service` stays the authorized C1/C2/C5
request handler. Group its handler, host-bound Project Init adapter, read/save,
records and validation under `src/service/`; group native assembly under
`src/host/`. Move the existing listener implementation rather than duplicating
it, and use one explicit `src/bin/layerfs-server.rs` entrypoint that calls
`host::run`. Keep `lib.rs` for the reusable Service/Server APIs.
Add Store/history create/open, peer grants, telemetry Runtime and SandboxOwner
assembly there as one concrete `Server` type.
`Server::create` prepares a new Store for Init and `Server::open` opens a
prepared clone. The SDK depends on `layerfs-server` and constructs
`ProjectApi`, `SandboxApi` and `WorkspaceApi` from a borrowed `&Server`;
`layerfs-server` never depends on the SDK, so this remains acyclic. Retire the
current SDK `host.rs` and redundant `client.rs` after their Init callers use
`ProjectApi::init` directly. Add Branch fork to the existing SDK `project.rs`;
do not add a `branch.rs` for one method. `layerfs-daemon` remains the Linux
FUSE/Exec process. The package rename is source relocation, not a measured
speed improvement; existing #236 receipts keep their old source/build identity.

Current blockers: the SDK exposes Project Init and Sandbox/Workspace methods,
but no public Branch fork or `SandboxApi::delete`; the current
`layerfs-service` package lacks `Server::open` for a prepared Store
clone with the listener, daemon grants, shared enabled `layerfs-telemetry`
Runtime and SandboxOwner lifecycle. The
[current live Docker test](../../../crates/layerfs-api/sdk/tests/agent_route.rs)
currently uses direct `Service::handle(HistoryCommand::Fork)` and constructs
the listener/owner itself. Its `Drop` cleanup calls `docker rm -f` and
`docker volume rm` directly. Implement setup and teardown as real public SDK
facilities before registering #232. Do not hide either in the benchmark
example or claim that dropping `SandboxOwner` removes its Docker resources.

`SandboxApi::delete(id) -> Result<(), DeleteError>` delegates to
`SandboxOwner::delete(id)`; an unknown ID
cannot name an arbitrary container. It performs a bounded graceful daemon stop,
then removes the owned container and its named Workspace volume. It reports
confirmed removal separately from retained/uncertain cleanup and removes the
owner's sandbox/workspace bindings only after confirmed success. `DeleteError`
retains the Sandbox ID, cause and which container/volume resources remain so a
caller can record partial cleanup and make an explicit later attempt. A retained
`CreateError.sandbox` ID can be deleted even when readiness never completed. No
silent force-kill, automatic retry or success on an unverified volume deletion.
The runner captures daemon LFT1 through an attached log stream before removal
and checks its run summary after shutdown. `Server` drop alone does not count
as a Sandbox cleanup receipt. A failure stops collection and retains the ID and
error for explicit recovery; it does not trigger a benchmark-side `docker rm`.

Keep the existing source guard in `runner.py`, allow `layerfs_server` only as the
composition owner and `layerfs_telemetry` only for observation, and reject direct
backend crates, host mutation, direct Docker Exec, and an independent operation
timer. Back it with runtime evidence: expected SDK call count/order, typed
results, post-timer SDK status/projection counts, and zero forbidden/fallback
route counts. The status query cannot run between Edit and Commit or act as a
pre-timer warm-up.
The source guard alone is insufficient for a PASS.

## 5. Telemetry and resource meaning

Use only `core/crates/layerfs-telemetry` for operation wall, CPU and memory:
enable native `Runtime` with timing, CPU and RSS, call
`OperationRecorder::run(key, "sdk.edit_commit.fuse", ...)`, time `edit` and `commit`
with its child scopes, then `Runtime::publish` outside the timed closure. Capture
raw `LFT1` from caller, Service and daemon where available; ingest it with the
existing `shared/telemetry.py` and retain one hashed `telemetry.lft1` file.
Require the expected caller root and two complete children, per-producer label
and cardinality expectations, producer identities, run summaries, zero
dropped/failed/overflow output, and exact source ranges. A nonzero Exec exit
or truncated output must become `Err` inside the recorded closure; otherwise
`OperationRecorder::run` would falsely mark the attempt successful.
Never replace a missing record with a Python timer, `resource.getrusage`,
`psutil`, cgroup CPU, or a synthetic duration.

CPU is the crate's covered **shared-process** user+system delta. Memory is its
**sampled process RSS maximum**. The host-direct SDK caller and Service share
one process, so report their CPU/RSS as one host-process window; the daemon is
a distinct process, and the shell edit child is not included in daemon RSS/CPU.
Do not call either process window whole-route CPU or container memory. The native
monitor's minimum interval is 10 ms, so a short Edit or Commit may have no
valid CPU/RSS window. Report `UNAVAILABLE`, never zero, and never extend the
operation or add sleeps to obtain samples. Operation wall remains valid when
resource coverage is unavailable, but a required resource gate cannot PASS.
Cache residency checks and complete-command wall are eligibility/supervision
evidence, explicitly separate from LFT1 operation telemetry.

## 6. Release builds, fast lane and cache eligibility

This #232 selection uses Cargo's default **release** profile with `--locked`
for the SDK driver, independent verifier, Linux daemon and any Rust edit tool.
Do not run or reuse `target/debug/` binaries. The host command begins with
`cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release` and
selects only the needed examples/packages; executables come from
`target/release/examples/`. Build the Linux daemon and edit tool as release
artifacts in an immutable image. Pin toolchain, profile, flags, Cargo.lock,
binary hashes and image digest. The existing #236 **Init-only debug** profile
is a different selection; do not silently change or pool it. Use a
worktree-local Cargo target and exact-binary reuse keyed by compilation seal.
Build wall is reported outside operation time; reuse never crosses into a
measured call.

The current Core runner builds Init only and archives from `debug/examples`.
Add an explicit #232 release build selection and include profile in its cache
key; do not toggle Init globally. Build the Linux daemon and workload executable
for Linux in the image build, rather than copying a macOS release binary into
Docker. The edit tool is a benchmark-only Rust binary under
`core/benchmark/fs-bench-pro/workload/`, built with its own locked manifest and
`--release` inside the Linux image. Its manifest declares a local `[workspace]`
so Cargo does not mistake it for an undeclared Core product workspace member;
include that lockfile and source in the
compilation seal. The benchmark image must contain `/bin/sh`, the release
daemon and the sealed edit tool. Pin the immutable image ID and actual binary
hashes.

Fast-lane iteration is one selected representative case first (a 1 MiB middle
overwrite), one performance-only attempt at a frozen source identity, and
diagnosis from its LFT1/call/byte counts. No full verifier in the exploratory
performance command; record `SKIPPED`. An uncontrolled-cache diagnostic is
`INELIGIBLE` regardless of speed. After the final source identity, collect
one sample per registered case and arm, then independently verify each matching
receipt once. If the selected receipt already has the final identity and cache
proof, reuse that exact receipt for its case; never resample it. If code changes,
retain the old attempt under its old identity and sample the changed source
once. No n3, retry, best-of, case removal, timeout inflation or worker
increase. The complete performance command, including container lifecycle and
cleanup, has a hard ≤15 s budget; a miss is recorded, not hidden. The owner sets
a #232 independent-verifier hard ceiling of **≤15 s**, with <10 s as the design
goal; this explicit #232 ruling supersedes the repository's default <10 s
verifier limit. A verifier miss stays failed; the budget is not expanded.
Commit/construction uses one product worker; Init keeps its
separate exception. Keep all `FAIL`, `INELIGIBLE`, `TARGET_MISS` and `NOT_RUN` rows.

For fast preparation, resolve only the selected case. Create each of the six
pristine masters once through public SDK Project Init, close and seal it, then
reuse its immutable Store/history and fixture manifest by compatibility key.
For a sample, clone the selected master as an independent writable byte copy,
open that clone through `layerfs_server::Server::open`, and create the sample
Branch through
the SDK. Create a fresh Sandbox for that case; after Commit, query public
Workspace status, Unmount, and call `SandboxApi::delete` before the performance
command exits. Never reuse a live Sandbox or mutated Workspace as preparation,
and never regenerate or reimport the whole source for every case.
The master compatibility key includes the fixture bytes/metadata manifest,
generator and seed, Core storage/canonical format, preparation route, and
the release binary/compilation identity. Unknown compatibility fails closed.
Validate the master once per acquisition, retain its seal, and avoid repeated
full-source rehashes per sample. First-use generation is separately reported.
The prospective recurring acquisition + byte-copy + clone-qualification goal
is ≤5 s for 1/10/100 MiB and ≤10 s for 500 MiB inputs; report every actual
preparation wall and `PREP_SLOW` on a miss. These goals do not move reads or
construction required by Edit/Commit into preparation. Reuse release builds,
Linux image layers and immutable binary archives through compilation seals,
as v0.1.6 did; report every reuse identity.

The cache contract must be frozen **before** benchmark implementation and
enforced per relevant macOS Store and Linux FUSE/backing data domain. Reuse a
closed validated prepared master by independent byte-copy `--setup clone`, but
never call that cold. After Mount and before Edit, invalidate and non-faultingly
check every data source the edit/Commit will read; include device-read evidence
where the claim requires storage reads. Mount-created metadata state is declared
as the same untimed precondition for every case. A Commit that reads Edit's
recent writes from resident backing is **not** cold-qualified merely because
both calls are in one timer. Do not insert a benchmark-only eviction between
Edit and Commit; fix the authentic product residency behavior or mark the row
`INELIGIBLE`. If the relevant process/cache domain cannot be checked, retain
the raw diagnostic with `INCOMPLETE`/`INELIGIBLE`, not a speed PASS. Cold and
explicitly warm diagnostics never share a summary.

## 7. Expected file structure and LOC budget

This is the proposed minimum layout; add a file only when its named work is
needed. Expected lines are planning ranges, **not** the required per-commit
production LOC count. Count exact nonblank, non-comment production LOC from the
first parent and final staged tree for each eventual commit; report legacy,
Core and combined totals in its message.

```text
core/
  Cargo.toml, Cargo.lock                # replace service member/dependency
  crates/layerfs-api/
    core/src/project.rs                 # typed Branch result
    sdk/Cargo.toml                      # depend on layerfs-server
    sdk/src/project.rs                  # ProjectApi::init and Branch fork
    sdk/src/sandbox.rs                  # public Sandbox.delete
    sdk/src/workspace.rs                # post-timer public status route
    sdk/src/lib.rs                      # three API modules/reexports only
    sdk/src/{client,host}.rs            # delete after caller migration
    sdk/tests/init_project.rs           # retain direct ProjectApi tests
  crates/layerfs-server/               # rename existing layerfs-service package
    Cargo.toml                          # package name/deps; no SDK dependency
    src/lib.rs                          # export existing Service and new Server
    src/service/mod.rs                  # declarations/reexports only
    src/service/handler.rs              # existing service.rs: auth/admission
    src/service/init_project.rs         # existing project.rs: Service Init
    src/service/{input,error,records}.rs # existing handler support
    src/service/{read,save}/           # existing C1/C2/C5 operation modules
    src/host/{mod,config,run}.rs        # moved native listener and config
    src/host/assembly.rs               # create/open Store, owner, SDK context
    src/bin/layerfs-server.rs           # tiny executable: calls host::run
    src/main.rs                         # delete after entrypoint move
    examples/benchmark_init.rs          # relocate existing #236 driver
    examples/benchmark_edit.rs          # release #232 driver
    examples/verify_namespace.rs        # existing independent Init verifier
    examples/verify_edit.rs             # independent #232 verifier
    tests/agent_route.rs                # SDK-only live Docker route
    tests/                            # existing Service tests, renamed imports
  crates/layerfs-daemon/src/lifecycle.rs # status counter wire mapping
  crates/layerfs-sandbox/src/
    owner.rs                            # DeleteError, owned deletion, registry
    docker.rs                           # bounded stop/container/volume deletion
  crates/layerfs-fuse/src/adapter.rs    # count metadata mutations
  crates/layerfs-workspace/src/filesystem/
    projection_counters.rs             # bounded callback classes
  crates/layerfs-bridge/src/
    contract/control.rs                # status counter fields
    adapters/native/protocol/response.rs # status encode/decode
  crates/layerfs-service/              # remove old path after rename
  benchmark/fs-bench-pro/
    runner.py                          # existing runner gains #232 selection
    families/edit_length_preserving.py # 12 overwrite cases
    families/edit_length_changing.py   # 32 size-changing cases
    families/edit_canonical_chunk_count.py # 12 count cases
    shared/edit_common.py              # case/fixture/target definitions
    shared/edit_preparation.py         # six masters and byte-copy setup
    shared/edit_cache.py               # macOS/Linux eligibility/self-check
    shared/telemetry.py                # LFT1 #232 label/cardinality checks
    tests/test_edit_families.py         # focused route/receipt/cache guard
    image/Dockerfile                   # sealed Linux daemon/tool + /bin/sh
    image/build.py                     # release image build and identity check
    workload/Cargo.toml                # standalone benchmark-only Rust tool
    workload/Cargo.lock                # locked Linux release dependency seal
    workload/src/main.rs               # POSIX/FUSE edit tool, no library
  docs/issues/232/SPEC.md              # this plan
  docs/benchmark/fs-bench-pro/exec2edit.md
  docs/architecture/{10-counters,14-service-runtime,16-history}.md
                                        # update affected product boundaries
```

The existing `runner.py` remains the only `list`/performance/verification/report
entrypoint. Do not copy v0.1.6's `setup.sh`, `perf.sh` and `verify.sh` into every
family. Keep the good part of its layout: three small family definitions over
one shared fixture recipe and one sealed workload binary. Each selected case
gets a fresh append-only performance output; independent verification writes a
different fresh output bound to that performance receipt, and reporting reads
both without running the product again:

```text
benchmark-results/fs-bench-pro/<performance-run>/
  build.json, run.json, report.txt, manifest.json
  sdk-exec-fuse/<family-id>/<case-id>/
    perf.jsonl, receipt.json, telemetry.lft1, driver.stdout, driver.stderr

benchmark-results/fs-bench-pro/<verification-run>/
  verification.json, verifier.stdout, verifier.stderr, manifest.json
  # verification.json pins the exact performance row and source/image identities
```

| Planned change | Expected added physical lines | Production LOC contribution |
| --- | ---: | ---: |
| Typed Branch result and ProjectApi fork | 80–140 | about 65–115 |
| Service→Server package rename and runtime assembly | 220–350 new; existing Service files relocated | about 125–245 net after Host move and Client retirement; package move itself 0 |
| SDK/owner Sandbox Delete and Docker custody | 90–150 | about 70–120 |
| SDK Workspace status and bounded FUSE count export | 100–170 | about 80–135 |
| **Base product subtotal** | **490–810 added physical lines** | **about 340–615 net production LOC** |
| SDK driver + independent verifier + Linux edit tool | 360–600 | 0 (examples/benchmark tool) |
| Runner, three family registries, preparation/cache, image build, telemetry ingest, tests | 800–1,300 | 0 (benchmark/test tooling) |
| Live test and guideline updates | 40–100 net | 0 (test/docs) |

The base product estimate is a **floor**: it excludes FUSE positional-edit
optimization, callback **byte** totals, and any cache-related product change.
Git may display the moved Service tree and `server/`→`host/` module as
deletions/additions; that relocation
does not become new production LOC or a speed improvement.
Keep both `src/service/mod.rs` and `src/host/mod.rs` below 200 physical lines
with declarations/delegation only; the existing 283-line `service.rs` body
becomes `service/handler.rs`, not `service/mod.rs`.
If those become required, estimate and review their source separately before
implementing it; do not conceal their LOC in the benchmark budget. Keep every
new production file below 1,000 physical lines
and `lib.rs`/`mod.rs` below 200.

## 8. Five implementation phases and exit gates

1. **Freeze the case and cache contract.** Commit 56 new Workspace Exec case
   IDs, exact command templates, editor algorithms, tool contract, payloads,
   fixture sizes, expected result recipes and per-case G2 engineering targets.
   Freeze the macOS Store and Linux FUSE/backing cache acquisition and
   non-faulting self-check before benchmark implementation. Pin the image base
   and recipe now; pin the actual binary/image digests after the release build.
   Keep the 20 structural cases registered as `NOT_RUN` until an authentic
   algorithm is feasible. **Gate:** registry cardinality, oracle and cache
   method are independently reviewable; no timed sample exists.

2. **Consolidate Server and complete the public SDK route.** Rename
   `layerfs-service` to `layerfs-server`, group operation code under `service/`
   and native assembly under `host/`, and move the tiny binary to
   `src/bin/layerfs-server.rs`. Move/retire SDK Host and Client, update Cargo
   dependencies, imports, Init example/tests and runner paths without changing
   historical receipts. Add `ProjectApi` Branch fork, `Server::create/open`,
   `SandboxApi::delete` and post-timer `WorkspaceApi::status` with bounded
   projection counts. Extend the existing live Docker test to prove SDK-only
   setup, edit visibility, Commit publication, fresh readback, historical root,
   conflict/unknown outcomes, and Sandbox deletion after normal and retained
   creation. **Gate:** the live test/driver calls product operations through
   SDK APIs; owned containers and volumes have a confirmed cleanup result.

3. **Build the release benchmark substrate and fast preparation.** Extend the
   one Core runner with a separate `--release --locked` #232 selection; keep
   #236 Init debug identity separate. Build a sealed Linux daemon/tool image
   with `/bin/sh`. Keep the three edit family definitions directly under
   `families/` and share one fixture recipe and preparation path;
   the existing runner owns selection, setup, performance, verification and
   reporting, so do not add per-family shell wrappers. Prepare only the
   selected one of six pristine Store/history
   masters through the SDK, validate its compatibility key once, and make an
   independent writable byte copy per case. Add the independent verifier and
   LFT1 ingestion, source guard, cache self-check, one-sample/cardinality and
   append-only receipt checks. Status/projection proof runs after the timer.
   **Gate:** focused route and harness checks pass; no debug binary, warm
   source, direct Docker edit or unverified clone can qualify.

4. **Run the fast lane and diagnose by counts.** On a frozen source, run one
   selected 1 MiB middle overwrite with Edit→Commit LFT1 and verification
   `SKIPPED`. Require its declared cache eligibility; an uncontrolled-cache
   result is retained as `INELIGIBLE`. Report preparation wall (goal ≤5 s for
   1/10/100 MiB, ≤10 s for 500 MiB), complete performance command (hard
   ≤15 s), and the case-specific v0.1.6 G2 speed target. Diagnose misses from
   the retained LFT1, SDK/FUSE counts and byte evidence, not a second sample of
   the same arm. A product fix creates a new source identity and retains the
   earlier attempt. **Gate:** one authentic selected route with complete
   evidence, or an explicit `TARGET_MISS`/`INELIGIBLE` diagnosis; no best-of
   selection.

5. **Collect and verify the full registry.** At the final source identity,
   collect each registered case/arm exactly once; reuse the Phase 4 receipt
   for its case if its source and cache identity still match. Independently
   verify each performance receipt under the owner-directed ≤15 s hard cap
   (<10 s design goal), including final size/mode, expected canonical root and
   chunk count, Branch head, unchanged paths and retained old Commit. Use the
   v0.1.6 fast-verifier precedent: fresh reconnect/reopen and at most 192 KiB
   of declared edit-boundary bytes, reporting
   `full_file_bytes_verified=false` with exact coverage and omissions. A full
   500 MiB digest is separate optional proof only if it fits the cap; cached
   expected bytes never replace observed output. **Gate:** publish every
   `GOAL_MET`, `TARGET_MISS`, `FAIL`, `INELIGIBLE` and `NOT_RUN` row with raw LFT1,
   source/build/image/cache identities, independent proof and cleanup.

#232 is complete only when every registered case has an authentic SDK/FUSE
route, eligible Edit+Commit evidence, goal status, independent proof and cleanup
record. A single fast canary, a warm diagnostic or #236's functional receipt
does not complete the pilot.
