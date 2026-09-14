# V1 generic-mechanism probe: can a daemon get a chosen-instant copy of writable-mapped bytes?

Status: bounded experiment on the deployed kernel. **V1 remains open**, with one
kernel-contract fact added to the record (a daemon-initiated writeback drain does
hand the daemon *owned, coherent per-file* copies of mapped dirty bytes) and two
mechanisms now measured as non-satisfying (`FUSE_NOTIFY_RETRIEVE` atomicity; the
daemon's own cached read). No product source was touched; no benchmark or #122
case was executed; nothing outside this directory was modified.

Environment identity only (not an approved dependency): Docker Desktop on macOS
arm64, container kernel `6.12.76-linuxkit`, image `rust:1.85.1-bookworm`
(`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`), GCC
12.2.0, negotiated protocol **minor 31** with flags `0x21`
(`FUSE_ASYNC_READ|FUSE_BIG_WRITES`) and `0x10021` (+`FUSE_WRITEBACK_CACHE`),
`max_write=1048576`, `max_pages` 32 (default).

## The question

Can a generic, in-tree-supported FUSE implementation obtain, at a chosen instant,
a copy of the bytes that writable shared mappings of a file currently expose to a
reader — such that the copy is stable (later mapped writes do not change it) —
without a global freeze?

## Probe design (reproducible)

One C program, no libfuse, no product code: it serves a cached FUSE session over
`/dev/fuse`, mounts it with `mount(2)` under `CAP_SYS_ADMIN`, `mmap`s served files
`MAP_SHARED|PROT_WRITE`, and measures. Sources:
[probe.c](probe.c) + [probe_experiments.h](probe_experiments.h) (T1–T6),
[followup.c](followup.c) (T8 control, T7, T9; it `#include`s `probe.c`).

**Concurrency oracle.** One writer thread performs single aligned 8-byte stores
separated by a full barrier (`__sync_synchronize()`, i.e. `DMB` on aarch64), one
round value per round, page 0 first and page N−1 last (ascending). Therefore at
*every* instant `value(page p) >= value(page q)` for all `p < q` (one round
completes before the next begins). A collected sample with `v[p] < v[q]` for some
`p < q` mixes two instants and is not the file's content at any instant. The
oracle assumes nothing about the order in which pages are copied or delivered, and
the quiesced control (T3C) shows it is satisfiable. The writer's round counter is
also the sampled value, so every sample set carries its own timeline, and
`writer_rounds_during_*` measures elapsed writer rounds per operation.

| Probe | What it measures | Mechanism |
| --- | --- | --- |
| T1 | retrieved payload vs a known mapped store; is the daemon's buffer changed by a later mapped store? | (a) |
| T2 | with the server parked before `read(2)`, does the reply reflect a store that completed after the notification returned? | (a) |
| T3 / T3C | is one retrieve reply (128 KiB = 32 pages) a single-instant image? quiesced control | (a)(c) |
| T4 | 20 sequential 2-file collections under a 2-file writer: is the collected pair a single-instant image? | (a)(c) |
| T5 | daemon's own `pread(2)` of the whole file through its own mount | (c) |
| T6 | daemon-initiated per-file `fsync` drain under a live writer | (b)(d) |
| T8 | control: park the daemon 15 ms with no flush — does parking alone slow the writer? | (b)(d) |
| T7 | hold a queued writeback WRITE 15 ms: which instant do the delivered bytes correspond to? | (b)(c) |
| T9 | 4 MiB file (1024 pages ⇒ 32 writeback WRITE requests): is the union of all delivered pages one instant? | (b)(d) |

T9 uses a third served file (`big`, 4 MiB) added to the server for this probe only.

### Exact commands

```bash
cd docs/roadmap/0.1/0.1.6/evidence/v1-probe-generic
# main: T1-T6, write-through (run 01) and FUSE_WRITEBACK_CACHE (run 02)
docker run --rm --name v1gen-probe-01 --device /dev/fuse --cap-add SYS_ADMIN \
  --cpus 2 --memory 1g --pids-limit 256 -v "$PWD":/probe -w /probe \
  rust:1.85.1-bookworm sh -c 'ulimit -c 0; gcc -std=c11 -O2 -pthread -Wall -Wextra \
  -o /tmp/v1gen-probe-bin probe.c && sha256sum /tmp/v1gen-probe-bin && /tmp/v1gen-probe-bin'
# ... and the same with a trailing 'writeback' argument, name v1gen-probe-02
# follow-up: T8/T7/T9
docker run --rm --name v1gen-probe-fu-01 --device /dev/fuse --cap-add SYS_ADMIN \
  --cpus 2 --memory 1g --pids-limit 256 -v "$PWD":/probe -w /probe \
  rust:1.85.1-bookworm sh -c 'ulimit -c 0; gcc -std=c11 -O2 -pthread -Wall -Wextra \
  -o /tmp/v1gen-followup followup.c && sha256sum /tmp/v1gen-followup && /tmp/v1gen-followup'
# ... and the same with a trailing 'writeback' argument, name v1gen-probe-fu-02
```

[run_probe.sh](run_probe.sh) / [run_followup.sh](run_followup.sh) are the wrappers
actually used; they contain exactly these commands plus container-removal handling.
Observed binaries: `a0aee75e…` (`probe.c`, both modes), `baf482ee…` (`followup.c`,
both modes). Source hashes: [SHA256SUMS](SHA256SUMS).

### Raw output

| Run | Mode | Log | Metadata | Exit |
| --- | --- | --- | --- | --- |
| 01 | write-through (`0x21`) | [run-01-writethrough.log](run-01-writethrough.log) | [json](run-01-writethrough.json) | 0 |
| 02 | `FUSE_WRITEBACK_CACHE` (`0x10021`) | [run-02-writeback.log](run-02-writeback.log) | [json](run-02-writeback.json) | 0 |
| fu-01 | write-through | [run-fu-01-writethrough.log](run-fu-01-writethrough.log) | [json](run-fu-01-writethrough.json) | 0 |
| fu-02 | `FUSE_WRITEBACK_CACHE` | [run-fu-02-writeback.log](run-fu-02-writeback.log) | [json](run-fu-02-writeback.json) | 0 |

Every run removed its own container; `docker ps -a | grep v1gen` is empty after
all four. Two earlier containers **wedged**: a probe that terminated with dirty
`MAP_SHARED` mappings and a live FUSE mount left a thread unkillable in the kernel
(`State: Z (zombie)`, `Threads: 2`), the container's `sh` stuck in `do_wait`, and
`docker rm -f` failing with `did not receive an exit event`. They were removed by
killing the container's `containerd-shim-runc-v2` inside the Docker Desktop VM
(`docker run --rm --privileged --pid=host alpine:3.20 … kill -9 <shim>`), after
which containerd reaped and removed them. The committed probe unmaps every mapping
and closes every fd **before** `umount2`; no container has wedged since. This is
container hygiene, not a mechanism claim.

## Results

### T1 — the retrieved payload is a copy, and it is stable once received

`T1 retrieve size=4096 reply_word=0x4141414141414141 … daemon_writes_delta=0` →
`RESULT T1 delivered_is_copy_of_current_bytes=1 daemon_copy_stable_after_mapped_store=1 write_callbacks_for_mapped_stores=0` (both modes).
The notification delivered the byte the mapping had stored, with zero WRITE
callbacks, and a later mapped store did not change the daemon's retrieved buffer.
So *"a stable copy"* is obtainable. What is not obtainable is a *chosen instant*.

### T2 — the copy instant is not the notification return : DOES-NOT-SATISFY

Server parked before `read(2)`; the mapping held `0x43…` when the notification was
accepted; a store to `0x44…` completed after the notification returned; the reply
carried `0x44…`, with `daemon_writes_delta=0`.
`RESULT T2 reply_bound_to_notification_instant=0 reply_reflects_store_after_return=1` (both modes, 0 WRITEs).
This reproduces the retained-page counterexample of
[the earlier investigation](../v1-investigation/README.md) in both negotiated modes.

### T3 — one retrieve reply is not a point-in-time image : DOES-NOT-SATISFY

| Run | Trials | Replies violating a single instant | Max spread | Control (quiesced) |
| --- | --- | --- | --- | --- |
| 01 write-through | 20 | **20/20** (496 compared pairs each) | 764 writer rounds | `T3C quiesced_violations=0` |
| 02 writeback cache | 20 | **19/20** | 2468 writer rounds | `T3C quiesced_violations=0` |

`writer_rounds_during_reply` was 20 000–60 000, i.e. the writer ran at full speed
(≈46 ns/round) while the 32 pages were copied, and the returned pages span up to
764 rounds. The retrieved 128 KiB is a mixture of two or more instants: the reply
is not the file at any single instant. (Consistent with the cited mechanism: the
notification holds page references and `fuse_copy_pages` copies contents into the
daemon's buffer at delivery time, page by page — see
[kernel-citations.txt](kernel-citations.txt) §1.)

### T4 — sequential per-file collection has no common instant : DOES-NOT-SATISFY

Writer stores round `r` at file A page 0 then file B page 0, so at every instant
`A >= B`. The daemon collected A (retrieve) then B (retrieve):
**20/20 collected pairs had `B > A`**, deltas 0.5–2.9 million writer rounds
(`RESULT T4 collected_pairs_violating_single_instant=20`, both modes). The collected
pair never coexisted at any instant. Any mechanism whose acquisition is a sequence
of per-file or per-range collections has this defect for a whole-Workspace Commit,
independently of whether each individual collection is atomic.

### T5 — the daemon's own cached read already sees the bytes, and they are torn : DOES-NOT-SATISFY

`T5 self_pread_bytes=131072 daemon_read_requests=0 daemon_write_requests=0 daemon_fsync_requests=0`
→ `RESULT T5 bytes_visible_without_write_or_fsync_callbacks=1 collected_is_single_instant=0 write_callbacks=0`.
A `pread(2)` of the whole 128 KiB file through the daemon's own mount returned the
mapping's current bytes with **zero** FUSE requests (the pages were cache-resident,
so the kernel served the read from the page cache) and **496/496** compared pairs
violated the single-instant invariant (max spread 708/1228 rounds; the sample lagged
the mapping's current value by 40/52 rounds at the same moment). Reading the bytes
is not the problem; reading them *as one instant* is.

### T6 / T7 / T8 / T9 — writeback: owned, coherent per-file copies at a kernel-chosen, undisclosed instant

- **T8 control.** Parking the daemon 15 ms with no flush pending: the mapped writer
  ran at 21 400–36 000 rounds/µs-range (≈28–47 ns/round) with **0** daemon reads and
  **0** daemon writes. Parking alone does not throttle the writer, so the throttle
  seen in T7/T9 is caused by the flush, not by the experiment's scheduler.
- **T6.** `fsync` under a live writer delivered **all 32 pages in one WRITE**
  flagged `FUSE_WRITE_CACHE` (`write_cache_flagged=1`), with **0/496** ordering
  violations, while the mapping's value at `fsync` return was 203 (run 01) / 1679
  (run 02) rounds *ahead* of the delivered bytes.
- **T7** (8 trials per mode). With the queued writeback WRITE held for 15 ms, the
  delivered payload in every trial was an exact single-round image
  (`payload_spread_rounds=0`, `0/496` violations) whose value equals the writer's
  round **at unpark**, 980–4528 rounds **after** the flush request — not the
  request instant. The writer advanced only 1418–5517 rounds in those 15 ms, i.e.
  ~100× slower than the T8 control: the file's mapped writer is stalled by the
  kernel while that file's pages are under writeback.
- **T9** (3 trials per mode, 4 MiB = 1024 pages, 32 WRITE requests).
  `distinct_pages=1024/1024`, `payload_spread_rounds=0`,
  `violating_pairs=0 of 523776`, `write_cache_flagged=32`; the writer advanced only
  **3–9 rounds** during the whole 4 MiB drain, and the drained image corresponds to
  an instant ~25–40 writer rounds before the mapping's value at drain return.

Two facts are therefore established by measurement plus the cited kernel source:

1. A daemon-initiated writeback drain hands the daemon **owned** copies of the
   dirty mapped bytes (`folio_copy(tmp_folio, folio)` at fill time under the page
   lock — [kernel-citations.txt](kernel-citations.txt) §2), delivered as
   `FUSE_WRITE` with `FUSE_WRITE_CACHE`, and at 4 MiB the delivered union was a
   **coherent single-instant image** of that file's mapped bytes (0/523 776
   violations, 3 trials, both modes).
