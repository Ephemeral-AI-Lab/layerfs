# #232 Linux staged range ioctl ABI, version 3

> **Status:** Current planning checklist; no release candidate exists.
> Exact prospective byte layout frozen after the test-only
> [carrier GO](evidence/phase1-staged-carrier-probe/REPORT.md) and before
> staged product implementation. The existing [LFE2 inline ABI](../241/RANGE_IOCTL_ABI.md)
> remains unchanged.

All integers are little endian. The kernel `_IOC` type is `0xf5`. Unknown
commands return `ENOTTY`; unknown version or flags return `EOPNOTSUPP`;
malformed length, offset, reserved bytes or nonzero tail return `EINVAL`.
The ioctl's open descriptor, not user-supplied serial alone, identifies the
file. The semantic operation, Capacity boundary and SDK/Commit route are in
[the Phase 1 contract](PHASE1_ALL_IOCTL_CONTRACT.md).

## BEGIN: `_IOWR(0xf5, 0x42, 128)` = `0xc080f542`

Exactly 128 input bytes and 128 output bytes. BEGIN reserves the declared
logical replacement length under the per-mount aggregate 8 MiB quota, checks
the writable nonappend descriptor and exact current stamp, and creates a
private stage. It does not hold a Workspace mutation permit or change bytes,
revision or mtime.

| Input bytes | Value |
| --- | --- |
| `0..4`, `4..6`, `6..8` | ASCII `LFB3`, version `3`, zero flags |
| `8..16` | expected file serial `u64` |
| `16..48` | expected Workspace incarnation, 32 bytes |
| `48..56`, `56..64` | expected generation and global revision `u64` |
| `64..72`, `72..80` | offset and delete length `u64` |
| `80..88`, `88..96` | declared logical and literal replacement length `u64` |
| `96..128` | SHA-256 of the logical replacement bytes, including zero runs |

Output bytes `0..4` are `LFB3`, `4..6` version `3`, `6..8` zero status,
`8..24` the 128-bit unpredictable single-use token and `24..128` zero.
The output token is meaningful only after the caller receives a successful
BEGIN reply; a lost BEGIN reply leaves an expiring private stage with no
mutation. An empty edit (`delete_length=0`, logical length `0`) is refused.
The result length must pass checked `u64` math and the 4 GiB cap before stage
creation. Logical length above 8 MiB returns definite `ENOSPC`/Capacity
before any Workspace mutation. An aggregate reservation failure also returns
`ENOSPC`; a second stage on the same descriptor returns `EBUSY`.

## DATA: `_IOW(0xf5, 0x43, 4224)` = `0x5080f543`

Exactly 4,224 input bytes and zero output bytes. The 128-byte header is:

| Input bytes | Value |
| --- | --- |
| `0..4`, `4..6` | ASCII `LFD3`, version `3` |
| `6..8` | element kind `1=Bytes`, `2=Zero` |
| `8..24` | stage token |
| `24..32` | logical stream offset `u64`, exactly the next expected offset |
| `32..40` | element logical length `u64` |
| `40..44` | literal payload length `u32` |
| `44..128` | zero reserved |
| `128..4224` | literal bytes then zero tail |

Bytes requires logical length = literal length in `1..=4096`. Zero requires
logical length in `1..=8 MiB`, literal length 0 and all payload bytes zero.
Fragments must be ordered, contiguous and within the declared logical and
literal totals. A 64 KiB Bytes stream uses sixteen DATA calls; a Zero run of
64 KiB uses one DATA descriptor and no dense zero buffer. DATA changes no
Workspace bytes, revision or mtime. Malformed or out-of-order DATA fails
definitely before publication; the caller must ABORT or let the stage expire.

## APPLY and ABORT

`_IOW(0xf5,0x44,128)=0x4080f544` is APPLY (`LFA3`);
`_IOW(0xf5,0x45,128)=0x4080f545` is ABORT (`LFX3`). Both have version `3`
at `4..6`, zero flags `6..8`, the token at `8..24` and zero `24..128`.
Both have zero output bytes. A wrong descriptor/token/incarnation returns
`EBADF`; a stale stamp returns `ESTALE`.

APPLY consumes the token exactly once. It checks full logical/literal
lengths and SHA-256, current handle/incarnation/stamp and mutation capacity,
then obtains one Workspace mutation permit and performs one semantic splice
and revision. Definite validation failure before publication changes no
Workspace state. A lost APPLY reply or notification failure after publication
is UNKNOWN and is never automatically retried. ABORT consumes an unfinished
token and frees its quota with no publication. Descriptor release, unmount,
daemon shutdown and a 30-second monotonic deadline do the same. Expiry is
actively enforced while mounted; the caller cannot extend it with DATA.

The Linux adapter owns staging and its quota; Workspace owns semantic
Bytes/Zero splice, replay/piece limits and exact publication. No SDK range
edit method or POSIX fallback is part of this ABI. Published `fuser`'s reply
method cannot certify delivery to the caller; the cooperating editor confirms
STATE, fstat and boundary readback after a zero APPLY return.
