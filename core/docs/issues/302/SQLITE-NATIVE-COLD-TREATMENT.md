# Native cold-contract treatment

Status: Cold qualification passes; competitive Init outcome unmeasured.
Based on947c0ef49. Full100000-file Python count diagnostic took11970979333ns
on already-nonresident contents; residency200000/path-stat200000 and198000
mmap/mincore/munmap sequences. No product speed claim. The repeated traversal,
Python/ctypes/object work and page-size queries are avoidable mechanism overhead.

A first-party C harness helper performs the same mincore->conditional msync
invalidation->immediate mincore, followed by a separate whole-input mincore pass.
It obtains host page size once, walks deterministically without following links,
keeps one payload descriptor/mapping at a time and reuses only the residency byte vector.
It never reads/touches payload pages. Inode/device/size and final count/byte/page
checks refuse inventory changes; input is the closed identity-pinned fixture.
Vector and helper child RSS are counted separately from product memory. No claim
about cold filesystem metadata or full concurrent-mutation safety is introduced.

Native build is outside the performance envelope, sealed by source/flags/clang
identity/platform/architecture and binary SHA; immutable archive is reused through
its seal. Both arms run identical helper binary/cache rules. Its source is part
of harness identity. All invalidation and attestation remains inside the complete
performance envelope. The product gets only the remaining15s deadline. No silent
fallback or enlarged deadline. Independent verifier remains separate<=9.5s.

Actual tests: warm invalidation matches existing primitive, already-nonresident
pass, empty/non-directory/symlink/FIFO refusal and source/binary seal refusal.
Three native testsPASS; four registry/arithmetic/wait4 testsPASS.
C builds with Wall/Wextra/Werror. No product source changes; prior product owning
checks are retained, not repeated. Core fmt/boundary/23 self-tests checked.

Prospective next: one count-only full100000 fixture diagnostic, no product sample,
<=15s; then one new matched100000-file Init-v2 pair if the measured mechanism
leaves budget for the product. Do not substitute this diagnostic for speed proof,
credit its cache state to a later sample, or rerun an unchanged failed arm. Other
cases/history remain required; older receipts retain original identities/status.
