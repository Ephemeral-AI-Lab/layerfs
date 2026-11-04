# Access

[Architecture](../../README.md) › [Core](../README.md) › [Objects](README.md) › Access

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reading an object by ID: access context and output.

| | |
| --- | --- |
| Component | [Objects](README.md) |
| Source | [`core/crates/layerfs-content/src/object/access.rs`](../../../../core/crates/layerfs-content/src/object/access.rs), [`core/crates/layerfs-content/src/object/context`](../../../../core/crates/layerfs-content/src/object/context), [`core/crates/layerfs-content/src/object/output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |

Content opens no database, pack or file. It reads through a provider the caller
passes in, and it writes by handing finished objects to a consumer the caller
passes in.

```text
          caller (Storage, Engine)                  Content
 +-----------------------------------+     +---------------------------+
 | AuthenticatedObjects provider     |<----| read_canonical_batch(ids) |
 |   returns bytes whose ID is ids[i]|---->| decode, read, construct   |
 +-----------------------------------+     |                           |
 | FinalizedConsumer                 |<----| accept(FinalizedObject)   |
 |   stores or forwards; owns bytes  |     | (no copy kept)            |
 +-----------------------------------+     +---------------------------+
```

## Access context

`FinalizedObject::validate_context` checks an object against its direct
children in an expected filesystem context. It goes further than
[admission](kinds.md#admission), which checks one object alone.

```text
 validate_context(provider, policy, inode_scope, root_serial, timing)
   1. policy.validated(); InodeIdentity::new(scope, root_serial)
   2. derive references again; must equal object.references()
   3. dispatch by role, reading each child as a one-ID demand
      (span "content.context.read"):
        WholeFile, Chunk, Symlink   no child check
        ExtentLeaf      each chunk exists; slice end <= chunk length
        ExtentBranch    child level = level - 1; cumulative bytes and
                        extent counts match the summaries
        FileState       root page level, length, extent count match
        InodeLeaf       serials in scope; validate(serial == root);
                        content kind matches inode kind; metadata reads
        InodeBranch     child fill, level, maximum key, ordering, count
        DirectoryLeaf   serials in scope; the root serial is never bound
        DirectoryBranch child fill, level, maximum, ordering, count, bytes
        AttributeLeaf   value root is a FileState of 1..=32768 bytes
        AttributeBranch child fill, level, maximum, ordering, count, bytes
        FilesystemRoot  scope and root serial match; root inode is a
                        directory with count 0
```

| Rule | Detail | Source |
| --- | --- | --- |
| Refusals before demands | Reference and identity checks run before any child read | [`context/dispatch.rs`](../../../../core/crates/layerfs-content/src/object/context/dispatch.rs), [`context/inode.rs`](../../../../core/crates/layerfs-content/src/object/context/inode.rs) |
| One-ID windows | Each child read is its own batch of one, so a page of large children is never one oversized demand | [`context/read.rs`](../../../../core/crates/layerfs-content/src/object/context/read.rs) |
| No retry | A provider failure ends validation once, with the provider's error | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| No second hash | The object's bytes and ID cannot change after construction, so the ID is not recomputed | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| Small file states | A regular inode may point to a `FileState` below the cutoff (a no-op edit keeps it); it is not reclassified | [`context/inode.rs`](../../../../core/crates/layerfs-content/src/object/context/inode.rs) |
| Not covered | Peer authority, saved closure, serial membership, reverse bindings, aliases, cycles, full descendant topology | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |

During a Save, the provider must be that Save, so children accepted earlier in
the same Save are visible. At HEAD only tests call `validate_context`
([`object_context.rs`](../../../../core/crates/layerfs-content/tests/object_context.rs)).

## Reading an object

`AuthenticatedObjects` is the one read port. The provider promises that the
bytes it returns belong to the requested ID.

| Method | Required | Behaviour |
| --- | --- | --- |
| `read_canonical_batch(ids)` | yes | One grouped demand; values in demand order, exactly one per ID |
| `read_canonical_batch_scoped(ids, scope)` | no | Default forwards to the batch call; a provider with its own work overrides it so the work appears as a child span |
| `read_canonical(id)` | no | A batch of one; any other count is `BatchCardinality` |
| `read_canonical_scoped(id, scope)` | no | A scoped batch of one |
| `decoded_pages()` | no | Default `None`; a provider may share decoded tree pages so a read skips a copy and decode, never a check |

| Answer | Meaning |
| --- | --- |
| bytes | Canonical bytes for that ID, already authenticated |
| `MissingObject` | The provider does not hold the ID |
| `ProviderFailure { .. }` | It holds state but cannot serve it: corrupt bytes, outside this reader's visibility, or refused by a capacity |
| `IdentityMismatch` | It cannot establish that the bytes match the ID |
| `PathNotFound` | Not a provider answer: a tree read found no binding for a name |

Errors are defined in
[`contract/error.rs`](../../../../core/crates/layerfs-content/src/contract/error.rs).
Content re-checks framing and structure of what it reads but does not re-hash
it.

| Provider | Where | Source |
| --- | --- | --- |
| Store reader | Storage read path | [`read/provider.rs`](../../../../core/crates/layerfs-storage/src/read/provider.rs) |
| Open Save | Construction reads what the same Save produced | [`save/provider.rs`](../../../../core/crates/layerfs-storage/src/save/provider.rs) |
| Store ports, cached reads | Engine Store calls | [`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs), [`read/cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| Compare memo | Shares pages between the two sides of a compare | [`compare/memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |

## Output

```text
 constructor ---- FinalizedObject::new(role, canonical) ----+
                   checks envelope, computes ID once        |
                   .with_references(ids)                    |
                   .with_predecessors(list)                 v
                                         FinalizedConsumer::accept(object)
                                                    |
                                       into_parts() v
                    ObjectParts { id, role, canonical, references,
                                  predecessors }
```

| Piece | Meaning | Source |
| --- | --- | --- |
| `id` | `ObjectId` of the canonical bytes ([Identity](identity.md)) | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| `role` | `ObjectRole` ([Kinds](kinds.md)) | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| `canonical` | Owned bytes, envelope included; ownership moves to the consumer | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| `references` | Direct logical child IDs in canonical order | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| `predecessors` | Up to four advisory delta bases; `into_parts` must keep them | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |

Construction emits an object only when later input cannot change its bytes,
role or references. If `accept` returns an error, construction ends with that
error: no retry, resend or other path.

| Consumer | Use | Source |
| --- | --- | --- |
| `DiscardingConsumer` | Counts objects, total bytes and the largest object; stores nothing | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| Save sink | Storage Save | [`save/operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs) |
| Commit staging | Engine Commit construction | [`store/captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs), [`construction/driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |

## Related

- [Identity](identity.md) and [Kinds](kinds.md): what an output object contains.
- [Files: Reads and views](../files/reads-and-views.md): logical reads through this provider.
- [Physical storage: Reads](../physical-storage/reads.md) and [Saves](../physical-storage/saves.md): the provider and consumer behind these ports.
- [Telemetry](../telemetry/README.md): the `TimingScope` passed to every entry point.
- [Glossary](../../glossary.md) for LayerFS terms.

---

← [Objects: Kinds](kinds.md) · [Up: Objects](README.md) · [Core: Files](../files/README.md) →
