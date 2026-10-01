# R1d-binding-sites: combined paged exclusive sites and base facts

> **Status: Frozen implementation contract; named direct-provider exits PASS, full R1 open.**
> Parent: published `420e6a2c0149326fe2c380ff0e4156eadd3d326d`, #287 checkpoint5921238411.
> SC-03/05/07/08. Full R1/R2–R7/#288 remain open. No next product edit preceded
> this coordinated contract selection and source/resource/point audits.

## Complete delivery and retained boundaries

Replace the actual strict prepared Server/C1 exclusive-claim plus resident alias
site/name/target-parent/base-binding-list authorities with one typed paged
BindingSites phase. Every non-file exclusive claim is a site, including a fresh
inode; regular aliases create no site. The common semantic checks retain kind,
identity, duplicate, root and failure order. Legacy public v1/v2 validators and
construction APIs remain explicit compatible paths. New strict construction uses
the same ordinary source and consumer, no capability-error fallback or workload
recognizer. Canonical v1 is independently unchanged; canonical v2 and certified
namespace parent policy remain R3.

This slice removes only the named site/parent/base-list populations. Existing
positive/absence memo, unreachable map, graph seen/pending/reachability and later
reference/count/touched/final-row/release authorities remain separate open gates.
No global/native/physical/healthy progress, speed, release or full R1 admission.

## Issued point and source before effects

`BindingSourceId` is an opaque read-only view of the existing BindingAuthority
issuer. No raw ID factory, new issuance mechanism, registry or guessed adoption.
`BindingPoint` has private encoded issuer8/parent8/headerDescriptor8/ordinal4,
all big-endian, total28. Mint from an issued DirectoryHeader and ordinal strictly
below its binding_count. Empty headers mint no point. `decode_for(source,bytes)`
requires the actual opaque source capability and exact width/issuer/positive
parent through MAXIMUM_INODE_SERIAL=i64MAX; actual row membership still belongs to the receiver.

`BindingRows::binding_source_id() -> ContentResult<BindingSourceId>` and
`binding_at(&BindingPoint) -> ContentResult<(PathName,Option<u64>)>` identify one
actual entry, with inner None a tombstone and no outer synthetic EOF. Default
unsupported is explicit for external old producers. Receiver checks its own
issuer before metadata/body, then original known failure, actual immutable
parent/descriptor/range and exact selected span. Point access never grants
DirectoryCompletion. Full cursor completion and supported immutable source remain
required; point/issuer binding is not global namespace or cryptographic name
certification.

Borrowed/Slice select an existing immutable element and clone only that name;
never recompute header NB by folding names per point. Compatibility owns its
actual issuer and delegates a new explicit RowSource::legacy_binding_at(parent,
ordinal); its default unsupported is not caught to reconstruct a full row.
FilesystemInput and both Prepared wrappers provide/delegate that bounded path.
RowSpool uses ordinal/16 to select a checkpoint block, validates adjacent spans,
decodes exactly its <=16 records with name/order/serial/end checks, then returns
one tuple. No binary name search, prefix replay or partial result on later block
failure. All six first-party producers/delegates retain the same live authority.

`SpoolPreparation::new(declaration,capacity)` performs existing pure declaration/
table/capacity admission, then owns exactly one existing BindingAuthority without
file/body effects. `source_id()` exposes its opaque view; RowSpool::create_prepared
consumes the preparation and moves that authority into the actual file/source.
Old create/create_declared remain compatible. Server pure admission creates this
preparation before native scratch/Save/body; C2 begin_sites takes this SourceId.
No mutable header enrollment/Unknown phase or second issuer is introduced.

## Closed records, versions and arithmetic

Actual StateTable currently allocates DirectoryRoots1 and BindingClaims2. This
contract allocates **BindingSites3**, under a separate `SiteScope` containing
StateScope81 plus opaque SourceId8, encoded89. Sites use phase1/table3; roots use
phase2/table1 in the same live native selection. `SiteConstructionState` exposes
IndexedState + BindingSiteState through explicit indexed() delegation; old
ConstructionState/ConstructionScopes v2 stay unchanged.

