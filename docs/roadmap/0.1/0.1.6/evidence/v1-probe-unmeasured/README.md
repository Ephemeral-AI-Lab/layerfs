# Unmeasured FUSE mechanisms: PASSTHROUGH, DAX, userfaultfd, sub-page tearing

Status: **raw probe evidence, captured under an owner stop order.** The probe was
cancelled mid-flight when the owner stopped the campaign, so this README was
written from the recorded logs rather than by the probing agent. Everything below
is read off the raw logs in this directory; nothing is inferred beyond what those
logs state. **No verdict here changes the V1 contract, and no semantic change is
authorized.** V1 remains OPEN.

Context: this directory continues
[`../v1-probe-generic/`](../v1-probe-generic/README.md), which settled four
mechanisms (negative) and explicitly left sub-page tearing, PASSTHROUGH, DAX and
userfaultfd unmeasured. The owner's answer to the V1 decision question was
"investigate unmeasured mechanisms first" (ledger L34).

Environment: kernel `6.12.76-linuxkit` (environment identity only, not an
approved dependency), image `rust:1.85.1-bookworm`
(`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`),
`--device /dev/fuse --cap-add SYS_ADMIN`, 2 cpus / 1 GiB / 256 pids, max 1 stack
depth. Every container was removed; no container remains. Exact `docker run`
commands are recorded in each `run-*.json` and in `run_probe.sh`.

## 1. Capability survey (`run-caps.log`, `run-caps.json`)

| Capability | Result |
| --- | --- |
| `FUSE_PASSTHROUGH` kernel-advertised | **yes** |
| `FUSE_HAS_INODE_DAX` | **NO** |
| `FUSE_SUBMOUNTS`, `FUSE_MAP_ALIGNMENT`, `FUSE_ALLOW_IDMAP` | NO |
| `FUSE_DIRECT_IO_ALLOW_MMAP`, `FUSE_WRITEBACK_CACHE`, `FUSE_INIT_EXT` | yes |
| `BACKING_OPEN` without negotiated passthrough | `rc=-1 errno=1 (EPERM)` |
| `BACKING_OPEN` with negotiated passthrough | `rc=1 errno=0` → `verdict=PASSTHROUGH_AVAILABLE_AND_NEGOTIABLE` |
| `userfaultfd(2)` | `rc=-1 errno=1` → `userfaultfd_available=0 errno_name=EPERM` |

So on this kernel **PASSTHROUGH is a live option** while **DAX is not available
at all** (`FUSE_HAS_INODE_DAX` not advertised) and **userfaultfd is blocked by
permission** inside the probe container.

## 2. PASSTHROUGH (`run-passthrough.log`, `run-passthrough2.log`)

| Question | Measured result |
| --- | --- |
| Can the daemon read mapped dirty bytes from its own backing fd, without client cooperation and without fsync? | **Yes.** `daemon_reads_mapped_dirty_bytes=1 without_client_cooperation=1 without_fsync=1 fuse_requests_for_that_read=0` |
| Is such a read a single instant? | **No.** `daemon_read_trials=50 reads_that_are_NOT_a_single_instant=19` (~38%); even after `fsync` of the backing file, `fsync_backing_then_read_trials=20 still_not_single_instant=11` (~55%), `max_delta_rounds=1` |
| Does reading stall the mapped writer? | **Partly.** `PAS3 writer_rounds_per_ms_during_reads=1699604 writer_rounds_per_ms_idle=2525555 stall_ratio_pct=67` — the writer continues at ~67% of its idle rate, versus the ~100× stall measured for the writeback drain in the previous probe |
| Descriptor cost | `daemon_fds_per_passthrough_inode=4` for 4 backing files (`daemon_fd_delta_for_4_backing_files=4`); the log notes the kernel additionally allocates one `backing_file` per passthrough open and one unbounded idr entry per registration |
| Coexistence with the cached-handle path | **Mutually exclusive.** With a cached handle open, a passthrough open fails `rc=-1 errno=5 (EIO)`, and symmetrically a cached open fails with `EIO` while a passthrough handle is open; both succeed once the other is closed |
| open-unlinked inode | Mapping survives unlink, the daemon still reads its own backing, reopen-by-path returns `ENOENT` |

