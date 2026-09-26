# #241 mounted caller acknowledgement probe — frozen before execution

This is a test-only Linux FUSE diagnostic from source base
`6f56e5d6f47bf43559bb365a28544526cdf1a248`, using unchanged `fuser`
0.18.0. It does not call Workspace, Store, SDK or Commit. One attempt per
named case, each with a fresh mount and logical state. Failed attempts stay
in append-only evidence; no repeat-to-pass. The complete Docker command wall
is recorded for accounting, with no performance gate or cold-cache claim.
Cache state is uncontrolled.

The request is the prior provisional `LFR1` 4,128-byte `_IOW` command
`0x5020f541`: 4 KiB insert at offset 4,093 in an 8,192-byte virtual file.
Replacement byte `i` is `i % 239`, original byte `i` is `i % 251`. Expected
length is 12,288. The mounted file has a 60-second attribute TTL and returns
`FOPEN_DIRECT_IO`. Before ioctl, the caller holds one open writable descriptor
and caches its initial 8,192-byte size/mtime with `fstat`. The daemon logs
the exact callback bytes, inode, handle, command, and notification/reply
decision. No raw `/dev/fuse` reply frame is captured: the observable reply
is the caller's ioctl return/errno.

| Case | Deliberate action | Frozen caller decision |
| --- | --- | --- |
| `ack_success` | Publish virtual insert; send real `inval_inode(2,0,0)` before ioctl reply 0 | Return **SUCCESS** only if ioctl returns 0, same existing FD `fstat` shows 12,288 and newer mtime, 16-byte read at 4090 matches the inserted boundary, and read at 12,288 returns EOF. One ioctl, no retry. |
| `ack_suppressed` | Publish the same virtual insert but deliberately omit notifier send; return ioctl 0 | Caller must observe at least one stale/mismatched same-FD size, mtime, boundary, or EOF condition and return **UNCERTAIN**, never SUCCESS or retry. This is test-only suppression, not a real provider failure. |
| `ack_lost_reply` | Publish the virtual insert and a test-only exact 12,288-byte accepted-state artifact, then exit daemon process 23 before reply | Caller gets an ioctl error and returns **UNCERTAIN** without retry; retained artifact verifies accepted bytes independently. No durability or Workspace claim. |

The source build and binary hash are fixed before the first case. Run cases
once, in the table order. Any unexpected syscall errno or failed assertion
stays a failure at that identity; do not change the gate or rerun it. A real
notifier send error inside a live callback is outside these induced cases and
remains **INCOMPLETE** unless independently observed.
