# #273 admission capability decisions still required

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Original owner-ruling request; §4 records the later prospective work-plan
> choice, not a completed SDK contract, lease proof, release waiver, matched
> speed result or authorization to edit frozen #271 product.
> Source of the candidate C1 provenance: `8801b9c095b9975eaa81fc98c738df6ef9752710`.
> Refer to the append-only [iteration ledger](CHECKPOINT5-OPTIMIZATION-LOG.md)
> §§016–018 and the local gitignored iter-017/018 receipts. Do not publish a
> GitHub URL for raw evidence that is not committed.

## 1. Public same-Workspace SDK journal pin: approve a contract or rule unrun

The current `WorkspaceApi` public surface is `mount`, `exec`, `commit`,
`status`, `unmount`; it has **no** retained same-Workspace journal selector
or pin/release lease. The checkpoint runner's `retained` route deliberately
fails closed; the two clean/one-edit controls produce `NOT_RUN` blockers at
the new product identity. A detached Store, closed prepared clone, an active
Workspace-only Stage control or a status read is not a substitute for the
registered *same Workspace* sequential SDK Exec/Commit with pinned G1
journal and continuing G2 process.

**Owner decision requested (choose one, prospectively):**

1. Authorize a public SDK/Bridge lease design. Specify selection identity,
   admission/release/refund, explicit expiry and cancellation, old-reader
   bytes, G2 process/edit continuity, maximum 32 captures, one worker, known/
   unknown canonical custody, deadline and cache/measurement boundaries.
   Implement and test against the same mounted Workspace, then freeze a
   new workload/harness identity before *any* control attempt. A new API is
   not permitted here on speculation.
2. Explicitly rule the two registered SDK pinned-journal selections
   unsupported for this profile, retain `NOT_RUN` with an owner-approved
   scope/waiver and do not call the 12-row registry complete or promote a
   benchmark admission PASS.

No owner ruling is recorded here. Until one arrives, **NOT_RUN**.

## 2. Frozen #271 control and private cache/phase-cgroup capability

A *separate owned read-only checkout* is prepared at the unchanged
`48b51e874a41b3e1e6c6661e145316df8b408f07`, tree
`64d06dabba1a6d123d1002487c189809ded05cc0`, under this assigned
worktree's ignored `core/target/issue273/owned-frozen-271-control/`. It
is clean, detached, and has its **own** prospective Cargo target. It is not
the other owner's #271 worktree. No control arm has been built or sampled,
no frozen product file was edited, and no numeric denominator has been
invented. Product/input seals and checkout custody are in the local
iter-018 `iter018-owned-control-provenance.json`.

The existing candidate rows attest host whole-input invalidate/mincore;
that is **not** private container metadata, VM/backend/device or host cache
attestation. O_DIRECT on Linux private files is a request, not a complete
cache guarantee. On this Docker Desktop host, a standalone **owned**
privileged cgroup-v2 probe created ~36 MiB `memory.peak`, released its
32 MiB shmem, then wrote `0` to the apparently writable `memory.peak`.
Although the write exited `0`, peak remained **36,163,584 bytes** while
`memory.current` was **2,428,928** after reset. Thus lifetime peak cannot
be called a reset phase peak on this runtime. The benchmark driver reports
`container_cgroup_memory=None` and has no independent Exec/Commit cgroup
proof. This probe measures a capability, not a product arm; readback is
retained in local iter-018. No cache flushing or a new daemon/worker is
allowed to move measured work out of a phase.

**Owner decision requested:** provide an independently *verified* identical
private-cache/VM/backend/device/host and phase-local cgroup method for both
arms that preserves running Workspace process/custody, and authorize only
common harness/observer changes on a separately sealed unchanged-#271
product; or explicitly rule numeric comparison ineligible in this profile.
The prepared checkout alone is not a matched pair. Do not start row-major
control->candidate samples until that capability and the shared identities
are prospective and auditable. No such approval/capability exists yet:
matched control **NOT_RUN**, numeric results **INELIGIBLE**.

## 3. Current product and unrelated test gate

The changed candidate emitted one complete `LFS_C1_EDIT_LOAD v=1` zero on the
independently byte-verified original #248 public gate. That fixes *its*
previous missing-zero provenance **only at the new candidate identity**;
historical iter-012/017 receipts stay INCOMPLETE. All nine current candidate
rows also have complete C1 counters, bytes, cleanup and under-limit walls,
but are still numerically INELIGIBLE for §2. C1 edit-node work is distinct
from `LFS_FILE_INPUT` final-record reads and `LFS_C1_SAVE_COUNT` construction
counts; never infer missing zero from either.

The broad host release run at this new source **FAILS two
`layerfs-content/tests/filesystem_ordering.rs` ordering-limit tests**:
`a_fresh_build_charges_its_count_array_to_the_ordering_ceiling` and
`a_high_pending_ceiling_runs_spill_free_to_the_byte_bound`. The changed C1
apply/server code does not modify the filesystem ordering implementation;
the tests were not run at the first-parent source as a matched baseline, so
this is **an unresolved release test blocker**, not a proven pre-existing
failure or a reason to silently relax ceilings. Targeted C1 edit tests,
Server/Workspace/FUSE host suites, warning-denying Clippy/fmt and product
boundary pass. Assign the ordering-limit diagnosis to its owning lane before
claiming an all-tests or release PASS.

No 2×/10×, release admission, merge or #273 closure follows. PR #274 remains
draft pending explicit owner decisions and matched evidence.

## 4. Owner direction received after the request (2026-09-28 UTC)

The owner accepted the *optimization-first* recommendation in the handoff
conversation: **1A / 2A / repairs yes / functional-only scope yes**. See the
[dated preintegration review §4](PREINTEGRATION-REVIEW-20260928.md#4-owner-direction-pursue-substantive-optimization-not-the-easy-waiver)
for precise scope, dependencies and stop conditions. The request paragraphs
above record the earlier state; this addendum does not amend past receipts.

- **§1:** pursue a real public same-Workspace G1 journal lease with reviewed
  SDK/Bridge contract and full G1/G2/32-pin/cancel/custody proof. No waiver
  was chosen. No API is specified or implemented merely by this decision;
  clean and one-edit registered controls remain **NOT_RUN**.
- **§2:** pursue a verified common private-cache and phase-local cgroup
  capability before any future matched campaign. It is not currently
  available/proved; old control **NOT_RUN**, all nine candidate numbers
  **INELIGIBLE**. Stop if no common method can be independently verified.
- **§3:** authorize C1 red-test diagnosis/repair and #269 duplicate-seal
  correction or justification in **separate owned worktrees**, never by
  increasing the ceiling. Old FAIL receipts and paired-parent NOT_RUN stand.
- **Integration:** #256 package scale and #270 path-local C1 move-Commit
  are deferred, explicitly **NOT_PROVED** for an initial functional-only
  integration. #270 is prioritized as separate substantive follow-up once
  combined functional correctness is proved. No draft PR merge, combined
  tree, benchmark, release admission or test PASS was authorized by choosing
  this work plan. Seek separate explicit integration authorization.
