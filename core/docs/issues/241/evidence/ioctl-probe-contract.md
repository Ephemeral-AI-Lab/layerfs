# #241 ioctl carrier probe: frozen before execution

This is a test-only Linux FUSE carrier diagnostic, not a Workspace mutation or
an Edit→Commit speed arm. Source base: `65ffd102c905ff5eda3e5681f044a7d7344ca8b1`.
Published `fuser` 0.18.0 remains unchanged. One attempt per named case; every
failure and skipped case remains in the result. No cold-cache latency claim.

## Provisional request

`ioctl` command is Linux `_IOW(0xf5, 0x41, 4128)`; exactly 4,128 input bytes,
no output bytes. Byte offsets: `0..4` ASCII `LFR1`, `4..6` version=1 LE u16,
`6..8` flags=0 LE u16, `8..16` offset LE u64, `16..24` deletion length LE u64,
`24..28` insertion length LE u32 (0..4096), `28..32` reserved=0 LE u32,
`32..4128` fixed inline payload. Only the first insertion-length bytes are
replacement data; the remaining payload bytes must be zero. The command's
encoded input size is 4,128 bytes, under Linux's 14-bit `_IOC_SIZE` ceiling
of 16,383; this does not establish a 64 KiB request. A successful reply is
zero with no data. Unknown command => `ENOTTY`; malformed/version/flags/range
=> `EINVAL`; nonwritable/stale handle => `EBADF`. Validation precedes mutation.

## Cases and gates

| Case | Exact action | Required observation |
| --- | --- | --- |
| `insert` | 4 KiB insertion at byte 4093 of 8192 | one ioctl callback, exact command/inode/handle/4128 bytes, resulting length 12288 |
| `delete` | delete 4096 at 4093, insert 0 | same delivery, resulting length 4096 |
| `overwrite` | delete and insert 4096 at 4093 | same delivery, resulting length 8192 |
| `refusals` | unsupported version, length 4097, flags 1, `u64::MAX` offset, read-only fd, stale handle, unknown command | distinct errno/callback and unchanged data/size/mtime |
| `coherence` | insert at 4093; inspect old FD, second FD, hard-link alias, size/mtime, boundary/EOF; then ordinary write | exact immediate observations with direct I/O plus inode invalidation before ioctl reply |
| `uncertain` | inject post-publication notification failure | client error and retained accepted mutation marked uncertain; no automatic retry |
| `ioctl-{1,10,100,500}m` | 4 KiB insertion at midpoint of virtual file of named MiB | one ioctl callback; no suffix read/write callbacks; bounded bytes and no size-proportional work; caller and daemon LFT1 |
| `write-{1,10,100,500}m` | one 4 KiB positional WRITE at midpoint of same virtual file | carrier-only control with same cache declaration; not an insert baseline |

For efficiency, the probe represents untouched file bytes virtually and holds
only the 4 KiB replacement. The carrier budget is ≤1.0 ms for caller ioctl
wall time and one callback; ≥5.30 ms is a direct no-go for the smallest
historical whole-route target. CPU/RSS are process observations, not per-call
isolated samples. Each case starts with a fresh test-only mount and logical
fixture; host instruction/page cache is uncontrolled, so all timing rows are
diagnostic and cold admission is `INELIGIBLE`. The WRITE control and ioctl
receive the same uncontrolled cache classification. A failed required gate
stops further implementation.
