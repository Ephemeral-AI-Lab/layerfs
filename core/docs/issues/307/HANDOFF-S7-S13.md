# After S5/S6: next ownership handoff

> **Status:** Dated planning checkpoint,2026-10-06. Continues the current303 primary
> contracts and [S4–S6 handoff](HANDOFF-S4-S6.md). It is not a release contract,
> qualification report or authorization to skip remaining milestones.

## Completed stopping boundary

S5 and S6 are complete in the primary checkout on local main. The S6 completion
commit is the commit containing this handoff; its exact source/tree/LOC and tracker
receipt identify it. [S5 audit](S5-EXIT-AUDIT.md) and [S6 audit](S6-EXIT-AUDIT.md)
map every scoped exit to implementation and retained evidence. No push, release,
deployment or legacy-root deletion has occurred. S0 and S7–S13 remain open.

| Local checkpoint | Concrete scope | Combined production LOC |
| --- | --- | ---: |
| a0dc7da9b | S5 payload/read/write and canonical zero runs |146209->147386(+1177)|
|32bd3bec0|S6 bounded namespace failure composition/live maintenance|147386->148227(+841)|
|1775fdf98|Owner-selected source organization,80 moves,public paths preserved|148227->148327(+100)|
|cae3d43ed|Independent regular-file/capture/operation custody|148327->150019(+1692)|
|be651a048|Relative name inheritance, non-file lookup/request custody|150019->150533(+514)|
|Completion commit containing this handoff|Physical reservations/accounting/wakes/resource closure|150533->151299(+766)|

Reference production remains65417; core subtotals are each combined total minus
65417. These include excluded predecessor product source and shipped SQL, exclude
all tests/inline tests/docs/tools/manifests/harnesses/builds, and use unchanged
counter SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Before every new commit archive the exact first parent and final staged tree,
compute core/reference/combined before/after/signed delta and confirm the committed
tree matches. `/tmp/layerfs-count-staged.py` is an ephemeral helper; the authoritative
counter is tools/production_loc.py and receipts are under core/target/cluster2-307/loc.

## Next ready work and open dependencies

| Milestone | Required remaining implementation/exit |
| --- | --- |
| S0 | Reconcile R/P algorithms, host/runtime authority and native build risks against actual S4–S6 implementations. Do not mark it complete from component proof. Fuser timestamp blocker below remains |
| S7 | Consolidate complete-operation request/statement/VM/row/page/BLOB/copy/queue/resource/debt costs, worst-case/amortized/cumulative bounds and real residency. Include two backed count updates per DML, one freelist-cookie admission query and Linux's one precise-range allocation call. Count full256MiB daemon reservation/high-water storage. No source or diagnostic count equality establishes cold speed or universal sustained service |
| S8 | Actual product FUSE and daemon/control/Exec: exact request/open/lookup lifetime mapping, deferred credited replies, coherent cached/writeback-off profile, mmap/time semantics, cheap FORGET, multi-call mounts, ordinary short/long Bash and terminal detach/join cleanup. No automatic Exec duration cap, implicit Commit or automatic unmount |
| S9 | Bounded authenticated logical demand/policy/serial/Save/history transport, same-Save reads, exact contextual authority and disconnect/restart fences, faithful complete initial roots including ignored files/dependencies/caches/outputs/symlinks. Preserve initial native KK/SDK public boundaries. Linux owning Persistence allocation/provider remains unavailable; compiler warning is recorded, not capability |
| S10 | Backed canonical file/namespace construction over captured-reader/operation scratch APIs, SaveFinish/StageChanges/conditional CommitStaged, known paired install and exact refused/conflicted/uncertain custody. Resolve P3/P6/P7/P13/P14 and the authenticated history resolver/fence. S6 local definite failure resolution requires caller proof/fencing; unknown history cannot invoke it |
| S11 | Remove superseded excluded core backing/construction/duplicate Init/server wiring only once real adapters replace it. Make actual members authoritative, preserve needed source accounting/evidence and no legacy dependency/fallback |
| S12 | Freeze integrated source/specs and owning platform proofs for complete roots, frequent fresh mounts and sustained same-mount calls/Commits, short/long Exec, concurrent service and cleanup. Report all seven selected families with one sample per arm/case, preserved failures/unrun rows and only qualifying unchanged-evidence reuse. FAIL/NOT_RUN does not complete a required gate |
| S13 | After integrated qualification, retire root crates/obsolete wiring; preserve shared ARM64 config, historical evidence/baselines and exact reference/core/combined retirement LOC. No early deletion |

P3 remains specifically `core/crates/layerfs-content/src/file/edit/tree.rs`:
EDIT_DEFERRED_LIMIT protects resident deferred maps. Removing only the refusal
would make state unbounded. S5 explicitly carried it to S10; it is not resolved.
P4's public `construct_runs`/FileRuns zero reuse is implemented and proves canonical
root equality against streaming. P6 resident DirectoryUpdate changes, P7 new-parent
membership, P13 demanded/touched/validation/release state and P14 whole-base topology
walks require backed bounded inputs. P12 native Init symlinks/raw names/scan state/
inherited file cap and P1/P10 authority/history completion remain open.

