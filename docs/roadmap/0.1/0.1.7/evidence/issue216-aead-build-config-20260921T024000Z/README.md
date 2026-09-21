# Why the transport is slow now: the ARMv8 build flags were never applied

> **Status: DIAGNOSTIC / root-cause record. Not a gate, not a release claim.**
> Question: the repository records **1.043 GB/s** authenticated upload, but today's
> measurements show **0.22-0.26 GB/s**. Answer: **nothing in the product regressed.**
> The builds in between lost `core/.cargo/config.toml`, so the bridge fell back
> from ARMv8-accelerated AES-GCM to ChaCha20-Poly1305. Rebuilt with the flags, the
> same source measures **802 MiB/s** on one stream and **1.556 GiB/s** on two.

## 1. It is a build flag, measured head to head

Same source commit, same machine, same probe protocol, same session, only the
build invocation differs. Both binaries come from an identical tree; the only
difference is whether cargo applied `core/.cargo/config.toml`.

| Build | upload 1 stream | upload 2 streams | download 1 stream |
| --- | ---: | ---: | ---: |
| `cargo … --manifest-path core/Cargo.toml` **from the repo root** | 0.209 GiB/s (214 MiB/s) | 0.415 GiB/s (425 MiB/s) | 0.209 GiB/s |
| `cd core && cargo …` (**flags applied**) | **0.783 GiB/s (802 MiB/s)** | **1.520 GiB/s (1556 MiB/s)** | **0.785 GiB/s** |

**3.75x on one stream, 3.66x on two.** The flagged build is the "700 MB/s+" the
owner remembered, and it matches the committed `tdx1v2` receipts (upload
1.043 GB/s, download 0.737 GB/s) within this host's headroom.

The probe sha256 and the exact invocations are in `identity.json`; the per-case
receipts are in `arms/ab-*`.

## 2. The bridge says so itself

`core/crates/layerfs-bridge/src/adapters/native/connection.rs` gates AES-GCM on
`all(target_arch = "aarch64", aes_armv8, polyval_armv8)` and documents the
consequence: aarch64 **needs** those cfgs, otherwise it "keeps ChaCha20-Poly1305
(0.52 GB/s, `poly1305` has no aarch64 backend at this pin)", while unaccelerated
AES-GCM would be 0.111 GB/s. So an unconfigured aarch64 build is not broken - it
silently takes a supported but **2-4x slower** suite. Correctness, framing,
authentication and the declared bounds are unaffected either way.

`core/.cargo/config.toml` supplies exactly the missing half:

```toml
[target.'cfg(target_arch = "aarch64")']
rustflags = ["--cfg","aes_armv8","--cfg","polyval_armv8","--cfg","chacha20_force_neon",
             "-C","target-feature=+aes,+sha2"]
```

## 3. Why the flags vanish: cargo reads config from the CWD, not the manifest path

Cargo discovers `.cargo/config.toml` by walking up from the **current working
directory**. `core/.cargo/config.toml` therefore applies only when cargo runs with
a cwd inside `core/`. Two invocations that look equivalent are not:

| Invocation (from the repository root) | cwd | config found | result |
| --- | --- | --- | --- |
| `cargo build --manifest-path core/Cargo.toml …` | root | none (root has no `.cargo/`) | **no flags** |
| `cd core && cargo build …` | `core/` | `core/.cargo/config.toml` | flags applied |

Verified from the compiler command line, not inferred: the `layerfs_bridge` rustc
invocation from the root carries no `target-feature` at all, while the one from
`core/` carries `target-feature=+aes,+sha2` plus the three cfgs.

**This is the form the repository's own rule prescribes.**
`core/AGENTS.md` says to verify with
`cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`, and the
repository root is the natural cwd for that command. Tests and clippy are
unaffected in *correctness* terms, but any build produced that way - including
service and daemon binaries and anything a benchmark runs - is the slow-AEAD
build.

## 4. The committed evidence already shows the drop, and its cause

The transport-probe artifact manifests record both the rustflags and whether
`core/.cargo/config.toml` was part of the identity set:

| Manifest | rustflags recorded | `core/.cargo/config.toml` in `files` | Receipt rate (upload, 1 stream) |
| --- | --- | --- | ---: |
| `transport-probe-artifacts-20260920.json` | `--cfg=chacha20_force_neon` | no | - |
| `transport-probe-artifacts-v2-20260920.json` | `--cfg=chacha20_force_neon` | no | - |
| `transport-probe-artifacts-v3-20260920.json` | `--cfg=chacha20_force_neon` | no | **0.260 GB/s** (`tdx1-noise-upload-1-perf-v3`) |
| `transport-probe-artifacts-v4-20260920.json` | `--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2 (aarch64 profile supplied by core/.cargo/config.toml)` | **yes** | **1.043 GB/s** (`tdx1v2-...-upload-1-perf`, image `d0ad65db…` = the v4 manifest's image) |

So the 1.043 -> 0.260 GB/s step in the repository's own evidence is the v4 -> v3
artifact change: the run whose artifacts carried the aarch64 profile is 4x faster
than the one whose artifacts did not. Nothing else differs - same case, same
carrier, same streams, same 1 GiB payload, same 65536 chunks, and both container
configs are `nano_cpus=1`, 128 MiB, read-only, no mounts.

## 5. Corrected route numbers

The route measurements in this session were built from the repository root, so
they are slow-AEAD numbers. Re-run with the flagged build (concurrency 1, same
protocol as the owner-requested pair):

| Shape | slow-AEAD build (committed) | flagged build (this page) |
| --- | ---: | ---: |
| 1 x 512 MiB | 6.7407 s, 73.7-77.3 MiB/s | **5.342 / 4.892 s, 95.8 / 104.7 MiB/s** |
| 64 x 8 MiB | 7.3473 s, 68.4-70.8 MiB/s | **5.550 / 5.617 s, 92.3 / 91.2 MiB/s** |
| ratio | +9.0% | **+9.1%** |

The *relative* penalty of the 64 x 8 MiB batch is unchanged (+9%), so that finding
stands; the absolute rates were depressed ~30% by the slow suite, and effective
cores fall from 1.29-1.33 to 0.94-1.07 because AES runs on the crypto unit instead
of several cores of NEON ChaCha. The route is now close to the store-only rate
(115 MiB/s), which is what a fast transport should leave as the bottleneck.

## 6. What is unaffected

- **Correctness, format, authentication, bounds**: ChaCha20-Poly1305 is a
  supported suite; both builds pass the same protocol and verification cases.
- **In-process C1/C2 measurements** (`measure_admission`, `measure_ingest`: the
  store rate, drift, within-run shape, memory law) never enter the AEAD path, so
  their conclusions are unchanged; they were, however, compiled with different
  target features, so their absolute CPU figures carry a small unquantified delta.
- **[#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209) is a different
  regression and is not this one.** #209 is the single-writer *store* path: the
  multi-writer model (`eb319aaa9`) commits once per pack append - 42x more commits -
  and cost `history-stride10` 2.06x (16.360 -> 33.116 s); one treatment (removing
  the locator `ORDER BY … LIMIT ?`) recovered 6.65 s to 26.467 s, and the remaining
  commit cost was attributed to page writes proportional to pack body, with the
  page-cache hypothesis refuted. That lane is metadata-heavy history storage and
  was not measured here.

## 7. What would stop it recurring

1. **Build where the config lives**: `cd core && cargo build --release --locked …`,
   or pass the flags explicitly
   (`RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2"`).
   For aarch64 musl cross-builds the same flags must be given for the target.
2. **Make the suite observable**: the bridge currently reports nothing about the
   negotiated AEAD, so a lost flag is visible only as a rate. A one-line diagnostic
   naming the suite at connection setup would make the configuration checkable at
   runtime.
3. **Make the guard fail instead of pass**: the tdx1 driver records whether
   `core/.cargo/config.toml` is present and records the rustflags, but a run
   without them still reports `status: PASS` (only `target_met` is false, against a
   target that is not this lane's gate). A manifest that names the aarch64 profile
   could require the config file in its identity set.

Items 2 and 3 change product source or harness policy, so they are **proposed
here, not implemented**; item 1 is an invocation change for whoever builds.

## 8. Gaps

- A/B is one sample per case per build; the route pair has two samples per shape.
- Both A/B endpoints ran on this loaded host and the frozen container topology was
  not reproduced, so the absolute numbers are this host's, not a qualification.
- The two probes differ only by build flags, but they were built into different
  target directories; the tree, commit and lockfile are identical (see
  `identity.json`).
- No measurement of the *x86_64* path was taken (there AES-NI is detected at
  runtime, so the cfg loss would not apply).
