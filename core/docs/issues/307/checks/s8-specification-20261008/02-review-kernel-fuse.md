# Kernel/FUSE and native ownership review — retained reviewer report

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Retained verbatim from one explicitly authorized read-only review subagent of
> the S8 specification task, 2026-10-08, local `main` at `32d969776`
> (product pin `f0797c646`). The reviewer ran no build, test, mount or
> measurement and edited nothing. Findings are input; their accepted, rejected
> or deferred disposition is in the [finding ledger](05-finding-ledger.md), and
> the [S8 specification](../../S8-SPECIFICATION-20261008.md) owns every decision.
> Paths and line numbers are the reviewer's citations at that pin.

---

## (a) Scope and source pins

- **Repository:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, HEAD `32d969776`, product pin `f0797c646`. Read-only; nothing was built, mounted, tested or measured.
- **F** = `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/vendor/fuser-0.18.0/src`. This is the crates.io 0.18.0 archive (sha256 `b82b6597…baecfd`, upstream `9c957f74` per `.cargo_vcs_info.json`) with only `time.rs` patched.
- **P** = `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/layerfs-fuse/src` (excluded predecessor).
- **K** = `torvalds/linux` tag **v6.12**: `fs/fuse/{inode,file,dir,dev,control,readdir,xattr}.c`, `fs/fuse/fuse_i.h`, `fs/inode.c`, `fs/namespace.c`, `Documentation/filesystems/fuse.rst`.
- **libfuse** tag `fuse-3.16.2`, `include/fuse_lowlevel.h`.
- **Retained environment kernel:** `6.12.76-linuxkit` aarch64 (`core/docs/issues/307/FUSER-REGISTRY-PATCH-20261006.md:52`). I read v6.12, not the 6.12.76 stable tree; backports and line drift were not diffed. The page size of that kernel is not recorded: unknown.
- All repository docs you listed were read, plus `06-cluster-one-integration.md` §6 because it affects unmount.

## (b) Claim classification

| Claim | Class | Basis |
|---|---|---|
| Active native FUSE crate exists | Not implemented | `core/Cargo.toml` excludes `layerfs-fuse`. The predecessor also imports legacy-only symbols (`MountLease`, `MutationReceipt`, `MAX_READ_BYTES`, `ProjectionReplyPermit`; P/mount.rs:9-11, P/adapter.rs:4-7) that exist only in `layerfs-workspace-legacy`, so it cannot compile against the active Workspace crate |
| 128 KiB windows in the engine | IMPLEMENTED | `layerfs-overlay/src/contract/types.rs:12-14` |
| fuser signed-timestamp fix | IMPLEMENTED in vendor, PROVED | F/time.rs:42-54; 5 PASS in FUSER-REGISTRY-PATCH; unused by the active graph |
| Mounted `(i64::MIN, 200000000)` | PROVED FAIL (platform) | Same record; cause confirmed at K fs/inode.c:2612-2619 |
| Deferred, thread-crossing replies | Supported by pinned fuser; PROPOSED for LayerFS | F/reply.rs:104, 150-160 |
| 60 s TTL, KEEP_CACHE, 128 KiB, 2 loops, bg 1/1, writeback off, default_permissions | PROPOSED; all expressible | Finding 12 |
| FUSE_INTERRUPT cancellation | UNSUPPORTED-BY-PINNED-FUSER | F/request.rs:117-120 |
| `batch_forget` override | UNSUPPORTED | Type unnameable: F/lib.rs:34, 90, 429 |
| Notify retrieve, expire-only entry invalidation | UNSUPPORTED | F/request.rs:453-456; F/ll/notify.rs:68-78 |
| Removing ASYNC_READ / BIG_WRITES / MAX_PAGES | UNSUPPORTED | Only `add_capabilities` exists: F/lib.rs:107, 119-125, 342-349 |
| Receive buffer sized to negotiation | UNSUPPORTED | F/read_buf.rs:8, 25 |
| Reply delivery receipt (including INIT) | UNSUPPORTED | F/reply.rs:132-139 |
| Separate entry/attr TTL on CREATE and READDIRPLUS | UNSUPPORTED | F/reply.rs:502-519; F/ll/reply.rs:205-221 |
| Mmap-origin WRITE carries FUSE_WRITE_CACHE with writeback off | RESEARCH-ONLY, source-confirmed | K file.c:2081, 1818-1819 |
| Mmap flush emits a ctime-bearing SETATTR (01 §6, 03 §6) | Contradicted for writeback off | Finding 26 |
| Per-WRITE `inval_inode` removal is an optimization | Understated: it is a liveness requirement | Finding 20 |
| `attr_version` protects stale attribute replies | Source-confirmed for inode attributes only | K inode.c:327-331 |
| FORGET not guaranteed at unmount | Source-confirmed; RELEASE has the same gap | K inode.c:161; Finding 23 |
| Kernel sends DESTROY at unmount | Contradicted for the `fuse` fs type | K inode.c:1919-1923, 1950-1953 |
| Negative entries, CACHE_SYMLINKS, READDIRPLUS, PARALLEL_DIROPS, EXPLICIT_INVAL_DATA | RESEARCH-ONLY, expressible | F/ll/flags/init_flags.rs |
| Exec in a private mount namespace (06 §6) | PROPOSED; interacts with EBUSY/ENODEV | Finding 24 |
| All A1/A2/B numbers | Diagnostic, INELIGIBLE | 05 §1 |

