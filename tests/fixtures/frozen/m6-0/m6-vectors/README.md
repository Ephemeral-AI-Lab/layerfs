# M6.0 documentation-only vector generator

> **DOCUMENTATION-ONLY VERIFICATION TOOLING — NOT PRODUCT IMPLEMENTATION.**

This package mechanically regenerates and checks the frozen structural
identity, `ChangeReceiptV1`, direct-to-pack, path-grammar, lifecycle-lease,
recovery, and journal-retirement vectors. It is documentation evidence only; it
is outside the product checkout, imports no product code, does not prescribe a
product module/file structure, and grants no implementation authority.

Run from this directory:

```sh
CARGO_TARGET_DIR="$(mktemp -d)" cargo run --locked -- --check
```

`--check` renders the generated block in memory, compares it byte-for-byte
with the delimited block in `../M6_0_GOLDEN_VECTORS.md`, executes all valid,
boundary, and hostile assertions, and writes no vector output. Run without
`--check` to print the regenerated block to standard output for review. After
a documentation-contract change, `--write` mechanically replaces only that
delimited documentation block; it does not write the product checkout:

```sh
CARGO_TARGET_DIR="$(mktemp -d)" cargo run --locked -- --write
```
The reference parser is a byte/semantic oracle for documentation evidence; it
is not a product decoder implementation and does not prescribe product module
structure. It does execute the contract's pre-effect resource state model:
the exact combined receipt reservation must succeed before decoder-owned
collections or authentication, and unavailable/deadline/cancellation paths
return the sealed typed refusals.

The physical-pack oracle constructs and authenticates complete small packs,
keeps physical records in discovery order while sorting only fixed 80-byte
metadata, then proves an exact record/index bijection with bounded offset-order
processing. The state-model vectors additionally seal exact journal and
quarantine path/collision laws, prohibit I/O under the lifecycle gate, and
exercise old/new/unknown recovery plus crash-safe journal retirement. It does
not stage payload, import product code, prescribe product module layout, or
authorize product implementation.

The structural oracle keeps implicit-root and explicit-directory registries
separate, permits only explicit-directory IDs in directory child edges, and
permits only an implicit-root ID in `VersionIdV1`. It also binds every
`LogicalFileIdV1` chunk length to the payload length parsed from the referenced
`LogicalChunkIdV1` record. Normal occupied-ID comparisons recompute BLAKE3-256
over both canonical byte strings inside the oracle. The synthetic
`ForcedSameId` branch is reached only after both objects parse canonically and
their real digests are computed; it demonstrates the required collision
handling for a computationally infeasible same-ID/different-byte input and is
not a product hashing shortcut.

Receipt metadata authority is independent of receipt bytes: the validation
context holds the immutable source entry kind and portable mode, the oracle
recomputes the exact `CHGMETA` digest from that source fact, and only then
compares it with the receipt. A receipt-supplied digest therefore cannot mint
metadata authority, including the structural root sentinel.

The dependency is deliberately identical to the future product pin:

```toml
blake3 = { version = "=1.8.5", default-features = false }
```
