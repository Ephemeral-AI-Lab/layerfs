# Store Doctor table coverage

> Implemented inventory contracts; qualification receipts belong to the named
> tests and parent issue116. This is not a new database format or repair API.

Every logical check below uses the same SQLite read transaction or PostgreSQL
REPEATABLE READ READ ONLY snapshot as canonical readers. Schema applicability
comes from that snapshot, not guessed layout names. Every supported schema is
checked against its actual table set at admission. SQLite diagnostic admission/audit compares the saved source/definition and
every actual schema object against the shipped format in the same source
transaction. Rewriting the saved definition after dropping a required trigger
does not establish schema authority. Bounded normative DDL scratch is private;
no source migration or repair runs. Catalog incarnation is revalidated in that
same audit view, so a prior successful admission cannot authenticate a replaced
Store.

Physical engine checks have
separately stated observation intervals. Quick retains metadata checks and
explicitly skips exhaustive pack/object/namespace guarantees.

| Table/category | Role and inventory key | Validator | Quick/full |
| --- | --- | --- | --- |
| `history_meta` | Catalog authority, singleton `id` | Binding/catalog admission, exact schema, identity/version/incarnation/scalar shape | Both |
| `store_policy` | Frozen format/policy and allocator singleton | Existing policy validation, row count, pack/ordinal allocation boundaries | Both |
| `pack` | Authoritative physical representations, `pack_id` | Actual descriptors and length/digest/domain; every group directory, extent and record boundary through Core | Shape/accounting in both; complete authentication full |
| `pack_unit` | SQLite group-row physical representation, `pack_id,group_number` | Existing bounded prefix and incremental unit reader verifies every ordering/count/range/length; engine FK check catches orphan units | Engine structure/FK quick; complete pack walk full; absent in monolithic/PG |
| `object_location` | Authoritative winning locator, `object_id` | Fixed-width/scalar shape, canonical identity/role/length, locator binding and every actual pooled/delta dependency | Shape/accounting quick; complete canonical/content semantics full |
| `inode_value_block` | Authoritative ordinal data, `first_ordinal` | Actual4000-byte bound before extraction, all CRC/framing/values including unused blocks; nonoverlap and allocation boundary | Both |
| `content_signature` | Advisory bounded ring, `slot` | At most8192 rows, slot/stamp arithmetic and fixed32-byte identities/signatures bounded inside SQL | Both |
| `file_digest` | Advisory reuse hints, `digest,length` | Actual fixed-width metadata; stream content BLAKE3/length where root exists | Metadata quick; logical bytes full |
| `binding_hint_block` | Advisory hints, `grp,seq` | Actual4000-byte bound, key shapes, all CRC/framing including unused hints | Both |
| `layer_stack` | Authoritative catalog, tagged ID | Typed row/name/scope/profile, referenced head ownership and full root context | Rows/graph quick; namespace qualification full |
| `branch` | Authoritative catalog, tagged ID | Typed row/name, stack/base/head/fork relationships | Both |
| `layer` | Authoritative history, tagged ID | Actual identity derivation, parent/stack/source Commit/Branch/root, graph acyclicity; every historical context retained | Rows/graph quick; namespace full |
| `commit` | Authoritative history plus persisted merge facts, tagged ID | Typed metadata/identity, parent/base/second-parent, graph acyclicity even when a changed parent also invalidates the saved identity; fact grammar streamed in bounded byte windows through the owning codec | Both; canonical namespace full |
| `layer_lineage` | Derived index, `layer_id` | Graph-derived height/stack and exact pre-publication overwrite reconstruction | Both |
| `commit_lineage` | Derived index, `commit_id` | Independently derived depth/newest/merge; saved run partition validated against actual first parents | Both |
| `absorbed` | Derived index, `run,depth` | Rebuilt earliest publication coverage from actual local Commit ancestry | Both |
| `reach` | Derived index, `key,x` | Rebuilt publication frontier from validated partitions and authoritative graph | Both |
| `scope_allocator` | Retained allocation authority, `scope_id` | Exact identity widths, nonnegative highwater and catalog authority; no recycling or inferred equality to currently used serials | Both |
| `store_totals` | Publication accounting and last-Init telemetry, singleton `id` | Nonnegative shape; logical=sum retained Layers+Commits, canonical=sum winning locator lengths, physical=sum retained pack descriptors by domain | Both |
| `init_operation` | Authoritative operation-private Init state, `operation_id` | Epoch/phase/source/scope/stack shapes, exact held row/payload charge, peak/removed accounting inequalities | Both when schema includes acquisition |
| `init_entry` | Operation-private staged namespace, `operation_id,parent_position,name` | Bounded names/inline portable metadata; kind/root/position/host shapes; active parent/canonical/position relationships | Both when applicable |
| `init_host_file` | Operation-private host identity, `operation_id,canonical_position` | Fixed evidence/identity/path widths; active canonical/alias relationships; exact charge including constructed roots | Both when applicable |
| `merge_operation` | SQLite persisted private working owner, `operation` | Token/touched/operation shape; does not expire or delete owners | Both; PostgreSQL other-session temporary tables excluded |
| `merge_record` | Opaque private indexed working values, `operation,kind,key` | Owner relation, u32 kind,32-byte key and existing64KiB apply-value envelope, without materializing values | Both when persisted; private bytes have no canonical-object meaning |
| `sqlite_sequence` | Engine allocation history, `name` | Nonnegative sequence; deleted owner/allocation gaps remain legitimate | Both when present; no counterpart table in PG owned schema |
| `store_lane` | PostgreSQL publication singleton, `id` | Exact singleton shape without FOR UPDATE or publication admission | Both; absent in SQLite |
| Indexes/triggers/constraints and DB pages | Physical schema/engine metadata | Actual schema contract plus SQLite original integrity/quick/FK checks, or supported separately labeled PG physical checks | Mode-specific engine coverage |

