# Experiment `copies`: fewer userspace copies on the cold-read path

> **Status:** Diagnostic prototype on branch `exp/copies` (base `4db7d765c6`);
> not a product contract. Every change is behind `LAYERFS_PROBE_COPY`, off by
> default. One sample per arm; times are diagnostic, copy counts are exact.
> Receipts: [`../receipts/exp-copies/`](../receipts/exp-copies/).

## Hypothesis

Each delivered byte is copied several times between SQLite and the kernel.
Shared immutable buffers in the object cache, moving instead of copying on
insert, and emitting into the reply buffer would remove 5–10% of the cold
read.

## Counting first

A new process-wide counter (`layerfs-telemetry` `timer::copies`) records the
count and bytes of each copy by site. The probe prints it per cold read. Per
delivered byte (500 MiB), control arms:

| Site | What is copied | 128 KiB / 1, async | 512 KiB / 4, single |
| --- | --- | ---: | ---: |
| `pack` | SQLite pager into the pack body `Vec` (blob `read_at_exact`) | 1.014 | 1.113 |
| `frame` | stored chunk record into its own `Vec` (`decode_payload`, `frame.to_vec()`) | 1.000 | 1.000 |
| `canonical` | payload into the canonical object (`encode_chunk_object`) | 1.001 | 1.001 |
| `insert` | object cache insert (`Cache::insert`, `to_vec`) | 1.003 | 1.003 |
| `get` | object cache hit (`Cache::get`, `clone`), whole chunks for partial use | 1.242 | 1.293 |
| `page` | mapping pages in `Wave::pages` (insert clone, `to_vec` out); counted from `c82fabca` | 0.167 | 0.167 |
| `emit` | planned range into the reply `Vec` (`Wave::flush`) | 1.000 | 1.000 |
| `join` | split READ window appended to its one reply (`windows`) | — | 1.000 |
| `reply` | reply into `/dev/fuse` (`writev`, kernel) | 1.000 | 1.000 |
| **Total** | | **7.43** | **8.58** |

So the brief's six copies are really seven at 128 KiB (the Store makes two:
`frame` then `canonical`) and eight at 512 KiB (the prototype split READ adds
`join`). Not counted: SQLite's own `pread` into its pager (kernel), since the
enforced profile keeps `mmap_size=0`.

## Change

Commits `680aac3541` (binary `9c0b1f53…`) and `c1c368981b` (binary
`c82fabca…`, adds `page`). `LAYERFS_PROBE_COPY` takes a comma list or `all`:

- **`frame`** (`layerfs-storage` `encoding/decode.rs`): a stored chunk record
  is encoded straight from the pack body, with the same length and base
  checks, without the intermediate frame `Vec`.
- **`move`** (`layerfs-workspace` `base/client.rs`): a read-ahead fetch moves
  its objects into the cache. The allocation is trimmed to its length first
  (`shrink_to_fit`; a no-op on every decode path inspected).
- **`share`**: the object cache holds `Arc<Vec<u8>>`. A new
  `AuthenticatedObjects::read_shared_batch_scoped` (default: wrap each owned
  value, no copy) lets `Wave::flush` and the read's `PageCache` hold the
  cached buffers instead of clones. A fetched miss is cached and returned as
  one buffer.
- **`join`** (`layerfs-fuse` `request/reply.rs`, `layerfs-mount-ports`,
  `KernelWindow::finish_into`): each window of a split READ is emitted
  straight into the one reply buffer. A failed window's partial bytes are
  truncated, as a failed window added none before.

Removed copies: `frame` 1.0, `insert` 1.0, `get` 1.24–1.29 (to 0.002–0.05),
`page` 0.17, `join` 1.0 at 512 KiB. **7.43 → 4.02 copies per byte at 128 KiB
/ 1, 8.58 → 4.17 at 512 KiB / 4.** What remains is `pack`, `canonical`,
`emit` and the kernel reply.

## Arms

Container settings: `LAYERFS_PROBE_READ=async,noverify` at 128 KiB / 1, and
`single,noverify` with `LAYERFS_PROBE_FUSE_WINDOW=524288
LAYERFS_PROBE_FUSE_BACKGROUND=4` at 512 KiB / 4, except batch 2 (below).
Every arm exited 0 and read the 500 MiB back with the correct fold. Objects
demanded: 27,214 in every arm. Stage times in ms.