2. The instant is chosen by the kernel, is inside the drain, and is **not
   observable** by the daemon: the delivered bytes lagged the flush request by
   980–4528 writer rounds (T7) and the mapping by 203–1679 rounds at flush return
   (T6). Nothing in the FUSE ABI carries a boundary or version for the drained
   range (§4).

Under the frozen constraints this still does not satisfy the requirement:
acquisition work is payload-proportional (4 MiB drained = 4 MiB copied) and the
daemon cannot even enumerate which files hold kernel-dirty mapped bytes — `mmap`
sends no request (`fuse_file_mmap`, §3), and open-unlinked inodes have no path for
a daemon self-open (predecessor evidence), so only a mount-wide `syncfs` reaches
them; and the mapped writer of each drained file is stalled for the drain.

### Mechanism verdicts

| Mechanism | Verdict | Basis |
| --- | --- | --- |
| (a) `FUSE_NOTIFY_RETRIEVE` with an explicit stability test | **DOES-NOT-SATISFY** | owned copy and stability: T1 PASS (0 WRITEs). Not a chosen instant: T2 (reply carried a store that completed after the notification returned). Not one instant for a file: T3 20/20 (run 01) and 19/20 (run 02) replies torn over 32 pages, control 0/496. Cited: page references held until request end, contents copied at delivery (§1) |
| (b) writeback cache / page-cache-dirty accounting (`writeback_cache`) | **DOES-NOT-SATISFY** as specified, with a positive kernel-contract core | the kernel *does* deliver the dirty payload for a mapped file: T6/T9 all pages as `FUSE_WRITE_CACHE`, owned copies (§2), coherent for one file (0/523 776). Disqualifiers: instant is kernel-chosen and undisclosed (T7/T6), work is payload-proportional, enumeration of dirty mapped inodes is unavailable (§3) or lacks a path (open-unlinked), writer stalled per drained file (T7/T9). Negotiating `FUSE_WRITEBACK_CACHE` changed none of the acquisition properties (runs 01 vs 02) |
| (c) a supported request/notification returning a *stable copy* at a chosen instant | **DOES-NOT-SATISFY** | ABI surface: `FUSE_NOTIFY_RETRIEVE` = references (§1); `FUSE_READ` = live page cache (T5: 0 daemon requests, 496/496 torn); `FUSE_COPY_FILE_RANGE` is daemon-side (no kernel instant); `FUSE_LSEEK` = size/hole query; `ioctl` is daemon-defined and returns no kernel copy; master's `FUSE_NOTIFY_INC_EPOCH` (7.44) and `FUSE_NOTIFY_PRUNE` (7.45) are cache invalidation only and the deployed kernel offers neither (§4). No primitive returns a copy plus its boundary |
| (d) a documented primitive giving the daemon a coherent point-in-time view by the kernel writing back dirty pages and reporting that boundary | **DOES-NOT-SATISFY** | The daemon-visible artifacts of a drain are `FUSE_FSYNC`/`FUSE_WRITE` only; no struct or reply carries the boundary or a version (§1/§4). Measured: the delivered instant moved 980–4528 rounds after the request (T7) with no daemon-visible signal, and per-file drains have no common instant across files (T4) |

