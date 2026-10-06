# Current Durable100 physical-write and acquisition findings

> **Status:** Count diagnostic/research. No new latency/storage admission PASS.
> The original trace orchestrator FAIL and SHM closure gap are retained.

The selected5,000,000-byte input submits11,051,708 bytes through SQLite VFS writes
and makes26 VFS sync calls. Five Storage publications account for9,410,416 of
those bytes. One large file-content publication writes4,000,520 B to WAL and
4,161,536 B to the main file within its COMMIT. The explicit final checkpoint
writes another1,241,088 B to main. Removing small publication boundaries alone
does not remove this payload WAL/checkpoint work.

Acquisition's eight write units submit197,792 WAL B and make nine WAL sync calls.
The current topology therefore has two distinct costs: repeated durable
acknowledgement of modest working rows and a large payload path through WAL and
main. A placement-only change would leave the large publication/checkpoint path.

## Caller, proof and limits

One public `durable_init_costs --vfs` Init ran at observer source
593e259714967c9330f9d59199f9d091c9069ddf, tree19c662e0994bbd3e360e1e07d49e17910ca750b5.
Its production seal44f07dd216a13d3103440b15b6d295a026ee80878f6e572e468855f27e2e74c5
matches the active reservation-only9b74ac035 implementation exactly. Scope is
fresh Monolithic schema4, Durable WAL/FULL/fullfsync/checkpoint_fullfsync,
automatic checkpoint1000, original100 files/2 directories/102 paths/5MB seed1,
four constructors and environment-workers1. Input manifest/root remain the
original ones. No profile, topology, byte/row limit or ordinary gate is changed.

The owned managed worktree is clean and holds the normal nonblocking phase7 lock.
Locked release build5,587,021,708 ns and observer build154,005,042 ns each fit30s;
complete diagnostic command1,760,474,959 ns fits15s. Cache is uncontrolled and
instrumentation enabled. All latency/RSS/rate values are INELIGIBLE for numerical
admission. No ordinary candidate/reference speed sample was replayed.

The first-party VFS delegates original unix VFS v3, retaining optional methods,
arguments, exact returns and aligned176-B observer/192-B underlying file state.
Default diagnostic name is `layerfs-cause-delegate`, explicitly different from
the underlying name. No SQLite code/dependency is patched. Native SQL status
counters are not reset by any trace callback.128 fixed step events are diagnostic
capacity; omitted events invalidate data and do not stop product operations.

Capability readback42 PASS, host/Linux example Clippy warnings denied PASS, C
build warnings denied PASS. Actual caller has42 COMMIT/checkpoint step events,
zero omission/IO errors/live files/close errors/unknown sync flags and five
publication COMMITs matched to their port deltas. Step counts include the
checkpoint's ROW and DONE calls. Native Init observes207 SQL statements/87,882 VM
steps/17 write commits; five publications54 statements/27,453 VM/5 commits. With
schema creation, total writes remain18. Caller/step/lifecycle levels are nested;
they must not be added as independent costs.

The independent verifier initially refused with `NotPresent`: the orchestrator
omitted its required cursor-key environment, before Store open. The original
orchestrator receipt remains FAIL. A separately recorded corrected read-only proof
PASSs in26,219,959 ns under9.5s, inventories102 paths and samples53 files/3,354,003 B.
It confirms the original canonical root/history and exact sampled bytes; the
producer ran once. Main database SHA256 is unchanged before/after that proof.
The source/native test evidence complements this sampled payload oracle.

**Retention gap:** SQLite's read-only verifier updates writable SHM coordination
bytes. Its original producer SHM was not separately archived before the corrected
proof, so that one original manifest hash cannot be verified afterward. Preserve
original expected hash and post-proof hash; original SHM closure is INCOMPLETE.
Every other trace/raw file, main DB, production source, driver/verifier/library and
independent current closed copy is SHA256-verified. No receipt is rewritten to
hide the gap and no producer reruns to repair it. See
[closure/post-seals](checks/durable100-vfs-20261006/closure-and-postseals.json),
[complete identities](checks/durable100-vfs-20261006/complete-identities.json) and
[raw producer/proof](checks/durable100-vfs-20261006/raw/).

## Delegated physical work

