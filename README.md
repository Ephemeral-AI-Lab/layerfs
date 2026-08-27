# LayerFS

LayerFS is a content-addressed, copy-on-write filesystem for branchable agent
workspaces. This README describes the architecture and active source layout;
installation and operational instructions will be added when the public setup
path is production-ready.

## Architecture

**One canonical filesystem. Two persistence boundaries.**

```text
1 · CLIENTS
────────────────────────────────────────────────────────────────────────
Agents · applications · CLI                    Immutable snapshot reads
Bash · Git · editors · builds · tests           stat · list · read_range
                                  │
                                  │ begin_operation
                                  ▼

2 · PRIVATE OPERATION WORKSPACE
────────────────────────────────────────────────────────────────────────
OperationWorkspace · temporary workspace wrapper
┌──────────────────────────────────────────────────────────────────────┐
│ Direct              Linux FUSE              APFS                    │
│ no path             private mount + spool   private physical view   │
│ one exact base · recovery · private changes                         │
│ quiescence · bounded spool · cleanup · candidate disposition        │
└──────────────────────────────────────────────────────────────────────┘
                                  │
                                  │ quiesce → candidate transition
                                  ▼

3 · CANONICAL MODEL
────────────────────────────────────────────────────────────────────────
Canonical Core
resolve · read · mutate · diff · merge      FastCDC · CAS · COW · RootId
platform-free · path-free · SQLite-free
                                  │
                                  │ OperationCommit
                                  ▼

4 · WORKING AUTHORITY
────────────────────────────────────────────────────────────────────────
WorkingStore                               WorkingRecorded or Conflict
Branch: OperationVersion ──▶ OperationVersion ──▶ HEAD
nested child Branches · immediate-parent merge · rollback
                                  │
                                  │ explicit Push / Fetch
                                  ▼

5 · SHARED DURABILITY
────────────────────────────────────────────────────────────────────────
Authenticated service  ───────────────────────────────▶  DurableStore
missing objects and accepted history only

Durable Branch:     OperationVersion ──▶ OperationVersion ──▶ HEAD
LayerStack:         immutable Layer ──▶ immutable Layer ──▶ HEAD
                    separate LayerStackMerge · backup · restore
```

The five layers separate concerns:

- Clients see ordinary filesystem behavior; they do not manage canonical
  objects directly.
- `OperationWorkspace` temporarily owns one operation's presentation,
  recovery state, resources, and cleanup.
- `layerfs-core` defines the same canonical filesystem for Direct, FUSE, and
  APFS presentations.
- `WorkingStore` records nearby operation and Branch history.
- Only explicit Push and Fetch cross the authenticated boundary to or from a
  physically distinct `DurableStore`.

### CAS + CDC + COW

```text
file bytes
    │
    ▼
FastCDC 8 / 16 / 32 KiB
    │
    ▼
authenticated chunk ObjectIds
    │
    ▼
persistent COW extent tree
    │
    ▼
immutable RootId
```

A localized edit creates a new root while preserving and reusing unchanged
content:

```text
Old root    A ── B ── C ── D
                         │ edit
New root    A ── B ── X ── D

Shared      A · B · D
New         X plus the changed COW spine
```

FastCDC finds content-defined boundaries, CAS reuses identical canonical
objects, and persistent COW trees rewrite only changed structure. Complete
input still has to be read, chunked, and hashed before exact deduplication is
known.

### Temporary OperationWorkspace

```text
BeginOperation
      │ pin exact Branch head and base RootId
      ▼
OperationWorkspace
      │ Direct · Linux FUSE · APFS
      │ private changes · recovery · bounded resources
      ▼
quiesce processes · writers · mappings · request Q
      │
      ▼
candidate RootTransition
      │
      ▼
OperationCommit
      ├── WorkingRecorded ──▶ acknowledge ──▶ identity-safe cleanup
      ├── Conflict ─────────▶ preserve candidate
      └── refusal/failure ──▶ preserve recoverable state and residue
```

The wrapper is temporary and operation-scoped. FUSE checkpoints can update
private candidate state, but only `OperationCommit` creates one accepted
`OperationVersion` and advances the exact expected Branch head.

### WorkingStore and DurableStore

```text
WorkingStore A                authenticated service          DurableStore
StorageId W-A  ── Push ───────────────────────────────────▶  StorageId D
               ◀─ Fetch ───────────────────────────────────
                                                                │
                                                                │ Fetch
                                                                ▼
                                                          WorkingStore B
                                                          StorageId W-B
```

Push starts from an accepted Working closure, negotiates `ObjectId`s, and
transfers only missing objects and accepted history in bounded, resumable
batches. DurableStore independently authenticates the transfer before
changing an exact durable head. Fetch reconstructs accepted history into a
different disk-backed WorkingStore; WorkingStores never become peer
authority.

Canonical identity never includes workspace paths, mount points, spools,
native files, processes, or SQLite pages.

### Branches and LayerStacks

| | Branch | LayerStack |
|---|---|---|
| Represents | Active, continuing work | Promoted immutable Layers |
| Head names | `OperationVersionId` + `RootId` | `LayerId` + `RootId` |
| Advanced by | Operation commit, child merge, rollback, or durable Branch Push | Separate LayerStack merge or rollback |
| Nesting | Recursive child Branches; immediate-parent merge only | Every descendant inherits one originating LayerStack |
| After merge | Source Branch remains active | Exact stack head advances or conflicts |

```text
Originating LayerStack
Layer ───────────────▶ Layer [HEAD]
                           │ inherited fork origin
                           ▼
Branch
OperationVersion ──▶ OperationVersion [HEAD]
                           │
                           ├──▶ continue work
                           ├──▶ child Branch
                           ├──▶ Push durable Branch
                           └──▶ separate LayerStackMerge
                                      │
                                      ▼
LayerStack
Layer ───────────────▶ Layer ───────────────▶ new Layer [HEAD]
```

Branch Push and LayerStack merge are independent: pushing work does not
promote it into the LayerStack, and promotion does not delete or close the
source Branch.

## Repository structure

The active product path is:

```text
layerfs/
├── Cargo.toml
├── crates/
│   ├── layerfs-core/             canonical filesystem model
│   ├── layerfs-storage/          SQLite, CAS, integrity, compaction
│   ├── layerfs-working-store/    local Branch and Operation authority
│   ├── layerfs-durable-store/    shared durable authority
│   ├── layerfs-sync/             bounded Push and Fetch protocol
│   ├── layerfs-service/          authenticated DurableStore service
│   ├── layerfs-workspace/        operation lifecycle and path custody
│   ├── layerfs-mount/            mounted model and Linux FUSE adapter
│   ├── layerfs-materialization/  physical projection and APFS adapter
│   └── layerfs-sdk/              public programmatic API
├── containers/
│   └── layerfs-fuse/             Linux FUSE image definition
├── tools/
│   └── layerfs-eval/             qualification and benchmark tooling
├── docs/
│   └── architecture/             architecture-focused documentation
├── poc-product-ready/            product-readiness contracts and evidence
├── SPEC.md                       canonical filesystem specification
└── architecture.md               detailed architecture reference
```

The active Rust workspace contains exactly the ten crates listed above. The
repository also retains research, historical PoC, evaluation, and
implementation-detail material outside the active product path.
