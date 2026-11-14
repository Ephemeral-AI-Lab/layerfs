# Experiment: adaptive READDIRPLUS

> **Status:** Diagnostic prototype on branch `exp/readdirplus` (base
> `4db7d765c6`), issue #684. Not a product contract. One sample per arm;
> times are diagnostic, counts are exact.

## Hypothesis

`ls -l` of 2,000 committed files costs 2,022 requests (2,001 LOOKUP, 17
READDIR). READDIRPLUS answers names and attributes together, so the same
walk needs about 85 requests. The kernel takes one lookup reference per
READDIRPLUS entry, as for LOOKUP, so custody must match LOOKUP exactly.

## Change (commit `8fb240056a`, all off by default)

- `LAYERFS_PROBE_READDIRPLUS=plain` requests `FUSE_DO_READDIRPLUS`; `auto`
  adds `FUSE_READDIRPLUS_AUTO` (`mount/profile.rs`). Unset: READDIR only.
- READDIRPLUS entries carry LOOKUP's attributes, 60 s TTL and generation
  (`request/directory_plus.rs`). A reply that publishes its offsets takes
  one kernel lookup reference per handed-out entry **in its publishing
  visit**, one owner job per window (`DirectoryBatch::adopt`, the path NFS
  READDIRPLUS already uses; same `add_kernel_lookup` + directory parent as
  LOOKUP). `.`/`..` take none; the kernel ignores their attributes. A
  reused published reply (seek back to a handed-out offset) publishes
  nothing, so each entry is looked up as LOOKUP does; a name gone since
  gets node id 0 and no reference.
- **Reply-sized windows.** A READDIRPLUS window visits at most
  `page / 160 + 1` = 26 keys and reads facts only for them, reusing the
  names the reading visit merged from memory
  (`list_from_window_rows`, `KernelDirectoryVisit::with_rows`,
  `RequestServices::directory_visit_rows`, `finish_limited`).
  `LAYERFS_PROBE_READDIRPLUS_ROWS` overrides it;
  `LAYERFS_PROBE_READDIR_ROWS` sizes READDIR windows the same way.
- Probe phases, each on a fresh mount: `ls -l`, `ls`, `find -printf
  '%s %m %n %f'` (stats every file), seek-back relist (`getdents64`,
  `lseek`, `getdents64`), and `ls -l` after creates, renames, a rename
  over a name, deletes, `mkdir`, `symlink`. Output hashes compare arms.
- Custody check (`LAYERFS_PROBE_FORGET_CHECK=1`, after timing): drop guest
  dentries/inodes (`drop_caches=2`), wait until the kernel has forgotten
  everything, and read the engine's kernel lookup rows. They must fall to
  the root's implicit row; an extra engine reference stays as a row, a
  missing one fails FORGET as an underflow.

Production LOC 164,027 → 164,488 (+461).

## Results

Batch 3, binary `5878ccf474c0fae1990f898a7fb83ab7f8715c07334706cb7ac7298db542a1e9`
(= commit `8fb240056a`). BLAKE3 18.4–18.5 µs/call in every arm. Wall ms;
requests exclude the FORGETs the custody check causes.

| Phase | control | READDIR rows 129 | auto | plain |
| --- | ---: | ---: | ---: | ---: |
| `ls -l` wall / requests | 175.1 / 2,022 | 121.1 / 2,022 | 37.5 / 87 | **36.2 / 87** |
| — opcodes | 2,001 LOOKUP, 17 READDIR | same | 82 READDIRPLUS, 1 LOOKUP | 82 READDIRPLUS, 1 LOOKUP |
| `ls` wall / requests / refs | 18.7 / 21 / 2 | **5.0** / 21 / 2 | 19.6 / 22 / 25 | 15.9 / 86 / 2,002 |
| `find` stat walk | 142.3 / 2,022 | 129.3 / 2,022 | 144.8 / 2,000 | **16.8 / 87** |
| modified `ls -l` | 152.5 / 2,087 | 127.4 / 2,087 | 21.7 / 159 | **21.2 / 159** |
| seek relist equal | yes | yes | yes | yes (25 fallback lookups) |
| creates, µs/file (10,004 req.) | 262.8 | 200.7 | 195.9 | 263.5 |
| cold read MB/s | 176.4 | 178.3 | 177.6 | 178.7 |

