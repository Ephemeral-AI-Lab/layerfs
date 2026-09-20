# Issue #192 upload throughput — read-only source review of the branch working copy

> **Status:** Research; informative and not a product contract.

Date: 2026-09-20. Reviewer: external handoff (not the #192 owner). This review
answers one question — *where does the 0.26 GB/s authenticated-upload cost come
from, and what does v0.1.6 already do better?* — from source, from the owner's own
receipts, and from one component microbenchmark (§5,
[`evidence-aead/`](evidence-aead/README.md)). Nothing in the #192 working tree was
modified, and no product build, container run or product measurement was performed
by this review; source reading waited for the owner's measurement lock to clear
([`benchmark_rules.md`](../../../../../general/benchmark_rules.md)).

## 1. Custody: what was read, and which bytes

Reviewed source is the **dirty working copy** of `/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs`
on branch `codex/pair3-foundation` (197 modified/untracked files; committed head
`fc647e91a`). The files below hash-match the `identities.files` block inside the
owner's own receipt `tdx1-noise-upload-1-perf-v3-20260920/result.json`, so this
review describes exactly the source that produced 0.2598 GB/s.

```text
core/crates/layerfs-bridge/src/adapters/native/connection.rs   3306bf445bebf0f288f70baf1411fc263a4fa95a6e02478d5c81fef6cdea44cc
core/crates/layerfs-bridge/src/adapters/native/payload.rs      854bd842d69abab604311910012284a898a7ea85579a898d405160f5b2291f95
core/crates/layerfs-bridge/src/adapters/native/pipe.rs         194b25dce74123876f451a879d1700ddf764a9a939c2bb8a42805d06a81cef42
core/crates/layerfs-bridge/src/adapters/native/client.rs       7afab826292ab6342d0a4da5632fffa96db8bbdd36d8a5c184544bd0dfb2fc5b
core/crates/layerfs-bridge/src/adapters/native/server.rs       3e4d02839db57defb2e5049d9a7d064b84a90545905eca18ae1ad1a781f0e01d
core/crates/layerfs-bridge/src/adapters/native/protocol/frame.rs 9ee4a5e58afa9d070492bba55decd1771293a90dce519d4dc7c2d1cf6705c5c7
core/crates/layerfs-bridge/src/contract/request.rs             cdac0790b8039e2e06607377738edb240b39ab7be59815ba9fb1e7741861ed0f
core/crates/layerfs-daemon/src/headless.rs                     2a15b9db1b3aea2d81d4aeff6087ed4f12743db37424fac07038d6902387f0ee
core/crates/layerfs-daemon/examples/transport_probe.rs         cd7be4620cafe8fa281220b9ee9bf413bff315bc8d1b811213b8e178a399e02c
```

Receipts read (owner-produced, unmodified):
`implementation/evidence/tdx1-{tcp,noise}-{upload,download}-{1,2}-{perf,verify}-v3-20260920/result.json`
plus the `-perf-v2` TCP upload control. The design this reviews is
`implementation/optimization-spec.md` §3–§4 and `implementation/optimization-handoff.md §1`
(branch-only files; they are not on `main`).

## 2. The measured matrix (owner receipts; not this review's numbers)

Containers: `--cpus=1 --memory=128m --memory-swap=128m --pids-limit=16 --read-only`,
`nano_cpus = 1000000000`. Payload 1 GiB per stream. Target 2.0e9 B/s.

| carrier | direction | streams | transfer s | decimal GB/s | Gbit/s | target |
| --- | --- | --- | --- | --- | --- | --- |
| tcp | upload | 1 | 0.571 | **1.882** | 15.06 | not met |
| tcp | download | 1 | 1.073 | **1.001** | 8.01 | not met |
| noise | upload | 1 | 4.132 | **0.260** | 2.08 | not met |
| noise | download | 1 | 4.611 | **0.233** | 1.86 | not met |
| noise | upload | 2 | 4.027 | 0.533 | 4.27 | not met |
| noise | download | 2 | 3.496 | 0.614 | 4.91 | not met |

Three facts follow directly, and they reframe the problem:

1. **The plaintext control also misses the target** (1.88 up, 1.00 down). No
   change to the crypto layer alone reaches 2 GB/s on this boundary.
2. Noise costs **7.2×** on upload and **4.3×** on download versus the same
   carrier without it.
3. Two streams need **two containers** (two CPUs) and scale ~2.05×/~2.6×. The
   wall is per-container CPU, not the host, the link or the accept loop.
   Upload (container encrypts) and download (container decrypts) are symmetric,
   so the cost is not send-side or receive-side specific.

## 3. Where a 16 KiB frame's cost actually is

`FRAME_BYTES = 16384` (`contract/request.rs:3`), `MAX_FILE = 4 GiB` → 65,536
frames per GiB. Per frame, on the sending side:

| Item | Cost | Source |
| --- | --- | --- |
| Caller buffer clone | 1 alloc + 16 KiB copy | `client.rs` upload thread (`buffer[..n].to_vec()`); probe `buffer.clone()` |
| Frame encode | 1 alloc (16,404 B) + copy | `protocol/frame.rs` `Frame::encode` |
| AEAD output buffer | 1 alloc (16,420 B) | `connection.rs` `Sender::write` (`vec![0; plain.len() + 16]`) |
| AEAD | 1 ChaChaPoly message per 16 KiB | `Sender::write` → `state.write_message` |
| Record write | **2** `write_all` → 2 × (`set_write_timeout` **setsockopt** + `write`) = **4 syscalls**; the 4-byte prefix is its own TCP segment under `TCP_NODELAY` | `connection.rs` `write_record`, `Socket::write` |
| Bookkeeping | 2 `Mutex` lock/unlock + ~6 `Instant::now()` | `Socket::read`/`write` |

Receiving side adds: 2 × (`set_read_timeout` setsockopt + `read`), a ciphertext
`Vec`, a plaintext `Vec`, `Frame::read`'s `vec![0; n]`, and the copy in
`payload.rs` `Input::read` (`out[..n].copy_from_slice`). At 4.13 s / 65,536
frames the sender spends **63 µs per 16 KiB frame** on one CPU.

**The product path is worse than the probe, because the probe excludes it**
(`scope: "bridge transport; C1/C2 and daemon stdin/stdout excluded"`). In
`core/crates/layerfs-daemon/src/headless.rs`:

- **stdin parse**: `Frame::read_optional` issues `read(&mut h[..1])` — a
  **1-byte read** — then `read_exact(&mut h[1..])`, then `read_exact(&mut bytes)`,
  and `Pipe::read` runs a `poll(2)` before *every* read
  (`pipe.rs` `Pipe::ready`). So ≥3 poll + ≥3 read syscalls per frame, plus
  `Submission::read`'s copy into the client buffer.
- **stdout results**: `Pipe::write` hard-caps each syscall at **512 bytes**
  ("POSIX guarantees atomic pipe writes only through PIPE_BUF"). A 16 KiB result
  frame therefore costs 33 iterations of `write_all` × (poll + write) = **66
  syscalls per 16 KiB**, ≈4.2 M syscalls/GiB. The stated justification —
  atomicity — does not apply: the comment itself records that no other product
  writer shares that pipe, and a single writer needs no atomicity.

That asymmetry is consistent with the owner's own full-path observation of
**9.77 s user vs 34.89 s system CPU** in a 45 s run (reported in the #192 thread
on 2026-09-20; it is not a receipt this review read): system time is the
syscall/copy path, which the transport-only cases do not exercise.

## 4. What v0.1.6 already does better on this same boundary

Reference: [`crates/layerfs-fuse/src/live_transport.rs`](../../../../../../crates/layerfs-fuse/src/live_transport.rs),
[`live_wire.rs`](../../../../../../crates/layerfs-fuse/src/live_wire.rs).

| Concern | v0.1.6 | v0.1.7 branch today |
| --- | --- | --- |
| Framing write | `write_frame` uses `write_vectored` with `IoSlice` for prefix + payload — "in the same writev without another payload allocation"; one syscall; short-write aware | two `write_all` calls; prefix is a separate 4-byte segment; payload copied twice before it |
| Frame size | `MAX_FRAME = 1 MiB + 64 KiB` (`live_wire.rs:16`) | 16 KiB → 64× more frames per GiB |
| Timeouts | `set_nodelay(true)` once; tokio's driver owns timeouts — **no per-I/O `setsockopt`** | `set_read_timeout`/`set_write_timeout` before every read/write |
| Steady-state allocations | zero per frame on the send path ("without another payload allocation") | ~3 senders + ~3 receivers per frame |
| Round trips | batched exchanges (`call_batch`, `request_group`, `BatchProgress`) | one frame at a time |
| Confidentiality | capability-authenticated **plaintext** — deliberately *not* an acceptable v0.1.7 profile | per-frame Noise AEAD |

The last row is the important one to read correctly. v0.1.6's advantage is not
"drop encryption" — the optimization spec already rejects a plaintext bearer as a
non-equivalent security profile. The transferable lessons are **granularity**
(1 MiB frames, one vectored write, timeouts set once, no per-frame allocation)
and **cheap AEAD**, not the absence of AEAD.

## 5. The crypto term, measured

E1 is **RUN**: [`evidence-aead/`](evidence-aead/README.md) holds the harness, its
resolved dependency lock and four raw transcripts (Apple M3 Max, rustc 1.85.1,
the exact crate versions in `core/Cargo.lock`, 16 KiB and 60 KiB messages).
Noise suites via `snow`, per core, encrypt / decrypt:

| build configuration | ChaChaPoly | AESGCM |
| --- | --- | --- |
| container-equivalent (`neon` only, `--cfg chacha20_force_neon`) | **0.521 / 0.514 GB/s** | 0.111 / 0.112 |
| `+aes,+sha2 --cfg aes_armv8` | 0.518 / 0.514 | 0.492 / 0.495 |
| plus `--cfg polyval_armv8` | 0.518 / 0.514 | **1.165 / 1.155** |

Raw components at the full-hardware configuration: chacha20 keystream 1.01 GB/s,
poly1305 1.05 GB/s, AES-128 rounds **15.9 GB/s**, AES-GCM 1.185 GB/s.

Four consequences, all measured:

1. **ChaChaPoly is pinned at ~0.52 GB/s and no flag can move it.**
   `poly1305 0.8.0` ships `avx2` and `soft` backends only — there is no aarch64
   backend at this pin. `1/(1/1.01 + 1/1.05) = 0.51` reproduces the AEAD exactly.
2. **AESGCM needs two crate-specific cfgs, not just target features.** Target
   features alone change nothing (0.111 → 0.113); `aes_armv8` gives 0.50;
   `polyval_armv8` (PMULL GHASH, in `polyval 0.6.2`) gives 1.17. All three are
   documented crate knobs, not patches.
3. **Every AEAD here is hash-bound near 1 GB/s while the AES engine does 15.9.**
   The ceiling is the library stack, not the silicon — which makes a different
   AEAD implementation an evidence-backed option rather than a guess.
4. **`snow` costs nothing measurable** (0.52 vs 0.51 raw; 1.17 vs 1.19 raw), and
   **rate is flat in message size** (4 KiB → 60 KiB). Larger records buy framing
   only. `snow` also caps one Noise message at 65,519 bytes (`MAXMSGLEN`; a
   256 KiB message returns `Error::Input`), so **a 1 MiB writev record must be
   several Noise messages — record size and AEAD message size are separate knobs.**

## 6. Learnings, prioritized

F1–F5 are source-level and still unmeasured; F6 is now measured in §5; F7 is
recommended regardless.

| # | Change | Why | Risk / gate |
| --- | --- | --- | --- |
| F1 | Record size 16 KiB → ≥256 KiB, as **several ≤64 KiB Noise messages per record** | **Measured 4.0× framing win** vs today (≈2.7 µs per 16 KiB equivalent); matches v0.1.6's ~1 MiB frames without violating `MAXMSGLEN`. Crypto rate does not improve with size (§5.4) | Protocol/profile change; verify per-direction working set, `MAX_REPLAY` 8 MiB, C1 streaming batches, bounded-transaction rule (`frame_budget` already tolerates it) |
| F2 | One vectored record write (prefix + payload) | Removes the 4-byte segment and halves write syscalls; exact v0.1.6 `write_frame` pattern | Bounded writev slices; short-write loop required |
| F3 | Stop `setsockopt` per I/O | One syscall per read and write today — but **not demonstrated as a win** (within run-to-run noise on loopback) | Cleanliness only; re-measure in-container. Deadline granularity stays per-operation |
| F4 | Reuse buffers; decrypt into a slice | ~3 allocations + ~4 copies per frame today | Borrow lifetimes across `Frame::read`; keep "provisional until terminal success" |
| F5 | Fix the daemon relay: drop the 512-byte cap, one header read, writev results — **now measured as the first-priority fix** | [`evidence-phase1/`](evidence-phase1/README.md): the cap alone pins the stage at 0.30 GB/s / 34 syscalls per 16 KiB frame; fixed ⇒ 3.25 GB/s / 3 syscalls | Keep bounded memory and the no-extra-writer rule; a single-writer pipe needs no `PIPE_BUF` atomicity |
| F6 | **Measured:** move the suite to `Noise_KK_25519_AESGCM_SHA256` and build with `+aes,+sha2 --cfg aes_armv8 --cfg polyval_armv8`; or evaluate an assembly-backed AEAD (the hash is the cap, not the hardware) | 0.52 → 1.17 GB/s per core for this one change | Becomes an explicit **hardware requirement** (ARMv8 crypto) that must fail loudly, never fall back to the 0.111 GB/s soft path; a new dependency needs its own decision and review |
| F7 | Record the CPU split per diagnostic case | Today's receipt has no CPU field; §5 now supplies the crypto term, so the remainder is framing/relay by subtraction | cgroup `cpu.stat` (`usage_user`, `usage_system`, `nr_throttled`, `throttled_usec`) is one command |

## 7. Ceilings and the remaining experiments

Crypto is **~50% of the measured per-frame budget**: the owner measured 63 µs per
16 KiB frame, and §5 measures 31.5 µs of that as the ChaChaPoly AEAD alone; the
other ~31.5 µs is framing, allocation, copies, two record writes, per-I/O
`setsockopt` and the relay. Arithmetic from measured components (not measured):

| scenario | per-frame | one stream, one CPU |
| --- | --- | --- |
| measured today | 63 µs | **0.260 GB/s** (measured) |
| framing fixes only | 31.5 + ~9 | ≈0.40 GB/s |
| AESGCM only | 14 + 31.5 | ≈0.36 GB/s |
| framing fixes + AESGCM | 14 + ~9 | ≈0.71 GB/s |
| framing fixes + AESGCM, **in-container measured terms** | 14 + 7.9 | ≈0.75 GB/s (upload) · ≈0.54 (download, receive-path bound) |
| plaintext control, no crypto | 8.7 | **1.882 GB/s** (measured) |

So **fixing either term alone cannot reach 2 GB/s, and neither can fixing both in
a single stream on one CPU** — even the plaintext control reaches only 1.88 GB/s.
With the measured 2.05× two-stream scaling, the target needs ≥2 CPUs or a
multi-stream aggregate definition: that is the D04 profile decision.

| ID | Case | State |
| --- | --- | --- |
| E1 | AEAD-only crypto rates | **RUN** — [`evidence-aead/`](evidence-aead/README.md) |
| E1c | Same rates inside the `--cpus=1` container, musl cross-build | **NOT_RUN** — host rates assumed to transfer; confirm before quoting container numbers |
| E2/E3 | Framing ladder: current record vs one `writev` vs 256 KiB records | **RUN** — [`evidence-phase1/`](evidence-phase1/README.md): 1.6× (one `writev`), 4.0× (256 KiB records); per-I/O `setsockopt` not demonstrated |
| E5 | Relay ladder: daemon stdin parse and the 512-byte stdout cap | **RUN** — [`evidence-phase1/`](evidence-phase1/README.md): the cap alone pins the stage at **0.30 GB/s, 34 syscalls/frame**; fixed ⇒ 3.25 GB/s, 3 syscalls/frame |
| E2c | Same ladders inside the `--cpus=1` container | **RUN** — [`evidence-phase1/`](evidence-phase1/README.md): framing 21–33 µs/frame today vs **7.9 µs at 256 KiB records**; download flat at ≈16.2 µs/frame for every shape; harness reproduces tdx1's 1.88/1.00 GB/s controls |

## 8. What this review does not claim

- The only measurements taken here are the component microbenchmarks in
  §5/[`evidence-aead/`](evidence-aead/README.md); every end-to-end number is
  quoted from the owner's `tdx1-*` receipts with their container limits stated.
- Those microbenchmarks are **not** product acceptance: no socket, pipe,
  container, Store or daemon was involved, and the container-side rates are
  explicitly `NOT_RUN` (E1c).
- The ceilings in §7 are arithmetic from measured components, not measurements.
- Two-stream scaling does not prove the host side is idle — only that it scales.
- This review does not authorize changing the frozen workload, the 2 GB/s target,
  the frame counts or any timeout; per the optimization spec §7, a size that
  cannot fit its budget is recorded `NOT_RUN`.
- Nothing here changes the multi-writer decision: framing and AEAD work is
  orthogonal to save-owned publication and to the accepted duplicate bytes.
