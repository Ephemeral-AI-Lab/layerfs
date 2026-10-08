# Read-service source review and retained failures

Build04/05 fails in the external fixture: HistoryProvider is not Clone. The
fixture now shares the exact original provider through Arc instead of cloning
or replacing it. No product contract or provider implementation was changed.

All selected runtime tests in08/09 and final functionality12/13 pass. Linux17
also passes every selected runtime, including actual daemon application within
its9s budget. The complete Linux check18 fails only at warning-denying Clippy:
ports.rs used map_err to retain and return the same Arc error. Change it to
inspect_err, retaining the same original Arc and failure-lock scope. No runtime
failure/timeout is suppressed; final receipts use a new source seal21.

Review additions before the final runtime source10: the reader lease retains
its exact Workspace and admission-to-grant wait. Explicit port-on-lease methods
refuse a grant from another Workspace before provider work. Poison is visible
in read-service observations and per-Workspace count queries refuse poisoned
state, so an untrusted count cannot establish drain. Source scheduler accounting
includes the pool/Arc header plus actual fixed vector capacities and reader
struct boxes, excluding provider caches/pagers/OS residency. No performance or
whole-process residency claim is made.

The ReadTicket Future parks only before a provider attempt. Sync port methods
remain compatible for control/constructors and must not be called by native
workers. Native integration must use the explicit ticket/lease path, preserve
bounded original demand grouping, release grants at each owned demand boundary,
and retain every result/custody guard through disposal. This checkpoint does
not claim the remaining Fuse/Workspace demand orchestration is implemented.

Host Clippy24 additionally identifies unused fields/methods in the shared test
fixture when included by the new test. The new fixture now checks its one-file
cardinality and uses the existing cleanup method from a last-dropped owner; it
does not suppress warnings or alter the shared fixture. Production source is
unchanged from21; source34 includes this external-test cleanup correction.