## Verdict

**No generic, in-tree-supported FUSE mechanism gives a Commit a chosen-instant,
stable copy of the bytes writable shared mappings currently expose for a
Workspace.** What the kernel contract *does* provide (new fact, measured and
cited): a daemon-initiated per-file writeback drain yields **owned, coherent
single-instant copies of that file's mapped dirty bytes**, with the writer stalled
during the drain, at a kernel-chosen instant the daemon cannot observe.

The precise missing capability, in three parts:

1. **A chosen instant.** The daemon can *cause* a copy (retrieve, drain) but cannot
   *choose* or *learn* the instant. `FUSE_NOTIFY_RETRIEVE` copies at delivery time
   and admits later stores (T2/T3); a drain's boundary is inside the flush and
   unobservable (T6/T7/T9). No ABI field reports a boundary or version for a
   collected range, and the daemon cannot detect that its own copy is torn.
2. **One instant for more than one file.** A sequential per-file collection is a
   mixture of instants (T4: 20/20, deltas 0.5–2.9 M rounds). No primitive collects
   many inodes at one instant.
3. **Bounded, enumeration-free acquisition.** Work is proportional to dirty payload,
   and the daemon cannot learn which inodes have writable mappings
   (`fuse_file_mmap` sends no request) nor reach open-unlinked inodes except through
   a mount-wide `syncfs`. A bounded acquisition needs a kernel-side enumeration or
   an owned-copy primitive the current ABI does not have.

