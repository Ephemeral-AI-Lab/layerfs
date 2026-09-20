# F5 + record batching — applied to the #192 branch working copy

> **Status:** Research; informative and not a product contract. Applied change, not a release.

Date: 2026-09-20. Executes F5 and the coalescing half of F1/F2 from the
[parent review](../README.md) in
`/Users/yifanxu/.codex/worktrees/pair3-foundation/layerfs` (branch
`codex/pair3-foundation`). The owner thread was idle (last event 09:10:30Z, no
processes) before the edits; its other work — multi-writer C2, service, schema 7 —
was not touched.

## 1. What changed

| file | change | measured before → after |
| --- | --- | --- |
| `adapters/native/pipe.rs` | `Pipe::write` writes the whole slice; the portable 512-byte `PIPE_BUF` cap is gone, with the single-writer justification recorded | relay stage: ≤0.30 GB/s (34 syscalls per 16 KiB frame) → **1.47–1.56 GB/s** (11 µs/frame) |
| `adapters/native/protocol/frame.rs` | the fixed 20-byte header is read in one `read`; a short read falls back to `read_exact`; a zero-length first read is still the only clean-EOF signal | one fewer poll+read per submitted frame |
| `adapters/native/connection.rs` | full-size `Body`/`ResultData` records coalesce into one `write_vectored` up to 256 KiB; a control record or any record under half a frame is written immediately; `end_upload` flushes | product upload path: 93.0 → **82.9 µs/frame** on the host (release) |
| `tests/relay_write.rs` (new) | asserts the relay writes frames intact and every byte arrives; prints the rate as a diagnostic | — |
| `tests/upload_batch.rs` (new) | asserts a batched bulk upload is byte-identical frame by frame at the receiver; prints sender and total rates | — |

The `Frame` and record sizes are unchanged, so **the bytes on the wire are
identical** — only how many records one syscall carries changed. No contract,
limit, schema, C1/C2 or service file was modified.

**The progress contract decided the batching rule.** The first version batched
every data record; `productive_upload_keeps_the_response_wait_alive` failed
(seven 1-byte frames one second apart were held in the batch, so the peer's
`IO_PROGRESS_MS` clock expired). Short and control records now write immediately:
coalescing must never defer a progress signal.

## 2. Verification actually run (branch working copy)

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked        all crates: no failures
cargo +1.85.1 test -p layerfs-bridge                             5 protocol, 2 native_connections, relay_write, upload_batch: PASS
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check   clean (whole core workspace)
cargo +1.85.1 clippy -p layerfs-bridge --all-targets -- -D warnings   clean
python3 core/tools/check_product_boundary.py                     PASS (174 production files)
```

`clippy --all-targets -D warnings` **fails on the owner's untracked
`layerfs-daemon/examples/transport_probe.rs`** ("use of `format!` to build up a
string from an iterator") — pre-existing, unrelated to this change, left for its
author.

## 3. Measurements (host, release, loopback, ChaChaPoly as pinned)

```text
 relay result path (256 MiB, 16 KiB frames)     1.47–1.56 GB/s · 11 µs/frame
                                                 same rule before the cap removal: ≤0.30 GB/s
 product upload (128 MiB, 16 KiB frames)        coalescing ON   82.9 µs/frame  (sender 81.8)
   — A/B with coalescing disabled by a          coalescing OFF  93.0 µs/frame  (sender 92.2)
   temporary one-line flush (now reverted)      ⇒ +12 % on the host
```

**The batching win is small on the host because crypto dominates there.** The same
A/B inside the container is the number that matters and is `NOT_RUN`: E2c measured
the framing shapes at 18.9 µs/frame (one write per frame) against 7.9 µs/frame
(one write per 256 KiB) for 16 KiB-equivalent payload, so in-container the effect
should be substantially larger than 12 %.

**New finding, and a correction to the parent review.** The product upload path
costs ~83 µs per 16 KiB frame while the measured ChaChaPoly term is 31.4 µs — so
**~51 µs/frame is allocation, copy and buffer-touch overhead in the sender**, not
syscalls and not crypto. The parent review's §6 table called F4 (buffer reuse)
"≈1 % by arithmetic" from memcpy throughput; that arithmetic was wrong, and F4 is
now the largest identified remaining term on the sender.

## 4. Reverting

`f5-batching.patch` contains exactly these changes. The baseline files were
reconstructed by inverse substitution and **SHA-256 verified against the hashes
recorded before any edit**:

```text
 connection.rs 3306bf445bebf0f288f70baf1411fc263a4fa95a6e02478d5c81fef6cdea44cc
 pipe.rs       194b25dce74123876f451a879d1700ddf764a9a939c2bb8a42805d06a81cef42
 frame.rs      9ee4a5e58afa9d070492bba55decd1771293a90dce519d4dc7c2d1cf6705c5c7
```

The two new tests are copied here verbatim. Nothing was committed: the branch
keeps its own commit history, and these changes sit alongside the owner's
uncommitted work as reviewable edits.

## 5. Not done here

- **F6 (AESGCM + the three cfgs)** — it changes the Noise security selection, which
  the tdx1 specification froze ("does not … change the Noise suite"). It needs an
  explicit owner revision of that line plus a runtime capability check; it is not
  applied.
- **F4 (buffer reuse)** — now the top remaining sender-side item (see §3), not started.
- **In-container measurement of this change** (a `tdx2-relay-1gib` companion and a
  re-run of the framing ladder) — `NOT_RUN`, and the number that would confirm the
  batching benefit.
