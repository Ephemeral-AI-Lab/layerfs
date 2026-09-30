# R1d binding claims: selected authority and complete consumer

> **Status: Prospective implementation contract; exits unrun.**
> First parent: `d3a10aadbb7506772552474478b14ef25947c899`.
> This is a named R1d submilestone. Full R1, native engine/physical progress,
> R2–R7 and #288 qualification remain open. No release candidate exists.

## Selection and scope

The common scalar validator currently retains `additions: BTreeMap<u64,u64>`
to reject more than one same-batch binding of a non-file. The canonical update
does not consume that result map. Its final pass also populates a zero entry for
each regular file solely for the old public checked-input result. Replace this
authority in the new Server/C1 route with exact typed exclusive-binding claims.
Preserve the explicit legacy public result under its old resident profile.

SC-03/05/07/08 apply: wide identities/names, exact aliases and malformed input,
known/Unknown custody, admitted overlapping resources. This delivery does not
retire base-record/demand, alias-site/base-binding, cycle/frontier/reachability,
unreachable, reference/count/touched/final-row or release populations. No whole
base-fact spill is admitted: the maximum existing parent/child union can exceed
the current row/byte class before SQLite overhead. That is a later design gate,
not a reason to enlarge a quota.

## Shared typed records and phases

DirectoryRoots table1 and its key25/value32/frame63/seal130 remain unchanged.
Allocate private `StateTable::BindingClaims = 2` only for the implemented
claim producer/provider. No Bridge opcode, wire version, canonical role, Store
schema, worker count or public workload limit changes.

`ClaimKey` is a distinct fixed25-byte key: token8/phase8/table2/positive
serial8, big-endian integers. Its complete association is the existing scope81:
selector32/token8/native-binding32/phase8/table-byte1. `ClaimRecord` is key plus
one exclusive class byte with exact code1 for both directory and symlink;
stored canonical kind wins over a caller value when classifying an existing
inode. Frame32 is key-length2/key25/value-length4/value1;
no ObjectId padding, arbitrary BLOB or general key/value contract. Only a
directory/symlink claim is stored; regular-file aliases need no claim.

`ClaimCapacity` reports admitted rows and frame32 bytes. `ClaimLedger` accepts
strict final primary-key order in windows at most128, checks widths/scope/count
and computes BLAKE3 with domain `layerfs/binding-claims/v1\0`, scope, framed
ordered records and exact count/record-byte totals. `ClaimSeal` encodes version1
plus scope81/count8/bytes8/digest32 =130 and is distinct from StateSeal.
Candidate output is never an independent expected transcript.

`BindingClaimState` has typed capacity, exact `claim_present(scope,key)` lookup
(full selected key/class validation before any duplicate result),
`claim_batch(scope, keys)` returning
`ClaimAdmission::{Fresh,Duplicate}`, exact claim seal, and exact-seal retirement.
The batch is globally unordered and at most128 keys (frame bytes4096 + existing
header88), independent of one final ordered seal pass. Duplicate is a semantic
result and cannot erase/mask a provider/rollback/Unknown error. A repeated claim
terminalizes this construction attempt; later root effects remain denied.
Direct `claim_batch` Duplicate also terminalizes its phase while retaining prior
acknowledged rows and zero additions from that current batch. Independent
external SQL/owner observation can prove atomic absence; another write/seal
attempt cannot inspect it by reopening the phase. Presence true is a read
result; its owning validator explicitly abandons upon treating it as a duplicate.

`ConstructionState` combines the two specific ports and supplies an explicit
`indexed()` delegation for Rust1.85; no trait-upcast requirement. Its blanket
implementation belongs in a focused implementation file. `ConstructionScopes`
selects exactly claims phase1/table2 followed by roots phase2/table1 for the same
bound live selection. Claims must acknowledge known retirement before roots
capacity/append/seal/get/page can become available. Old single-phase APIs remain
explicit compatibility, with no caught-error conversion or retry.

The exported method names are `claim_capacity`, `claim_present`, `claim_batch`,
`claim_seal`, `claim_page`, `claim_retire` and `claim_abandon`.
`claim_abandon(scope)` is exact-scope logical terminalization only: no SQL,
native I/O, rollback, guessed retirement, cleanup or refund. Any common-checker
error invokes it explicitly once and preserves the original error and provider
Unknown/capsule. A duplicate in an unflushed window or an immediate presence
result cannot leave the same owner/scope eligible for a second validation
attempt. The owner denies subsequent claim access and root effects; repeated
logical abandonment has no effects and foreign scope is refused. No Drop action
implements this gate. `ClaimKey::new/decode` validates
selected table2 and serial; ClaimRecord decode validates frame32 and code1.
The separate `ClaimPageLimit` charges header163 plus32 per record; output native
Vec capacity is also <=128 and within the requested count. `ClaimPage` carries
ClaimSeal130/presence1/last-key25/count2/record-bytes4/EOF1. Provider pages select
keys after the exact prior key and derive EOF from the acknowledged maximum.
The owning `ClaimCursor` checks full ordered count/bytes/digest/EOF before
retirement, in one additional O(U) pass with no prior page allocation retained.
Neither StateSeal nor StatePage's63-byte arithmetic is cast/reused for claims.
`ConstructionScopes::new(selection)`, `claims()` and `roots()` are the fixed
scope constructor/accessors. All these types remain independently usable.

## Complete C1 and Server caller

