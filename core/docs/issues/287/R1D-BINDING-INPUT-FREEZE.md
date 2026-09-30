# R1d-binding-input: exact row/header interface and private spool v2

> Implementation input, exits UNRUN. Selected2026-10-01 at published
> d1e478ffb97905eb77acdeef288fa091c40cae2d; no bound or canonical proof follows
> from this freeze. R1e provider checkpoint remains the current coordinated gate.

SC-03/05/06/07 affected. Smallest complete responsibility: actual prepared
Server input, C1 directory consumers and canonical builder pull scalar names
from the same sealed source, with exact indexed changed-name probes. Wider
paged graph/reference/release populations and strict global/physical admission
remain separate exits; a remaining whole graph/vector cannot be called bounded.

## Shared C1 interface

Keep existing DirectoryUpdate, RowSource, PreparedRows and public entry points.
Add rows/binding types, reexported by rows/mod.rs (declarations only):

```text
BindingLookup = Unmentioned | Absent | Present(u64)
BindingAuthority::new() -> ContentResult<BindingAuthority>
BindingAuthority::header(parent:u64, ordinal:u64, bindings:u32,
                         wire_name_bytes:u64) -> ContentResult<DirectoryHeader>
BindingAuthority::accepts(&DirectoryHeader) -> bool
DirectoryHeader accessors: parent(), binding_count(), wire_name_bytes(), ordinal()
DirectoryHeader is a fixed private-field scalar record; no caller-mutated offsets.
DirectoryCompletion accessors: parent(), binding_count(), wire_name_bytes()
DirectoryHeaderSource::next_header() -> ContentResult<Option<DirectoryHeader>>
BindingRowSource::next_binding() -> ContentResult<Option<(PathName,Option<u64>)>>
BindingRowSource::finish() -> ContentResult<DirectoryCompletion>
BindingRows: RowSource
  directory_headers() -> ContentResult<Box<dyn DirectoryHeaderSource + '_>>
  directory_header(parent:u64) -> ContentResult<Option<DirectoryHeader>>
  bindings(&DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>>
  binding_for(parent:u64,name:&[u8]) -> ContentResult<BindingLookup>
PreparedBindingRows: PreparedRows + BindingRows
```

Authority is issued once per source, uncloneable/unforgeable as a public owner;
use a checked monotone opaque token with no growing registry, or the existing
issued-selection primitive where sound. Header carries its private issuer token
and selected ordinal/count/bytes. The issuing source must check both issuer and
its exact selected slot before opening a cursor; header offsets are never trusted.
Completion can be constructed only by an exact finished cursor. Its private
issuer/ordinal bind it to that header; expose a matches(header) check. No heap
population, pointer exposed as identity, or foreign-source adoption is allowed.

Cursor finish is valid only after exact EOF; it does not drain hidden remaining
rows. Early finish, count/byte/order/span/provider errors are terminal/sticky,
with no representation change. One current full name, one previous full name
and fixed state suffice; EOF checks exact slot-end and completion. Root supplies
an ordinary fallible Iterator adapter to existing directory::apply_bindings.
No129th row prefetch. If a page API is introduced,128 records AND64KiB apply.

SliceBindingRows::new(&FilesystemInput) -> ContentResult<Self> is the explicit borrowed slice adapter:
no complete DirectoryUpdate clone; point lookup binary-searches original names
and never recomputes whole-row byte totals per point. CompatibilityBindingRows::new
(&dyn PreparedRows) -> ContentResult<Self> is an explicitly selected legacy adapter for external old
RowSource implementations; it may request their complete rows under the old
profile and must never be caught-error fallback for the bounded entry. PreparedBindingUpdate retains the old PreparedUpdate fields but stores
rows:&dyn BindingRows, delegating methods directly. The old PreparedUpdate is
explicit compatibility. Root owns update_filesystem_binding_rows_with_state and
old-wrapper delegation; issuance errors are never ignored. Rust1.85.1
has no trait upcasting: consumers call supertrait methods directly or use narrow
generic bounds, not casts between dyn supertraits.

## Exact private format and writer

Private RowSpool format2 is allocated here; it is not a canonical/Bridge/schema
version. No open/import API exists, so no adoption of old private spools occurs.
The48-byte header has u64BE D,I,F,B,NB,version2. Keep32-byte slots exactly:
key8/offset8/length8/records4/kind1/reserved3zero. Kinds1directory/2inode/3fresh
unchanged. Checked arithmetic replaces unchecked slot offset+length and D+I+F.

Directory key supplies parent; records supplies B_i. Payload is8Q_i relative
checkpoint offsets followed by u8 name-length/full-name/u64BE child records.
Child0 is tombstone. First checkpoint is implicit at ordinal0; explicit offsets
name ordinals16,32,..., Q_i=floor((B_i-1)/16), or0 when B_i=0. Offsets relative
to the selected directory data start are checked, increasing and inside its exact
span; a complete pass verifies each at its true expected ordinal. Empty directory
has zero payload/checkpoints, real exact completion, and distinct builder behavior
for maintained versus genuinely new empty directories. Full-name255 works.

Inode key supplies serial; payload65 bytes kind1/content32/metadata32. Fresh key
supplies serial; its payload is empty. Validate kind/reserved/range/length at read.
No row/offset may select the slot table or another payload.

