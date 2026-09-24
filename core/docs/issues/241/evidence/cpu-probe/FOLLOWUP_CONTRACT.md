# #241 CPU probe follow-up selection (frozen before new attempts)

The first [contract](CONTRACT.md) and its `ioctl-1m`/`ioctl-500m` attempts
remain append-only. This follow-up uses a new test source/binary identity and
does not rerun either earlier ioctl case.

Exactly six new carrier-only cases, one mounted attempt each, in this order:

| Case | Virtual file length | Offset | Expected callback |
| --- | ---: | ---: | --- |
| `ioctl-10m` | 10 MiB | 5,242,880 | 1 ioctl, 4,128 input bytes |
| `ioctl-100m` | 100 MiB | 52,428,800 | 1 ioctl, 4,128 input bytes |
| `write-1m` | 1 MiB | 524,288 | 1 WRITE, 4,096 input bytes |
| `write-10m` | 10 MiB | 5,242,880 | 1 WRITE, 4,096 input bytes |
| `write-100m` | 100 MiB | 52,428,800 | 1 WRITE, 4,096 input bytes |
| `write-500m` | 500 MiB | 262,144,000 | 1 WRITE, 4,096 input bytes |

Each case has fresh virtual state and an actual Linux FUSE mount. The ioctl
request remains `LFR1` version 1, command `0x5020f541`, with 4 KiB inline
payload, direct I/O and inode invalidation. WRITE only measures a 4 KiB
FUSE carrier callback; it is **not** a semantic insert baseline. No suffix
READ/WRITE or ioctl callback may appear beyond the selected carrier call.

Use the test-only `WindowSource` from the first probe: LFT1 wall inside
two `getrusage(RUSAGE_SELF)` process CPU and `/proc/self/statm` RSS boundary
snapshots, with `CLOCK_THREAD_CPUTIME_ID` in a separate sidecar. CPU is shared
process CPU, not an exclusive whole-operation value; RSS is two absolute
endpoints, not a phase peak. Record full `docker exec` wall separately.
Source bytes have no storage backing; kernel/instruction/metadata caches are
uncontrolled. All results remain unknown-cache diagnostics, never cold PASS.
Do not pool different source identities, rerun a case to select a number, or
compare WRITE against ioctl as if WRITE performed an insert.
