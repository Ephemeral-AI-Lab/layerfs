# R1d exact alias-discovery frontier freeze

> Status: Implementation input; provider, resource and canonical exits unrun.
> Inspected parent 53b6bf741a5693f4d00ec98b914ce34645ee9ab3, 2026-10-01.

The actual prepared Graph caller discovers base aliases while immutable Sites
membership remains live. SC-03/05/06/07/08 apply. Replace its pending Vec and
seen BTreeSet with the same-owner AliasFrontier port. BaseFact/count/release
populations remain separate open rows. Existing selective Graph and v1 roots
are preserved. No native memory, physical fit or latency qualification follows.

## Exact discovery and custody

Reserve closed private tables AliasFacts=6 and AliasJobs=7; registry currently
ends at GraphEdges=5. Private profile5 supplies these tables; profiles1-4 retain
their original grammars. The full issued selection/native binding and immutable
SiteMembership bind every call; equal bare tokens or membership bytes do not
create authority. Jobs have one positive serial, monotonically burned sequence
and exact pending/current/expanded fact. Fact key is token8/phase8/table1/serial8;
value is sequence8/status1, with six framing bytes:40 encoded bytes. Job key
uses sequence8 instead of serial8; value serial8 gives39 encoded bytes.

Every discovered occurrence of a Pending serial replaces its old job sequence
with a newly issued sequence. Current/Expanded serials ignore rediscovery. Taking
MAX sequence removes exactly that job and selects one current parent. This
preserves the old stack priority: A,B,A visits A then B, rather than B then A.
Each directory expands once. The current parent finishes its entire base and
selected-site listing before another job is taken. Restatement exclusion,
active-site/unreachable checks and existing semantic error order remain.

One fixed owner retains the current serial and an advancing continuation:
Base-name pages, then Changed-site-ordinal pages, then Complete. Names use the
complete255-byte maximum, explicit length/presence and zero absent bytes; no
hash/truncation. The fixed progress grammar is264 bytes:stage1/name-present1/
name-length2/name255/ordinal-present1/ordinal4. Same-stage nonterminal progress
must advance strictly; Base->Changed->Complete are the only phase transitions.
No current parent can complete without acknowledged Complete progress.

Enqueue batches are at most128 records and64KiB including fixed membership and
transaction headers. Each checked old/proposed fact/job and owner counter is
retained in a bounded heap-owned attempt until SQL COMMIT and native allocation
observation are known. A successful continuation advances. Known refusal or
Unknown is terminal: no directory page resend, adoption or alternate resident
route. Already accepted observations/enqueues remain owned if a later step fails.
Unknown keeps the original typed StorageError plus the proposed records/progress.

Finish verifies no pending job/current, exact discovered=expanded and terminal
EOF. Retirement removes exact fact keys in <=128-record transactions, recording
remaining count/bytes/last serial; verify both fact/job indexes empty before
allowing Graph mutation. Abandonment is metadata-only, with no SQL/native refund.
Native close/unlink remains the separate ScratchSession owner.

## Combined admission

There is one captured native S, one file/connection/cache class, and no second
full-S reservation. Actual simultaneous logical bytes are Sites60*B +
Facts40*F + Jobs39*J, plus the fixed585-byte owner header. The header is scope89/membership-present1/
membership164/stage1/counters32/current17/progress264/remaining8/last-present1/
last-serial8; absent fields are zero. B is the admitted declared site
bound; F/J are proposed exact counts. The independent alias class is selected
before effects with its own record and byte limits, and every growth checks the
combined expression before SQL. Graph R=S/256 is not an alias-record budget.
Existing work/request/worker/deadline limits remain. The profile5 explicit AliasCapacity argument binds the admitted class; its Sites
bytes equal60*declared bindings and aggregate ceiling is<=captured S. Server
selects the existing logical ROW_LIMIT*63 ceiling and existing ordering-derived
walk ceiling, independently from Graph R. The chosen logical ceiling
is an admission input, separate from actual SQL metadata/index/page/journal
allocation and fit; neither encoded bytes nor a max_page_count readback proves
physical/provider fit. Unsupported fit remains explicit.

The C1 current page has<=64 entries and8KiB; pending child window<=128 serials;
observations<=128; one name/ordinal progress and one current inode value remain.
Sites, current base acquisition/decoded page, frontier attempt, SQLite copies,
indexes, MEMORY journal and native scratch remain simultaneous. Reserve each
actual capacity/compiled layout before effects; wire widths are not heap bounds.

## Integration ownership and independent exits

Namespace worker owns new C1 alias types/port/resident compatibility, the alias
algorithm, new C2 alias provider/SQL files and external owning tests. Root owns
entry declarations, Resource/native/profile5 factory and real Server integration;
C2 owner retains graph/session helpers. Architecture04-filesystem.md records the
change. No competing Cargo command or measurement runs in this worktree.

Expected vectors are fixed before candidate output: duplicate-priority stack
interpreter, restated different-child edge, cross-parent exclusive alias legality,
shared repeated child, wide/deep traversal, stale membership, decreasing cursor,
exact EOF and partial retirement. Existing sealed filesystem v1 roots/object
sets remain independent canonical expectations. Actual SQL/kernel tests separately
own indexed plans, allocation/overlap, real COMMIT Unknown and typed custody.
Every exit is unrun at this input freeze.

## Scoped resident allocation admission addendum

Before the dependent boxed Alias owner/attempt or128-change Vec allocation, obtain
its actual compiled payload/wrapper/vector capacity credit from the same existing
GraphMemory64KiB authority. AliasProgress retains fixed264-byte arrays, without
heap names. Owner wrappers drop their Box/Vec first and their last-owner lease
last. Deferred Graph and alias owner/attempt overlap; after alias retirement the
fixed owner remains charged alongside the maximum Graph attempt/returned ACK.
A compiled assertion and owning layout proof cover both combinations. This adds
no independent allowance and makes no8MiB/global/physical containment claim.
