# S7–S9 continuation prompt

> **Status:** Current planning checklist; no release candidate exists.
> Updated 2026-10-06 after the owner-authorized fuser correction and instruction
> to commit all remaining working-tree documents and generated graphics.

Use the prompt below to continue the remaining batch in a fresh local chat.
The [fuser stopping receipt](HANDOFF-FUSER-PATCH-S7-S9.md) and earlier audits retain
their exact evidence. The [S5/S6 handoff](HANDOFF-S7-S13.md) remains the completed
chat's stopping-boundary record; do not rewrite it or restart S5/S6.

## Copy-and-paste prompt

```text
Continue LayerFS cluster-two S7–S9 in the primary checkout
/Users/yifanxu/Ephemeral-AI-Lab/layerfs, current local main, product code under core/.
This is a fresh local chat. Start with core/docs/issues/307/HANDOFF-S7-S9.md,
then reconcile actual HEAD, working-tree state, active members and tracker #307.
Continue from the current checkout; do not reset to an older checkpoint.

Completed/checkpoint identities:
- S5 completed at a0dc7da9b.
- S6 completed at 983c2ee6d36a4417d8fff2d14db6b141f5386c8c, tree
  be2744223a450eaa01b9f31c4e3c850bbd141d72; original receipt/handoff
  4ecea41983b673d62db90880b777a94565eac985. Keep the completed S5/S6 chat and
  HANDOFF-S7-S13.md as their stopping-boundary record.
- S7 cost observations/S9 local history checkpoint:
  ae03d22e7675058e7cd062d66882ee3d7949f03e, tree
  2377fa37f4b6abb2ca6a6fec8f829087bc263314; receipt 2fc73b7a19464ac6cf68ead45a1625d54eb2ca13.
- Authorized fuser correction/proofs:
  b8d87019f93e897476f24bd3e7572933da18900f, tree
  3cd19d2e96348731cab8681f6de02877da926a0d.
- Following receipt/handoff: 5cde942fc6b84abbfa6c12a11db83fc6689a2ad9, tree
  344d992a8a31c89b25f76fce713bab8b148a1a29. This prompt and the all-state
  documents/assets commit follow that receipt; use actual current HEAD.
S1–S6 are checked. S0/S7/S8/S9 remain incomplete and unchecked. Never infer
milestone completion from a prerequisite proof, source existence or this prompt.

Read current root/core AGENTS.md, both root handbooks, the #303 design index,
the relevant primary mount/Exec/Commit/unmount/status, engine, FUSE, runtime
integration and implementation/validation contracts. Read the separate
S7-EXIT-AUDIT.md, S8-EXIT-AUDIT.md and S9-EXIT-AUDIT.md before selecting work.
Historical #301 packets and old review pins remain context, not current topology.
The optimization investigation and WORKSPACE-EFFICIENCY-ANALYSIS.md are research.

Use SOURCE-ORGANIZATION-S7-S13.md as the owner-selected organization recommendation
extending SOURCE-ORGANIZATION.md. Its dated S6-in-progress statement is superseded
by the completion identities above. It selects organization, not algorithms or
capabilities. Preserve crate boundaries/public APIs; add real modules as needed,
no empty scaffolds. Keep lib.rs/mod.rs thin within200 lines and new/replacement
production files within999 lines. Excluded packages are not built replacements.

S7 — close the complete engine cost gate:
Current receipts already cover original-job SQL/direct-versus-trigger changes,
returned BLOBs, payload copies, allocation/freelist/high-water and queue/parking
observations. Extend the owning overlay diagnostics/database and daemon service
boundaries to complete-operation SQL/request/page/byte/copy/queue/residency/debt
accounting, including startup/preparation, dirty/index/overflow/journal/device-I/O,
retained results, maintenance and whole-system phase residency. Include the entire
268435456-byte (256MiB) per-daemon reservation, actual high-water allocation,
committed freelist credit, count triggers and range-allocation calls. Derive and
justify worst-case, amortized and cumulative costs with actual evidence. Reuse
qualifying unchanged evidence at its original scope; retain failures/unrun rows.
S6 diagnostic counts and current uncontrolled-cache component tests cannot credit
cold speed, phase RSS or sustained arrival/service-rate qualification.

S8 — implement native FUSE and daemon/control/ordinary Bash lifecycle:
Use FUSE mount/requests/dispatch/ownership/coherence and daemon service/registry/
lifecycle/execution/control/upstream groups. Preserve exact kernel request,
open/lookup/reference and one-reply ownership, bounded deferred dispatch and fair
short jobs, stable identity, permissions, coherent caches/mmap and writeback off.
Qualify frequent fresh mounts and sustained same-mount calls/Commits with exact
frontier and terminal detach/join/cleanup ownership. Keep one daemon-owned overlay
SQLite database initialized before readiness; one writer does not serialize whole
Execs/Commits. Ordinary short/long Bash Exec has no automatic runtime timeout,
command-specific restoration, implicit Commit or automatic unmount. Only explicit
terminal unmount closes local ownership. Bash exit is not an activity fence.
Resolve relevant S0 prerequisites alongside implementation.

Owning FUSE verification uses Docker on Linux ARM64, as the owner reaffirmed.
Use the pinned Rust image, an owned fresh container, /dev/fuse, CAP_SYS_ADMIN
and the documented mount permissions; record the actual backend kernel and all
source/binary/image identities. Do not introduce a QEMU verification workflow.
The existing mounted timestamp and lifecycle proofs already ran through Docker.

Fuser is already corrected by the explicitly authorized local patch:
The owner's latest instruction was "use fuser 0.18.0 from crates io and apply patch".
It supersedes the earlier no-patch/corrected-published-release-only condition for
this specific correction. Keep version exactly0.18.0 and both owning root
[patch.crates-io] entries selecting core/vendor/fuser-0.18.0. Only src/time.rs differs
from the official archive; other84 files remain identical. The recorded diff,
all85 original file hashes and resulting source hash live under
core/patches/fuser-0.18.0/. Do not edit shared registry source, adopt an unreleased
Git dependency or silently expand the recorded dependency exception.
Run the focused core/tools/check_fuser_integrity.py before native builds.
Core currently excludes replacement FUSE and records this patch as unused; the
independent native harness actually builds it. Activate product members only with
real implementation and preserve platform cfgs.

Read FUSER-REGISTRY-PATCH-20261006.md and append-only checks/fuser-registry-patch/.
Four time unit tests, five exact public parser/reply cases, mounted negative
fraction, whole signed minimum and native mount/read/unmount/join pass on pinned
Rust1.85.1 ARM64 Linux6.12.76. Those library defects are fixed. The required mounted
(i64::MIN,200000000) case remains FAILED: Linux VFS clears nanos at the filesystem
boundary before setattr; public Session preserves the exact fraction when supplied.
Retain the failed case, original published-package failures/forced-abort137 and
historically rejected Git candidate receipts. No complete native timestamp PASS,
smaller contract or raw-wire product replacement is authorized. Keep the hard
owning-platform boundary gate explicit and do not mark S8 complete while unresolved.
Continue useful independent S7/S8/S9 implementation; the old wait-for-fuser-release
ruling must not stop the entire batch or cause repeated unchanged timestamp probes.
The owner subsequently asked to fix the remaining issue and selected Docker.
Treat the remaining failure as an exact native platform requirement: a Docker
container shares its backend's kernel, and rebuilding an image does not change
the VFS rule before the FUSE callback. Preserve the four unrelated containers and
the shared backend. No Linux correction or alternative backend was installed in
this checkpoint. A kernel-source/build-tools investigation ran no new native
test and its failed Docker tool-image build is retained. Any actual resolution
must preserve the requested timestamp and be qualified through Docker; do not
guess the removed fraction, reduce the contract or relabel the existing FAIL.

S9 — complete authenticated host adapters and full roots:
The active SDK has local authenticated binding, object/policy/serial/Save boundaries
and successful-SaveFinish-bound stage/commit/discard history receipts. Complete
SDK client/runtime/handlers/service, Bridge contract/codec/native, Project import
and owning Sandbox/API-core integration. Implement bounded fair authenticated
object/policy/serial/Save/history delivery, same-Save reads and exact disconnect/
restart fences. Preserve exact authority/reference closure, original typed
refused/conflicted/uncertain outcomes, one attempt and retained custody. No guessed
resend, Branch refresh/re-stage, automatic conflict discard or unfenced install.
Fix faithful initial full-root acquisition: include ignored files, dependencies,
caches, outputs, symlinks and .git/index; Git ignore rules do not filter state.
Resolve P12 import symlink/scan/job/4GiB-cap gaps in their owning implementation.
Repeated binding to an acquired root performs no whole-root scan/copy/materialization,
dependency restoration or new database. Demand loading pays actual metadata/content I/O.

S10–S13 are outside this batch. Keep P3/P6/P7/P13/P14 explicit later Commit
prerequisites, not silently resolved. Preserve root reference until authorized
retirement. Existing local commit and #307 evidence/checklist authorization
continues; pushes/releases/deployments are outside scope. No CI or aggregate
pre-push wrapper. The newly committed side documents/guides/historical packet and
generated image bundle are retained artifacts, not algorithms or new qualification.

Preserve unrelated/concurrent working-tree state and these four unrelated containers:
9cf2fe345496 layerfs-experiment-305-dev
ce75ac504df9 layerfs-4c8cde9ee1cf49f8049298b3377d4927
d2433851ea59 layerfs-dbd59ea75fdc62de0a1f6d9d099609ab
d2550144998b layerfs-76b116dc9d984679f5802e2ec9c0798d
Use only owned fresh native proof containers and dispose of them after bounded runs.

Build first with --no-run and --locked. Every actual test invocation has an explicit
wall timeout<=120s; owning native timestamp proofs keep their8s ceiling. No background
test or repeat loop. A timeout is FAILED requiring source/output diagnosis before
a justified rerun. Bounded waits and panic-safe thread ownership are required.
Product Bash Exec does not inherit test/measurement timeouts.
Follow one-attempt/no replay, third-party exception scope, disposable backing/no-sync,
ARM64 .cargo/config.toml flags and scoped measurement/cache/setup rules. Before every
measurement read its policies/report template; pin source/build/dependency/binary/
image/harness/workload/cache identities, use worktree-local locks/targets and fresh
append-only receipts, one sample per case/arm. Export LAYERFS_CONSTRUCTION_WORKERS=1.
Reuse closed prepared inputs through declared mechanisms; never credit setup/own-write
warmth, a lifetime peak or unobserved paging as a cold/phase-residency result.

Every commit records exact first-parent/final staged production LOC before/after/
signed delta, with separate core/reference/combined totals using unchanged
tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Include all first-party shipped source/SQL and application adapters, including
excluded predecessors; exclude tests/docs/tools/harnesses/manifests/builds/third-party.
Prepare comparison before commit and verify the committed tree against its receipt.
Baseline: core86379, reference65417, combined151796. Recompute actual snapshots;
do not treat this baseline as an estimate or a source-size/performance gate.

Maintain separate milestone audits and tracker receipts, with blockers and gaps
explicit. Persist through useful independent batch work. Stop after S7–S9 is
complete, or report precisely a hard required external gate that remains, the
independent work completed and remaining implementation, concrete next-ready work
and exact source/receipt/tree/LOC handoff. Never mark S8 complete solely because
the dependency patch or prerequisite smoke proofs pass.
```