## (c) Findings

### Q1 — pinned fuser surface

1. **Session creation.** `Session::new` runs option checks, mounts, then blocks in `handshake()` until INIT is read and answered (F/session.rs:154-194, 335-478). `Session::from_fd` wraps an already-mounted `/dev/fuse` descriptor, does the same handshake, and owns no mount (198-222).
2. **Mount path.** fuser tries direct `mount(2)` with `fd=,rootmode=,user_id=getuid(),group_id=getgid()` plus kernel options (F/mnt/fuse_pure.rs:384-501); nodev and nosuid are defaults (436-457).
   - `user_id` is hard-wired to the daemon's uid.
   - On EPERM it silently falls back to a `fusermount3`/`fusermount` binary, found by probing names and honouring the `FUSERMOUNT_PATH` environment variable (120-130, 161-187, 491-493).
   - `/dev/fuse` is opened through std (close-on-exec set atomically; F/dev_fuse.rs:28-34). The helper path receives the descriptor without atomic close-on-exec and sets it afterwards, ignoring errors (195-200, 343).
3. **Unmount ownership is unsafe to rely on.**
   - It uses plain `umount(2)` (F/mnt/mod.rs:169-191). On EPERM it does a lazy `MNT_DETACH`, then `fusermount -u -q -z`, and returns Ok (F/mnt/fuse_pure.rs:92-97, 132-159).
   - `Mount::umount(self)` consumes the handle even on error (F/mnt/mod.rs:146-155). After one EBUSY the mount is forgotten, and a later `SessionUnmounter::unmount()` returns `Ok(())` while still mounted (F/session.rs:506-511).
   - `run()` drops `UmountOnDrop` on every return, which is an implicit unmount attempt (103-122, 250).
   - `BackgroundSession::umount_and_join` returns early on error and detaches the thread (571-576).
4. **Loops.** `run()` consumes the session, spawns `n_threads` threads named `fuser-{i}` (default 1, Linux only for more), and joins them (F/session.rs:246-333).
   - A second loop exists only through `Config.n_threads`, either sharing the descriptor or with `clone_fd` (FUSE_DEV_IOC_CLONE on a fresh open; F/channel.rs:67-79). Loops cannot be added later.
   - Each request's reply sender is its own loop's channel (session.rs:533), which the kernel requires for cloned descriptors (K dev.c:2001-2002).
   - If a loop thread panics, `run()` returns without joining the others (310-316). They keep serving, unjoinable.
