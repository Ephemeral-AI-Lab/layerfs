# V1 kernel-visibility probe: raw evidence

Status: Dated planning checkpoint; not release evidence or a product contract.

Date: 2026-09-15. Question: can a FUSE daemon obtain the bytes of a dirty
`MAP_SHARED` mapping without an explicit kernel writeback trigger, and if not,
which triggers exist and what do they cost?

This probe is deliberately independent of the LayerFS product: it is a single
dependency-free C program that serves one cached (write-through, no
`FUSE_WRITEBACK_CACHE`) FUSE session over `/dev/fuse`, mounts it with `mount(2)`
under `CAP_SYS_ADMIN`, mmaps a served file `MAP_SHARED`, stores bytes into the
mapping, and reports what the daemon observed.

## Reproduction

```bash
cd benchmark-results/v016/v1-probe
docker run --rm --device /dev/fuse --cap-add SYS_ADMIN -v "$PWD":/probe -w /probe \
  rust:1.85.1-bookworm sh -c 'gcc -O1 -pthread -o probe probe.c && ./probe'
```

Container kernel: `Linux 6.12.76-linuxkit` (Docker Desktop, x86_64 image on an
arm64 macOS host). Image: `rust:1.85.1-bookworm`. Source and binary hashes are in
`SHA256SUMS`; the raw console log is `run.log`.

## Result (verbatim from `run.log`)

```text
probe: mounted write-through cached FUSE session
probe: negotiated minor=31 flags=0x21 max_write=1048576
probe: after-map-store daemon_write_requests=0 daemon_bytes=0 daemon_saw_first_byte=0 observed_after_ms=-1
probe: after-syncfs daemon_write_requests=1 daemon_first_byte=65 observed_after_ms=0
probe: after-second-store daemon_saw_second_byte=0 observed_after_ms=-1
probe: per-file self-open pre-fsync daemon_write_requests=1 daemon_saw_third_byte=0
probe: per-file self-open post-fsync daemon_write_requests=2 daemon_third_byte=67 observed_after_ms=0 third_store_wait_ms=-1
probe: open-unlinked daemon_saw_fourth_byte=0 self_open_errno=2 store_wait_ms=-1
probe: open-unlinked after-syncfs daemon_fourth_byte=68 observed_after_ms=0
```

`flags=0x21` is `FUSE_ASYNC_READ | FUSE_BIG_WRITES`: `FUSE_WRITEBACK_CACHE` and
`FUSE_DIRECT_IO_ALLOW_MMAP` are *not* negotiated, matching the product's current
`init` (`crates/layerfs-fuse/src/filesystem.rs:37-47`). The kernel still accepted a
writable shared mapping on a cached handle.

| # | Observation | Raw field |
| --- | --- | --- |
| 1 | A store into a `MAP_SHARED` mapping is not delivered to the daemon; the daemon's own view of the file is unchanged | `daemon_write_requests=0`, `daemon_saw_first_byte=0`, waited 1000 ms |
| 2 | `syncfs()` on the mount root delivers the dirty mapped page | `daemon_first_byte=65` (`'A'`) within 10 ms, exactly one WRITE request |
| 3 | A later store is again invisible until the next writeback trigger | `daemon_saw_second_byte=0`, waited 500 ms |
| 4 | A per-file flush works: the daemon opened the file through its own mount and called `fsync`, so no mount-wide barrier is required | pre-`fsync` `daemon_saw_third_byte=0`; post-`fsync` `daemon_third_byte=67` (`'C'`) within 10 ms |
| 5 | The per-file flush cannot address an open-unlinked file: after `unlink`, the daemon's self-open fails | `self_open_errno=2` (`ENOENT`), `daemon_saw_fourth_byte=0` |
| 6 | `syncfs` does cover the open-unlinked inode | `daemon_fourth_byte=68` (`'D'`) within 10 ms |

## Kernel contract this follows from

- `mmap()` of a cached FUSE file sends **no** request to the daemon
  (`fs/fuse/file.c`, `fuse_file_mmap`): the daemon cannot even learn that a
  writable mapping exists.
- In write-through cached mode only `write(2)` is delivered synchronously
  ([FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html));
  mapped stores are written back later by the VM through `fuse_writepages`.
- The kernel exposes no "read the current page cache" facility. The only
  triggers are `fsync`/`msync`/`munmap`/memory pressure and filesystem-wide
  `syncfs`, so a daemon-side acquisition must force writeback to see those bytes.
- `FUSE_DIRECT_IO_ALLOW_MMAP` does not avoid this: the first shared mapping of a
  direct-I/O file *enters caching inode I/O mode* (`fuse_file_cached_io_open`),
  after which mapped stores are cached and written back lazily.

## Consequence for V1

A snapshot that reads only host-installed state omits bytes the filesystem
contract makes visible; forcing writeback at acquisition is the only way to
include them, and its work is proportional to dirty payload. The options and the
rule tension they create are recorded in
`docs/roadmap/0.1/0.1.6/overlay-snapshot-contract-resolution.md`.
