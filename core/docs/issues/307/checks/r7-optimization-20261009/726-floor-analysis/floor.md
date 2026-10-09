# R7 floor: what the cell commands cost through a trivial passthrough, and with no FUSE, in this environment

> **Status:** DIAGNOSTIC floor probes, 2026-10-09. One sample per configuration, no best-of, every output kept.
> These are **not** registered benchmark rows and carry no verifier, residency proof or admission eligibility.
> No tracked file was edited; the product was not built or run. Product numbers are read from receipts 652–703.

## 1. Headline

1. **About 45 ms of every registered "command" time is command launch, not filesystem work.** The registered span is
   taken on the macOS host from before Docker `exec create` to after `exec inspect`. With the same launch, `true`
   costs 34.8–53.7 ms (4 samples); over all 64 probe runs the span minus the time bash itself measured around the
   body is 33.6 / 45.1 / 55.1 ms (min / median / max). The A2 column was timed **inside** the container and does
   not contain this term.
2. **One synchronous FUSE request costs 39–40 µs here today** when the daemon's work is negligible (100,000
   negative lookups through the passthrough: 39.4 µs with one receive loop, 40.2 µs with two). A pipe ping-pong
   between two processes costs 35.9 µs per round trip, i.e. **about 18 µs per cross-process wake**. Of the 39.4 µs,
   10.1 µs is client CPU, 11.9 µs daemon CPU and 17.4 µs is time in which neither runs; the same access costs
   0.26 µs natively.
3. **C01 through the passthrough is 285.5 ms on the registered clock** with the product's own request stream
   (5,001 requests, one receive loop), and 356.3 / 378.6 ms when FLUSH is answered as A2 and the registered P
   binary answer it (7,000 requests, one / two loops). **L is 379.7 ms: 94 ms above the floor.** A2's 183.4 ms is
   below this environment's passthrough floor on either clock.
4. **L's "outside the owner" time is the floor.** L C01: (379.7 − 164.6) / 5,001 = 43.0 µs per request outside the
   owner. Passthrough P1E, same clock and stream: (285.5 − 71.0 daemon CPU) / 5,001 = 42.9 µs per request outside
   the daemon. The whole L − P difference (94 ms) is the size of the difference between L's owner time (164.6 ms)
   and the passthrough's entire daemon CPU (71.0 ms). The passthrough's 14.2 µs of daemon CPU per request is
   itself mostly the cost of being woken (11.9 µs in the zero-work probe), which L pays outside its owner time;
   counted that way L's outside-owner cost is at or below the passthrough's, i.e. no excess is measurable with
   one sample per arm taken hours apart.
