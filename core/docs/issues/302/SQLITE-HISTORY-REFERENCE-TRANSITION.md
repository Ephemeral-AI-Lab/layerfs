# Phase4.5 history reference vehicle and transition diagnostics

> Status: functional root/inventory equivalence for two states. Full stride10
> reference count diagnostic FAIL_COMMAND_BUDGET. All history speed gates NOT_RUN.
> Product unchanged from b2d67754c; benchmark vehicles modified/dirty for diagnostics.

The reference vehicle is generated from the shared `benchmark_history` producer
by `diagnostics/history_reference_vehicle.py`. Exact, single-occurrence source
seams replace engine creation, public reader/save entry calls, C5 handle access
and profile-specific checkpoint/close. The filesystem construction, changed-byte
predecessors, advisory selection, canonical collector, inventory and retained-C5
logic are shared. A seam that moves fails generation. The original baseline is
7edddbdb8e8512627aed0ed42533ef099d802384, unmodified/clean before and after.
Its temporary SDK example is removed even on failure; the release/locked build
uses its own target and no manifest, dependency or production edits.
The baseline keeps native C2 plus native C5 databases and original MEMORY/OFF;
the candidate keeps shared durable SQLite/WAL/FULL. No profile weakening occurs.

New `probe-transition` mode executes two states, explicitly DIAGNOSTIC. Both
vehicles record a distinct canonical inventory (`id -> role/canonical length`)
and a SHA256 digest over sorted fixed-width identity, role and little-endian
u64-length tuples. That map is a harness owner proportional to emitted identities,
not a production bounded-memory claim. No canonical payload copy is added.
The inventory describes constructed/offered canonical identities; it is not a
physical allocation count or proof of every persisted locator after failure.

## Two-state functional evidence

Both arms completed exactly states1/2 (full157 indices1/11), retained by actual
C5 initialize/fork and then fork/stage/commit/add-layer operations. Both roots:

- ff484f8d92f57413ee83e7201fb04c8512ffd7bed3083471713db4e8e17c70d3
- 2906f3669420afb3ec336bc5d87856328371db4e9ad644ced4224cc8d3cd75ab

Both offered1,447 distinct canonical objects /8,207,253B; inventory SHA256
6e93471da148f2bfc36938d217acdc590bc42898699b93b4547299a6fc0dcb64.
These are independent pinned-baseline equality observations for this prefix,
not full17-state root pins or full O3 admission. The separate candidate verifier
reopened read-only and checked both custody states, all1,502paths/kinds/sizes,
and118declared content samples /846,229authenticated logicalB.

| Diagnostic | Complete command ns / declared bound | Disposition |
| --- | ---: | --- |
| reference release/locked build | 1,934,823,292 /30,000,000,000 | PASS; baseline clean before/after |
| reference two-state producer | 2,029,935,958 /25,000,000,000 | DIAGNOSTIC, two states/custody roots |
| candidate two-state producer | 1,078,537,416 /25,000,000,000 | DIAGNOSTIC, same roots/inventory |
| separate candidate verifier | 1,532,000,583 /9,500,000,000 | DIAGNOSTIC; full two-state tree, declared byte sample |
| reference full17-state count diagnostic | 25,014,771,375 /25,000,000,000 | FAIL_COMMAND_BUDGET; watchdog SIGKILL, no final stdout/inventory |

All producer caches are uncontrolled original corpus read-probes. Numeric speed
eligibility is false. Their lifecycle clocks466,186,625ns reference and
412,564,250ns candidate must not support a speed ratio: acquisition alone is
233,303,792/94,788,375ns, and initial/successor cache conditions were not enforced.
The prefix clocks do not predict full-case time. The separate verifier excludes
its wall from product timing. Per-child lifetime wait4 RSS is67,076,096B reference,
65,126,400B candidate,20,496,384B verifier; no phase-only/whole-importer bound.

## Count-driven next mechanism

Candidate's two-state actual SQLite counters:1,978statements,161,159VM steps,
503transactions,65write commits,0rollbacks. C2 made12reservations and
46publications,53immutable body INSERTs /2,284,905sealedB,13read-packs calls,
244locate calls,5value-group calls and11ordinal reservations. SQL COMMIT spans
accumulated102,373,603ns; their scope includes these logical commits, not observed
physical sync counts. Save/C5 stage215,169,917ns also contains overlapping work.

Source `save/select.rs` acknowledges any candidate with a private framed location
before using it as a physical base. That is a first-writer race/depth safeguard:
a concurrent winner may have a deeper representation than the private one.
It can force `register_ready` within an admission wave, in addition to normal
wave/final publication. These counters make publication/acknowledgement frequency
an actionable next mechanism. They do not justify deleting the safeguard,
assuming every publication has the same cost, or changing candidate selection
without checking physical storage consequences. No product optimization was
applied in this tooling/evidence round.

## Full-reference budget failure

The count diagnostic prospectively declared25s (diagnostic-only exception),
17states and expected full O3 inventory51,689objects /380,559,460B. It printed
15published-state records, through full157 checkpoint141, before termination.
Neither the17root vector nor final inventory exists. The final recorded root
is3728f7dbc9b8276322b1c92d3db550bf95d46b27e74e14a9bc13ca740110689f.
The partial Store/scratch and logs remain on disk. Its wait4CPU19,602,698,000ns /
lifetimeRSS237,895,680B are diagnostic observations only. There is no completed
cleanup/checkpoint or storage admission claim for that killed command.

Do not retry this unchanged full arm, extend its deadline, use these partial
roots as the complete reference, or count the diagnostic as the required speed
sample. The budget conflict is now evidenced; original60/170s history allowances
remain superseded/unresolved against current15s/declared25s policy. There are
still useful independent Init optimization steps; the all-seven goal remains
ACTIVE rather than declaring blocked or complete.

Raw append-only folders: `issue302-history-reference-vehicle-build1`,
`issue302-history-transition-{baseline,candidate}-probe1`,
`issue302-history-transition-candidate-verifier-probe1`,
`issue302-history-stride10-reference-count-diagnostic1`, and
`issue302-history-transition-comparison-diagnostic1` under owned
benchmark-results/fs-bench-pro. Compact receipts/prospective/manifest/error files
are retained under this issue's checks/sqlite-history-* folders. Generated source
and shared source hashes are recorded with the build; executable SHA256 archives
are named in receipts. Both worktree run locks were held, report contract read
before each invocation, no overlapping build or product sample.

Checks: candidate release/locked builds and unchanged-baseline release/locked
adapter build PASS; scoped warning-denying Clippy for both candidate examples
PASS; Core fmt --all --check PASS; exact API seam generation remains valid after
formatting. Prior b2d67754c production-boundary438/self-tests23 proof covers the
unchanged product tree; no production/full workspace test rerun or full history
proof is claimed. Full speed/root-pin/cold/budget qualification remains pending.

Production LOC:137677->137677(delta+0), root reference65417/core72260,
active28216/inactive reference44044; migration old191/new7806/rest64263.
Method: exact first-parent/final staged snapshots via existing
`target/phase7-agent/commit_loc.py` and `tools/production_loc.py`, excluding
examples/harness/evidence; confirm committed tree before handoff.
