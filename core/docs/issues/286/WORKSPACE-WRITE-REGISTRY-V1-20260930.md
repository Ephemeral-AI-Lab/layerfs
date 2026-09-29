# #286 Workspace write matrix: registered, not sampled

> **Status:** Prospective family-3 workload/oracle registry at source after the
> family-2 allocation diagnostic. All nine cells remain `NOT_RUN` until the
> family-2 checkpoint passes. This is not a speed or release claim.

The one selection is append, dispersed, repeated, in that order, each at 100,
512 and 4,097 one-byte writes. The public `WorkspaceApi` route creates a
Sandbox, mounts the prepared branch, executes the ordinary POSIX writer through
FUSE, commits, queries Status, unmounts and checks deletion. The external
driver launch-to-exit limit is **15 s** for every 100/512 cell and **25 s**
for every 4,097 cell; a separate independent verifier has **9 s**. No
cross-cell average or invented relative speedup threshold applies.

The fixed source is the closed 10,485,760-byte all-`A` master from #271's
`fourhop-patterns-prepared-v1`: C2 SHA-256
`20ea70dc9509e1819b11667bf97ab2a5ab3bd2b5d628d0a48f4c4fa6876691be`,
C5 SHA-256 `55a02d7da0c35b084b721706354dcaf6e30aee3f824ad32543cf2e108dbdfd19`,
and independent master proof SHA-256
`d2f535e0a405eca68746b7968350118b85ddbe69b95a66416a38238a7da5f6c0`.
The old oracle manifest SHA-256 is
`d664c30d37508679421ab4989c3256f5da430edda0c4dc4d148d863085249927`.
These are acquired and validated outside the timer; each later cell needs an
independent writable byte copy (`--setup clone` semantics), never a reused
mutated result. The copied source must meet the declared symmetric cold-source
contract before launch. The cache contract for the **Exec-to-Commit backing**
still needs an enforceable implementation; until then, an under-budget command
is numeric `INELIGIBLE`, never speed `PASS`.

The writer uses `O_APPEND` and `write` for append; otherwise `pwrite` at
`(104729 + i × 2654435761) mod 10485760` for dispersed or offset 5,242,880
for repeated. For every `i` from zero to count−1 the byte is `B + (i mod 24)`.
The independent read-only verifier reopens both old/new C1/C2/C5 heads and
checks every path, mode, size and byte against separately derived manifests,
the old Commit and new parentage, and the exact schedule. The donor
`verify_checkpoint5.rs` is an external example, not product source; the
product path has no SDK edit members for these cells.

The registry is [the family module](../../../benchmark/fs-bench-pro/families/workspace_write.py).
Its nine IDs have suffix `-v1`; the original #273/#248 case IDs and their
receipts remain historical. This registration changed no prior outcome and
does not grant permission to collect family 3 before family 2 passes.
