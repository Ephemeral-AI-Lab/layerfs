# FUSE request-size and in-flight tuning: where the cold read goes

> **Status:** Diagnostic research on the 2.0 line; not a product contract and
> not for merge. Measured 2026-11-14 on branch `issue-fuse-profile-tuning`
> from `origin/layerfs_v2_0_0` at `8cfb6e2734`. This is the priority-3 row
> "Request-size/in-flight tuning" of the FUSE optimization investigation
> (`core/docs/issues/303/fuse-optimization-investigation.md`, removed from the
> tree by #671; read it at `1c1fa4dbc0^`). Kernel write-back stayed off and
> fuser was not patched. Each arm is one sample; times are diagnostic (see the
> cache contract), counts are exact.

## Answer

- **The cold read is bound by the Store read path, not by the kernel
  profile.** At the current profile (128 KiB, one background request), 75% of
  wall time is a synchronous 8 MiB read-ahead that the READ request waits for,
  once every 64 requests. Authenticating the same bytes three times (pack
  SHA-256, then BLAKE3 in the Store resolver, then BLAKE3 again in the
  Workspace client) is 38% of wall time. The kernel round trip outside the
  callback is 11%. The reply copy into the kernel is 7.6%.
- **Larger requests alone do not help the cold read.** With one background
  request, 128 KiB, 512 KiB and 1 MiB requests read at 151.9, 147.4 and
  161.9 MB/s. READ frames fall 4,000 → 1,000 → 500. The round-trip time
  removed (380 → 181 ms) is about 6% of wall time.
- **More requests in flight raise the cold read about 32%, but double the
  work.** 512 KiB with 4 in flight reaches 205.0 MB/s. Concurrent windows miss
  the read-ahead that is still being fetched and demand the same objects
  again: 27,214 object demands become 53,608, and callback time rises from
  3.1 s to 4.7 s. At 128 KiB the kernel keeps one READ in flight anyway, so
  `max_background` changes little (151.9 / 160.2 / 172.2 MB/s).
- **Writes:** sequential writes may gain about 10–20% from larger requests,
  but one sample per arm cannot separate that from noise. 4 KiB random writes
  are unaffected (43–55 MB/s): each is one WRITE whatever the profile.
- **Recommendation:** keep the 128 KiB / 1 profile for now. Change the read
  path first (single-flight, asynchronous read-ahead; authenticate once). Then
  try 512 KiB with 4 in flight again. Of the measured arms it is the fastest
  for both cold read and sequential write, and costs 0.75 MiB more receive
  buffer per mount.
- **Small files:** a create costs 5 requests and `ls -l` costs one LOOKUP per
  entry. FLUSH is already sent only once per mount, because ENOSYS is sticky,
  so eliding it saves nothing. READDIRPLUS would cut an `ls -l` of 2,000
  entries from 2,022 requests to about 84 (estimate).

## Method

**Code.** Commit `41628498cf` is the exact source of the measured binary
(`e6b610b79a` only reformats it):

- `layerfs-telemetry` `timer::stages` adds process-wide counts and wall
  nanoseconds per stage.
- Timers sit at:
  - the READ owner visit
  - Store reader admission
  - window finish
  - `CanonicalClient::read_ahead`
  - `CanonicalClient` demands
  - the daemon read session's Store demand
  - SQLite locate and pack reads
  - pack SHA-256
  - zstd
  - BLAKE3 `ObjectId::for_bytes`
  - the kernel reply
- The profile comes from `LAYERFS_PROBE_FUSE_WINDOW` (`max_write`, the
  `max_read` mount option and the requested readahead) and
  `LAYERFS_PROBE_FUSE_BACKGROUND` (`max_background` and the congestion
  threshold, set equal). Unset, the profile is unchanged.
- A READ larger than the engine's 128 KiB read window is served as
  consecutive window reads and replied once. A WRITE larger than its 256 KiB
  write window becomes consecutive window jobs. `fuser` sizes `max_pages`
  from `max_write`.

**Kernel readahead cap.** The kernel offers 128 KiB as the largest INIT
readahead, and every receipt shows `max_readahead` 131072. For 512 KiB and
1 MiB the probe raises the mount's `/sys/class/bdi/0:N/read_ahead_kb` after
mounting. A product would need the daemon to do the same.

**Prototype limits.** These would need a product design before any merge:

- A split READ is several owner visits, so it can mix states under a
  concurrent write.
- A split WRITE publishes several windows before its one reply.

**Probe.** `engine/crates/layerfs-daemon/examples/fuse_profile_probe.rs`
serves the daemon in-process, as `mounted_sequential_read.rs` does.

- `prepare` runs Init once (`--setup clone` source, Disposable/WAL/OFF) on:
  - `rand`: 500 MiB of non-repeating xorshift64\* bytes
  - `small/`: 2,000 files of 1 KiB
  - Result: 2,003 entries, Store 532,844,544 bytes, 2.265 s.
- Each `run` arm copies that Store into a fresh directory in its own
  container. It then runs each phase on its own fresh mount:
  1. Native reference: the same 500 MiB from the volume's ext4.
  2. Cold read of `rand`, 1 MiB user reads, checked against a fold of the
     source.
  3. Sequential write of a new 256 MiB file in 1 MiB writes.
  4. 16,384 random aligned 4 KiB `pwrite`s into the committed `rand`.
  5. 2,000 creates of 1 KiB files in a new directory.
  6. `ls -l` of `small/`.
- Request counts and callback nanoseconds come from each mount's drain
  receipt (`OpcodeWork`). Peak memory is the process `VmHWM`, reset with
  `clear_refs` 5 at the start of each phase. The daemon's 64 MiB object cache
  is the product default.

### Identity and custody

| Field | Value |
| --- | --- |
| Source | `41628498cf` (parent `8cfb6e2734`, `origin/layerfs_v2_0_0`) |
| Binary | `fuse_profile_probe`, release, `--locked --offline -j 6`, SHA-256 `731ac61f30289bc5f157b6727d8a85fdc291b8f521e23e8b2216d70ccb7ea1c8` |
| Build image | `rust:1.85.1-bookworm` `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` (arm64). The registered image `sha256:378b799e…` was removed from the host by another session's cleanup during this work; the launcher called it as `cargo +1.85.1`. |
| Host | Docker Desktop VM: 8 CPUs, 8 GB memory, Linux 6.12.76-linuxkit aarch64, ext4 volume. Containers: `--device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined`, `/sys` remounted rw for the bdi write. `LAYERFS_CONSTRUCTION_WORKERS=1`. |
| Interference | Idle sibling containers (Sandboxes, PostgreSQL, MinIO). Guest 1-min load at arm start 0.91–2.32 (receipts). No other build or measurement ran in this worktree. |
| Commands | `fuse_profile_probe prepare /probe/fixture` once; then `fuse_profile_probe run /probe/fixture /probe/work-<arm>` once per arm under `timeout 115`; every arm exited 0. |
| Receipts | [`receipts/`](receipts/): `PROBE` JSON lines for every phase of every arm |

### Cache contract (declared and enforced)

| Phase | Declared state | Enforcement / evidence |
| --- | --- | --- |
| Native reference | Guest-cold file | `fsync` (setup), `POSIX_FADV_DONTNEED`; `fincore` 0 resident bytes before every reading |
| Cold read | Guest-cold Store; empty daemon object cache; new FUSE connection | Store copy evicted after mounting, `fincore` 0 resident before the read; new process per arm; all 27,214 objects were demanded from the Store |
| Writes, creates | No read of committed payload | New file / no Store payload reads needed |
| `ls -l` | New FUSE connection | Fresh mount; kernel entries empty |

The guest page cache is controlled. The macOS host's cache of the VM disk
image is not: the native reference read 1.51–2.39 GB/s, which suggests host
caching. Treat throughput as **diagnostic**, not a cold-disk claim.
LayerFS's cold read is CPU-bound by its own measured stages (below), so this
state does not explain its time.

## 1. Where the cold read goes (128 KiB, 1 in flight)

4,000 READs of 131,072 bytes; wall 3,451 ms (151.9 MB/s), 863 µs per READ.
The stages are wall time summed on the threads that ran them. Nested rows are
parts of the row above. BLAKE3 is split between its two call sites by count:
54,429 calls = 2 × 27,214 objects + 1, over the same bytes.

| Part | ms | µs / READ | % of wall |
| --- | ---: | ---: | ---: |
| Outside the callback: kernel round trip, dispatch, user copy-out | 380 | 95 | 11.0 |
| Owner visit (overlay job), 4,000 | 44.8 | 11.2 | 1.3 |
| Store reader admission, 4,000 | 0.6 | 0.2 | 0.0 |
| Synchronous read-ahead, 62 calls of ~8 MiB | 2,581 | 645 | 74.8 |
| — Store demand, 130 batches, 27,214 objects | 1,967 | | 57.0 |
| —— SQLite locate, 130 | 210.5 | | 6.1 |
| —— SQLite pack read, 2,295, without its SHA-256 | 735.4 | | 21.3 |
| —— Pack SHA-256, 2,261 | 294.8 | | 8.5 |
| —— zstd decode, 26 calls (random data is stored raw) | 0.9 | | 0.0 |
| —— BLAKE3 in the Store resolver, ≈27,214 | ≈506 | | ≈14.7 |
| —— Rest: resolve, group handling, copies | ≈220 | | ≈6.4 |
| — BLAKE3 again in `fetch_ahead`, ≈27,214 | ≈506 | | ≈14.7 |
| — Rest: mapping pages, cache inserts | ≈108 | | ≈3.1 |
| Planned range from the object cache, lookup + copy + emit | 151 | 38 | 4.4 |
| — of which `CanonicalClient` demands, 20,186 | ≤133 | | ≤3.9 |
| Reply copy into `/dev/fuse`, 4,000 | 263.7 | 65.9 | 7.6 |
| Other inside the callback | 31 | 8 | 0.9 |

The rows of the first level sum to 3,452 ms. The read-ahead window is
`READ_AHEAD` = 8 MiB in `base/client.rs`. With one request in flight, the
next READ cannot start while the read-ahead runs. One READ in 64 waits about
42 ms, and the other 63 are served from the cache in about 100 µs.

Authentication is SHA-256 of whole packs (`port/read_pack.rs`), BLAKE3 of
every object in the resolver (`delta/read.rs`), then BLAKE3 of the same
object in `CanonicalClient::fetch_ahead`. Together they take 1,307 ms (38%).
SQLite I/O (locate + blob reads) takes 946 ms (27%). Per byte, the copies are:

1. pack body
2. resolved object
3. cache insert
4. cache read
5. reply buffer
6. kernel

Overhead of the timers is ≈10⁵ calls × ~50 ns ≈ 5 ms (<0.2%). This is an
estimate: an uninstrumented arm was not run.

## 2. Request size and requests in flight

| Arm | Cold read MB/s | READs | Callback ms | Outside ms | Read-ahead calls | Store demands | Objects demanded | Seq write MB/s | WRITE frames | 4 KiB random MB/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 128 KiB / 1 (current) | 151.9 | 4,000 | 3,072 | 380 | 62 | 130 | 27,214 | 368.0 | 2,048 | 52.8 |
| 128 KiB / 4 | 160.2 | 4,000 | 3,356 | — | 74 | 209 | 27,663 | 384.1 | 2,048 | 55.2 |
| 128 KiB / 16 | 172.2 | 4,000 | 3,186 | — | 206 | 349 | 27,355 | 413.2 | 2,048 | 53.4 |
| 512 KiB / 1 | 147.4 | 1,000 | 3,284 | 273 | 62 | 130 | 27,214 | 422.3 | 512 | 43.4 |
| 512 KiB / 4 | **205.0** | 1,000 | 4,661 | — | 982 | 1,190 | 53,608 | **531.3** | 512 | 53.2 |
| 512 KiB / 16 | 199.3 | 1,000 | 4,831 | — | 988 | 1,197 | 53,661 | 459.2 | 512 | 54.8 |
| 1 MiB / 1 | 161.9 | 500 | 3,057 | 181 | 62 | 130 | 27,214 | 435.2 | 256 | 54.6 |
| 1 MiB / 4 | 168.4 | 500 | 5,773 | — | 293 | 468 | 51,095 | 368.6 | 256 | 51.2 |
| 1 MiB / 16 | 200.0 | 500 | 4,827 | — | 493 | 707 | 53,771 | 505.3 | 256 | 54.9 |

Notes on the table:

- **Callback** is the READ callbacks' summed wall time. **Outside** is cold
  read wall minus callback; it is only meaningful with one request in flight
  ("—" otherwise, because callbacks overlap).
- **Seq write and random write** come from the same arms; `max_background`
  does not apply to them (writes are synchronous without write-back).
- **Engine write jobs** for the sequential write: 2,048 at 128 KiB; 1,024 at
  512 KiB and 1 MiB (each kernel WRITE split into 256 KiB jobs).

How to read it:

- **Request size alone (one in flight):** cold read stays at 147–162 MB/s.
  Store work is unchanged (130 demands, 27,214 objects), and only the round
  trip shrinks (380 → 181 ms). Each larger READ is several engine window
  visits plus one more reassembly copy, which takes back part of the gain.
- **In flight > 1 at 512 KiB / 1 MiB:** two receive loops overlap, so wall
  time falls by about a quarter. The read-ahead stream in `Streams::follow`
  expects consecutive offsets, though. Out-of-order and concurrent windows
  start new streams and demand objects that another window is already
  fetching. That doubles the objects demanded and BLAKE3 calls (107,217), and
  callback time rises 50–90%. The gain is bought with duplicate Store work,
  not with less work.
- **In flight > 1 at 128 KiB:** the bdi window is one request, so the kernel
  rarely has a second READ to send. The read changes only slightly.
- **Writes:** at a fixed request size, `max_background` cannot change a
  synchronous write. The spread across the three background values therefore
  shows the noise of one sample: 12% at 128 KiB, 25% at 512 KiB, 37% at
  1 MiB. Means by size are 388 / 471 / 436 MB/s, a possible 10–20% gain that
  these samples cannot confirm. A 4 KiB random write is 77.5 µs at 128 KiB /
  1: 42.6 µs in the callback (one SQLite transaction) plus about 35 µs of
  round trip. No profile setting changes that.

### Memory cost

**Receive buffers (analytic).** fuser receive loops are sized
`max_write + 4096` (the authorized receive-buffer patch), with 2 loops per
mount:

| Request size | Per mount | Change | Four Workspaces |
| --- | ---: | ---: | ---: |
| 128 KiB | 270,336 B | — | 1.03 MiB |
| 512 KiB | 1,056,768 B | +0.75 MiB | 4.03 MiB |
| 1 MiB | 2,105,344 B | +1.75 MiB | 8.03 MiB |

The prototype's READ reassembly adds up to one window per READ in service
(≤2 × the request size, transient).

**Measured process peak (`VmHWM`, KB, rise from `VmRSS` at phase start).**

| Arm | Cold read | Seq write | Random write |
| --- | --- | --- | --- |
| 128 KiB / 1 | 14,148 → 117,304 | 150,444 → 152,748 | 135,336 → 135,356 |
| 512 KiB / 1 | 14,392 → 132,580 | 166,104 → 169,388 | 151,972 → 151,980 |
| 1 MiB / 1 | 14,412 → 168,148 | 202,344 → 206,548 | 189,092 → 189,116 |
| 512 KiB / 4 | 14,396 → 99,712 | 133,264 → 136,548 | 119,136 → 119,160 |
| 1 MiB / 16 | 14,392 → 102,168 | 135,316 → 140,676 | 124,156 → 124,164 |

The cold-read peak holds the 64 MiB object cache, two 8 MiB read-ahead
windows and pack caches. With one request in flight it grows with request
size (+15 MB at 512 KiB, +51 MB at 1 MiB), far more than the buffers above.
That is consistent with the glibc mmap threshold rising after the first freed
large reply buffer, the mechanism recorded for fuser buffers on 2026-10-14.
It is not proven: one sample, and the arms with more in flight peaked lower.
Every arm's figures are in the receipts.

## 3. Small-file creates and `ls -l`

Counts were identical in all nine arms. The figures below are from 128 KiB /
1.

**2,000 creates of 1 KiB:** 598 ms, 299 µs per file, 10,003 requests.

| Request per file | Count | Callback µs each |
| --- | ---: | ---: |
| LOOKUP (negative) | 2,001 | 16.7 |
| CREATE | 2,000 | 40.2 |
| WRITE | 2,000 | 43.0 |
| GETATTR | 2,001 | 32.3 |
| RELEASE | 2,000 | 21.0 |
| FLUSH | **1** | 6.1 |

That is about 153 µs in callbacks and about 29 µs of round trip per request.

- **FLUSH elision saves nothing measurable.** The adapter answers FLUSH with
  ENOSYS, which the kernel remembers per connection (`no_flush`). Only the
  first close of a mount sends FLUSH: 1 request, about 7 µs per mount.
- **The GETATTR per create is the larger target:** about 61 µs (32 µs
  callback plus round trip), 20% of a create. Its inode was not recorded. It
  is consistent with `default_permissions` re-validating the parent directory
  after each create invalidates its attributes, but this was not checked.

**`ls -l` of 2,000 committed entries on a fresh mount:** 197 ms, 2,022
requests.

| Request | Count | Callback ms |
| --- | ---: | ---: |
| LOOKUP | 2,001 | 69.8 (34.9 µs each) |
| READDIR | 17 | 18.9 |
| OPENDIR, GETATTR, GETXATTR (sticky ENOSYS), RELEASEDIR | 1 each | — |

86.1 ms is in callbacks and 111 ms outside them (kernel and `ls` itself).

**READDIRPLUS estimate (not implemented).**

- A `fuse_direntplus` for these names is about 160 bytes, so about 25 entries
  fit in each one-page reply.
- About 80 READDIRPLUS frames would replace 17 READDIRs and 2,001 LOOKUPs:
  about 84 requests instead of 2,022 (−96%).
- The attribute work stays: about 10–35 µs per entry, depending on whether a
  directory window can produce attributes in one owner job.
- Most of the 111 ms outside the callbacks is round trip. The estimate for
  this `ls -l` is about 60–110 ms instead of 197 ms.
- Costs: every READDIRPLUS entry takes a kernel lookup reference, as a LOOKUP
  does today, so FORGET disposal is unchanged. An adaptive mode
  (`FUSE_READDIRPLUS_AUTO`) avoids paying for attributes on plain `ls`.

## 4. Recommended profile and next steps

**Profile.** Keep 128 KiB requests, one background request and congestion 1.

- Larger requests with one in flight gave no measured read gain (147–162
  MB/s against 152) for +0.75 to +1.75 MiB of buffers per mount. They also
  need three things: adapter READ/WRITE splitting with a defined
  multi-window contract, a post-mount bdi `read_ahead_kb` write, and a larger
  `MAX_INPUT_BYTES`.
- More requests in flight gained up to 35% wall time by doubling Store reads
  and BLAKE3, which wastes CPU shared by every Workspace.

**Once read-ahead is single-flight and asynchronous**, re-run this probe at
512 KiB with 4 in flight: 205 MB/s cold read and 531 MB/s sequential write
were the best results here. These read-path changes would close more of the
gap, ranked by their share of the 3,451 ms:

1. **Read-ahead outside the request (75%).** Run it asynchronously behind
   the stream, and make an in-flight fetch single-flight, so concurrent
   windows wait for it instead of demanding the same objects again. With one
   thread, the Store path still caps the read at about 500 MiB / 2.58 s ≈
   200 MB/s. Overlap only pays once fetches also run in parallel or cost less.
2. **Authenticate once (≈15–38%).** The client re-hashes objects that the
   Store just authenticated in the same process (≈506 ms). Pack SHA-256 and
   object BLAKE3 cover the same bytes (295 ms). Keeping one check per trust
   boundary is a design decision for the owner.
3. **SQLite reads (27%) and copies.** There are six copies per byte; the
   reply copy alone is 7.6%.

**What the profile can and cannot close.** Cold read here is 152 MB/s, while
the native ext4 read of the same bytes on this VM is 1.5–2.4 GB/s (the paper
compared 943 MB/s for ovnfs on its host). The profile can affect only the
round trip and part of the reply cost: at most 19% of cold-read wall time,
and 6% in practice. The rest is the design of the Store read path. Writes
follow the same pattern. Sequential writes may gain 10–20% from larger
requests. A 4 KiB random write is dominated by one SQLite transaction per
write plus one round trip, which no profile setting changes.

## Checks and gaps

**Run:**

- Linux release build of the probe and every touched crate (`-p
  layerfs-daemon --example fuse_profile_probe`, `--locked --offline`).
- rustfmt on changed files (`e6b610b79a`).
- All nine arms and the prepare step, each once, each exited 0.

**Not run:**

- **Clippy:** `NOT_RUN`. The substitute image has no `clippy` component, and
  builds are offline.
- **Existing mount tests, such as `fp2_negotiation_flags`:** `NOT_RUN`. With
  the probe variables unset the profile is the selected one, but
  `MAX_INPUT_BYTES` now admits 1 MiB.
- **Uninstrumented baseline arm:** `NOT_RUN`; timer overhead is estimated
  above.
- **Durable:** `NOT_RUN`, disabled by the owner until explicit
  reauthorization.

**Limits of the evidence:**

- One sample per arm, so differences under about 15% in write MB/s are within
  observed spread.
- Throughput is diagnostic: the host cache of the VM disk is uncontrolled.
- The workload is random, so it is incompressible and zstd is idle.
  Compressible content would add decode time to the Store path.