VFS request/submission bytes are not physical device bytes. `xSync` calls are not
a count of fsync/F_FULLFSYNC syscalls. Their inclusive wall can contain scheduling
and underlying VFS work. File allocation/release syscalls outside SQLite VFS,
phase/system residency, physical device traffic and sustained rate are unavailable.

| Lifecycle file class | xWrite calls | Submitted B | Write ns | xSync calls | Sync ns |
| --- | ---: | ---: | ---: | ---: | ---: |
|Main|1320|5406720|4488147|3|2544292|
|WAL|1372|5644464|7130174|22|26504707|
|Main journal|2|524|56417|1|104042|
|Other|0|0|0|0|0|
|Total|2694|11051708|11674738|26|29153041|

WAL reads1321 calls/5,410,816 requested B/1,221,105 ns. These can occur even
though C2 reads no immutable packs: SQLite itself reads WAL for checkpoint work.
Main reads57 calls/221,316 requested B/101,666 ns; journal reads8 requested B.
No byte/clock total is added to containing SQL or Init wall to fabricate a timer.

| Port group, disjoint deltas | Submitted B | Write ns | VFS syncs | Sync ns |
| --- | ---: | ---: | ---: | ---: |
|Bootstrap|169452|527086|4|9727709|
|Eight acquisition writes|197792|975538|9|9393249|
|Two Storage reservations|8240|15499|2|949959|
|Five Storage publications|9410416|9212380|7|6431999|
|Required final checkpoint|1241088|919985|2|1506667|
|Two direct history commits, outside wrapped ports|24720|24250|2|1143458|

The history remainder is identified by the unlabelled COMMIT events' preceding
history statements, not charged to acquisition. These disjoint port groups cover
the lifecycle write/sync totals. Parent port and contained COMMIT rows overlap.

| Storage publication | Submitted WAL B | Main B inside COMMIT | VFS syncs | COMMIT step ns |
| --- | ---: | ---: | ---: | ---: |
|Scan attributes, unit13|8240|0|1|785209|
|Large file-content wave, unit18|4000520|4161536|3|14317750|
|File-content tail wave, unit21|992920|0|1|1568875|
|Final file groups, unit23|193640|0|1|820792|
|Tree/value publication, unit37|53560|0|1|663292|

The provider's transaction code performs plain `COMMIT`, with no explicit
checkpoint there. Given the recorded1000-page automatic threshold and main-file
writes inside that step, this is the automatic checkpoint path. That conclusion
is an inference from caller code plus direct VFS events. The later explicit
TRUNCATE checkpoint is separately tagged. `complete_files` subsequently has two
WAL syncs while other small acquisition units have one; WAL-header restart/sync
behavior is a plausible cause, but file-offset/header attribution has not been
collected and is not asserted as proved.

## Architecture disposition

[Placement review](DURABLE100-ACQUISITION-PLACEMENT-REVIEW-20261006.md) confirms
A1's host-global, same-Session decision and separate daemon-overlay scratch
ownership. Moving working rows requires explicit crash loss, never-reissued owner
identity, fencing, uncertain custody and cleanup contracts. In particular a lost
operational database must not reissue a tuple that makes an old Owner live again.
Runtime/authenticated incarnation fencing or a durable epoch issuer is needed;
a disposable file alone is not a complete replacement.

The measured grouping failure is now explained more precisely: reducing two
small acknowledgements leaves the large WAL payload write and its automatic/final
main-file transfers. Acquisition placement can reduce working-row sync burden;
it cannot remove the9.41MB publication submission volume observed here. These
diagnostic counts do not prove a new architecture can meet the46.19ms threshold.

Next review should target a durable immutable payload backing that avoids putting
large already-sealed bodies through both WAL and main, while retaining SQLite
authority/history/catalogue and unchanged canonical pack bytes. That is a physical
layout/schema/creation/recovery/GC decision, not a fewer-commits patch. The current
Monolithic schema4 case cannot silently become that layout. A new candidate
identity must keep durability, workload, cache, worker and1.10x/final-allocation
gates, include all payload/backing creation/sync/close costs, and preserve the old
v2 FAIL. No production implementation or new gate is selected by this report.

S7/S9 remain incomplete. The retained reservation fix remains active; rejected
prerequisite/final-wave changes stay rejected. No legacy retirement, old campaign
replay, profile relaxation, fourth speculative patch, push/release/deployment or
S10–S13 action follows from the diagnosis.
