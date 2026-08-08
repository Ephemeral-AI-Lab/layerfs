# Frozen fixture custody

These files are read-only copies of the completed Phase 1 M6.1.2/M6.0
authorities. They are test fixtures, not an implementation source of truth and
are not compiled into the LayerFS runtime crates.

The M6.0 receipt identifies these three artifacts as frozen:

- `m6-0/M6_0_GOLDEN_VECTORS.md`
- `m6-0/m6-vectors/main.rs` (documentation-only generator)
- `m6-0/m6-vectors/README.md`

The M6.0 change receipt and M6.1.2 verification receipt/spec are retained as
provenance alongside those vectors. Their exact source paths and SHA-256
values are recorded in `SHA256SUMS`; verification is intentionally explicit
and does not add a hashing crate to the runtime dependency graph.

Source authority roots:

- `/Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-docs/ephemeral-sandbox-v2`
- `/Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-v2-worktrees/ephemeral-sandbox-v2`

Exact source-to-fixture mapping:

| Fixture | Read-only source authority |
|---|---|
| `m6-0/M6_0_CHANGE_RECEIPT_V1.md` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/M6_0_CHANGE_RECEIPT_V1.md` |
| `m6-0/M6_0_GOLDEN_VECTORS.md` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/M6_0_GOLDEN_VECTORS.md` |
| `m6-0/m6-vectors/README.md` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/m6-vectors/README.md` |
| `m6-0/m6-vectors/main.rs` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/m6-vectors/src/main.rs` |
| `m6-1-2/M6_1_2_VERIFICATION_RECEIPT.md` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/M6_1_2_VERIFICATION_RECEIPT.md` |
| `m6-1-2/M6_1_SPEC.md` | `ephemeral-sandbox-docs/ephemeral-sandbox-v2/phases/01-choose-design/M6_1_SPEC.md` |

The codec, error, and path implementation files are byte-preserved imports of
the checked M6.1.2 product sources, recorded separately by the implementation
repository’s Git diff and rechecked against the read-only product worktree.

Do not edit these files in place. If an upstream frozen artifact changes, copy
it again deliberately and update the custody manifest after independently
rechecking the source hash.
