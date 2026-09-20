# Registered selection — the owner's six cases on the new profile (identity set v4)

> **Status:** Research; informative and not a product contract. One performance
> sample and one full-byte verification per row, exactly as the frozen tdx1
> specification requires. No best-of, no median, no timing-based retry.

Run with the owner's own harness, unmodified except that its artifact manifest is
now selectable (`--manifest`, default unchanged = v3), against a new identity set
`transport-probe-artifacts-v4-20260920.json` (192 pinned files; **7 differ from
v3**: the three changed bridge sources, the bridge manifest, and the host/musl
probe binaries — plus the new `core/.cargo/config.toml` and three new tests).

## Results (receipts: `tdx1v2-<case>-<mode>-20260920/result.json`)

| row | owner baseline (`tdx1-*` v3) | registered now | change | target met |
| --- | --- | --- | --- | --- |
| raw TCP control, upload, 1 | 1.882 GB/s | **2.539** | control | **True** |
| raw TCP control, download, 1 | 1.001 | 1.218 | control | False |
| **authenticated upload, 1** | **0.2598** | **1.043** (15.70 µs/frame) | **+301 %** | False |
| **authenticated download, 1** | **0.2329** | **0.737** (22.22 µs) | **+216 %** | False |
| **authenticated upload, 2** | **0.5332** | **1.460** (11.22 µs) | **+174 %** | False |
| **authenticated download, 2** | **0.6142** | **0.764** (21.43 µs) | **+24 %** | False |

Every one of the six `verify` rows is `PASS` with the full payload byte-verified
(1 GiB per stream, 2 GiB for the two-stream rows) and every `cleanup` is `PASS`.
All twelve rows are `status: PASS`; none is `INELIGIBLE`, `INCOMPLETE` or `FAIL`.

## What the registered numbers say

1. **The authenticated path is 2.7–4.0× faster than the baseline** on the upload
   side and 3.2× on single-stream download.
2. **It does not meet the 2 GB/s target**, and only the plaintext upload control
   does. Upload scales with streams (1.043 → 1.460, +40 %), download does not
   (0.737 → 0.764, +4 %).
3. **The download direction is now the weak side.** It is receive-bound: the AEAD
   sits on the critical path (receive, then decrypt, then next record), and E2c
   already measured the container's receive path as flat in record shape
   (≈16 µs/frame for every variant). Overlapping receive with decrypt is the
   remaining design lever there, not more micro-optimisation.
4. **Machine state moved the control by up to +35 %** (1.882 → 2.539). Diagnostic
   samples from the reduced driver on the same source ranged 0.862–1.021
   (upload-1) and 1.418–2.026 (upload-2); those are retained but are **not** the
   claim — the registered rows above are. A campaign would need repeated matched
   samples; single samples cannot carry a target claim.

## Harness change (one option, default preserved)

`core/crates/layerfs-daemon/tests/transport_diagnostic.py` gained
`--manifest <path>` (default: the original
`transport-probe-artifacts-v3-20260920.json`). Existing v3 receipts remain exactly
reproducible; a new source/profile identity set is selected explicitly instead of
silently reusing a stale pin.

## Also found this round

The transport build profile (`core/.cargo/config.toml`) is part of the identity:
on `aarch64-unknown-linux-musl` the crate cfgs alone leave AES-GCM at 0.54 GB/s,
and only `-C target-feature=+aes,+sha2` reaches 1.39 GB/s. The macOS host target
enables `+aes` by default, which hides this — the container measurement is the one
that decides. The profile therefore declares the ARMv8 crypto requirement, and
`handshake()` refuses a CPU that lacks it rather than running the soft path.

## Repeat diagnostics — what reproduces and what does not

Three attempts per row with the reduced driver (diagnostic only; every attempt
retained, failures included). This exists to test the single-sample registered
rows above, not to replace them.

| row | samples (GB/s) | median | spread |
| --- | --- | --- | --- |
| raw TCP control, upload, 1 | 2.265 · 2.172 · 2.596 | 2.265 | 20 % |
| raw TCP control, download, 1 | 1.266 · 1.259 · 1.225 | 1.259 | 3 % |
| authenticated upload, 1 | 0.866 · 1.041 · 1.044 | **1.041** | 20 % |
| authenticated download, 1 | 0.748 · 0.782 · 0.797 | **0.782** | 6 % |
| authenticated upload, 2 | 2.051 (2 attempts failed readiness) | — | — |
| authenticated download, 2 | 1.541 (2 attempts failed readiness) | — | — |

**Single-stream rows are confirmed.** The repeat medians (1.041 upload, 0.782
download) agree with the registered single-sample rows (1.043, 0.737) inside the
observed spread. The +301 % / +216 % single-stream claims stand.

**Two-stream rows do not reproduce, and the disagreement is systematic rather than
random:** the reduced driver measured 2.051 (upload-2) and 1.541 (download-2) where
the owner's harness measured 1.460 and 0.764 on the same source, the same image and
the same container limits. Two of three attempts failed readiness in both drivers.

Differences between the two drivers that remain candidates (none confirmed):

- the owner's harness starts the second container only after the first reports
  READY; the reduced driver starts both concurrently;
- the owner's harness adds `--cap-drop=ALL --security-opt=no-new-privileges`;
- the owner's harness wraps the run in a 13 s `SIGALRM` budget.

**Consequence for the target:** the two-stream aggregate is not yet a decision-grade
number in either direction (0.76–2.05 GB/s across drivers). The 2 GB/s question
needs the two-stream rows instrumented — for example per-container client-side
elapsed alongside the server-side `transfer_ns` — before any campaign is run. No
target claim is made here, in either direction.