| Arm | Binary | Profile, read switches | `COPY` | MB/s | vs batch control | Store demand | `pack_read` | `client_demand` | BLAKE3 µs/call | Copies/byte | `VmHWM` KB |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `b1-c128` | `9c0b1f53` | 128K/1 async,noverify | — | 378.6 | — | 1,291 | 632 | 940 | 13.4 | 7.26 | 105,580 |
| `b1-x128-all` | `9c0b1f53` | 128K/1 async,noverify | all | 400.6 | +5.8% | 1,269 | 660 | 845 | 13.4 | 4.10 | 99,768 |
| `b1-x128-frame` | `9c0b1f53` | 128K/1 async,noverify | frame | 375.6 | −0.8% | 1,319 | 690 | 831 | 13.5 | 6.26 | 104,288 |
| `b1-x128-move` | `9c0b1f53` | 128K/1 async,noverify | move | 389.9 | +3.0% | 1,300 | 650 | 891 | 13.4 | 6.26 | 107,564 |
| `b1-x128-share` | `9c0b1f53` | 128K/1 async,noverify | share | 384.1 | +1.5% | 1,282 | 643 | 908 | 13.3 | 6.10 | 104,884 |
| `b2-c512` † | `9c0b1f53` | 128K/1 none | — | 243.5 | — | 1,243 | 618 | 84 | 13.3 | 7.26 | 117,000 |
| `b2-x512-join` † | `9c0b1f53` | 128K/1 none | join (no-op) | 244.1 | +0.2% | 1,240 | 621 | 79 | 13.4 | 7.26 | 114,600 |
| `b2-x512-nojoin` † | `9c0b1f53` | 128K/1 none | frame,move,share | 261.0 | +7.2% | 1,193 | 618 | 41 | 13.0 | 4.10 | 106,512 |
| `b2-x512-all` † | `9c0b1f53` | 128K/1 none | all | 251.8 | +3.4% | 1,243 | 643 | 42 | 13.1 | 4.10 | 107,244 |
| `b3-c512` | `9c0b1f53` | 512K/4 single,noverify | — | 384.8 | — | 2,019 | 653 | 249 | 13.3 | 8.41 | 98,696 |
| `b3-x512-all` | `9c0b1f53` | 512K/4 single,noverify | all | 412.4 | +7.2% | 2,012 | 626 | 207 | 13.2 | 4.25 | 97,232 |
| `b3-x512-join` | `9c0b1f53` | 512K/4 single,noverify | join | 389.4 | +1.2% | 1,961 | 634 | 280 | 13.4 | 7.40 | 106,092 |
| `b3-x512-fms` | `9c0b1f53` | 512K/4 single,noverify | frame,move,share | 412.8 | +7.3% | 2,004 | 625 | 210 | 13.2 | 5.25 | 97,620 |
| `b4-c128` ‡ | `c82fabca` | 128K/1 async,noverify | — | 249.0 | — | 2,007 | 1,245 | 1,460 | 13.7 | 7.43 | 186,132 |
| `b4-x128-all` | `c82fabca` | 128K/1 async,noverify | all | 396.7 | (‡) | 1,282 | 669 | 839 | 13.3 | 4.02 | 99,804 |
| `b4-c512` | `c82fabca` | 512K/4 single,noverify | — | 389.6 | — | 2,015 | 635 | 253 | 13.4 | 8.58 | 98,608 |
| `b4-x512-all` | `c82fabca` | 512K/4 single,noverify | all | 406.3 | +4.3% | 2,015 | 639 | 227 | 13.2 | 4.17 | 100,596 |
| `b5-c128` | `c82fabca` | 128K/1 async,noverify | — | 364.2 | — | 1,344 | 680 | 975 | 13.3 | 7.43 | 105,584 |
| `b5-x128-all` | `c82fabca` | 128K/1 async,noverify | all | 400.0 | +9.8% | 1,271 | 659 | 846 | 13.4 | 4.02 | 99,580 |

† Batch 2 was meant to be 512 KiB / 4. A shell quoting error passed the
window, background and read switches as one malformed variable, so the
probe ran at 128 KiB / 1 with no read switches (setup lines show window
131072, background 1; BLAKE3 54,429 calls). `LAYERFS_PROBE_COPY` was passed
separately and applied. The names are kept; the settings column is what ran.
`join` does nothing at 128 KiB, so `b2-x512-join` is in effect a second
control (0.2% from `b2-c512`) and `b2-x512-all` repeats `b2-x512-nojoin`
(3.6% apart). That spread is this setup's noise floor.