| Item | Exact selected encoding / class |
| --- | --- |
| SiteKey | token8/phase8/table3-byte/positive serial8 =25; no root/claim key codec |
| SiteRecord | keylen2/key25/vallen4/flags1/point28 =60 |
| Flags | bit0 authenticated base record (immutable); bit1 saw_base; bit2 any_legal_base. Only0/1/3/7 valid; birth0/1 only. |
| SiteObservation | selected key25 + legal byte0/1; no point replacement |
| SiteScope | StateScope81 + SourceId8 =89 |
| SiteBirthSeal / SiteSeal | version1/scope89/count8/recordbytes8/digest32 =138; distinct typed domains |
| SiteMembership | approved birthseal138 + max-present1/max-key25 =164 |
| SiteAppend | version1/scope89/count2/recordbytes4 =96, +128*60 =7776 |
| SitePage | finalseal138/last-present1/last-key25/count2/bytes4/EOF1 =171, +128*60 =7851 |
| SiteParentPage | membership164/parent8/last-present1/last-ordinal4/max-present1/max-ordinal4/count2/bytes4/EOF1 =189, +128*60 =7869 |
| Logical admission | D/B<=65536, max(63D,60B)<=4128768; site65536 framebytes3932160; no increase |
| Native owner | unchanged16MiB allocation, page4096/max4096, cache512KiB, prior SQLite limits unchanged |

Fixed pending/page/observation windows are <=128 and header-inclusive <=64KiB;
actual native record Vec capacities must also be <=128. Decoded record layout,
fixed attempted capsules/local sort buffers, hashers, simultaneous pages/memo/
SQL/index/cache/OS owners are separate from frame arithmetic and must be measured
or remain explicitly unavailable. Header200 is a format change, not extra quota.

C2 adds Plan::SitesThenRoots and `begin_sites(selector,D,B,BindingSourceId)` only
for private LFCS profile3. Header is `LFCSOWN3`, u16version3, six reserved zero
bytes, unchanged token/selector/binding/nonce/parent/directory/file prefix layout
through192, then SourceId8 through200. Native binding domain is
`layerfs/construction-state/native/v3\0`, followed by existing nonce32,
parentIdentity24/directoryIdentity24/fileIdentity24/selector32/tokenBE8, then
SourceId8. Existing v1/v2 header192/binding-v1 bytes/APIs are preserved exactly.

Private schema3 keeps application_id0x4c464353, MEMORY/OFF/no-WAL/no-sync,
workers0/busy0/mmap0, limit-column16/variables8 and all existing limits. A fixed
root owner and separate fixed site_owner avoid a17th column. Native table
binding_sites has key/flags/point/parent/binding_ordinal; redundant parent/ordinal
must match the point before trust. Parent is positive <=i64MAX and lossless SQL
INTEGER; issuer and opaque header descriptor are fullu64 and stay BLOB bytes.
UNIQUE(parent,binding_ordinal) supports birth-order scans; a partial index with
exact `(flags&1)=1` predicate supports existing-parent membership/projection.
These are TWO real additional B-trees, not free logical frames. Maximum-class
proof must include their pages/native allocation and later root reuse under the
same16MiB owner before any supported-profile claim.

## Closed phase ports and exact custody

BindingSiteState supplies site_capacity/get/insert_batch, close_membership,
parent_present/parent_page, observe_base_batch, final_seal/sealed_page, retire and
abandon. All take exact SiteScope/source association or the acknowledged typed
membership/final seal; foreign scope/source refuses before effects and cannot
terminalize another selected owner. Public typed methods do not accept arbitrary
raw source IDs/flags/values. Every parent-addressed port and persisted point checks
that same positive <=i64MAX range before SQL or signed binding casts. Parent-page
Some(0) is a valid binding ordinal; explicit presence bytes distinguish it from
None, with zero placeholders required only for absent ordinals. Parent pages expose immutable birth records only;
mutable fact pages are not read through a prematurely frozen full-record seal.

Insert batches are unordered <=128; validate all key/point/birth fields,
selected prefix/class and existing rows before effects. Fresh acknowledges all;
Duplicate acknowledges none, rolls back current batch known, retains prior rows
and terminalizes. C1 checks pending membership and provider presence immediately,
preserving duplicate precedence before later parent/kind errors. One producer.

