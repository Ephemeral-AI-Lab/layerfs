# Phase 4 rollback deletion record

Status: complete through WP3; WP4 and all later work are intentionally pending.
Date: 2026-08-17
Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
Branch: `codex/empty-worktree`

## Starting checkpoint

- HEAD: `e760a122d128dc242e9364483a7259b360dacf87`
- Subject: `phase4-backpaddling-rollback-fix-checkpoint-0`
- Starting worktree: clean (`git status --short` produced no paths).
- `git diff --check`: clean.
- No files in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` were touched.

## Intended deletion surface

The following rejected implementation paths are deleted by WP1 and WP2:

- `crates/layerfs-engine/src/append_only.rs`
- `crates/layerfs-engine/src/bin/phase4b_benchmark.rs`
- `crates/layerfs-engine/src/bin/phase4_fair_benchmark.rs`
- `PackedInMemoryCas`, `ChunkLocation`, and packed-only content entry points
  in `crates/layerfs-core/src/cas/mod.rs` and
  `crates/layerfs-core/src/content/mod.rs`

The removed Cargo dependency surface is:

- workspace `fs2` dependency in `Cargo.toml`;
- `layerfs-engine`'s `fs2` dependency; and
- the now-unused `fs2`, `winapi`, and `winapi-*` lockfile packages in
  `Cargo.lock`.

No deleted source is copied into this record.

## Starting SHA-256 fingerprints

These hashes were captured before editing:

| Path | SHA-256 |
|---|---|
| `crates/layerfs-engine/src/append_only.rs` | `fde82df85a66002fe8c8c6333ec1036cf3476bbab182985095126974147337d1` |
| `crates/layerfs-engine/src/bin/phase4b_benchmark.rs` | `bd94d1c4b24b324d9dc6cc4cddee5d2aef91b5c305bda3f1a5d5bfb3ee648b66` |
| `crates/layerfs-engine/src/bin/phase4_fair_benchmark.rs` | `50c7721d949a96421aecf6a8e1082fd7c277bdc79556bb97506cb1c7f6c6aca6` |
| `crates/layerfs-core/src/cas/mod.rs` | `ef6eaf5fb531a678e91e362a6e1f99192ebfb8e17e97a60329cf2a3254f8603b` |
| `crates/layerfs-core/src/content/mod.rs` | `bf7be4651e4dc716c9ddbf3b6e2421f4625d1434e5e3e998c12e3f9329c8a722` |

## Retained evidence

The Phase 2/4B specifications, acceptance ledger, JSONL benchmark report,
and finding document remain in place. The finding document now records the
later five-row same-source proxy facts, exact source fingerprints, workload
counts, and their explicit non-promotion status. The status notices in the
Phase 4B specification and ledger identify the candidate as rejected and
superseded for active implementation without changing the evidence below.

## Commands and outcomes

The initial pre-edit source search was scoped to `crates`, so it did not cover
the workspace member `tools/layerfs-eval`. A repository-wide review found its
active Phase 2 packed benchmark modes and callers after the core deletion;
those modes, dispatch entries, and the packed verifier were removed as the
final WP2 correction. The final workspace-wide search below is authoritative.

No Cargo command was run concurrently with another Cargo command.

Post-deletion absence searches:

```text
rg -n -S "append_only|AppendOnly|Phase4B|phase4b|carrier marker|store\.log" tools crates Cargo.toml Cargo.lock --glob '*.rs' --glob 'Cargo.toml' --glob 'Cargo.lock'
rg -n -S "\bfs2\b|FileExt" tools crates Cargo.toml Cargo.lock --glob '*.rs' --glob 'Cargo.toml' --glob 'Cargo.lock'
rg -n -S "PackedInMemoryCas|ChunkLocation|full_replace_packed|packed_cas|phase2-opt2|packed" tools crates Cargo.toml Cargo.lock --glob '*.rs' --glob 'Cargo.toml' --glob 'Cargo.lock'
```

All three returned no active-code hits. Remaining matches are in retained
historical specifications, ledgers, JSONL reports, the rollback instructions,
and this deletion record; they describe the rejected experiments and do not
compile or expose them.

Verification outcomes (all exit 0):

- `cargo metadata --offline --no-deps --format-version 1` — workspace resolved.
- `cargo test -p layerfs-core --offline` — 40 passed, 0 failed; 0 doc tests.
- `cargo test -p layerfs-engine --offline` — 4 passed, 0 failed.
- `cargo check -p layerfs-eval --offline --all-targets`.
- `cargo check --workspace --offline --all-targets --all-features`.
- `cargo test --workspace --offline` — core 40 passed, engine 4 passed, eval 5
  passed; all workspace doc-test suites passed with 0 tests.
- `cargo check -p layerfs-core --offline --all-targets`.
- `cargo check -p layerfs-engine --offline --all-targets`.
- `cargo check -p layerfs-core --offline --all-targets --all-features`.
- `cargo check -p layerfs-engine --offline --all-targets --all-features`.
- `cargo fmt --all -- --check`.
- `git diff --check`.

The first formatting check returned nonzero because deletion left one
indentation/blank-line mismatch in `cas/mod.rs`; it was corrected with
`apply_patch`. The repository-wide review also reproduced one nonzero
`cargo check -p layerfs-eval --offline --all-targets`, caused by stale packed
benchmark callers. Removing that benchmark surface was the final WP2
correction; the workspace check/test and final formatting/diff gates then
passed. No final check remained nonzero or unavailable.

The SQLite schema, profile, SQL, and data files were not changed. Canonical
bytes, object IDs, CDC outputs, and ordinary Phase 1/2/3 core paths were not
changed; the core test suite continued to pass its canonical/CDC/COW assertions.

The final worktree contains intentional WP0–WP3 edits only. `git diff --stat`
shows 15 tracked files changed, 42 insertions, and 6,402 deletions; the
untracked deletion record itself is visible in `git status --short` but is not
included in that tracked diff statistic. The active source fingerprint is:

- unchanged HEAD: `e760a122d128dc242e9364483a7259b360dacf87`;
- `Cargo.toml`: `dbcb7eeb7672bdd5e8bb8ece8d238879e867b6f7f343ddfed50e20f807760621`;
- `crates/layerfs-engine/Cargo.toml`:
  `d61019278eaeaeabf39d8393705a62484403a4c116105c41e75086b9cc3bff61`;
- `crates/layerfs-engine/src/lib.rs`:
  `9475d9d32d2e59cdf7b8a5f9cc3e35ecf3c58e47152fcfbf96c7a8b896eeaadb`;
- `crates/layerfs-core/src/cas/mod.rs`:
  `a066ec92f5f70204be39e73408943079d334d54e7f7cd54c370edcb2feeac76a`;
- `crates/layerfs-core/src/content/mod.rs`:
  `72eca561436bca8c20f0eddebfaf13697cc833586af8587c4a0c0c7d83b7256e`.
- `tools/layerfs-eval/src/main.rs`:
  `1fe48d986cc403012129338c7fa5f55e5e09a7f1ad9cc2a394693ab87975074c`.

## Final source fingerprint

The final active source hashes above and unchanged HEAD are the source
fingerprint. No commit was created.
