# E1 — AEAD and framing-free crypto rates for the pinned Noise stack

> **Status:** Research; informative and not a product contract.

Date: 2026-09-20. Companion to the [parent review](../README.md). This directory
holds the harness, its resolved dependency lock, and four raw transcripts. It
answers the question the tdx1 receipts cannot: **is the authenticated upload
crypto-bound or framing-bound, and what is the per-core crypto ceiling?**

**It is a component microbenchmark, not product acceptance.** No socket, pipe,
container, Store or daemon is involved, so it does not qualify the bridge path;
it sizes one term of it.

## 1. Custody and conditions

```text
host              Apple M3 Max, 14 CPUs, macOS (aarch64)
rustc             1.85.1 (4eb161250 2025-03-15) — the pin the #192 image uses
crate versions    snow 0.9.6 · chacha20poly1305 0.10.1 · chacha20 0.9.1 · poly1305 0.8.0
                  aes-gcm 0.10.3 · ghash 0.5.1 · polyval 0.6.2 · aes 0.8.4
                  (identical to core/Cargo.lock; see Cargo.lock in this directory)
harness           bench.rs (Noise suites via snow) · decompose.rs (raw crates)
command           RUSTFLAGS="<cfg set>" cargo run --release --offline [--bin decompose]
payload           256 MiB per configuration, 4 KiB / 16 KiB / 60 KiB messages
cache contract    in-memory generated buffers, reused; no storage read or write,
                  so no cold-cache contract applies; warm component measurement
samples           one per configuration; configs A and D were repeated and agreed
                  within ~2% (compare run-A/run-D transcripts with the first runs)
```

`60 KiB` is used rather than 64 KiB because **snow caps one Noise message at
65,519 bytes** (`MAXMSGLEN`; a 256 KiB `write_message` returned `Error::Input`).
That cap is itself a design constraint, see §4.

The three non-obvious build knobs, all crate-documented and none of them a patch:

| knob | needed for | effect here |
| --- | --- | --- |
| `--cfg chacha20_force_neon` | ChaCha20 NEON on aarch64 (`chacha20 0.9.1` gates on this cfg **and** `target_feature="neon"`) | already used by the #192 image |
| `--cfg aes_armv8` (+ `-C target-feature=+aes`) | `aes 0.8.4` ARMv8 AES backend | AES-GCM 0.113 → 0.50 GB/s |
| `--cfg polyval_armv8` | `polyval 0.6.2` PMULL GHASH backend (used by `ghash`/`aes-gcm`) | AES-GCM 0.50 → 1.18 GB/s |

## 2. Measured results

Container-equivalent build (run-A / decompose-A: `neon` only, no `aes`/`sha2`),
i.e. what today's musl image actually compiles:

| component | 16 KiB msg | 60 KiB msg |
| --- | --- | --- |
| Noise ChaChaPoly (snow) | 0.521 / 0.514 GB/s | 0.520 / 0.516 |
| Noise AESGCM (snow) | 0.111 / 0.112 | 0.111 / 0.112 |
| chacha20 keystream | 1.008 | 1.029 |
| poly1305 (soft) | 1.045 | 1.044 |
| chacha20poly1305 AEAD | 0.507 | 0.521 |
| aes256gcm AEAD (soft AES) | 0.113 | 0.114 |
| aes128 ECB blocks (soft) | 0.195 | 0.194 |

Full-hardware build (run-D / decompose-D: `+aes,+sha2 --cfg aes_armv8
--cfg polyval_armv8`):

| component | 16 KiB msg | 60 KiB msg |
| --- | --- | --- |
| Noise ChaChaPoly (snow) | 0.518 / 0.514 | 0.523 / 0.515 |
| Noise AESGCM (snow) | **1.165 / 1.155** | **1.176 / 1.155** |
| chacha20 keystream | 1.013 | 1.033 |
| poly1305 (soft) | 1.050 | 1.061 |
| chacha20poly1305 AEAD | 0.513 | 0.520 |
| aes256gcm AEAD (hw AES + PMULL) | **1.185** | **1.178** |
| aes128 ECB blocks (hw) | **15.883** | **15.551** |

