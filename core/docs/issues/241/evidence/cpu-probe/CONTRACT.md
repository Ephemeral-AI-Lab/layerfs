# #241 ioctl CPU diagnostic contract (frozen before attempts)

Scope: the published `fuser` 0.18.0, a real privileged Linux Docker/FUSE mount,
and a virtual file. This is carrier-only; no Workspace, Store, SDK or Commit.
The test-only probe retains the existing 4,128-byte `LFR1` version-1 insert
request, 4 KiB payload, command `0x5020f541`, writable open handle, direct I/O,
and inode invalidation before successful ioctl reply.

One attempt each, in this order:

1. `ioctl-1m`: virtual initial length 1 MiB, insert at byte 524,288.
2. `ioctl-500m`: virtual initial length 500 MiB, insert at byte 262,144,000.

Each case uses a fresh logical file and mount. No priming or previous sample
is used. The virtual file computes no source data from storage. Kernel and
instruction caches are uncontrolled; declare cache state unknown and treat
wall/CPU as diagnostic, not a cold performance PASS. No WRITE comparison or
speed ratio is part of this selection.

LFT1 operation windows use the existing `OperationRecorder` with a test-only
`WindowSource`. Its `begin` and `finish` take `getrusage(RUSAGE_SELF)` user and
system CPU (microsecond native quantum), `/proc/self/statm` RSS endpoints, and
`CLOCK_THREAD_CPUTIME_ID`. The first snapshot completes before the LFT1 wall
timer starts; the second begins after it stops. LFT1 `cpu_shared_ns` is the
bracketed **process-shared** CPU delta, including unrelated threads if any;
it is not exclusive operation CPU. The thread-clock delta is a separate
test-only sidecar for the caller/callback thread, not an LFT1 field. RSS is the
maximum of two absolute endpoints, never a phase peak or allocation delta.
Reject missing, regressing, or zero-duration observations rather than
fabricating CPU. The daemon log must retain ioctl/READ/WRITE callback and byte
counts. The host records the complete `docker exec` wall for each attempt.

Admission: delivery and count checks may PASS independently. CPU attribution
is diagnostic even with two valid boundary samples. If process CPU includes
uncontrolled concurrent work or its 1 µs quantum erases the delta, mark the
per-operation CPU conclusion INCOMPLETE. No repeat-to-pass samples.