‡ `b4-c128` is a disturbed sample: SQLite pack reads took 1,245 ms against
618–690 ms in every other arm, with BLAKE3 unchanged (CPU speed normal) and
guest load 4.8 at start. It is kept and not used for a speed comparison.
Batch 5 repeats that comparison with its own control. Rerunning a control
can only lower the reported gain, not raise it.

## Accounting and memory

- **Cache accounting is unchanged.** Every arm ends with the same charge
  (67,100,168 bytes, 3,431 objects, 23,787 evictions at 128 KiB;
  67,096,003 / 3,430 / 23,788 at 512 KiB). The charge is still the object
  length plus 256.
- Under `share`, an evicted entry stays alive while a READ still holds its
  buffer: at most one wave (≤ 32 chunks of ≤ 32 KiB) per READ in flight. The
  charge does not show that transient memory.
- Under `move`, the cache holds the Store's own allocation, trimmed to its
  length.
- **`VmHWM`** is equal or lower with the switches on: 105.6 → 99.6–99.8 MB
  at 128 KiB, 98.6–98.7 → 97.2–100.6 MB at 512 KiB.

## What worked, what didn't

- Within batch, all switches together gained **+5.8%, +7.2%, +4.3% and
  +9.8%** (`b1`, `b3`, `b4` 512, `b5`), and +7.2% / +3.4% with no read
  switches (`b2`). That is within the hypothesis (5–10%) but close to the
  noise floor of about 3.6%. The copy counts are exact.
- **`move` and `share` carry the gain.** Single switches at 128 KiB (`b1`):
  move +3.0%, share +1.5%, frame −0.8%. At 512 KiB, frame+move+share alone
  equals all (+7.3% against +7.2%); join alone gave +1.2%.
- `client_demand` time, where the `get` copy happens, falls by about 95–130
  ms per 500 MiB (940 → 845, 975 → 846). Store demand barely moves
  (1,291 → 1,269, 1,344 → 1,271): at 128 KiB / 1 the read session is still
  busy about 97% of wall time with SQLite reads, pack SHA-256 and BLAKE3.
  Removing `frame` from it is too small to see in one sample.
- Each removed copy of 500 MiB saved only about 20–40 ms of wall time
  (`b1`: 76 ms for 3.2 copies; `b3`: 92 ms for 4.2; `b5`: 128 ms for 3.4).
  Seven or eight copies were never most of the time, so removing three or
  four of them gives a single-digit gain.

## Risks

- **`share` changes a trait.** It adds a default method to
  `AuthenticatedObjects` and makes the cache value `Arc<Vec<u8>>`. Product
  code would choose one path, not both.
- Memory held by a READ past eviction is not charged (see above).
- **`join`** threads a `&mut Vec<u8>` through `KernelData::read_into` and
  `KernelWindow::finish_into`. It belongs to the prototype split READ only.
- **Tests run (host, macOS, `cargo +1.85.1 --offline --locked`, 120 s alarm
  each):**
  - `layerfs-content`, `layerfs-storage`: 622 passed.
  - `layerfs-workspace`, `layerfs-mount-ports`: 182 passed.
  - All four with `LAYERFS_PROBE_COPY=all`: 804 passed.
- **Not run:**
  - Clippy: unavailable in the image.
  - Linux-only crate tests: `layerfs-fuse`, `layerfs-daemon`. The probe's
    fold check is their end-to-end coverage.

## Recommendation

**Adopt `move` and `share` together in product form**: one `Arc<Vec<u8>>` cache
value and a shared demand for the read path. They are the cheap part (+5–7%
within batch at both profiles) and leave cache accounting unchanged.

`frame` is a one-line Store fix with no measurable effect alone. Take it
when that code is touched anyway. If the split READ is adopted, `join`
belongs in its design.

The next copies cannot be switched off as easily:

- `canonical`: the cache would hold a view of the pack body.
- `emit`: needs a scatter reply, which fuser's public `ReplyData` does not
  offer and fuser may not be patched for it.
- SQLite's pager copy: `mmap_size` is fixed by the enforced profile.

None is worth more than about 3% while the single Store read session is the
limit.
