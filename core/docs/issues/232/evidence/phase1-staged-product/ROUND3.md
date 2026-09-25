# #232 staged ioctl sparse 4 GiB boundary

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Functional size-arithmetic proof, not a performance sample.

At source `5bdde8480d13fb3d3a46c074aecf2baeada0649f`, the locked release
[mounted test](kernel-staged-max-file-math-01/result.json) passed on an
independent copy of the closed [64 MiB master](fixture-64m.json), using the
same published `fuser` and immutable Linux image as [round 1](ROUND1.md).
It created a new file and set its **logical** length to exactly 4 GiB using
Workspace's Zero-piece size operation. No 4 GiB payload was allocated,
written, read or timed as an edit. A version-3 BEGIN whose offset plus
deletion overflowed `u64` returned `EINVAL`; a one-byte append that would
produce 4 GiB + 1 returned definite `ENOSPC`; and a one-byte replacement
of the last byte with a 4 GiB result admitted a private stage. ABORT consumed
that stage. STATE remained unchanged. The test unmounted, removed the
temporary file, then committed the remaining namespace so no 4 GiB content
construction was used as a shortcut or performance row. Clean close and
container/volume cleanup passed. Complete functional command wall was
0.885 s, reported only as supervision.

This establishes the checked 4 GiB result boundary through the live mounted
carrier without a large data copy. It does not claim 4 GiB replacement
support (the replacement cap remains 8 MiB), a 4 GiB Commit latency, or a
public SDK benchmark PASS.
