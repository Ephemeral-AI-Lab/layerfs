# Project and Workspace SDK component facades

> **Status:** Implemented source during R1a after `eb9b06c4c`; scoped verification
> is retained under [R1 receipts](../issues/307/checks/r1-sdk-sandbox-20261008/).
> Actual [SandboxApi/runtime lifecycle](72-owned-sandbox-lifecycle.md) is now implemented
> with scoped R1d evidence. Native Ready/mounted Commit and full R1 qualification
> remain open. No integrated acceptance or performance is claimed.

SDK `project/` owns existing complete native Init/seal and bounded authenticated
sealed install, with a stateless ProjectApi forwarding those operations. New
application Init requires explicit Disposable, and install checks both sealed Store
and manifest profile metadata before file/channel effects;
no default Durable selection is inherited. Original requests/output, known
publication, failed exchange/fence and typed provider causes remain unchanged.
The host retains no installed Store or data runtime. Existing free-function and
root type exports remain for current qualified callers.

ProjectApi fork/history borrows caller-owned Control and sends one existing typed
History operation. WorkspaceApi borrows the same sequential Control for logical
bind/Commit/status/terminal control. Concurrent callers retain separate channels.
Control remains the sole Connection/correlation/quarantine/reply-validation owner,
relocated under `control/connection.rs`; no facade adds exchange, reconnect, retry,
Branch refresh, command tracking or unknown resolution.

BoundWorkspace contains exact WorkspaceToken and BranchSnapshot from Reply::Bound.
It has no mounted location, native Ready or invented directory. Native mount/Ready
must come from actual R2 attachment and its validated original receipt. Current
pre-S8 unmount establishes logical Close only; the facade cannot add a native
teardown claim. Commit forwards the daemon's actual constructor route; current
component proof uses direct Content, not a live namespace normalizer.

OperationFailure owns the exact selected Request and either original boxed
ControlFailure, original typed ControlRefusal, or unexpected original Reply.
Transport uncertainty and correlated remote refusal stay distinct; the latter
can leave a healthy channel for a later explicitly requested operation. Lost
replies never authorize automatic rebind or re-execution.

Current tests exercise ProjectApi Init against the existing complete namespace/
byte/first-Branch/sealed-file oracle and Workspace/Project controls against an
actually installed Store and real fair Overlay owner over authenticated channels.
They retain exact overwrite/captured-parent/current-root UpToDate, anchored history,
Busy, stale token, unknown and publication/install custody scope. There is no
native FUSE/kernel or ordinary Sandbox execution coverage in this subcheckpoint.
That R1a subcheckpoint added no Sandbox placeholder/member/dependency. R1d now
adds the actual one-way SDK-to-Sandbox edge and facade over owned runtime source;
its narrower control/access proof does not imply full R1 completion.
