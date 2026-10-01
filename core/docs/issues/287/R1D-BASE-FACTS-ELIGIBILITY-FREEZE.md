# Profile7 BaseFacts and ParentEligibility freeze

> Status: implementation input, all new exits unrun. Parent inspected at
> 53b6bf741a5693f4d00ec98b914ce34645ee9ab3, 2026-10-01.

SC-03/05/06/07/08 apply. Profile7 composes profile5 graph/alias infrastructure
with BaseFacts17 and ParentEligibility18. Profiles1-5 keep explicit compatibility
semantics. This replaces the actual strict validation memo/absence and new-parent
eligibility authorities; counts/release remain the next dependent work.

## Binding and exact records

FactSubject uses the current full GraphSubject (issued SourceId, namespace,
selected base filesystem-root, root serial and captured operation class), plus
actual authenticated InodeTable presence/root/root-serial. C1 loads and validates
the selected filesystem root before first bind; SourceId is never a table-root
substitute. Base presence and table presence must agree, and both root serials
must match. Fresh absence has zero table placeholders. The subject framing is
GraphSubject106/table-present1/table-root32/table-root-serial8=147. Factory captures
the static GraphSubject; its initially unbound table becomes immutable exactly
once on C1's supplied checked subject. Foreign/stale subject refuses before SQL
and cannot terminalize another issued owner.

FactScope is issued bound StateScope81 (phase4/table17) plus subject147=228.
ParentScope uses phase5/table18 and the same subject. Full native selector/binding
and SourceId remain live capabilities, not decoded authority from bare bytes.

BaseFact key25/value74/framing6=105 encoded bytes. Value is present1/typed-inode73;
absence tag0 requires73 zero bytes. Unknown is no row. Present and absence are
immutable; conflicting or duplicate insertion refuses before the batch effects.
ParentEligibility key25/value1/framing6=32. Only declared-new nonroot directory
headers are inserted, ordered, with bound=false. After exact declaration EOF,
full scalar binding replay monotonically marks incoming children bound=true.
Unknown point is not an excluded parent; false is excluded; true is retained.
A sealed count/bytes/digest/last-key/EOF transcript closes marking before Sites.
The root is never an eligibility row or excluded.

## Windows, phases and exact work

Strict C1 uses Hot8, an independently admitted fixed resident cache. Every miss
queries exact persisted facts before authenticated lookup, so eviction does not
reauthenticate a previously established fact. Logical demand/site charges remain
at the same semantic demand sites. Prefetch64 preserves the current raw replay
transcript, local deduplication, bounded lookup waves and late-error accounting;
no union or complete memo/absence population is retained in C1.

Facts grow monotonically through Sites+alias discovery and Graph construction.
After all dependent readers end, seal and verify an advancing128/64KiB page
stream, then retire exact keys in bounded transactions before Roots. Eligibility
stays sealed/live through canonical construction and is retired only after all
point/cursor consumers end. Each known continuation advances. COMMIT Unknown
retains exact old/proposed records, counters and continuation/credit, permits
no resend/adoption, and remains distinct from absence/Unknown lookup knowledge.
All windows include headers/framing. First-row-too-wide refuses without a stalled
empty non-EOF page. Statements close at page boundaries; no prefix rescanning.

## Shared admission and integration

One captured S and native file/connection/profile owns these tables. Incremental
admission sums actual Sites60B + AliasFacts40F + AliasJobs39J + BaseFacts105N +
Parent32P plus fixed owner/header bytes while those classes coexist. Graph+facts
and Roots+eligibility are separately summed after known retirement. Neither D/B
nor Graph R predicts the v1 base V/E population. No per-table S, automatic growth
or shape/work/deadline limit increase is selected. Logical ceilings and actual
SQL index/metadata/page/journal/native allocations remain separate observations.

Use shared GraphMemory64KiB for actual boxed fact/eligibility coordinator and
bounded attempt/vector/hot-cache capacities before allocations/effects; no new
allowance. Fixed scalar read pages coexist with C1/C2 decoder/consumer owners.
Compiled simultaneous expressions and actual provider exits are mandatory; a
capacity/engine refusal is retained evidence, not permission to grow the class.

