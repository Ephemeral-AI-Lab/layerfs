# tdx2 — the six owner cases on the corrected build profile

> **Status:** Research; informative and not a product contract. Single sample per
> case per run; repeats are labelled and retained.

Profile: the workspace now supplies the transport build flags itself
(`core/.cargo/config.toml`, aarch64 targets), so a plain `cargo build` negotiates
AES-GCM with the ARMv8 backend. `--cpus=1 --memory=128m --memory-swap=128m
--pids-limit=16 --read-only` containers, 1 GiB per stream, owner's probe example.

## The trap this run exposed

```text
 AES-GCM at 16 KiB messages, measured INSIDE the --cpus=1 container
   --cfg aes_armv8 --cfg polyval_armv8                      0.54 GB/s   (30.4 µs)
   the same cfgs + -C target-feature=+aes,+sha2             1.39 GB/s   (11.8 µs)  ← 2.6×
 ChaCha20-Poly1305, same container
   --cfg chacha20_force_neon                                0.68 GB/s
   without it (soft)                                        0.31 GB/s
```

The cfg knobs alone select the backend source but not the codegen: on the
`aarch64-unknown-linux-musl` target the AES/PMULL intrinsics stay behind
non-inlinable `#[target_feature]` boundaries and the AEAD runs at 0.54 GB/s. The
macOS host target enables `+aes` by default, which **hid this difference** in the
host microbenchmark — the container number is the one that decides. The profile
therefore carries `-C target-feature=+aes,+sha2`, and that is a declared hardware
requirement (a CPU without the ARMv8 crypto extension is refused at the handshake
instead of silently falling back to the measured 0.111 GB/s soft path).

## Results

| case | owner baseline | sample 1 | repeats | change |
| --- | --- | --- | --- | --- |
| raw TCP control, upload, 1 | 1.882 GB/s | 2.223 | 2.259 | control; the machine was ~19 % faster than at baseline |
| raw TCP control, download, 1 | 1.001 | 1.196 | — | control |
| **authenticated upload, 1** | **0.2598** | **0.862** (19.02 µs) | **1.021** (16.05 µs) | **+232 % … +293 %** |
| **authenticated download, 1** | **0.2329** | **0.782** (20.95 µs) | — | **+236 %** |
| **authenticated upload, 2** | **0.5332** | **2.026** (8.09 µs) | 1.418 (11.56 µs) | +166 % … +280 % |
| **authenticated download, 2** | **0.6142** | **1.534** (10.68 µs) | — | **+150 %** |

Honest reading of the aggregate target: the two-stream rows now sit at
**1.42–2.03 GB/s against 2.0 GB/s** — one sample meets it, two do not — so the
transport is *at the target's boundary* rather than 4× below it. Machine state
moved the control by ~19 %, and the spread between samples of one configuration is
larger than that, so a campaign (not single samples) is required before claiming
the target met. The spec's rules require a registered campaign for that claim;
none is claimed here.

## Ladder across the whole slice

```text
 1 stream authenticated upload      2 stream authenticated upload
 baseline       0.2598              baseline       0.5332
 +F5+batching   0.299               +F5+batching   0.595
 +F4            0.427               +F4            0.704
 +F6 (cfg only) 0.493               +F6 (cfg only) 0.962
 +F6 (correct)  0.862–1.021         +F6 (correct)  1.418–2.026
```

## Harness limitation (retained, not hidden)

The two-stream cases need the container runtime warm: the probe's server allows
5 s per stream for a worker to become ready, and two cold `docker run` starts on
Docker Desktop intermittently exceed it (`AssertionError('endpoint readiness')`).
Those failed attempts are retained; the successful samples above were taken after
warming the runtime with two throwaway container starts.

## Verification on this profile

```text
cargo test --manifest-path core/Cargo.toml --locked          92 test binaries ok
cargo fmt --all --check                                      clean
cargo clippy -p layerfs-bridge --all-targets -D warnings     clean
python3 core/tools/check_product_boundary.py                 PASS (174 production files)
```

## Identities

```text
musl probe     (see tdx2-final-matrix.txt run header) rebuilt from the source hashes below
image          layerfs-tdx2-probe, scratch, arm64, rebuilt for this run
rustc          1.85.1 (4eb161250 2025-03-15) · --locked --release
profile        core/.cargo/config.toml: --cfg aes_armv8 --cfg polyval_armv8
               --cfg chacha20_force_neon -C target-feature=+aes,+sha2 (aarch64)
containers     --cpus=1 --memory=128m --memory-swap=128m --pids-limit=16 --read-only --log-driver=none
```