**This is the closest any mechanism has come**, and it is why it must be
recorded rather than dismissed: it obtains mapped dirty bytes with no fsync, no
client cooperation and no FUSE request for the read, and it does not stop the
writer. But it fails the contract on three independent counts: the read is
**not a single instant** (and fsync does not fix it), it costs **one descriptor
per inode**, and it is **mutually exclusive with the cached handles** the product
uses today.

## 3. Sub-page tearing — the gap the previous probe left open

The previous probe tested 32-page replies and left sub-page tearing untested.
Measured now (`run-subpage.log`, `run-subpage2-writethrough.log`,
`run-subpage2-writeback.log`):

| Mode | Trials | Replies torn | Tear rate | Basis |
| --- | --- | --- | --- | --- |
| write-through | 200 | 103 | **51%** | `RESULT T-SP-A mode=page0 retrieve=4096B ... single_page_replies_torn=103 tear_rate_pct=51 max_delta_rounds=1 breaks_in_page=114 breaks_at_page_end=0` |
| writeback | 200 | 93 | **46%** | `RESULT T-SP-A ... single_page_replies_torn=93 tear_rate_pct=46 ... breaks_in_page=101 breaks_at_page_end=0` |

Tearing occurs **inside** a page, not at page boundaries (`breaks_at_page_end=0`
in every trial), and at word granularity (`first_break_word` varies per trial,
`delta=1`). A single 4 KiB retrieve is therefore frequently not even internally
consistent, which strengthens the negative verdict rather than weakening it.

## 4. Can the daemon learn a mapping exists? (`run-enumdirty.log`)

The earlier record's premise was that the daemon "never learns that a mapping
exists". That premise is **refuted** for a locally observable process:

| Question | Measured result |
| --- | --- |
| Mapping existence/range learnable | `mapping_existence_learnable=1 range=0xffff95e50000-0xffff95e70000` (from a `/proc` maps line) |
| Soft-dirty clear available | `soft_dirty_clear_available=1 errno=0` |
| Does the scan identify the written pages? | `scan_rc=2 pages_reported=2 pages_reported_WRITTEN=2 pages_actually_written_by_the_mapping=2` — the identified set matched the actually-written set on this sample |
| Soft-dirty bit observed on present pages | `pages_present=2 pages_with_soft_dirty_bit=0x0` — **counter-intuitive and not explained by this probe**; do not read the two rows as agreeing until that is resolved |

This does not by itself yield a snapshot: learning ranges and dirty sets via
`/proc` requires enumerating processes and mappings in the client's namespace,
covers only locally visible processes, and reproduces exactly the torn-read
problem measured in §3 unless something fences the writer.

## 5. What this does and does not change

- **Unchanged:** no checked mechanism satisfies exact visibility + mmap support +
  non-freezing + bounded acquisition together. V1 stays OPEN. No constraint was
  relaxed and no semantic change was adopted.
- **Newly available fact:** PASSTHROUGH is negotiable on this kernel and is the
  first mechanism that reads mapped dirty bytes without fsync, without client
  cooperation and without stalling the writer to a standstill — but it is not a
  single instant even after fsync, costs a descriptor per inode, and cannot
  coexist with the cached handle path on the same inode.
- **Newly closed gap:** sub-page tearing is real and frequent (51%/46% of
  single-page replies), and it happens within pages.
- **Newly refuted premise:** "the daemon never learns that a mapping exists" is
  false for locally observable processes.

## 6. What was NOT established

- **The probe was cancelled before completion.** DAX was established as
  unavailable here and userfaultfd as permission-blocked; neither was pursued
  further. No coherence or cost measurement exists for either.
- **The passthrough read's coherence was not resolved.** The logs show it is not
  a single instant; whether a bounded retry, a backing-file fence, or a
  per-inode serialization could make it one is untested.
- **A T10-style two-file/whole-Workspace variant was not run** for passthrough.
- **The soft-dirty discrepancy in §4 is unexplained** and needs its own
  investigation before §4 can be used as a positive capability.
- **No product source was changed and no repository test was run** for this
  directory. These are standalone C probes; nothing here is a product
  verification, a performance gate or a benchmark.
- **Nothing here is a benchmark or a speedup claim.** The `pread_ns` and
  stall-ratio numbers are probe-internal diagnostics of the probe's own loop.
