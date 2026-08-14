# M8 closeout handoff specification

Status: blocked, implementation candidate only

This document is the handoff contract for closing Milestone 8 across the two
approved worktrees. It does not authorize changes to the original dirty
repository:

`C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-fs`

## 1. Starting state and invariants

Work only in:

- `C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-fs-m7-audit`
- `C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-computer`

Current implementation candidates:

- FS: `9607fffa4fd374301efb68907df7fe0acef52808`
- Computer: `6a1774e01c15542272f3fbf836f1086c6576350b`

The accepted M7 predecessor and its evidence topology are authoritative. Keep
`validate:accepted` pointing at M7 until every M8 gate passes. Do not reset,
rebase, discard, or overwrite the approved M8 planning documentation. Do not
create an M8 evidence or acceptance commit while any gate is missing or
blocked.

The implementation must preserve all accepted M0-M7 behavior, limits,
authentication, restart semantics, branch isolation, fault positions, and
evidence requirements.

## 2. Hard blockers to resolve first

The following are acceptance blockers, not optional test additions.

### 2.1 Core-owned bounded export capture

Implement a durable, core-owned export snapshot operation:

- Create and maintain an outbound export lease for the selected main revision
  or branch generation, including owner nonce, expiry, protected roots, and
  cleanup state.
- Capture branch rows through durable keyset pages, not `OFFSET` scans or one
  capture transaction. Persist the page cursor and snapshot summary after each
  accepted page.
- Keep branch capture bounded for the configured 100,000-row branch limit.
- Preserve the exact generation, predecessor generation/digest, base revision,
  namespace overlay, inode state, COW pages, patches, expectations, links,
  symlinks, and immutable references.
- Renew only live leases; an expired lease must never be revived.
- Expiry, abort, retry exhaustion, compaction, and garbage collection must
  release every root, buffer, reservation, and lease.

The API must remain schema-free and core-owned. Do not expose SQL, tables,
repositories, raw manifests, CAS insertion, or COW mutation to the replication
package.

### 2.2 Bounded destination activation

Replace full-generation activation with a bounded activation protocol:

- Stage immutable content and branch rows in bounded durable batches.
- Maintain a durable staged-generation summary/digest while accepting pages.
- Activate by a constant-row pointer/generation swap guarded by exact base,
  generation, predecessor digest, and generation digest.
- Move materialization, old-row cleanup, and staged-row deletion to bounded
  maintenance after the pointer swap.
- Do not call a full staged-row materializer or rescan the complete branch
  generation during final activation.
- Enforce configured limits above 65,536 rows; the accepted maximum is 100,000
  changed paths.

The resulting visible generation must be atomic and reconnectable after every
durable statement fault.

### 2.3 Main incremental and genesis continuation

Implement durable continuation for every bounded state category:

- Namespace inode rows, entry rows, manifest roots, revision fragments, and
  checkpoint rows need explicit cursors and fragment completion state.
- Never advance a revision cursor after only a bounded prefix was emitted.
- Genesis bootstrap must continue beyond the first 256 rows.
- Main catch-up must use the destination’s actual durable head, not a constant
  zero, and must transfer only missing revisions/content.
- Add tests with rows above negotiated batch limits and with a destination
  already at revision N.

### 2.4 Durable replay and terminal authorization

Complete the protocol’s durable retry semantics:

- Persist missing-content response bytes or an equivalent durable replay receipt
  before advancing the outbound sequence.
- Drop and replay requests and responses independently in every phase,
  including missing-content, activation, result acknowledgement, and restart.
- Reject renewal after expiry with the canonical semantic error.
- Only an authority source may originate merged/discarded terminal state or a
  publication result. Validate flow and source role inside the destination
  activation command.
- Repeated terminal delivery with identical branch identity, generation,
  digest, terminal state, and retained result must replay idempotently. Any
  mismatch must leave the destination unchanged.
- Keep operation IDs bound to the complete guarded request.

### 2.5 Generation/publication correctness

Retain and extend the existing generation guard behavior:

- Compare expected generation and expected generation digest inside the
  authoritative publication transaction.
- Verify repeated authority-branch delivery after the source branch advances;
  the exact predecessor digest must be carried and checked.
- Verify lost publication responses replay exactly one stored result and create
  no second revision.
- Verify terminal publication/discard state and retained result return to the
  execution replica, followed by stale-branch reconnect rejection with no main
  fallback.

## 3. Computer closeout

Computer must remain a thin carrier/lifecycle adapter. Any additional
filesystem or replication state machine belongs in the host-neutral FS runtime.
The documented Computer production budget is approximately 100 net-new lines;
if the integration requires materially more, stop and move the abstraction to
the FS worktree before continuing.

### 3.1 Production transport

Use the actual Cap’n Web carrier:

- Authenticate and bind the peer before the first replication exchange.
- Keep replication on a separate uncompressed `/efs` connection or make the
  replication connection uncompressed; preserve legacy `/ws` behavior.
- Enforce raw frame ceiling `4 MiB + 64 KiB`, decoded request/response ceiling
  `3 MiB`, mutating acknowledgement ceiling `64 KiB`, scratch ceiling `2 MiB`,
  one exchange per operation, and one process-wide 20 MiB admission pool.
- Permit at most one 17.25 MiB exchange, with smaller reservations coexisting
  only when the aggregate fits.