5. **Buffers.** Each loop allocates a zeroed `Vec` of 16 MiB + 4096 (F/read_buf.rs:8, 25; session.rs:527), plus one transient buffer in the handshake (336). The kernel only requires header + `max_write` (K dev.c:1275-1279). Residency of untouched pages was not verified natively.
6. **Replies.**
   - Reply types are `Send + 'static` and own a sender backed by `Arc<DevFuse>` (F/reply.rs:104-127); request header, names and WRITE data borrow the reused buffer.
   - Dropping an unanswered reply sends EIO with a warning (150-160).
   - Send errors are only logged (132-139).
   - `written` takes a `u32` (431-433).
7. **Interrupts.** INTERRUPT is answered ENOSYS (F/request.rs:117-120), so the kernel sets `no_interrupt` for the connection lifetime (K dev.c:2018-2019). After that, a task whose request was already read by the daemon waits uninterruptibly, including for SIGKILL (K dev.c:427-464).
8. **FORGET.** Both forms are dispatched without reply (F/request.rs:130-132, 457-462); the default `batch_forget` loops over `forget` (F/lib.rs:429-433).
9. **Notifier.** It is `Clone`, offers `inval_entry`, `inval_inode`, `store`, `delete` and `poll`, and is thread-safe through `writev` (F/notify.rs:52-105; F/channel.rs:86-93). ENOENT is mapped to Ok (108-116), so a notification after unmount also reports success.
10. **Negotiation.**
    - Everything is set in `init(&mut self, …, &mut KernelConfig)` (F/session.rs:415-424; F/ll/request.rs:998-1032).
    - Defaults: ASYNC_READ, BIG_WRITES, MAX_PAGES when offered (F/lib.rs:107, 119-125); `max_background` 16; 1 ns granularity; protocol 7.40 (F/ll/fuse_abi.rs:34-44).
    - `max_pages` is derived, not settable (F/lib.rs:388-390). `max_read` is only available as `MountOption::CUSTOM("max_read=N")`.
    - The trait requires `Send + Sync + 'static`; operations take `&self` (F/lib.rs:400-413).
    - Unknown opcodes (SYNCFS, TMPFILE, STATX) get ENOSYS (F/request.rs:71).
11. **Patch.** It changes only `system_time_from_time`, the SETATTR atime/mtime/ctime decode (F/time.rs:42-54; callers F/ll/request.rs:383-415). The fix is correct negative-fraction arithmetic, no `i64::MIN` negation, and a nanosecond clamp. The encode direction is unchanged.

### Q2 — candidate profile

12. **Per-element behaviour.**

