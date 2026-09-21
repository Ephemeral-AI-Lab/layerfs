# E2/E3/E5 — framing and relay ladders (no crypto, syscalls counted)

> **Status:** Research; informative and not a product contract.

Date: 2026-09-20. Companion to the [parent review](../README.md) and
[`evidence-aead/`](../evidence-aead/README.md). Phase 1 of the plan closed the
last unknown — *how much of the non-crypto half of the per-frame cost is
recoverable?* — and produced one result big enough to reorder the whole plan.

**Component measurements, not product acceptance.** No Store, no container, no
product source was modified; the harness is `phase1bench.rs` in this directory.

## 1. Conditions

```text
host          Apple M3 Max, 14 CPUs, macOS (aarch64); rustc 1.85.1
E2/E3         1 GiB per variant over 127.0.0.1 loopback, TCP_NODELAY,
              one sender thread + one receiver thread, no crypto, no pipe
E5            1 GiB per variant through pipes with poll(2) before every syscall,
              modelling the daemon's stdin parse and stdout write rules
payload       E2/E3: 16 KiB frames, 256 KiB records · E5: 16 KiB frames
samples       three runs each; medians and full ranges below (variance is real)
cache         generated in-memory payload; no storage read or write
```

## 2. E2/E3 — framing ladder (µs per 16 KiB frame, three runs)

| variant | counted syscalls/frame | run 1 | run 2 | run 3 | median |
| --- | --- | --- | --- | --- | --- |
| plain-16k control (one write, one read) | 2.00 | 4.59 | 5.29 | 6.00 | 5.29 |
| **record, 2 writes + `setsockopt` per I/O (today)** | 8.00 | 10.95 | 9.29 | 13.24 | **10.95** |
| record, 2 writes, timeouts set once (F3) | 4.00 | 10.91 | 9.34 | 10.70 | **10.70** |
| record, one `writev` + one read, timeouts once (F2) | 2.00 | 6.61 | 6.73 | 7.27 | **6.73** |
| **256 KiB record, one `writev` (F1+F2)** | 2.00 | 43.05 | 43.04 | 43.48 | **43.05 per record ≈ 2.7 per 16 KiB** |

Findings:

1. **A single vectored record write is ~1.6× faster than today's two-write
   record** (10.95 → 6.73 µs/frame) and matches the plaintext control within
   noise. F2 is the durable framing win.
2. **Larger records are worth much more than syscall micro-tuning**: 256 KiB
   records land at ≈2.7 µs per 16 KiB equivalent — **≈4.0× better than today's
   shape** and ≈2.5× better than the already-fixed 16 KiB shape (6.09 GB/s vs
   1.50 GB/s in run 2). This is F1.
3. **Removing the per-I/O `setsockopt` (F3) is NOT a measured win on this host**
   (10.95 vs 10.70 µs — inside the run-to-run spread, and one earlier run showed
   the opposite ordering). It stays a cleanliness change, not a lever, until it
   is measured inside the container where syscalls cost more.
4. Variance is 20–40% between runs on this machine; every claim above is a
   median/ratio, and the raw transcripts carry all three runs.

## 3. E5 — relay ladder (µs per 16 KiB frame, three runs)

| variant | counted syscalls/frame | run 1 | run 2 | run 3 | median GB/s |
| --- | --- | --- | --- | --- | --- |
| stdin parse, today (1 B + 19 B + body, poll per read) | 6.03 | 7.51 | 6.86 | 6.79 | 2.39 |
| stdin parse, fixed (one 20 B read + body) | 3.07 | 5.26 | 5.03 | 5.21 | 3.14 |
| **stdout write, today (512 B cap + poll per write)** | **34.00** | 56.23 | 55.60 | 51.67 | **0.295** |
| stdout write, fixed (64 KiB, poll per write) | 3.07 | 5.04 | 4.99 | 5.17 | **3.25** |

```text
 daemon stdout relay, one 16 KiB result frame
   today  ████████████████████████████████████████████████████ 55 µs · 34 syscalls · 0.30 GB/s
   fixed  █████                                                5 µs ·  3 syscalls · 3.25 GB/s
          ⇒ 10.4× faster, 11× fewer syscalls — for deleting a 512-byte cap
```

Findings:

1. **The 512-byte `Pipe::write` cap alone pins any payload crossing the daemon's
   stdout at ≈0.30 GB/s** (0.291/0.295/0.317 GB/s across runs — extremely stable,
   unlike the socket ladder). `Pipe::write` caps each syscall at 512 bytes, so a
   16 KiB frame costs 33 poll+write pairs; the comment's atomicity justification
   does not apply to a single-writer pipe.
2. Fixing it is a 10.4× improvement on that stage with 11× fewer syscalls, and it
   is a **deletion**, not new machinery.
3. The stdin parse pattern (1 B, then 19 B, then body, `poll` before each) costs
   2.96 extra syscalls/frame ≈ **1.35×**; smaller than the stdout effect but free
   to fix.
