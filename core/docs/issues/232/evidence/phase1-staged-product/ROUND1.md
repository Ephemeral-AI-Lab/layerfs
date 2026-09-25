# #232 staged product functional round 1

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> These are external functional checks, not public SDK benchmark attempts or
> latency admission. The staged product proof remains incomplete.

Product source `a1f745221121b27ed0d828bd50501282c154ddeb` uses locked
release host server/daemon and Linux test binaries. The
[prepared 64 MiB master](fixture-64m.json) was created once from an independent
byte copy of a closed schema 10 Store; every selected case used its own
writable Store copy and fresh history. Each receipt pins test source, binary,
product-input and image SHA-256, kernel, public/native test selection, result
and cleanup. All six passing cases used immutable Docker image
`sha256:9babee938c8c9c7a6678331c98f76a88a40594569b2f6a726c08d3677d836026`.
No case claimed a controlled Edit→Commit cache state, LFT1 operation wall or
public SDK route. The subsecond complete functional command walls below are
supervision only.

| Fresh case | Result | Complete functional command | What passed |
| --- | --- | ---: | --- |
| [Workspace stream](projected-stream-02/result.json) | PASS | 0.891 s | 64 KiB Bytes, delete, mixed Bytes/Zero, exact bytes and revisions, Commit and cleanup. |
| [Workspace Zero capacity](projected-zero-capacity-01/result.json) | PASS | 0.883 s | >8 MiB Capacity before mutation; exactly 8 MiB sparse Zero on a pristine Chunked base; boundary reads, EOF, Commit and cleanup. |
| [Mounted stage lifecycle](kernel-stage-lifecycle-03/result.json) | PASS | 0.942 s | BEGIN, wrong-order `EINVAL`, wrong-FD `EBADF`, private DATA, ABORT, post-ABORT refusal, unchanged STATE and cleanup. |
| [Mounted staged APPLY](kernel-staged-apply-01/result.json) | PASS | 0.987 s | Sixteen 4 KiB DATA frames yielded one 64 KiB edit/revision; pristine pre-APPLY reads, same-FD and alias size/read/EOF, Zero DATA run, canonical Commit readback. |
| [Inline variants](kernel-inline-variants-01/result.json) | PASS | 1.034 s | Frozen LFE2 malformed/stale/append refusal, overwrite, delete, later write and Commit regression. |
| [Inline insert](kernel-inline-insert-01/result.json) | PASS | 0.982 s | Frozen LFE2 insert, alias/fstat/EOF, one revision and canonical Commit regression. |

Every PASS receipt reports no retained container or volume. The source-changed
Workspace proof followed the retained
[replay-bound test failure](WORKSPACE-STREAM-FAIL-01.md). The mounted stage
proof followed the retained [empty test selection](KERNEL-STAGE-ROUTE-NOT-RUN-01.md)
and [cleanup Busy](KERNEL-STAGE-CLEANUP-FAIL-02.md). Those failed receipts
stay failed and were not used as passing proof.

The next product-level checks are stale staged stamps, bad digest, concurrent
quota, descriptor close, deadline and unmount disposal, 4 GiB arithmetic,
and exact 8 MiB through the mounted carrier. This round does not include a
public SDK Exec→ioctl→Commit route, independent benchmark verifier, cache
eligibility, or a Phase 1 performance selection.
