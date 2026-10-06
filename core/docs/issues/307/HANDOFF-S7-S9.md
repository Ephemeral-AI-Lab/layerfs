# S7–S9 continuation prompt

> **Status:** Current planning checklist; no release candidate exists.
> Updated 2026-10-06 with S7 startup accounting, typed S9 service and native-import
> path/link acquisition. S7/S8/S9 remain incomplete.

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
  344d992a8a31c89b25f76fce713bab8b148a1a29.
- All-state documents/assets checkpoint: ba9304e4960a229de908f9ff3f19edcbc6285db2,
  tree5e3e271a3ace6c42e0f1392e9c1d4a9a67186278. The subsequent Docker acceptance
  update is b9f3a8dc9ffcf78c9e625c86be8b4bb2f661ae6c, tree
  2a55aa86c2d0ca88c5a6b2059d8f506092dc7ab5. The new S7/S9 implementation
  checkpoint is ec109e3969bb867281f8273940cbe9196a608750, tree
  6f6cab43bdb3e277cdeb70f35f99f36de52bc66c. Availability/submission attribution
  source is7a12b3f3a453b7ef841a777ba0cb3c8c29e6c3d9, tree
  2a9235a7872ba9c375356915e09a67c04c7d6201. The final receipt commit follows
  it without production changes; see exact source/LOC/tracker receipts under
  checks/s7-startup-s9-service and actual current HEAD.
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
observations. New create_observed/start_observed receipts also include finite
profile/schema/accounting SQL, startup failure and separate allocation observation.
OwnerStart::creation_reported marks whether actual worker creation work arrived;
pre-receipt worker costs are unavailable rather than observed zero. Logical
column delivery and supplied SQL bytes are explicit; approximate statement
memory samples are not cumulative allocation, pager/RSS or phase peaks. Extend the
owning overlay diagnostics/database and daemon service
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
The owner's latest acceptance is "docker verification is enough". Those existing
Docker proofs are sufficient for verifying the fuser correction. No QEMU or
custom-kernel campaign is a prerequisite for this dependency fix or for continuing
the batch. S8 product implementation and mounted product acceptance remain required.

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
or raw-wire product replacement is claimed. The owner's Docker-verification
acceptance records the endpoint case as a known Linux platform limitation, without
changing its FAIL verdict or claiming the exact fraction survived a native syscall.
Do not describe the fuser correction as still blocked by that kernel case. S8 is
incomplete because its remaining implementation and product qualification are open.
Continue useful independent S7/S8/S9 implementation; the old wait-for-fuser-release
ruling must not stop the entire batch or cause repeated unchanged timestamp probes.
The owner selected Docker and accepted its existing verification as sufficient.
Retain the exact remaining platform outcome: a Docker
container shares its backend's kernel, and rebuilding an image does not change
the VFS rule before the FUSE callback. Preserve the four unrelated containers and
the shared backend. No Linux correction or alternative backend was installed in
this checkpoint. A kernel-source/build-tools investigation ran no new native
test and its failed Docker tool-image build is retained. Do not restart that kernel
campaign as a prerequisite for the accepted fuser fix. Any future platform work
needs its own scope/evidence; do not guess the removed fraction or relabel the FAIL.

S9 — complete authenticated host adapters and full roots:
The active SDK has local authenticated binding, object/policy/serial/Save boundaries
and successful-SaveFinish-bound stage/commit/discard history receipts. Typed
runtime/service now provides fair Workspace/class rotation, same-Save ordering,
retained byte/job/receipt credits and local disconnect/owner-epoch fences. Preserve
original queued cancellation bodies and completed outcomes. Those local fences do
not establish socket/process-restart custody or unknown resolution. Per-class
submission attempts/refusals include errors before credit admission, separate
from dispatched adapter failures and the credit-window refusal counter. Complete
SDK client/runtime/handlers/service, Bridge contract/codec/native, Project import
and owning Sandbox/API-core integration. Implement bounded fair authenticated
object/policy/serial/Save/history delivery, same-Save reads and exact disconnect/
restart fences. Preserve exact authority/reference closure, original typed
refused/conflicted/uncertain outcomes, one attempt and retained custody. No guessed
resend, Branch refresh/re-stage, automatic conflict discard or unfenced install.
Fix faithful initial full-root acquisition: include ignored files, dependencies,
caches, outputs, symlinks and .git/index; Git ignore rules do not filter state.
Project native Init now preserves opaque symlink targets without traversal, uses
the existing canonical0777 symlink mode and removes inherited4GiB rejection.
Saved-root proofs cover ignored/dependency/cache/output/.git/index paths and exact
targets after deleting native source. Resolve remaining P12 backed scan/job/frontier/
child/namespace collections and regular hard-link identity in their owners;
greater-than4GiB native streaming proof remains NOT_RUN. No whole-importer bound
or complete P12 acceptance follows from the small membership fixture.
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
Prior baseline: core86379, reference65417, combined151796 before this new
implementation checkpoint. Use its exact LOC receipt/commit message and actual
HEAD for updated totals. Recompute actual snapshots;
do not treat this baseline as an estimate or a source-size/performance gate.

