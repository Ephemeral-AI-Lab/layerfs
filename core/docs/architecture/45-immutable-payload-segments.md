# Immutable payload segments

> **Status:** Implemented explicit layout; performance qualification is separate.
> Source: the Durable100 payload-layout change following `7399fac994`.

`PersistenceConfig::with_sqlite_pack_layout(SqlitePackLayout::PayloadSegments)`
creates schema7 without acquisition or schema10 with `Tables`. Existing schema1–6
readers and the default Monolithic creation remain. Opens select the stored version
and exact recorded schema; no migration, fallback or reconstruction algorithm is
introduced. Acquisition remains A1 in the same durable Session.

SQLite owns Store policy, physical ID reservation, history, acquisition, pack
descriptors, object locators and extent locations. Metadata-domain packs and payload
packs at most64KiB remain immutable SQLite BLOBs. Larger payload-domain packs use
one exclusively created `<database>.payload/<minimum-pack-id-hex>.segment` per
bounded publication. The file concatenates unchanged sealed packed bodies in caller
order. `body_segment` records device/inode/length; each `pack` records segment ID
and offset. Existing canonical IDs, headers, codecs and digest semantics remain.

The Store exclusively creates its payload directory with mode0700, records its
device/inode in `body_directory`, and binds it again on reopen. Every operation
opens a no-follow directory descriptor, checks path/descriptor identity before
and after work, and opens names relative to that descriptor with no-follow and
exclusive creation. Body custody checks require a regular single-link file, the
exact catalogue device/inode/length, and the same named and opened identity. Files
are set read-only after writing. This is checked local Store custody, not protection
against an administrator rewriting the database or filesystem.

In a Durable publication, a single producer writes sealed Arc-backed slices without
a concatenated body copy, then calls `F_FULLFSYNC` on the file and its directory.
Both directory and parent full synchronization are paid during fresh creation.
Every temporary descriptor is checked closed before acknowledgement. Only then does
the same short `BEGIN IMMEDIATE` job insert segment/pack/locator/value/signature
rows and `COMMIT`. Payload durability precedes the catalogue acknowledgement.
Unsupported synchronization, an uncertain close or SQLite uncertainty quarantines
the Session; there is no weaker synchronization substitute or retry. Disposable
selects no segment synchronization, consistently with its existing crash limitation.
[Apple's fcntl contract](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html)
describes full synchronization; successful calls on the qualification host and
process-kill recovery are separate evidence, not a physical power-loss experiment.

Neither definite nor uncertain failures unlink a created segment. Its name keeps
the consumed physical ID and its exact file custody. Catalogue rollback does not
permit reusing that ID/file, guessing that an owner finished, or deleting a lost
acknowledgement. Ordinary reopen preserves referenced and unreferenced files.
Automatic global segment GC/recovery is not implemented; any future reclamation
needs authoritative reference checks and explicit producer fencing. Database and
payload directory must be retained together. Persisted physical identities refuse
an uncoordinated copy or replacement; no portable backup/migration API is claimed.

Publication admission conservatively charges two rows and128 additional physical
bytes per external pack, covering extent/segment catalogue/name overhead even
though the segment row is shared. Existing8191-row and4MiB-minus-one canonical and
physical windows, plus the existing permitted large singleton, remain. The file
length cannot exceed the existing singleton pack bound16MiB+4096. No total
file/namespace/Store cap follows. The bounded external-reference and inline Arc
lists are publication-sized; neither retains earlier publications or body copies.

Readers query the pack and segment by primary key, validate nullable layout choices
and signed coordinates before opening a file, then use exact bounded offset reads.
Strict acquisition streams the existing whole-pack authentication; scoped acquisition
uses the existing header/selected-unit authentication. Whole reads retain the existing
2MiB dependency allowance or one existing large singleton. There is no file-sized
mmap, materialization cache or alternate format. Checked close uncertainty dominates
read success/failure and follows the ordinary Session quarantine.

For K packs/rows and B submitted bytes against N catalogue rows, preparation is
O(K+B), indexed publication/read location work is O(K log N), and file acquisition
pays the bytes its existing plan requests. No global segment enumeration occurs in
product publication, open, read or checkpoint. Final benchmark allocation inventory
does enumerate all retained bodies outside the operation and charges directory,
segments, database, WAL and SHM. Native direct-write/read/sync/close observations
are separate `SqlWork.segment_*` counters; write/read counters describe complete
safe calls and successful bytes, not exclusive device traffic or syscall count.

The owning prospective selection is
[Durable100 payload candidate](../../../docs/roadmap/0.1/0.1.7/durable100-payload-segments-20261006.md).
Old Monolithic Durable100 failures stay retained under their original identity.
