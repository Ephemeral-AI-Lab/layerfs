# External resource observations

> Status: implemented observation helpers and focused parser/custody proofs.
> E3 calibration, product phase coverage and numerical qualification remain open.

The existing harness now has
[`shared/evidence_resources.py`](../../benchmark/fs-bench-pro/shared/evidence_resources.py).
It follows the S7 E3 obligation in the
[current plan](../issues/307/IMPLEMENTATION-PLAN-S7-S13-20261007.md) and
[performance companion](../issues/307/PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md).
It supplies external observations for later driver integration; it is not a
new measurement runner, cache controller, resource authority or gate evaluator.

Each process observation records its actual monotonic read-open/read-close
interval and the reported clock implementation/resolution. Linux reads one
bounded procfs window per file, using unbuffered FileIO rather than several
buffered reads across a changing mapping walk. It retains `/proc/PID/status`
reported memory fields separately from `smaps_rollup` RSS/PSS/mapping totals,
and byte/call counters from `/proc/PID/io`. Two actual stat start-time reads
bracket the operation and reject an observed PID-incarnation change. They do
not establish future liveness, cross-boot authority or an application fence.
Permission, missing-file, malformed-unit and capacity errors retain their
original failure rather than falling back to another source or reporting zero.

The explicit cgroup-v2 reader records `memory.current`, supported byte fields
from `memory.stat` and per-device `io.stat` counters, plus actual directory
device/inode facts before/after the reads. Its scope includes the visible
hierarchy, descendants and any observer process in it. Neither a path string
nor a matching inode proves exclusive container/Workspace ownership; the
execution topology still requires an independent attestation. Overlapping
memory fields stay separate. Lifetime `memory.peak` and process VmHWM are never
read as substitutes for a measured phase peak.

macOS uses one `/bin/ps` call with an explicit two-second stop to record PID,
reported start time and RSS in bytes. Its start-time precision is retained as
such. It supplies no anonymous/file/swap decomposition or physical-I/O counter;
those fields remain unavailable. The child and observation costs require
inclusive accounting and calibration. A separate one-artifact stat observation
records actual logical length and `st_blocks × 512`, device and inode. It covers
neither other WAL/journal paths nor VFS/device I/O, and establishes no exclusive
allocation or continuous lifetime claim.

Input reads make one raw read and one close attempt. If both fail, the identical
original read exception is raised with the close exception retained separately
as `close_failure`. A close-only failure also remains an error. A bounded read
does not independently witness EOF or the full field/device inventory. Cgroup
observations explicitly list unavailable memory-stat fields; absence is not zero.

`Stream` creates fresh synchronous JSONL output and retains one observation
buffer at a time. A failed original read, serialization, write or flush stops
that stream. The acknowledged prefix and original exception remain available;
there is no output truncation, observer restart or automatic read/write retry.
Actual serialized records have a declared 65,536-byte admission window. This
is not proof of aggregate process memory, observer precision or a total flow cap.
Caller-held returned observations follow their actual caller lifetime.

The focused external
[tests](../../benchmark/fs-bench-pro/tests/test_evidence_resources.py) have
18 passing cases after matching syntax compilation in
[receipt 31](../issues/307/checks/e04-writes-20261007/31-resource-final-tests.json).
They cover units and exact
field inventories, overlapping subsets, lifetime-peak rejection, PID changes,
missing original paths, actual stat allocation, duplicate devices/counters,
bounded windows and terminal original failure with retained output prefix.
Earlier twelve- and thirteen-case helper evidence remains separate from the
rollup and raw-output extensions. The final raw-output negatives cover a partial
acknowledged prefix, no remainder replay and exact primary/secondary close
custody. The input read/close regression first failed in
[receipt 24](../issues/307/checks/e04-writes-20261007/24-resource-custody-regression.json);
that original failure remains retained beside its corrected result.

The actual macOS/Linux
[point-read checks](../issues/307/checks/e04-writes-20261007) exercise only the
observer Python process and explicitly read container-visible cgroup. They are
uncontrolled functional observations, sample count zero, qualification
NOT_EVALUATED. The reported clocks differ: macOS `mach_absolute_time()` reports
42 ns resolution; the pinned Linux image's `CLOCK_MONOTONIC` reports 1,000,000 ns.
These are source reports, not an attested conversion or precision guarantee.
No product phase, peak, cold state, service bound or exclusive physical domain
is qualified by those checks.

E3 still needs baseline/interior/final coverage of each actual product phase,
sealed process/container/artifact ownership, cross-clock calibration where
needed, complete ordered sample inventories, largest-gap/uncertainty checks,
observer-overhead controls and registered numerical limits. Exact missing
pager/dirty/journal/index/overflow/device/host-kernel domains stay unavailable
unless an owning contract permits a derived conservative bound. The E1 registry
and its 27 NOT_RUN proposals remain unchanged. E4 sustained service and eligible
debt drain retain their separate finite-workload gates.