C1 SiteBirthLedger folds ACKNOWLEDGED original birth frames in strict
(parent,bindingOrdinal) source order, fixed previous tuple/count/hash only.
Domain `layerfs/binding-sites/birth/v1\0` +Scope89 +frame60 records +countBE8+
recordbytesBE8. After the last source row is completed and pending births flushed,
close_membership takes the expected SiteBirthSeal. C2 validates indexed birth
order, immutable point/flag0-or1, exact expected U/60U/digest and MAX in one known
transaction before facts. Returned SiteMembership must equal expected birth
seal; membership/point/base bit can never change afterward.

Observe batches <=128 contain key/legal only. Each selects a known existing site
with bit0, reads/checks every old row before effects and updates ONLY OR2|(legal?4:0).
Never clear flags, replace a point or change key/parent/ordinal; repeated facts
remain monotone. Exact old/proposed flags and keys are retained for failure
custody; an unchanged fact is acknowledged explicitly, no hidden retry.

Finalseal rehashes the immutable birth projection in parent/ordinal order,
compares approved expected digest/U/60U, THEN folds full current records in key
order in the SAME transaction. Domain `layerfs/binding-sites/final/v1\0` +Scope89
+frame60 records +countBE8+bytesBE8. No ORDER BY without an actual matching index,
prefix COUNT/OFFSET/rank or full population collection. C1 independently verifies
final key-ordered pages/digest/count/bytes/EOF, resolves every active point through
the same live source and checks exact candidate child before predicate/retirement.
This final alias predicate/verification/retirement occurs immediately after alias
facts, BEFORE remaining shared root/cycle checks; a later selected checker
refusal abandons once and denies roots even after known retirement.

Retirement deletes exact keys in <=128 windows against seal/U/MAX/remaining/after,
verifies primary table and both projections exactly empty, COMMITs and observes
actual native allocation BEFORE roots become eligible. No proportional allocation
or adoption after that permission. Unknown retains exact stage/scope/source,
pending birth keys/points or observation old/proposed flags, expected/proposed
seals/MAX and remaining/after. No resend/query adoption/refund/rollback-on-guess.
Known failures preserve original error/cleanup; abandon is metadata-only, exactly
selected and logically idempotent, without SQL/native cleanup or changing Unknown.
Source identity acquisition/mismatch is preselection: original getter errors and
foreign input SourceId refuse without consuming another selected owner. After
exact input/scope SourceId match, early/shape/source/checker/postchecker
root-constructor failures abandon the selected attempt once;
Server Save cleanup remains independent of native scratch Unknown.

The private SiteStage codes are BirthOpen0, Facts1, FinalSealed2, Retiring3,
Retired4. They are distinct from old claim-stage codes. Only Retired4 allows
phase2 roots. Root-owner/site-owner fixed metadata each stay <=16 columns; no
reopen/import/adoption or post-receive enrollment API exists.

The exact shared public port shape is:

```rust
trait BindingSiteState {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity>;
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey)
        -> ContentResult<Option<SiteRecord>>;
    fn site_insert_batch(&mut self, scope: &SiteScope, records: &[SiteRecord])
        -> ContentResult<ClaimAdmission>;
    fn site_close_membership(&mut self, expected: &SiteBirthSeal)
        -> ContentResult<SiteMembership>;
    fn site_parent_present(&mut self, members: &SiteMembership, parent: u64)
        -> ContentResult<bool>;
    fn site_parent_page(&mut self, members: &SiteMembership, parent: u64,
        after: Option<u32>, limit: SiteParentPageLimit)
        -> ContentResult<SiteParentPage>;
    fn site_observe_base_batch(&mut self, members: &SiteMembership,
        observations: &[SiteObservation]) -> ContentResult<()>;
    fn site_final_seal(&mut self, members: &SiteMembership)
        -> ContentResult<SiteSeal>;
    fn site_sealed_page(&mut self, seal: &SiteSeal, after: Option<SiteKey>,
        limit: SitePageLimit) -> ContentResult<SitePage>;
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()>;
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()>;
}
```

SiteScope::new(StateScope,BindingSourceId) checks table3. SiteRecord::birth(scope,
serial,point,has_base) and decode(scope,bytes) check exact key/issuer/flag framing;
encode is60bytes. Point/key/birth flag cannot mutate; observe(legal) yields only a
checked monotone value. SiteBirthLedger::new(scope)/acknowledge(source-order
records)/seal() supplies SiteBirthSeal. SiteMembership binds that exact seal and
MAX; final SiteLedger/SealedCursor validate key order and final digest separately.
Each parent page masks mutable bits back to immutable birth flags, validates
parent/source/order/range and exact max/EOF, with original typed errors/sticky
cursor refusal. Concrete private helper naming may vary without altering these
public methods or encodings.

