# G4 Round-1 native materialization, cache-state, durability, and OS-path report

Status: **ROUND-1 RESEARCH COMPLETE / G4 PREREGISTRATION INPUT / NO PRODUCTION PROMOTION**

Date: 2026-08-22

Lane: materialization only. This report does not run G4 acceptance, start G5,
change production source, authorize a persistent cache, or claim native/cold
performance. It records one non-timing disposable capability falsifier.

## 1. Disposition first

1. The retained `materialize-warm` and `materialize-fresh` rows are authenticated
   **logical reconstruction into a hashing/counting sink**, not native file or
   directory materialization. The benchmark explicitly calls
   `reconstruct_file` and records only reconstruction wall
   (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:12151-12181`,
   `:12217-12235`). The governing algorithm says the native path is not
   implemented (`implementation-detail/phase-4/algorithm/spec.md:901-924`).
2. G3-v13 is the only actual native path. It is a benchmark-private, one-file,
   same-open protected-seed path. It correctly implements descriptor-relative
   clone/patch, complete fallback, file/metadata synchronization, atomic rename,
   directory synchronization, and exact target/prior reconciliation
   (`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs:1954-2250`). It
   proves neither first materialization nor persistent cross-process seed
   authority.
3. G3's complete fallback must **not** be copied unchanged into the 100-MiB G4
   first-materialization path. Its `stream_root` performs one borrowed SQLite
   query per chunk/reference and computes only an output digest
   (`phase4_g3_materialization.rs:1087-1117`, `:2055-2081`). The accepted
   `verify_file_inner` path batches leaf BLOB reads and additionally folds the
   ordered closure and occurrence sequence
   (`phase4_create_edit_benchmark.rs:8237-8368`, `:9038-9177`). G3 seed
   preparation also writes the full file and then rereads/hashes the seed before
   unlinking it (`phase4_g3_materialization.rs:1303-1383`), all outside the G3
   operation timer. It cannot be relabeled as first-materialization work.
4. The best format-preserving G4 implementation hypothesis is therefore small:
   add a bounded `Write` sink to the existing batched `verify_file_inner`
   traversal, emit each raw chunk only after complete canonical-object
   authentication, and send it directly to a same-directory private temp. Keep
   the current closure/sequence/output folds, exact summaries, typed errors, and
   current canonical-v2/profile identities. Then apply metadata, synchronize,
   atomically publish, synchronize the parent directory, and reconcile any
   ambiguous publication.
5. Honest controlled cold is **unavailable during this Round-1 run**. Process
   restart is not cold. `/usr/sbin/purge` exists, but it is machine-global and
   was not invoked while three research lanes share the host. Apple describes it
   only as an approximation of initial-boot cold disk-buffer-cache conditions
   ([Apple `purge(8)` source](https://github.com/apple-oss-distributions/system_cmds/blob/main/purge/purge.8)).
   `F_NOCACHE` returned success in the disposable probe, but Darwin defines it as
   turning caching off for that descriptor; it does not prove eviction of
   ordinary-path SQLite, mapping, directory, or device caches
   ([Apple `fcntl(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/fcntl.2)).
6. A prospective G4 controlled-cold row is possible only on an exclusive host:
   finish all hash/manifest preflight first, synchronize, successfully invoke
   `purge` outside the row timer, launch the one row immediately, and label it
   `controlled_cold_buffer_cache_approximation; device_cache=Unavailable`. If
   exclusivity or purge success is absent, the row is `Unavailable`, not
   `fresh-process` or `cold`.

## 2. Custody and environment

### 2.1 Repository custody

| Item | Observed value |
|---|---|
| Working directory | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| HEAD | `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a` |
| Starting tracked diff | empty |
| Starting untracked set | only `implementation-detail/phase-4/experiments/g4-materialization-acceptance/` |
| Preserved handoff | `implementation-detail/phase-4/experiments/g4-materialization-acceptance/round-1-research-handoff.md`; not read, edited, renamed, or chmodded by this lane |
| Benchmark/campaign processes | none found for the G3/G4 executables or runners |
| G3-v13 lock | absent |
| Guessed G4 lock | absent; no timing work attempted |

Only this report was added. No source, sealed evidence, shared ledger, sibling
report, or sibling repository was changed.

### 2.2 Host and toolchain

| Item | Observed value |
|---|---|
| OS/kernel | macOS 26.4.1 build 25E253; Darwin 25.4.0 arm64 |
| CPU | Apple M3 Max; 14 physical/logical CPUs reported |
| Memory | 38,654,705,664 bytes |
| Filesystem | APFS `/System/Volumes/Data`; 4,096-byte device/allocation blocks; Apple Fabric SSD; writable |
| Power at inspection | battery, 55%, discharging; unsuitable for an acceptance timing campaign |
| Rust | `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2 |
| Cargo | `cargo 1.96.0 (30a34c682 2026-05-25)` |
| System SQLite CLI | 3.51.0; default page 4 KiB, default synchronous 2, default mmap 0, default worker threads 0 |
| LayerFS Rust SQLite API | `rusqlite = 0.40.2`, features `cache`, `blob`, `hooks`; macOS `libc = 0.2.189` (`crates/layerfs-engine/Cargo.toml:9-15`) |

The selected product/benchmark path itself enforces `DELETE`, `FULL`,
`temp_store=FILE`, `mmap_size=0`, and `cache_spill=2000`
(`phase4_create_edit_benchmark.rs:2340-2374`). The simpler engine also has one
connection protected by a mutex and ordinary rowid BLOB storage
(`crates/layerfs-engine/src/lib.rs:243-266`, `:683-799`).

### 2.3 Controlling G3 and G2 anchors rehashed

| Artifact | SHA-256 |
|---|---|
| G3 source set | `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d` |
| G3 executable | `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e` |
| G3 raw JSONL | `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` |
| G3 primary / independent analysis | `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` / `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` |
| G3 cleanup / row cleanup | `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e` / `1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506` |
| G3 static closure | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` |
| G3 67-entry manifest | `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` |
| G3 terminal / verification | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` / `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |
| G2-v5 raw | `c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb` |
| G2-v5 primary / independent analysis | `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803` / `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e` |
| G2-v5 manifest | `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` |
| G2-v5 terminal / verification | `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` / `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` |

The G3-v13 measurement is not attributable to the current clean checkpoint
alone. Its sealed STATIC-CLOSURE/ENVIRONMENT custody records bind measurement
HEAD `d79f0e0e2582d1bc491410224fec2b6cef7482e9`, the then-dirty frozen
four-file source set
`3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`,
and executable
`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
Those exact source bytes were committed later in the clean controlling
checkpoint `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.

The G3 terminal says `G3 PASS / G4 READY` and binds the exact source,
methodology, executable, ledger, static, manifest, and G2 anchors
(`target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json:1-34`).
The independent verification reports 67 exact entries, no hash/mode/symlink
mismatch, and absent lock (`.../TERMINAL-VERIFICATION-v13.txt:1-39`).

### 2.4 Reusable fixture custody found locally

The sealed canonical-v2 complete-validation fixture manifest records these
exact inputs (`target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/FIXTURE-MANIFEST-v1.tsv:1-4`):

| Fixture | Bytes | SHA-256 |
|---|---:|---|
| S1-1 | 1,048,576 | `4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a` |
| S1-10 | 10,485,760 | `0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e` |
| S1-100 | 104,857,600 | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |

They are all read-only local files. G4 should reuse these source bytes by hash,
but prepare fresh, version-bound materialization databases and output roots. A
prepared database is inseparable from its exact authority and expectations;
copying only the database recreates the G2-v3 authority failure
(`implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/PROSPECTIVE-G2-MATERIALIZATION-DECOMPOSITION-v5.md:11-31`).

## 3. State dictionary: rows that must not be collapsed

| State | Exact meaning | Authority/performance consequence |
|---|---|---|
| first/full | No verified native representation exists for the requested file root; every mapping and chunk required by the root is authenticated and every output byte is produced | `Theta(S+N+J)`; no clone/cache credit |
| empty destination | Final name/tree is absent before publication | Says nothing about source or SQLite page-cache state |
| warm source | Same source/store bytes were accessed earlier without a cache-control step | Legitimate warm label; may include SQLite and VFS caches |
| fresh process | New process opens the same store | Process state is fresh; OS/device cache is warm-or-unknown |
| reopened | Same as fresh process for evidence purposes when cache is not controlled | Never a synonym for cold (`implementation-detail/evaluation.md:303-314`) |
| controlled cold | A machine-level procedure directly controls the relevant OS buffer/page cache | Unavailable in Round 1; prospective `purge` approximation only |
| protected seed | Exact authenticated file bytes remain under operation-local descriptor authority, read-only and unlinked | G3-qualified same-open clone/patch only |
| trusted persistent seed | Seed survives process/open boundaries under a protected authority that excludes replay, rollback, substitution, and malicious same-UID mutation | Not implemented; cannot be inferred from `0600`, root ID, inode, or mtime |
| incremental | Exact current destination is proven to be the authenticated parent and only authenticated changed ranges/paths are applied | G3 proves one-file same-open/same-count; ordinary directory remains unsolved |
| fallback | Qualification/clone/path capability fails and complete authenticated output is generated | Must consume no single-use fast permit and must have complete first/full counters |

