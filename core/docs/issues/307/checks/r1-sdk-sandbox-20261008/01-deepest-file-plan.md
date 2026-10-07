# R1 deepest-file SDK and ordinary Sandbox plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Starts after R0 commit `eb9b06c4c8633053e9b1cbf5f545d141b88915fc`, 2026-10-08.

R1 is implemented in coherent subcheckpoints; none closes R1 until actual ordinary
Sandbox lifecycle/execution/access and daemon control route are externally proved.
Main owner alone edits/builds/tests/measures. Read-only reviews inspect actual
public organization and native startup/access seams; no subagent execution.

| Deepest source home | Reuse/change/new | Operation/custody/cost |
| --- | --- | --- |
| SDK `project/{api,init,install,history}.rs` | Move/reuse existing init/install; add before-effect explicit profile checks and facade/typed history delegation | ProjectApi native Init/seal/install and one authenticated fork/history call; no retained host Store/data service |
| SDK `control/{mod,connection}.rs` | Move existing control.rs unchanged | Sole Connection/correlation/quarantine owner, original ControlFailure preserved; no second exchange driver |
| SDK `workspace/{api,binding,commit,status,unmount}.rs` | New real typed facade over existing controls | Exact token/BranchSnapshot binding; Bound explicitly not native mount/Ready or invented directory. Typed known remote refusal and original received reply/failure custody |
| SDK `sandbox/{api,lifecycle,execution}.rs` | New with actual replacement Sandbox | Ordinary lifecycle/execution standard streams/status; optional Workspace exec later delegates real mounted cwd; no daemon command registration |
| Sandbox excluded source | Preserve/relocate unchanged before replacement | Old manifest/host API-core dependencies cannot activate. LOC records integration→predecessor classification, relocation delta0 |
| Sandbox `sandbox/{types,config,lifecycle,ready}.rs`, `backend/docker/{command,lifecycle,execution}.rs`, `access/{identity,mounts,protected_paths}.rs` | Actual replacement | Own exact Docker resources only, no automatic replay/cleanup after unknown create/result. Standard runtime streams with bounded/no total-output cap; explicit caller cancellation has actual runtime identity and exact disposition; root-shell/CLI exit never proves FS drain |
| Daemon thin filesystem executable + startup/control composition | New actual app owner, reuse open/install/Service/Owner | One direct shared Disposable Store + one local Overlay initialized once. No process supervisor/launcher. Retain failed installation/control custody; do not fake namespace Commit or Ready |
| Boundary guard and scoped guard tests | Change when real dependency/package activated | Explicit SDK→Sandbox edge, real Sandbox unsafe-free/dependency coverage; no silent exemption or weakened existing boundaries |
| SDK/native source architecture and rollout ledger | Change with implemented subcheckpoints | Source/compiled/exercised/native/qualification distinct; R1 remains IN_PROGRESS until full scope |

R1a first implements and exercises real ProjectApi and WorkspaceApi component
facades using existing APIs. It preserves free initialize/install and control
exports for current consumers. Plain native mount remains R2; component binding
returns Bound, never Ready or a fake location. It adds no dependency or Sandbox
placeholder. No second history/Store encoder or legacy include is introduced.

Independent R1a proof uses existing complete Init/seal/first-Branch oracle through
ProjectApi and affected authenticated control tests through typed facades,
including exact original refusal/unknown/correlation custody; no mock product hook.
Build with locked core Rust1.85.1 first, run selected external tests under100s
explicit stop, then scoped warning-denying Clippy/fmt/boundary and relevant guard
coverage. Store profile is explicitly Disposable, all construction workers1;
ARM64 flags inherited from root config. These are functional checks, no timing
admission or new numeric gate. Exact first-parent/staged/committed LOC and honest
relocation/growth totals accompany each commit. Preserve all protected notes,
containers/processes/worktrees/history and R0 original receipts.
