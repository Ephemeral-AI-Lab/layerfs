# Issue 271: 32-child branch 4,097 gate failure

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The frozen product source `94d86303ca6748a33b5a8bf7ce012ef9325646d0`
keeps 124-record leaves and packs new branches to 32 children. Its native
extent suite passed 36/36, including a read and edit of an older full
248-child branch. The 100/512 separated trees have three/sixteen leaves, below
this branch target; their earlier passing combined-source checks were not
resampled. The [prospective #271 specification](../../../../docs/roadmap/0.1/0.1.7/issue271-root-edge-workload-spec.md)
declared one final public 4,097 attempt when meaningful. This source had one.

The existing release `benchmark_shell` SDK driver and independent master were
reused with sealed byte-copy clones. Only the changed daemon and Docker image
were rebuilt. The same generic one-fd writer, one public Mount → Exec →
Commit route, one construction worker, cache policy and **25 s** complete
command exception remained in force. The [raw receipt and postmortem](evidence/branch32-gate-fail-v1/)
pin the source, binary, image, fixture, command and cleanup identity.

The complete command expired at **25.005652 s** (`FAIL`) before the driver
returned any receipt, actual WRITE callback count or writer progress. It made
no observable Commit or independent verifier result; product cleanup was
`FAIL`. The owned daemon later logged `WorkspaceExec` error after
**29.106921 s**, with `daemon.exec_output` taking 29.090666 s. The 30-second
product Exec timer remains #249's separate issue and is not a benchmark
allowance. After the host timeout, the owned container remained alive and
eventually logged `sandbox shutdown retained: Busy`. Its deliberate stop
ended with exit 137; its container and named volume were then removed, with
no new owned container or volume left. That postmortem removal is not a
custody or clean shutdown PASS. No source-specific 4,097 retry or larger
deadline was used.

The failed row has no FUSE callback count or final branch/ledger checkpoint,
so it cannot isolate the remaining cost by itself. It disproves gate admission
at this identity. The earlier 32-record/eight-child [count regression](PACKING-32-8-REJECTED.md)
also rules out shrinking both levels as a simple solution. Further diagnosis
needs a prospectively declared count between 512 and 4,097 that crosses the
32-leaf branch transition, using the same public writer and unchanged
ordinary 15 s command budget. A new algorithm or harness source requires a
new identity; this failed arm is retained rather than rerun.