There is no foreign-key requirement from an acquisition's constructed content
root to a published object: Init records file/symlink/directory roots before Save
publication, and cleanup precedes final publication. Inactive/failed/discarding/
uncertain operations can retain partially removed parents or aliases. Their
remaining row shapes and charges are still checked. Portable Init metadata is
an inline32-byte mode/mtime blob, not an object identity. Host paths/evidence are
not re-read from the machine's filesystem during Store diagnosis.

Stored objects and physical representations are distinct inventories. Losing
physical duplicates need not each have a winning locator; a first-wins locator
can point forward to a physical dependency. Applicable local semantics of retained
unreachable objects are checked without inventing a surviving namespace scope.
Historical namespace qualification deduplicates by full root/scope/profile context,
not canonical root alone. Removed projects and allocator reservation gaps are
legitimate. Advisory damage is reported separately from authenticated content loss.

The Init telemetry columns record the previous acknowledged Init; they are not
recomputed from today's active working rows or current projects. PostgreSQL
identity sequences and another session's temporary merge scratch are engine/session
metadata, not extra persistent Store tables to copy into an audit. Engine-owned
physical checks and schema admission cover those applicable physical structures.

## Persisted adversarial qualification map

These crate-local fixtures invoke the actual Doctor inventory and readers on
explicitly Disposable Stores. Each damaged fixture starts from a verified valid
control. Raw mutations restore shipped trigger DDL before diagnosis. A corrupted
history parent can invalidate its identity and form a cycle simultaneously;
Doctor retains typed graph facts for separate topology findings and blocks
lineage reconstruction until authoritative findings are resolved.

| Requirement | Persisted proof in `engine/crates/layerfs-persistence/tests/` |
| --- | --- |
| B1 acquisition/auxiliary | `diagnostic_inventory.rs`: active unpublished roots, pending file identity, aliases, all cleanup phases, bounded paging, malformed metadata/owners/charges, opaque retained merge state and deleted sequence gaps |
| C01 physical framing | `diagnostic_framing.rs`: resealed truncation, malformed directory, bad unlocated ordinary record tag, unlocated Chunk/Singleton/compact WholeFile truncation, descriptor digest/length; `store_diagnostic.rs`: oversized body before extraction |
| C02 canonical identity | `store_diagnostic.rs::retained_object_corruption_with_resealed_pack_is_detected_and_quick_excludes_it` |
| C03 winning locators | `diagnostic_dependencies.rs`: wrong role/length/group/record and missing winning base with its physical copy retained; independent valid objects remain inventoried |
| C04 dependencies | `diagnostic_dependencies.rs`: forward dependency, missing base, actual two-object cycle, policy depth boundary, wrong base role and fully replacing delta requiring corrupt base |
| C05 ordinals/blocks | `diagnostic_content.rs`: missing pooled ordinal and CRC damage before/after unbinding its pooled object; `store_diagnostic.rs`: unused malformed oversized ordinal/advisory blocks |
| C06 file mapping | `diagnostic_content.rs`: valid-hash payload bounds, logical lengths, child summaries; real sparse constructor/Save negative control |
| C07 attributes/symlinks | `diagnostic_content.rs`: malformed portable metadata, nonportable keys/value kind and NUL symlink; dangling target accepted |
| C08 namespace | `diagnostic_namespace.rs`: missing inodes, aliases, islands, counts, cycles and same canonical root under incompatible scope/profile |
| C09 retained contexts | `diagnostic_namespace.rs`: corrupt historical root survives a valid current publication, full catches it and quick names omission; C02 fixture has no surviving namespace |
| B3 authority/derived | `diagnostic_history.rs`: missing/cross-stack heads/bases/parents, identities, graph cycles, Layer provenance; each lineage/absorbed/reach family mutated independently; valid forced history/removal retained data |
| B3 exact overwrite | Existing `diagnostic_overwrite.rs`: late publication, Commit/Layer merges, retained forks, independent stacks and seven altered overwrite counts |
| B1 unbounded facts | `diagnostic_facts.rs`: large text with split UTF-8, units grammar, flags/extents/UTF-8 damage, NUL-prefixed large text and SQLite STRICT BLOB enforcement |

Core's existing canonical construction/qualification tests remain the algorithm
proof for unchanged grammar details; these persisted tests establish that Doctor
reaches the rows and propagates those validators. PostgreSQL access/physical
qualification and concurrent snapshot/release proof use their separate owning
fixtures. Full audit does not reconstruct an unlocated compact payload's missing
canonical identity or invent dependencies between losing physical representations.
