# Family7 SDK count/custody diagnostic r076 —cause recovered, gate remains FAIL

**Status: Dated count-driven diagnostic; not another speed/gate sample.**

Source0b2cf37aa, unchanged1025 workload/25s bound, one worker/default quotas.
[Compact counts/receipts](20260930-workspace-shell-package-sdk-cause-r076-receipts.json).
Complete25010761125ns at the25s bound, row FAIL; interim Exec8842957917ns and
Commit9575685000ns are diagnostic intervals. Exact typed result is now retained
before cleanup: **Reconcile /KnownCommitLocalFailure /Capacity**, known and
observed canonical outcome agree, installed_revisionNone. Public post-status:
1044resident/dirty identities,1347866consumer-accounted B,6255lookup/9385getattr/
2068write/36readdir/18other callbacks,2053upstream calls. No result replay.

C5 counts:1044dirty rows,2050deletion keys/45100B;1036212B map charge,
4153transferred updates/199344B ordered scratch/506683B buffers. Index scratch
changes131072→1169339B; recorded budget2746834B before resize, calculated
post-resize3785101B (`2746834+1169339-131072`), below8MiB. This is recorded
admission arithmetic, not an asserted full phase peak. Compaction sources0.

Separate public read-only failure-custody verifier **PASS**641885125ns<9s:
old17paths/9files/1114501B→new1060paths/1034files/1121591B, every byte/mode,
head1224bb12152e034a31c8b2f865b1eb9869346130289ced7f240679ddb0452e6b57
and exact parent, unchanged core/index.js identity. It proves canonical
publication already succeeded; it cannot promote local reconciliation/cleanup.
Matched stopped container and all private files/logs are archived before
external removal; no graceful-close PASS. Original r071/r073 outcomes unchanged.

Source-backed hypothesis: the active C5 publisher spends the fixed208-page
completion escrow using `fund.take`, while the candidate rewrite grows with
4153changed keys.192512B of retained result pages already spend that fund; active
private index alone occupies1196032B. Memory admission is below its limit, and
failure is inside index preparation; the fixed fund can refuse a page despite
available configured disk quota. Test one narrow change: each C5 page credit
explicitly reserves any missing amount from **the same** physical quota before
page creation, then deducts once. No attempted fixed-credit failure/fallback,
quota or memory lift, new worker, retry, codec or streaming refactor. Initial
prepaid escrow and all normal small-case credits stay the same. Extra admission
is counted by existing capacity-diagnostic mode; quota refusal still preserves
typed known-local failure and retained ownership. Next instrumented1025 cause
at changed product source will confirm/refute the hypothesis before gate tail.
