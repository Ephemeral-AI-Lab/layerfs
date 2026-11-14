# Experiment `passthrough`: decoded-file cache and Linux FUSE passthrough

> **Status:** Prototype research on branch `exp/passthrough` (base
> `4db7d765c6`, issue #684). Not a product contract and not for merge.
> **The decoded-file cache keeps decoded committed content in plain files
> outside the one Store file. Current product rules forbid that** (one global
> database; no objects outside it). Everything here is behind switches that
> are off by default, and any adoption needs an explicit owner decision.

## Hypothesis

The per-tool-call re-read of an unchanged committed file is bound by the
Store read path (about 380 MB/s cold, about 540 MB/s with the Store in the
page cache). If the whole decoded file sits in a plain local file:

- a READ hit is one `pread` with no Store access and no hashing; and
- with kernel FUSE passthrough, the kernel reads that file itself, with no
  READ request at all, so the re-read approaches native disk speed.

## Step 1: does pinned fuser 0.18.0 expose passthrough? Yes, without patches

`engine/vendor/fuser-0.18.0` (unchanged) has everything needed:

- the init flag: `InitFlags::FUSE_PASSTHROUGH` (bit 37), requested with
  `KernelConfig::add_capabilities`. The INIT reply sends `FUSE_INIT_EXT` and
  `flags2`. `KernelConfig::set_max_stack_depth` sets `max_stack_depth`, which
  is sent only when passthrough was negotiated.
- backing ids: `ReplyOpen::open_backing(fd)` (`FUSE_DEV_IOC_BACKING_OPEN`)
  returns a `BackingId` whose `Drop` runs `FUSE_DEV_IOC_BACKING_CLOSE`.
- the reply: `ReplyOpen::opened_passthrough(fh, flags, &BackingId)`, which
  adds `FOPEN_PASSTHROUGH` and fills `fuse_open_out.backing_id`.

The guest kernel (6.12.76-linuxkit) has `CONFIG_FUSE_PASSTHROUGH=y`. The
probe container runs as root with `CAP_SYS_ADMIN`, which `BACKING_OPEN`
requires in the initial user namespace.

## Change (commits on `exp/passthrough`)

- `layerfs-mount-ports/src/decoded.rs` (new):
  - The cache. It is process-wide, switched on by
    `LAYERFS_PROBE_DECODED=<max bytes>`, and lives in
    `LAYERFS_PROBE_DECODED_DIR`. The probe sets that to `<work>/decoded`,
    under the daemon's work directory.
  - Keyed by content root, so one entry serves every Workspace whose
    unchanged base names that root.
  - Filled from the windows of the first full read through a mount. Those
    bytes have already been authenticated by the Store read path (pack
    SHA-256 and resolver BLAKE3). Windows may arrive in any order; each is
    written at its offset. The file is renamed to its root once every byte is
    covered.
  - Bounded by bytes: a file that does not fit is not cached. No eviction.
- `layerfs-mount-ports/src/operations/read.rs`, READ path:
  - After the owner visit, a window with no local row over a base regular
    file is served from the cached file when it is complete, before any Store
    admission.
  - On a miss, the window read from the Store is offered to the fill.
- `layerfs-fuse` passthrough:
  - `LAYERFS_PROBE_PASSTHROUGH=1` plus the cache: `profile.rs` requests
    `FUSE_PASSTHROUGH` with stack depth 1.
  - At a read-only OPEN, `reply.rs` makes one read-only visit, under the
    kernel's lookup reference, that names the file's content root when it has
    no local row.
  - `request/passthrough.rs` answers with the cached file as the backing
    file. It reuses one `BackingId` per inode and closes it at the last
    passthrough RELEASE. It counts this mount's opens per inode and uses
    passthrough only while no other open of the inode is live.
- Probe: three new phases.
  - `reread_cold`: a fresh mount after the cold read. The Store and every
    cached file are evicted first; residency is checked with `fincore`.
  - `reread_warm`: a fresh mount, no eviction.
  - `coherence` and `coherence_truncate`.

  Every re-read is fold-checked against the source.

## Arms (final binary, batch 4: one sample each, same batch)

Binary `fuse_profile_probe` SHA-256
`6da185f6107b83ab5f29f9c4df7de6259018cec493d6a6b702d7c0d2f0eb6d8a`.
Clippy: `NOT_RUN` (not in the image). BLAKE3 was 13.2–13.4 µs per call in
every arm. Guest 1-minute load at arm start was 3.5–4.9. The cache limit is
1 GiB. Every read was checked against the source fold and was correct.

| Arm | Profile, `LAYERFS_PROBE_READ` | Native | Cold read (fills) | Fill ms | Re-read cold | Re-read warm | READs on re-read | OPEN µs on re-read |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `b4-control` | 128K/1, async,noverify | 2,797 | 373.6 | — | 383.2 | 543.4 | 4,000 | 14 |
| `b4-cache` | same + cache | 3,060 | 378.2 | 122 | **1,077.3** | **1,513.7** | 4,000 (all hits) | 15 |
| `b4-pt` | same + cache + passthrough | 3,358 | 367.5 | 133 | **2,822.9** | **2,784.5** | **0** | 52 |
| `b4-control-w512` | 512K/4, single,noverify | 3,267 | 376.2 | — | 316.7 | 440.5 | 4,000 | 18 |
| `b4-cache-w512` | same + cache | 2,977 | 362.9 | 201 | 1,500.6 | 1,908.4 | 4,000 (all hits) | 35 |
| `b4-pt-w512` | same + cache + passthrough | 3,191 | 385.6 | 130 | 2,625.5 | 3,173.9 | 0 | 49 |

Notes on the table:

- All speeds are MB/s; "Native" is the native read of the same file.
- The 512K re-read mounts keep `read_ahead_kb` at 128; only the cold-read
  mount raises it, so those re-reads issue 128 KiB READs.
- Cache residency before `reread_cold` was 0 bytes in every arm, and the
  Store's was 0. Before `reread_warm` the cached file was fully resident
  (524,288,000 bytes).
- Peak memory (`VmHWM`) on the cold re-read: control 107 MB, cache 106 MB,
  passthrough 106 MB.
- The other phases were unchanged within one-sample noise:
  - sequential write 624–640 MB/s at 128K and 730–765 MB/s at 512K
  - 4 KiB random write 71–76 MB/s
  - small create 210–219 µs per file
  - `ls -l` 0.12–0.15 s

## Findings

- **Passthrough re-read is at native speed:** 2,823 MB/s cold against
  383 MB/s for the control, 7.4×, and 0.84× this arm's native read. The
  kernel sent no READ at all; the drain receipt has no `read` opcode.
- **The cache alone gives about 2.8× on the cold re-read** (1,077 against
  383 MB/s) **and on the warm one** (1,514 against 543 MB/s). Each READ is a
  `pread` of about 24 µs (`hit_ns` / 4,000), with no Store objects and no
  BLAKE3. What remains is the FUSE round trip and the reply copy. With 4
  READs in flight the cache-only cold re-read reaches 1,501 MB/s.
- **Cost of the first read:**
  - Writing the fill takes 122–201 ms of serial time in the READ callbacks
    per 500 MiB, about 0.25–0.4 ms per MiB.
  - The first-read MB/s stayed within ±4% of its batch control (one sample,
    inside noise).
  - The 500 MiB of dirty pages are written back later, outside the
    measurement. The probe's eviction did `sync_all` as setup.
- **Disk:** 524,292,096 allocated bytes for one 524,288,000-byte file. That
  is a second copy beside the Store's 532.8 MB. The data is random and
  incompressible; for compressible or deduplicated content the decoded copy
  costs more than the Store does.
- **Cost on every open:** with passthrough on, each read-only regular-file
  OPEN pays one extra owner visit plus the backing ioctl: about 50 µs
  against 15 µs in these samples. Every open of an unchanged file pays it,
  cached or not.

## Coherence

| Check | Control | Cache only | Passthrough |
| --- | --- | --- | --- |
| Reader open, writer opens the same file in the same Workspace | ok | ok | **open fails with EIO** |
| Reader sees the writer's 4 KiB write | new bytes | new bytes | (write refused) |
| Fresh open after the write: written head / untouched tail | new / base | new / base (overlay path, not cache) | new / base (not passthrough) |
| Another Workspace on the same base | base | base (cache hit) | base (passthrough) |
| Truncate by path to 4 KiB while a reader is open: reader at 8 KiB | 0 bytes | 0 bytes | **4,096 stale base bytes** |
| Unmount after all of the above | ok | ok | ok |

- **Cache only is coherent.** The owner visit decides each READ atomically.
  Once a Workspace has a local row for the inode, the window takes the
  overlay path and the cache is never consulted. The cache is keyed by
  immutable content root, so it cannot go stale across Commits or Branches.
- **Passthrough is not POSIX-coherent with Workspace writes.** The kernel
  holds one I/O mode per inode:
  - A cached (non-passthrough) open of an inode open for passthrough fails
    with EIO, and the reverse is also refused. The daemon avoids the reverse
    by counting its opens.
  - Changes that need no open (truncate, and any SETATTR) reach the daemon,
    but an open passthrough descriptor keeps reading the backing file. It
    returns stale base bytes, even past the new end of file.
  - Not tested but implied: a known install that changes a file's base
    content leaves open passthrough descriptors on the old bytes.
- **The kernel refuses `FOPEN_KEEP_CACHE` with passthrough** (EIO at open).
  The first passthrough arm, `b1-pt`, failed this way (kept as FAIL). The
  adapter now drops `KEEP_CACHE` for passthrough opens.
- **Cleanup:** backing ids close at the last passthrough RELEASE, and every
  unmount succeeded. The cache directory outlives mounts by design. Nothing
  evicts or collects it. A file that is never read whole keeps its `.part`
  file and its reserved bytes.

## Risks

- Product rule: decoded committed content outside the Store, and a second
  copy of the data on disk.
- Trust:
  - A hit is served without hashing. Bytes are authenticated once, at fill.
    After that, corruption or tampering in the cache directory is served
    as-is.
  - With passthrough the daemon never sees the read.
- Passthrough needs `CAP_SYS_ADMIN` in the initial user namespace, a kernel
  of 6.9 or later with `CONFIG_FUSE_PASSTHROUGH`, and a backing filesystem
  that is not stacked: stack depth 1, so not overlayfs.
- Prototype gaps:
  - No eviction and no limit on partial fills.
  - The fill runs synchronously in the READ callback.
  - No crash recovery: the in-memory index is lost at daemon restart, so
    the files would be orphaned.

## Recommendation

- **The decision for the owner is whether decoded content may live outside
  the Store, as a bounded and disposable local cache.** Without that
  exception nothing here can ship.
- **If allowed, take the cache-only READ path first.** It keeps Workspace
  semantics: every coherence check matched the control. It gives about 2.8×
  on both cold and warm re-reads.
- **Passthrough reaches native speed but changes semantics:**
  - EIO for a writer while a reader is open
  - stale reads after a truncate or a base change
  - about 35 µs more per read-only open

  It would need an explicit semantic exception, for example only for paths
  declared read-only such as dependency trees, or a daemon refusal such as
  `ETXTBSY` for mutations of inodes with live passthrough opens. Do not
  enable it generally.

## Failures and invalid arms

- `b1-pt`: **FAIL**. The open returned EIO because `KEEP_CACHE` is outside
  the kernel's passthrough flag mask. Fixed in the next binary.
- `b3-control-w512` and `b3-cache-w512`: **INVALID** (operator error). The
  `-e` flags were passed as one unsplit zsh word, so the 512K/4 profile and
  `noverify` were not applied (receipts show `max_write` 131072). They were
  rerun as the `-w512b` arms.
- `b3-*-w512b`: valid, and they showed the first fill design (in order only)
  abandoning at 512K/4 (`fills_abandoned` 1, no hits). The fill was
  redesigned to accept any order. Batch 4 is the final binary.
- Not run:
  - Clippy (not in the image)
  - existing mount tests
  - concurrent-open stress
  - known-install-while-open
  - daemon restart

## Receipts

[`../receipts/exp-passthrough/`](../receipts/exp-passthrough/). The first
line of each log is its binary SHA-256:

- `b1-*`: `276fb2dd…`
- `b2-*`: `4f42115b…`
- `b3-*`: `6b603163…`
- `b4-*`: `6da185f6…`, the final binary.
