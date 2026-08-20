# LayerFS Phase 1 Implementation Specification

This document is the active implementation contract for Phase 1. It applies
with `SPEC.md`, `architecture.md`, `IMPLEMENTATION_PLAN.md`, and `evaluation.md`.

## 1. Objective and boundary

Phase 1 establishes the smallest logical core:

```text
canonical names and paths
        ↓
logical object
        ↓
fixed canonical bytes
        ↓
typed BLAKE3 ObjectId
```

The core is independent of SQLite, storage engines, native paths, operating
system APIs, VFS projection, SDK APIs, CDC, CAS publication, copy-on-write,
delta handling, and materialization. Those systems must not be added to this
phase.

Phase 1 includes:

- bounded canonical paths and immediate directory names;
- deterministic byte ordering and typed object references;
- the fixed object envelope and payload grammar below;
- bounded, checked, streaming encode/decode entry points;
- one typed `ObjectId` backed by a 32-byte BLAKE3 digest;
- direct authentication of supplied canonical bytes;
- exact end-of-input checks; and
- direct tests and golden vectors for valid, invalid, malformed, and boundary
  cases.

Phase 1 does not choose the final large-file content layout. The production
target is:

```text
File → bounded immutable content tree → Chunk IDs → CAS
```

The next phase benchmarks flat, segmented, and fixed-fanout tree layouts before
selecting the content layout. No flat large-file manifest is frozen here.

The Phase 2 decision is deliberately semantic before it is physical:

- a `File` identifies the logical content root;
- a `ContentLeaf` may hold a bounded ordered list of chunk references and
  lengths; and
- a `ContentBranch` may hold bounded child references and subtree byte lengths.

Those names describe the candidate content graph, not Phase 1 object kinds or
stored encodings. The benchmark must establish that the selected shape keeps a
small edit and a bounded range read local as file size grows before those
objects are added to the canonical format.

## 2. Canonical names and paths

Canonical text is UTF-8 bytes with no host-dependent normalization. A
`CanonicalName` is exactly one non-empty directory component. It rejects `/`,
`\\`, NUL, `.` and `..`. A name is at most 255 bytes.

`CanonicalPath` is either the empty root path or non-empty components separated
by `/`. It rejects empty components, absolute paths, traversal components,
backslashes, and NUL. A path is at most 4 KiB and 256 components. Invalid UTF-8
is rejected. These limits are checked before allocation or iteration derived
from encoded input.

Path ordering compares components as unsigned bytes, with a parent before its
descendants. Directory entries use immediate `CanonicalName` values and sort
strictly by their name bytes; duplicate names are rejected.

## 3. Object model

The only Phase 1 object kinds are:

| Kind | Tag | Payload |
|---|---:|---|
| bytes | `0x01` | `u32` byte length, then bytes |
| directory | `0x02` | `u32` child count, then child entries |

A directory entry is:

```text
u32 name length
name bytes
u8 child kind
32 raw ObjectId bytes
```

The name is an immediate `CanonicalName`, and the child kind is `Bytes` or
`Directory`. There is no separate stored root object or second root identity;
an `ObjectId` for a directory object is sufficient wherever a tree root is
needed later.

The model has no metadata, flags, reserved values, registries, arbitrary maps,
storage traits, backend identifiers, or host filesystem values.

## 4. Fixed canonical envelope

Every object has this exact 9-byte header:

| Offset | Size | Meaning |
|---:|---:|---|
| `0..4` | 4 | ASCII marker `LFSO` |
| `4` | 1 | object kind tag |
| `5..9` | 4 | payload length as big-endian `u32` |

There are no flags, reserved fields, compatibility fields, or alternate
headers. The payload length excludes the 9-byte header. The complete object is
at most 16 MiB, and its payload is checked against that bound before decoding.
All integer arithmetic and conversions are checked.

The encoder emits exactly one representation. The decoder accepts only this
grammar, fails closed on unsupported marker or kind bytes, and requires the
declared payload to end at the end of the input. Short input returns
`UnexpectedEof`; extra input returns `TrailingBytes`.

The streaming APIs operate on standard-library `Read` and `Write` values. The
decoder bounds the reader to the declared payload, checks every field before
allocation, limits byte fields to 8 MiB, and limits directory children to
100,000. It then probes the underlying reader for exact EOF.

## 5. Identity

`ObjectId` is the only Phase 1 identity type. It contains exactly 32 raw bytes
and has deterministic byte and lowercase hexadecimal text forms.

For canonical object bytes `B`:

```text
ObjectId = BLAKE3("layerfs/object\\0" || B)
```

The domain bytes are fixed and explicit. There is no second identity domain.
Object identity excludes database values, storage locations, native paths,
filesystem behavior, timestamps, temporary names, thread scheduling, and input
fragmentation. Streaming hashing of the same supplied bytes produces the same
digest as contiguous hashing.

Identity validation hashes the supplied canonical byte sequence directly and
compares that digest with the supplied `ObjectId`. It then decodes the same
bytes for grammar validation. It does not decode, re-encode, and hash a
reconstructed value.

## 6. Golden vectors

The following vectors are fixed for this phase. Hex strings contain the full
object envelope followed by its payload.

| Logical object | Canonical bytes | ObjectId |
|---|---|---|
| `Bytes("hello")` | `4c46534f01000000090000000568656c6c6f` | `a246e43d678984a154487ee08e96f5677f0100cf59041d6708103a517e383a49` |
| empty directory | `4c46534f020000000400000000` | `c705a66295b38b1e1dabe72fec9c4793bde8e3bea68af1ea775a51d1cc56547a` |

## 7. Required tests

The core tests must cover:

- valid root paths, nested paths, and immediate names;
- empty components, traversal, absolute paths, separators, NUL, invalid
  UTF-8, maximum lengths, and maximum component counts;
- deterministic path and directory-name ordering;
- typed object references and both object kinds;
- deterministic canonical bytes and the golden vectors above;
- slice and streaming round trips;
- truncated headers and payloads, declared lengths that are too short or too
  long, oversized fields, unknown marker/kind bytes, non-canonical ordering,
  and trailing data;
- fixed-size identity byte/text conversion;
- contiguous and streaming digest stability; and
- identity mismatch and direct supplied-byte authentication.

Production code must retain the no-unsafe-code policy and must not use
`unwrap` or `expect`. Test-only assertions may use them for concise fixtures.

## 8. Performance baseline and closure

Phase 1 is not closed from implementation tests alone. Before closure, run the
canonical-object baseline defined in [`evaluation.md`](evaluation.md):

```text
cargo build --release -p layerfs-eval
/usr/bin/time -l -o eval/phase1-<commit>/time.txt \
  target/release/layerfs-eval phase1 eval/phase1-<commit>
```

The baseline exercises the public `Read`/`Write`, encode, decode, path, and
identity entry points for 1 KiB, 1 MiB, and 8 MiB byte objects; 16, 256, and
4,096-child directories; and short and near-maximum paths. It uses one warm-up
and five measured iterations per case and checks every result for exact
correctness.

The retained closure artifact must include the environment, raw per-case
timings, summary, and external maximum-resident-size output. An unavailable
RSS observation is valid only when it is explicitly labeled unavailable; zero
is not a substitute.

This is a bounded canonical-core baseline. It does not claim large-file
locality, CDC/CAS small-edit scaling, SQLite throughput, materialization
performance, or process-wide high-concurrency memory bounds. Those are Phase 2
and later acceptance gates.