5. **On the registered clock the A2 target is at or below the passthrough floor for 7 of 12 cells** (C01, C03,
   C06, C07, C08, C11, C12). After removing launch (A2's own clock) it is below the floor for C01 (clearly) and
   C12 (by 4 %, inside the sample spread) and inside the passthrough's range for C03; for the large-file cells
   C06–C11 the passthrough is 20–86 ms faster than A2's record, whose method dropped all caches before each cell.
6. **A second receive loop does not help a serial client** (P2 vs P1: C01 378.6 vs 356.3, C03 501.1 vs 494.0,
   S01 40.2 vs 39.4 µs per request). It raises daemon sleeps from 0.8–0.9 to 1.0 per request.
7. **Wake cost does not rise with how long the peer was idle** (H5 refuted up to 80 µs): round trip minus
   responder work is 35.9, 37.8, 38.3, 35.6, 38.2, 38.1 µs at 0, 5, 10, 20, 40, 80 µs of work.

## 2. What the registered "command" span contains

Read from `core/benchmark/r7-runtime/src/runtime.rs` (`command`) and `session.rs`, and from
`core/crates/layerfs-sandbox/src/backend/docker/{request,streams,http}.rs`:

- The clock is the **host's** monotonic clock (`Instant` in the macOS `layerfs-r7-runtime` process), relative to
  the runtime's own origin (`clock_domain: sdk-runtime-monotonic-relative-origin`).
- `begun` is taken just before the Docker Engine API `POST /containers/{id}/exec`
  (`User "501:20"`, `Env`, `WorkingDir`, `Cmd ["/bin/bash","-o","pipefail","-c",BODY]`, attach stdout/stderr).
  The span ends after `POST /exec/{id}/start` (attached, HTTP 101), the copy of the stream to EOF, and
  `GET /exec/{id}/json`. Each request uses its own connection to the Docker socket.
- So the span = exec create + exec start + (runc exec → bash start → body → exit → stream EOF) + exec inspect,
  plus the publication of two event rows. It is **not** an in-container time and nothing in the receipt records
  one. In receipts 659–703 the three API parts are 2.3–3.2, 1.4–2.0 and 1.7–2.5 ms; mine are 1.7–2.9,
  1.1–2.2 and 1.5–3.5 ms (same shape). The larger part sits inside `streams` and is invisible there.
- Class B in the registered runner: the identical body once on a throwaway Workspace, unmount, wait for cleanup
  Gone, fresh mount, measured command. uid:gid 501:20, the twelve environment variables of `workloads.ENV`.

## 3. Probe envelope

- Kernel `6.12.76-linuxkit`, 8 CPUs, Docker Desktop VM (3.9 GiB), no pinning, no scheduler change, no
  drop_caches. Pinned image `sha256:378b799e…2cd6`, `--network none`.
- One fresh container and one fresh named volume per arm (`layerfs-r7-floor-<n>-<cell>-<arm>`), created like the
  Sandbox's container: user 0:0, `CAP_SYS_ADMIN`, `/dev/fuse`, `no-new-privileges`, `apparmor=unconfined`, the
  volume at `/layerfs-store` (ext4 of the VM, `/dev/vda1`). PID 1 is `sleep infinity` instead of the daemon.
- Command launched and timed exactly as section 2 (my own client of the Docker socket on the host,
  `time.monotonic_ns`), as 501:20, same environment, working directory = the fresh directory.
- **N**: body in `/layerfs-store/floor/run` (ext4 volume). **P**: body in `/workspaces/1`, a passthrough mount
  over `/layerfs-store/floor/run`, the passthrough started as root by `docker exec` as the registered P arm does.
- Class-B mirror: the exact registered body once in a throwaway directory / throwaway passthrough mount, one
  plain unmount, then a fresh directory / fresh mount for the measured run.
- The measured body is wrapped: `R7S=$EPOCHREALTIME; BODY; R7E=$EPOCHREALTIME; echo …; times` (bash builtins, no
  fork). "Inside-body ms" is R7E − R7S. "Client CPU" is bash `times` (shell + waited children) minus the same
  value of the `true` run of that arm (launch share).
- Before and after the measured command, as root and outside the span: per-thread `comm`, `schedstat`, `stat`,
  context switches and `io` of the passthrough, `/proc/<pid>/io`, `cpu.stat` of the container's cgroup, `uname`,
  `nproc`, loadavg, meminfo, the mount table row.
- **Requests** = sum over receive threads of `syscr` deltas minus READ callbacks (each `/dev/fuse` read returns
  one request; each READ callback does one `pread`). It matches the passthrough's drain counts once the one or
  two requests `umount` itself causes (STATFS, and GETATTR of a changed root) are removed. It includes the one
  GETATTR of the root that the launch's `chdir` causes. `/sys/fs/fuse/connections` is empty in the container
  (fusectl not mounted) and would not hold request counts.

### The passthrough and its kernel profile

- The registered binary (`core/target/r7-passthrough-release/release/layerfs-r7-passthrough`, sha256
  `bb700118…62cc`) supports **only two receive loops**: `n_threads = Some(2)` is hard-coded and readiness requires
  `configured = created = entered = 2`. It answers **FLUSH with success every time** (as A2 did); the product
  answers ENOSYS once and the kernel then stops sending FLUSH.
- I built a variant from a copy of the same source under `core/target/floor/pt/` (sha256 `a4de03a7…6a87`, built in
  the pinned image with `--locked --offline --release`, repository ARM flags inherited). The only changes
  (`726-floor-analysis/pt-variant.diff`): `R7_FLOOR_LOOPS=1|2` (default 2) and `R7_FLOOR_FLUSH=ok|enosys`
  (default ok), and the patch path to the vendored fuser. Arms: **P1** one loop / FLUSH ok, **P2** two loops /
  FLUSH ok (the registered binary's behaviour), **P1E** one loop / FLUSH ENOSYS (the product's request stream).
  The registered binary itself was run once on C01 (**P2R**): 355.2 ms, 7,000 requests, 1.00 sleeps per request.
- Negotiated profile, identical in every P run and equal to `core/crates/layerfs-fuse/src/mount/profile.rs` and
  `mount/syscalls.rs`: ABI 7.41, selected `0x400021` = ASYNC_READ | BIG_WRITES | MAX_PAGES, max_write 131072,
  max_readahead 131072, max_background 1, congestion_threshold 1, 1 ns granularity, 60 s entry/attr TTL,
  FOPEN_KEEP_CACHE, no writeback. Mount row: `rw,nosuid,nodev,noatime - fuse layerfs-r7-passthrough
  rw,user_id=0,group_id=0,default_permissions,allow_other,max_read=131072`.

## 4. Fixed launch cost

| | min | median | max | n |
| --- | ---: | ---: | ---: | ---: |
| exec create ms | 1.66 | 2.35 | 2.91 | 64 |
| exec start ms | 1.13 | 1.59 | 2.20 | 64 |
| stream minus inside-body ms (runc exec, bash start, exit, EOF) | 28.4 | 39.5 | 48.1 | 64 |
| exec inspect ms | 1.47 | 1.93 | 3.52 | 64 |
| **command span minus inside-body ms** | **33.6** | **45.1** | **55.1** | 64 |

`true` itself: N 53.7, P1 51.3, P2 47.2, P1E 34.8 ms span; 0.02 ms inside; 0 FUSE requests in the body (1 GETATTR
from the launch's `chdir`). The launch also costs about 20 ms of CPU in the container's cgroup (runc init runs
there). L's receipts do not record this term for L; nothing here measures it inside L's container.

## 5. The price of one wake and of one FUSE round trip

Pipe ping-pong, two processes, unpinned, 100,000 round trips after 1,000 untimed (`724-floor-PIPE`):

| Responder work µs (measured) | Round trip µs | Round trip − work µs | Parent CPU µs | Child CPU µs | Sleeps per round trip (each side) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 35.90 | 35.90 | 9.21 | 9.18 | 1.00 |
| 5.05 | 42.84 | 37.79 | 9.44 | 14.62 | 1.00 |
| 10.09 | 48.37 | 38.28 | 9.52 | 19.88 | 1.00 |
| 20.11 | 55.73 | 35.61 | 9.15 | 29.35 | 1.00 |
| 40.12 | 78.33 | 38.21 | 9.47 | 49.87 | 1.00 |
| 80.21 | 118.30 | 38.09 | 9.59 | 90.06 | 1.00 |

- **One wake = 17.9 µs of wall** (35.9 / 2): about 9.2 µs of CPU charged to the woken side for going to sleep and
  coming back, and about 8.7 µs in which neither side runs.
- The two sides did not settle on one CPU (last CPUs 6 and 5; each side changed CPU 10–13 times in 100,000).
- The series is flat: wake cost does not depend on how long the peer slept, up to 80 µs.

100,000 negative lookups (`723-floor-S01`), one synchronous LOOKUP each:

| Arm | µs per lookup | Client CPU µs | Daemon CPU µs | Neither running µs | Daemon sleeps per request | Daemon run-queue wait per sleep |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 0.26 | 0.26 | — | — | — | — |
| P1 | 39.4 | 10.1 | 11.9 | 17.4 | 1.00 | 0.05 µs |
| P2 | 40.2 | 10.2 | 12.2 | 17.8 | 1.00 | 0.03 µs |

A FUSE round trip with no real work is a pipe round trip plus 3.5 µs. `schedstat`'s run-queue wait is 0.02–0.05 µs
per sleep in every P run: the kernel does not book the wake of an idle CPU as run-queue wait, so that field
cannot show wake latency (see H1).

## 6. Main table (milliseconds on the registered clock unless stated)

| Cell | N | P 1 loop | P 2 loops | P 1 loop, product's FLUSH (P1E) | L (receipt) | A2 (#305, in-container) | L − P1E | L owner wait+service | P1E daemon CPU | P1E inside-body | P2 inside-body | A2 vs floor, registered clock | A2 vs floor, in-container clock |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| C01 | 62.0 | 356.3 | 378.6 | 285.5 | 379.7 | 183.4 | 94.2 | 164.6 | 71.0 | 243.2 | 326.3 | below | below (243.2–326.3) |
| C02 | 551.0 | 897.0 | 935.0 | 844.1 | 914.8 | 965.0 | 70.6 | 177.7 | 86.3 | 797.2 | 888.1 | above | above (797.2–888.1) |
| C03 | 72.1 | 494.0 | 501.1 | 406.6 | 715.9 | 410.8 | 309.3 | 359.7 | 117.9 | 366.5 | 454.1 | at | within (366.5–454.1) |
| C04 | 463.0 | 899.5 | 891.7 | 819.0 | 823.4 | 952.6 | 4.4 | 174.1 | 97.7 | 773.0 | 846.7 | above | above (773.0–854.2) |
| C05 | 474.4 | 974.8 | 931.6 | 854.2 | 880.5 | 949.6 | 26.3 | 245.3 | 107.8 | 813.7 | 888.1 | above | within (813.7–928.7) |
| C06 | 65.9 | 100.3 | 100.5 | 101.8 | 226.8 | 78.6 | 125.0 | 148.2 | 24.5 | 55.0 | 59.1 | below | above (55.0–59.1) |
| C07 | 81.0 | 173.2 | 184.8 | 174.7 | 499.4 | 171.6 | 324.7 | 320.9 | 53.4 | 129.0 | 131.6 | at | above (128.9–131.6) |
| C08 | 82.6 | 111.4 | 95.1 | 114.4 | 261.8 | 106.1 | 147.4 | 151.9 | 25.2 | 65.2 | 61.6 | below | above (61.6–67.0) |
| C09 | 46.0 | 90.4 | 86.2 | 82.6 | 167.1 | 106.2 | 84.4 | 82.1 | 22.1 | 41.2 | 43.2 | above | above (56.3–58.7 cold) |
| C10 | 66.9 | 134.9 | 127.9 | 137.1 | 400.2 | 178.2 | 263.1 | 317.7 | 49.5 | 93.7 | 85.5 | above | above (92.7–107.6 cold) |
| C11 | 62.5 | 97.3 | 87.1 | 87.2 | 242.4 | 85.8 | 155.2 | 147.5 | 17.4 | 47.4 | 48.5 | at | above (47.4–48.5) |
| C12 | 57.3 | 265.2 | 249.2 | 244.7 | 317.7 | 189.3 | 73.0 | 139.0 | 50.6 | 197.7 | 205.0 | below | below (197.7–218.6) |

- "L − P1E" compares two single samples taken hours apart; the same passthrough run repeated on a second mount
  in the same container (warm-up against measured) differed by 3 % in the median and by up to 34 % (C08 P2), with
  several at 10–18 %. Read differences under about 10 % as nothing. C04's 4.4 ms is that: L's owner time alone is
  174 ms.
- "A2 vs floor, registered clock" asks whether L could reach the A2 number as the ledger compares it (L's host
  span against A2's in-container Exec). "In-container clock" compares A2 with the passthrough's inside-body range
  over P1, P2, P1E (cold variants for C09 and C10, since A2 read `big` from storage).

## 7. Per cell

Columns: command ms is the registered span; inside-body ms is bash's own clock; requests are kernel requests in
the measured window; daemon CPU is all passthrough threads; sleeps are voluntary context switches of the receive
threads; "idle" = (inside-body − client CPU − daemon CPU) / requests, the time in which neither ran (it is
meaningless where client processes overlap, i.e. the fork-heavy cells C02, C04, C05 and the two-loop readahead
cases, and can go negative). L's rows carry what the receipt has; L's per-thread CPU and sleeps are not recorded.
A2's requests are its contract's predicted counts.

#### TRUE  (710-floor-TRUE)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 53.7 | 0.0 | 53.7 | — | — | — | — | — | 0.0 | — |
| P1 | 51.3 | 0.0 | 51.3 | 1 | 25.0 | 0.0 | 45.5 | 1.00 | 0.0 | -20.5 |
| P1E | 34.8 | 0.0 | 34.8 | 1 | 22.9 | 0.0 | 46.6 | 1.00 | 0.0 | -23.7 |
| P2 | 47.2 | 0.0 | 47.2 | 1 | 26.0 | 0.1 | 58.1 | 1.00 | 0.0 | -32.1 |

#### C01  (711-floor-C01)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 62.0 | 15.5 | 46.6 | — | — | — | — | — | 16.0 | — |
| P1 | 356.3 | 307.1 | 49.1 | 7000 | 43.9 | 97.3 | 13.9 | 0.86 | 77.0 | 19.0 |
| P1E | 285.5 | 243.2 | 42.3 | 5001 | 48.6 | 71.0 | 14.2 | 0.80 | 54.0 | 23.6 |
| P2 | 378.6 | 326.3 | 52.3 | 7000 | 46.6 | 109.3 | 15.6 | 1.00 | 84.0 | 19.0 |
| L (659) | 379.7 | not recorded (streams 372.8) | not recorded | 5001 | 74.5 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 183.4 (in-container Exec) | not in its span | 7001 (predicted) | 26.2 | — | — | — | — | — |

#### C02  (712-floor-C02)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 551.0 | 507.7 | 43.2 | — | — | — | — | — | 532.0 | — |
| P1 | 897.0 | 850.8 | 46.3 | 8002 | 106.3 | 108.2 | 13.5 | 0.88 | 604.0 | 17.3 |
| P1E | 844.1 | 797.2 | 47.0 | 6003 | 132.8 | 86.3 | 14.4 | 0.83 | 591.0 | 20.0 |
| P2 | 935.0 | 888.1 | 46.9 | 8002 | 111.0 | 118.8 | 14.8 | 1.00 | 630.0 | 17.4 |
| L (663) | 914.8 | not recorded (streams 907.8) | not recorded | 6002 | 151.2 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 965.0 (in-container Exec) | not in its span | 8001 (predicted) | 120.6 | — | — | — | — | — |

#### C03  (713-floor-C03)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 72.1 | 20.9 | 51.2 | — | — | — | — | — | 22.0 | — |
| P1 | 494.0 | 448.7 | 45.3 | 11012 | 40.8 | 141.5 | 12.8 | 0.82 | 107.0 | 18.2 |
| P1E | 406.6 | 366.5 | 40.1 | 9013 | 40.7 | 117.9 | 13.1 | 0.78 | 86.0 | 18.0 |
| P2 | 501.1 | 454.1 | 47.0 | 11012 | 41.2 | 160.7 | 14.6 | 1.02 | 112.0 | 16.5 |
| L (667) | 715.9 | not recorded (streams 710.0) | not recorded | 9021 | 78.7 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 410.8 (in-container Exec) | not in its span | 10011 (predicted) | 41.0 | — | — | — | — | — |

#### C04  (714-floor-C04)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 463.0 | 423.4 | 39.6 | — | — | — | — | — | 454.0 | — |
| P1 | 899.5 | 854.2 | 45.3 | 7341 | 116.4 | 123.4 | 16.8 | 1.00 | 570.0 | 21.9 |
| P1E | 819.0 | 773.0 | 46.0 | 5342 | 144.7 | 97.7 | 18.3 | 0.98 | 540.0 | 25.3 |
| P2 | 891.7 | 846.7 | 45.0 | 7341 | 115.3 | 124.9 | 17.0 | 1.00 | 552.0 | 23.1 |
| L (671) | 823.4 | not recorded (streams 815.2) | not recorded | 5342 | 152.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 952.6 (in-container Exec) | not in its span | 7321 (predicted) | 130.1 | — | — | — | — | — |

#### C05  (715-floor-C05)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 474.4 | 435.1 | 39.3 | — | — | — | — | — | 470.0 | — |
| P1 | 974.8 | 928.7 | 46.1 | 7888 | 117.7 | 134.2 | 17.0 | 0.98 | 615.0 | 22.8 |
| P1E | 854.2 | 813.7 | 40.4 | 5889 | 138.2 | 107.8 | 18.3 | 0.98 | 549.0 | 26.6 |
| P2 | 931.6 | 888.1 | 43.5 | 7888 | 112.6 | 133.9 | 17.0 | 1.00 | 563.0 | 24.2 |
| L (675) | 880.5 | not recorded (streams 873.9) | not recorded | 5889 | 148.4 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 949.6 (in-container Exec) | not in its span | 7644 (predicted) | 124.2 | — | — | — | — | — |

#### C06  (716-floor-C06)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 65.9 | 17.3 | 48.6 | — | — | — | — | — | 18.0 | — |
| P1 | 100.3 | 55.2 | 45.1 | 518 | 106.5 | 24.0 | 46.4 | 1.00 | 22.0 | 17.6 |
| P1E | 101.8 | 55.0 | 46.8 | 517 | 106.4 | 24.5 | 47.4 | 1.00 | 22.0 | 16.4 |
| P2 | 100.5 | 59.1 | 41.4 | 518 | 114.1 | 26.6 | 51.4 | 1.00 | 23.0 | 18.3 |
| L (679) | 226.8 | not recorded (streams 219.9) | not recorded | 517 | 425.4 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 78.6 (in-container Exec) | not in its span | 519 (predicted) | 151.4 | — | — | — | — | — |

#### C07  (717-floor-C07)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 81.0 | 33.9 | 47.1 | — | — | — | — | — | 35.0 | — |
| P1 | 173.2 | 128.9 | 44.3 | 1553 | 83.0 | 55.5 | 35.7 | 1.00 | 47.0 | 17.0 |
| P1E | 174.7 | 129.0 | 45.7 | 1550 | 83.2 | 53.4 | 34.4 | 1.00 | 46.0 | 19.1 |
| P2 | 184.8 | 131.6 | 53.2 | 1553 | 84.8 | 54.7 | 35.2 | 1.00 | 47.0 | 19.3 |
| L (683) | 499.4 | not recorded (streams 492.5) | not recorded | 1550 | 317.7 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 171.6 (in-container Exec) | not in its span | 1553 (predicted) | 110.5 | — | — | — | — | — |

#### C08  (718-floor-C08)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 82.6 | 27.6 | 55.0 | — | — | — | — | — | 29.0 | — |
| P1 | 111.4 | 67.0 | 44.4 | 523 | 128.1 | 26.7 | 51.0 | 0.98 | 31.0 | 17.9 |
| P1E | 114.4 | 65.2 | 49.2 | 521 | 125.2 | 25.2 | 48.3 | 0.99 | 32.0 | 15.4 |
| P2 | 95.1 | 61.6 | 33.6 | 523 | 117.8 | 24.0 | 46.0 | 1.00 | 26.0 | 22.1 |
| L (687) | 261.8 | not recorded (streams 254.5) | not recorded | 521 | 488.5 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 106.1 (in-container Exec) | not in its span | 523 (predicted) | 202.9 | — | — | — | — | — |

#### C09  (719-floor-C09)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 46.0 | 6.6 | 39.4 | — | — | — | — | — | 6.0 | — |
| Nc | 67.8 | 23.0 | 44.8 | — | — | — | — | — | 14.0 | — |
| P1 | 90.4 | 46.6 | 43.8 | 517 | 90.1 | 26.1 | 50.4 | 1.00 | 19.0 | 2.9 |
| P1E | 82.6 | 41.2 | 41.4 | 517 | 79.8 | 22.1 | 42.8 | 0.99 | 17.0 | 4.1 |
| P1Ec | 101.6 | 58.7 | 42.8 | 517 | 113.6 | 32.9 | 63.7 | 1.00 | 25.0 | 1.5 |
| P1c | 100.2 | 56.3 | 43.9 | 517 | 108.8 | 30.7 | 59.4 | 1.00 | 25.0 | 1.1 |
| P2 | 86.2 | 43.2 | 43.0 | 517 | 83.7 | 22.9 | 44.4 | 1.00 | 18.0 | 4.5 |
| P2c | 93.0 | 58.5 | 34.5 | 517 | 113.1 | 32.9 | 63.7 | 1.00 | 24.0 | 3.1 |
| L (691) | 167.1 | not recorded (streams 160.6) | not recorded | 517 | 310.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 106.2 (in-container Exec) | not in its span | 517 (predicted) | 205.4 | — | — | — | — | — |

#### C10  (720-floor-C10)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 66.9 | 16.8 | 50.1 | — | — | — | — | — | 18.0 | — |
| Nc | 80.3 | 27.2 | 53.1 | — | — | — | — | — | 22.0 | — |
| P1 | 134.9 | 89.7 | 45.1 | 1547 | 58.0 | 46.4 | 30.0 | 0.67 | 32.0 | 7.3 |
| P1E | 137.1 | 93.7 | 43.4 | 1546 | 60.6 | 49.5 | 32.0 | 0.67 | 34.0 | 6.6 |
| P1Ec | 154.1 | 107.6 | 46.5 | 1546 | 69.6 | 57.9 | 37.5 | 0.67 | 39.0 | 6.9 |
| P1c | 140.2 | 98.5 | 41.7 | 1547 | 63.7 | 53.3 | 34.5 | 0.67 | 35.0 | 6.6 |
| P2 | 127.9 | 85.5 | 42.4 | 1547 | 55.3 | 54.1 | 35.0 | 1.00 | 33.0 | -1.0 |
| P2c | 135.7 | 92.7 | 43.0 | 1547 | 59.9 | 58.6 | 37.9 | 1.00 | 39.0 | -3.2 |
| L (695) | 400.2 | not recorded (streams 393.7) | not recorded | 1546 | 254.6 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 178.2 (in-container Exec) | not in its span | 1547 (predicted) | 115.2 | — | — | — | — | — |

#### C11  (721-floor-C11)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 62.5 | 12.4 | 50.2 | — | — | — | — | — | 13.0 | — |
| P1 | 97.3 | 47.6 | 49.7 | 518 | 91.8 | 16.5 | 31.9 | 1.00 | 23.0 | 15.5 |
| P1E | 87.2 | 47.4 | 39.8 | 517 | 91.7 | 17.4 | 33.6 | 1.00 | 21.0 | 17.5 |
| P2 | 87.1 | 48.5 | 38.6 | 518 | 93.6 | 17.8 | 34.3 | 1.00 | 21.0 | 18.8 |
| L (699) | 242.4 | not recorded (streams 235.9) | not recorded | 517 | 456.2 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 85.8 (in-container Exec) | not in its span | 518 (predicted) | 165.6 | — | — | — | — | — |

#### C12  (722-floor-C12)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 57.3 | 14.6 | 42.7 | — | — | — | — | — | 16.0 | — |
| P1 | 265.2 | 218.6 | 46.7 | 3994 | 54.7 | 59.0 | 14.8 | 0.89 | 55.0 | 26.2 |
| P1E | 244.7 | 197.7 | 47.0 | 3435 | 57.6 | 50.6 | 14.7 | 0.86 | 46.0 | 29.4 |
| P2 | 249.2 | 205.0 | 44.2 | 3692 | 55.5 | 56.2 | 15.2 | 1.00 | 51.0 | 26.5 |
| L (703) | 317.7 | not recorded (streams 311.8) | not recorded | 3436 | 90.8 (streams) | not recorded | — | not recorded | — | — |
| A2 (#305) | — | 189.3 (in-container Exec) | not in its span | 3976 (predicted) | 47.6 | — | — | — | — | — |

#### S01  (723-floor-S01)

| Arm | command ms (host span) | inside-body ms | fixed launch ms | requests | µs/request (inside) | daemon CPU ms | daemon CPU µs/req | vol. switches/req | client CPU ms | idle µs/req |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| N | 65.1 | 27.3 | 37.8 | — | — | — | — | — | 28.0 | — |
| P1 | 4028.8 | 3977.7 | 51.1 | 101001 | 39.4 | 1201.5 | 11.9 | 1.00 | 1020.0 | 17.4 |
| P2 | 4119.8 | 4064.7 | 55.1 | 101001 | 40.2 | 1231.3 | 12.2 | 1.00 | 1036.0 | 17.8 |

`C01-registered-binary` (`725`): P2R 355.2 ms span, 312.8 inside, 7,000 requests, 44.7 µs per request, daemon CPU
14.3 µs per request, 1.00 sleeps per request.

### The floor, cell by cell

- **C01.** P = 285.5 ms on the registered clock with the product's request stream (P1E: 243.2 inside the container + 42.3 launch; 5001 requests), 356.3 / 378.6 ms with FLUSH answered on one / two loops. So L cannot be below about 285.5 ms plus its own work; L is 379.7 = P1E + 94.2 (L owner wait+service 164.6 ms; P1E's whole daemon CPU 71.0 ms). The A2 target 183.4 ms is **below** this environment's passthrough floor on the registered clock, and **below** the passthrough's in-container range (243.2–326.3 ms) on A2's own clock.
- **C02.** P = 844.1 ms on the registered clock with the product's request stream (P1E: 797.2 inside the container + 47.0 launch; 6003 requests), 897.0 / 935.0 ms with FLUSH answered on one / two loops. So L cannot be below about 844.1 ms plus its own work; L is 914.8 = P1E + 70.6 (L owner wait+service 177.7 ms; P1E's whole daemon CPU 86.3 ms). The A2 target 965.0 ms is **above** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (797.2–888.1 ms) on A2's own clock.
- **C03.** P = 406.6 ms on the registered clock with the product's request stream (P1E: 366.5 inside the container + 40.1 launch; 9013 requests), 494.0 / 501.1 ms with FLUSH answered on one / two loops. So L cannot be below about 406.6 ms plus its own work; L is 715.9 = P1E + 309.3 (L owner wait+service 359.7 ms; P1E's whole daemon CPU 117.9 ms). The A2 target 410.8 ms is **at** this environment's passthrough floor on the registered clock, and **within** the passthrough's in-container range (366.5–454.1 ms) on A2's own clock.
- **C04.** P = 819.0 ms on the registered clock with the product's request stream (P1E: 773.0 inside the container + 46.0 launch; 5342 requests), 899.5 / 891.7 ms with FLUSH answered on one / two loops. So L cannot be below about 819.0 ms plus its own work; L is 823.4 = P1E + 4.4 (L owner wait+service 174.1 ms; P1E's whole daemon CPU 97.7 ms). The A2 target 952.6 ms is **above** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (773.0–854.2 ms) on A2's own clock.
- **C05.** P = 854.2 ms on the registered clock with the product's request stream (P1E: 813.7 inside the container + 40.4 launch; 5889 requests), 974.8 / 931.6 ms with FLUSH answered on one / two loops. So L cannot be below about 854.2 ms plus its own work; L is 880.5 = P1E + 26.3 (L owner wait+service 245.3 ms; P1E's whole daemon CPU 107.8 ms). The A2 target 949.6 ms is **above** this environment's passthrough floor on the registered clock, and **above, by 2 %,** the passthrough's in-container range (813.7–928.7 ms) on A2's own clock.
- **C06.** P = 101.8 ms on the registered clock with the product's request stream (P1E: 55.0 inside the container + 46.8 launch; 517 requests), 100.3 / 100.5 ms with FLUSH answered on one / two loops. So L cannot be below about 101.8 ms plus its own work; L is 226.8 = P1E + 125.0 (L owner wait+service 148.2 ms; P1E's whole daemon CPU 24.5 ms). The A2 target 78.6 ms is **below** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (55.0–59.1 ms) on A2's own clock.
- **C07.** P = 174.7 ms on the registered clock with the product's request stream (P1E: 129.0 inside the container + 45.7 launch; 1550 requests), 173.2 / 184.8 ms with FLUSH answered on one / two loops. So L cannot be below about 174.7 ms plus its own work; L is 499.4 = P1E + 324.7 (L owner wait+service 320.9 ms; P1E's whole daemon CPU 53.4 ms). The A2 target 171.6 ms is **at** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (128.9–131.6 ms) on A2's own clock.
- **C08.** P = 114.4 ms on the registered clock with the product's request stream (P1E: 65.2 inside the container + 49.2 launch; 521 requests), 111.4 / 95.1 ms with FLUSH answered on one / two loops. So L cannot be below about 114.4 ms plus its own work; L is 261.8 = P1E + 147.4 (L owner wait+service 151.9 ms; P1E's whole daemon CPU 25.2 ms). The A2 target 106.1 ms is **below** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (61.6–67.0 ms) on A2's own clock.
- **C09.** P = 82.6 ms on the registered clock with the product's request stream (P1E: 41.2 inside the container + 41.4 launch; 517 requests), 90.4 / 86.2 ms with FLUSH answered on one / two loops. With `big` evicted from the page cache: P1Ec 101.6, P1c 100.2, P2c 93.0, Nc 67.8 ms. So L cannot be below about 82.6 ms plus its own work; L is 167.1 = P1E + 84.4 (L owner wait+service 82.1 ms; P1E's whole daemon CPU 22.1 ms). The A2 target 106.2 ms is **above** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (56.3–58.7 ms, cold) on A2's own clock.
- **C10.** P = 137.1 ms on the registered clock with the product's request stream (P1E: 93.7 inside the container + 43.4 launch; 1546 requests), 134.9 / 127.9 ms with FLUSH answered on one / two loops. With `big` evicted from the page cache: P1Ec 154.1, P1c 140.2, P2c 135.7, Nc 80.3 ms. So L cannot be below about 137.1 ms plus its own work; L is 400.2 = P1E + 263.1 (L owner wait+service 317.7 ms; P1E's whole daemon CPU 49.5 ms). The A2 target 178.2 ms is **above** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (92.7–107.6 ms, cold) on A2's own clock.
- **C11.** P = 87.2 ms on the registered clock with the product's request stream (P1E: 47.4 inside the container + 39.8 launch; 517 requests), 97.3 / 87.1 ms with FLUSH answered on one / two loops. So L cannot be below about 87.2 ms plus its own work; L is 242.4 = P1E + 155.2 (L owner wait+service 147.5 ms; P1E's whole daemon CPU 17.4 ms). The A2 target 85.8 ms is **at** this environment's passthrough floor on the registered clock, and **above** the passthrough's in-container range (47.4–48.5 ms) on A2's own clock.
- **C12.** P = 244.7 ms on the registered clock with the product's request stream (P1E: 197.7 inside the container + 47.0 launch; 3435 requests), 265.2 / 249.2 ms with FLUSH answered on one / two loops. So L cannot be below about 244.7 ms plus its own work; L is 317.7 = P1E + 73.0 (L owner wait+service 139.0 ms; P1E's whole daemon CPU 50.6 ms). The A2 target 189.3 ms is **below** this environment's passthrough floor on the registered clock, and **below** the passthrough's in-container range (197.7–218.6 ms) on A2's own clock.

