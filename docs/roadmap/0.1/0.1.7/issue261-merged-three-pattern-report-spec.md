# Issue 261: merged-source three-pattern reporting selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before changing the existing runner's source allowlist or taking a
new sample. The product baseline is combined #266 → #265 merge commit
`6bcfa464f74ae9ca3859df31c678985ec69ba098` on draft #262. The
[#261 three-pattern specification](issue261-three-pattern-100-spec.md) remains
the workload contract: one public Mount → one generic one-process/one-fd
shell Exec → one explicit Commit, then an independent full old/new-head
oracle. The selections, in order, are 100 true EOF appends, 100 dispersed
one-byte edits to one 10 MiB file, and 100 repeated one-byte edits at one
position. The old master, writer, offsets, bytes and expected final content
are unchanged. This is a distinct combined-source diagnostic, not a rerun of
the #265 branch's passing identity.

Reuse the closed, independently verified #265 prepared master whose source is
`7221e177be9ebbe0baf4d51e8f7067a7dc66b37e`, prepared-record SHA-256
`f9944a5c4f689d920277738d6b708859549287bec032e44e6d6cf5d0bcaf0191`.
Read it without modifying its owner's worktree, make one independent writable
byte copy of its Store/history into this worktree, and verify hashes before
each attempt. Rebuild affected SDK, verifier and daemon binaries from the
combined product with locked Cargo release flags in this worktree. The
existing runner may accept only the reviewed #266 product-source paths added
since the master; do not bypass its other seal and source checks.

Take one labelled attempt per pattern at one frozen source/harness/image
identity. Retain every row, including FAIL/INELIGIBLE; no repeat to select a
better wall. Preserve the ordinary 15 s complete-command and separate 9 s
verifier limits, one construction worker, four 25-write backing checkpoints,
the same cache policy and clone method. Collect actual FUSE WRITE callbacks,
Exec/Commit/complete raw walls, independent oracle, private payload and
metadata allocation, ledger/page/extent counts, Store size, sampled host RSS,
consumer charging, and positive cleanup. Report raw timing as
cache-`INELIGIBLE`, never as a qualified before/after speedup. Keep #271's
later 2,048/4,097 FAILs separate from this merged-product profile.
