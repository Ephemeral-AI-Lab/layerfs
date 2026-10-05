# 05 — FUSE assessment: current code, qualified evidence, profile and coherence

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. No measurement was run for this document. Every number is
> copied from a retained report and keeps that report's status. Claim labels are
> defined in the [entry point](README.md#claim-labels).

## 1. What was measured and what was not

**No retained cell in #305 or #306 exercised product code.** [source-verified]

| Name | What it is | What it is not |
| --- | --- | --- |
| Product on `main` | A private copy-on-write page store, one file per 4,096-byte page (`core/crates/layerfs-workspace/src/backing/active/pages.rs:130-131`), mounted with zero TTL and direct I/O (`core/crates/layerfs-fuse/src/adapter.rs:22`, `:228-234`) | Not a SQLite overlay. Never timed by #305 or #306 |
| A1 | A throwaway ext4 passthrough daemon mounted with the product's profile values | Not the product: it shares none of the Workspace code |
| A2 | The same passthrough with a 60 s entry/attribute lifetime, `FOPEN_KEEP_CACHE` and `fsync` accepted as a no-op | No overlay, no generations, no capture, no Commit, no base acquisition, no cluster one work, no invalidation of any kind |
| B | A throwaway four-table SQLite prototype over a local directory, under the A1 profile | Not the design in this set. Its only non-trivial cell FAILED |
| "LayerFS A2" in #306 | The A2 passthrough column, copied from #305 | Not a LayerFS product cell |

Evidence location: branch `codex/phase7-experiment-305` at `1451b68a7` (local,
unpushed), under `core/docs/issues/305/` and `core/docs/issues/306/`. Read a
file with `git show codex/phase7-experiment-305:core/docs/issues/305/<name>`.
Experiment source is `core/experiment/real-tree/src/` on the same branch.

### 1.1 Qualifications that apply to every number below

[measured diagnostic evidence]

- One record per cell. Every record ran after `sync` and VM `drop_caches=3`
  with **no residency proof**, so under root `AGENTS.md` §1 every numeric row is
  `INELIGIBLE`: a diagnostic, never a `PASS`. `admission_eligible=false`
  throughout (`STAGE-A-REPORT.md`, `SUMMARY.md`).
- Windows are not comparable. The same arm with the same request count read
  41.381 s and 33.788 s in two windows (E12/A2WN), and 4.019 s and 4.786 s
  (E02/A2P).
- Timer erratum. Every contract before `cf-fsbench-v2` waited with a polling
  timeout: Exec is an upper bound high by up to about 51 ms and Unmount by up to
  about 16 ms (`CF-FSBENCH-V2-CONTRACT.md`). The E01 fixed-cost rows (Exec
  24–29 ms, Unmount 14–15 ms) lie inside that band. Receipts were not rewritten.
- Shared Docker Desktop VM with other owners' containers present; overlap with
  a timed phase is unknown.
- A1 and A2 differ in three coupled settings (lifetime, cache mode, `fsync`).
  No arm separates them.
- A2 runs as uid/gid 1000 with `allow_other`; the product mounts owner-only as
  uid 0 (`core/crates/layerfs-fuse/src/mount.rs:209`, `:400-427`).

### 1.2 Owner decisions on record

[owner requirement, quoted from #305]

- **Promoted, prospectively:** "promote the original A2 cached profile, not A2O
  or A2P, as the preferred profile for the next experiment and Phase 7
  mount-design candidate" (issue comment 5972685996, 2026-10-03T19:24:50Z; the
  issue body dates the decision 2026-10-04 local time). Settings: 60 s
  entry/attribute lifetime, cached reads with `KEEP_CACHE`, 128 KiB requests,
  the existing two threads, background/congestion 1, kernel writeback **off**.
- **Limits stated with it:** "It does not change the product implementation or
  qualify a release." "Cache invalidation for overlay/Commit transitions,
  bounded memory and concurrency remain to be established."
- **Not promoted:** A2O (narrow lock, stable enumeration) and A2P (always
  readdirplus) stay diagnostics.
- **No owner ruling exists** for the walk (A2W), no-permission (A2WN),
  handle-free (A2S), placement (A2S1), negative-entry (A2SL) and identity (A2SI)
  arms. Each report ends "Proposals, not adopted".

### 1.3 Failed, incomplete, exceeded and unrun cells

[measured diagnostic evidence]

**#305**

| Status | Cells |
| --- | --- |
| FAILED | B/E10/F. The command exited 0 and stdout matched Native; the required post-Commit mounted-tree proof reached its 10 s bound, so whole-tree survival is unestablished. Nothing was shown wrong and nothing was shown right |
| NOT_RUN | E09 on N, A1, A2 and B (nondeterministic TAP output); B/E01/S; B/E02–E08; B/E11–E17; all of Stage C (C01–C08); three network rows of `fs-bench.sh` |
| EXCEEDED, retained | Two E08 native oracle attempts at 60 s, superseded prospectively by a 600 s profile and never relabelled |
| SLOW (correct, past the 15 s line) | Stage A: E03/A1, E03/A2, E04/A1, E05/A1, E08/N, E08/A1, E08/A2, E10/A1, E11/A1, E12/A1, E12/A2, E13/A1, E13/A2, E14/A1, E14/A2. Follow-ups: E12 on every FUSE arm, E03 on every FUSE arm, E08 on every arm including native |
| Unexplained | E05/A2WN 22.3% slower than E05/A2 with equal requests; E03/A2W 34.7% slower than E03/A2WN with equal requests |
| Checkpoints | CP5 (Stage B) and CP6 (Stage C) incomplete |

**#306** (every numeric cell declared `INELIGIBLE` by the issue itself)

| System | Completed | EXCEEDED | FAILED / INCOMPLETE | NOT_RUN |
| --- | ---: | ---: | --- | ---: |
| A2 passthrough | historical #305 rows | — | — | E09 |
| computerd 0.4.0 (x86-64 under emulation) | 16 | 14 | timestamp fidelity FAIL | 2, plus 3 baselines |
| Drive9 (TiDB + MinIO) | 4 | 4 | 3 FAILED, 2 INCOMPLETE | 19 |
| JuiceFS 1.4.1 (SQLite + local objects) | 16 | 4 | — | 12 |

#306 is **not a matched ranking**: the systems differ in architecture, backend,
CPU architecture and cache policy. The A2 column was taken under 60 s and 600 s
caps while the others ran under a 15 s stop, so A2's E03 (21.40 s), E08
(75.75 s), E12 (42.20 s), E13 (58.60 s) and E14 (16.29 s) would themselves be
`EXCEEDED` under the #306 bound. One structural observation is worth keeping:
Drive9's C01 shows 1.29 s of Exec followed by 5.86 s of drain, which is why this
design reports drain and Commit beside Exec and never Exec alone.

## 2. The product FUSE layer on `main`

[implemented and source-verified] Paths are under `core/crates/`.

| Aspect | Current value | Source |
| --- | --- | --- |
| Library | `fuser =0.18.0`, pure-Rust mount | `layerfs-fuse/Cargo.toml` |
| Capabilities | fuser defaults only (async read, big writes, max pages). No writeback, readdirplus, splice, no-open, no-opendir, passthrough | `layerfs-fuse/src/adapter.rs:126-128` |
| Request size | `max_write`, `max_readahead`, `max_read` = 128 KiB | `adapter.rs:114-119`; `mount.rs:225` |
| Queue | `max_background` 1, congestion 1 | `adapter.rs:120-125` |
| Threads | 2 loops on one `/dev/fuse` descriptor | `mount.rs:210-211` |
| Lifetimes | Entry and attribute lifetime zero; a missing name is an uncached error | `adapter.rs:22`, `:156` |
| Open flags | `FOPEN_DIRECT_IO` on every writable open and every create | `adapter.rs:228-234`, `:965` |
| Enumeration | READDIR only, 128 entries per page; name-anchored per-handle cookies over a pinned view; each entry resolved and the result discarded | `adapter.rs:413-422`; `layerfs-workspace/src/filesystem/directory.rs:28-49`, `:79-91` |
| Invalidation | One `inval_inode` per projected WRITE, before the reply | `layerfs-workspace/src/filesystem/write.rs:398-406`; `mount.rs:247-260` |
| Inode number | The canonical serial; `ctime = mtime` | `layerfs-fuse/src/replies.rs:63-75`, `:99-102` |
| Permissions | Kernel `default_permissions`, plus an adapter uid guard, plus a Workspace check. Commands run as uid 0 | `mount.rs:220`; `adapter.rs:45-53` |
| Unsupported | `fsync`, `fsyncdir`, readdirplus, xattrs, ownership changes | `adapter.rs:407-428`, `:463-471` |
| Contention | A mutation that arrives while another callback holds a reply permit returns `Busy`, mapped to `EBUSY` | `layerfs-workspace/src/runtime/coherence.rs:482-493`; `layerfs-fuse/src/replies.rs:14` |
| Daemon | One control session; one `slot` lock held across a whole Exec and a whole Commit | `layerfs-daemon/src/control.rs:126-131`, `:310-317`, `:435-451` |

Three things the current layer already does right and the replacement keeps:
canonical serials as stable inode numbers, name-anchored enumeration, and
kernel-enforced permissions.

## 3. Target mount profile

[proposed design, implementing the owner's promoted candidate]

| Setting | Value | Status |
| --- | --- | --- |
| Entry and attribute lifetime | 60 s | owner-promoted candidate |
| Open flags | `FOPEN_KEEP_CACHE`; no `FOPEN_DIRECT_IO` | owner-promoted candidate |
| Kernel writeback cache | off; every `write()` reaches the daemon before it returns | owner requirement |
| `max_write` / `max_read` | 128 KiB | owner-promoted candidate |
| Threads | 2 | owner-promoted candidate; see §7 row 12 |
| `max_background` / congestion | 1 / 1 | owner-promoted candidate; see §7 row 13 |
| `default_permissions` | on | retained |
| `fsync`, `fsyncdir` | accepted, no work, no durability claim | owner-promoted candidate |
| xattrs | refused | retained |
| Negative entries | not cached in the first slice | this design; see §7 row 2 |
| READDIR | plain READDIR with OPENDIR handles in the first slice | this design; see §7 rows 6–9 |

The profile is a mount configuration. It becomes correct only with the
coherence contract of §4, which the passthrough never needed and never had.

## 4. Coherence: a lifetime is not a design

[proposed design]

A 60 s lifetime bounds how long a *bug* stays visible. It is not what makes the
cache correct. Correctness comes from one invariant:

> **The view of a mounted Workspace changes only through (a) a kernel request on
> that mount, or (b) a daemon operation that notifies the kernel before it is
> acknowledged.**

Under (a) the kernel keeps its own caches coherent: it updates the dentry cache
on create, unlink and rename, and it owns the page cache for the bytes it wrote
through. Under (b) the daemon must act. The complete list of view-changing
events:

| Event | Changes the view? | Required action |
| --- | --- | --- |
| Any mutation that arrives as a FUSE request, from any Exec | Yes, through the kernel | None. The per-WRITE `inval_inode` in today's product is removed: with cached reads it would throw away the page the kernel just wrote |
| A second Exec in the same Workspace | Same mount, same kernel caches | None |
| Capture | No. The generation number changes; no name, attribute or byte does | None |
| Install after a known Commit | No. The new base equals the captured overlay by construction, and inode numbers and times are preserved (§5) | None. This is a proof obligation: [07](07-implementation-validation.md) carries the stat-identity check across install |
| Commit refused, conflicted or uncertain | No | None |
| Retirement of folded rows | No | None |
| A mutation that does not arrive as a FUSE request (an SDK or control-plane write, if any is kept) | Yes | `inval_entry(parent, name)` for a name change and `inval_inode(ino, offset, length)` for data or attributes, after the SQL commit and before the caller is answered |
| Discard or rebase of a mounted Workspace | Yes, wholesale | Refused while mounted in the first slice. Unmount, change, mount |
| A different Workspace | Separate mount, separate caches | None |

**Kernel dirty pages.** With writeback off, `write()` is synchronous to the
daemon. A shared writable mapping is not: its dirty pages reach the daemon at
`msync`, at the last close, or when the kernel flushes. Capture therefore
includes exactly the FUSE requests acknowledged before it. A Commit taken while
a process still holds dirty mapped pages does not include them. A Commit taken
after the Exec has exited does, because the last close flushes them. This is a
stated semantic, not a defect to hide.

## 5. Identity and attributes

[proposed design; the first row is implemented and source-verified today]

| Field | Rule |
| --- | --- |
| `st_ino` | The canonical inode serial. A new file takes a serial from a range reserved through `HistoryCatalog::reserve_inodes`, so the number it has in the overlay is the number it has after Commit |
| `st_mtime` | Stored. Preserved by Commit (portable metadata) |
| `st_ctime` | Reported equal to `st_mtime`. Cluster one stores mode and mtime only, so a real change time could not survive Commit; reporting one in the overlay would make every file look changed in the next mount |
| `st_atime` | Reported equal to `st_mtime`; the mount is `noatime` |
| `st_size`, `st_mode` | Stored; preserved by Commit |
| `st_uid`, `st_gid` | The configured command identity for every inode; not stored |
| `st_nlink` | Stored for files; directories report 2 as today |
| Generation | 0. Serials are never reused, so a stale handle cannot name a new file |

**Invariant:** an unchanged file reports identical `ino`, `size`, `mtime`,
`ctime`, `mode`, `uid` and `gid` in every mount of every Workspace that contains
it, and across an install. The supporting diagnostic is E18: on the passthrough,
taking the inode number from the backing file cut a second-mount `git status`
from 38,294 requests to 21,158 and from 15,159 READs to 37
(`A2-IDENTITY-REPORT.md`). That arm had no permission enforcement and its
sub-second totals lie inside the timer erratum; the request counts are the
usable part. The product already has the mechanism; the replacement must not
lose it.

`ctime = mtime` is a deviation from POSIX: a `chmod` does not advance `ctime`
past `mtime`. It is recorded as owner question O-9.

## 6. Handles, open-unlinked files and enumeration

[proposed design]

**Files.** OPEN returns a handle that carries no per-open table entry beyond a
per-inode open count. There is no handle cap; the bound is the process
descriptor limit of the commands. RELEASE decrements the count. An inode whose
link count is zero keeps its rows until the count reaches zero
([04 §7](04-concurrency-commit.md#7-open-unlinked-files)). FLUSH does no work in
a write-through profile; whether to answer it `ENOSYS` so the kernel stops
sending it is §7 row 10.

**Directories.** OPENDIR pins nothing. A directory handle holds a cursor: the
last name returned, the offset the kernel was told, and the names of the last
reply. Enumeration is a merge of two name-ordered sequences, the base directory
and the overlay's `dentry` rows, so:

- an entry that exists for the whole enumeration is returned exactly once;
- an entry created or removed during it may or may not appear;
- a resumed READDIR at the offset the kernel last consumed restarts after the
  remembered name, including when the kernel consumed only part of the previous
  reply;
- `rewinddir` restarts; a seek to any older offset restarts and skips, which is
  exact only if the directory did not change;
- memory per open directory is one reply, not the directory.

The current product's per-entry cookie maps, charged against a shared budget
until RELEASEDIR, are not carried over.

## 7. Optimization disposition

Evidence strength: **none**; **weak** (one unpaired or confounded observation);
**count** (request counts predicted before the record and matched, time not
established); **moderate** (matched arms, effect far larger than window noise,
still one sample, no residency proof, passthrough only). Nothing is stronger
than moderate and nothing is product evidence.

| # | Optimization | Current implementation | Established problem | Proposed mechanism | Requests / work removed | Correctness obligations | Resource consequences | Evidence strength | Future validation | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Long entry/attribute lifetime | Zero (`adapter.rs:22`) | Every path component and stat is a round trip: requests A1 vs A2, E04 576,425 vs 66,255; E12 5,834,083 vs 698,292 | 60 s lifetime under the §4 invariant | Most LOOKUP and GETATTR | §4 in full. No serial reuse | The kernel holds more inodes, so the daemon's lookup-count table grows with the visited tree | moderate, confounded with row 3 | An arm that differs only in lifetime; one coherence test per §4 row | **adopt** |
| 2 | Negative-entry caching | Absent; a base miss is an upstream call | Install replay: 252,231 lookups among 604,145 requests | Reply with node 0 and a lifetime | E12 604,146 → 460,360 | A name created by anything other than a kernel request stays invisible until `inval_entry`. With §4's invariant that is only the non-FUSE mutation row | Kernel negative dentries; a bounded daemon miss cache | count; 4 records; measured with permissions off | Re-measure with `default_permissions` on and an overlay create path | **investigate**; first candidate after the first slice |
| 3 | Kernel page cache (`KEEP_CACHE`) | `FOPEN_DIRECT_IO` on writable opens | Every read syscall is a READ: E15 5.230 s vs 0.641 s; E05 42.26 s vs 5.14 s | Cached opens; write-through keeps kernel writes coherent | Repeated READ; exec and mapping faults | Remove the per-WRITE `inval_inode`. §4 for non-kernel changes | Guest page cache grows with file size. Root `AGENTS.md` §1 forbids excusing that; it needs a phase-local cgroup gate | moderate, confounded with row 1 | Matched arms; cgroup file-cache domain from a reset cgroup | **adopt**, with the memory gate |
| 4 | Kernel writeback cache | Off; a cached WRITE is refused (`adapter.rs:565-572`) | Small writes are one request each (E16: 10,240) | None | Would batch small writes | Breaks "an accepted write has reached the daemon" and with it exact capture | Dirty pages in guest memory | none; no arm enabled it | — | **reject** |
| 5 | Request size above 128 KiB | 128 KiB | E03: 10.5 s of 20.8 s Exec inside READ callbacks | Raise `max_write`, `max_read`, `max_pages` | Up to 8× fewer sequential READ/WRITE | One request stays one transaction; larger extents ([02 §5](02-base-overlay.md#5-payload-extents)) | Larger per-request buffers | none; no arm varied it | Count requests and bytes per request on E03, E15, C06–C11 | **investigate** |
| 6 | READDIRPLUS | Refused; READDIR resolves each child and discards it | E02: 16,867 lookups after enumeration | `DO_READDIRPLUS`, probably with the adaptive flag | The LOOKUP after each listed entry | Each returned entry takes a lookup reference to count and release | The kernel instantiates an inode per entry; Unmount rose to 0.121 s | weak and mixed: −6.3%, −4.5%, **+7.9%** on E10 | Adaptive mode on the overlay | **investigate**; never always-plus |
| 7 | Stable enumeration cookies | Name-anchored per-handle cookies (product) | About 0.7 KiB charged per listed entry until RELEASEDIR | §6: keep the semantics, hold one reply per handle | Per-entry bookkeeping | §6 bullets | Bounded memory per handle | none measured. The experiment's positional cookies are **not** stable | Concurrent rename/unlink during enumeration | **retain** the semantics, **replace** the representation |
| 8 | Cached directory listings | OPENDIR returns no cache flag | A second walk repeats enumeration | `FOPEN_CACHE_DIR` | Smoke count only: a second walk sent no enumeration request | `inval_inode` on the directory for a non-kernel name change | Kernel readdir cache | weak; a smoke count, not a record | Matters only if a mount outlives one call | **investigate** |
| 9 | Handle-free files and directories | Every open takes one of at most 128 handles | OPEN + FLUSH + RELEASE per file: 14,121 each on E04 | `NO_OPEN_SUPPORT`, `NO_OPENDIR_SUPPORT` | E04 80,682 → 38,360; E05 136,355 → 50,871 | (1) With no OPEN or RELEASE the daemon cannot count opens, so an unlinked file must live until the kernel's last FORGET with **correct lookup counting**; the experiment released on the first FORGET. (2) Unlinked-open inodes must survive capture and retirement. (3) Per-open state disappears: access mode, append, exec check. (4) Unmount loses its drain signal. (5) `ENOSYS` is sticky and mount-wide; a CREATE handle then gets no RELEASE | Removes handle state; adds retained unlinked inodes | moderate on time, **measured only with permissions off** | Re-measure with `default_permissions` on; open-unlinked and capture tests | **investigate**; adopt only after (1)–(5) are designed and proved |
| 10 | FLUSH elision | FLUSH takes the Workspace lock | E17: FLUSH is 10,000 of 60,002 requests | FLUSH → `ENOSYS` in a write-through profile | E17 60,002 → 50,003 | Nothing may be deferred to close | None | count | A FLUSH-only arm with OPEN retained and permissions on | **investigate**; cheap, decide in slice S8 |
| 11 | Removing `default_permissions` | Set | The kernel refetches parent attributes after each directory change: E12 94,149 GETATTR for 95,021 changes | None that preserves enforcement is known | E14 232,821 → 150,793 | With cached reads there is no request at which the daemon could check a read. The measured arm let a mode-000 file be read | None | count; the report calls it "not a product candidate" | Decide the command identity model first (O-8) | **reject** removal; **retain** kernel enforcement |
| 12 | Thread count | 2; admission refuses a third reply | The owner requires several Execs and activity during Commit | Keep 2 in the first slice; mutations **wait** on the Workspace mutex instead of being refused | Removes `EBUSY`, not requests | One mutation at a time per Workspace stays, as a short wait | 16 MiB virtual buffer per fuser thread | none; every arm used 2 and Stage C is NOT_RUN | Four-Exec and writer-during-Commit scenarios; lock-wait counter | **retain** 2; **investigate** more |
| 13 | `max_background` / congestion | 1 / 1 | Under a cached profile readahead is background I/O; E03/A2 21.4 s vs 9.2 s native, cause not isolated | Raise with the thread count | None; raises concurrency | The daemon must admit that many replies | More in-flight buffers | none | In-flight depth on E03, C08, C09 | **retain**; **investigate** with row 12 |
| 14 | CPU pinning | Absent | About 40 µs per request is cross-CPU wake-up in this VM | None as policy | None; changes cost per request | A fixed pin is a harness setting. E08 was 35.1% slower pinned; the native build alone lost 21.8 s on one CPU | Starves compute-bound commands | moderate for this VM; "not answerable" for a Linux host | Run the pipe half of `roundtrip_diag.py` on a target host first | **reject** fixed pinning; **investigate** placement |
| 15 | Splice / zero copy | `read` then `writev`; two extra full copies per READ | Not isolated | Remove the avoidable copies first | Copies, not requests | SQLite reads copy by nature | Fewer transient buffers | weak | Bytes copied per READ, as a counter | **investigate**, low priority |
| 16 | Kernel FUSE passthrough | Absent | — | None | All READ/WRITE for a passed-through file | Writes would bypass the daemon, so capture would not be exact; needs a backing file per inode, which a SQLite overlay does not have | — | none. Not what "passthrough A2" means | — | **reject** |
| 17 | SQL granularity per request | No SQL on `main` | B/E10: 4,691,869 statements in 1,259,910 transactions, about 3.7 per transaction, most of it `BEGIN`/`COMMIT` framing around reads | One write transaction per mutating request; reads use autocommit statements with no framing; a cache hit runs no SQL | Framing statements; SQL on cache hits | A reply's rows come from one critical section | — | weak; B FAILED and "does not isolate their costs" | Statement and transaction counters per operation class | **adopt** as a rule ([03](03-mutation-hot-path.md)) |
| 18 | Daemon dentry/inode cache | A node table of kernel-referenced nodes only | E18/A2SI: 0.385 s of 0.540 s Exec inside callbacks for 16,876 lookups | A bounded exact cache of base lookups, shared by every Workspace on the same base | SQL and host calls for repeated lookups | The base is immutable, so base entries never need invalidation; overlay rows are not cached here | A declared byte bound with eviction | weak (passthrough) | Hit-rate and byte counters; second-mount case | **adopt** |
| 19 | Stat identity across mounts | Canonical serial; `ctime = mtime` | Without it a second mount re-reads every tracked file: E18 4.404 s (A2) vs 0.578 s (A2SI) | The §5 invariant | E18 38,294 → 21,158 requests | No serial reuse; `ctime = mtime` hides metadata-only changes | None | moderate; sub-second cells inside the timer erratum | Two-mount and post-Commit stat-identity proof on the product | **adopt** as an invariant |
| 20 | Mount, drain and teardown | 3–5 upstream calls at attach; 1 ms polling loops at unmount; 5 ms Exec poll | Passthrough fresh mount 22–35 ms per call; B mount 0.140 s. Product lifecycle unmeasured | Event-driven drain and exit; no polling sleep on the path; base binding and base cache kept across per-call mounts | Upstream calls and sleeps per call | A mount per call resets kernel caches for free | Idle state between calls | weak (passthrough); none for the product | A fixed-cost row with an event-resolution timer | **adopt** the no-polling rule; **investigate** the rest |

## 8. Cost attribution

[measured diagnostic evidence]

| Bucket | What the evidence supports | What it does not support |
| --- | --- | --- |
| FUSE protocol | Little beyond a generic two-process round trip: FUSE `fstat` 43.84 µs vs a pipe round trip 42.05 µs unpinned; 5.20 µs vs 2.49 µs on one CPU (`A2-WALK-REPORT.md`) | A warm, 50,000-iteration smoke diagnostic, "not a record". Nothing about large payloads |
| VM scheduling | The dominant per-request term for metadata-heavy cases in this Docker Desktop VM: about 38.6 µs × 36,649 requests ≈ 1.42 s of the 1.67 s spent outside the daemon on E02/A2W; pinning saved 43.0 µs and 37.4 µs per request on E02 and E14 with identical counts | Not uniform (15.5 µs on E04). The report labels the wake-up explanation "inferred, not measured". **Not established on any Linux host that is not this VM** |
| Daemon callbacks | Passthrough callback wall with overlapping spans: E04/A2S 1.574 s of 3.293 s; E03/A2WN 15.0 s of 20.8 s | Those are ext4 syscalls in a throwaway daemon. The spans "are not an Exec decomposition". The product's callback cost is unmeasured |
| SQLite | Counts only: B/E10 issued 4,691,869 statements in 1,259,910 transactions; WAL 11.3 MB | **No time attribution.** Callback instrumentation was off and the cell FAILED. B/E10 30.76 s and A1/E10 27.43 s are unpaired observations from different images and windows |
| Base acquisition | Nothing | The base was a local directory in every arm |
| Construction | Nothing | Stage C is NOT_RUN; the prototype's Commit is an untimed stand-in |
| Lifecycle | Passthrough Mount 17–29 ms and Unmount 5–18 ms; B prototype Mount 0.140 s and 0.083 s | The product's attach, control calls, drain and container lifecycle are unmeasured |

The consequence for design: the evidence justifies removing **requests** (rows
1, 3, 17, 18, 19) and says nothing yet about the cost of the overlay itself.
[03](03-mutation-hot-path.md) therefore states overlay targets as counts, which
are reproducible across windows, and leaves time to a prospectively frozen
qualification.