What the cells have in common:

- **Metadata cells (C01, C03, C12): the floor is round trips.** 41–58 µs per request inside the body, of which the
  passthrough's CPU is 13–16 µs (11.9 µs of that is the price of being woken and going back to sleep, from S01)
  and 16–29 µs is nobody running. L is 73–94 ms above P1E for C01 and C12 and 309 ms above for C03.
- **Fork-heavy cells (C02, C04, C05): the floor is process starts plus round trips.** N alone is 463–551 ms; the
  passthrough adds 290–380 ms; L is within 71 ms of P1E in all three, and within noise in C04 and C05.
- **Large-file cells (C06–C11): the floor is small and L is far above it.** 512 WRITE requests of 128 KiB cost
  the passthrough 47 µs of CPU each (C06: 24 ms in total; one copy and one `pwrite` into the page cache) and the
  whole body 55 ms; L's owner service for the same 512 requests is 148 ms. L − P1E is 84–325 ms here and equals
  L's owner time within 5 % in C07, C08, C09 and C11 and within 16 % in C06 (C10: 263 against 318). No floor argument protects these cells.
- **C09 / C10 (reads of a pre-existing file)** are the only cells where the floor on the registered clock is
  below the A2 number: 82.6 / 137.1 ms warm and 101.6 / 154.1 ms with `big` evicted, against 106.2 / 178.2.

