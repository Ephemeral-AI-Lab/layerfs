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
retry: a stopped owner, a charge that no released credit could cover, or a
full table of admission registrations returns the original command
unattempted. It has no bound of its own, and waiters are woken together with
no queue order, so sustained traffic can delay a Commit without limit.

The Commit thread keeps completions of its own while it waits: the capture's
(one ordinary credit) throughout, and on a failure the original completion of
a refused acquisition or port call (one Lifecycle or ordinary credit).
`commit_captured` therefore refuses before any effect, as a settled
`Context` failure, when the owner was started with fewer than two ordinary or
two Lifecycle job slots per Workspace: its own wait could never end. After a
locally settled failure the done resolution's completion is dropped before
the releases, because with the default two Lifecycle slots it and a refused
acquisition's completion would otherwise hold both; `locally_settled` records
the fact and `CommitFailure::local` is then `None`.

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
| Uncertain cause, known publication with failed install, or failed local resolution | Kept. `CommitFailure::namespace` names both. When construction itself failed it also holds the attempt's record and file custody, with the first cause moved out into `CommitFailure::error` |

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
Commit and normal unmount are refused as for any retained custody. A settled
failure whose reader or owner was not released is recorded `Uncertain` and
answered with code `Unknown`, with the original cause in the detail: the
nonpublication is known, the custody is not ended.

A guarded record change that the Overlay answered `NotApplied` is a definite
nonapplication, yet its completion carries an `Ok` reply and is classified
with the unknown outcomes: the producer's own records were not what it had
written, and both owners are kept for inspection instead of being released.

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
- A queued install holds back later source acquisitions. Since R6 those
  held-back jobs are counted in the Workspace's own Source slots, so they
  leave the ordinary slots to the request whose source the install waits
  for; see [owner slot accounting](21-daemon-owner.md#r6-slot-accounting-source-acquisitions-have-their-own-bound).
  Proven at owner scope; the mounted interleaving is not staged there.
- `CommitSuccess` holds the capture and install completions, and a
  `CommitFailure` the capture's and any original failed completion, each one
  credit, until dropped. On the driver's own route (`Service::execute`) a
  failure also keeps the resolution's.
- Admission waits are unordered and unbounded; fairness between a waiting
  Commit and filesystem requests is R6 work.
- An unknown outcome has no resolver. Its custody stays until an owner ruling
  or forced teardown.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.
