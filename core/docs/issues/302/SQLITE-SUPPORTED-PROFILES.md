# Supported SQLite profiles and qualification order

> **Status:** Current planning checklist; no release candidate exists.

Source: this commit, based on ab9932295. The product now supports explicit
`SqlitePersistenceProfile::Durable` and `Disposable` through
`PersistenceConfig::sqlite(path).with_sqlite_profile(profile)`. Durable remains
the constructor/default selection. Disposable is a disk-backed MEMORY/OFF Store,
not an in-memory database. It permits crash corruption and loss of acknowledged
data; immutable CAS does not restore durability.

Both profiles use the same objects/metadata/history schema, SQL, publication,
conflict handling, reservations and transaction algorithms. Profile selection is
applied and read back before schema creation/first mutation. Every newly opened
connection validates the selected settings. No environment-driven product mode,
interposer, live mode switch, automatic Store conversion or fallback is used.
Existing Store journal-family mismatches are refused before schema/history work.
WAL is required for Durable. Closed Disposable MEMORY journals reopen DELETE;
the explicitly selected Disposable connection reselects MEMORY. Conversion of
incompatible Stores is unsupported, rather than attempted on an open handle.

| Setting | Durable | Disposable |
| --- | --- | --- |
| Journal /synchronous | WAL /FULL2 | MEMORY /OFF0 |
| fullfsync /checkpoint_fullfsync | 1 /1 | 0 /1 |
| Foreign keys /temp_store | 1 /2 | 1 /2 |
| Page bytes /cache KiB /mmap | 4096 /-2048 /0 | Same |
| Busy timeout /journal-size limit /WAL threshold | 0 /4194304 /1000 | Same |
| Final WAL checkpoint | Required, actual result recorded | No WAL; explicitly false, frames-1 |
| Allocation release and connection close | Measured | Measured |

The unused Disposable checkpoint-fullfsync flag matches the recorded native
Phase4.5 ancillary value. Existing reference page/cache geometry is retained
and disclosed, not made artificially identical. Current macOS capability
requirements and explicit unavailable PostgreSQL selection are unchanged.

External checks cover both profiles' effective settings, acknowledged canonical
save/read, atomic rollback after a body insertion, matching writable/read-only
reopen, opposite-profile refusal and read-only refusal before SQL. An explicit
Disposable finalization test preallocates16MiB of extra extents, proves their
release preserves logical length/canonical bytes and matching readonly reopen.
The full namespace oracle covers100/1000 files under both profiles. Covering
persistence suites27 checks, new Disposable release1 and full namespace oracle1
PASS (29unique). Owning all-target persistence/project Clippy, final allocation
fixture Clippy, Core fmt, production boundary441files/23guard tests and4focused
benchmark registry/arithmetic tests PASS. No aggregate/CI gate or full-workspace
claim. No production dependencies, schema or named buffer/worker bounds changed.

## Measurement order and registered identities

The benchmark driver/verifier accept explicit durable/disposable arguments and
open through the supported API. Candidate receipts require real effective
journal/sync/ancillary/checkpoint readbacks to match their selected profile.
Disposable verifies its original measured database directly with the supported
readonly profile; no copy/header conversion or diagnostic intervention.

The seven existing Durable selections retain their identities. Seven separate
Disposable selections are registered:

| Disposable case | Current status at new source |
| --- | --- |
| phase7-sqlite-disposable-init-100-v1 | NOT_RUN |
| phase7-sqlite-disposable-init-1000-v1 | NOT_RUN |
| phase7-sqlite-disposable-init-10000-v1 | NOT_RUN |
| phase7-sqlite-disposable-init-100000-v1 | NOT_RUN |
| phase7-sqlite-disposable-history-stride10-v1 | NOT_RUN; history driver/proof/cold binding pending |
| phase7-sqlite-disposable-history-stride3-v1 | NOT_RUN; same pending requirements |
| phase7-sqlite-disposable-history-stride1-v1 | NOT_RUN; same pending requirements |

All preserve the original corpus/state counts, integer10% time margin, allocation
ceilings, source-content cold contract, prepared-fixture reuse outside timing,
release/locked worktree-local artifacts, workers/bounds,15s Init complete command
and9.5s separate verifier. The owner explicitly restored60/170/170s history performance bounds in the
[scoped ruling](HISTORY-BUDGET-RULING-20261004.md); separate proof remains9.5s. Missing contracts fail closed
before build/setup/sample. Per-profile required-case lists prevent a Disposable
row from supplying Durable admission. Fresh case IDs/append-only outputs and
one sample per frozen case/arm remain mandatory. Existing intervention diagnostic
is informative only and does not qualify this direct-open profile.

First qualify the supported Disposable profile against unmodified Phase4.5
MEMORY/OFF across all seven agreed workloads. Freeze implementation/artifacts
once it qualifies. Then run that same frozen implementation as Durable and
measure the durable gap. Any new matched reference/observer identity must obey
existing sampling rules. A Disposable PASS qualifies only Disposable; the
all-seven durable competitive terminal objective remains outstanding.

Before that campaign, finish the profile-aware history vehicle/proof/cold
binding and measurement observer recording so a later harness change does not
invalidate an early partial pair. Required side-by-side evidence includes
bootstrap/operation/finalization/close/complete command, comparable internal
stages, SQL/VM/read-write transactions/COMMIT, and VFS writes/sync with phase or
publication attribution where feasible. Missing observations stay explicit.
No new production speed sample was taken for this implementation commit.