Maintain separate milestone audits and tracker receipts, with blockers and gaps
explicit. Persist through useful independent batch work. Stop after S7–S9 is
complete, or report precisely a hard required external gate that remains, the
independent work completed and remaining implementation, concrete next-ready work
and exact source/receipt/tree/LOC handoff. Do not stop on the accepted fuser
dependency verification or demand another verification platform. Never mark S8
complete solely because the dependency patch or prerequisite smoke proofs pass.
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

The latest S7/S9 checkpoint is described in [startup/typed-service/import checks](checks/s7-startup-s9-service/README.md).
It continues implementation with S7/S9 unchecked; no external fuser verification
blocker remains. Concrete next work is bounded logical authenticated runtime/client
transport and its real disconnect/restart custody, backed faithful Init (including
hard-link aliases), and complete engine page/I/O/residency/service-debt acceptance.
P3/P6/P7/P13/P14 remain S10 prerequisites. Unaffected S6/device and earlier handler
proofs retain their original identity; all new failure/unrun rows are preserved.

## This implementation checkpoint's exact source-size comparison

First parent b9f3a8dc9ffcf78c9e625c86be8b4bb2f661ae6c to final staged product:
core86379 ->87552 (delta+1173), reference65417 ->65417 (delta+0),
combined151796 ->152969 (delta+1173). Counted from exact Git archives using
unchanged tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
New service/startup/source modules are first-party production; external tests,
receipts/docs/tools/examples/third-party remain excluded. Existing moved creation
code and all excluded predecessors stay in scope. No reference retirement,
algorithmic shrink or performance inference is claimed. Final staged tree and
committed-tree correspondence are recorded in the commit and following receipt.


Follow-up attribution source comparison (first parent ec109e3969bb867281f8273940cbe9196a608750):
core87552 ->87578(delta+26), reference65417 ->65417(delta+0),
combined152969 ->152995(delta+26). Same unchanged counter and exact first-parent/
final staged archives, verified again after staging documentation. This growth
adds explicit receipt availability and fixed submission observations. The prior
+1173 source checkpoint remains separately recorded; combined batch growth is+1199.


## Exact latest implementation and remaining work

Production source:7a12b3f3a453b7ef841a777ba0cb3c8c29e6c3d9, tree
2a9235a7872ba9c375356915e09a67c04c7d6201. First implementation checkpoint:
ec109e3969bb867281f8273940cbe9196a608750, tree
6f6cab43bdb3e277cdeb70f35f99f36de52bc66c. Exact source/LOC/committed-tree verification
is retained in [receipt directory](checks/s7-startup-s9-service/README.md).
The following documents/receipt-only commit has core87578 ->87578(+0),
reference65417 ->65417(+0), combined152995 ->152995(+0), prepared from exact
first-parent/final staged snapshots and verified against the committed tree.
No Rust check is repeated for that metadata-only change.

Separate [S7 source receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6008261712)
and [S9 source receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6008268044)
are followed by availability/submission attribution receipts. Tracker S7/S8/S9
checkboxes are verified unchanged/unchecked. Runtime context/transport/restart,
backed import/hard-link identity and complete S7 resource acceptance remain open.
These are unfinished implementation/qualification requirements; the accepted
fuser correction is not an external wait. Greater-than4GiB native stream and all
cold speed/RSS/sustained-rate selections remain NOT_RUN/ineligible as recorded.
Keep S10–S13 and P3/P6/P7/P13/P14 outside this batch unless the owner changes scope.