## Current implementation boundaries

Schema14 has namespace/global accounting, orphan source waits and bounded generation
wakes. One private overlay database/descriptor belongs to the daemon. Ordinary
admission has128MiB growth plus128MiB cleanup capacity, offset by committed freelist
pages; cleanup needs its own128MiB class. Linux qualifies ext4 device-full admission
and last-owner idle cleanup. macOS qualifies allocation/functional behavior here,
without device-full/COW qualification. The exclusive fresh dense backing assumption
excludes independent clone/punch/truncate/mutation, snapshots/COW interference and
device failure. Disposable state has no sync/crash recovery promise.

Namespace failure composition stays at two live domains, with next capture waiting
while ordinary writes continue. Orphans have one independent gen=-1 domain plus
at most two fixed retained lower sources, never a new source per later Commit.
Snapshot owners park migration; compatible physical rows move without payload
copy. Indexed bounded maintenance handles source wakes, stale cells, abandoned
steps, redundant whiteouts, retired generations, processing scratch and terminal
cleanup. Last owner release queues rather than sweeps. One failed attempted
maintenance job retains exact error/cursor and stops without automatic replay.
Raw cell APIs remain stored observations; composed APIs define visibility.

Source navigation: overlay database/namespace/payload/lifetime/maintenance/diagnostics;
Workspace base/workspace/mutation/operations/ports; daemon overlay owner/queue/credits/
commands/read_port/file_port. The current SOURCE-ORGANIZATION.md guide and architecture
19–35 describe boundaries. The untracked SOURCE-ORGANIZATION-S7-S13.md and
WORKSPACE-EFFICIENCY-ANALYSIS.md are unrelated proposal/research, not a new assignment
or algorithm selection. Preserve them.

## Binding execution/evidence rules

Read root/core AGENTS.md, both root handbooks,303 index, relevant operation/engine/
FUSE/runtime contracts and current policies before implementation. The optimization
investigation remains research. Do not reopen closed Stage0–6 campaigns/prompts.
Local commits and307 comments/checklist updates remain authorized; pushes/releases/
deployments are not. Stage only named owned paths. Preserve unrelated docs/README.md,
core/docs/issues/301, the three docs/general guides, extent-normalization research,
output and the two307 side documents. Four unrelated containers remain untouched.

Every test gets an explicit wall timeout<=120s and a no-run build first. Select by
package/test; never background it, repeat unchanged commands in a shell loop or
leave a timeout hanging. Timeout is FAILED; diagnose bounded output/source before
rerun. Tests have bounded waits and panic-safe thread ownership. Ordinary Bash
product calls do not inherit test timeouts.

No failed-operation retry/busy handler/reprepare/refresh/replay, guessed cleanup,
third-party patch/fork/vendor/version replacement, new dependency when existing
capability suffices, inline production test hooks, custom mutable namespace mirrors,
aggregate pre-push wrapper or CI claim. Keep product files<=999 lines and lib/mod
<=200 thin declaration lines. Root Cargo ARM64 flags remain binding.

Before actual measurement, read measurement workflow/benchmark rules/report template
and owning family contract. Freeze specifications, exact sealed source/build/image/
harness/workload/cache identities, use worktree-local locks/targets, clone closed
prepared setup and pay each phase's cache work independently. One sample per arm/
case, fresh append-only outputs, no unchanged treatment resampling or workload/
budget/cache changes to obtain PASS. Default performance command<=15s, declared
exceptions<=25s; independent proof<10s. Export LAYERFS_CONSTRUCTION_WORKERS=1.
These S6 resource tests have uncontrolled caches and no speed/RSS eligibility.

Retained fuser0.18.0 converts signed fractional timestamps before LayerFS callback
and panics at i64::MIN in the tested debug build. Its original forced-abort137
receipt remains FAIL. Owner forbids third-party substitution/patch or timestamp
contract reduction. S0/S8/S12 require an allowed corrected published package and
actual native requalification; see FUSE-TIME-BLOCKER-20261005.md.

Continue from reconciled tree/tracker and exact audit scope. Checkpoints use the
existing Milestone/State/Source/Delivered/Validation/SQLite/Resources/LOC/Remaining/
Next template. Never infer success or release authority from absence, lost reply,
Bash exit, research, a scaffold, a document or a complete report containing failures.

The S6 completion comparison is core85116->85882(+766), reference65417->65417(0),
combined150533->151299(+766), exact staged-parent archives with the counter above.
The six implementation/organization commits since S4 total+5090; organization is
reported separately and no reference retirement or algorithmic shrink is claimed.