| Element | Kernel behaviour | Hazard |
|---|---|---|
| Entry/attr TTL 60 s | Lookups and stats are served from cache until expiry (K dir.c:206, 1327). The kernel still invalidates on its own: size/mtime/ctime after each write (K file.c:1125), parent attributes after a directory change (dir.c:132-136), ctime after link/unlink/rename (954-958), atime after each READ, READDIR and READLINK (file.c:906, 934; dir.c:1609). O_EXCL and rename targets always revalidate (dir.c:207) | TTL is not the correctness mechanism |
| KEEP_CACHE | Without it every open drops the page cache (K file.c:279-280) | A size mismatch in any applied attribute reply truncates and drops the whole cache (K inode.c:349-352, 367-368) |
| 128 KiB | `set_max_write` and `set_max_readahead` (capped by the kernel's offer; K inode.c:1390, 1363-1364) plus `max_read=` | READDIR and READLINK buffers are one page (K readdir.c:339-357; dir.c:1591), so a 4096-byte symlink target allowed by `layerfs-content/src/filesystem/limits.rs:43` fails with EIO on a 4 KiB-page kernel |
| Two loops | Shared descriptor, exclusive wake (K dev.c:1290) | Without PARALLEL_DIROPS the kernel serializes LOOKUP and READDIR per directory (K inode.c:549-559; dir.c:428-431) |
| Background 1 / congestion 1 | Applied at K inode.c:1208-1219. Exactly one background request is in userspace at a time (K dev.c:330-343). Background means async readahead READ (file.c:972-975), mmap WRITE (1818-1821) and RELEASE (102-121) | Counterexample 1 |
| Writeback off | Do not request FUSE_WRITEBACK_CACHE. Inodes are then S_NOCMTIME (K inode.c:471-472) and server size and times are authoritative (292-296) | Full pages are marked up to date and unlocked before the WRITE is sent (K file.c:1236-1245); they are cleared only afterwards on error (1160-1161) |
| Permissions | `default_permissions` is the only correct option: requests carry one uid/gid and no supplementary groups, and cached reads never reach the daemon. The check runs on cached attributes (K dir.c:1541-1567) | Revocation delay applies only to out-of-band changes (1569-1572) |

### Q3 — coherence

13. **Short replies.**
    - A short READ shrinks kernel `i_size` (K file.c:820-849).
    - A short WRITE count becomes EIO (1301-1303).
    - An oversize reply becomes EIO (K dev.c:1944-1945).
    - The active read path returns "short at EOF" only (`layerfs-workspace/src/operations/file/read.rs:8-12`); this must hold exactly.
14. **Hard links.** Aliases share one kernel inode per nodeid, hence one page cache and one attribute set. LINK applies the reply attributes; UNLINK and rename-replace drop nlink locally and invalidate ctime (K dir.c:960-981, 1130-1134).
15. **Stale attribute replies.** A reply sampled before a newer inode version, or arriving while a write or truncate is in progress, is discarded (K inode.c:327-331; file.c:1118, 1310; dir.c:1976, 2053). The stat caller may still see the stale values (dir.c:1293-1294).
16. **Negative lookups.** An ENOENT error installs an uncached negative dentry (K dir.c:432-452). Only a nodeid-0 success caches it (387-389; expressible as `ReplyEntry::entry` with `attr.ino = 0`). Negative dentries are never revalidated without the parent lock (213-215).
17. **Truncate and O_TRUNC.**
    - A size-changing SETATTR truncates and drops the cache (K dir.c:2047-2051).
    - Without ATOMIC_O_TRUNC, OPEN arrives without O_TRUNC, followed by SETATTR size 0 with no file handle (K file.c:34-35; dir.c:1944-1958).
18. **Mmap-origin WRITE.**
    - It carries FUSE_WRITE_CACHE and is clipped to `i_size` (K file.c:2081, 1808-1815), so it never changes size.
    - The file handle is the first writable open of that inode, from any process (1972-1984); it holds a reference, so its RELEASE follows the write.
    - The request is force/no-credentials background (1818-1821). Header uid/gid/pid are the zero-initialized values; the zeroing allocator line was not verified.
    - Mtime is whatever the daemon assigns when it processes the write.
19. **O_APPEND.** In write-through mode the kernel resolves append offsets under the inode lock from cached `i_size`, without refreshing. The active `write` accepts `Position::End` (`layerfs-workspace/src/operations/file/write.rs:33-36`); using it for a guessed handle would misplace mapped data.

### Q4 — notifications

20. **Locks.**
    - `inval_entry` and `delete` take `killsb` (read) and the parent's inode lock exclusively; `delete` also takes the child's (K dev.c:1550, 1598; dir.c:1372, 1393). They block until every in-flight LOOKUP, READDIR, CREATE, UNLINK, LINK or RENAME on that parent is answered. libfuse documents this deadlock (`fuse_lowlevel.h:1685-1697`, 1752-1756).
    - `inval_inode` with offset ≥ 0 takes folio locks (K inode.c:542). These are held by an in-flight READ and by a partial-page WRITE (K file.c:1240-1245, 1172-1173). It also launders dirty folios by issuing a new mapped WRITE and waiting for it (2477-2490).
    - The predecessor's per-WRITE `inval_inode(ino, 0, 0)` before the reply (P/mount.rs:257) therefore self-deadlocks under cached I/O on any partial-page write.
    - Offset < 0 invalidates attributes only and never blocks on a reply (K inode.c:531-536).

### Q5 — readiness and detach

21. **Readiness.** A successful `mount(2)` queues INIT (K inode.c:1817). The INIT reply is processed synchronously inside the daemon's `writev`. Requests issued before the loops start simply queue (K dev.c:90-92, 124-129). INIT rejection is invisible to fuser, since send errors are only logged (F/reply.rs:132-139).
22. **Unmount.**
    - Plain umount returns EBUSY while there are users (K namespace.c:1920-1922).
    - `MNT_FORCE` aborts the connection before the busy check (1882-1883; K inode.c:567-574).
    - `MNT_DETACH` leaves the superblock and connection alive.
    - On final teardown the kernel aborts first (K inode.c:1984-1989, 1953-1954), so no DESTROY arrives. Loops see ENODEV (K dev.c:1296-1297; F/session.rs:551), then `destroy()` runs (324-330).
23. **FORGET and RELEASE at teardown.** No FORGET is queued once the superblock is inactive (K inode.c:161-170). Queued asynchronous RELEASE and mapped WRITE requests are ended by the abort (K dev.c:2278-2298).
24. **Mount namespaces.** Whether `umount(2)` returning 0 implies the superblock is gone depends on other mount-namespace copies. With the private namespace proposed in 06 §6 (lines 216-217) and private propagation, the daemon's umount would succeed while Exec processes still use the mount, and the join would never complete. This comes from general VFS semantics; `fs/pnode.c` was not line-verified.
25. **Fresh mount.** Each mount has a new connection, superblock and anonymous `st_dev` (K inode.c:1658, 1734). Inodes and pages are per superblock; `st_ino` is the server's `attr.ino` (K inode.c:275; dir.c:1345). git's index comparison rules were not verified from a permitted primary source.

### Q6 — timestamps

26. **Times are daemon-owned with writeback off.** FATTR_CTIME is produced only under local-time trust (K dir.c:1800-1801, 1915-1918) or by flushing time-dirty inodes (dir.c:1868-1882; file.c:2010-2011), which S_NOCMTIME prevents (K fs/inode.c:2214-2222). The daemon should never receive `ctime: Some(_)`; the predecessor's refusal at P/adapter.rs:465 is unreachable under this profile.
27. **The retained FAIL is independent of the profile.** The VFS zeroes nanoseconds at the superblock time limits before the filesystem sees them (K fs/inode.c:2612-2619).

### Q7 — confinement

28. **Kernel access rule.** Without `allow_other`, only tasks whose real, effective and saved uid and gid equal `user_id`/`group_id` may access; root is not exempt by default (K dir.c:1453-1466). With `allow_other`, any task in the mount's user namespace may (fuse.rst:83-95, 204-310).
29. **fuser's own filter.** `SessionACL::Owner` rejects any request uid other than the daemon's effective uid (F/request.rs:73-98). Under O-24 it must therefore be `All`.
30. **Control files.** The fusectl `abort` and `waiting` files are owned by `user_id` (K control.c:228-229, 265-271).

### Q8 — contradictions and untested interpretations

31. Summary of items in the docs that the sources contradict or leave unproved:
    - The ctime-bearing SETATTR claim (Finding 26).
    - DESTROY as a teardown signal (Finding 22).
    - Per-WRITE invalidation removal framed as an optimization rather than a liveness requirement (Finding 20).
    - `fuse.md` §3 claims parked requests do not block unrelated files; this holds only for foreground requests under background 1 (Counterexample 1).
    - Cancellation text omits that a parked request makes its process unkillable (Finding 7).
    - xattr refusal must be ENOSYS to be sticky (K xattr.c:60-61, 85-88). The predecessor's EOPNOTSUPP (P/adapter.rs:423-427) would plausibly cost one GETXATTR per `write(2)` through the privilege-removal check (K fs/inode.c:2140-2152; K inode.c:1584); the LSM hop was not line-verified.
    - "Exec exit is not a flush fence" is correct but imprecise (Counterexample 3).

## (d) Hazards and counterexample interleavings

1. **Head-of-line blocking under background 1.** P1 reads file x; readahead READ(x) is the single active background request and the daemon parks it. P2 reads uncached file y; its readahead waits in the kernel queue. Every later RELEASE and mapped WRITE on the mount also queues. Extra daemon threads cannot help.
2. **Unkillable process.** P blocks in WRITE and the daemon parks the reply. Exec cancel sends SIGKILL; P does not die until the daemon replies. If teardown waits for process exit before completing parked replies, it deadlocks.
3. **Commit misses mapped stores.**
   - Sequence: P maps a file shared, closes the fd, stores, exits. Exit queues the mapped WRITEs without waiting (K file.c:2496-2502, 2129), one at a time. The caller reaps P and Commits; capture drains daemon-side jobs but cannot see the kernel queue.
   - The fence that does exist: any `close()` on that inode after the last store waits for all mapped writes (K file.c:516-522), even if FLUSH is answered ENOSYS (529-530).
4. **Guessed append handle.** Q holds the inode open with O_APPEND; P stores through a mapping at offset 0. The kernel sends the WRITE at offset 0 with Q's handle. A daemon that honours the handle's append mode appends the page at EOF.
5. **Past-EOF mapped bytes.** P stores via mapping at offset 5500 in the last page of a 5000-byte file; those bytes never reach the daemon. P then writes at offset 6000. The cached page shows the stored bytes in 5500-5999; the daemon and Commit hold zeros. The live view then differs from the committed view. Inferred from the clip at K file.c:1808-1811 and the absence of tail zeroing in the write path; not verified natively.
6. **Stale-view size reply.** If a GETATTR sent after a WRITE reply is answered from an older view, the version check passes, the kernel truncates the cache and discards mapped dirty pages beyond the stale size.
7. **Notification deadlock.** A thread holding the guard for inode x calls `inval_inode(x, 0, 0)` while a READ on x is parked on that guard: the notifier waits on the folio lock, the READ waits on the guard.
8. **fuser unmount trap.** `unmount()` returns EBUSY and the handle is consumed. A later `unmount()` returns Ok while mounted. A non-root daemon silently gets a lazy detach, and the loops never see ENODEV.
9. **Uncommitted visibility.** A reader can see a full page of a write the daemon then rejects, until the reply clears it (Finding 12, writeback row).

## (e) Recommended specification decisions

**Mandatory for S8**

1. Own mount and unmount first-party: open `/dev/fuse` with close-on-exec, call `mount(2)` and `umount2(2)` directly, and use `Session::from_fd` with `run()` on a daemon-owned thread. `nix =0.31.3` is already in the graph; its mount feature is a manifest change to state. Never use `Session::new`, `spawn`, `SessionUnmounter`, the helper fallback, or lazy detach on the normal path.
2. Readiness means: `mount(2)` returned 0, the handshake returned, and all loop threads are spawned. Record negotiated facts inside `init` (ABI, offered and selected flags, limits, page size) in the mount receipt; there is no later accessor.
3. Negotiate exactly: `max_write` and `max_readahead` 131072, `max_read=131072`, background 1, congestion 1, 1 ns granularity. Assert the absence of WRITEBACK_CACHE, AUTO_INVAL_DATA, ATOMIC_O_TRUNC, the open-less flags and both KILLPRIV flags. Refuse the mount if a required capability is missing.
4. Background-class requests (readahead READ, mapped WRITE, RELEASE) are never parked on a guard another parked request can hold. If that cannot be proved, raise `max_background` in the profile.
5. Every admitted request gets an explicit reply in bounded daemon-owned time. Cancellation and teardown complete parked replies with a defined errno before waiting for process exit. No path relies on reply-drop EIO or on INTERRUPT.
6. READ returns exactly `min(size, EOF − offset)` bytes. WRITE replies the full count or an error. The offset is always the kernel's (`Position::At`), never `Position::End`.
7. Mapped WRITE: accept the flag, skip uid/pid/handle-mode authorization, validate only that the handle is live for that inode, never change size. Do not treat ctime in SETATTR as a supported route.
8. Every attribute-bearing reply (GETATTR, LOOKUP, CREATE, SETATTR, LINK) is computed from the published frontier at or after request receipt. Lookup counts are incremented exactly when a positive entry reply is attempted.
9. No `Notifier` use in S8. Any non-FUSE view change to a mounted Workspace is refused. Known install must be proved identical in ino, size, mtime, ctime, mode, uid, gid and nlink; otherwise it is refused.
10. Terminal sequence: fence → `umount2(path, 0)` → on EBUSY return Busy with nothing changed → on success wait, event-driven, for loop exit and join → retire outstanding lookup and open owners as a group. The mountinfo check is corroboration only.
11. State the mount-propagation contract between the daemon and the Exec namespace so that the kernel busy check and ENODEV stay truthful. Recommendation: the Exec namespace receives the Workspace mount as a slave.
12. `SessionACL::All`, with the kernel-side restriction per owner decision 1. Answer GETXATTR, SETXATTR and LISTXATTR with ENOSYS.
13. State the mode boundary already in active source: suid/sgid on files and sgid on directories are refused (`layerfs-workspace/src/operations/attributes.rs:27-36`). Also state the fsync no-op and that file locks are kernel-local (no lock flags are requested; K inode.c:1254-1262).

**Optional**

14. PARALLEL_DIROPS, EXPLICIT_INVAL_DATA and CACHE_SYMLINKS, each with its own proof.
15. A fusectl `waiting == 0` drain fence for Commit and a `max_background` read-back for readiness.
16. `statfs` with non-zero free space (the default is all zero: F/lib.rs:776-778).
17. A forced-teardown mechanism through `MNT_FORCE` or fusectl abort.

## (f) Required native oracles

Fault-holding fixtures must be external, not in product source.

1. **Negotiation receipt:** selected flags and limits; maximum observed READ and WRITE sizes; mountinfo options; fusectl read-back.
2. **Through-mount coherence matrix:** a cached reader plus a mutator over write, append, shrink, regrow, O_TRUNC, create, unlink, link, rename-replace, chmod and utimens. Compare cached read, fresh open, stat and daemon truth, with zero notifications and request counts proving the cache was actually used.
3. **Alias:** write through one link and cached-read through the other; nlink and ctime after unlink without waiting for the TTL.
4. **Reply ordering:** GETATTR held across a WRITE (external fixture) and discarded by the kernel; size never shrinks during concurrent append and read.
5. **Mmap:**
   - Variants: `msync`, `munmap` without `msync`, and map-close-store-exit.
   - Observe write flags, header uid/gid/pid, handle liveness, RELEASE ordering, and zero FATTR_CTIME.
   - Guessed append handle (Counterexample 4).
   - Past-EOF store then extend (Counterexample 5).
   - Commit inclusion with and without the drain fence.
6. **Background head-of-line** (Counterexample 1) and **kill-while-parked** (Counterexample 2).
7. **Readiness** without sleeps; first request served under the command identity.
8. **Unmount:**
   - Busy leaves the Workspace usable and a later attempt succeeds.
   - Success yields ENODEV, join and connection removal.
   - Counted FORGET and RELEASE shortfall.
   - The Exec-namespace case.
   - Forced abort.
9. **Two mounts of one root:** identical ino, size, mtime, ctime, mode, uid, gid and nlink; differing `st_dev`; the git read count on the second mount.
10. **Descriptor hygiene:** the fd table of a spawned Bash under concurrent mount and spawn.
11. **GETXATTR count** over many small writes; atime-induced GETATTR counts.
12. **Timestamps:** keep the retained fractional-minimum FAIL; add ctime == mtime after chmod, link and rename.

## (g) Owner decisions

1. **Access model for O-24.** Either `allow_other` (any uid in the namespace, including root and the daemon, gated by mode bits) or a kernel owner restriction with `user_id` = the Bash uid and no `allow_other`. The second excludes the daemon and root, but makes the Bash user the mount owner: it could abort through fusectl or unmount through a setuid `fusermount -u` if either is reachable.
2. **Commit and mapped stores.** Must a Commit issued after Exec exit include shared-mapping stores of exited processes? If yes, the drain fence becomes a product mechanism.
3. **Setuid helpers in Exec.** Should Exec children run with no-new-privileges or without setuid helpers? This changes "ordinary Bash" (`sudo`, `su`).
4. **Uniform Bash identity.** Is the numeric Bash uid/gid identical across all daemons sharing a Store? It is reported as `st_uid`/`st_gid`, which reaches committed tool state such as `.git/index` (the git comparison was not verified).
5. **Kernel scope.** Is S8 qualification scoped to the recorded 6.12.x Docker kernel, or to a stated range? Several findings above are version-specific.
