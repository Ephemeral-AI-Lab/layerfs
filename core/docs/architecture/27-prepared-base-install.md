# Prepared Workspace base install

> **Status:** Implemented S3 checkpoint after `8e2976e4e`; effective namespace merge and complete S3 remain unfinished.

Workspace keeps its selected immutable BaseView behind a short binding lock.
`base()` returns a retained clone; callers do content I/O after the lock has been
released. A clone copies fixed root identities and shares the initialized client/
immutable object cache, not the complete namespace or file data.

`prepare_base_install` checks Workspace route and old-base identity, then acquires
and validates one canonical next root with the existing scope. It does not scan
or validate the entire root closure, construct data or infer history success.
The upstream caller must already own the faithful saved/published root and exact
history outcome before attempting install. PreparedBase retains the exact capture,
expected binding and checked next BaseView; accessors do not authorize publication.

`install_prepared_base` checks that prepared input belongs to this Workspace and
that its expected immutable binding is still selected. It performs the existing
one-attempt Overlay install while holding only the local binding lock, then swaps
the prepared BaseView with no fallible work after known SQL success. No canonical/
transport I/O runs under that lock. A stale/cross-owner/uncertain refusal returns
both the original Workspace/Overlay error and prepared input. It does not resend
history or infer rollback. Unknown SQL outcome leaves the old selected binding and
retained prepared candidate; quarantine still prevents use as an effective live view.

Existing BaseRead and cloned BaseView retain exact immutable identities across the
install. New base access sees the installed root; later active overlay rows and
reply tickets remain unchanged. Independent overlay reader/orphan eligibility and
live-generation deletion remain S6. Current global cluster one has no content GC;
this pointer retention is not a future distributed retention lease.

Root preparation is fixed root-object/authentication work plus its real cache/
provider cost. Install reuses the S2 bounded metadata/index statements and their
EXPLAIN/runtime evidence; no new SQL query or payload-sized fold is introduced.
Native actor composition must carry this prepared root into the owner and preserve
the paired binding transition without canonical demand on the SQL service thread.
That actual daemon/FUSE/Commit assembly remains S8/S10.

External public tests build complete canonical roots with aliases/symlink/Git/
ignored dependency/cache/output names. A seven-byte captured cell agrees with the
constructed candidate; root preparation demands one object. Actual install updates
both selected IDs while old read bytes stay unchanged. Later engine metadata/ticket
retention is checked; effective later inherited-byte semantics are still S5.
Cross-Workspace prepared input and altered capture revision are refused with the
candidate intact. No full development workload, cache-cold timing or complete
Commit/runtime claim follows from this fixture.
