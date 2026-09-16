# layerfs-content (C1)

> **Status:** Implemented slice; the accepted profile is a checked supported range.

Canonical objects, complete-file construction and known-edit construction. C1 owns
canonical identity, envelope framing, the frozen GEAR content-defined chunker, the
extent-tree mapping, bounded logical reads and localized edits. It opens no
database, pack or file: construction emits finalized canonical objects to a
caller-supplied bounded consumer, an edit consumes an immutable base through a
caller-supplied authenticated provider, and reads ask the same provider for
canonical bytes. C1 never calls a delta codec and never imports SQLite.

## Accepted profile (this slice)

| Item | Accepted value | Notes |
| --- | --- | --- |
| Construction cutoff `T` | a power of two in `131072..=1048576`, exclusive | Default `131072`; 256 KiB and 1 MiB are supported and exercised end to end |
| Whole-file delta depth | `0..=50`, default `8` | `0` disables whole-file delta selection; a depth never widens a chain or memory budget |
| Chunk delta depth | `0..=50`, default `4` | Same rule for chunk payloads |
| Largest whole-file frame | `max(135168, raw + raw/128 + 1024)` | Derived from the accepted cutoff; the default cutoff keeps its frozen value |
| Advisory predecessors | at most `4` per object | Bounded hints for physical selection; never a logical dependency |
| Empty file | file state over a defined empty mapping page | No whole-file object is produced |
| `0 < length < T` | one `WHOLE_FILE` canonical object | Value layout `LFS5SML\0`, version `1`, raw payload |
| `length >= T` | `FILE_STATE` → extent tree → `CHUNK` objects | Frozen v3 mapping grammar |
| Chunk window | `8192..=32768` bytes | Frozen two-byte rolling GEAR profile |
| Mapping page | `<= 128` entries, `>= 64` for non-root pages, level `<= 31` | Canonical partition |
| Envelope ceiling, per canonical object | `16 MiB` | Role-independent guard applied before the role tag is known; not a file or chunk limit |
| Per-field ceiling | `8 MiB` | The frozen format bounds one field separately; a bytes-role value is one field |
| Largest object any role produces | `131,094 B` whole-file, `32,789 B` chunk, `8,192 B` mapping page | The envelope ceiling never binds on a produced object |
| Maximum file size | none declared | Logical length is `u64` with checked arithmetic; only cost bounds it in practice |

Any other value fails with `ContentError::UnsupportedPolicy` before work begins.
Nothing is silently clamped, and an opened Store's recorded policy is never
reinterpreted.

## Public surface

```text
object::ObjectId          fixed 32-byte identity over frozen domain framing
object::codec             canonical envelope encode/decode with checked lengths
object::AuthenticatedObjects  narrow provider: read_canonical_batch, read_canonical
object::FinalizedObject   identity + role + owned canonical bytes + direct references
object::FinalizedConsumer bounded sink; DiscardingConsumer is the non-persisting one
file::construct_bytes     known-length complete-file construction
file::construct_stream    unknown-length construction with a bounded cutoff probe
file::read_all(_bounded)  logical read of a whole file
file::read_range          logical ranged read across extents and pages
file::FileView            one authenticated base, opened once per operation
file::EditStream/Edit/EditSource   validated ordered edits and replacement bytes
file::apply_edits         known-edit construction over an immutable base
policy::ConstructionPolicy / ConstructionCapacities
```

Every entry point takes a `layerfs_telemetry::timer::TimingScope`; recording
enabled or disabled changes no product work and no result.

## Scope limits

- Filesystem trees, attributes and metadata ropes are not implemented here.
- Known-edit construction is implemented for ordered edit streams. It re-derives
  the mapping from the retained extent sequence: unchanged chunk payloads are
  retained and referenced, but unchanged mapping *pages* are re-encoded rather
  than reused from the stored tree, so a large-to-large edit does not reproduce a
  reference (v0.1.6) root in general.
- An edit may not reach back into bytes an earlier edit in the same stream
  introduced; that range is rejected with `InvalidEdit` rather than reordered.
- Unsupported older physical representations are explicit scope limits, not
  silently handled fallbacks.
- The `AuthenticatedObjects` contract requires the provider to authenticate the
  bytes it returns; C1 revalidates framing and structure but does not re-hash an
  unchanged owned allocation.
- A range read navigates the tree with one-ID batches and batches only payload
  acquisition; grouped node acquisition is a later change, not a silent default.

## Checks

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --tests
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

External targets: `object_identity`, `file_complete`, `file_read`, `streaming`,
`timing`. The frozen identities in `object_identity.rs` were computed
independently from the profile formulas, so a framing or hashing change fails.
