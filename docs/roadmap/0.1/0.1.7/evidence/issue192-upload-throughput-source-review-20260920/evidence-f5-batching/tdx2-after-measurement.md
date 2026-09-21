# tdx2 — the owner's upload cases after the bridge transport slice

> **Status:** Research; informative and not a product contract. One sample per case.

Date: 2026-09-20. Same cases, carrier, 1 GiB payload recipe, container limits and
probe example as the owner's `tdx1-*` receipts; only the source changed. One fresh
container per stream, one sample per case, no repeat or best-of. Driver: a reduced
re-implementation of the owner's unmodified `transport_diagnostic.py`.

## Result — the full ladder

| stage | 1 stream | 2 streams |
| --- | --- | --- |
| owner baseline (`tdx1-*`) | 0.2598 GB/s · 63.05 µs | 0.5332 GB/s · 30.73 µs |
| + F5 relay cap, one-read header, 256 KiB record batching | 0.299 · 54.81 | 0.595 · 27.52 |
| + F4 buffer reuse (contiguous batch, reused scratch) | 0.427 · 38.36 | 0.704 · 23.28 |
| **+ F6 AEAD suite (AES-GCM with the ARMv8 backend)** | **0.684 · 23.97** | **1.612 · 10.17** |
| **total change** | **+163 %** | **+202 %** |
| raw TCP control, same container (1 stream) | 1.807–1.983 · 8.26–9.07 µs | — |

```text
 1 stream, per 16 KiB frame                       2 streams, per frame (aggregate)
 owner  ███████████████████████████████████ 63.05   owner  ███████████████████████████████ 30.73
 +F5    ███████████████████████████████ 54.81       +F5    ███████████████████████████ 27.52
 +F4    █████████████████████ 38.36                 +F4    ███████████████████████ 23.28
 +F6    █████████████ 23.97  ← 0.684 GB/s           +F6    ██████████ 10.17  ← 1.612 GB/s
 control███████ 8.26                                control███████ ~8.3
 ⇒ at two streams the authenticated path is within ~23 % of the plaintext control's
   per-frame cost; the AEAD overhead is ~2 µs/frame aggregate.
```

Single-stream remains latency-bound (23.97 µs/frame against 10.17 µs/frame at two
streams on two containers), so further single-stream gains need send/receive
overlap rather than less CPU. Two streams is the target-relevant configuration and
now sits at 1.612 GB/s against the 2 GB/s aggregate.

## F6: what changed and why it is safe

- Suite: `Noise_KK_25519_AESGCM_SHA256`, selected by
  `connection::NOISE` **only** where the backend is accelerated — x86_64 (runtime
  AES-NI/CLMUL detection) or aarch64 built with `--cfg aes_armv8
  --cfg polyval_armv8 -C target-feature=+aes,+sha2`. An aarch64 build without
  those flags keeps ChaCha20-Poly1305 (0.52 GB/s) rather than dropping to the
  measured 0.111 GB/s soft-AES path. No build can select the soft path, and two
  builds that chose differently reject each other's handshake explicitly.
- No dependency change: `aes-gcm 0.10.3`, `ghash 0.5.1`, `polyval 0.6.2`, `sha2
  0.10.9` are already in `core/Cargo.lock` through `snow 0.9.6`.
- `tests/aead_profile.rs` asserts the negotiated suite matches the compiled
  backend in both configurations.

**Profile note for the owner:** the tdx1 specification froze "does not … change the
Noise suite". This change revises that line, and the receipt identities (image,
binaries, source) are new. Its build recipe now needs the three flags.

## Verification (both configurations)

```text
cargo test --manifest-path core/Cargo.toml --locked                          92 binaries ok (ChaChaPoly arm)
RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 -C target-feature=+aes,+sha2" \
  cargo test --manifest-path core/Cargo.toml --locked                        92 binaries ok (AESGCM arm)
cargo fmt --all --check                                                      clean
cargo clippy -p layerfs-bridge --all-targets -- -D warnings                  clean in both configurations
python3 core/tools/check_product_boundary.py                                 PASS (174 production files)
```

## Identities (final)

```text
musl probe     6097677c665b7955ca331c7db886b056a00e924b3ead14eb224481d5ba4e531f
image          sha256:31f59f640af32dd925460ab6669af29bdc90de44afd8172f080792b9bc0edfca (scratch, arm64)
rustc          1.85.1 (4eb161250 2025-03-15) · --locked --release
               RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 -C target-feature=+aes,+sha2"
connection.rs  eeeefd806252389c1fae0423d183d66df5e9691e4b7c8402bc69e1cb231691d7
pipe.rs        1d2f5b23cd493b485df1a7ac9368e223662115d6f93fe84b312f3f41f5cb482b
frame.rs       208e4bf255a1708ae9316311efae1b3d599958bbc84d2614d1b1cfb4e7208c41
Cargo.toml     1d2c5b62ea5288a464fcc9fcc0aeb6f272cdbf2940c6ffb0d887a78804144c4f
containers     --cpus=1 --memory=128m --memory-swap=128m --pids-limit=16 --read-only --log-driver=none
```

## Production LOC (core scope, the three changed files)

`481 → 553` (**delta +72**) via `tools/production_loc.py`; tests, manifest and docs
excluded from the count.

## Not measured

The daemon relay with the real daemon in the loop (the relay rate above is the
bridge-level product path on the host). F6's effect on the *download* direction is
`NOT_RUN`; receive-side decryption sits on the critical path there, so its gain
should be similar but is unmeasured.
