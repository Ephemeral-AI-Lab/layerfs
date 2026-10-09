# Resume S7 and S9 from the verified consumer checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prepared on 2026-10-06 (Asia/Singapore) at the owner's request in a side conversation.
> S7 and S9 are incomplete. This note records current source and a next-agent assignment.

## Assignment and boundaries

Finish the independent S7 engine cost/resource qualification and S9 authenticated
runtime/complete-root implementation in the primary checkout
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, local `main`, product code under `core/`.
Carry useful authorized work through implementation, qualification, local commits,
separate audits and tracker receipts. A verified intermediate checkpoint is not
the stopping boundary while independent work remains.

The latest main-thread owner direction separates S8 into a later batch. Keep its
audit open; do not implement ordinary Bash/FUSE/control lifecycle as a substitute
for finishing S7/S9. S10–S13 and P3/P6/P7/P13/P14 remain explicit later Commit
prerequisites. Existing local commit and tracker-evidence authorization continues;
pushes, releases and deployments are outside this assignment. Preserve the root
reference until its authorized retirement after cluster two completes.

The main thread `01a10e9a-bfbb-79e3-b175-d3c8bad99041` was idle at inspection. Its
latest `continue` turn was interrupted before it recorded new actions. No local
Cargo/consumer check process or owned consumer container was found at inspection.
These observations do not establish a hard external blocker or explain why the
turn was interrupted. Recheck ownership before starting; do not overlap another
agent's work in the same checkout.

This side conversation created only this new note. It did not edit product code,
stage or commit files, change the tracker, stop the main thread or change containers.

## First reads and reconciliation

Read current [root AGENTS](../../../../AGENTS.md) and [core AGENTS](../../../AGENTS.md),
both root handbooks ([cluster one](../../../../cluster_one_handbook.md) and
[CAS/CDC/delta](../../../../cas_cdc_deltaencoding_handbook.md)), then the
[303 design index](../303/README.md). Read its relevant primary operation contracts,
[daemon/SQLite](../303/daemon-sqlite.md), [FUSE](../303/fuse.md),
[runtime integration](../303/06-cluster-one-integration.md) and
[implementation/validation](../303/07-implementation-validation.md).