SpoolDeclaration public fields: directories:usize,inodes:usize,fresh:usize,
bindings:u64,wire_name_bytes:u64. It validates totals and computes checked
required_bytes_upper():48+32(D+I+F)+8Q_max+(NB-B)+65I,
Q_max=0 if B=0 else floor((B-1)/16). RowSpool::create_declared(path,
declaration,capacity) checks that upper bound before creating the file and stores
exact declared totals. Existing create(path,D,I,F,capacity) is deliberately
undeclared compatibility: accumulate actualB/NB and write them at seal, never
pretend guessed totals. Both use the same private format/algorithm.

Streaming methods: begin_directory(parent:u64,bindings:u32),
push_binding(name:&PathName,child:Option<u64>),
end_directory(parent:u64,bindings:u32,wire_name_bytes:u64).
Reserve/check the directory checkpoint region before first binding; reserve each
record before writing. Publish its slot only after exact row count/bytes/order
acknowledgement. Partial row cannot be read or sealed. Old push_directory delegates
these methods without a temporary whole payload; inode/fresh methods retain APIs.
General header/binding reads require sealed source. completed_directory(parent)
-> ContentResult<bool> only answers acknowledged-slot presence before seal for
receive-time DirectoryDeclaration; it never exposes partial names. Server calls
seal after common decoder/global EOF, before check_subjects or C1. Seal validates
declared/actual counts,B/NB, no unfinished row and exact end;
flush is visibility only. The header is not a cryptographic digest/native identity
or tamper proof. Existing private-file native/Drop cleanup limitations remain
explicit and cannot become scratch Unknown authority or a stronger PASS.

## Existing admission preservation

W=1+12D+NB+73(I-A)+25A, A=patches+declarations.
S0=48+32N+12D+(NB-B)+73I+8F, N=D+I+F.
S2=48+32N+8Q+(NB-B)+65I=S0-12D-8I-8F+8Q.
8Q<=B/2<=32768 under currentServerB<=65536. For N>=4096, removed
bytes>=8N>=32768; otherwise S0-W<=88N+47<=360407 and additional
checkpoint upper fits unchanged1MiB slack. Thus every valid old-fitting Server
declaration remains admitted. No count/body/slack/profile increase. The14,000
fresh-star overhead994159 old versus777131 new is arithmetic, not a measured row;
adding dense8B offsets without retirement would improperly exceed the slack.
Root/Server checks predictable upper before scratch/Save/body effects.

Point work: O(log D) slot search + O(log ceil(B_i/16)) full-name checkpoint
probes +<=16 local decodes. No prefix replay, hashes or truncated names.
Sequential work: D slots+B decodes+Q checkpoint verification, with fixed semantic
passes recorded separately. One local block<=4224 bytes. These are prospective
work/representation bounds, not native-call/residency/speed proof.

## Coordinated Bridge and Server

Root owns the sole Bridge common decoder/contract. Preserve wireversion/bytes.
Add PreparedBindingSink with begin_directory(parent:u64,bindings:u32),
binding(name:&[u8],child:Option<u64>),
end_directory(PreparedDirectoryCompletion),identity(PreparedIdentity).
Bridge PreparedDirectoryCompletion public fields parent:u64,bindings:u32,
wire_name_bytes:u64. read_prepared_bindings has the same arguments as the old
reader but takes this sink. The old reader is an explicitly selected collector
over the common decoder. Before begin, reject count beyond remaining total names
or minimum11*count beyond remaining name-byte budget; check10+name-length against
remaining byte budget before copying/writing each name. Fixed current/previous
names, role/order/root-binding and full aggregate EOF checks remain exact.

Server's SpoolSink directly uses streaming methods; no changed-name Vec.
Root delegates Server-specific files to a non-overlapping owner after freeze;
root retains Stage pre-admission/common C1/Bridge/graph integration. C1 row owner
owns rows/* and input.rs/adapters/external row/index tests only; no update/validate/
directory/Bridge/Server/docs/manifests/reference-run edits. Everyone preserves
others' edits. Root coordinates Cargo after real common caller freeze.

Independent names/children/full-byte oracle covers15/16/17,127/128/129,1024,
255-byte names, exact tombstone/unmentioned/replay/count/order/corrupt-span/EOF,
foreign header, pre-effect refusal and real file cleanup. Preserve accepted4088/
4096/4097 and4120 wide-base rebind, #256129/257/1025 and sealed v1 roots. Actual
Server wide Stage composition/native cleanup and narrow provider count/allocation
observations are required for this responsibility; full graph/global/strict/
Linux and #288 qualification stay open. No campaign, runner/family or speed PASS.


### Shared-port refinement before implementation freeze

BindingAuthority::cursor(header:&DirectoryHeader,rows:I)
-> ContentResult<CheckedBindings<I>>, I:Iterator<Item=ContentResult<(PathName,
Option<u64>)>>, makes the public port independently implementable. This checked
production cursor proves issuer/full-order/range/count/NB/exact iteratorEOF with
sticky failure and no hidden drain; it alone constructs completion. Built-in
spool also verifies exact physical span/checkpoints. General seal/read phase
and completed_directory presence are unchanged. Old generic PreparedRows entry
points deliberately select compatibility; slice callers explicitly choose
SliceBindingRows plus bounded entry (no runtime subtype recognition). Constructors
are fallible. PreparedBindingUpdate/six-argument bounded entry and generic legacy/
bounded CheckedInput preserve shared metadata without Rust1.85 trait upcasting.
