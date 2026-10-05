# Transient base-source windows and install readiness

> **Status:** Implemented S3/R5 checkpoint after `6e84b9181`; effective view/native/runtime and full R1–R8 acceptance remain unfinished.

Schema6 adds `base_source(ns,owner,base_root)` and a maintained Workspace
`base_readers` count. Acquire/release update both atomically. Tokens contain the
engine-qualified route, exact operation owner and selected root. Observation uses
the actual PRIMARY KEY; it never releases or replays an operation. Duplicate
acquisition, stale release, closed acquisition and cross-route custody fail.
Existing source metadata reads/releases remain valid during logical close.
Terminal ready eligibility includes zero pending sources, without an owner scan.

A source covers one bounded filesystem processing window, not an open descriptor
or Exec lifetime. Canonical demand runs outside the SQL owner. While it is owned,
the selected base cannot install; final overlay selection uses the latest local
metadata over that unchanged base. No mutable generation chain is pinned. Source
release follows actual completion/cancellation fencing; no Drop hides SQL/error.
Future independent orphan/read-plan custody must take over before a source ends.

Direct engine known install refuses active sources before changing state. The
actual fair daemon owner instead checks readiness before the one install attempt.
Its admitted install fences later source acquisitions in that namespace, drains
earlier acquisition jobs, and parks until exact releases. A sixth Source class
keeps new acquisition waits separate from existing Read continuations. Mutation,
Read, Lifecycle/release and unrelated namespaces remain runnable; no connection/
binding lock spans provider I/O. Lifecycle release wakes the original parked job.
There is no failed install retry or history inference. Native actor pairing with
the prepared Workspace base swap remains required integration work.

Production EXPLAIN uses base_source PRIMARY KEY(ns,owner). Correlated one-row
observation remains14 VM steps on macOS and13 on Linux beside128/1024/4096
unrelated owners, without fullscan/sort/autoindex/reprepare. The operation also
pays its route-state point check in the Workspace family; quoted source-family
work is not a whole-operation total. Count diagnostics are not latency/cold-cache
measurements. Acquire/release touch fixed rows/index state, O(log N) per window;
Q real pending windows use O(Q) backed state. The maintained count makes readiness
fixed point work. Six-class/fair queue traversal is bounded by configured W/J,
with result/argument credits retained; actual pager/journal/process/kernel physical
resources and delayed-install debt remain S6/S7/S8 acceptance.

Public engine/daemon tests prove exact selected root/count, duplicate/refused
atomicity, capture allowed while a source exists, direct refused install retaining
capture, close/last-release cleanup eligibility, lost result custody, finite source
fence and progress of existing reads/mutations/unrelated state. Final core571 and
Linux overlay/daemon/Workspace30 tests pass with prescribed checks; raw logs are
in [s3-source](../issues/307/checks/s3-source/core-test.log). These prove service/
custody primitives, not actual provider-delay/native dispatch or complete view merge.
The [selected source contract](../issues/307/S3-SOURCE-FENCE.md) records the scope.

Acquire/release use the same runtime SQL templates as their operational plan
diagnostics: VALUES insertion has a finite VM program with no row loop; counter
updates seek workspace INTEGER PRIMARY KEY and deletion seeks base_source PRIMARY
KEY. Live acquire/release including BEGIN/COMMIT, all Workspace route/count checks
and exact source reads cost99/123 VM on macOS and94/117 on Linux, changing2 rows
each with no fullscan/sort/autoindex/reprepare. Closed last release additionally
pays the declared existing readiness/reclaim statements. Final covering logs are
[host](../issues/307/checks/s3-source/core-test-covering.log) and
[Linux](../issues/307/checks/s3-source/linux-test-covering.log); prior passing
source-only logs retain their scope before this diagnostic coverage extension.
