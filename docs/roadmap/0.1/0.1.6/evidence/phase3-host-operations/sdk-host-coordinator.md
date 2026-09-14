# Host SDK coordinator custody

The new `host_sdk.rs` composes the existing HostOverlay mutation, exact installed
Snapshot, HostOperations read reservation and versioned control messages. It is
one owner per HostRuntime, rather than one owner per public call. The scope
counter therefore remains monotonic across calls and retries.

| State | Retained ownership | Retry / recovery action |
| --- | --- | --- |
| Before BEGIN | target pin, memory permit, optional pre-admitted reader | Same input retries; recovery can finish local cleanup |
| BEGIN uncertain | same scope and target, digest of exact input, bounded mask, reader reservation | Same input retries BEGIN; recovery CANCELs the exact scope |
| Installed, reader activation failed | exact installed root and unchanged reservation | Activate that root; never reinstall edit |
| APPLY uncertain | immutable file lease, exact control frame and scope, target pin | Retry same APPLY; never resolve current path or rebuild live content |
| Control completed, unpin failed | known applied/unchanged result and pin | Retry unpin only |

The streaming input digest includes workspace identity, path, order, ranges,
replacement classes, lengths and all inline bytes. It does not retain the caller's
edit vector or payload copies. A different input while pending returns Busy.
After a completed operation, the same edit is a legitimate new operation.

The host reserves mask/frame memory before target pinning and read-lease capacity
before BEGIN or payload mutation. Its mask union is bounded by the existing
control frame size. Exact equal-length replacement intervals are unioned; an
unequal replacement includes its shifted suffix as in the existing SDK cache
contract. These masks do not cause eager reads or copying of that suffix.

CANCEL of a newer absent scope now records its cancellation watermark. This
resolves the case where BEGIN's request or reply was lost and prevents a delayed
BEGIN with that identity. An active different scope and a scope with an installed
APPLY identity remain ineligible for cancellation.

Local unmounted and actual mounted coherence must be selected explicitly by the
runtime owner. Missing notifier/backing state on a mounted path cannot become a
successful no-op. End/Discard must settle SDK recovery before destroying the
runtime or transport; an uncertain APPLY still owns its reader and scope.

This is an ordinary SDK integration component. It does not resolve Commit V1,
claim complete writable-mapping snapshot visibility, execute a benchmark, or
change any #122 exclusion.

The preinstall cancellation decision was also checked against the shared root
installation contract: `Overlay::install` has no fallible operation after
replacing the current Arc, and `HostOverlay::mutate` returns its captured output
only after installation succeeds. A returned edit error therefore authorizes
preinstall CANCEL. Lease activation is deliberately a separate retry phase so
its failure cannot be mistaken for failure to install the SDK edit.

`after_detach` is a separate lifecycle entrypoint, allowed only after the caller
has verified exit of the mounted consumer and its control worker. It permanently
retires this SDK owner. It directly releases an installed lease (including APPLY
never delivered to the daemon), drops activation reservations/snapshots, and
unpins the target. A failed lease release retains the original phase; a completed
release is recorded before retryable unpin. No BEGIN/CANCEL/APPLY is sent from
this path. Independent snapshots retain their own ownership after this cleanup.

## Bounded-stop handoff

Component verification is in `adapter-ledger.md`. Current host SDK path guard
reproducer is resolved without changing path semantics: the existing canonical
validator rejects invalid paths before scope admission; the original FAIL and
focused successful rerun are retained. The observed normal-unmount busy failure
was traced to the client-owned preopened mount descriptor and repaired; the
native ownership proof passed, and the corrected Linux helper identity is in
`sdk-shutdown-linux-attempt02-result.json`. Root owns the decisive mounted rerun.

Remaining integration obligations are not discharged by these component passes:

- Generic Commit V1 still lacks an accepted, implemented dirty writable-mapping
  acquisition mechanism. The actual mounted ordinary SDK check supplies an owned
  host input to construction; it is not proof of full kernel-visible Commit input.
- The broader public Workspace runtime migration and obsolete non-Commit consumer
  removal remain outside this bounded stop. Existing explicit startup choices
  cannot become a request-error fallback between HostClient and LiveOwner.
- Normal End must resolve SDK coherence before asking an active helper to unmount.
  `prepare_shutdown` closes the preopened mount descriptor but does not cancel an
  installed SDK scope, declare its worker drained, or replace `recover_sdk`.
  In-flight syncfs work owns its own Arc until completion.
- `HostSdk::after_detach` is callable only after verified mounted-consumer/control
  retirement. It handles reservations or installed leases not delivered to the
  daemon; calling it before that proof could release a still-used reader.
  Retry errors keep the pending owner; a failed control result is not completion.
- Callback tests cover lost BEGIN/APPLY/CANCEL results. A complete production
  transport failure/reconnection and interrupted End/Discard acceptance set is
  still required before claiming the final supported surface passes.

No benchmark or #122 scenario was run by this SDK work. No release, tag, phase
completion, issue closure or terminal feature PASS is asserted here.
