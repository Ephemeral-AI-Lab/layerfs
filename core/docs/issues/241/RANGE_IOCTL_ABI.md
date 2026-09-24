# Linux projected range request — frozen v2 carrier contract

> **Status:** Frozen for the Linux product implementation after the exact
> test-only [STATE/EDIT mounted-kernel proof](evidence/protocol-probe/REPORT.md).
> This is not a release or a portable macOS/Windows ABI. Its diagnostic
> receipt does not become a product Edit→Commit performance sample.

The semantic operation is platform-neutral: replace `[offset, offset +
delete_len)` in one authorized projected file with owned replacement bytes.
The Linux carrier uses the open mounted descriptor. No SDK command string or
benchmark-only hook selects product behavior. A cooperating tool must discover
support and obtain a current stamp on that descriptor before EDIT. Both calls
and all caller confirmation work occur inside `WorkspaceApi::exec` when that
route is later measured.

## Fixed Linux commands

Integers are little-endian. Every reserved byte must be zero. STATE uses
ASCII `LFS2` and EDIT uses `LFE2`, both at version `2`. Linux `_IOC_SIZE` has 14 bits; the
largest message below is 4,192 bytes. This contract supports at most 4,096
inline replacement bytes; it makes no 64 KiB atomic-edit promise.

### STATE: `_IOWR(0xf5, 0x40, 88)` = `0xc058f540`

The 88-byte input is `LFS2` at `0..4`, version u16 at `4..6`, operation `1`
at `6..8`, and zero bytes at `8..88`. It changes no Workspace state. The 88-byte
output is:

| Offset | Value |
| --- | --- |
| `0..4`, `4..6` | `LFS2`, version `2` |
| `6..8` | status u16, zero on success |
| `8..16` | file serial u64 |
| `16..48` | Workspace incarnation, 32 bytes |
| `48..56` | current generation u64 |
| `56..64` | current global Workspace revision u64 |
| `64..72` | current file length u64 |
| `72..80` | portable mtime seconds i64 |
| `80..84` | portable mtime nanoseconds u32, `<1_000_000_000` |
| `84..88` | zero reserved u32 |

STATE must read the handle, inode, stamp and attributes under one consistent
Workspace view. The same descriptor is used for STATE, EDIT and later
confirmation. An unsupported kernel/adapter returns an error before any
mutation. STATE is read-only even on a read-only mount. A cooperating editor
opens the file writable and refuses a failed open or unsupported STATE before
issuing EDIT; the adapter independently rejects EDIT on a read-only mount.

### EDIT: `_IOW(0xf5, 0x41, 4192)` = `0x5060f541`

| Offset | Input value |
| --- | --- |
| `0..4`, `4..6`, `6..8` | `LFE2`, version `2`, zero flags u16 |
| `8..16` | expected file serial u64 from STATE |
| `16..48` | expected Workspace incarnation from STATE |
| `48..56`, `56..64` | expected generation and global revision u64 from STATE |
| `64..72`, `72..80` | offset and deletion length u64 |
| `80..84`, `84..96` | insertion length u32 (0–4096), then 12 zero reserved bytes |
| `96..4192` | inline replacement; exactly insertion length meaningful bytes, zero tail |

EDIT has no output bytes and returns ioctl result zero only after publication
and the adapter's notification attempt. It must check actual kernel inode,
open handle, mounted writable authority and nonappend access independently
of the caller's byte fields. Workspace compares the supplied stamp before
ownership/publication and again at its exact revision/generation publication
boundary. It owns at most 4,096 replacement bytes before mutation. A
zero-deletion, zero-insertion request is rejected without revision or mtime
change. Pure insert, pure delete and equal-length overwrite are valid; use
checked offset/end/result-length arithmetic and existing file, quota, replay
and piece bounds. There is one range mutation request, with no suffix-copy
fallback.

Unknown command returns `ENOTTY`; unknown version/flags or unsupported
capability returns `EOPNOTSUPP`; malformed/reserved/nonzero tail/overflow or
empty edit returns `EINVAL`; read-only mount returns `EROFS`; nonwritable,
wrong-inode or stale handle returns `EBADF`; stale stamp or mutation permit
returns `ESTALE` for the stamp and `EBUSY` for the permit, respectively;
definite prepublication capacity failure returns `ENOSPC`.
Transport loss, ambiguous deadline/I/O, publication followed by notification
failure, or an uncertain reply must never be translated into a definite
rollback. Preserve the mutation receipt/custody and do not retry automatically.

## Caller-observed outcome and Commit boundary

After EDIT returns zero, the tool calls STATE again on the same descriptor.
It requires the same incarnation, generation and inode; global revision must
be exactly the earlier value plus one and length must equal the checked
result. Same-descriptor `fstat` must match the second STATE length and mtime.
For equal-length edits, bounded boundary readback must also match expected
bytes; length alone cannot confirm visibility. Functional mounted tests check
alias, cross-boundary bytes, EOF and later ordinary writes for all edit shapes.
No assumption that wall-clock mtime strictly increases is made.

The tool reports success only after these checks. An ioctl error after the
request may have been issued, a mismatch, or a lost second STATE yields a
distinct **UNKNOWN** tool result (exit code 75 with bounded request/stamp/
errno evidence), not a retry. A definite prepublication refusal is separate.
The #241 SDK caller invokes explicit Commit only for `ExecResult.exit_status
== Some(0)`; `None`, 75, any other nonzero status, and transport uncertainty
stop its automatic workflow. Current `WorkspaceApi::exec` exposes process
exit status, not a typed per-range receipt. A deliberate later Commit by an
independent caller and exact reconciliation after daemon death need a separate
product custody/API decision; this candidate does not claim to solve them.

The existing `CoherenceStatus::Ready` following notifier `Ok` denotes only
provider-side completion. `fuser` can return `Ok` after unmount and has no
reply-delivery acknowledgement. A real mounted caller-confirmation probe
already showed why `ioctl == 0` alone is insufficient: suppressing notification
left `fstat` stale while direct reads saw new bytes. The prospective v2
probe then observed `STATE in=88,out=88` with the full 88-byte caller reply,
`EDIT in=4192,out=0`, one published revision, exact post-STATE/fstat/readback,
and `ESTALE` with unchanged state for a stale stamp. Suppressed notification
and lost reply produced caller UNKNOWN without retry. Those are carrier
observations only; projected Workspace mutation and Commit still need their
own proof.
