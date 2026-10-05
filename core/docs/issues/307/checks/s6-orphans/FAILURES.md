# S6 independent custody failure ledger

> Append-only correctness/work diagnostics on the dirty checkpoint tree, dated
> 2026-10-06. These are not latency/cache or release qualification runs.

- `build-integration-1.log`: overlay built with an unused import warning; removed
  unused `SERIAL_RETIRE` import in the orphan maintenance module.
- `build-integration-2.log` and `-3.log`: Workspace immutable-root rebinding passed
  a byte array where the owning API uses `ObjectId`; fixed checked conversion and
  `to_bytes()` at the boundary. `-4.log` built the integrated file/captured ports.
- `build-integration-5.log` and `-6.log`: new exact `Changes.open` field required
  explicit `None` in namespace and compound fixtures. Added it without changing
  the effects. `-7.log` retained a faulty mechanical insertion into a function
  signature; corrected it. `-8.log` built successfully.
- `build-owner-1.log` and `owner-orphan-first.log`: the canonical metadata fixture
  needed a checked u32-to-u16 mode conversion. The attempted test did not execute;
  it repeated the already-failed compiler result instead of waiting for a successful
  build. This violated the build-first procedure; subsequent tests run only after
  a successful `--no-run` build. `build-owner-2.log` compiled the corrected test.
- `overlay-first.log`: the former 32-page quota could not initialize schema v10.
  The failure is at startup, not the selected mutation. The explicitly configured
  v11 fixture now uses 64 pages and still proves definite SQLITE_FULL rollback,
  prior publication/capture preservation, and unrelated-Workspace isolation.
  This fixture change does not turn the original 32-page startup failure into PASS.
- `overlay-final-custody.log`: targeted unlinked cleanup adds one maintenance row
  to the compound transition. The prior seven-row assertion now accounts for
  eight actual changes. All three unrelated namespace scales kept equal work.
- `overlay-custody-repaired.log`: trusted legacy scratch cleanup used target zero,
  rejected by `maintenance.target<>0`. Assigned its distinct -2 domain; minted
  scratch uses target 1. No schema constraint was weakened.
- `build-operation-2.log`/`owner-orphan-built.log`: duplicate scratch class match
  emitted an unreachable-pattern warning. Removed the duplicate, preserving the
  class and charge handling. The owner orphan proof itself passed.
- `overlay-custody-plans.log`: a reused fixture's prior file added an index neighbor
  at the read range end (196 vs 197 VM steps). The compared input histories were
  unequal. The repaired profile uses a fresh closed fixture and identical file
  serial/content/state at each unrelated-owner scale. Final 128/1024/4096 counts
  are retained without relabelling the failed run.
- `daemon-custody.log`: one daemon fixture still asserted schema 9. Updated the
  startup identity assertion to 11. Eight other daemon tests passed in this run.

No test reached the 120-second ceiling in this checkpoint. Original paused S5
hang and its correction remain in the S5 immutable failure ledger. No failed SQL
operation or automatic maintenance operation was replayed. Source review also
fixed maximum-serial cursor overflow and retained the namespace tombstone while
an independent orphan holds its old source; covering boundary evidence follows.

Source review also added the missing closed-state refusal before minting a new
captured reader. Existing sealed readers remain usable after logical close and
retain terminal cleanup custody. The final close proof covers both sides.
