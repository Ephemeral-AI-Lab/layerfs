# #241 exact-size final-candidate kernel probe — frozen before execution

Source base `241096083f29fba74835581b4f3297c015924e8c`. Published `fuser`
0.18.0, locked release aarch64 Linux Docker/FUSE. Test-only virtual bytes;
no Workspace, Store, SDK, Commit, product source or benchmark route. Five
named cases, one attempt each in table order, fresh mount/state per case.
Retain every failure; never retry to pass. Cache state is uncontrolled, and
complete Docker command wall is accounting only, not a speed result.

## ABI

STATE is Linux `_IOWR(0xf5,0x40,88)` = `0xc058f540`. The caller supplies an
88-byte zero-filled buffer whose first eight bytes are `LFS2`, LE version 2,
LE op 1. All remaining request bytes are zero. The daemon must see input
length 88 and `out_size=88`, then return **exactly 88 bytes**. Reply fields:

| Byte range | Field |
| --- | --- |
| `0..4`, `4..6`, `6..8` | `LFS2`, LE version 2, LE status 0 |
| `8..16` | inode LE u64 = 2 |
| `16..48` | 32-byte incarnation = `0x24` repeated |
| `48..56`, `56..64` | generation LE u64 = 7, revision LE u64 = 3 initially |
| `64..72` | length LE u64 = 8192 initially |
| `72..80`, `80..84`, `84..88` | mtime seconds LE i64 = 1700000000 initially, nanoseconds LE u32 = 0, reserved = 0 |

EDIT is Linux `_IOW(0xf5,0x41,4192)` = `0x5060f541`. The daemon must see
exactly 4,192 input bytes and `out_size=0`; its success reply has **zero
data**. Header `0..4="LFE2"`, `4..6=version 2`, `6..8=flags 0`,
`8..16=inode 2`, `16..48=expected incarnation`, `48..56=expected generation`,
`56..64=expected revision`, `64..72=offset 4093`, `72..80=delete length 0`,
`80..84=insertion length 4096`, `84..96=reserved zero`. Payload `96..4192`
is exactly byte `i % 239`. The virtual base is 8,192 bytes, byte `i % 251`.
One accepted edit yields revision 4, length 12,288, mtime seconds 1700000001.
Wrong stamp returns `ESTALE` before publication. There is no nonce, ACK,
automatic replay or silent fallback. A real normal edit invalidates inode 2
before reply. The test-only suppressed-notifier and deliberate lost-reply
cases are fault probes, not provider-failure proofs.

## Cases and gates

| Case | Sequence | Required observation |
| --- | --- | --- |
| `state_exact` | One STATE on an existing writable FD | One exact callback; caller receives all 88 response bytes and initial inode/incarnation/generation/revision/length/mtime; zero publications. |
| `edit_state` | Initial STATE; EDIT with its stamp; same-FD `fstat`, 16-byte boundary read at 4090, EOF read at 12288; second STATE | EDIT exact 4,192 input and zero output, ioctl success, readback new size/mtime/bytes/EOF, STATE exact revised stamp, one publication. |
| `stale_refusal` | Initial STATE; EDIT with revision 2, all other fields valid; second STATE and `fstat` | `ESTALE`, unchanged bytes/size/mtime/stamp, zero publications. |
| `suppressed_unknown` | Initial STATE; accepted EDIT but test-only omit notification, reply success; same-FD readback and second STATE | Caller sees stale or mismatched `fstat`/readback and reports UNKNOWN with no retry, even though STATE may show revised stamp; one publication. |
| `lost_reply_unknown` | Initial STATE; accepted EDIT then deliberate daemon exit 23 before reply, retaining exact test-only accepted-state artifact | Caller receives an error and reports UNKNOWN with no retry; accepted artifact equals all 12,288 expected bytes; one publication event. No crash-durability claim. |

Raw daemon logs include the full request hex, command/inode/handle/in/out
counts, intended reply bytes or errno, and publication/notification events.
Caller logs include ioctl return/errno and all received STATE bytes and
readback. No raw `/dev/fuse` reply frame is claimed. If the exact 88-byte
STATE output or zero-output EDIT fails on kernel/provider, retain the
nonpassing attempt and stop product-path claims.