4. **This stage is entirely outside the tdx1 diagnostic scope** ("daemon
   stdin/stdout excluded"), which is why no receipt so far shows it. It does not
   affect the *upload* path's 1 GiB payload (upload results are tiny frames); it
   affects every large **read/result** payload the daemon writes out — i.e. the
   download direction and `ReadFile` results.

## 4. Revised priority (measured value per unit of change)

| # | change | measured effect | verdict |
| --- | --- | --- | --- |
| F5 | delete the 512-byte stdout cap, fix the 1 B+19 B header read | **10.4×** on the relay stage; 1.35× on stdin parse | **first**: trivial, enormous, in-scope for the product path |
| F1+F2 | 256 KiB records + one vectored write | **4.0×** framing vs today (2.5× vs fixed 16 KiB) | second: protocol/profile change, still bounded |
| F6 | Noise AESGCM + `+aes,+sha2 --cfg aes_armv8 --cfg polyval_armv8` | **2.3×** on the crypto term (`evidence-aead`) | third: ~2.3× on ~50% of the probe cost |
| F4 | buffer reuse (11 alloc/copy sites) | ≈1% by arithmetic (copies run at ~60 GB/s) | cosmetic; do it while touching the code |
| F3 | drop per-I/O `setsockopt` | **not demonstrated** (within run-to-run noise) | cleanliness only; re-measure in-container |

## 4. E2c — the same ladder across the real Docker<->host boundary

`phase1std.rs` (std only, cross-compiled to `aarch64-unknown-linux-musl` with the
owner's `rust-lld`), run against the owner's own scratch image with the tdx1
container limits (`--cpus=1 --memory=128m --memory-swap=128m --pids-limit=16
--read-only`). No crypto, no Store. Single run per variant; the upload ladder was
run twice to expose VM variance.

**Upload — container client (1 CPU) sends 1 GiB to the host** (µs per 16 KiB):

| variant | run 1 | run 2 | 16 KiB-equivalent µs | GB/s |
| --- | --- | --- | --- | --- |
| plain-16k control (1 write, 1 read) | 8.31 | 8.34 | **8.3** | **1.96** |
| record, 2 writes + `setsockopt` per I/O (today) | 32.62 | 21.14 | 21–33 | 0.50–0.78 |
| record, 2 writes, timeouts once | 27.03 | 18.90 | 19–27 | 0.61–0.87 |
| record, one `writev` | 18.75 | 18.90 | **18.8** | **0.87** |
| **256 KiB record, one `writev`** | 7.96 | 7.93 | **7.9** | **2.06** |

**Download — host sends, container client (1 CPU) receives**: every variant,
including 256 KiB records, lands in **15.7–16.6 µs per 16 KiB (≈1.0 GB/s)**. The
record shape makes no measurable difference on the receive side; the container's
receive path is the wall.

The harness reproduces the owner's own controls, which is what makes these numbers
usable: plaintext upload **1.96 GB/s** vs `tdx1-tcp-upload-1` **1.882**;
plaintext download **0.99–1.00** vs `tdx1-tcp-download-1` **1.001**; 256 KiB
record upload **2.07** (no prior receipt).

### The model closes

```text
 tdx1-noise-upload-1 measured                     63.0 µs/frame   0.260 GB/s
   in-container framing, today's record shape     21.1–32.6 µs   (E2c, measured)
   + ChaChaPoly AEAD (evidence-aead)              31.5 µs
   ------------------------------------------------------------
   sum                                            52.6–64.1 µs   ⇒ closes on run 1
```

### What that changes in the projections (single stream, one CPU)

| scenario | upload | download |
| --- | --- | --- |
| today (measured) | **0.260** | **0.233** |
| F1+F2 (256 KiB records + one writev) | 7.9 + 31.5 ⇒ ≈0.42 | ≈0.23 (framing-insensitive) |
| F6 (AESGCM + cfgs) | 26.9 + 14.0 ⇒ ≈0.40 | ≈0.54 (crypto is on the receive path) |
| **F1+F2+F6** | **7.9 + 14.0 ⇒ ≈0.75** | ≈0.54 |
| + faster AEAD implementation | ≈1.4 | ≈0.83 |
| plaintext ceiling (measured) | 1.96 | 1.00 |

Two consequences beyond the earlier plan:

1. **Framing fixes pay on the send side and not on the receive side.** The
   download path is pinned near 1.0 GB/s by the container's receive path, so
   F1–F4 buy nothing there; its only lever is F6 and, later, overlapping receive
   with decrypt (the AEAD currently sits on the critical path — receive 16.2 +
   decrypt 31.9 µs, against 70.4 µs measured).
2. **F3 is real in-container** (21.1 vs 18.9 µs, and 32.6 vs 27.0 in run 1) even
   though it was invisible on macOS loopback. Keep it, but as part of the record
   rewrite rather than as a standalone claim.

### Variance warning

The upload ladder's 16 KiB record variants moved 21.1 → 32.6 µs between two
identical runs on the same machine (the VM and cgroup quota are noisy), while the
plaintext control, the 256 KiB record and the whole download ladder were stable
to ~1%. **The stable findings are: 256 KiB records (≈7.9 µs), the plaintext
control (≈8.3 µs), and the flat ≈16.2 µs download floor.** The ordering among the
16 KiB record shapes needs repeats before anyone quotes a single figure.

## 5. Ceiling after the fixes (arithmetic, still not a product measurement)

```text
 probe path (transport-only), one stream, one CPU
   container plaintext baseline (measured, tdx1-tcp-download-1)   16.4 µs/frame
   + AESGCM crypto (measured)                                     14.0 µs/frame
   + framing after F1+F2 (loopback ratio; container unmeasured)    ~5–16 µs/frame
   --------------------------------------------------------------
   ⇒ ≈0.55–0.78 GB/s single stream; two streams (2.05× scaling) ≈1.1–1.6 GB/s
   ⇒ 2 GB/s remains out of reach for one stream on one CPU (plaintext control: 1.88)

 product read/download path
   today bounded by the relay at ≈0.30 GB/s no matter how fast the bridge is
   after F5, bounded by the same bridge figures as above
```

## 6. Not measured here

- **Container-side confirmation (NOT_RUN):** the loopback and pipe figures are
  host-side; the Docker VM's syscall/network costs are higher (the container's
  plaintext baseline is 16.4 µs/frame against ~5 µs/frame on loopback), so the
  absolute framing savings in-container still need E2c.
- **No product source was changed**, and none of these numbers are acceptance
  evidence for O01–O11.
