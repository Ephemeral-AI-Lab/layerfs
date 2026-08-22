# G3-v1 pre-source custody freeze

Status: **prospective / captured before any G3 product-source edit**

- Captured: 2026-08-22 Asia/Shanghai
- Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
- Branch: `codex/empty-worktree`
- HEAD: `d79f0e0e2582d1bc491410224fec2b6cef7482e9`
- Tracked diff SHA-256: `40d4b4251a10504cb21a0825ed93b867fb56d3dbd6287acf67420f632a9e39a4`
- Porcelain-status SHA-256: `5e3b8157750d2a2b625b72f73d5acbd1db5cc95ec12bae81ec749a6f38152889`
- Pre-existing G3 execution handoff SHA-256: `51039bd1f5a34899a976a2baa5ea3da66593194984d873f88932de839d86baed`
- Current roadmap SHA-256 before G3: recorded separately at terminal so the user-owned tracked diff is not conflated with G3 evidence.

The tracked diff affected only
`implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md`. The untracked
set consisted only of the declared append-only G2 methodology/closure files and
the pre-existing G3 execution handoff; their exact hashes are in
`PREEXISTING-UNTRACKED-SHA256-v1.tsv`.

Relevant product-source hashes before G3:

| File | SHA-256 |
|---|---|
| `crates/layerfs-engine/Cargo.toml` | `f2f17cf5d302dfeaab12c4b1d0b6af660c229cd737c773f3a5d417dcb2eb1242` |
| `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` | `157699e0cd4cb1e3b5ec631cefb7c967ff7433bdeeb10ee1336e70961b402ad2` |
| `crates/layerfs-engine/src/lib.rs` | `9475d9d32d2e59cdf7b8a5f9cc3e35ecf3c58e47152fcfbf96c7a8b896eeaadb` |
| `crates/layerfs-core/src/canonical_v2.rs` | `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` |
| `crates/layerfs-core/src/validation.rs` | `f42eb13125cc19ecfc3e4567d35926b2871cd65b46d9f0af985c5a1782f02a5e` |

Sealed G2-v5 control hashes reverified before G3:

| Artifact | SHA-256 |
|---|---|
| payload manifest | `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` |
| terminal | `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` |
| terminal verification | `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` |
| raw JSONL | `c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb` |
| primary analysis | `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803` |
| independent recomputation | `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e` |
| cleanup verification | `38a588d3fb5cfe0bfa7968c83731705c78539f51724e3f685deaf011b2a03e46` |
| base-proxy custody | `ebfdb9a5f9a2f3fcb5763a6caf36424b6ced59e019bf9e30f922e9d38a28d08f` |

No G0/G1/G2 artifact was modified, chmodded, relabeled, or rerun.