Add separate six-argument scalar build/update entry points with construction
state and scopes. Common validation produces input/topology without a resident
additions result on this route. Its maximum declared claim shape is derived
from exact header counts before canonical construction; common exact row/EOF,
allocator, kinds, aliases, roots and cycles checks retain their semantics.
At most128 claim keys are live; before enqueue check both the local pending
window and exact already acknowledged provider presence. This preserves the
existing first duplicate verdict even when a later parent/source is malformed;
delayed batch flushing must not change semantic failure precedence. Flush before
final seal, check acknowledged
claim count/scope/bytes, and retire before `DirectoryRoots::new` or root append.

Legacy `check`, `check_bindings` and old with-state entry points explicitly
select resident compatibility and preserve the additions result, including
regular-file zeros. Shared canonical and semantic bodies remain single
implementations. No opaque-source prefix recovery, new map of all keys, or
hidden materialization enters the new route. The existing remaining graph
populations retain their actual profile and open paged gates.

Server validates declared shape before token/slot/native scratch/Save/body
effects. Its phased begin takes D and B, checks each <=65536 and prospective
max(63D,32B) <=4128768 with max(D,B)<=65536. It keeps one existing16MiB native
owner and unchanged Store capacity; no separate phase session, refund between
phases, transient second file, new worker or idle full-buffer reservation.
The successful route is receive/seal/subject check/common C1 claim validation/
claim seal/known retirement/DirectoryRoots construction/known native cleanup/
Save finish/Stage. Scratch Unknown preserves its exact capsule and credit while
known unfinished Save still receives its own explicit abort.

## C2 private provider lifecycle and bounded work

`begin()` keeps private LFCS v1 for existing single-phase callers and historical
independent format proofs. Explicit `begin_phased(selector,D,B)` selects new
private schema/header2; `LFCSOWN2`/u16 version2 retain the192-byte native header
and reserved six zeros. There is no reopen, migration or fallback of temporary
files. Fixed typed phase owner facts record exact issued scopes, declarations,
accepted/remaining counts, seal and terminal disposition. Native16MiB credit,
identity checks, actual allocation readback, SQLite page4096/max4096/cache-512/
mmap0/MEMORY journal/temp/OFF sync/busy0/safe limits remain unchanged.

Claims live in STRICT WITHOUT ROWID primary-key25/class1 rows without insertion
ordinals pretending to be sorted ranks. Validate batch shape/selection/local
duplicates before SQL. BEGIN IMMEDIATE verifies native header/open phase/counts;
read all keys by exact primary-key lookup before insert. Existing valid claim
causes known rollback and Duplicate; malformed/provider/rollback failures remain
errors. Only after all keys are absent check U+K against the actual declared
claim class, insert K, conditionally update exact owner totals and commit once.

Final seal takes one exact indexed maximum key and walks primary keys once in
<=128-record pages with last key plus emitted count, exact scope/record decoding
and ordered digest. Exact count and final last-key equality with that maximum
establish EOF, including the exact empty case; no129th row is buffered. All
reads and seal publication share one explicit transaction. It uses no prefix
COUNT, OFFSET, full scan per page, growing key vector or insertion ordinal.

Retirement uses <=128 selected keys per transaction, exact seal/scope and
remaining-count checks, per-key exact affected row checks, monotone last key and
confirmed terminal EOF. Fixed phase state is not discarded until known success;
partial known retirement never enables roots, and Unknown cannot be adopted,
queried into success, retried, refunded or destructively cleaned. No whole-table
DELETE/DROP/VACUUM/rebuild or retained old authority beside new roots. Freelist
pages may serve the later table inside the same already admitted native class;
logical retirement is not physical refund or a heap/cache claim.

For K non-file occurrences, U unique claims and D directories: O(K log U)
indexed claim operations (one immediate known-open presence lookup, then bounded
batch presence/insert validation) in <=ceil(K/128) write transactions, O(U)
records in each provider seal pass, C1 seal-verification pass and bounded
retirement, then the existing root work.
No historical prefix scan, recursive checkpoint or quadratic relocated work.
Pure admission arithmetic is proposed capacity, never measured physical healthy
progress. Actual SQLite transaction/cursor semantics are checked against its
[transaction specification](https://www.sqlite.org/lang_transaction.html) and
[WITHOUT ROWID specification](https://www.sqlite.org/withoutrowid.html).

## Ownership and prospective exits

- C1 owner: state claim codecs/port/resident compatibility/scopes; common
  validation result/refactor, update entry/body ordering and external C1 proofs.
- C2 owner: construction-state provider/private SQL/profile/native version
  selection and external real-provider/independent transcript proofs.
- Root: coordinated Bridge/Server integration, root admission and cleanup,
  docs/checklists/log/evidence, checking/count/commit/push/#287 ownership.
- Independent reviewer: shared interface/custody/work/resource review; no
  overlapping product edits. Everyone preserves others' edits; only root Cargo.

Require independent frame/digest/scope/EOF vectors; exact same/cross-window
duplicates, regular aliases, directory/symlink semantics and legacy map behavior;
provider refusal before effects, foreign/stale/sealed/retired phase denial,
malformed/tampered rows and real SQL lock-induced failure/Unknown custody;
actual maximum U65536 then known retirement then D65536 in one16MiB owner;
SQL primary-key plans and bounded windows/work; complete real direct Service
Stage and known cleanup with non-file claims before roots, plus failed Stage
preservation and known Save cleanup. Scoped locked tests/examples/fmt/Clippy and
boundary/self-tests follow coherent source freeze once.

Darwin direct library/provider correctness is separately scoped from native main:
the selected Apple SQLite hard-limit readback0 remains UNSUPPORTED, four original
R1e eligible-body FAILs remain failures, Linux/native physical/protected healthy
progress bodies remain unrun. No speed PASS or #288 campaign/runner/family edit.