## 8. Hypotheses H1–H7 of `per-request-floor-outside-owner-fdc24ef3f.md`

Everything below is measured on the **passthrough** and the pipe. No per-thread observation of the product
daemon exists (I did not run the product), so the L-specific predictions stay open; `runner.py` and `runtime.rs`
are being edited by another agent in this checkout, apparently to add that observation.

| # | Hypothesis | Settled? | Evidence |
| --- | --- | --- | --- |
| H1 | Outside time is wake latency, not daemon CPU | **Settled for the environment; L's own split open.** | A request with no real work costs 39.4 µs: 10.1 client CPU + 11.9 daemon CPU + 17.4 idle. The CPU on both sides is itself the sleep/wake path (native lookup 0.26 µs). L's 43.0 µs per request outside the owner equals P1E's 42.9 µs outside its daemon. The predicted observable "run-queue wait ÷ timeslices ≈ 15–25 µs" **cannot appear**: it is 0.02–0.05 µs in P although the idle term is 17 µs, so read `fuser-0` on-CPU time and wall, not field 2. H1's other branch (L's `fuser-0` on-CPU ≥ 300 ms) needs L's per-thread sample. |
| H2 | RELEASE precedes GETATTR on the critical path | **Confirmed as kernel behaviour; L's count open.** | P1E C01: 4,012 sleeps for 5,001 requests (4.01 per file): one read per file finds a request already queued. P1 with FLUSH: 6,021 for 7,000. C03 P1E 7,010 for 9,013; C12 P1E 2,969 for 3,435. With two loops the saving disappears (1.00 per request). The prediction for L's `fuser-0` is therefore about 4,000 in C01. |
| H3 | No hidden per-request wake (L's other threads) | **Not settled**: L-only. | In P only the receive threads run; the main and session threads show 0 CPU, 0 switches in every run. |
| H4 | A2's 183 ms comes from a cheaper-wake regime | **Supported.** | Same profile, two loops, FLUSH answered, in-container clock: 326.3 ms (P2) and 312.8 ms (registered binary) against A2's 183.4. "Flat 21 µs wakes predict about 300 ms": measured 307–326. Per request 44–47 µs here against A2's 26. A2's record also had N at 31.8 ms where N is 15.5 ms here, so its machine state differed in both directions. Switch split of the two threads: 3,508 / 3,497; run-queue wait per sleep 0.05 µs. |
| H5 | Wake latency rises with idle time | **Refuted up to 80 µs.** | Section 5: 35.6–38.3 µs at every responder work from 0 to 80 µs. |
| H6 | Allocator or page-fault churn inflates the thread | **Refuted for P; L open.** | Passthrough minor faults in the whole measured window: 67 in C01 (0.01 per request), 1 in S01, 33 in the read cells. L's `minflt` is not recorded. |
| H7 | Client and daemon share no CPU | **Supported for P and the pipe; L open.** | Nonvoluntary switches of the receive threads: 0–4 per run (4 in 101,001 requests). S01: client last CPU 0, daemon last CPU 2; the client changed CPU 15 times in 100,000 lookups. Pipe: last CPUs 6 and 5. |

Consequences for the proposals of that note:

- P3–P7 (trimming 3–5 µs of user-space fixed cost per request) act on a term this probe could not see as
  different from zero: L's outside-owner cost per request already equals the passthrough's.
- P1 (take RELEASE's close off the next request's path) acts on a real pattern (H2), but the pattern is one
  avoided sleep per file in L's favour as well; its saving is the close's service time, not a wake.
- The lever the floor shows is **fewer synchronous requests** (each costs about 39 µs before any work), and for
  the large-file cells **owner service per WRITE/READ** (288 µs against the passthrough's 47 µs of CPU).

## 9. Differences of envelope

Between this probe and the registered L rows (652–703):

- **Time of day.** L's samples were taken earlier on 2026-10-09; these at about 09:10–09:20 container time. The
  environment's wake cost is state-dependent; one sample each.
- **Launch.** Same API sequence and clock, but my client is Python and the container's PID 1 is `sleep`, not the
  daemon. L's own launch share is not recorded.
- **Wrapper.** The measured body carries two `$EPOCHREALTIME` reads, one `echo` and `times`. The warm-up used the
  exact registered body; it differs from the measured run by 3 % in the median.
- **Request stream.** P1E's request total equals L's in C01 and C04–C11, and is 6,003 vs 6,002 in C02, 9,013 vs
  9,021 in C03 (9 READDIR replies vs 17) and 3,435 vs 3,436 in C12 (10 READDIR vs 11). Per opcode the only other
  differences are outside the window (the unmount's own STATFS and GETATTR) or uncounted by the passthrough
  (the one COPY_FILE_RANGE of `cp` in C07 and C10, which fuser's default handler answers; it is in the total).
  C12 depends on git's timing: 3,435 (P1E), 3,994 (P1), 3,692 (P2, 100 fewer OPEN/RELEASE/FLUSH), A2 3,976.
- **Work per request.** The passthrough does real ext4 work (`openat`, `pwrite` into the page cache, an extra
  `O_PATH` open, `fstat` and `fchownat` per create because the daemon is root and the command is 501). Nothing is
  made durable; ext4 writeback of the 64 MiB happens after the command.
- **Cache state.** Tool binaries and the image are warm (class B mirror). C06–C08 and C11 write into the page
  cache. C09 / C10 read `big` from the page cache in the plain arms (16,384 of 16,384 pages resident, recorded)
  and from storage in the `c` arms (`fsync` of that file + `posix_fadvise(DONTNEED)` on it, 0 pages resident
  afterwards, recorded). L reads `big` from its Store, cold in neither sense.
- **Preparation** of `big` (C09–C11) is run natively in the backing directory before any mount; in L it is part
  of the committed base.
- **No verifier.** Only exit code 0 and an entry count / `du` of the backing directory were checked.

Between this probe and A2 (#305 `cf-fsbench-v2`):

- A2's Exec was timed **inside the container** by a Python runner (`subprocess` to pidfd readiness): it contains
  bash start and exit but no Docker launch. Compare it with "inside-body ms", not with the registered span.
- A2 ran `sync` + VM `drop_caches=3` before every cell: cold tool binaries, and storage reads in C09–C11. Its N
  column is 31.8–46.7 ms where N is 6.6–33.9 ms here (inside), and 721–827 ms on the fork-heavy cells where N is
  423–508 ms here.
- A2 used uid 1000, no warm-up, two receive loops, FLUSH answered, a different passthrough binary
  (`core/experiment/real-tree`), and ran on 2026-10-04/05.

## 10. Commands and outputs

Every Docker or cargo command ran under `R4_LOCK_WAIT=900 perl core/target/r4-locked.pl <limit> …` (limit 120 s
for probes, 560 s for the one build, 30–60 s for two read-only checks).

```
# environment facts (no output directory; printed only)
… r4-locked.pl 60 docker run --rm --name layerfs-r7-floor-env --network none --device /dev/fuse --cap-add SYS_ADMIN \
    --security-opt apparmor=unconfined sha256:378b799e…2cd6 /bin/bash -c 'uname -a; nproc; …'
# variant build (benchmark code, own Cargo.toml, target under core/target/floor/pt-target)
… r4-locked.pl 560 docker run --rm --name layerfs-r7-floor-build --network none \
    -e CARGO_HOME=/work/core/target/cluster2-linux-cargo -e CARGO_TARGET_DIR=/work/core/target/floor/pt-target \
    -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs:/work -w /work sha256:378b799e…2cd6 \
    cargo build --locked --offline --release --manifest-path core/target/floor/pt/Cargo.toml
# probes: one invocation = one cell = one output directory; each arm in its own fresh container and volume
… r4-locked.pl 120 python3 -B core/target/floor/probe.py --number 710 --cell TRUE --arms N,P1,P2,P1E
… --number 711 --cell C01   … --number 712 --cell C02   … 713 C03   714 C04   715 C05   716 C06   717 C07   718 C08
… --number 719 --cell C09 --arms N,P1,P2,P1E,Nc,P1c,P2c,P1Ec        … --number 720 --cell C10 (same eight arms)
… --number 721 --cell C11   … --number 722 --cell C12
… --number 723 --cell S01 --arms N,P1,P2                              # 100,000 negative lookups
… --number 724 --cell PIPE --name PIPE                                # pipe ping-pong, six responder works
… --number 725 --cell C01 --name C01-registered-binary --arms P2R     # the registered P binary
# reduction (no Docker)
python3 -B core/target/floor/analyze.py <scratch>/floor/floor.json <scratch>/floor/tables.md
```

Outputs, all under `core/docs/issues/307/checks/r7-optimization-20261009/` (untracked, append-only):
`710-floor-TRUE`, `711-floor-C01` … `722-floor-C12`, `723-floor-S01`, `724-floor-PIPE`,
`725-floor-C01-registered-binary`, `726-floor-analysis` (reduced `floor.json`, the tables, the variant diff and
the sha256 of every script and binary). Each arm directory holds `result.json` (spans, streams, exit codes,
passthrough events with negotiated profile and drain counts), `snapshot-before/after.stdout`, the passthrough's
own stdout/stderr, setup, residency, sanity and `container.inspect.json`. All 16 runs completed; no arm failed;
every container and volume I created was removed (checked) and nothing else was touched.

Scripts (git-ignored): `core/target/floor/probe.py`, `analyze.py`, `stage/{pp.c,nl.c,residency.py}`, the variant
source `core/target/floor/pt/` and its diff `core/target/floor/pt-variant.diff`.

## 11. Limitations

- One sample per configuration; identical-run spread 3 % median, up to 34 %. No statement here is a
  qualification, and "at the floor" means within that spread.
- The product was not run: L's launch share, per-thread CPU, sleeps, faults and CPUs are not measured. H1 (L's
  split), H3, H6 and H7 for L remain open.
- Client CPU comes from bash `times` at 1 ms resolution minus a `true` baseline, and overlaps in time with
  children in the fork-heavy cells; the idle term is reliable only for the serial cells and S01.
- The cgroup `cpu.stat` delta was recorded but carries about 20 ms of launch CPU per window; it is in
  `floor.json` (`client_cpu_cgroup_ms`) and is not used above.
- Request counts per opcode come from the passthrough's callback counters over the whole mount (they include
  the unmount's STATFS/GETATTR); the kernel offers no per-opcode count here. Totals come from per-thread `syscr`.
- The passthrough answers from ext4 and the page cache with no durability; it is a floor for kernel round trips
  and launch, not a model of any store.
- Other containers were running during the probes (the four named in the task, idle as far as `docker ps`
  shows); the checkout lock excludes other builds and measurements, not host activity outside Docker.
- A2's numbers are quoted from its report; its environment on that day cannot be re-observed.
