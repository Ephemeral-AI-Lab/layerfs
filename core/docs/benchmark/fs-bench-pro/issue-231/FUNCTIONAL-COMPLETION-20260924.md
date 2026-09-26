# #231 functional completion ruling, 2026-09-24

After reviewing the [release-only, lite-verifier four-tier result](SDK-VERIFIER-LITE-RESULTS-20260924.md), the owner revised #231's completion criterion to **functional completion**. The required evidence is one public SDK native-directory Init per 100, 1,000, 10,000, and 100,000-file case using locked release binaries, followed by separate lite verification, cleanup, and retained receipts. All four cases meet that criterion. This ruling permits merging the treatment and closing #231.

| Files | Public SDK Init | Lite verifier | Complete paths | Files fully read and hashed | Functional result |
| ---: | ---: | ---: | ---: | ---: | --- |
| 100 | 0.030653500 s | 0.029412625 s | 102 | 53 | PASS |
| 1,000 | 0.105125042 s | 0.051693583 s | 1,011 | 70 | PASS |
| 10,000 | 1.226295125 s | 0.626640167 s | 10,101 | 72 | PASS |
| 100,000 | 5.588053292 s | 2.849658958 s | 101,001 | 73 | PASS |

The verifier authenticated the persisted root, inventoried every path and inode kind, checked every directory's portable metadata, and checked full bytes, size, SHA-256, and metadata for its deterministic file sample. The 100,000-file sample read 200,286,236 bytes, including both 100-MB anchors. Every verifier finished below its frozen 9.5-second limit. The [curated receipts and hashes](evidence/sdk-lite-four-tier-20260924/) pin source `92608113f23cd89a7b8dc94b54f65693456d60b9`, release binary and harness identities, fixture manifests, separate verification, cleanup, and evidence custody. Earlier failing and full-content receipts remain unchanged.

This ruling does **not** promote any performance receipt: all four rows remain `source-cache-uncontrolled-v1`, `admission_eligible=false`, and performance `INELIGIBLE`. No numeric SDK admission target was frozen. The historical 100,000-file 2.7-second cold target is not met or proved by the 5.588-second observation. The sample is not a full-content readback of every file. The broader #230 performance migration gates, including cold-cache qualification and the other pilots, remain open; #231's functional closure alone does not clear them. Continue speed/cache qualification under #230/#237 with a prospective contract and fresh samples.

Final source checks passed: `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`, `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --all-targets --locked -- -D warnings`, `cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --examples --release`, `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`, `python3 core/tools/check_product_boundary.py`, seven Core tool tests, nine benchmark harness tests, and `git diff --check`. No performance arm was repeated for this ruling.
