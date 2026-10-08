# Product Commit: the captured namespace through the control route

> **Status:** Implemented product wiring after `bb5b3c220` (R5). Functional
> proofs and their scope are in the
> [R5 record](../issues/307/R5-COMPLETION-20261009.md). No timing, cold-cache,
> storage or resident-memory claim.

A control `Commit` now captures the Workspace's shared published frontier and
constructs it with the captured namespace producer. The
[Store Commit driver](65-store-commit-composition.md) and the
[producer](78-captured-namespace-construction.md) are unchanged; this document
describes only what joins them.

## Route

| Step | Owner |
| --- | --- |
| `Request::Commit(token)` on an authenticated control connection | [`control/operations.rs`](../../crates/layerfs-daemon/src/control/operations.rs) `execute_control` |
| Registry admission to `Committing`, refusal of retained custody | [`control/registry.rs`](../../crates/layerfs-daemon/src/control/registry.rs) |
| Capture, begin Save, finish, validate, publish, install | [`store/commit.rs`](../../crates/layerfs-daemon/src/store/commit.rs), unchanged |
| The constructor closure, classification and release | [`store/captured.rs`](../../crates/layerfs-daemon/src/store/captured.rs) `BoundWorkspace::commit_captured` |

`Service::execute` and `serve_one` still accept a caller's constructor. The
application calls only `execute_control`.

Commit runs on the control connection's own thread. It holds one control slot
for its duration. It never runs on the SQL owner thread or on a Fuse receive or
request-service worker; each captured port call is one short owner job. There
is one construction producer by structure: one Storage producer, one Save, one
closure, and the Workspace's `committing` flag. No construction thread is
spawned.

Every owner job of the Commit thread — Capture, the reader and owner
acquisitions, each captured port call of the producer, the local resolution,
the install and the releases — waits for a credit before its one attempt
(`OwnerClient::submit_waiting`, [`overlay/admission.rs`](../../crates/layerfs-daemon/src/overlay/admission.rs)),
as a filesystem request does. A Workspace busy with other callers delays a
Commit; it does not refuse it. The wait is readiness before the attempt, not a
retry: a stopped owner, or a charge that no released credit could cover,
returns the original command unattempted. It has no bound of its own.

Commit does not consult the native mount state. A Bound Workspace and a
mounted one commit the same way, and the frontier is whatever is locally
published, whichever process published it and whatever that process's exit
status was. Commit forces no kernel writeback: a store through a shared mapping
is in the frontier once the kernel has written its page back.

## The constructor

Inside the driver's closure, in order:

1. One `StoreOperation` for the Workspace.
2. The request number is the capture's generation. Only Commit acquires
   captured readers and operation owners, one capture at a time, so it is
   unique among their live rows. A generation that is not positive is refused.
3. `AcquireCapturedReader`, then `AcquireOperation`. Each acquired owner is
   recorded before any later step can fail.
4. One `CapturedNamespace::construct_untimed` into the Save's sink, with
   `store.policy().construction()` and the Save as authenticated objects. The
   untimed form lives in Workspace, which owns the telemetry dependency.
5. The first original cause is taken from the custody slot that holds it and
   handed to the driver as what it was:

| Original cause | `CommitError` |
| --- | --- |
| An owner job's completion | `Completion` |
| An owner refusal that never admitted the job | `Owner` |
| The Store provider's typed failure | `Content { provider: Some }` |
| A Content error | `Content`, with the operation's retained provider failure if any |
| Any other Workspace error | `Workspace` |
| A label with no retained cause | `Content`, with the retained provider failure if any |

A result that arrives together with a retained cause is treated as a failure.
Pre-admission refusals that arrive as an opaque service error stay uncertain.

## Release

Nothing is released inside the closure. When the driver returns:

| Driver outcome | Reader and operation owner |
| --- | --- |
| Committed or UpToDate, with known install | Released: reader, then owner |
| Definite failure that the driver resolved locally | Released the same way |
| Uncertain cause, known publication with failed install, or failed local resolution | Kept. `CommitFailure::namespace` names both, with the attempt's record and file custody |

The attempt's custody is dropped before the first release. A release that is
refused or fails ends the sequence; the owner it did not release stays named
in the receipt beside the original completion or submission error. Nothing is
retried. The completion of a release that is done is dropped at once and only
its fact is recorded (`released`): a held completion keeps a Lifecycle credit,
and a Workspace has two.

`CommitSuccess::namespace` and `CommitFailure::namespace` carry
[`CapturedConstruction`](../../crates/layerfs-daemon/src/store/commit_types.rs):
the producer's counted work, Content's filesystem counters, which owners were
released, and whatever was kept. It is `None` when the closure never
ran, and under a caller's constructor.

A known Commit whose reader or owner was not released is still published and
installed. The registry records `LocalFailure` with the publication, the reply
is the existing "known result followed by a local failure" form, and later
Commit and normal unmount are refused as for any retained custody.

## Wire

Unchanged: `Reply::Committed` with `Committed` or `UpToDate`, or a refusal with
code, phase, optional publication and detail. Commit refusal detail is now
bounded to the encoder's 2,048 bytes; the original stays in daemon custody.

## Install on a live mount

No kernel notification is sent and `layerfs-fuse` gained no install rule. The
contract permits that only when names, bytes, links, attributes, serials and
link counts are identical before and after. That holds by construction: the
root is built from exactly the captured rows, fresh serials are declared, and
the driver refuses a candidate that changes root inode identity. No runtime
comparison of the whole view is made, because it would be whole-namespace
work. Kernel node ids (`serial + 1`) and the mount generation do not change at
install. Open-file, lookup, orphan and captured-reader owners do not pin a base
root. Per-request sources do: install waits for them, and a queued install
holds back later source acquisitions until it has run.

## Limits

- `FilesystemResources` are Content's defaults; no Store-derived source exists.
- A request that stayed retained with its source would park install after a
  known publication.
- A queued install holds back later source acquisitions, and those parked
  jobs still occupy ordinary slots of the Workspace. Sixteen concurrent
  requests can then leave no slot for the request whose source the install
  waits for. Not fixed here; it is R6 work.
- `CommitSuccess` and an unresolved `CommitFailure` still hold the capture
  and install or resolution completions, each one credit, until dropped.
- An unknown outcome has no resolver. Its custody stays until an owner ruling
  or forced teardown.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.