## Current evidence and preserved history

- [S7 audit](S7-EXIT-AUDIT.md), [S8 audit](S8-EXIT-AUDIT.md),
  [S9 audit](S9-EXIT-AUDIT.md) and [progress](PROGRESS.md).
- [Fuser qualification](FUSER-REGISTRY-PATCH-20261006.md),
  [maintenance/provenance](../../../patches/fuser-0.18.0/README.md) and
  [source/tracker receipt](checks/fuser-registry-patch/tracker-receipt.json).
- [Owner-selected organization](SOURCE-ORGANIZATION-S7-S13.md),
  [applied organization](SOURCE-ORGANIZATION.md) and
  [informative analysis](WORKSPACE-EFFICIENCY-ANALYSIS.md).
- [Current primary contracts](../303/README.md) supersede
  [the historical #301 packet](../301/README.md).
- This all-state commit adds documents/assets only: core86379 ->86379(+0),
  reference65417 ->65417(+0), combined151796 ->151796(+0), verified from exact
  first-parent/staged snapshots with the unchanged counter. The commit message
  records the staged tree; final commit/tree correspondence is checked afterwards.
  Committing retained research/proposals/illustrations does not advance milestones
  or create new measured results. No old failure or proof is relabeled.

Docker-only verification and the retained platform investigation are recorded in
[the boundary clarification](LINUX-TIMESTAMP-DOCKER-20261006.md). This clarification
does not advance S0/S8 or authorize an unrecorded shared-backend change.
