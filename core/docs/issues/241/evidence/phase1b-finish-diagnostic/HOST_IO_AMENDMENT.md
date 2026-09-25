# Phase 1B host physical-read observation amendment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Prospective correction before either live diagnostic attempt. The original
> [diagnostic contract](CONTRACT.md) remains unchanged.

The Core SDK benchmark runs its caller and Service on **macOS**; only the
mounted daemon runs in Linux Docker. The Linux `/proc/<service-pid>/io`
reading named in the original prospective contract cannot describe this
Service. At the new diagnostic identity, the release public-SDK driver reads
macOS `proc_pid_rusage(RUSAGE_INFO_V2).ri_diskio_bytesread` immediately
before and after its one LFT1 Exec→Commit operation. It records before,
after and checked delta in the existing driver receipt when
`LAYERFS_FINISH_DIAGNOSTIC` is set. The source is the same native platform
feature already used by the root benchmark's process resource observer.

This is a **process-wide physical-read counter**, not a `service.finish`
exclusive read count. Concurrent work in the caller/Service process would
confound it; the receipt must report concurrent work and refuse a narrow
attribution when present. LFT1 remains the only wall/CPU/RSS source. Missing
or decreasing disk counters are `UNAVAILABLE`, never zero. SQLite page-count
observations are likewise labelled at their actual post-run boundary; no
page count is invented for an individual finish substep. This amendment
does not relax the cache contract, source identity, one-sample rule or the
15 s complete-command and under-10 s verifier budgets.
