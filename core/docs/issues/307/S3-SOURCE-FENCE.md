# S3/R5 base-source lifetime and install readiness

> **Status:** Current selected first-party implementation contract; engine/service primitives implemented and proved after `6e84b9181`; effective-view/native composition remains in progress. Not completed S0/S3/S6 or native qualification.

Effective lookup/metadata demand must run outside the SQL owner. A canonical
lookup begun against base A cannot combine its answer with an unrelated base B
installed while that demand was pending. Keeping a binding lock on a caller while
waiting for an owner job can deadlock the owner behind install; holding the SQL
owner across demand blocks unrelated work. Neither mechanism is selected.

Use one backed, exact base-source lease for one bounded filesystem processing
window. Acquire returns the selected immutable root and a minted namespace/owner
capability; the Workspace retains that BaseView. Canonical demand occurs outside
SQL. Final overlay point/plan selection checks the exact still-owned source and
uses current overlay values over the unchanged base. Explicit release follows
known request completion/fencing. Lost replies and cleanup errors retain exact
lease custody, observable by its original operation ID. No Drop performs hidden
SQL or guesses that an operation ended.

Known install is not attempted until existing source leases finish. The fair owner
parks that original job at a readiness boundary, fences later source acquisitions
in its namespace and allows existing reads/releases, ordinary mutation windows and
other namespaces to proceed. Earlier admitted source acquisitions belong to the
finite frontier. A separate acquisition class avoids blocking continuation reads
behind new source waits. No failed install is retried. Direct engine install refuses
active source leases before its transition; native callers use the owner readiness
path. Capture can still seal while base sources exist because it does not change
that base. Known upstream publication with pending local install retains its exact
candidate/capture; it is not reported as unpublished history.

The lease delays one base-pointer install; it does not pin a mutable generation
chain or make an open-unlinked file own a Workspace's generations. After a bounded
read plan has copied its selected overlay bytes and retained its exact immutable
file source, that source lease ends. Open/lookup/orphan ownership then belongs to
the independent S4/S6 custody model. Exec/file-descriptor/command duration never
becomes a base-source lease lifetime. No Bash timeout is introduced. Individual
provider/request cancellation requires an actual fence, not elapsed-time inference.

Engine state: a namespace-leading source-owner table with32-byte selected root,
plus a maintained reader count on workspace. Acquire/release update both atomically;
ready install observes the point count rather than scanning owners. Logical close
revokes acquisition but accepts exact existing reads/releases. Terminal deletion
is eligible only after sources/captures/other owners/reply tickets end. All hot
queries require production-template EXPLAIN and correlated DB counters.

Cost: O(log N) indexed acquisition/observation/release per actual source window,
fixed metadata/capability output and O(Q) backed rows for Q real pending windows.
No namespace mirror, full copy or per-orphan install loop. Queue windows/bytes
remain aggregate admission, not total filesystem/flow/time limits. Delayed install
and real physical/canonical/queue work remain visible. Native dispatch/runtime
provider fairness, independent orphan composition, headroom and aggregate resource
qualification remain separate required evidence; this contract alone proves none.
