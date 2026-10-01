# V4c3b bounded span reads and SQL operation windows

> Status: Research; informative and not a product contract.

Parent715785ae3166491b1a8df41dddb6fe62d39813d6. Previous turn was verified
progress: bounded directory/source retirement and full5root live proof published.
Full seven-family/DeepSeek/admitted streaming objective remains unchanged.

## Concrete replacements and ownership

Engine.read currently selects start<end AND end>at using the (ino,start) index.
Without a lower start bound, a late read can scan the entire earlier span prefix.
Replace with one indexed start<=at predecessor lookup, then start>=selected_lower
AND start<end range. Nonoverlapping final extents make that lower bound exact;
holes/EOF/partial source offsets remain literal. Actual SQLite VM-step/query/row
counters (fixed-size) and real plan tests prove work, not a timing resample.

Whole operation transactions presently wrap namespace prepare/install and daemon
captured-row adoption/extent deletion. MEMORYjournal can grow with changed pages.
Use existing SQL connection, small explicit64row mutation windows and point writes;
main indexed scratch is built before semantic/portable/file READY seal. Failed
prepare leaves unsealed owned draft; never changes committed index. No blanket
outer transaction around all certified namespace work. Intrinsic immutable facts
may persist only after their own complete validation. No WAL/sync/recovery expansion.

Known namespace installation marks the current verification index installing before
changing any committed row; all source checks reject that state. Changed rows and
parent effects install incrementally in bounded windows; scratch clears by indexed
64row deletes. Exact new root stamp and installing=false publish only after complete
known installation. Failure retains installing/pending and accepted canonical C5root,
blocks future selected-source validation, never guesses adoption or retries.

Daemon known installation likewise adopts each captured inode, clears its local
extents/name edits in64row transactions and retires sources in admitted windows.
All loops hold existing mutation owner; reads retain exact current logical bytes
from new immutable root plus remaining identical overlay. Partial failure blocks
future writes/construction/install, keeps native pending/Unknown custody and accepted
bytes. No population-sized transaction, frontier vector, new connection per Commit
or postpublication proportional allocation. Prepared and edit scratch cleanup uses
indexed64row windows. Edit staging clears/spools in windows; existing512admission
remains until the next explicit population/operation profile milestone.

Own dedicated span-read/SQL-window/install helpers, engine/edits, namespace index/
semantic walk/callers and external checks/generic case preparation/docs. Preserve
C1/C2/MinIO/C5 identities/ACK/conditional publication, one producer, current512/256
scope, provider/deadline/worker/canonical/wire/durability profiles and foreign work.
Do not remove validation to simplify batching. SQL work-window counts are not
physical journal/cache/allocator containment; collect those separately before claim.

## Exit checks and live composition

Real SQL/files tests compare literal holes/spans/offsets/EOF with independent bytes.
Count-driven old predicate vs new predecessor/range over64and4097actual span rows:
late one-span read must have bounded actual VM steps and exact lower-bound index
plan, not depend on earlier rows. No fake allocator/clock or product test hook.

Known install over multiple64row windows must preserve C1bytes/old roots, exact
prepared/name/edit clear, bounded source retirement and clean UpToDate. Namespace
stale/extra/missing/refs/cycles/moves/split/merge tests cover new unsealed/installation
barriers; partial failed selection cannot seal or become current. Explicit provider
failure preserves accepted bytes and no retry. Owning locked tests/Clippy/fmt/native
and Linux release; no unchanged passing suite/speed replay.

Generic live case preserves complete original128fixture and prior5literal commands,
then creates a sparse258049byte file with64ordinary dd writes spaced4096bytes,
checks a late byte and commits. Independent sixth full manifest adds exact literal
zero-filled file with X at each write. Actual public Exec/FUSE/SQL/C1/C2/MinIO/C5 and
separate readonly6root every-file/metadata/history/cleanup proof; source/work counts
reported. One frozen run15s performance/9.5s proof, cacheINELIGIBLE; retain failures,
no larger timeout/worker or smaller fixture. No new family runner/public Init claim.

Next paged scope reservations/admission and explicit inherited import/mount remove
current fixed caps using these bounded mechanisms. Then remaining syscalls, genuine
seven-family and complete DeepSeek import/command/locality plus physical/Unknown/
concurrency/canonical qualification. Full goal active; checkpoint/LOC/comments#294,
major outcomes#293, no issue closure or #288campaign claim.

## Source-evidenced mutation/oracle addendum

Initial6root gate failed only after5passed roots because a new manifest encoded
octal0644 in decimal field. Independent correction420 preserves commands/bytes.
Actual write boundary queries share earlier-prefix defect: seek nearest, check
overlap in typed caller, delete exact start range.64/4097span count proof fixes50
VMsteps versus270/16402old. Truncate accepts size/visibility in short transaction,
then crossing span point update/64row tail delete, preserving accepted logical
intent and quarantine after cleanup failure. Current128KiBbody/512/256scope retained;
physical journal/cache observation remains required. Corrected source gets one
covering6root gate; old failure preserved. No unchanged passing arm rerun.