Read [optimization policy](../../../../docs/general/optimization-guide.md) before
changing performance-sensitive paths. Before measurements, read the
[measurement workflow](../../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[report template](../../../../benchmark_agent_report.md) and
[core harness routing](../../../benchmark/fs-bench-pro/AGENTS.md).

Use [SOURCE-ORGANIZATION-S7-S13](SOURCE-ORGANIZATION-S7-S13.md) as the owner's source
organization selection extending [SOURCE-ORGANIZATION](SOURCE-ORGANIZATION.md).
Its dated S6-in-progress statement is superseded by the completed identities below.
It selects organization, not algorithms or capabilities. Preserve public APIs and
crate boundaries. Add real implementation modules as needed, without empty scaffolds.

The older [S7–S9 handoff](HANDOFF-S7-S9.md) retains useful provenance, but its
S8-in-batch assignment and older next-work lists are superseded here. Do not redo
completed wire delivery or hard-link identity work because an earlier paragraph
calls them unfinished. The [S5/S6 stopping record](HANDOFF-S7-S13.md) remains intact.

Reconcile actual HEAD, working tree, active members in [core Cargo](../../../Cargo.toml)
and [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307) before adopting
these snapshot identities. At inspection, #307 was OPEN, last updated
`2026-10-06T04:44:09Z`, and S7/S8/S9 checkboxes were all unchecked.

## Exact committed baseline

Current HEAD: `a3b2ee13eb07569eb1be6875c6f281d4b882d35c`.
Current tree: `2821682e63efbb1bbf93fcc3e2d0ce1299e3dc7b`.
Branch: local `main`; checkpoints are local/unpushed.

| Commit | Role | Core production LOC | Reference LOC | Combined LOC |
| --- | --- | --- | --- | --- |
| `1b2580f2ef5b6d29c577237a9757c7b86d3898fb` | Authenticated native logical runtime and exact custody | 87578 → 91262 (+3684) | 65417 → 65417 (+0) | 152995 → 156679 (+3684) |
| `32f5073593ea60f27a85db5ab4ecdb2ea4a1dd1b` | Runtime source/tracker receipt only | 91262 → 91262 (+0) | 65417 → 65417 (+0) | 156679 → 156679 (+0) |
| `551f165ed332955067b30ce184ca3578f4e13f05` | Preserve native regular hard-link identity in Init | 91262 → 91342 (+80) | 65417 → 65417 (+0) | 156679 → 156759 (+80) |
| `a3b2ee13eb07569eb1be6875c6f281d4b882d35c` | Alias source/tracker receipt only | 91342 → 91342 (+0) | 65417 → 65417 (+0) | 156759 → 156759 (+0) |

Runtime source tree: `9d941a9c4bac9268f5a818452fbffb24f28c36d2`.
Alias source tree: `ca31b91c7793b3ff8a4936ae66f85a3ce6355452`.

Earlier completed identities: S5 `a0dc7da9b`; S6
`983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, tree
`be2744223a450eaa01b9f31c4e3c850bbd141d72`; S5/S6 receipt/handoff
`4ecea41983b673d62db90880b777a94565eac985`. They are stopping-boundary history,
not assignments to reopen completed campaigns.

Existing separate tracker receipts:
[S7 runtime costs](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009305142),
[S9 runtime wire](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009317243),
[S9 native aliases](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009500696).
These explicitly retain CHECKPOINT/incomplete status.

## Uncommitted consumer checkpoint: preserve and finish

The working tree contains these product changes from the main agent:

- SDK client additions: [call.rs](../../../crates/layerfs-api/sdk/src/client/call.rs),
  [call_error.rs](../../../crates/layerfs-api/sdk/src/client/call_error.rs) and
  [ports.rs](../../../crates/layerfs-api/sdk/src/client/ports.rs).
- Modified SDK [client exports](../../../crates/layerfs-api/sdk/src/client/mod.rs),
  [reply validation](../../../crates/layerfs-api/sdk/src/client/reply.rs),
  [runtime tests](../../../crates/layerfs-api/sdk/tests/runtime.rs) and
  [wire tests](../../../crates/layerfs-api/sdk/tests/wire.rs).
- Modified Bridge [contract errors](../../../crates/layerfs-bridge/src/contract/error.rs).
- Modified [architecture 40](../../architecture/40-runtime-wire-ownership.md), new
  [architecture 42](../../architecture/42-native-consumer-ports.md), and append-only
  [consumer receipts](checks/s9-consumer-ports/).

Also present: [S7/S9 speed test plan](S7-S9-SPEED-TEST-PLAN.md), an owner-requested
side-conversation draft. Preserve it as a separate proposal. All proposed rows are
NOT_RUN; it is not a registered runner, measured result, algorithm selection or
authorization to change the frozen qualification contracts.

Implemented consumer behavior:

- `Calls` uses one existing authenticated connection and serializes one bounded
  adapter call, with exact original grant/reply correlation, operation, identity,
  cardinality/order and receipt-phase checks. Failure is retained and terminal;
  no reconnect, refresh or replay is added.
- Independent `CloseHandle` is outside the call mutex and wakes blocked socket I/O.
  Partial-drain ownership is nonblocking and never claims an in-progress call joined.
- `RemoteObjects` implements `AuthenticatedObjects`, preserves its first original
  failure/credited response and refuses subsequent reads in that failed operation.
  Only typed owning MissingObject/ObjectMissing results become absence.
- `RemoteLengths` implements Workspace `FileLengths` without full payload demand;
  `RemoteSerials` implements `InodeSerials` using authoritative consumed ranges.
  Original call/remote/shape failures survive `WorkspaceError::Service`.
- Canonical hash failure is represented by `FrameError::IdentityMismatch`.

Verified receipts at this exact current source:

| Selection | Outcome | Actual wall | Scope |
| --- | --- | --- | --- |
| `consumer-repaired-tests-build` | PASS | 2.222530125 s | Host SDK/Bridge locked all-target no-run |
| `consumer-host-covering` | PASS, 40 bodies | 3.943456875 s | Bridge 14, SDK runtime 19, wire 7 |
| `consumer-linux-build` | PASS | 8.499906333 s | Docker SDK/Bridge locked all-target no-run |
| `consumer-linux-covering` | PASS, 21 bodies | 1.081544834 s | Bridge 14, wire 7; host provider cases cfg out |
| `consumer-clippy-initial` | PASS | 1.588690709 s | Host all-target SDK/Bridge, `-D warnings` |
| `consumer-linux-clippy` | PASS | 3.375255917 s | Docker all-target SDK/Bridge, `-D warnings` |
| `consumer-guard` | PASS | 0.448062208 s | Product boundary, 606 production files |

The consumer-specific real Store path and two portable fence cases are included
in final covering tests. All actual test commands have a 120 s outer wall ceiling;
Docker uses an additional 110 s TERM plus 1 s kill fence. No timeout occurred.
Seven compiled binaries per platform match their before/after SHA256 receipts.
Global provider execution remains macOS-only; portable transport executes in Docker.

Seven selected build/test/Clippy/guard receipts each contain an identical current
`source_sha256` map with 888 Rust/SQL/manifest inputs, including external tests and
excluded source. All 888 files were rehashed during this handoff and match.
The map cohort digest is
`d8ceabf2ca88891fee060b531b869c6e31d0235b1e0917af3e1414e0491c2e69`,
computed as SHA256 of sorted `path + NUL + hex_sha256 + newline` UTF-8 records.
This map is an execution identity, not the production LOC scope or a cold cache seal.

Pre-note `git diff --binary HEAD` SHA256:
`579145d9f150d758eb75f32e1dfcd6c72186ae761391df4a64f02391d77efcdf`.
That diff excludes untracked files. New product file hashes:

| File | SHA256 |
| --- | --- |
| `client/call.rs` | `d001708af4e5672e54eeb40d0570b024796d6392fcc08694822fd9781ce862e1` |
| `client/call_error.rs` | `270ecf93985a56f76274d0201d0d4ce470b2d4be543f344d5393b3a15da7ceb6` |
| `client/ports.rs` | `7ba8290ad8386f9ca89d157a0a62391c38e05a8e9f5297ba16024fa2eb1f39c1` |

The [failure ledger](checks/s9-consumer-ports/FAILURES.md) preserves an E0433 missing
`Arc` fixture import and a command-order mistake: a functional command was launched
after failed no-run, recompiled and failed with the same error, executing zero test
bodies. A qualified std type repaired the fixture; successful no-run preceded the
final executions. Do not delete these failures or label them runtime passes.

Consumer checkpoint still needs its final fmt/whitespace/docs/identity review,
exact staged LOC comparison, local source commit, committed-tree verification and
separate tracker/audit receipt updates. Reuse qualifying unchanged checks; rerun
affected coverage only if source changes or evidence requirements demand it.

## S9 remaining implementation, in useful next-work order

1. Finish the consumer checkpoint above without reverting its original custody or
   independent-close behavior. Review actual source and public contracts rather
   than assuming the passing functional tests establish whole milestone acceptance.
2. Implement faithful **backed initial acquisition**. Current Project
   [scan](../../../crates/layerfs-project/src/import/scan.rs),
   [namespace](../../../crates/layerfs-project/src/import/namespace.rs) and
   [work](../../../crates/layerfs-project/src/import/work.rs) still retain scan/job/
   frontier/children/inode/directory collections proportional to input. Move
   input-sized mutable state into owning backed/indexed storage, use bounded
   keyset/stream windows and charge scratch/copies/cleanup. Preserve public APIs,
   crate boundaries, native source stability checks and one-attempt failure.
3. Use existing public canonical constructors where appropriate:
   [directory `build_directory`/`empty_directory`](../../../crates/layerfs-content/src/filesystem/directory/update.rs)
   and [inode `build_table`](../../../crates/layerfs-content/src/filesystem/inode/update.rs).
   Do not route initial acquisition through a resident whole-namespace update or
   silently include excluded legacy source. A backing interface/concrete backend
   arrangement was considered by the predecessor but is **not implemented or
   selected merely by this note**. Choose it from the owning contracts and source.
4. Preserve already-fixed identity: opaque symlink targets without traversal;
   ignored files, dependencies, caches, outputs and `.git/index`; one logical inode
   and one payload construction per native regular device/inode; distinct equal-byte
   files; in-root refcounts excluding outside-root links; consumed serial holes
   never recycled. Validate saved roots after source removal. The old 4 GiB refusal
   is removed, but actual greater-than-4-GiB native streaming proof remains NOT_RUN.
5. Complete owning application/daemon runtime assembly and supervision for actual
   authenticated object/policy/serial/Save/history consumers. Keep socket I/O out
   of the provider owner; preserve demand/control service capacity, bounded fair
   jobs, interleaved Saves and same-Save visibility. No mutex/provider checkout
   spans a complete Save/Commit. This is S9 adapter ownership, not the deferred
   S8 native mount/control/Bash implementation.
6. Complete contextual authority/topology/provenance and exact disconnect/process-
   restart custody. Authentication alone does not establish authorization or
   reference closure. Old owner epochs/capabilities cannot become new authority;
   retained original queued bodies, completed outcomes and unresolved publication
   knowledge must survive the appropriate fence. Inspect P10's completion-fenced
   unknown-history resolver policy; never resolve unknown via an unfenced read,
   guess, resend, Branch refresh, re-stage or automatic conflict discard. Record
   exact scope/dependency if a resolver obligation belongs to later integration.
7. Qualify through actual public APIs and real Store: interleaved Saves, preserved
   typed refusal/conflict/uncertainty, disconnect at precise boundaries, slow-peer
   fairness, full roots and repeated bound use without whole-tree reacquisition,
   dependency restoration or a new database per call. Resource evidence includes
   all resident and backed state; a small fixture cannot prove huge-root bounds.

The [S9 audit](S9-EXIT-AUDIT.md), [wire ownership](../../architecture/40-runtime-wire-ownership.md),
[native aliases](../../architecture/41-native-regular-aliases.md) and
[consumer ports](../../architecture/42-native-consumer-ports.md) distinguish implemented
checkpoints from these open exits. Earlier table rows are dated; later completed
identity/wire appendices supersede their older gap wording.

## S7 remaining cost/resource gate

Keep the implemented SQL/startup/operation and native transport observations.
See [S7 audit](S7-EXIT-AUDIT.md), [operation costs](../../architecture/36-operation-cost-observations.md),
[engine receipts](checks/s7-costs/), [startup/service receipts](checks/s7-startup-s9-service/)
and [wire receipts](checks/s9-runtime-wire/). They establish real counts and scope;
they do not establish cold speed, phase RSS or sustained numerical acceptance.

Complete the following under a prospective owning workload/cache/observer contract:

- Whole-operation SQL/request/statement/VM/row/byte/copy/queue/custody/cleanup work,
  including actual triggers and failed attempted operations; correlated EXPLAIN
  and runtime profiling for affected database paths.
- Exact page/dirty/overflow/index/journal/device-I/O observations, or explicit
  derived bounds where the contract permits them. Current safe driver does not
  expose every exact dimension; do not relabel logical counters as physical I/O.
- Entire 268435456-byte per-daemon reservation (128 MiB mutation plus 128 MiB
  cleanup), high-water physical allocation, committed freelist credit, count
  triggers, identity observations and each range-allocation attempt. Requested
  range bytes are not automatically newly consumed disk; reservation is not RSS.
- Actual operation-phase residency across first-party buffers/queues, SQLite
  pager/journal, host/kernel caches and file-backed scratch. Heap limits, memory
  hints, statement-memory sums or a lifetime high-water mark are insufficient.
- Numerical arrival/service rates, per-class progress and cumulative reclamation
  debt during live activity and idle drain. Freeze offered load and acceptance
  limits before sampling; count later cleanup after foreground acknowledgement.
- Worst-case, amortized and cumulative costs as file size, namespace population,
  fragmentation, job pressure and retained lifetime grow. Reject quadratic work,
  hidden scans, input-sized residency and inflated caps; use bounded indexed state.

The [implementation dependency graph](../303/07-implementation-validation.md)
places S7 after S1–S6 and before S8. Do not invent a blanket S8 prerequisite for
independent engine qualification. Actual mounted kernel request/open/lookup/reply
costs require S8 product integration and remain explicitly unqualified here.
Reconcile that exact deferred scope against the owner's complete-operation
requirements before declaring the S7 exit; do not omit an owner-required dimension
or stop all independent engine work because the native witness is later.

The [draft speed/resource plan](S7-S9-SPEED-TEST-PLAN.md) provides candidate rows and
the intended macOS-global/Docker-local topology. Register/freeze a valid owning
family and observer/verification boundaries before qualified sampling. All of its
rows start NOT_RUN; it cannot supply retrospective thresholds or measured claims.

Reuse unchanged S6 allocation/device and earlier S7 SQL proofs at their exact
identity/scope. Retain old failures and unrun outcomes. S6 diagnostic counts and
the current uncontrolled-cache functional tests are ineligible as cold speed/RSS/
sustained-rate results. Do not repeat an unchanged passing treatment to choose a
better sample, warm a phase through setup, or remove timed product work.

## Settled fuser correction; preservation and execution inputs

The owner explicitly authorized the fuser **0.18.0 crates.io source plus the local
signed-timestamp patch** and subsequently accepted Docker verification as enough.
The old no-patch/published-corrected-release-only requirement is superseded for
this exact correction. Do not wait for a new release, adopt an unreleased Git
dependency or reopen QEMU/custom-kernel qualification as a batch prerequisite.

Keep [vendor source](../../../vendor/fuser-0.18.0/) and
[patch provenance](../../../patches/fuser-0.18.0/README.md). Only `src/time.rs` differs
from the official archive; all other 84 files are unchanged. Both owning manifest
patch entries select it. Run `python3 -B core/tools/check_fuser_integrity.py` before
native fuser builds; no shared registry edits or expanded third-party exception.
Read [the accepted qualification](FUSER-REGISTRY-PATCH-20261006.md). Linux's mounted
fractional signed-minimum endpoint FAIL remains a platform limitation with its
original verdict. Acceptance of dependency verification does not make that case
PASS or complete S8 product integration.

Preserve [SOURCE-ORGANIZATION-S7-S13](SOURCE-ORGANIZATION-S7-S13.md),
[WORKSPACE-EFFICIENCY-ANALYSIS](WORKSPACE-EFFICIENCY-ANALYSIS.md), the separate draft
speed plan, all historical receipts/assets and unrelated working-tree changes.
Do not revive `layerfs-server`, use root reference as a fallback, or activate
excluded FUSE/API-core/Sandbox/legacy source without real implementation and
explicit boundary accounting. Existing directories are not built capabilities.

These four unrelated containers were still running at inspection; leave them alone:

| ID | Name |
| --- | --- |
| `9cf2fe345496` | `layerfs-experiment-305-dev` |
| `ce75ac504df9` | `layerfs-4c8cde9ee1cf49f8049298b3377d4927` |
| `d2433851ea59` | `layerfs-dbd59ea75fdc62de0a1f6d9d099609ab` |
| `d2550144998b` | `layerfs-76b116dc9d984679f5802e2ec9c0798d` |

Consumer host checks use Rust 1.85.1, `--locked`, root ARM64 config and target
`core/target/cluster2-runtime-tests`. Docker uses Linux ARM64 image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
container workdir `/work` mounted from the primary checkout, Cargo home
`/work/core/target/cluster2-linux-cargo`, target `/work/core/target/cluster2-linux`.
The generic receipt's host `cargo_target` field is not the Docker build target;
the exact Docker command carries the actual target. Preserve that distinction.

Root aarch64 flags: `--cfg aes_armv8 --cfg polyval_armv8
--cfg chacha20_force_neon -C target-feature=+aes,+sha2`.
Export `LAYERFS_CONSTRUCTION_WORKERS=1`; Namespace Init alone retains its existing
supported four-worker profile. Global persistence runs through the supported
macOS provider, with Durable and Disposable guarantees kept separate. Local
Workspace backing remains disposable; no fsync/fdatasync/sync_data/sync_all.

Ephemeral predecessor helpers are currently present at `/tmp/layerfs-consumer-check.py`,
`/tmp/layerfs-consumer-binary-seal.py` and `/tmp/layerfs-count-staged.py`.
They are convenience tooling, not product source or authority. Read/recreate their
scoped behavior if useful; existing JSON/log receipts remain canonical if they vanish.
Never blindly reuse output names that would overwrite append-only evidence.

## Verification, commits and stopping boundary

Build selected changed packages first with `--no-run --locked`. Every actual test
invocation has an explicit wall timeout at most 120 s and ends at expiry. No
background tests or repeat loops. A timeout is FAILED: diagnose source and bounded
output before a justified rerun. Native timestamp proofs retain their tighter
owning ceiling. Functional ceilings do not waive stricter measurement/proof budgets.
Ordinary product Bash never inherits those test deadlines.

Run final affected-scope tests/examples, `-D warnings` Clippy, fmt, boundary guard
and applicable tooling tests. Reuse unaffected evidence honestly. No CI or
aggregate pre-push wrapper; `tools/preflight.sh` stays retired. Keep product files
within 999 physical lines and `lib.rs`/`mod.rs` thin and within 200; external tests
stay outside production source. Update affected architecture/API docs with changes.

Every local commit must include exact first-parent → final staged → committed
production LOC, with core/reference/combined before/after/signed deltas, using
unchanged `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Count first-party product, shipped SQL and excluded predecessors/application
adapters; exclude tests, docs, tools, fixtures, harnesses, manifests, third-party
and generated builds. Baseline at current HEAD is core 91342, reference 65417,
combined 156759. Uncommitted consumer growth is **not yet counted for a commit**;
recompute exact snapshots before committing. Verify the committed tree matches
the prepared receipt. Metadata-only commits still record the unchanged totals.

Update [S7 audit](S7-EXIT-AUDIT.md) and [S9 audit](S9-EXIT-AUDIT.md) separately, with
appended tracker receipts linking exact source/tree/build/workload/cache identities,
all original failures and every NOT_RUN/ineligible row. Leave S8 and later milestone
boxes unchecked. Complete S7/S9 only when their actual owning exits are satisfied.

Stop when the authorized S7/S9 batch is complete, or after all useful independent
work is finished and a precise required dependency/external gate remains. Report
the exact unmet requirement, why it cannot be supplied in scope, and the concrete
next-ready work. Unfinished implementation, observer design or benchmark registration
is work to perform, not an external gate by itself. The accepted fuser/Docker
correction is settled. Final handoff includes committed source/tree identities,
per-commit LOC receipts, separate milestone verdicts and any residual dirty state.
