# Issue #236: host-direct SDK Init functional proof

Source basis: `main` at `7df25f9790996cf83232782c7b35f7c26fcc3252`,
with the staged `core/crates` tree
`c3de9def7a365ca038cd0385be7759068596b090` at the proof run. The only
subsequent change under that tree was a test-only hex formatter correction for
Clippy; product source and the verified bytes did not change. This is a
functional proof, not a numeric performance PASS or a replacement for the
historical `daemon-host` receipts.

The source generator reused the declared `init_namespace` case shapes with a
new fixture profile `core-sdk-init-fixture-v1`, route `host-direct-sdk-v1`,
and seed `1`. Both fixtures were freshly prepared in this worktree under
`core/target/issue236-proof/59278-1790135592685236000/`; `reused=False` was
checked. The SDK called the host Service directly, which bound the checked
source to one authorized native import. No daemon or FUSE path was involved.

| Case | Sealed manifest SHA-256 | Files | Bytes | Result |
| --- | --- | ---: | ---: | --- |
| [100](namespace-100-compact-v3/proof.txt) | `4988076670f9583043de635b91f86b1468c0f7b61438f66645f925668469b108` | 100 | 5,000,000 | PASS |
| [1,000](namespace-1000-compact-v3/proof.txt) | `a4a31730c4af4a5673e85e7c0841bf7e0f0cea9151197fcc9d101b023dd06271` | 1,000 | 20,000,000 | PASS |

Each proof dropped the writer, reopened the Store and history, checked the
LayerStack's genesis Layer and root, then used public Service `Attributes`,
`List` and `ReadFile` operations to verify every manifest path, portable mode
and nanosecond mtime, logical byte count and file SHA-256. Directory listings
were compared with the manifest to reject extra or missing children. The
manifests and generator identity records are retained beside the receipts.

The focused test also refused nonexistent, regular-file and source-symlink
paths, refused a duplicate project name, and imported two different paths
concurrently without cross-import. Failures remained typed through the SDK.
`mount_workspace` reported `Unsupported`; the other Workspace/exec methods
are the same explicit skeleton. MCP and CLI remain README-only placeholders.

Checks on this worktree:

- `LAYERFS_PROOF_TREE=c3de9def7a365ca038cd0385be7759068596b090 cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-sdk --test init_project -- --nocapture` — 2 passed.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace --exclude layerfs-sdk` — passed; the SDK integration target was run separately above to avoid repeating it.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings` — passed after the test-only hex formatter correction. The earlier Clippy failure was retained in the task transcript.
- `cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --workspace --examples` — passed.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` — passed.
- `python3 core/tools/check_product_boundary.py` — passed, 266 production inputs scanned.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — 7 passed.
- `python3 -m unittest discover -s tools -p 'test_production_loc.py'` — 18 passed.

The SDK route has no qualified cache state, performance sample, daemon
delivery measurement, or four-tier Init performance gate. Those remain open
under #231/#237; no historical receipt was changed or promoted.