The evaluation contract defines cold, warm, and reopened separately
(`implementation-detail/evaluation.md:303-314`). The retained hot/cold handoff
also separates logical reconstruction, native materialization, empty
destination, protected seed, and controlled cold
(`research/phase-4/handoffs/hot-cold-materialization.md:35-52`).

## 4. Actual current path, end to end

### 4.1 Logical source and authority

The current accepted profile is canonical-v2. A file occurrence stores only
`raw_length[4] || canonical ObjectId[32]`; the profile is versioned and
domain-separated (`crates/layerfs-core/src/canonical_v2.rs:15-31`, `:72-138`).
Each raw chunk is at most 32 KiB (`crates/layerfs-core/src/cdc/mod.rs:11-15`).
Canonical Bytes framing is checked strictly before raw exposure
(`crates/layerfs-core/src/object/codec.rs:64-115`, `:153-165`).

The benchmark-private store borrows a SQLite row BLOB, authenticates the
complete canonical object against its `ObjectId`, charges it, and only then
invokes the consumer (`phase4_create_edit_benchmark.rs:3040-3062`). The accepted
full read uses bounded leaf batching (`:8237-8368`) and validates:

```text
head/receipt/profile
  -> namespace object and singleton file entry
  -> file root and canonical K64/F64 topology
  -> every mapping node and cumulative summary
  -> every selected canonical chunk ObjectId and Bytes framing/raw length
  -> ordered occurrence commitment
  -> ordered closure commitment
  -> output fingerprint and exact total/reference count
```

G2's sealed v1 decomposition, carried by the v5 terminal, observed the following
disjoint work-family medians
(`target/phase4-g2-materialization-decomposition-20260822-v1/results-v1/G2-PRIMARY-ANALYSIS-v1.json:1-1886`):

| Family | Median | Current authority disposition |
|---|---:|---|
| canonical authentication | 94.8165635 ms | required |
| closure commitment | 88.483070 ms | required by current contract |
| source/output fingerprint | 87.8899425 ms | required by current evidence contract; product necessity may be challenged only with replacement authority |
| SQLite BLOB acquisition | 59.4037705 ms | required access, implementation constants researchable |
| second Bytes decode | 0.141476 ms | removable but immaterial |

These families total about 330.735 ms and overlap the observed 338.776-ms warm
complete reconstruction budget only through their exact recorded timer
equation; no gross component is subtracted twice.

The simpler public engine is not the accepted high-performance path. It first
authenticates an object through two full BLOB opens/passes and then opens the
BLOB again for a requested range (`crates/layerfs-engine/src/lib.rs:912-1017`).
That duplicated production path is a reconstruction-lane concern; native G4
must use the accepted benchmark path, not accidentally benchmark the simpler
API and call the result representative.

### 4.2 G3 native path

G3 performs this operation:

```text
fresh fstatat(AT_SYMLINK_NOFOLLOW) destination preflight
  -> head/receipt/store/profile/epoch/open/mutation authority validation
  -> retained directory and unlinked read-only seed descriptor validation
  -> same length/reference-count gate
  -> fclonefileat(seed_fd, dirfd, random private basename)
  -> reopen O_RDWR|O_NOFOLLOW and prove entry/descriptor identity
  -> consume one single-use permit
  -> exact authenticated target range read and pwrite patch, or complete fallback
  -> fsync(file) for data
  -> fchmod(file, target mode)
  -> fsync(file) for metadata
  -> renameatx_np(..., RENAME_NOFOLLOW_ANY)
  -> fsync(parent directory)
  -> on rename/directory-sync ambiguity, fresh exact target then prior compare
  -> final parent sync or typed conflict/ambiguity; remove owned temp
```

The descriptor helpers and clone/open/sync/rename primitives are at
`phase4_g3_materialization.rs:430-612`; clone-temp identity continuity is at
`:679-756`; authority revalidation is at `:1632-1735`; the native operation is
at `:1954-2250`.

The retained v13 raw rows establish exact route counters, not broader
performance:

| Row | Observed native result |
|---|---|
| 10-MiB qualified no-op | 0.993791 ms; one clone; zero payload/auth/reconstruction/patch/fallback |
| 100-MiB one-byte | 3.414166 ms; one clone; 22,551 canonical bytes authenticated; one byte patched |
| 10-MiB one-MiB | 2.926167 ms; 1,086,013 canonical bytes authenticated; 1,048,576 patched |
| complete fallbacks | 1-MiB only; 3.684-4.360 ms; no 100-MiB first/full evidence |
| lost acknowledgement | 3.123083 ms at 1 MiB; full target compare and 56,849-byte reconciliation Q |

Source: `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/rows-v13/G3-V13-RAW.jsonl:1-9` and the exact row table in
`implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md:95-260`.

V11 defects must not recur: fixed reconciliation-buffer Q was omitted, guards
were armed after namespace creation, cleanup could mask publication failure,
and canonical changed-range proof was incomplete
(`.../G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md:35-67`). V12 repaired those
four source defects (`.../v12/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v12.md:15-42`)
but was not executed because five evidence/custody defects remained
(`.../v12/V12-PREEXEC-REVISE.md:8-54`). V13 reran all nine rows and closed both
product and evidence defects.

### 4.3 First/full native file path proposed for G4

For an absent final file under a retained destination directory descriptor:

```text
authenticate head/root and preflight canonical/native name + mode
  -> openat private random temp O_CREAT|O_EXCL|O_RDWR|O_CLOEXEC|O_NOFOLLOW, 0600
  -> existing batched verify_file_inner traversal
       authenticate mapping and bounded canonical chunks
       update closure + occurrence + output commitments
       write raw bytes directly to temp after each chunk authenticates
  -> validate exact root summaries, output length, references, and required commitments
  -> fchmod(target mode) while temp remains private
  -> one final accepted-durability fsync after all data+metadata changes
  -> fstat identity/length/mode
  -> renameatx_np dirfd-relative with NOFOLLOW_ANY into absent final name
  -> fsync retained parent directory
  -> on uncertain publication return, fresh no-follow descriptor/name continuity
     and exact target/prior reconciliation
```

