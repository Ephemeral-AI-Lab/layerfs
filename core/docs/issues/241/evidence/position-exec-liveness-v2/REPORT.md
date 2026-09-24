# #241 10 MiB Exec liveness diagnostic v2 result

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This is one functional diagnostic selection, not a replacement
> position campaign or a performance sample.

The first frozen attempt at source
`fa1c0774c9d12cd6e58f730118164140fa04fa22` selected all 66 existing
10 MiB positions from one independently byte-copied, validated v3 master.
The [summary](attempt-01/summary.tsv) records **29 PASS / 1 FAIL / 36
NOT_RUN**, with final SDK Sandbox list empty. The complete test command took
102.22 s, including setup, mounted checks, failure observation and cleanup;
it is not an edit-latency measurement. Each completed passing baseline Exec
returned success in 26.198416 ms or less of host context wall. Cache state
was uncontrolled and `admission_eligible=false`.

The failed [case](attempt-01/10mib-overwrite-band13.tsv) was
`10mib-overwrite-band13` at byte 9,113,539. SDK Workspace mount returned
`UncertainMount(Unknown,true)` **before FUSE mount, baseline Exec, ioctl
EDIT, or Commit**. The daemon's [late telemetry and logs](attempt-01/10mib-overwrite-band13-edit-daemon-late.stderr)
record a read-only `HistoryQuery` error after **5.001619502 s** and
`WorkspaceOpen`/`daemon.workspace_attach` errors after **5.001784835 s** and
**5.001766294 s**, respectively. Workspace attach issues
`HistoryQuery::GetBranch` first. There is no `daemon.fuse_mount` or
`WorkspaceExec` record for this case. The host [console](attempt-01/console.txt.gz)
has no completed matching Service `HistoryQuery` telemetry record after its
successful control Hello; a handler that has not returned would also have no
final record, so this absence does not prove the request was never admitted.

The pre-delete [Docker inspect](attempt-01/10mib-overwrite-band13-edit-docker-inspect.stdout)
shows a running daemon and `OOMKilled=false`. The [early](attempt-01/10mib-overwrite-band13-edit-docker-top.stdout)
and [six-second-later](attempt-01/10mib-overwrite-band13-edit-docker-top-late.stdout)
process snapshots contain only `/layerfs-daemon`, with no shell child. SDK
Sandbox deletion and absence passed. The [identity](attempt-01/IDENTITY.json)
pins source/tree, host test binary, manifest, fixture, qualified master and
immutable image. The [SHA manifest](attempt-01/SHA256SUMS) validates all 82
copied raw files; the mutable private Store/history copy remains in the
ignored target directory for forensics.

**Interpretation:** this is a new control-path liveness failure at the
daemon-to-host `HistoryQuery` boundary. It cannot be caused by ioctl or by
the baseline FUSE WRITE in this case. It does not prove that the original
`10mib-delete-band13` baseline Exec failure had the same cause. The current
trace cannot distinguish TCP connect, authenticated Hello, request send,
host admission/handling, and terminal delivery for the nested query. A
causal fix requires those request-scoped stage events; changing the shared
five-second progress rule or omitting FUSE invalidation would mask an
unlocated stall. The original [integrated selection](../position-sweep-integrated/REPORT.md)
remains **243 PASS / 1 FAIL / 20 NOT_RUN** and Phase 3 is still incomplete.