Exact external input that would close this: either (X) a kernel FUSE ABI addition
that returns, for caller-named ranges, an owned copy **together with the
instant/boundary it corresponds to** (e.g. write-protect/COW-on-retrieve, or a
negotiated epoch/version for cached mapped ranges) — a maintainer-provided surface,
not a LinuxKit or VM adapter; or (Y) an explicit owner decision accepting the
narrowed, measured semantics of the drain path: payload-proportional work on
kernel-dirty bytes, a per-file writer stall for the drain, enumeration restricted to
inodes the daemon can open (with a declared open-unlinked gap), and a declared
boundary that is not a single instant across files. Both are decisions outside this
probe; nothing here asks for or assumes either, and no semantic relaxation was
adopted. There is no repository-local implementation to point at: no change inside
`crates/layerfs-fuse` can synthesize the missing ownership/boundary signal from the
current kernel surface.

## What I did NOT prove

- **Not** that every possible generic mechanism is exhausted. Assessed only by
  citation here (not re-measured): `FUSE_PASSTHROUGH`/DAX, io-uring FUSE transport
  (the deployed kernel negotiated minor 31), shmem + `userfaultfd` write-protect,
  `PAGEMAP_SCAN`, reflink/APFS clone, Btrfs-level snapshot. The predecessor and the
  [retrieval investigation](../v1-investigation/README.md) cover those; none is
  re-tested by this probe.
