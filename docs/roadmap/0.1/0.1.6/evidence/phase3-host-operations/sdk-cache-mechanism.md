# SDK cache reconciliation hypothesis

This is a changed-mechanism probe for explicit SDK edit reconciliation. It does
not capture a Commit snapshot and does not resolve V1. The existing original
`v1-probe/probe.c` is included unchanged; the new harness intercepts backing
copies to model one protected byte read from an immutable post-edit input.

The SDK needs to preserve its installed ranges while the kernel launders a
previously dirty folio, retaining incoming unrelated dirty bytes. Eagerly storing
every byte of a shifted logical suffix would read/rewrite the entire suffix.
The tested hypothesis instead protects incoming writeback, invalidates existing
cached pages, and checks a pre-opened mount descriptor with `syncfs`.

This extra check is necessary: the pinned Linux implementation calls page-cache
invalidation but discards that function's error return. An invalidation syscall
result alone therefore cannot prove that laundering succeeded. The source is
[`fuse_reverse_inval_inode`, Linux 6.12.76](https://github.com/gregkh/linux/blob/39b686f8d57d7506af7789e915fe7fd103b0fe57/fs/fuse/inode.c#L480).

One run, `sdk-invalidation-probe-attempt01.json` / `.log`, used the existing
immutable Linux image, generic `/dev/fuse`, cached write-through mode, GCC 12.2,
one CPU, 512 MiB and 128 PIDs. The measurement lock was held only for this probe.
No benchmark case, including any #122 case, was executed.

| Identity | Actual result |
| --- | --- |
| Protected queued writeback without NOTIFY_STORE | PASS. One laundering WRITE; invalidation and syncfs return zero. Mapped and backing byte 16 both contain SDK `S`; mapped and backing byte 128 both retain unrelated dirty `u`. |
| Injected laundering I/O failure | PASS error-observation obligation. Invalidation still returns zero, while syncfs returns -1/EIO. Exactly one WRITE reply was rejected. No SDK success may be reported from this outcome. |
| Retry after restoring backing writes, retaining protection | PASS. Mapped and backing byte 16 both contain SDK `T`; mapped and backing byte 128 both retain `z`. |

Probe binary SHA-256:
`4af3be9e2c6631ffff2f0401058a0bb9c88aedb263517f16fd7ea43883783e2b`.
Exact harness/original-source hashes, image identity and command are in the JSON
receipt. The temporary mount belonged to the probe container, which exited and
was removed.

The result supports implementing protected writeback plus checked invalidation
for the SDK scope without an eager whole-suffix STORE loop. It is deliberately a
one-folio hypothesis check. Required product tests must still cover the actual
adapter, unequal edits, mapped concurrency, multiple cached pages, resource bounds,
and retry/End cleanup. The production callback gate and immutable lease must own
protection through all admitted writeback replies. They must not become a Commit
freeze or an acquisition fallback.
