# Issue 261: ownership I/O treatment after the three 100-write cases

> **Status:** Prospective source and evidence contract. Commit this file before
> changing product source or taking a changed-source diagnostic.

The retained baseline at source `7361312e6` uses one public Mount, one Exec
with 100 one-byte writes from one fd, and one public Commit per case. The
append, dispersed and repeated 10 MiB cases, old-head fixture, offsets,
values, case order, independent full-byte oracle, deadlines, worker count and
uncontrolled cache state are frozen by `issue261-three-pattern-100-spec.md`.
Every existing latency row remains `INELIGIBLE`; do not resample or relabel it.

## Selected source changes

Old-root `cleanup_step` authenticates a page owner with `read_owner`, then
calls `load_raw`, which reads and authenticates the same 4 KiB ledger page
again. Reuse the owner only within that one cleanup step. Keep the later
ledger **path identity** check, the page-file identity and checksum checks,
quarantine, error ordering and the read on every later cleanup phase or retry.
The predicted reduction is one ledger read per edge-bearing cleanup page that
takes this branch; its count is not isolated by the baseline. This single
change does not alter the payload/page format, quota, root shape, public write
contract, atomic publication, G1/G2 custody, or one-worker policy.

Two other source-supported opportunities remain proposals. A one-page tiny
segment would cut one-byte payload allocation from 8,192 to 4,096 bytes, but
changes a physical format; issue #261 excludes speculative packed-page
formats. The current `symlink_metadata` absent-path precheck precedes an
exclusive `O_EXCL|O_NOFOLLOW` create and adds one pathname lookup per segment,
but removal changes the disappearing-collider race. Neither change is in this
treatment; each needs its own custody ruling and prospective evidence contract.

## Evidence and gates

Reuse the already closed, independently verified 10 MiB Store/history master
and unchanged sealed static writer after checking their exact source and binary
hashes. Rebuild the host SDK driver, independent verifier and daemon with
locked Cargo release against the changed product, then seal them and rebuild
the matching daemon image. Reprove the old master with the rebuilt verifier.
Each case gets an independent writable byte copy of the Store/history master.
Take one labelled count-driven diagnostic
per case at one frozen changed-source identity, in append/dispersed/repeated
order; never replace a failing or cache-ineligible receipt. The complete
command remains bounded at 15 s and separate oracle at 9 s. Cache remains
ordinary/uncontrolled with `admission_eligible=false`, and construction uses
one worker.

Record Exec/Commit/complete wall, 25/50/75/100 progress, actual FUSE
callbacks, payload count and physical allocation, ledger reads/writes,
metadata pages/read counts, reclaim scans, extents/runs, Service/Store work,
resources, cleanup and exact old/new head oracle. Keep full identities,
logs, failures and SHA-256 custody. A focused metadata cleanup contract
check under 30 s and the three public old/new-head oracles must pass on the
changed source. Do not rerun unrelated previously passing checks.

Do not change the workload, product or benchmark deadline, worker count or
cache policy to turn a miss into a pass. This treatment does not remove #249's
product Exec timer or resolve the earlier 512-write `EBUSY`. The separate
#248 4,097 public gate is attempted once per frozen identity only when
meaningful, with every FAIL/INELIGIBLE/NOT_RUN retained.