## Active continuation: logical runtime wire (2026-10-06)

The next checkpoint adds real Bridge contract/codec/native framing and SDK client/
handler/input/output ownership, original Binding delivery, pre-body authority/Save
admission, demand/control service-slot protection and exact typed retained receipts.
Architecture40 and checks/s9-runtime-wire pin source/build/workload/cache/binary
identities and retain every failure/repair. Host37 and Docker19 bodies cover the
final custody source; global Store/provider remains macOS. New copy/crypto/I/O/
queue gauges do not close page/journal/RSS/cold/sustained gates.

This is active work, not a completed S7–S9 stopping boundary. S7/S8/S9 remain
unchecked. Next independent implementation is daemon-facing consumer adapters and
owning application control/connection/custody wiring, then backed complete native
acquisition and regular hard-link identity using the existing canonical public
streamed directory/table constructors. Do not treat its resident scan/input gaps
as an external fuser wait. Preserve the completed S5/S6 stopping record, root
reference, both side documents and four unrelated containers. Continue useful
independent work; S10–S13/P3/P6/P7/P13/P14 remain outside this batch.

The implementation commit records exact parent/staged/committed core/reference/
combined production LOC with unchanged tools/production_loc.py. A subsequent
receipt-only record pins its actual commit/tree and tracker comments without
repeating unchanged Rust checks or changing milestone verdicts.

Runtime wire checkpoint production LOC: core87578 ->91262(delta+3684), reference
65417 ->65417(delta0), combined152995 ->156679(delta+3684). Exact first-parent/
final staged archives use unchanged counter SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
New runtime adapters/framing are implementation growth; excluded predecessors and
root reference are counted, with no retirement/relocation or shrink claim. The
final prepared-tree receipt is in core/target/cluster2-307/loc and must match the
committed tree before its post-commit evidence record is adopted.

Verified implementation identity: commit1b2580f2ef5b6d29c577237a9757c7b86d3898fb,
tree9d941a9c4bac9268f5a818452fbffb24f28c36d2, first parent
f614e7d22d1764704132687abd17aaab74c33904. The committed tree and every production
fingerprint match the exact staged/build receipts. Separate appended tracker
[S7 receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009305142)
and [S9 receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009317243)
record CHECKPOINT, with S7/S8/S9 unchecked. This receipt-only commit has unchanged
core91262/reference65417/combined156679 production LOC, delta0, using its own
exact parent/staged comparison and committed-tree confirmation.

Native regular alias correction follows the wire checkpoint: the importer keeps
one inode per native device/inode, constructs once, distinguishes equal-byte copies
and derives in-root link count independently of outside links. Exact mode/ctime
observations accompany identity/length/mtime. Host66 and Docker3 acquisition/scaling
checks pass; existing resident native/input collections are still open. Architecture41
and checks/s9-native-aliases retain source/binary/cache/failure identities. This does
not complete S9 or create a new external fuser wait. Continue consumer/application
custody and backed initial acquisition using public streamed directory/table APIs.

Native regular alias checkpoint production LOC: core91262 ->91342(delta+80),
reference65417 ->65417(delta0), combined156679 ->156759(delta+80). The exact
first parent/final staged archives use unchanged tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb, counting product/
shipped SQL/excluded predecessors and excluding tests/docs/tools/manifests/third-party.
No reference retirement, new resident alias map or algorithmic shrink is claimed.

Verified regular-alias implementation: commit551f165ed332955067b30ce184ca3578f4e13f05,
treeca31b91c7793b3ff8a4936ae66f85a3ce6355452, first parent
32f5073593ea60f27a85db5ab4ecdb2ea4a1dd1b. Its committed tree/production hashes
match prepared/build receipts. [S9 alias receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-6009500696)
retains CHECKPOINT and all gaps; no checkbox completion. Following metadata-only
receipt comparison is core91342/reference65417/combined156759, delta0, with its
own exact first-parent/staged/committed confirmation. No source check is repeated.