- **Not** that the writeback drain tears at 4 MiB. The measured claim is the
  opposite (0/523 776 violations) and it comes with the writer stalled ~100×; I did
  not establish the *reason* for the stall magnitude (T7 1.4–5.5 k rounds/15 ms vs
  T6 406–2061 rounds), nor whether the drain's fill instant is the flush request,
  the unpark, or somewhere between. I report the observation only.
- **Not** the sub-page behaviour: samples are the first 8 bytes of each page; a
  store inside a page overlapping the copied range is not resolved, and no
  within-page tearing test was run.
- **Not** multi-process or multi-thread writers, larger than 4 MiB, sparse files,
  truncate/unlink during a drain, or writeback-error delivery. Those matter for a
  Commit contract and are untested here.
- **Not** the writeback scheduling rule. Run 01 counted only 1 WRITE in 200 ms of
  mapping stores, while T9's run counted 105–125 writes during mapping activity; the
  earlier "mapped stores never reach the daemon" statement is therefore scoped to a
  quiescent mapping over the observed window and does not hold over a longer
  horizon. I did not determine what triggers each writeback.
- **Not** a two-file *drain* test. A T10 variant (drain A, then drain B, compare the
  collected pair) was written and is **not** included as evidence: file A's drained
  payload was never observed (`fileA_p0=0` in all 12 trials) and the variant was
  removed rather than reported. The cross-file claim rests on T4 only.
- **Not** verified that the oracle detects all tear patterns: it is proven to flag
  the ascending-writer/ascending-copy mixture (T3, 20/20) and to be satisfiable
  quiesced (T3C, 0/496); other writer/copy order combinations were not examined.
- No benchmark, #122 case, Cargo build, issue comment, commit, or release was
  produced. No file under `crates/` was read-modified; nothing outside this
  evidence directory was edited.
