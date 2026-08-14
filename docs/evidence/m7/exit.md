# M7 candidate exit record

- Candidate commit: `ce9035e49037f60a8c52d2775fd2d88d34e57cd4`
- Candidate parent: accepted M6 evidence commit
  `891fd0691a824144dde9adb469d5c480325ace6a`

<!-- prettier-ignore -->
- Sequential predecessor: accepted M6 candidate `082f4e98711035c2be2bd7d2f668f6c23e7a5b16`
- M7 status: passed
- Latest accepted milestone before this evidence commit: M6

## Exact candidate validation

The clean candidate passed `pnpm validate:m6` in 1,090,158 ms. Its accepted Node target
completed in 540,853 ms and its faithful-local M6 target completed in 545,257 ms, each
below its independent 600,000 ms deadline.

`pnpm test:m7:local` passed all 23 Node VFS tests. The selected
correctness/fault/resource target completed in 85,549 ms and the complete local gate in
90,271 ms, below the 600,000 ms selection deadline. Coverage included the adversarial
namespace, empty-create, ambiguous-commit, inode-metadata, metrics, multi-edit COW, and
caller-allocation regressions; seven shared file-backed cases; all 36 three-session
commit/close orders; exact 1/16/64-session pressure; 4/8/16 KiB persisted COW formats;
process restart; retryable close; and every observed SQL position in separate
152-position staging and 203-position visible-commit fault phases.

The 100 MiB fixture retained SHA-256
`dbd3abb6b32a319a2156c5312956281c6939d950f823eb6f7e039eaf4e9d0435` after 1,000
deterministic one-byte overwrites. Managed resident memory peaked at 102,983,960 bytes
below the 134,217,728-byte aggregate limit. The default 64-session pressure case peaked
at 78,556,130 bytes while admitting the exact 67,108,864-byte resident boundary.

## Real mounted-FUSE proof

`pnpm test:m7:fuse` ran from a clean checkout of the same candidate on WSL2 Linux. It
opened the real writable `/dev/fuse`, selected `/usr/bin/fusermount`, and recorded four
kernel mount cycles across four distinct provider PIDs and exactly three restarts.

The exact operation-count profile wrote and recovered a deterministic 16 MiB fixture,
observed 5,000 control-delimited mounted one-byte callbacks, performed 2,000 namespace
operations, ran 16 readers and 16 writers for 64 operations each, and completed 9,056
counted operations. Its edit window used one successful optimized COW flush with no
failed flush, while total source work remained below the fixture plus the profile's
524,288-byte workload ceiling. The final payload digest was
`3238fa53923434d162289488f802739eecc4a45303799b7ca4c4b38fddba5d1a`.

The smoke separately proved fsync across abrupt provider death, close durability,
interrupted/resumed collection, a fresh completed final collection, full integrity and
usage verification, zero active leases/staging/reservations, and final unmount. It
completed in 24,767 ms and the gate in 24,815 ms, below 60 seconds. Managed resident
memory peaked at 59,762,873 bytes below the 134,217,728-byte aggregate limit.

The candidate deliberately retains `validate:accepted` on M6 while these gates run. This
evidence is its atomic direct child; a following constrained acceptance commit may
select M7 only after the verifier accepts this record.

## Log integrity

- `predecessor-m6.log`:
  `c61c7d01c2959f0c89d21a8eb51fb7d5a81ccb7e0f11fd0e5990b488fd3c618c`
- `m7-local.log`: `084c340cca0861f5024eecd91002d617127e1b86c1cb4487ef5701da82304529`
- `m7-real-fuse.log`: `c7f62044bc39267568d2ca50aae00999a13352c3361120538cc2435e1be5b180`

All three logs identify the exact candidate. The machine-readable artifact owns the
authoritative commands, environments, capabilities, limits, seeds, counts, timings,
resource peaks, real-FUSE identities, batching proof, and log hashes.
