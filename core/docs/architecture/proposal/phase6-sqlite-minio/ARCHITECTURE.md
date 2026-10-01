# Proposed authority and storage boundaries

> **Status: Current planning checklist; no release candidate exists.**
> Source, scope and evidence: [README](README.md).

## Responsibility split

**Proposed:** replace the heavy host construction/data-processing role with a
small metadata authority. Global SQLite still needs an owning process and API.
Sharing its database file among cloud daemons is not the proposed consistency
model.

```text
Applications / generic commands / ordinary FUSE mutations
                           |
                         Daemon
              live metadata: embedded SQLite
              data: streaming C1/C2 construction
                   /                       \
          immutable pack I/O          metadata/control API
                 |                           |
               MinIO                 Metadata service
          physical pack objects       global SQLite
```

| Owner | Proposed authority | Required boundary decision |
| --- | --- | --- |
| Daemon SQLite | Live inode/name/extent facts, owners/locations, pending edits, selected generations and retirement records | Database lifetime, Workspace keys, immutable capture, handle/pin accounting |
| Daemon/client C1/C2 | Chunking, canonical identities, exact membership lookup, compression/delta selection and packing | Format compatibility, one construction producer, upload admission and validation trust |
| MinIO | Physical immutable pack bytes | Object identity, create/overwrite policy, authorization, ACK meaning and retrieval validation |
| Metadata service SQLite | Committed namespace/history, selected chunk locators, Branch refs, authorization and publication outcomes | Transaction boundary, conditional head update, fencing and authenticated namespace certification |

SQL indexes hold populations; selected roots and locators identify immutable
sources. No second mutable representation may silently become authoritative.
Define which tables are reconstructible indexes and which are required metadata.

## CAS, packing and deltas

**Proposed:** retain logical chunk identity separately from physical pack
identity. Small chunks and small-file payloads can share packs; SQL records exact
locations, lengths and encoding information. Read paths must validate the
selected logical object using the format's existing integrity rules.

Competing producers may discover identical logical content while producing
different pack layouts. Freeze deterministic locator selection, verified exact
deduplication and safe loser-pack retirement. A hash match or MinIO object key
alone does not certify a namespace or enforce immutability. Specify the actual
provider operations and permissions before choosing object naming.

Delta objects require reachable base custody, bounded reconstruction work and
format compatibility. File transitions between small and large representations
must preserve byte identity and retire only unreachable storage. The existing
thresholds, codecs and pack formats require a source audit before adoption;
this proposal allocates no new format versions or thresholds.

## Open — required

- Choose the trust model for daemon-generated roots, metadata and pack locators.
  Specify how the service certifies the committed namespace and authorizes every
  publication without importing an unbounded candidate into RAM.
- Define the relationship between SQL namespace/history rows and canonical
  authenticated roots, including hard links, orphan handles, permissions and
  metadata. Preserve required v1 compatibility and independent root oracles.
- Freeze metadata-service tenancy, actor/incarnation fencing, connection pools
  and service restart behavior. Single-process local SQLite is the initial
  design candidate; replication and failover require separate contracts.
- Choose daemon and global persistence profiles separately. Existing product
  profiles and experimental WAL/fsync rows remain unchanged. Do not infer cloud
  durability from local ACKs or from enabling WAL.

The intended simplification comes from a clearer authority split, fewer duplicated
cross-host states and reuse of SQLite's indexed transactional engine. SQLite is
already present in prior paths; choosing it alone does not prove fewer operations
or faster Commit. Audit every displaced mechanism before claiming its removal.