- Account raw frame, decoded string, base64 expansion, decoded envelope,
  acknowledgement, scratch, transient RPC copies, stubs, and process buffers.
- Use `session.ping` for liveness; never use an empty replication transaction.
- Disconnect cleanup must release stubs and process reservations while keeping
  durable resumable filesystem state.

### 3.2 Lifecycle and mounts

Prove all lifecycle states with a real persistent database:

- Fresh empty replica: only unbound provisioning is exposed; no FS or Node VFS
  view exists before binding.
- Restart after every accepted provisioning batch and around final activation.
- Bind the exact authority identity, root, revision-zero metadata, timestamps,
  conflict tokens, page size, writer profile, manifest format, and FastCDC
  configuration.
- Transfer main, mount the exact active branch ID, reconnect after restart,
  and preserve branch isolation.
- Exercise shell/Git operations, hard links, symbolic links, rename, chmod,
  truncate, range writes, fsync, unmount, remount, and digest verification.
- Return the active branch to the authority through the actual Cap’n Web
  carrier, publish exactly once with generation-and-digest guards, replay a
  lost publication response, and return the terminal result.
- Delete/replace the local database, reprovision and retransmit main plus the
  active branch, remount the same branch, and verify exact identity/digest.
- Verify pinned readers survive activation, dirty writers receive the stable
  documented busy/divergence error, caches invalidate, and no dirty state is
  silently rebased or discarded.
- Bind each mount to workspace, engine, and branch. Enforce read-only policy
  locally. External mounts are not replication peers and must not receive
  private branch writes before explicit publication policy permits them.

The normal production path must not silently remain a DOFS-only workspace path
when the EFS carrier/profile is selected. If `/ws` remains legacy DOFS, the
EFS lifecycle must be explicitly and completely wired through the documented
Computer ownership boundary.

## 4. Required test and fault matrix

Add or extend shared tests for:

- All canonical golden vectors and corrupt/noncanonical inputs.
- Every legal/illegal role-flow pair and changed authorization/policy/limits.
- Fresh provisioning, every-batch restart, binding identity, wrong database,
  wrong schema/engine/workspace/authority, and database replacement.
- Empty, deduplicated, multi-batch, checkpoint, main, and every active-branch
  transfer flow.
- 100 MiB one-byte edit: only changed roots/nodes/objects, bounded metadata,
  and overhead transfer; no complete-file replication memory.
- Request loss, response loss, duplication, reordering, and process restart in
  every phase, including missing-content and activation responses.
- Fault injection after every durable statement and activation boundary.
- Branch base visibility, private isolation, read-only main, no fallback,
  reconnect, terminal closure, and exact generation digest.
- Guarded publication, intervening mutation, conflict, lost response, terminal
  return, and stale reconnect rejection.
- Lease expiry, non-revival, receipt compaction, abandoned staging, retry
  exhaustion, cleanup, garbage collection, and zero residue.
- 64 streams, 64 Node VFS writers, replication, queries, and GC under the one
  aggregate managed-memory ceiling.
- The unchanged CT-SCALE-1 100,000-row Node-to-Node and
  Node-to-Durable-Object fixtures.

The affected runner must retain live output, must map every new package/source
area in `scripts/run-affected-tests.mjs`, and must conservatively select the
quick suite for unknown changes.

## 5. Mandatory Computer gate

Run the exact clean pair of candidate commits through all 17 required steps in
the controlling M8 plan, using:

- actual Cap’n Web over WebSocket;
- authenticated peer binding;
- real privileged Linux FUSE kernel mount;
- persistent SQLite files and real process restarts;
- no mock, shim, binary loopback, or Node-VFS-only substitute.

The gate must record pass/fail for every numbered step, every dropped request
and response position, every restart position, and every cleanup assertion.

If privileged FUSE, the actual carrier, or another required external capability
is unavailable, preserve the exact diagnostic and mark M8 blocked. Do not
weaken the gate or substitute a mock.

## 6. Evidence and commit order

For a passing run only:

1. Start from clean FS and Computer candidate commits.
2. Run all mandatory gates and collect commands, versions, carrier settings,
   capabilities, limits, seeds, fixtures, identities, digests, flow counts,
   fault points, restart counts, timings, memory peaks, WAL/database growth,
   transferred/reused bytes, lease/reservation state, cleanup, and log hashes.
3. Extend evidence verification to reject candidate drift, wrong topology,
   fabricated logs, wrong commands/workloads, wrong carrier settings, missing
   FUSE identity, resource violations, and incomplete cleanup.
4. Commit the evidence atomically as the direct child of the production
   candidate.
5. Create a narrowly scoped acceptance commit only after evidence verification
   passes.
6. Only then update the milestone acceptance pointer; retain M7 acceptance
   until that point.

Never push, deploy, publish packages, change production Cloudflare state, or
delete user data without explicit authorization.

## 7. Definition of done

Handoff is complete only when all of the following are true:

- Every blocker in section 2 is fixed in the core-owned runtime and covered by
  a regression test.
- Computer is a thin, authenticated, bounded carrier/lifecycle adapter within
  the documented production budget.
- The exact mandatory Computer/FUSE gate passes on the exact clean pair.
- All required FS, Computer, fault, performance, cleanup, and evidence checks
  pass without weakened workloads or limits.
- Candidate, evidence, and acceptance commits have the required topology.
- `validate:accepted` is advanced only after M8 acceptance.
- The original dirty repository remains byte-for-byte untouched by this work.