The two intermediate configurations are in `run-B-plus-target-features.txt`
(`+aes` alone: AES-GCM 0.50) and `run-C-aes-armv8.txt`.

## 3. What the numbers mean

1. **Both AEADs are bounded by their universal hash, not by the cipher.**
   `1/(1/1.01 + 1/1.05) = 0.51` reproduces ChaChaPoly exactly; AES-GCM's
   1.185 GB/s against a 15.9 GB/s AES engine implies GHASH at ≈1.28 GB/s.
   On this stack every AEAD tops out near 1.0–1.3 GB/s/core.
2. **`snow` adds no measurable overhead** — the Noise suite numbers equal the
   raw-crate numbers (0.52 vs 0.51; 1.17 vs 1.19).
3. **ChaChaPoly cannot be rescued by build flags.** `poly1305 0.8.0` ships only
   `avx2` and `soft` backends (`~/.cargo/registry/.../poly1305-0.8.0/src/backend/`):
   there is no aarch64 backend, so 0.52 GB/s is its ARM ceiling at this pin.
4. **Rate is flat in message size** (4 KiB → 60 KiB: ChaChaPoly 0.468→0.523,
   AESGCM 1.069→1.176). Larger records buy framing and syscall savings only —
   they do not improve crypto throughput.
5. **The hardware is not the limit:** raw AES rounds run at 15.9 GB/s. The
   ~1 GB/s AEAD ceiling is a property of this library stack, which makes a
   different AEAD implementation a legitimate, evidence-backed option to review
   (dependency decision; not evaluated here).

## 4. Ceilings for the product path (arithmetic, not measurements)

Crypto is **~50% of the measured per-frame budget**: the owner measured 4.132 s
for 65,536 frames = 63 µs/frame, and this harness measures 31.5 µs/frame for the
ChaChaPoly AEAD alone at 16 KiB. The other ~31.5 µs/frame is framing, allocation,
copies, the two record writes, the per-I/O `setsockopt` and the relay.

| scenario | per-frame | one stream, one CPU |
| --- | --- | --- |
| measured today (tdx1-noise-upload-1) | 63 µs | **0.260 GB/s** (measured) |
| framing fixes only (F1–F5), crypto unchanged | 31.5 + ~9 | ≈0.40 GB/s |
| AEAD → AESGCM only (F6), framing unchanged | 14 + 31.5 | ≈0.36 GB/s |
| framing fixes + AESGCM | 14 + ~9 | **≈0.71 GB/s** |
| framing fixes + AESGCM + a faster AEAD implementation | ~3 + ~9 | ≈1.4 GB/s |
| measured plaintext control (no crypto at all) | 8.7 | **1.882 GB/s** (measured) |

Conclusions:

- **Fix both.** Crypto alone caps at 0.52/1.18 GB/s and framing alone caps at
  ~0.40 GB/s; neither fix reaches the target by itself.
- **2 GB/s is unreachable in a single stream on one CPU under any option above** —
  even the plaintext control reaches only 1.88 GB/s. The owner's two-stream
  measurements scaled 2.05×, so ≥2 CPUs (or a multi-stream aggregate definition)
  is required. This is the D04 profile decision, and it is now arithmetic rather
  than opinion.
- **A 1 MiB record cannot be one Noise message.** The writev record and the
  ≤64 KiB Noise message are two separate knobs; the record layer should aggregate
  messages rather than raise the AEAD message size.

## 5. Not measured here

- **Container-side confirmation (NOT_RUN).** These are host per-core rates on the
  same silicon; the Docker VM with `--cpus=1` must be re-measured to confirm, and
  the container build needs `-C target-feature=+aes,+sha2 --cfg aes_armv8
  --cfg polyval_armv8` (a musl cross-build) for the AES numbers to apply there.
- **Framing (E2/E3) and relay (E5) remain NOT_RUN** — see the parent review §7.
- **No dependency or product change is proposed by this directory.** It sizes a
  term; the decisions belong to the #192 owner.
