# V4c3a bounded live directory cursors and source retirement

> Status: Research; informative and not a product contract.

Parent bc056168b1d8112b8030f410fd63f5d9a137e919. The previous goal turn made
verified progress: original67/270/128live namespace proof and changed-only service
work passed. Full seven-family/DeepSeek/bounded larger-profile objective stays open.
This is a concrete dependency before raising population/serial/operation limits.

## Current source and replacements

Engine::directory_entries currently collects up to512names sorted by name. FUSE
readdir builds another full vector and resumes by ordinal, reconstructing the same
population for each kernel callback. Engine::write stores final extents correctly,
but sources remain forever: the owning64overwrite test finds65physical source rows
for3live spans. Known install deletes extents without retiring displaced sources.

Keep the existing Engine SQLite connection, cache2MiB/MEMORYjournal/OFFsync,
single producer/capture lock and current512population/operation/serial/256handle
profile. Fresh experimental schema gains monotonic directory-cookie allocation
and source references/retirement state. No legacy-schema auto-import/fallback,
canonical format, wire opcode, provider, worker, timeout or durability change.

## Directory grammar and bounded work

Each live name row owns one positive monotonically allocated cookie>=3, independent
of inode ID (future aliases need distinct name cookies). Same-transaction counter
allocation plus names(parent,cookie) index; no lifetime cookie registry. Dot and
dotdot use1/2. Rename preserves source cookie and replaces any target cookie; deleted
cookies remain valid ordering positions and are not reused. Returned cookies are
opaque positions, not ordinal offsets or snapshot promises. Rewind0 and backward
seek to a previously returned cookie remain supported. Namespace mutation during
enumeration has ordinary live iteration semantics; it does not freeze all names.

Actual issued directory handle/ino/type and signed cookie range are checked before
query. Query WHERE parent=? AND cookie>? ORDER BY cookie LIMIT64; owned page payload
<=16KiB (plus fixed64row capacity). Decode one bounded page, fill actual kernel reply,
continue at last accepted cookie; no OFFSET, skipped-prefix scan, whole-directory
vector or name-history list. A callback has a declared finite512row maximum before
returning a resumable partial reply; request/reply buffer remains provider-owned
and physical qualification separate. Dot/dotdot use the actual inode and parent.
C1construction still consumes names in canonical name order, independent of FUSE
cookie order. External reference-builder test adapts outside runtime to bounded
pages then its own independent ordered fixture collection.

## Source ownership and safe retirement

Sources carry exact live extent reference count. Actual INSERT/DELETE extent SQL
triggers update those counts and an indexed zero-reference retirement queue in the
same mutation transaction. Split/join/truncate can temporarily remove/recreate a
reference; an inserted live reference cancels its queued retirement. No full source
or historical-write scan. Source files remain immutable after their owning write.

Only after a known successful mutation transaction may zero-reference files be
removed. The mutation lock excludes reads/construction while a batch is selected
and retired. Each callback retires at most64source rows/files; surplus work stays
in indexed SQL. Known Commit installation adopts actual immutable C1root, deletes
captured extents/edits and drains retirement in64row windows. Unknown publication
never installs or deletes captured source references. Open unlinked victims keep
extents and their source references; complete orphan inode/lookup retirement is a
later separate gate. File removal/SQL errors fail explicitly, preserve accepted
writes and never trigger retry/fallback or claimed cleanup PASS. No fsync/WAL or
crash-recovery promise. File creation failure cleanup/custody is recorded honestly.

Exact source counts are assertions over external real SQLite/filesystem tests, not
inline counters/hooks. Runtime fixed-size observations count actual directory pages/
rows/owned capacities and source retirements/pending existence; no lifetime vector.
Resource arithmetic is not physical memory/page-cache admission.

## Ownership and exit proof

Own engine schema/create/install/write/truncate, dedicated directory/source-retire
modules, rename cookie handling, FUSE readdir, external engine/rename/construction
checks, generic sealed case preparation and source-pinned report. Preserve current
C1/C2/MinIO/C5/index/session behavior and all prior receipts/foreign worktrees.

External actual SQLite/filesystem checks: exact multi-page inventory/no duplicates;
seek/rewind/deleted cookie/rename/replacement; indexed query plan without OFFSET or
sort; byte and row budgets before vector growth; invalid handle/type/range refusal;
repeated overwrite retains only referenced files; split/truncate/regrow bytes;
open replaced victim read; known-install source retirement; no acceptance rollback.
Run covering locked tests/Clippy/fmt/native+Linux release at frozen source.

Live generic five-step diagnostic reuses the complete original128fixture/case and
literal first3commands/manifests, then64ordinary overwrites of many/f0 plus real
ls/find enumeration, then a clean observation of no local source files and complete
128name enumeration. Same literal edit-0 expected bytes/full tree at steps4/5;
independent readonly every-file/metadata/history proof across5roots. This also
covers clean UpToDate C5/index installation. No daemon workload recognizer, new
family runner, smaller fixture or hidden preparation credit. Once per frozen source,
15s performance/9.5s separate proof, cacheINELIGIBLE; failures retained and diagnosed.
Exact timings are diagnostic; owning seven families and DeepSeek remain unrun.

Next V4c3b: paged scope reservations, population/operation admission based on actual
paged work and explicit inherited import/mount, then broader syscall/source lifetime
and real resource/progress proofs. No current fixed cap is silently enlarged here.
Publish each checkpoint/commit/LOC to#294, major results#293; full goal active.
