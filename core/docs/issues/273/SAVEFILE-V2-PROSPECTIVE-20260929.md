# #273 prospective internal SaveFile v2 decision

> **Status:** Owner-approved prospective implementation specification;
> not implemented or release evidence.
> Reviewed source: `e1d17c6d22279d13f47d6880b4fa0a3c69a8a159`;
> product: `f00644479a9b7dfe0b74e02438eed6d60e30dc0a`.

This is the concrete decision requested in
[#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5883171583).
[The existing research](LOWERING-GENERIC-EXEC-RESEARCH-20260929.md) and all
original failed receipts remain unchanged. This proposal grants no merge or
numeric admission. The retired Workspace range entrypoint and FUSE ioctl stay
absent; arbitrary caller commands retain ordinary mounted POSIX semantics.

Owner decision in the handoff conversation on 2026-09-29: **approve internal v2
with an authenticated attachment capability check**. The numeric-host direction
is macOS host Service plus Docker Linux daemon/FUSE; it is not a numeric waiver,
proof of cache/observer capability, or permission to merge. Preserve those gates.

The source contradiction is specific: `service/save/file_stream.rs` requires
each retained Base offset to be at least the previous Base end. A valid ordinary
copy from a later immutable Base offset to an earlier logical destination breaks
that condition. `commit/active.rs` uploads Base descriptors without their bytes;
resolving them by another daemon Service call while uploading would deadlock
behind the authenticated transport mutex. The Service already holds an immutable
`StoreProvider` beside its C2 `SaveHandoff` in `service/save/content.rs`.

## Decision requested

Implement a distinct, authenticated **internal SaveFile v2** operation that accepts
bounded backward and repeated Base ranges. Retain v1's opcode, grammar and
forward-only validation exactly. Allocate a new opcode after checking the full
registry: capability opcode 28 and v2-save opcode 29 are currently free; give both
only the existing SaveFile authorization capability. The v2 body begins with
version byte 2 and carries the same base root, base length, final length,
extent count and local/zero byte total. The body keeps fixed-width descriptors
and exactly the declared local/zero bytes. No Workspace, SDK editing method, ioctl
or command dispatch is added. C1/C2 canonical object formats and History formats
are not changed by this proposal.

Compatibility must be checked **before enabling read-provenance WRITE reuse**:
positively establish the Service's v2 capability during authenticated attachment.
An older peer must
refuse unsupported capability before that optimization can acknowledge a WRITE.
No error-driven replay with v1, recursive ReadFile, or later silent refusal is
allowed. The capability response is a typed maximum supported SaveFile version;
new LocalEdit attachments require version 2. ReadOnly attachments do not need it.
Existing v1 request acceptance is unchanged. This boundary is not implemented yet.

## Bounded Service adapter

Validate the declared base root and its actual logical length, every source
offset/length with checked arithmetic, every nonzero descriptor, exact final
length, all byte totals, framing EOF, authorization and original deadline before
acknowledging a save. Base references always address that single authenticated
immutable content root; they cannot name arbitrary other Store objects.

Keep the ordered C1 edit builder. During a descriptor pass, retained forward Base
runs remain anchors. A Base run behind the preceding retained end is a replacement
source run, as are Local/Zero runs. Persist only bounded, explicitly limited run
metadata and the caller's actual local bytes. A composite `EditSource` resolves
the replacement Base runs using the Service's immutable provider and C1 bounded
`read_range`, in fixed windows, while Local/Zero follow their declared source.
It must never spool backward Base bytes or allocate a final-file-sized buffer.
The upload source never makes another daemon RPC. Both replacement traversal and
canonical construction pay their own work under the original operation deadline.

This bounds resident data by the existing construction/read windows plus charged
descriptor navigation, and private disk by actual Local bytes plus declared run
metadata. It does **not** promise constant cost for arbitrarily many descriptors
or for rewriting an entire file with genuinely new bytes. A repeated Base range
may require canonical replacement construction; counter reports must count those
resolved bytes separately from uploaded local bytes and retained-anchor bytes.
Run metadata has at most one 32-byte source record per descriptor, at most one
32-byte edit per descriptor plus one final edit, and at most one 16-byte zero
record per descriptor. Check `80*extent_count + 32 + local_bytes <= 2*MAX_FILE`
with overflow refusal, before opening the spool files. Keep the existing
`MAX_FILE=4 GiB`, `MAX_SAVE_STREAM_BYTES=8 GiB` and 64 KiB read window. Source
navigation reads one fixed record at a time using binary search. These are upper
bounds, not a claim that metadata proportional to extent count is constant space.

Canonical identity is established against the existing ordered-edit semantics
for the same anchors and actual replacement bytes, including representation
transitions and no-op identity. Do not claim that a localized edit and a complete
rechunking produce the same root merely because their logical bytes match.

## Generic Workspace read provenance and custody

Any provenance optimization uses a bounded, charged record of bytes actually read
through the generic path, tagged with Workspace incarnation, selected canonical
root, inode identity and source interval. Compare every supplied WRITE byte and
revalidate the selected identity before sharing Base origin. Nonmatches use the
ordinary charged payload route. A stale record, a different root, a fresh/local
range or mixed origins cannot invent canonical ownership. Do not copy the reverted
experimental patch. Pinned G1 and later G2 selections keep their own authenticated
origin roots across reconcile; provenance is neither a pin nor a refund permit.

Definite prepublication errors abort checked candidate ownership. Unknown save or
Commit acknowledgement preserves captured/submission custody and does not retry,
refund a pin, delete canonical data, or switch algorithms. Physical private files
remain accounted by verified `st_blocks * 512`, including failed candidates.

## Required clean-source falsifiers before any gate PASS

1. The unchanged registered 64 MiB native lowering, three ordered edits, 8 local
   replacement bytes, 64 MiB quota, 10 s Stage deadline and 60 s complete limit;
   independent full 67,108,662-byte saved-C1 oracle and physical charge equality.
2. Real public SDK Exec performing ordinary reordered and duplicated Base copies,
   a known successful SDK Commit, independent full old/new bytes, old pinned G1,
   live G2 and repeated Commit with exact changed root/identity semantics.
3. Nonmatches, stale/different-root provenance, overlapping later writes, failure
   before publication, denied and lost save/Commit results, exact checked pin
   release/refund and cleanup. Existing C5 and response-Budget proofs must regress
   after any product change.
4. v1 acceptance/refusal unchanged, v2 roundtrip/authentication/malformed framing,
   unknown version/opcode and the chosen mixed-runtime admission boundary; C1
   transition/no-op/reference identity and full external Linux routes.

The separately observed 32 KiB pinned SDK read `Io` remains unresolved. The
proposal does not assert that an SDK 128 KiB read works. Historical numeric rows
stay INELIGIBLE and the frozen control stays NOT_RUN until its distinct method
gate has independent proof or an explicit owner numeric-profile ruling.