- **Correctness.** Every phase's output hash is equal in all 13 arms that
  reached it (`ls -l` `e089b0947caa0423`, `ls` `053cff30e17f4f55`, `find`
  `f6bc3f8bacb470ed`, modified `3412b3d5c485930b`); the modified listing
  has exactly the 2,001 expected names; the seek-back relist returns the
  same names and nonzero inode numbers.
- **Custody.** After every listing phase of every arm the engine's kernel
  rows fell to 1 (`ls -l`: 2,002 → 1, FORGET units 2,001 in all arms;
  seek relist plain: 50 → 1 with 49 units). No underflow, no leftover.
- **Window size is what made it pay.** Batch 2 (`e9d38d1b…`) ran plain
  with `ROWS=1024` (full 1,024-key windows and facts): `ls -l` 197.3 ms,
  `ls` 168.9 ms, against control 132.4 / 18.8 and plain 35.7 / 16.2. Each
  READDIRPLUS reply holds only ~25 entries; a full window costs ~900 µs
  of visit plus facts for 1,024 names per request.
- **READDIR has the same waste.** READDIR rows 129 (one reply's worth)
  cuts READDIR callbacks from ~1 ms to ~0.2 ms each: `ls` 18.7 → 5.0 ms.
- **auto** equals plain for `ls -l` (the kernel re-advises READDIRPLUS
  when a READDIRPLUS dentry is used) and for plain `ls` takes only the
  first window's 24 references. It does nothing for `find`/fts walks,
  which read the whole directory before stat.
- **Creates do not regress.** Request counts and code path are identical;
  per-file time moves ±30% between arms with no pattern (batch 1:
  205/225/221; batch 2: 247/272/272/273; batch 3 above).

Earlier batches (receipts): batch 1 (`29f197eb…`, facts limited, full
windows) `ls -l` 124.4 → 80.0 (plain) / 80.5 (auto) ms, `ls` 13.5 → 65.9
(plain). The first smoke (`4972512d…`, unlimited facts) spent 171 ms in
82 READDIRPLUS callbacks, then **failed** at `find`: the probe passed the
path after the expression (probe bug, fixed before `smoke2`). Custody
checks: 62 of 62 fell to 1.

## Risks

- plain makes every listed entry a kernel inode and an engine kernel row
  (2,002 for a plain `ls` against 2), and FORGET work later (6–12 ms of
  callbacks per 2,000 when evicted).
- Facts are read outside the owner from the reading visit's snapshot,
  the same staleness class as a LOOKUP reply.
- Node id 0 for a vanished name of a reused reply: glibc skips `d_ino` 0,
  raw `getdents64` readers see it.
- The 26-entry budget assumes the kernel offers at least one page per
  READDIRPLUS (6.12: exactly `PAGE_SIZE`); a bigger buffer only carries
  fewer entries than it could.
- Cache: each phase has a fresh mount (kernel caches empty), but the
  daemon's object cache is per process: `ls -l` is the first phase to read
  `small/`; later directory phases find its objects warm, in every arm.
  `drop_caches` is guest-wide and runs only after timing.
- Not run: crate tests (scripts build only the probe), Clippy
  (unavailable in the image), NFS (unchanged by construction: its calls
  take the default full window).

## Recommendation

Adopt READDIRPLUS in plain mode (`FUSE_DO_READDIRPLUS`) **with
reply-sized windows**: `ls -l` 175 → 36 ms (−79%, 2,022 → 87 requests),
stat walks −88%, plain `ls` no slower than today. Separately, size READDIR
windows to one reply (rows ≈ `page / 32 + 1`) whatever the listing mode.
Choose auto + reply-sized READDIR instead only if kernel-reference growth
on name-only listings matters more than `find`-style walks; that
combination was not measured as one arm.
