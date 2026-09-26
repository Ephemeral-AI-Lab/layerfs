# Linux projected range ioctl

> **Status:** Actual Linux mounted product insert, variants, refusal and
> lower-layer notification-failure functional proofs passed
> ([retained receipts](../../../issues/241/evidence/product-functional/REPORT.md)).
> Public SDK Exec and the registered four-case Edit→Commit selection remain
> separate gates.
> Implementation parent: `93fe2ee41` (status profile 4 integrated before
> this FUSE adapter change). The frozen Linux ABI is
> [`RANGE_IOCTL_ABI.md`](../../../issues/241/RANGE_IOCTL_ABI.md).

The Linux adapter accepts two fixed ioctl commands on an already open regular
file. `STATE` is a read-only, fd-bound 88-byte query for the file serial,
Workspace incarnation, generation, global revision, length and portable
mtime. `EDIT` is one 4,192-byte request carrying a 96-byte expected stamp and
range header followed by at most 4,096 owned replacement bytes. Both were
proved as carrier shapes with published `fuser` 0.18.0 on the actual Linux
kernel before this product source change; that test-only proof is not a
Workspace or performance receipt.

`range_ioctl.rs` parses and bounds the Linux byte layout. It rejects unknown
commands, versions, flags, sizes, reserved bytes, nonzero payload tail,
overflow and empty edit before payload ownership or publication. The adapter
checks request identity, mount writability and actual `(ino, fh)` authority.
Its STATE path retains one projection reply permit through the send attempt.
Its EDIT path retains the exclusive one-use mutation permit through the reply
attempt, owns the FUSE slice through the existing `OwnedPayload` ingress,
then calls the platform-neutral projected Workspace RangeEdit path. No command
text, local-path edit, Store API or suffix-copy algorithm is selected here.

Workspace compares the fd-bound expected stamp before candidate work and at
its publication boundary. The existing piece splice, quota, dirty frontier,
mtime, mutation receipt and post-publication invalidation path remain the
semantic implementation. A stale stamp maps to `ESTALE`; read-only and bad
handle errors are definite prepublication refusals. Accepted replacement
bytes are counted when the new overlay and revision become visible, even if a
later notifier fails. The physical suffix-copy counter is zero for the piece
splice; it does not mislabel the logical suffix length as I/O. Status profile
4 carries distinct `range_state`/`range_edit` callback counts and those two
bounded byte totals.

The notifier's `Ok` is provider-side completion, not proof that the caller
received a reply or refreshed cached attributes. A cooperating caller must
check the ioctl result, then a second STATE and same-descriptor `fstat` against
the expected revision, size and mtime; equal-length edits also need bounded
readback. A failed post-issue call or observation is UNKNOWN and cannot be
automatically retried or passed to the #241 workflow's explicit Commit.
`ReplyIoctl` has no checked delivery acknowledgement, so the adapter itself
does not claim an end-to-end success certificate. The underlying Workspace
retains a `CoherenceFailure` receipt when notification fails. Exact public
reconciliation after a lost reply is not implemented here.

The route is Linux-only. The ABI does not imply 64 KiB atomic edits, macFUSE
or WinFsp support. The 8 KiB mounted product tests proved old-FD/alias bytes,
size, mtime, EOF, later WRITE, stale/read-only refusals and canonical Commit.
They did not prove retained old-root readback or the 1–500 MiB position range.
The public SDK Exec tool and four-case timed selection must include STATE,
EDIT, caller confirmation and Commit work inside their declared boundaries,
and cannot inherit a cold-cache or latency PASS from these functional checks.
