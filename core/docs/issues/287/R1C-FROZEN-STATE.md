# R1c DirectoryRoots state contract freeze

> **Status: Current implementation contract.**
> Parent `06fe8363d5c317c49876d5189d374c1a34cc6010`, 2026-09-30.
> This is the concrete R1c input; implementation/provider exits remain unrun
> until recorded in the append-only log. No strict memory capability is implied.

The first complete caller is Server's prepared namespace construction through
C1 filesystem/update. Directory roots become exact typed indexed facts with
sealed bounded pages; canonical algorithms/formats remain v1. Other resident
graph/draft/reference populations remain R1d. The dated R1C design is historical
prospective input; this freeze resolves its key/digest/ownership choices.

## Selection, records and exact pages

C1 remains std/BLAKE3/telemetry only. Add a focused filesystem/state module for
the narrow supplied IndexedState capability and concrete typed codecs. Entry
modules declare/reexport only. The initial closed table is DirectoryRoots=1;
future tags are introduced only with their actual producer and private format.

An issued selection binds full nonzero operation-selector32 and compact token8.
Tokens are issued once, monotonically in the live issuer, with checked exhaustion;
callers cannot construct arbitrary issued-token fields. A private live Arc identity
distinguishes issued owners. `bind_owner(binding32)` consumes the uniquely owned,
unbound selection; live sharing/prior bind refuses, and Scope refuses an unbound selection.
A C2 session binds the full selector/token, issuer incarnation and exact native
file/directory owner. Native binding is BLAKE3 of
`layerfs/construction-state/native/v1\0`, fresh issuer_nonce32, exact native
parent/directory/file device/inode/UID/mode and full selector/token in checked
fixed big-endian fields. Nonce acquisition failure is explicit, with no clock or
hash substitute. Resident compatibility binds a unique logical owner only and
claims no native identity. Every call checks issued identity and exact binding.
No old file is reopened or guessed adopted after process/owner loss. Equal token
numbers from different issuers/native sessions are not the same authority.

The stored key is `token8 + phase8 + table1 + typed local key`, all big-endian
scalars. Positive DirectoryRoots serial8 gives25 key bytes; root value32 gives
63 encoded bytes with key-length:u16/value-length:u32 framing. Full selector and
issuer/native association remain separately owned/checked and enter the seal
transcript. They are factored out of each stored key rather than truncated.
The previous full-selector draft would require306 bytes for a maximum name.
This17-byte prefix preserves a future complete parent8+name-length2+name255
key at282 bytes, inside the existing288-byte maximum. No name hash/suffix
truncation or increased limit is selected.

DirectoryRoots is ordered write-once metadata: append batches strictly increase
keys, refuse duplicates/decreasing keys before any batch SQL, then seal once.
No mutation is accepted after sealing. Point get and page-after use exact
selection/seal; every successful nonempty continuation advances its last key.
The sealed count establishes EOF; no unaccounted129th row or whole population
vector is required. The narrow initial table does not pretend to implement
mutable graph/draft semantics before their typed producers exist.

The coordinated scope transcript is81 bytes: selector32/token8/binding32/phase8/
table1. Versioned seal is130; append header88 and output-page header163 include
exact selection, counts/bytes/cursor/EOF fields. Exact layouts are seal
`version1 + scope81 + count8 + record-bytes8 + digest32`; append
`version1 + scope81 + count2 + record-bytes4`; page
`seal130 + last-key-present1 + last-key25 + count2 + record-bytes4 + EOF1`.
Presence/EOF are0/1; absent key bytes are zero and empty non-EOF is invalid.
Every actual last-emitted key advances; empty EOF preserves its prior cursor.
`capacity(scope)` validates the
actual owner and reports the declared class for C1 pre-effect output admission.

Both append and output pages enforce <=128 records AND <=65536 encoded bytes,
including keys, framing, values and any page header.128 DirectoryRoots records
occupy8064 record bytes. Key/value/output Vec capacities/descriptors and borrowed
SQLite row/decoder overlap have their own charge; wire length is not heap size.
With the fixed header, append occupies8152 and an output page8227 bytes.
Read borrowed SQL BLOB lengths before allocating copies. A first row that cannot
fit an empty requested page returns exact capacity refusal, never a stalled
empty continuation. Close statements at every page boundary.