SiteConstructionScopes::new(StateSelection,BindingSourceId) selects sites1/table3
and roots2/table1. C1 check_with_sites returns CheckedTopologyInput and abandons
exactly once on failure. New build/update_filesystem_binding_rows_with_site_state
functions take the existing six construction arguments, using SiteConstructionState
and SiteConstructionScopes. Old with_construction_state functions stay v2. No
trait upcast dependency. Root Server calls only the new explicit path.

SpoolPreparation::new(SpoolDeclaration,u64), source_id()->BindingSourceId and
RowSpool::create_prepared(PathBuf,SpoolPreparation) are the shared receive contract.
Pure native Plan arithmetic precedes native state token/slot/file/SQL effects;
a caller's already issued source capability is not recreated/refunded.

## Alias algorithm and independent expected result

Retain the current semantics, including uncertified v1 base facts. Active means
immutable has_base and parent not unreachable. A scalar activeStoredCount provides
the existing early return if no active site; do not perform an added base walk
that can create a formerly absent walk-limit refusal. Graph pending/seen/memo
remain their existing separate admitted/open authority.

For each base parent visited once, exact indexed parent membership establishes
whether it owns active sites. A restated name skips the WHOLE old base entry when
the final candidate has an active site at that same parent/name, even if old and
new serial differ. Nonrestated old entries keep existing child-directory traversal.
For an active old child site, evaluate the original survives decision immediately
and OR its known base observation through bounded native facts. Then follow the
parent's immutable projected sites once in bindingOrdinal/name order, including
new names absent from base; never follow every site once per base name.

Final active acceptance is exactly `!saw_base || any_legal_base`. Do not strengthen
to every old binding removed, assume a certified single-parent base, or include
fresh/regular/unreachable sites. Binding points return full names only when
needed, and no full names/target parents/base-binding history are retained.
Known source/provider failure remains explicit. Evaluating survives/fact dispatch
while walking replaces the old final child-sort decision order; fault precedence
between independent failures changes transparently, with no synthetic success.

## Ownership and prospective exits

- C1 owner: issued Point/SourceId/SpoolPreparation and all6 producer delegations;
  focused Site scope/key/record/seal/page/cursor/port/coordinator modules; new
  SiteConstruction APIs; common validator/aliases and exact external point/state/
  semantic/custody/reference tests. Old v1/v2 routes remain explicit. No Cargo.
- C2 owner: Storage construction_state focused site profile/SQL/index/session/
  lifecycle/adapter/status/native source association plus external real-provider
  maximum/projection/failure/Unknown tests. No C1 or Server edits, no Cargo.
- Root: sole shared/Bridge contract owner (wire unchanged), Server preparation/
  source/native/common construction/cleanup integration, owning tests/docs/log/
  checks/count/commit/push/#287. Reviewer read-only; all workers preserve others.

Before coding owners acknowledge these exact interfaces/formats; routine private
file/function choices are autonomous and documented. Exit proof: independent
literal Point28/frame60/birth/final vectors; exact foreign/stale/source/ordinal/
EOF/tombstone/fullname tests, <=16 spool decodes and zero full-row/prefix work;
independent alias star/permutation/move/tombstone/regular/fresh/unreachable/orphan/
uncertified-base OR/restated-oldserial and original failure outcomes; birth/final
immutable mutation refusal; duplicate beyond128 before later errors; actual native
65536-sites+bothindexes->retire->65536roots within16MiB, exact schema/query plans;
real process/provider barriers for birth/close/facts/finalseal/retire Unknown and
no root permission/refund/adoption; real Server Stage/Save/catalog/cleanup/prior
bytes. Scope own locked tests/examples/fmt/Clippy/boundary+selftests at coherent
freeze, retaining every failure/correction and exact source/LOC comparison.

No benchmark campaign/runner/family or #288 update. Count/resource diagnostics are
bounded and separate from speed. Global/native heap/cache/RSS/Linux/protected
progress remain unavailable/unrun unless their actual separate gates pass. Native
Apple required32MiB readback refusal stays unsupported, StrictServerMemory false.