DirectoryRoots becomes a nonborrowing coordinator: scope/ledger/pending/seal stay
owned, methods receive the existing supplied IndexedState. This permits exact
sealed ParentEligibility point access through the same native session while
Roots/count state is live. Old APIs select their explicit resident compatibility
probes before effects; new profile7 namespace APIs supply typed FactState and
ParentEligibilityState. No runtime error chooses a historical algorithm.

Independent vectors cover exact present/absence framing, selected table/root
mismatch, stale source/phase, eviction with preserved demand counts,128/64KiB
crossings, absent versus unknown, parent marked by distant directory bindings,
root protection, unreachable new-directory/orphan policy, exact final EOF,
retained moves/aliases, late reader/provider failure and real COMMIT Unknown.
Existing sealed v1 canonical roots/object sets remain independent expectations.
Root owns native profile/header/Service integration and coordinated Cargo runs;
namespace owner changes no graph-memory/draft files owned by other agents.

## Concrete captured class and deferred native binding

FactCapacity is24 bytes: fact-record-limit8/parent-record-limit8/aggregate-bytes8,
all big endian. Append these exact captured fields to profile7's original native
binding/header transcript alongside the unchanged GraphSubject and profile5 alias
class. Do not rebind/relabel that original header after construction starts.
The dynamic authenticated FactScope228 is stored once in fact/parent fixed owner
rows; its full subject enters every selected seal/capsule. The fixed fact/parent
metadata logical admission allowance is1024 bytes (a conservative allowance,
not a serialized-record or physical-allocation claim), inside the captured
aggregate and S. Alias fixed585 remains separately included. Actual rows charge
105/32 respectively; stage-overlap sums use current known row populations and
retain Unknown proposals/credits. Header-inclusive FactPage width is301 bytes;
its owned decoded Vec retains GraphMemory credit until the Vec is freed.

### Concrete coordinator completion order

The new supplied entry points are `build_filesystem_binding_rows_with_namespace_state`
and `update_filesystem_binding_rows_with_namespace_state`. Their selected checker loads
and validates the actual canonical filesystem root before binding FactSubject. Parent
membership is declared through ordered directory-header EOF, then incoming binding EOF;
an independent bounded ParentPage fold verifies scope, key, count, bound count, digest,
maximum and EOF before any eligibility decision. BaseFacts are shared by binding,
alias and graph checks, then closed, independently paged through EOF and retired before
DirectoryRoots begin. ParentEligibility remains live through all canonical directory,
reference and inode consumers. After final inode-stream EOF and reference cleanup,
`parent.complete` retires ParentEligibility first, `state.complete` releases Roots next,
and only known completions permit filesystem-root emission. This order keeps the common
operation owner live for both completions and applies to the verified empty provider.

C1 now uses a nonborrowing DirectoryRoots coordinator with explicit state arguments;
legacy public paths select their resident fact and eligibility calls prospectively.
The profile7 path selects narrow supplied FactState/ParentEligibilityState calls with
no error fallback. No count/release growing-owner completion claim follows from this
source change. Its first owning Cargo check and physical/provider qualification remain
pending with the coordinating owner.

### Fixed producer credit closure

The supplied FactAccess now obtains a shared working lease before the raw64
prefetch producer or its replay hasher is initialized. The exact compiled charge
is sizeof(Wave) +2*sizeof(Completed) +sizeof(blake3::Hasher), held across both
complete source passes and all overlapping native fact insertion attempts. The
64 answer/record window separately charges64*(sizeof(BaseFact)+sizeof(Option<InodeValue>));
Parent declaration/marking charges128*sizeof(u64) across its two source passes and
independent final page fold. Public const layout helpers expose these actual
private types to native compiled overlap assertions. Compatibility paths explicitly
select no supplied working lease; this changes no captured budgets or work limits.
