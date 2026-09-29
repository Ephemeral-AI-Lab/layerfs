# #273 lowering: public unaligned shift decision and bounded proof plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Reviewed against `4809dcbd5d5ec80a1dae7709046b502aff80db0f`; no product source,
> quota, fixture, oracle, deadline or receipt is modified by this proposal.
> **Superseded proposal — Option A is expressly rejected by the owner.**
> The direct Workspace range-edit entrypoint and FUSE ioctl must remain absent;
> they must never be restored. Optimize the generic read/write mutation route
> used by `WorkspaceApi::exec` for arbitrary shell commands, not an edit carrier.
> This page records the earlier research question, not current authority.

## The contract conflict, not an allocator workaround

The registered `linux::stage_lowering` case applies three ordered splices to
one **67,108,864-byte** immutable base file: `[100,110) → "abc"`, insertion
`[200,200) → "12345"`, deletion `[500,700) → empty`. Its final length is
**67,108,662 bytes**, eight literal replacement bytes, and the old-base
right tail is logically shifted by +202 at distant reads. It checks original
file serial, the first 1,024 saved bytes, 512 distant saved bytes, the
SaveFile replacement-byte count `8`, an unchanged Branch before Stage, and
retained Stage state. The registered fixture's Workspace private quota is
**67,108,864 bytes**; the Stage deadline and command hard limit are unchanged.

The external [fixture helper](../../../crates/layerfs-workspace/tests/support/native_workspace.rs)
uses fixed-offset public `set_len`/`write_file` to copy nearly the entire
right tail before Stage. Its retained local-only lowering FAIL (`core/target/issue273-next-stage-lowering-01/result.json`)
was `Backing(Acquire, StorageFull)` in this helper, not a C1/C5 lowering
refusal. A larger quota, a smaller file, a fabricated partial-byte oracle,
a test-only call into private extents, or reconstructing canonical C1 data
outside Workspace would not close this test.

This is not a matter of switching to an existing supported native range edit:
[#252's completed cutover](../252/REPORT.md) explicitly removed the direct
Workspace range-edit entrypoint, its projected range wire field and the Linux
FUSE ioctl, choosing SDK `WorkspaceApi::exec` → ordinary POSIX/FUSE mutations
→ Commit instead. The older
[Linux projected ioctl description](../../architecture/proposal/fuse-workspace-snapshot-overlay/58-projected-range-ioctl.md)
describes historical behavior, **not** a surviving product operation. Current
Workspace fixed-offset write/truncate and current FUSE adapter have no
unaligned atomic insert/collapse. POSIX `fallocate` insert/collapse normally
requires filesystem block alignment; the registered offsets 100, 200, 500
and 700 are not block-aligned. Pretending a byte-wise `copy_file_range` of
an overlapping tail is an atomic bounded shift is also not valid.

Internally the active extent record already carries `source_offset` and
`Extent::cut` preserves a Base origin when splitting. This is a representation
capability, **not** public permission to add a test-only path. See
[extent records](../../../crates/layerfs-workspace/src/backing/active/extents.rs)
and the [existing public mutation dispatcher](../../../crates/layerfs-workspace/src/filesystem/active_file.rs).

## Superseded decision alternatives (retained for the audit)

**Option A (rejected; MUST NOT implement):** explicitly authorize
one *generally supported*, public native Workspace operation for an unaligned,
length-changing regular-file splice on a writable LocalEdit handle. It must
be available to real clients, not keyed to a test or Stage. Proposed shape
is `Workspace::splice_file(handle, start, end, &OwnedPayload, deadline) ->
Result<MutationReceipt, WorkspaceError>`. This is **not** permission to revive
`Client::edit_workspace_file_range(s)`, the retired FUSE ioctl, a benchmark
selection, or a new per-call fallback route. The owner must separately choose
whether/how a genuine POSIX-capable adapter binds this operation, or specify
another approved public operation before claiming mounted/SDK coverage. The
native Stage test would exercise this authorized public Workspace operation;
it would not impersonate the ordinary mounted-write benchmarks.

**Option B (retain #252 without an exception):** identify an existing
owner-approved ordinary public operation capable of the same unaligned,
length-changing shift under this exact quota and oracle, or explicitly
dispose of the registered lowering gate as **FAIL / NOT_PROVED**. Existing
fixed-offset writes alone are not such an operation. Do not relabel a test
that checks only two small windows as a full-file proof.

The owner's later instruction rejects Option A. It is never an authorized
implementation route. The current generic write optimization and exact proof
are documented separately; this older page must not be used to restore an API.

## Historical unapproved A proposal (DO NOT IMPLEMENT)

1. Check file kind, handle scope/rights, `start <= end <= selected EOF`,
   replacement ownership and maximum admitted input, new EOF overflow,
   deadline and cancellation **before** mutation. A zero-length insertion
   with empty bytes is a validated no-op; deletion with empty input changes
   EOF. Return the existing typed mutation receipt only after publication;
   lost acknowledgement remains uncertain and never retried blindly.
2. Under the existing one-producer publication boundary, snapshot the
   original indexed interval. Precharge all affected extent, inverse-key,
   inode/dirty and index-candidate records before publishing one new root.
   Splits retain `source_offset`; retained Base never copies 64 MiB to private
   backing. Shift every suffix extent's **logical** `[start,end)` by the
   signed delta while preserving its source origin, including Zero, Packed
   and Payload. Update corresponding inverse keys, pack slot/payload owner
   selection, EOF and portable mtime. Fold adjacent compatible pieces only
   if that does not change byte identity. Ordinary pwrite/append/truncate
   and later splices must still work on the shifted view.
3. Preserve old pinned readers and G1 while publishing G2 atomically. Refuse
   on inadequate charged Budget/quota *before* publication; uncertain physical
   cleanup stays in custody. No relaxed 64 MiB private quota, 10 s Stage
   deadline, single worker, memory bound or failure/retirement semantics.
   Input and work may scale with affected extent count/charged index height,
   **not** with the inherited 64 MiB tail's byte length when it is one Base
   extent. Count actual local bytes written and backing blocks to rule out
   hidden spool/physical tail copies.
4. Native functional proofs at approved source: insertion, deletion,
   equal-length replacement, two further edits after a shift, EOF, overlap
   across Base/Packed/Payload/Zero, stale handle/wrong-kind/readonly/overflow,
   partial-allocation refusal, pinned old-G1 and live-G2 exact bytes, charged
   cleanup/refund and typed unknown result custody. Keep #252's retired SDK
   and ioctl entrypoints absent. Update the affected architecture documents
   in the same product commit; no third-party changes.
5. Replace **only** the failing helper's full-tail-copy call in
   `stage_lowering` with the authorized operation. Keep its three exact
   ordered edits, 64 MiB input, 64 MiB quota, oracle, replacement count and
   source identities. Add an independent, bounded-window **full 67,108,662
   byte** verifier: construct the expected old-base/insert segments from the
   three literal edits outside the product, stream saved content via the
   public native Service read route, compare every byte and EOF, and retain
   the original prefix/distant checks. Declare verifier wall separately;
   never enlarge the registered Stage deadline/command limit to fit it.
   Preserve every earlier FAIL with its own source, and rerun Stage once at
   each new frozen test/product identity only for an actual change.

This page grants no public range-edit permission, functional success, release,
numeric admission or PR merge. Its original FAIL remains historical evidence.
