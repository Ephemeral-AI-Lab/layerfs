# Narrow fuser receive-loop lifecycle proposal for owner decision

> **Status:** Proposal; target LayerFS0.1.7; not a released contract.
> Not applied, compiled or qualified. Requires authorization beyond time.rs.

## Requested boundary

Keep fuser exactly0.18.0, the signed-timestamp correction, shared descriptor,
two receive loops and all selected kernel flags. Add a public lifecycle owner
for a Session constructed from an externally owned fd. It must expose facts
about actual receive-loop execution and retain their original outcomes. An
outer thread-start notification alone is insufficient.

The API needs these concrete operations/facts; exact Rust names remain subject
to implementation review:

1. Start the configured loops with a fixed startup rendezvous **inside each
   loop**, after its receive buffer is allocated and before its first receive.
   The rendezvous depends only on startup owners, never a kernel request.
2. Retain the configured and actually created loop identities/handles, including
   on a later spawn failure. Return original startup failure with that owner;
   do not detach created threads by dropping their JoinHandles.
3. Expose a waitable startup result and monotonic serving/terminal state. The
   serving transition requires all configured loops at the rendezvous. Release
   them under the same startup ordering. Entry and exit notifications originate
   inside the actual loop lifetime, not a filesystem callback.
4. Any loop exit/panic records its original outcome and revokes serving state.
   First-party control reads that state when composing Ready. State/notification
   ordering must prevent a lost exit or a Ready transition after known failure.
5. Keep one explicit join/dispose operation that consumes all created handles
   and retains every original outcome. A panic in one loop must not discard the
   remaining joins. If another loop still waits in read, its handle stays owned.
   The caller performs its separately authorized connection stop/detach; the
   dependency adds no automatic unmount, abort, retry or fallback.
6. A prior failed/join-consumed result remains recorded. A second call cannot
   turn missing handles into a successful drain receipt. Normal successful
   completion and failed/retained completion remain distinct.

A small fixed state per configured loop is sufficient. It adds no per-request
queue, handler scheduler, SQL integration or Workspace/daemon dependency to fuser.
First-party Fuse continues to own admission, request/reply custody, mount controls
and connection facts; daemon continues to compose overall Ready and terminal drain.

## Proposed change scope

- Existing `src/session.rs`: split loop startup/join ownership into the new
  explicit session-lifecycle API while preserving the legacy public run route.
- One focused session lifecycle implementation module, plus only the necessary
  declarations/reexports in `src/lib.rs`.
- External dependency regression tests for the new public API. No test hooks in
  LayerFS product source and no new dependency selected by this proposal.
- A new provenance entry records the unchanged official archive, timestamp patch
  and separately authorized lifecycle delta. Original timestamp receipts remain
  immutable. The integrity checker and its scoped tests accept only those exact
  owner-approved bytes; no broad allowlist or ignored file is proposed.

This is a reviewable interface/change proposal, **not a code patch**. Authorizing
it would permit implementation and verification, not establish that it works.

## Required verification before native use

Build first and run bounded public tests for two-loop startup with no ordinary
filesystem probe needed, exit-before-Ready, delayed loop startup, partial spawn
failure, a loop panic while its sibling is alive, original I/O failure after
complete joins, and repeat disposal preserving the original result. Where a
failure cannot be induced through a supported public external fixture, record
the limitation; do not invent a product hook or claim the row passed.

Verify unchanged handshake/profile/ordinary callback semantics, then the native
product FP-1/2/21 proofs. No lifecycle callback by itself establishes full daemon
drain: queued service, pending owner outcomes, Store consumers and control work
remain separate required barriers before native revocation/Close.

## Alternatives not qualified

One stat, finite shared-fd probes, callback thread IDs and proc metadata remain
insufficient under the existing handoff. Callback-installed TLS exit guards can
improve termination observation but cannot supply unobserved loop entry or joins.
Raw thread pidfd syscalls would require another safe-interface/policy decision
and still need loop-start association. Copying fuser's protocol engine into
LayerFS or using one loop would change the selected implementation/profile.
No such alternative is applied or presented as a substitute proof.