Rust's `File::sync_all` attempts to synchronize content and metadata, while
drop ignores close errors
([Rust `File`](https://doc.rust-lang.org/stable/std/fs/struct.File.html)). For a
private unpublished temp, an intermediate data-only durability point is not
required for correctness if metadata application fails—the temp is deleted and
the old/absent destination stays authoritative. Therefore one final sync after
metadata is a valid **hypothesis**, not an accepted change. G4 should report a
two-sync control and one-final-sync candidate only if it can keep this a true
one-variable comparison. Do not alter the retained G3 two-sync route in place.

The output writer must never expose unauthenticated bytes to the final name.
Per-chunk writes to the private temp are safe because each v2 chunk is bounded
and authenticated before its callback. A later mapping/auth/error removes the
private temp. A full post-publication destination reread is a separate
`verification_wall`; it is `Theta(S)` and must not be silently omitted or
silently included in only one arm
(`implementation-detail/phase-4/algorithm/complexity-analysis.md:810-820`).

### 4.4 Directory/workspace publication

The current G3 path publishes exactly one file. `layerfs-vfs` and `layerfs-sdk`
are five-line stubs (`crates/layerfs-vfs/src/lib.rs:1-5`;
`crates/layerfs-sdk/src/lib.rs:1-5`), and `layerfs-os` only probes the host
(`crates/layerfs-os/src/lib.rs:1-181`). There is no native directory engine or
caller to integrate.

For an empty final workspace, the minimum atomic namespace design is:

1. preflight every canonical name for native case/normalization collision;
2. create one hidden sibling directory relative to a retained parent descriptor;
3. materialize all descendants beneath retained directory descriptors;
4. apply final directory modes bottom-up after child creation;
5. synchronize files and every changed directory bottom-up;
6. rename the complete hidden top-level directory to the absent final name;
7. synchronize the parent of the published top-level name.

For replacement of a nonempty existing workspace, ordinary per-file renames do
not give whole-workspace atomicity. Darwin `RENAME_SWAP` can atomically exchange
two names when the volume advertises support, including file/directory swaps,
but it creates old-tree cleanup and crash-reconciliation work
([Apple `rename(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/rename.2)).
That is later integration, not a G4 one-file shortcut. The portable fallback is
a generation directory plus an atomically renamed small active-generation
pointer/name, or an explicitly non-atomic per-path API. Either changes the VFS/
SDK contract and must be versioned as such.

## 5. macOS/APFS and portable OS contracts

### 5.1 Clone/reflink

Apple specifies that a clone shares source data blocks, subsequent writes are
private CoW, the destination must not exist, unsupported volumes return
`ENOTSUP`, and cross-filesystem clones return `EXDEV`; attributes have explicit
ownership/set-id/ACL exceptions. Directory cloning is strongly discouraged
([Apple `clonefile(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2)).
Consequences:

- Use `fclonefileat` from an already authenticated seed descriptor to a random
  absent basename beneath an already-open destination directory.
- Probe volume clone capability or treat `ENOTSUP`, `EXDEV`, `EEXIST`, `ENOSPC`,
  and I/O errors as typed clone misses. A clone hit can still receive `ENOSPC`
  on a later CoW patch.
- Normalize LayerFS-defined mode after cloning. Do not inherit seed metadata as
  target authority; clone ownership, set-id, ACL, and inherited ACL semantics
  differ.
- `st_blocks`, apparent bytes, and a successful clone syscall do not reveal
  exclusive/shared physical extents or media traffic. Count clone calls and
  `COPYFILE_STATE_WAS_CLONED` when using `copyfile`, but keep physical sharing
  `Unavailable` without a direct supported observer.

Apple `copyfile(..., COPYFILE_CLONE)` is a useful path-based best-effort
clone-then-copy fallback, while `COPYFILE_CLONE_FORCE` exposes clone failure and
`COPYFILE_STATE_WAS_CLONED` reports whether cloning occurred. It also copies
stat/xattr data by default and does not support forced directory cloning
([Apple `copyfile(3)`](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.3)).
LayerFS should prefer explicit `fclonefileat` plus its own descriptor-relative
copy/reconstruction fallback so authority, mode, bytes, and counters are not
hidden inside a path-based convenience call.

Portable hierarchy:

```text
macOS/APFS: fclonefileat -> on typed miss, authenticated streamed output to dest-volume temp
Linux reflink FS: FICLONE/FICLONERANGE -> typed miss -> copy_file_range -> bounded read/write
generic POSIX: bounded read/write or direct CAS reconstruction -> fsync -> renameat -> dir fsync
Windows: CreateFile handles with reparse-point policy -> bounded copy -> FlushFileBuffers -> ReplaceFile/MoveFileEx under an explicit platform contract
```

Linux exposes clone/remap only through filesystem-specific VFS support; CoW and
same-filesystem requirements remain
([Linux VFS](https://docs.kernel.org/filesystems/vfs.html),
[Linux man-pages `FICLONE`](https://www.kernel.org/pub/linux/docs/man-pages/book/man-pages-6.06.pdf)).
Do not call `copy_file_range` a reflink unless a direct mechanism counter proves
remapping.

### 5.2 Descriptor-relative safety

POSIX explains that `openat` and `renameat` exist to pin the target directories
and avoid current-working-directory/path replacement races; `O_NOFOLLOW`
rejects a final symlink
([POSIX `open/openat`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html),
[POSIX `rename/renameat`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html)).
Darwin additionally provides `O_NOFOLLOW_ANY`, `CLONE_NOFOLLOW_ANY`,
`CLONE_RESOLVE_BENEATH`, `RENAME_NOFOLLOW_ANY`, and
`RENAME_RESOLVE_BENEATH` in the current SDK/manpages. G3 uses a numeric
`RENAME_NOFOLLOW_ANY` but does not use the newer resolve-beneath flag
(`phase4_g3_materialization.rs:602-612`).

Minimum path contract:

- validate canonical components (`.`, `..`, slash, NUL, depth, and native
  collision policy) before any visible mutation;
- retain the workspace parent and each created directory fd;
- use relative single-component names, `O_EXCL`, `O_NOFOLLOW`/`O_NOFOLLOW_ANY`,
  and post-open `fstat` identity/type checks;
- never chmod, hash, patch, rename, or unlink through a re-resolved absolute
  path;
- hold cleanup ownership before each namespace-creating syscall, as v12 repaired;
- on platforms lacking resolve-beneath/no-follow-any, walk one component at a
  time through retained fds and reject symlinks at each step.

### 5.3 Sparse output and preallocation

Darwin `F_PUNCHHOLE` deallocates an aligned region, preserves logical file size,
and makes reads return zeros; support is filesystem-specific. `F_PREALLOCATE`
can request contiguous/all allocation and reports `fst_bytesalloc`, but the
filesystem may ignore some requests
([Apple `fcntl(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/fcntl.2)).
Apple `COPYFILE_DATA_SPARSE` requires an already sparse source and can fall back
to a full copy when combined with `COPYFILE_DATA`
([Apple `copyfile(3)`](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.3)).

Neither is a default G4 optimization:

- preallocation changes allocation timing, can improve fragmentation, and can
  fail early with `ENOSPC`, but removes no canonical authentication or logical
  destination bytes;
- sparse output helps only for authenticated block-aligned zero runs; detecting
  and punching holes has its own CPU/syscall cost and must retain exact logical
  length;
- a successful sparse/preallocation call and `st_blocks` are allocation
  observations, not physical-media I/O or clone-sharing proof.

### 5.4 Durability and stable media

The retained G3 accepted-durability sequence is data `fsync`, mode change,
metadata `fsync`, rename, and directory `fsync`. POSIX directory operations are
atomic/serializable but not necessarily durable; its rationale recommends
synchronizing the new file before rename and the directory when the new name
must survive a crash
([POSIX durability rationale](https://pubs.opengroup.org/onlinepubs/9799919799/xrat/V4_xbd_chap01.html)).

Darwin says ordinary `fsync` moves host buffers to the drive, but the drive may
still reorder or delay physical persistence; `F_FULLFSYNC` asks it to flush all
buffered data to permanent storage
([Apple `fsync(2)`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/fsync.2)).
Therefore G4 must name one of two contracts:

| Contract | Required calls | Claim boundary |
|---|---|---|
| retained accepted durability | file `fsync`; atomic rename; parent-directory `fsync` | same contract family as G3; stable media remains unavailable |
| macOS strict flush experiment | checked `F_FULLFSYNC` at the final file boundary plus separately specified directory handling | separate durability-policy row; not comparable to retained targets without an explicit control |

Do not infer device stable-media completion from ordinary `fsync`, wall time,
block-operation counters, or absence of a temp. Reconciliation can establish
which exact namespace/bytes are visible after an ambiguous return; it cannot
prove power-loss persistence beyond the chosen sync primitive.

## 6. Controlled-cold disposition

### 6.1 What exists on this host

- `/usr/sbin/purge` exists, is owned by root, and is not setuid.
- Apple documents `purge` as flushing/emptying the disk cache to
  **approximate** cold disk-buffer-cache conditions, not device-cache cold
  ([Apple `purge(8)`](https://github.com/apple-oss-distributions/system_cmds/blob/main/purge/purge.8));
  its implementation calls the global `vfs_purge()` syscall
  ([Apple `purge.c`](https://github.com/apple-oss-distributions/system_cmds/blob/main/purge/purge.c)).
- Darwin exposes `F_NOCACHE` per file descriptor, and the disposable probe saw a
  successful return. It is an uncached-I/O policy, not proof that an ordinary
  cached path began cold.
- The macOS SDK contains no `posix_fadvise`/`POSIX_FADV_DONTNEED`. Even on Linux,
  `DONTNEED` is advisory/attempted eviction rather than a universal cold proof
  ([Linux `posix_fadvise(2)`](https://man7.org/linux/man-pages/man2/posix_fadvise.2.html)).

### 6.2 Round-1 and prospective G4 labels

Round-1 disposition: `controlled_cold=Unavailable(shared_host; global purge not invoked; no device-cache observer)`.

Prospective exclusive-host procedure:

1. Acquire one repository benchmark lock and machine exclusivity; connect AC
   power, record thermal/power state, and reject competing I/O/build processes.
2. Verify immutable source/database/authority/expectation hashes and construct
   destination paths **before** cache conditioning. These reads may warm data.
3. Complete pending writes (`sync`/exact prerequisite sync), then invoke
   `/usr/sbin/purge`; require exit 0 and record its wall outside the row timer.
4. Immediately start exactly one no-warmup row. Do not rehash or reopen the
   source for preflight between purge and the timer.
5. Record source/store/destination volume IDs, operation cache policy, and direct
   process/VFS counters. Label the result
   `controlled_cold_buffer_cache_approximation`; label device/controller/media
   cache and physical bytes `Unavailable` unless a direct observer exists.
6. Any inability to establish exclusivity or purge successfully makes the cell
   `Unavailable`. It never falls back to `fresh-process=cold`.

An `F_NOCACHE` campaign may be useful as a separately named `uncached_fd` stress
path, but SQLite and all mapping/object opens must actually use it. Its results
cannot populate the ordinary-path controlled-cold cell.

## 7. Storage, cache, history, concurrency, and VFS consequences

### 7.1 Operation-local protected seed

G3's unlinked read-only seed consumes one full native representation only while
its fd is live. It is immune to path substitution after unlink but dies with the
process/open. Its exact storage is reported separately from clone temp storage.
No persistent GC, migration, or eviction system is implied.

**Cross-review answer:** no OS primitive found in the present macOS/product
path preserves LayerFS's exact seed-byte authority across a true broker restart
without either keeping an already authenticated kernel object alive or
reauthenticating the bytes. An unlinked authenticated fd can be transferred to
another *already live* process (for example with Unix-domain descriptor
passing), and the open file remains usable while at least one holder survives;
that is a live descriptor-capability handoff, not persistent authority. When
the last holder exits there is no pathname to reopen. Keeping a named file
instead permits replacement, rollback, write-through-an-existing-fd, storage
corruption, and privileged mutation unless a separate protection domain and
rollback-resistant authenticated catalog are introduced. Owner mode `0600`,
BSD immutable-looking flags, inode/generation/timestamps, clone provenance,
receipt, and root ID do not exclude a malicious same-UID actor; owner-changeable
flags in particular are not an integrity boundary. This is an inference from
the current same-UID architecture and Darwin capabilities, not a universal
impossibility claim. A different-UID/privileged broker plus access control and
authenticated durable catalog could be a future architecture, but that is not
one primitive, is not implemented, and still needs defined recovery,
rollback, and compromise semantics. Darwin exposes the relevant descriptor and
flag syscalls but supplies no LayerFS identity binding by itself
([Apple XNU syscall table](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/syscalls.master),
[Apple XNU file flags](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h)).

### 7.2 Persistent bounded native seed cache

A later cache may be worthwhile, but only as derived acceleration:

```text
key = profile/version + authenticated file byte identity
value = one exact raw native file, private and immutable under cache authority
capacity = C allocated bytes + U entry/metadata bound
history growth = min(sum(unique admitted seed allocation), C), not revisions * S
```

The canonical-v2 file root is the simplest key but includes mode; a separately
authenticated ordered `(raw_length, canonical ObjectId)` commitment could share
byte seeds across metadata-only roots. Inventing that second key without making
it authenticated product state is unsafe. A cache must define:

- capacity in allocated bytes, entry count, and open-fd count;
- deterministic LRU/clock eviction under a lock, with unlink-only-after-open-fd
  handoff semantics;
- temp-create, complete authentication, file sync, atomic cache publication,
  and cache-directory sync;
- corruption quarantine and full rebuild from canonical truth;
- one builder per key or duplicate-build disposal; no source-sized queue;
- exact cross-process authority. `0600`, immutable-looking mode, inode, mtime,
  or same UID does not exclude malicious same-UID mutation;
- backup/export behavior: cache is disposable and never required to interpret a
  canonical root.

With 10/100/1,000 revisions, unbounded per-revision seeds are `Theta(R*S)` and
rejected. Content-addressing plus capacity `C` bounds derived storage, but a
history of unique files reaches `C` and churns. APFS destination clones may
share extents initially, then allocate privately on edits; logical/apparent/
allocated values must all be reported, and none proves physical sharing.

### 7.3 Concurrency

The current store has one SQLite connection and synchronous caller-thread
execution. G3 permits are operation-local and single-use. A persistent cache
would add a second concurrency domain:

- one per-key build lock and one short index/eviction lock;
- no lock held across full reconstruction when duplicate private builders can
  safely race and one loses at atomic publish;
- deterministic cancellation: unpublished temp removed; already published
  cache seed remains valid; destination publication independently reconciled;
- eviction must never revoke a retained source fd during `fclonefileat`;
- store writer transaction/COMMIT and native destination publication remain two
  distinct authorities, never one pseudo-atomic transaction.

No cache or worker pool belongs in G4 acceptance. Measure single-operation G4
first; concurrency/history remains G5/later integration.

### 7.4 VFS/projection

The crate topology is `core -> engine + os -> vfs -> sdk`, but VFS and SDK have
no APIs yet (`Cargo.toml:1-19`; `crates/layerfs-vfs/Cargo.toml:8-14`;
`crates/layerfs-sdk/Cargo.toml:8-12`). G4 should exercise a benchmark-private
native boundary and leave production integration false.

A later VFS should serve reads directly from authenticated mapping/range paths,
not materialize a complete intermediate file merely to satisfy `read(offset,
len)`. Native full checkout and lazy projection are different products. Apple
File Provider can hydrate placeholders on access, but that is not exact full
materialization until all files exist and verify. The current algorithm already
protects range locality; do not trade it for a whole-file seed requirement.

## 8. Candidate rows

The fields below match the required Round-1 candidate schema. Values are tagged
Observed, Derived, Hypothesis, or Unavailable.

### C1 — batched one-pass authenticated native writer

| Field | Required content |
|---|---|
| Mechanism | Add a benchmark-private `Write` sink to accepted `verify_file_inner`; keep leaf batching, canonical authentication, closure/sequence/output commitments and summaries; emit authenticated chunks directly to a private temp; metadata -> sync -> rename -> directory sync -> reconciliation. Removes a hypothetical intermediate reconstruction buffer/pass and avoids G3's per-object fallback query shape. |
| Target paths | First/full and fallback native materialization; reconstruction logic shared. Range/create/edit/reopen unchanged. Future VFS can reuse the authenticated traversal, not the native temp. |
| Complexity | Before accepted logical read: `Theta(S+N)` to hash sink. Naive native: `Theta(S+N) + Theta(S)` second copy. Candidate: one `Theta(S+N)` authenticated traversal plus `Theta(S)` kernel destination writes in the same pass; span remains synchronous. Memory `O(max chunk + mapping frames + bounded SQLite/result batch + writer buffer)`. |
| Measured ceiling | Observed warm logical reconstruction 338.776 ms; fresh process 366.357 ms with cache warm-or-unknown. G2 work families: 94.817 auth + 88.483 closure + 87.890 fingerprint + 59.404 BLOB + 0.141 decode ms. Native write/sync wall is Unavailable. |
| Predicted gain | Not a speedup over logical reconstruction; it supplies missing native work with no extra full userspace pass. For warm 100 MiB `T_native = 338.776 + W_native+sync+publish - overlap`; meeting 400 ms requires net native overhead `<=61.224 ms`. Controlled-cold 500-ms objective leaves `<=161.224 ms` over the warm logical prior but is not a prediction. |
| CPU | Retains all required current hashes/folds; adds write syscalls/copies. No workers. User/system CPU, instructions/cycles where available, syscalls, and core span must be direct counters. |
| Memory/Q | No full-file buffer. Proposed owned Q: accepted mapping/auth Q + <=1 MiB output buffering; application RSS gate <=20 MiB; SQLite cache reported separately; terminal Q=0. |
| Storage | Empty destination: one temp of logical length S becomes final by rename; no simultaneous retained duplicate. Record logical/apparent/allocated temp and final. Fault before rename removes temp. History growth zero beyond requested destination. |
| Authority | Current head/receipt/profile; complete canonical-v2 mapping/chunk auth; exact closure/sequence/output/summary checks; no final-name exposure before all gates. Fixture digest is correctness evidence, not product seed authority. |
| Durability | Explicit data/metadata sync policy; atomic no-follow rename; parent-dir sync; exact target/prior reconciliation; ordinary fsync does not prove stable media. |
| Identity/format | No identity, mapping, schema, receipt, or profile change. Benchmark-private only. |
| Cross-operation effect | Should leave create/edit/range/reopen/G3 binaries byte-identical. Shared traversal change must prove reconstruction output/counters exact. Do not replace G3 fast route. |
| Experiment | First run one separate lock-held <=120-s candidate build/screen containing build, 1/10-MiB semantic/resource probes, analysis, cleanup, and source restoration; freeze the passing candidate binary/source/static identities. The later main campaign, with no build, contains its fixed 1/10-MiB smokes and one 100-MiB warm-source/empty-destination primary. Compare hash sink and native writer within candidate custody; retain separate G3 `M0-control` custody; record direct query/BLOB/auth/output/sync/rename/dirsync/RSS/Q/storage counters and a separate post-publication exact-verification timer. |
| Evidence | `phase4_create_edit_benchmark.rs:8237-8368`, `:9038-9177`; `phase4_g3_materialization.rs:1954-2250`; G2/G3 anchors above; Rust and POSIX/Apple sync/rename sources above. |
| Disposition | **DO NOW / G4 first native baseline.** Kill if it loses leaf batching, omits a current proof fold, emits unauthenticated final bytes, exceeds RSS/Q/storage gates, has any residue/ambiguity, or warm 100-MiB wall >400 ms. |

### C2 — retain and qualify protected-seed clone/patch

| Field | Required content |
|---|---|
| Mechanism | Retain exact G3-v13 same-open unlinked read-only seed, `fclonefileat`, range-authenticated patch, two-sync publication, fallback, and reconciliation. Measure the G4 matrix without source changes. |
| Target paths | Protected-seed no-op/new destination, one-byte/1-MiB same-size incremental, count-change/invalid-authority/external-mutation/clone-miss fallback. No fresh-process trusted seed or ordinary destination trust. |
| Complexity | Qualified clone no-op `O(1)` file syscalls for one file plus namespace/durability; patch `O(authenticated selected mapping/chunks + changed bytes)`; fallback `Theta(S+N)`. Directory tree remains `O(J)` file clones/metadata. |
| Measured ceiling | Observed v13: 10-MiB no-op 0.994 ms; 100-MiB one-byte 3.414 ms; 10-MiB 1-MiB 2.926 ms. Single rows, warm/cache unknown, operation-local, not G4 acceptance. |
| Predicted gain | Objective remains <=10 ms for protected-seed same-root clone and one-byte; no invented clone throughput. Compare wall and calls, not logical length / wall. Full fallback predicted by C1, not v13's 1-MiB rows. |
| CPU | Small authority/hash/tag work plus selected-range authentication; no complete payload CPU on qualified rows. Fault reconciliation may be full `Theta(S)`. |
| Memory/Q | G3 max RSS 16.679 MiB; row Q <=1.060 MiB with terminal 0. G4 retains <=20 MiB RSS and exact reconciliation buffer Q. |
| Storage | One operation-local seed S plus cloned temp/final. Logical/apparent may look like 2S; shared physical extents Unavailable. No persistent history. |
| Authority | Exact v13 binding: store, validation authority, profile, epoch, generation, receipt/transition, parent/target roots, directory/destination/open/mutation/publication identities, operation, nonce, seed and canonical range proof. |
| Durability | Exact v13 file data sync, metadata sync, no-follow rename, dir sync, target/prior reconciliation, cleanup/error precedence. |
| Identity/format | No format change; macOS/APFS platform mechanism with complete portable fallback. |
| Cross-operation effect | Protect create/edit/range/reopen because G4 uses frozen existing candidate. Count change remains fallback. Production/VFS false. |
| Experiment | <=120 s compact G4 rows: 10-MiB clone smoke; one 100-MiB no-op clone and one-byte patch primary; focused 1-MiB fault/fallback rows. Kill on any v13 counter/equation mismatch, >10 ms qualified 100-MiB objective, RSS >20 MiB, residue, or missing exact fallback. |
| Evidence | G3 v13 raw lines 1-9, report lines 1-350, module lines 430-2250, Apple clonefile contract. |
| Disposition | **DO NOW / G4 protected-seed scoreboard.** No cross-process claim. |

### C3 — clone-miss/cross-volume portable fallback

| Field | Required content |
|---|---|
| Mechanism | On `ENOTSUP`, `EXDEV`, `EEXIST`, clone reopen/identity failure, or disabled capability, create the temp on the destination volume and use C1's complete authenticated CAS-to-native stream. Do not create the temp beside the source seed and then cross-volume rename. |
| Target paths | First/full and every clone/fresh-process/cross-volume miss. |
| Complexity | `Theta(S+N)` authentication + `Theta(S)` destination writes; one destination-volume temp; no source-sized memory. |
| Measured ceiling | G3 tests prove clone miss chooses fallback without consuming the permit, but only at 128 KiB (`phase4_g3_materialization.rs:2720-2765`). 100-MiB fallback/native wall Unavailable. |
| Predicted gain | No gain claim. Protected regression objective: warm <=400 ms, controlled-cold buffer approximation <=500 ms. It is the correctness floor that makes clone optional. |
| CPU | Full current authentication/folds plus output system CPU. No copyfile-hidden fallback counters. |
| Memory/Q | Same as C1. Copy-from-seed variant, if separately tested, uses one <=1 MiB buffer and reauthenticates the resulting exact output before trust. |
| Storage | Exactly one destination-volume temp/final; no cross-volume temp residue; allocated storage up to S. |
| Authority | Clone failure never launders seed/destination authority; no permit consumed; complete canonical truth regenerates output. |
| Durability | Same C1 publication/reconciliation. Cross-volume `rename` is forbidden/`EXDEV`; final rename stays within destination parent. |
| Identity/format | Portable, no format change. Platform clone errors map to a stable fallback reason plus underlying diagnostic. |
| Cross-operation effect | Ensures APFS optimization never becomes a platform requirement. Range/edit/create remain unchanged. |
| Experiment | <=120 s: force clone-disabled without requiring a second volume, then one 10-MiB smoke and one 100-MiB full fallback. A real cross-volume row is required later only when a disposable second filesystem is available. Kill if permit consumed, temp on wrong volume, counters hide full work, wall misses protected bound, or exact state/residue fails. |
| Evidence | G3 `clone_temp` `:679-756`, operation `:2032-2081`, clone-miss test `:2720-2765`; Apple clone/rename `EXDEV`. |
| Disposition | **DO NOW / G4 fallback gate.** |

### C4 — bounded persistent content-addressed native seed cache

| Field | Required content |
|---|---|
| Mechanism | Store at most C allocated bytes of fully authenticated raw file seeds keyed by profile + authenticated file byte identity; atomic cache publication, bounded eviction, corruption quarantine/rebuild; clone into same-volume destinations. |
| Target paths | Repeated new destinations and possibly trusted full reads after process restart. No benefit to first miss. |
| Complexity | Hit: `O(lookup + J clone/metadata)`; miss: C1 `Theta(S+N+J)` plus cache publication. Storage `O(C+U metadata)`, not `O(revisions*S)`. |
| Measured ceiling | Gross avoidable logical path 338.776 ms warm; v13 operation-local clone observations. Persistent lookup/revalidation/eviction and first-miss wall Unavailable. |
| Predicted gain | A trusted hit could avoid most of 338.776 ms for materialization; final wall includes lookup/clone/metadata/sync/publication. Trusted-seed full **read** <=50 ms is plausible only with valid cross-process authority and favorable cache state; otherwise full auth is `Theta(S)` and objective is rejected. |
| CPU | Hit avoids SQLite/auth hashes; periodic seed revalidation/rebuild consumes full CPU and must be charged to an explicit maintenance/miss path. |
| Memory/Q | Index bounded by U entries/bytes; no decoded payload cache; per-operation fd/buffer bound; aggregate concurrency cap. |
| Storage | Capacity C hard limit; exact logical/apparent/allocated seed and clone accounting; deterministic eviction; 10/100/1,000-revision plateau required. |
| Authority | Requires exclusive service/root-owned authority, rollback protection, or equivalent. Same-UID `0600` files and receipts are insufficient. Cache never becomes canonical truth. |
| Durability | Seed temp auth -> sync -> cache rename -> cache dir sync before hit eligibility. Destination publication remains separate. |
| Identity/format | Derived-cache index/schema may be disposable; a new content-only file commitment would be version/profile state. No silent G4 integration. |
| Cross-operation effect | Potential read/materialization upside; extra create/miss work and storage. Must protect <=5% create/edit/range/reopen. |
| Experiment | Later <=120 s miss/hit screen with 1/10/100 files, explicit C, corruption and eviction. Kill if hit revalidates `Theta(S)`, cache exceeds capacity, first miss/create regresses >5%, or authority is same-UID mutable. |
| Evidence | Hot/cold handoff `:191-242`; G3 seed creation `:1303-1383`; Phase-3 unchanged identity contract `implementation-detail/phase-3.md:90-110`. |
| Disposition | **LATER PROFILE / ARCHITECTURE**, after authority and G4 native baseline. |

### C5 — sparse zero extents and preallocation

| Field | Required content |
|---|---|
| Mechanism | Optional authenticated aligned-zero hole punching; independently optional destination-volume preallocation. Never stack them in one causal screen. |
| Target paths | Zero-heavy first/full materialization; preallocation possibly large full fallback. |
| Complexity | Authentication stays `Theta(S+N)`; sparse physical writes potentially proportional to nonzero extents plus discovery; preallocation changes constants only. |
| Measured ceiling | Zero density and allocation wall Unavailable. Static probe saw API capability only. |
| Predicted gain | None without representative zero density or extension attribution. |
| CPU | Zero scan/extent coalescing and fcntl syscalls add work; preallocation negligible CPU but may force allocation. |
| Memory/Q | One bounded block/extent accumulator only. |
| Storage | Sparse logical length S with allocation dependent on FS; preallocation deliberately allocates S. Report both. |
| Authority | Zero holes are permitted only after the exact raw bytes authenticate and every punched byte is known zero. |
| Durability | Same publication/sync; hole/preallocation errors are typed fallback or failure before publication. |
| Identity/format | No logical format change, platform-specific representation. |
| Cross-operation effect | No read/range benefit guaranteed; possible fragmentation/storage effect; no seed authority. |
| Experiment | Only after density/extension counters: 10-MiB zero-heavy synthetic with one variable, then one 100-MiB if direct signal. Kill below 20% eligible zero bytes, no allocated-byte change, >5% CPU/wall regression, or unsupported API. |
| Evidence | Static falsifier below; Apple fcntl/copyfile sources. |
| Disposition | **DEFER**, not G4 acceptance. |

### C6 — trust ordinary destination receipts or fresh-process metadata

| Field | Required content |
|---|---|
| Mechanism | Use receipt, inode, length, mtime/ctime, watcher hints, or process restart as proof that an ordinary destination/seed still equals a root. |
| Target paths | Warm no-op/incremental/fresh-process. |
| Complexity | Claimed O(1), but honest exact verification is `Theta(S+J)` without exclusive authority. |
| Measured ceiling | Attempt A static NO-GO; no production authority. |
| Predicted gain | Invalid because it deletes required trust work. |
| CPU | Appears small only by omitting byte authentication. |
| Memory/Q | Irrelevant. |
| Storage | Sidecars add replayable state without authority. |
| Authority | Fails malicious/out-of-band mutation, event loss, rollback, and same-UID threat. |
| Durability | Receipt cannot settle lost acknowledgement or power-loss state. |
| Identity/format | Would weaken current correctness contract. |
| Cross-operation effect | Security/correctness regression, forbidden regardless of speed. |
| Experiment | None; static adversary proof already refutes it. |
| Evidence | `G3-REPORT.md:15-33`; `hot-cold-materialization.md:153-189`; evaluation cold rules. |
| Disposition | **REJECT.** |

## 9. G4 falsifier package from this lane

Two fail-fast wrappers use the exact global lock. First, a separate <=120-s
candidate build and 1/10-MiB screen completes analysis, cleanup, and source
restoration, then freezes a passing candidate identity. The later no-build
30-row measured campaign independently remains <=120 s. Its smallest useful
order is:

1. C1: 1- and 10-MiB native-writer correctness/resource smokes; one 100-MiB
   warm-source/empty-destination primary; post-publication exact verification
   separately timed.
2. C2: one 10-MiB clone/no-op smoke; one 100-MiB clone/no-op and one-byte
   primary. Retain exact v13 source/binary as the separate `M0-control`; use the
   already-screened candidate identity for candidate routes. Do not build in
   the main campaign.
3. C3: forced clone-disabled 10-MiB smoke and one 100-MiB fallback, preferably
   sharing the C1 full-output implementation so it is not a third mechanism.
4. Focused 1-MiB invalid-authority, external-mutation, count-change,
   before-publication, lost-ack, and symlink/wrong-kind rows. Do not spend 100
   MiB on every fault.
5. Controlled cold: at most one 100-MiB C1 row after exclusive-host purge. If
   exclusivity/purge is unavailable, emit `Unavailable`; do not replace it.

Mandatory direct fields: state labels, root/receipt/profile, source/destination
volume, SQL queries/rows/BLOBs, canonical/closure/sequence/output bytes and
objects, output writes/calls, clone/copy/patch/fallback calls and bytes,
file/metadata/directory sync calls/wall, rename/reconciliation, exact
verification bytes/wall, user/system CPU, supported instructions/cycles, RSS,
SQLite cache, Q high/terminal, temp/final/seed logical/apparent/allocated bytes,
residue, and explicit physical-I/O/cache/stable-media limitations.

## 10. Disposable experiment ledger-ready block

### Experiment M1 — APFS clone/sparse/preallocation/cache-policy capability

| Ledger field | Record |
|---|---|
| Classification | Historical/inadmissible syscall/API capability scratch compiled before the frozen global lock; **not timing/performance/cold evidence** |
| Hypothesis | On this exact APFS volume, `fclonefileat` can clone an unlinked read-only fd and retain CoW independence; `F_NOCACHE`, sparse length, and `F_PREALLOCATE` are callable. One variable family: native API capability/return and direct stat state. |
| Namespace | `/tmp/layerfs-g4-r1-materialization-capability.t3AFdJ` on device 16777232/APFS; unique; absent before; removed after |
| Source | 3,528-byte C probe created with `apply_patch`; SHA-256 `ad178b73d4fda7a50fc027a36663d3fbb7a68d78d8a468fb768f0523adf288d9`; removed, with bytes no longer retained or reconstructable from this package |
| Binary | `/usr/bin/clang -Wall -Wextra -Werror`; 34,456 bytes; mode 0500; SHA-256 `0210b3fb9f1d82da4b234ea6a931b9e8ec5f0e3f436ccef3e82d58eecc05bb79` |
| Exact measured command | `set -euo pipefail; experiment_root=/tmp/layerfs-g4-r1-materialization-capability.t3AFdJ; start_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ); start_mono=$(python3 -c 'import time; print(time.monotonic_ns())'); /usr/bin/clang -Wall -Wextra -Werror "$experiment_root/probe.c" -o "$experiment_root/probe"; /bin/chmod 0500 "$experiment_root/probe"; "$experiment_root/probe" "$experiment_root"; end_mono=$(python3 -c 'import time; print(time.monotonic_ns())'); end_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ); /usr/bin/shasum -a 256 "$experiment_root/probe.c" "$experiment_root/probe" "$experiment_root/clone" "$experiment_root/sparse" "$experiment_root/prealloc"; /usr/bin/stat -f 'path=%N mode=%Sp size=%z blocks512=%b dev=%d inode=%i' "$experiment_root" "$experiment_root/probe.c" "$experiment_root/probe" "$experiment_root/clone" "$experiment_root/sparse" "$experiment_root/prealloc"; printf 'start_utc=%s\nend_utc=%s\nstart_monotonic_ns=%s\nend_monotonic_ns=%s\nwall_ns=%s\n' "$start_utc" "$end_utc" "$start_mono" "$end_mono" "$((end_mono-start_mono))"` |
| Measured-command start/end | UTC `2026-08-22T06:22:32Z` / `2026-08-22T06:22:34Z`; monotonic `1146805540177958` / `1146807468332166` ns |
| Compile/probe timer | `1,928,154,208 ns`, covering compile, `chmod`, and probe only because `end_mono` preceded `shasum` and `stat`; hashing, stat collection, and cleanup are excluded, so this is not the complete experiment wall |
| Full experiment-through-cleanup wall | Exact wall `Unavailable`: no cleanup monotonic timestamp was captured. Start UTC was `2026-08-22T06:22:32Z`; successful final-cleanup UTC was `2026-08-22T06:22:57Z`. Because both stamps have one-second resolution, the conservative elapsed upper bound is `<26 s`, which proves the entire experiment including cleanup was `<=120 s`. |
| Inputs | Deterministic 4-MiB repeated 64-KiB pattern; seed `fsync` then `F_NOCACHE=1`, unlink while fd live; same-volume clone; one clone byte patch+fsync; 4-MiB sparse `ftruncate` then one byte+fsync; 4-MiB `F_PREALLOCATE` then `ftruncate` |
| Raw result | `seed-linked size=4194304 blocks512=8192 inode=734666560 nlink=1`; `f_nocache_enable=success`; `seed-unlinked ... nlink=0`; `clone-before-write size=4194304 blocks512=8192 inode=734666561`; `cow_seed_first=17 cow_clone_first=165 independent=true`; `clone-after-write ... blocks512=8192`; `sparse-truncated size=4194304 blocks512=0`; `sparse-one-byte size=4194304 blocks512=8192`; `preallocated size=4194304 blocks512=8192`; `f_preallocate=success first_contig_errno=0 bytesalloc=4194304` |
| Output hashes | clone `32b5296b0474973598987a4fb7a7acf06235d183ccba4d43510fb9098738ca8f`; sparse `e51c132c66a07f8e76e0cf43a91ee5be9045d5ed48e182a836598b909195bdc8`; preallocated `bb9f8df61474d25e71fa00722318cd387396ca1736605e1248821cc0de3d3af8` |
| Resource model | Peak live logical regular data <=16 MiB (unlinked 4-MiB seed + three 4-MiB named files) plus 38 KiB source/binary; named post-process data 12 MiB; observed named allocation about 12 MiB plus binary/source; <=512 MiB; retained 0 bytes |
| Retain rule | Retain mechanism as a G4 capability only if direct API success and CoW byte independence; PASS |
| Unsupported observations | No performance wall per syscall; no cross-volume operand; no clone capability bit query; no physical/shared-extent/media bytes; no device cold; `st_blocks` does not prove sharing; one sparse outcome does not generalize |
| Cleanup | First cleanup request was rejected before process creation by the command safety guard; second attempted `/usr/bin/unlink`, which does not exist, and deleted nothing. Final exact cleanup used `/bin/unlink` on the five named files then `/bin/rmdir` on the one namespace. `cleanup=PASS`, UTC `2026-08-22T06:22:57Z`; namespace absent. No broad/glob/recursive deletion. |

Observed implication: APFS capability is present on this volume and matches the
retained G3 test, but the experiment supports no throughput, cache-cold, clone
sharing, or physical-I/O statement. The one-byte sparse allocation outcome is a
warning that “logical hole” is not a guaranteed small allocated result after a
write. Custody is limited to exact source/binary/output hashes, exact command
text, and the embedded direct-result subset above. The deleted `probe.c` bytes
and complete `stat` stdout were not retained and cannot be reconstructed from
the current documents; only the conservative cleanup-inclusive `<26 s` bound
establishes total-duration compliance.

## 11. Local citation and source-hash ledger

Every repository/source/evidence file inspected by this lane is listed below.
`Lines` identifies the relevant reviewed/cited region; `all` means the complete
small document/artifact or the whole line-oriented JSON/TSV record was read.
The experimental C source/binary hashes are in section 10 and were removed.

### 11.1 Governing, design, research, and dependency inputs

The attachment is outside the repository; all other paths in this subsection
are relative to repository root. “Focused” means the listed lines were the
decision-bearing part, while the surrounding document was searched for the
same concepts.

| File | Lines reviewed/cited | SHA-256 |
|---|---|---|
| `/Users/yifanxu/.codex/attachments/ce13c3ad-f2bf-4e43-b20a-4cb846283dc2/pasted-text.txt` | all | `fa44b9550988bc9278206852d0ef1705add027b35a5771f4c208d59a63e623fe` |
| `implementation-detail/evaluation.md` | focused `117-162`, `270-314`, `424-483` | `067f4107b886a504511475f0977b269016d233b6186a0de70b1a5681460c46c3` |
| `implementation-detail/phase-3.md` | focused CoW/delta/storage requirements | `c27b6cb030aac3edaf4ed949498139c01a9ec94738f3f3c7b8d7d2041d356443` |
| `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md` | focused G2/G3/G4 gates | `03ca46e7772c63a9f39eaa50275edd82a0e5ece50fc1c0aff00b4a21bd8db304` |
| `implementation-detail/phase-4/README.md` | focused current status and next gate | `a5dc635898e53939e34e135471bffc22d6361babeb7d90a48e38678f4a67c830` |
| `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md` | focused retained materialize/reopen rows | `0cafb37d4d44659d226dae51d8ae7243612e628b4b3f943c540992393668d1de` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md` | `1-33`, `95-260`, `290-335` | `5748a36b9be0e2d21771483b1bc838804d47bc95801681df0863cb7c40caf462` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md` | `1-100` | `8226aacee217a58436b2c8405d953ee18882e5ad400662f1004368a91a26dae5` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v12.md` | focused `1-100` and repair contract | `39a081a185aa4560e60f5d6a862c47e0f13d9ac2d67ac769f6676a1238f8ecf8` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v12/V12-PREEXEC-REVISE.md` | `1-80` | `13d7bd160b730285ba4457fcabc0107c8064ed6c63bdf9a1cfc84e275596e2c8` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v13.md` | all contract sections | `70a8fedfa97a03ea56031cb06b033593d1595b7558c986ee625deab40ea33fee` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/COUNTER-DICTIONARY-v13.md` | all counter/state definitions | `8809034ee8fff0013eb622799a9c676e14c8a102ec5557172f121d7a0434fe58` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/DRY-RUN-v13.json` | all | `f20a3a562c6c83afed5228568abd681f06e2369b7c0b3883b5b10fa3bec17ca4` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/run_g3_v13.py` | all campaign/custody/cleanup logic relevant to G4 reuse | `1aa960ce75bae2a69ae3f3f73b4e1b2cbe01baad841b5095714890118491e915` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/analyze_g3_v13.py` | all acceptance equations | `b1121f44b29d991f7212153e4a26c841db320045bab5a57e604919bafd677c33` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/recompute_g3_v13.py` | all independent equations | `146c73f6adc43c3de00c8a1d14ad77b7ec83732d858c0afb524df2a8a46fd6c5` |
| `implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/finalize_g3_v13.py` | all terminal/manifest closure | `b0c11b720e9d1aa56e66c4eded6ac37c5525ad7652b1f083c733d00dfe199006` |
| `implementation-detail/phase-4/experiments/g2-materialization-decomposition/v5/PROSPECTIVE-G2-MATERIALIZATION-DECOMPOSITION-v5.md` | all; focused `11-31` and timer decomposition | `d778012b2d85006111eb31863ad4ea2c8e8fb1cf848a4d784a36130c317a00e6` |
| `implementation-detail/phase-4/algorithm/spec.md` | focused `780-940` plus materialization/lifecycle search | `67202cac261e401e103fe74143f7346fda3f2250ec6ede7fcf3e54016dc74fbf` |
| `implementation-detail/phase-4/algorithm/complexity-analysis.md` | focused read/materialize/create complexity | `c6a44fda3286b2e7e38b905f0336757563aec815068a23745011f0ec9b1c550b` |
| `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md` | focused materialization, reopen, cold, failure rows | `a8e65a188e4f5904c347f01d9bd65022c057c2348cf4d0350d8089f32a6e5fdf` |
| `research/phase-4/handoffs/hot-cold-materialization.md` | all; focused `35-52`, `85-189` | `3cb890cc34cf3667944482294a41bad4120e8bd3e7c86ebfdd09385b26b22429` |
| `research/phase-4/assurance/verification-security-resources.md` | focused exactness/adversary/receipt rules | `03f07d8337f346a411ed6138753dd8dc73781d191d8fdd9a35e0d8fc46341461` |
| `research/phase-4/foundations/invariant-matrix.md` | focused identity, durability, authority invariants | `c9a25b681fb5f15555adec5e356651fae06ce3cc8b075ebd617b7840a524c285` |
| `research/phase-4/foundations/benchmark-and-evidence.md` | focused benchmark labels, measurement, unsupported claims | `62d385cd7a7245429326e7a9f6f6ba053c30fcbdf322b7fa0cabd10bfe9007a2` |
| `research/phase-4/foundations/hypothesis-ledger.md` | focused candidate/falsifier fields | `1d4b3bb83f9dbb43d66e10702b946cb8f8dddc39c6c1faae00187ea4e4b6c2f9` |
| `research/phase-4/decision-map.md` | focused G4 boundary and dependencies | `8ddb236ff7d3cfa03257c9006d8b6f219b151f7433a331b4f2b9ea900c0c30fb` |
| `research/phase-4/core/cow/mapping-and-deltas.md` | focused range locality and mapping/delta reuse | `b48facb78eb05cd5d11b330e990a6fcc11b88d595dbe34e9d5f4d9ed207ee2ca` |
| `research/phase-4/core/canonical/canonical-v2-agent-prompt.md` | all governing canonical questions | `bf23fb6aee7a4582127ae421b0ff2c8a2f88406d84aa95c565f0b9ccba29f4d6` |
| `research/phase-4/core/canonical/canonical-v2-exploration-findings.md` | focused promoted identity/closure conclusions | `8b9b1fa13e56aed1b754da6b4b1dfe38d740199a0bded3b652fb3130ce824cd9` |
| `research/phase-4/core/canonical/canonical-v2-exploration-preregistration.md` | focused authority/format contract | `36cbd3f973532768a44f6e11d9a9162c28898cad62c829d2a367da8ad14ae69e` |
| `research/phase-4/core/canonical/h05-terminal-findings.md` | focused retained canonical witness result | `261ca204466438d69b0d2dfd96cb517c86145abff6440381cfcb749c9935f2bf` |
| `research/phase-4/core/canonical/identity-and-hashing.md` | focused domain separation and byte authority | `ce947becfe9105a5df58888314ead2491f17ff1ca5842cd78f45302ab18efdb6` |
| `research/phase-4/core/canonical/v2-single-identity.md` | focused canonical-v2 single identity | `0857d7633bfa8f8d7831087be4cea30479a9092553f9e08058528be593ac3cd7` |
| `research/phase-4/core/pipeline/full-create-pipeline.md` | focused one-pass pipeline and bounded memory | `daabf94a31a5613e1cf78fbaef1d46f3d8395fb3bc94c2fdbba6fdaf02a4be8d` |
| `research/phase-4/storage/compression-and-packing.md` | focused compression/physical-write distinction | `d5160bc38e9fb24601ec936e1ec46a0a0c81d06ff6f803f26534ca67c16d2815` |
| `research/phase-4/storage/sqlite/durability-and-layout.md` | focused WAL/DELETE, sync, page/cache, crash semantics | `12053708d794fa9737b3c388d1ae74887e4267b0b1334d3b654430c9ea1b3a3e` |
| `/Users/yifanxu/.codex/plugins/cache/ponytail/ponytail/4.9.0/skills/ponytail/SKILL.md` | all, per active coding/research workflow instruction | `1316a2f3f95741d2300b116fe0c2d81ce4a9568656ed0a62643f54aaf09957f2` |

Dependency manifests were checked to avoid designing against absent APIs:

| File | Lines | SHA-256 |
|---|---|---|
| `Cargo.toml` | all | `dbcb7eeb7672bdd5e8bb8ece8d238879e867b6f7f343ddfed50e20f807760621` |
| `Cargo.lock` | relevant `rusqlite`, `libsqlite3-sys`, `libc` records | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |
| `crates/layerfs-core/Cargo.toml` | all | `7104453012be05e2e9c9baa870dfba01c1a8ca321ac9b628649926437032849c` |
| `crates/layerfs-engine/Cargo.toml` | all | `35fd9c667575fdb3dd6ae720c4c43e6c654a9fd47da8b5dadc9f7672bd04498d` |
| `crates/layerfs-os/Cargo.toml` | all | `ee7387a8858d3900792b424c77153a291983885a361a2c3e12128c5aa7cea21d` |
| `crates/layerfs-vfs/Cargo.toml` | all | `e6868b66f840e56c3614e7da13e6ea099b2b4a9de15e15c0d1d4d42708ffd27d` |
| `crates/layerfs-sdk/Cargo.toml` | all | `e3c94ac5a46873b7a3d3b91e123bf6950f8ba589ff333ea0b5928e153f818fdd` |

### 11.2 Source implementation inputs

| File | Lines reviewed/cited | SHA-256 |
|---|---|---|
| `crates/layerfs-engine/src/lib.rs` | focused `243-266`, `683-799`, `912-1017`; API/caller search throughout | `9475d9d32d2e59cdf7b8a5f9cc3e35ecf3c58e47152fcfbf96c7a8b896eeaadb` |
| `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` | structural scan `1-18924`; focused `2184-3515`, `8080-8495`, `9038-9255`, `12060-12270` | `c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb` |
| `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs` | structural scan `1-2945`; focused `430-756`, `1087-1117`, `1303-1383`, `1632-1735`, `1782-2250` | `f9ffe7058761c60e7d81c5da18ed3d7a9afdb5344f41b9a97dcb8c2b8a51f032` |
| `crates/layerfs-core/src/lib.rs` | all exports | `ad1a0191dfe2ecafeae35f1f8d68b49ea3b1cd3cb36ce5226278f90cf3e0305b` |
| `crates/layerfs-core/src/canonical_v2.rs` | all; focused `15-31`, `72-138` | `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` |
| `crates/layerfs-core/src/cas/mod.rs` | focused object lookup/verification interface | `53a4effd5ccafedb649ad9c151e6ee7115958f5b9b4e5128f8c835518d3dd319` |
| `crates/layerfs-core/src/content/mod.rs` | focused content tree interface | `0969881a415f8bd4f4e1574170f8ee869b15145b215fad2c9a86dc0102ad6c9e` |
| `crates/layerfs-core/src/content/persistence.rs` | focused content persistence/reopen | `5b7831aa493e84aa77db274c1ac87db70b709a406e8241d7a665c6cefcf287fa` |
| `crates/layerfs-core/src/cow/mod.rs` | focused CoW public interface | `4043d8390cb9b86d4584340dc8c9929bb07720a978e47ac688b72e502424d657` |
| `crates/layerfs-core/src/cow/mutate.rs` | focused edit path | `59c22e102f235831e7ff5c12f119553c084044831199d015aaa53f57f88767fa` |
| `crates/layerfs-core/src/cow/persistence.rs` | focused reopen/persistence | `e2a25b67f7ee17a78a33aa0318bfcbcf020a5162b6670df8743941d282d65d56` |
| `crates/layerfs-core/src/cow/tree.rs` | focused mapping/tree traversal | `de3171a54ac9eb4c16be834d51e0b1636009529316e04703a67def3a335e48c7` |
| `crates/layerfs-core/src/delta/mod.rs` | focused delta interface | `c417e08dc2b6ecb39dc8371ccc5517780f948924425d33921b1036f725c46b1e` |
| `crates/layerfs-core/src/delta/codec.rs` | focused canonical delta framing | `e601dfcc561188d58d6cbb41d4ad0b606501995bce04e366afb601a7ba0f5c61` |
| `crates/layerfs-core/src/validation.rs` | focused typed validation | `f42eb13125cc19ecfc3e4567d35926b2871cd65b46d9f0af985c5a1782f02a5e` |
| `crates/layerfs-core/src/limits.rs` | all limits | `2ca5b3e8957331011f328fe87315c6fd43c6162c4da7ddee2960b571b30ea34f` |
| `crates/layerfs-core/src/identity/mod.rs` | all exports | `bd43ccb083a0b4659fc5303469983e928fecfc5707b596cf592163ad50ba744f` |
| `crates/layerfs-core/src/identity/digest.rs` | all digest rules | `8d22dbf8216da6cb2d88c3e067d41724d6dddaa0007a65cf5cbc5b9923151ce7` |
| `crates/layerfs-core/src/identity/ids.rs` | all typed IDs | `4e6fe13f99abc20d0395c8e95de937614070f7d7bf7e3027d52259990927f54c` |
| `crates/layerfs-core/src/object/mod.rs` | all exports | `1566a7de1146962d6b189daf39fe1167282d0d22305cebe840183d1533228659` |
| `crates/layerfs-core/src/object/codec.rs` | focused `64-115`, `153-165` | `513596fffcd7dca5f63fd0d86a9df6376e6794ee350c137eb6d786bba2c74659` |
| `crates/layerfs-core/src/object/model.rs` | focused object/metadata model | `fe6cb9e79d3d9aa16cc82896015d3a0765fb542be5a333a2f5d74f47e42801ae` |
| `crates/layerfs-core/src/cdc/mod.rs` | all constants/interface | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| `crates/layerfs-core/src/cdc/gear.rs` | focused chunker mechanics | `beb8637ea160f5b61401c0dec2b632927c81be0b491b443142973dc23108edb5` |
| `crates/layerfs-os/src/lib.rs` | all; host probe only | `13866474b3b8387e06d9c501c533c3067100eb573654ed2b0912292847d94996` |
| `crates/layerfs-vfs/src/lib.rs` | all; stub only | `20de55cdbe636b2219d7eaa60bc703b126bb18b77f17d35c137ba0228ee75849` |
| `crates/layerfs-sdk/src/lib.rs` | all; stub only | `7bdcac0987a591841ce31d17134e040eef651335abc550ffec1b3d1971c01210` |
| `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/sys/fcntl.h` | `155`, `243-265`, `319`, `374-500` | `807bfdae1695967faf13532c1d0eb0d5eb08a67e0bef8bf529f732f6ce00176b` |

### 11.3 Sealed evidence and fixture inputs

These artifacts were read only. Section 2.3 records the controlling aggregate
hashes; this table supplies file-level custody for the evidence actually
inspected.

| File | Lines | SHA-256 |
|---|---|---|
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CAMPAIGN-v13.json` | all | `70be7a26ada3f0c378faed061819338620cc43708c3e5226aff3a360b5eb7e88` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CLEANUP-v13.json` | all | `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-PRIMARY-ANALYSIS-v13.json` | all | `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-INDEPENDENT-RECOMPUTATION-v13.json` | all | `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json` | all | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv` | all 67 records | `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json` | `1-34` | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt` | `1-39` | `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STORAGE-v13.json` | all | `f6a2101e2e5f4cdc7cfd87b072a899d3ae0c4fda711f8c567ff04b6e3209d456` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/COMMANDS-v13.json` | all | `d0d71b1aad7a2a712abdd5707ea8f83bd36b5486aa19b894a8a4a41bd8c9f3c9` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/ENVIRONMENT-v13.json` | all | `c381064a91e1c58fade232c329346c032e2839f04332f3bf119c795a1237e11f` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/SOURCE-CUSTODY-v13.json` | all | `348b6409a8d45a74d5a80808a95611ea8d79f67d882292b549a84fbf464c004c` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/OPERAND-CUSTODY-v13.json` | all | `58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/METHODOLOGY-CUSTODY-v13.json` | all | `888213adc677a4634bbfd3b129b59f92ba8c13de447ee759907da0847e095849` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/ROW-CLEANUP-v13.jsonl` | all | `1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506` |
| `target/phase4-g3-incremental-materialization-20260822-v13/results-v13/rows-v13/G3-V13-RAW.jsonl` | `1-9` | `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-v5.json` | all | `09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/TERMINAL-VERIFICATION-v5.txt` | all | `41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-ANALYSIS.json` | all | `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/G2-V5-INDEPENDENT-RECOMPUTATION.json` | all | `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/rows-v5/G2-V5-RAW.jsonl` | all | `c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb` |
| `target/phase4-g2-materialization-decomposition-20260822-v5/results-v5/PAYLOAD-MANIFEST-v5.tsv` | all | `12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399` |
| `target/phase4-g2-materialization-decomposition-20260822-v1/results-v1/G2-PRIMARY-ANALYSIS-v1.json` | `1-1886` | `0840dcf353eff15a53eaa07f748678bfcab5b02b732ec9c592c12d0f38127282` |
| `target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/FIXTURE-MANIFEST-v1.tsv` | `1-4` | `6b088128ba02affae0edcd6d9a132da31acd2ac4f49da4681c63470d0a14bcc0` |
| `target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/wp4m-fixed-radix-fixture-manifest.json` | all | `92efe0a320dfe7926293d255c19da24cf688669a975cc26aab7dd424528dadb6` |
| `target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-1.source` | binary fixture, 1,048,576 bytes | `4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a` |
| `target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-10.source` | binary fixture, 10,485,760 bytes | `0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e` |
| `target/phase4-canonical-v2-complete-validation-20260821-v1/results-v1/work-v1/fixtures/S1-100.source` | binary fixture, 104,857,600 bytes | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |

## 12. Ranked recommendations

1. **DO NOW — C1: make accepted verification optionally materialize.** Add a
   bounded writer callback/sink to the existing batched
   `verify_file_inner` traversal, with no format or trust change. Authenticate
   each complete canonical object before emitting its raw payload. Publish
   through an absent, same-directory private temp with descriptor-relative
   no-follow checks, exact file metadata, data sync, metadata sync, atomic
   rename, parent-directory sync, and target/prior reconciliation. This is the
   only clean first/full, empty-destination baseline; predicted time remains
   `T_first_native = T_accepted_logical_auth + T_output_write + T_metadata +
   T_file_sync + T_rename + T_dirsync + T_reconcile_if_ambiguous`.
2. **DO NOW — C2: carry G3-v13 forward unchanged as the protected-seed
   incremental fast path.** Keep the unlinked read-only seed fd, same-open
   authority, one-shot permit, exact range authentication, clone/patch,
   complete fallback, and reconciliation contract. Never label its prebuilt
   seed preparation as first materialization, persistent cache, or cold.
3. **DO NOW — C3: make clone/cross-volume failure converge on C1.** Probe clone
   only after qualification. `EXDEV`, `ENOTSUP`, or an explicit clone-disabled
   row must use the same complete authenticated writer as C1, without consuming
   the permit or leaving temp names. Use `COPYFILE_CLONE_FORCE` only for a row
   whose purpose is to prove clone; best-effort `COPYFILE_CLONE` otherwise must
   expose whether it cloned and how many bytes were copied.
4. **DO LATER — C4: evaluate a bounded persistent verified-seed cache as an
   independent product.** Key by full content/profile/metadata identity, cap by
   measured allocated bytes, treat entries as derived/disposable, use atomic
   insertion and collision-safe eviction, and require cross-process authority
   that excludes same-UID substitution/rollback. Until that exists, persistent
   warm-source, fresh-process, and trusted-seed claims are unavailable.
5. **DEFER — C5: sparse/preallocation/packing.** Sparse holes and
   `F_PREALLOCATE` are capabilities, not proven wins; the disposable probe's
   single-byte sparse file allocated the whole 4 MiB. Benchmark only against a
   named sparse workload and report logical, apparent, allocated, clone/copy,
   and unsupported physical-device bytes separately. Packing/compression is a
   format/storage change and does not belong in the G4 materialization fix.
6. **REJECT — C6: receipts, inode/timestamps, process restart, or `F_NOCACHE`
   as proof.** None proves ordinary native bytes or persistent seed authority;
   process restart is not cold. On this macOS 26.4.1/APFS shared host,
   controlled cold is unavailable. Only a later exclusive-host successful
   `purge` immediately before a single no-warmup row may be labeled
   `controlled_cold_buffer_cache_approximation`, with device-cache state and
   stable-media physical I/O explicitly unknown.
