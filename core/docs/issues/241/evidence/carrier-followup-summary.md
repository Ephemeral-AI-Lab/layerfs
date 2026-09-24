# #241 Linux ioctl carrier follow-up — 2026-09-24

## Decision

**Continue with ioctl as the Linux carrier candidate.** The mounted probes found
no provider delivery blocker, no size-proportional callback work, and no
diagnostic carrier wall miss against the frozen 1.0 ms screen. The later probes
also resolved the original `cpu_shared_ns:null` *sampling* gap with test-only
boundary LFT1 observations. None is a product Edit→Commit or cold-cache
latency result.

**Do not treat Phase 1 as fully signed off.** The failure probe found that
`fuser::Notifier::inval_inode` can return `Ok(())` after unmount, and the
daemon's `ReplyIoctl::ioctl` does not acknowledge caller receipt. Those are
acknowledgement-contract limits, not a reason to abandon the ioctl carrier or
optimize its observed wall time. Product success/uncertainty, live mounted
coherence, and caller readback still require implementation and proof.

| Question | Evidence-backed result | Remaining boundary |
| --- | --- | --- |
| Does the Linux kernel deliver the provisional 4 KiB request? | **PASS:** the [initial probe](ioctl-probe-report.md) and follow-ups observed one exact 4,128-byte callback for each ioctl and no suffix READ/WRITE callbacks. | This does not imply a 64 KiB request or product Workspace behavior. |
| Can a length-changing request be immediately visible? | **PASS for tested positive path:** direct I/O plus inode invalidation gave old-FD, second-FD and alias size/mtime/read/EOF coherence and a later write. | Caller confirmation on the actual product route is untested. |
| What happens on a real read-only mount and a closed descriptor? | [Failure probe](failure-probe/REPORT.md): `ro` mount still forwarded ioctl; the test adapter returned `EROFS` before mutation. Closed FD gave kernel `EBADF` after RELEASE with zero ioctl callback. | Product must check mount writability and handle authority itself; callback stale-handle lifecycle was not reached by an ordinary closed FD. |
| Can a published request lose its reply? | [Failure probe](failure-probe/REPORT.md): after writing and verifying a test-only 12,288-byte accepted state, deliberate daemon exit before reply gave caller `ECONNABORTED` (103). No retry; raw attempt retained. | This is test-only custody, not Workspace publication or durability. Any post-publication syscall error must be treated as uncertain. |
| Does notifier `Ok` certify client-visible coherence? | **No.** A real `inval_inode` call returned `Ok(())` after unmount. `fuser` 0.18.0 maps `NotFound` on invalidation send to `Ok`; raw underlying errno was not captured. | A live post-publication notification error was not induced. Current Core `CoherenceStatus::Ready` from notifier `Ok` is provider-side state, not a caller acknowledgement. |
| Can LFT1 capture sub-millisecond CPU? | [CPU probe](cpu-probe/REPORT.md) and [six-case follow-up](cpu-probe/FOLLOWUP_REPORT.md): test-only boundary samples gave caller/daemon LFT1 `samples=2`, `gaps=0` and nonzero process CPU at virtual 1/10/100/500 MiB; 4 KiB WRITE controls also have valid CPU rows. | CPU is process-shared, 1 µs quantized, not exclusive operation cost. RSS is only two absolute endpoints. The two ioctl selections have different frozen binary identities and are not pooled. |
| Does uncontrolled cache defeat this carrier finding? | **No for functional/count feasibility.** Virtual bytes have no backing source pages; the existing Darwin 100k Init [cold contract](../../../../../benchmark/fs-bench-pro/shared/cold.py) does not apply. | Observed walls remain diagnostic and numeric cold latency admission is `INELIGIBLE`; fresh mounts do not establish a globally cold kernel or instruction cache. |

The CPU follow-up's caller ioctl LFT1 wall observations were 71,917 ns (1 MiB)
and 64,000 ns (500 MiB) under its first source identity, and 224,750 ns
(10 MiB) and 88,959 ns (100 MiB) under a separate prospective identity.
The higher 10 MiB row was retained, not resampled or explained by a size
trend. All four are below the diagnostic 1.0 ms screen, but none is a cold
latency PASS. The 4 KiB WRITE controls are transport controls, not semantic
insert baselines. Exact CPU/RSS values, callback counts, command walls, source
and binary hashes, Docker identities, commands, and append-only raw files are
in the linked reports. The initial probe's raw attempts were never rewritten.

## Minimal truthful success rule to prove in the product

1. Validate ABI, mounted writability, handle/version authority and admission
   **before** mutation. A refusal there is definite.
2. Publish once with a retained mutation receipt, notify before the ioctl
   reply, and never replay an unknown operation automatically. A failure after
   publication is uncertain. The test-only lost-reply attempt demonstrates why
   a caller error alone cannot mean rollback.
3. Let the cooperating caller report success only after ioctl returns zero
   **and** bounded same-FD `fstat`, boundary and EOF readback match the
   expected size/mtime/bytes. Keep alias coherence as a mounted regression
   test. This caller confirmation is a proposal, **not yet a measured product
   route**; its work and latency belong inside the eventual Edit→Commit
   operation boundary.

`fuser` [suppresses `NotFound` for invalidation](https://github.com/cberner/fuser/blob/v0.18.0/src/notify.rs#L107-L121)
and its [ioctl reply method returns `()`](https://github.com/cberner/fuser/blob/v0.18.0/src/reply.rs#L651-L655).
Linux's [reverse inode invalidation can return `ENOENT`](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L3025-L3043).
Core currently [derives `CoherenceStatus::Ready` from notifier result](../../../../crates/layerfs-workspace/src/runtime/coherence.rs);
that result must not be presented as end-to-end caller acknowledgement for
#241. These source facts explain the observed return behavior but do not
claim that the underlying errno of the after-unmount attempt was captured.

All new probes used published `fuser` 0.18.0, locked release binaries and real
Linux Docker/FUSE mounts. They added only test source and append-only evidence;
no product `src/`, benchmark workload/registry, third-party package or #232
receipt changed. The Core test suite, boundary checks, formatting and host
Clippy passed in each probe worktree. Linux-target Clippy stopped before these
tests on the pre-existing `layerfs-workspace/src/backing/segments.rs:192`
`clippy::useless_conversion` finding; the Linux tests compiled and ran in
locked release. Production LOC stayed 119,981 combined (Core 54,564;
reference 65,417) across these test/docs-only commits.