The private state seal uses the existing BLAKE3 implementation, with domain
`layerfs/indexed-state/v1\0`, exact full selection/issued token/phase/table,
ordered encoded records and terminal count/encoded-byte totals. This replaces
the dated scratch note's proposed SHA-256 without adding a dependency. FileSet
and result-unit SHA-256 wire seals remain their separately frozen contract.
DirectoryRoots digest/totals advance only after acknowledged ordered batches.

## Concrete C1/C2/Server responsibilities

C1 exposes issued StateSelection, closed StateTable, checked StateKey/StateRecord,
StateSeal/PageLimit/StatePage and IndexedState in focused implementation files.
The port is get/append/seal/page/release for the actual append-only table.
Supply state to a genuinely used filesystem update entry; existing independent
signatures delegate into the same canonical algorithm with explicitly scoped
resident compatibility state. No error-driven switch back to the old map occurs.
Standard <=64KiB resident-state selection is prospective, from exact known
shape; larger retained independent profiles are not silently clamped.

C2 owns construction_state session/profile/index/native ownership and shipped
sql/construction_scratch.sql. New database identity is application_id
`0x4c464353` (LFCS), private version1, distinct from current Store/history IDs.
It is metadata-only, STRICT WITHOUT ROWID, indexed primary-key range queries,
MEMORY journal/temp, synchronousOFF, foreign keysON, busy0, cache512KiB/mmap0,
with safe connection limits/readback and one producer. Store schema/packs stay
unchanged. No payload CAS, ATTACH, unindexed ordering or whole-table collect.

A narrow ScratchAuthority admits bounded simultaneous owners before dependent
creation/SQL/Save effects. Its per-operation DirectoryRoots backing class is
16MiB and max65536 rows; actual physical reservation/blocks and metadata/engine
shape must be proved separately from logical arithmetic. The exact
DirectoryRoots population is65536*63=4128768 record bytes, before physical pages.
This class does not admit the future full NameBinding/draft population.

Reuse existing safe native reservation primitives. New private files are created
once in exclusive owned directories; capture/check FD device/inode, directory
identity and owner/mode through SQLite open and cleanup. Unsupported required
native authority fails before effects. Do not reuse unknown leftovers or silently
change provider/profile. Engine-global32MiB enforcement is R1e and remains
unavailable at this profile; pragma readback alone is not its proof.

ScratchAuthority retains bounded exact known-failed/Unknown owners and credits;
no unbounded lifetime registry or guessed refund. Explicit release marks its one
attempt before close/unlink, preserves close-returned connection or owned file
identity on failure, and refunds only after verified removal. Drop never retries
an already attempted cleanup; Unknown denies mutation/destructive cleanup.
The C1 adapter retains one original typed StorageError so Server cannot flatten
Unknown into ordinary semantic refusal.

`begin(selector, declared_records)` checks the known DirectoryRoots count and
63-byte arithmetic before issued token/slot/native/SQL effects. Server supplies
the exact prepared directory total. A shape above65536 refuses without poisoning
the authority; valid smaller requests retain the same configured class. Phase1
is the actual single DirectoryRoots construction phase. Other phase/table
semantics remain unimplemented, without unused tag allocation.

Root owns Server's real prepared construction integration and coherent cleanup
before C2 finish/C5 publication. Acquire scratch admission before begin_save;
the actual slot/body ordering of existing R1a tests is preserved. Small path
and actual file allocation follow prospective known-shape selection, not a catch
and spill. No new strict/protected/concurrent capability is advertised.

## Ownership and exit

r1_catalog owns C1 state/DirectoryRoots/common update integration and external
C1 tests. r0_oracles owns C2 construction_state/runtime SQL/native resource
tests and agrees concrete port signatures with C1 first. Root owns Bridge,
Server composition, SQL LOC counter repair/tests, documentation/acceptance and
publication. Workers preserve others' edits; no worker Cargo run overlaps the
coordinated root gate. Native/global physical gaps remain explicit.

Exit requires real Server prepared Stage/update through supplied C2 state,
independent unchanged v1 roots/bytes, exact ordered128/64KiB/seal/EOF behavior,
refusal before batch effects, indexed query plans, actual native allocation and
checked release/custody. Run owning locked checks after a coherent source freeze;
no campaign or #288 issue modification. R1 remains open for its other gates.
