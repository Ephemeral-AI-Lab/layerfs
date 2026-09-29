# FUSE mount, arbitrary Exec, callbacks and checked unmount

> **Status:** Research; informative and not a product contract.
> Reviewed 2026-09-29 at product source
> `11a864fc133844cae7a4247b1f243d84d5763b10`, with documentation basis
> `5b23b3753b5a2621dd67dd14b40f07b7a924f536`.
> This is a source trace. No new runtime, benchmark or merge result is claimed.

FUSE exposes the attached Workspace as a Linux mount. Ordinary commands use
the kernel VFS and the implemented callbacks to observe or change that
Workspace. Attachment loads root facts; later reached names and bytes are
resolved lazily. A separate explicit Commit publishes canonical History.

This document covers the current mounted product, including supported
callbacks, refusals, ownership and cleanup. See [private backing and live
Commit](private_backing_workflow.md) for private records, physical accounting,
Service/C1/C2 construction and G1/G2 reconciliation, and [architecture
refinement](architecture_refinement.md) for before/after comparisons.

## Reading map

- [Public mount and lazy attachment](#1-public-mount-composes-attachment-and-projection)
- [Session, INIT and settings](#3-native-mount-performs-init-before-starting-the-worker)
- [Arbitrary Exec and syscall routing](#5-arbitrary-exec-uses-the-ordinary-mounted-posix-path)
- [Admission and inode conversion](#6-callback-admission-binds-owner-handle-and-reply-custody)
- [Lookup, open and reads](#7-lookup-getattr-access-and-open)
- [Writes and metadata changes](#9-write-selects-verified-private-bytes-before-the-reply)
- [Directory paging](#11-directory-handles-paging-and-continuation-cookies)
- [Cleanup and unsupported callbacks](#12-flush-release-forget-statfs-and-refused-operations)
- [Unmount and failure ownership](#13-checked-unmount-stops-drains-detaches-and-proves-absence)
- [Clean close and daemon stop](#15-unmount-clean-close-graceful-stop-and-sandbox-delete)
- [Efficiency, limits and evidence](#16-efficiency-and-retained-limits)

## 1. Public mount composes attachment and projection

The SDK's `WorkspaceApi::mount` creates a bound Workspace ID/incarnation and
sends WorkspaceOpen. Native/control WorkspaceAttach and WorkspaceMount also
exist, but the public mount method uses the composed route. The daemon profile
selects ReadOnly or LocalEdit; the public SDK has no writable boolean argument.

```text
MACOS SDK / SANDBOX OWNER                    DOCKER LINUX DAEMON
------------------------                    -------------------
WorkspaceApi::mount(sandbox, project, branch, optional commit)
             |
lookup admitted sandbox route and daemon instance
generate random Workspace ID + nonzero incarnation
bind {sandbox, instance, Workspace ID, incarnation}
             |
authenticated control connection / checked Hello
             |
WorkspaceOpen(project, branch, selected commit, instance)
             +----------------------------------------------+
                                                            |
validate wire request + authenticated grant + END_INPUT      v
revalidate grant/deadline; validate new Workspace identity and daemon instance
                                                            |
try_lock lifecycle slot; one selected mount owner            v
refuse unresolved prior ownership
                                                            |
WorkspaceHost::attach                                       v
  checked branch/root selection
  attached Workspace semantic owner
                                                            |
layerfs_fuse::mount[_writable]                               v
  actual Linux mount + INIT + checked notifier binding
                                                            |
retain MountHandle in lifecycle slot                        v
  completed or retained terminal
             +----------------------------------------------+
             |
SDK result:
  Completed -> Mount {id, location}
  Retained -> error retains Workspace ID
  unknown terminal -> UncertainMount {id, cause}
```

The mount directory is a projection location, not a full imported tree. Service
and canonical Store ownership remain separate from execution-side temporary
backing. Public mount errors preserve the ID when native attachment/mount
ownership might exist; lost terminal delivery is not evidence of absence.

Sources: [SDK mount](../../../../crates/layerfs-api/sdk/src/workspace.rs#L18),
[sandbox binding](../../../../crates/layerfs-sandbox/src/owner.rs#L497),
[authenticated session](../../../../crates/layerfs-sandbox/src/session.rs#L81),
[daemon authorization/slot](../../../../crates/layerfs-daemon/src/control.rs#L205),
[attachment lifecycle](../../../../crates/layerfs-daemon/src/lifecycle.rs#L63).

## 2. Attachment loads only initial root facts

```text
WorkspaceHost::attach(options, original bounded deadline)
        |
validate ID/incarnation/access/base/owner and shared Host resources
reserve registry entry + verified private-directory custody
registry = Attaching
        |
upstream admission
        |
LocalEdit? -> authenticated FileSaveCapabilities; require exact typed v2
        |
select immutable filesystem root
  supplied Root OR checked History Branch/Commit/Layer selection
        |
Inspect::Attributes(root, empty path)
  verify canonical root directory/metadata and expected serial if declared
        |
initial resident state
  only root Node is present
  node_index[root serial] = index0
  charged Node Vec capacity256; fixed handle capacity128
        |
owned empty mount leaf + verified private backing directory
LocalEdit metadata arena + ActiveBacking owner
        |
registry = Attached
        |
future LOOKUP/LIST/READ loads reached names and bytes
```

Lazy loading still has charged setup capacity and remote root inspection.
`NODE_LIMIT=256` is the growth chunk size, not a 256-file workspace cap.
LocalEdit's positive internal capability check prevents accepting generic
Base-origin writes on an incompatible SaveFile peer. ReadOnly does not require
that mutable capability or the same active owner.

Source: [Workspace Host attachment](../../../../crates/layerfs-workspace/src/runtime/host.rs#L275),
[private attachment ownership](private_backing_workflow.md#2-attachment-establishes-private-custody).

## 3. Native mount performs INIT before starting the worker

The locked provider is fuser 0.18.0 with default features disabled. Its
`Session::new` completes the kernel INIT handshake before returning. INIT
therefore precedes notifier binding and the outer Session worker.

```text
attached Workspace
        |
mount_profile(readonly or writable)
        |
admission
  original deadline live; writable requires LocalEdit
  Linux x86_64/aarch64; effective UID0 + root inode UID0 + CAP_SYS_ADMIN
  root attributes representable in FUSE reply
        |
Workspace::reserve_mount
  reject mounted/active operations
  precharge ProjectionState; mounted=true; coherence=Unbound
  return MountLease
        |
construct Adapter + fuser Config
        |
Session::new(Adapter, owned mount path, Config)
  actual native mount / /dev/fuse
  receive INIT -> Adapter::init -> kernel INIT response
        |
retain SessionUnmounter
retain pending Session in Arc<Mutex<Option<Session>>>
        |
bind checked invalidator; charge closure; Unbound -> Ready
        |
spawn outer "layerfs-mount" thread
        |
take pending Session; Session::run
        +-- fuser-0 callback/event loop
        +-- fuser-1 callback/event loop
            both use same FUSE channel FD; clone_fd=false
        |
return MountHandle {lease, unmounter, worker, pending ownership}
```

The two event loops are callback dispatch workers. They do not change the
single canonical construction worker rule. The owner UID is checked both by
the mount ACL and by the adapter's callback guard.

Entry invalidation first invalidates the parent inode's attributes and then
its named entry. Data invalidation uses whole-inode data invalidation. The
Workspace binds these notifications to selected publication receipts; unmount
must retire that binding through its lease rather than discarding it early.

Sources: [mount admission/session](../../../../crates/layerfs-fuse/src/mount.rs#L146),
[worker and retained failure](../../../../crates/layerfs-fuse/src/mount.rs#L228),
[invalidator](../../../../crates/layerfs-fuse/src/mount.rs#L247),
[privilege check](../../../../crates/layerfs-fuse/src/mount.rs#L401),
[mount lease](../../../../crates/layerfs-workspace/src/runtime/lifecycle.rs#L15),
[notifier binding](../../../../crates/layerfs-workspace/src/runtime/coherence.rs#L422),
[INIT settings](../../../../crates/layerfs-fuse/src/adapter.rs#L70),
[locked dependency](../../../../Cargo.lock).

Provider session ordering/default callbacks were read from the locked
fuser-0.18.0 source. The provider's versioned source is available through
[docs.rs](https://docs.rs/fuser/0.18.0/src/fuser/session.rs.html);
it is a dependency implementation, not first-party LayerFS source.

## 4. The mount and cache settings mean different things

| Setting | Current value | Meaning/qualification |
| --- | --- | --- |
| Entry/attribute TTL | Zero | No positive metadata validity interval in replies; not a cold-cache proof |
| Writable regular OPEN/CREATE | FOPEN_DIRECT_IO | Selected FUSE data profile bypasses file page cache |
| ReadOnly regular OPEN | Empty FopenFlags | Separate cached kernel read profile |
| Mount max_read | 128 KiB | Configured request maximum, not all-route read proof |
| INIT max_write / max_readahead | 128 KiB / 128 KiB | Oversize callback input refused; RO/RW behavior differs |
| max_background / congestion threshold | 1 / 1 | Background queue settings, not one total callback or syscall |
| Event loops / clone_fd | 2 / false | Two callback threads share device FD |
| ACL | Owner | Also checked against Workspace UID |
| Options | RW or RO, NoSuid, NoDev, DefaultPermissions, Exec, NoAtime, FSName/Subtype layerfs | Permission checks and selected executable profile |
| Directory page | At most 128 semantic entries | Adapter stops when kernel reply byte buffer fills |
| Callback deadline | 10 seconds | Observation checks, not preemption of blocking kernel/native calls |
| Negotiated defaults | ASYNC_READ, BIG_WRITES and supported MAX_PAGES | No writeback, readdirplus, stateless-open, symlink-cache or ATOMIC_O_TRUNC negotiation |

```text
cache/owner domains
  application buffers
        |
  Linux VFS dentries/inodes/metadata validity
        |
  FUSE file data page cache: RW direct-I/O profile differs from RO
        |
  Workspace name memo / selected hot nodes / optional read-origin bytes
        |
  verified private backing I/O
        |
  Service C1 provider / C2 SQLite and persistent packs
        |
  Linux VM / Docker backend / device / macOS host caches
```

Linux defines FUSE direct I/O to bypass file page-cache/readahead behavior;
shared writable mmap is not supported by this selected profile. ReadOnly can
use the cached mode. Ordinary syscalls can also reuse kernel pathname facts.
See the primary [Linux FUSE I/O documentation](https://docs.kernel.org/filesystems/fuse/fuse-io.html).

Zero TTL and direct I/O do not enforce cold private/Service/VM/device/host
caches or prove a phase resource peak. `max_background=1` limits background
requests; it is not a global one-request limit. See [Linux FUSE control and
mount documentation](https://docs.kernel.org/filesystems/fuse/fuse.html).

Sources: [Config](../../../../crates/layerfs-fuse/src/mount.rs#L203),
[INIT](../../../../crates/layerfs-fuse/src/adapter.rs#L70),
[OPEN reply flags](../../../../crates/layerfs-fuse/src/adapter.rs#L163),
[CREATE reply](../../../../crates/layerfs-fuse/src/adapter.rs#L873).

## 5. Arbitrary Exec uses the ordinary mounted POSIX path

```text
WorkspaceApi::exec(id, caller-supplied command)
        |
bound sandbox/daemon instance + Workspace incarnation
authenticated control request
  nonempty UTF-8 command; no NUL; <=4096 bytes
  requested Exec bound <=30 seconds
        |
validate grant/selector; try_lock lifecycle slot
require retained mount owner
        |
/bin/sh -c <caller command>
  cwd = workspace.mount_path()
  new process group; stdin=null; stdout/stderr=piped
        |
ordinary child processes
  openat/read/pread/write/pwrite/mkdir/rename/unlink/chmod/...
        |
Linux VFS resolves actual path or open inode
  kernel permissions/pathname/FD work
        |
resolved target is on LayerFS mount?
        +-- no -> owning filesystem handles it
        +-- yes
              |
              v
FUSE kernel request -> /dev/fuse -> fuser dispatch -> Adapter callback
              |
Workspace selected namespace + active backing
  local selected facts OR authenticated Service query/read
              |
FUSE reply attempt -> syscall result -> caller's shell behavior
              |
drain both stdout/stderr pipes
  keep <=8192 bytes each; drain/discard excess and mark truncated
  process exit AND both pipe EOF required for completion
              |
authenticated Exec result

separate WorkspaceApi.commit -> canonical Save/History workflow
```

A single command can touch several filesystems and launch multiple children.
Application-buffered operations do not generate callbacks until they issue
syscalls. Kernel pathname/FD behavior means one syscall need not equal one
callback. SDK mount/Commit/status/pinned-view control calls use their own
authenticated route rather than pretending to be FUSE syscalls.

At deadline or output failure, the daemon attempts `killpg(SIGKILL)` and waits
for the child, records an unknown result, and retains Workspace state. Accepted
filesystem mutations survive; Exec does not roll them back or Commit them.
A zero shell exit is the command's result, not canonical publication.

The dispatcher checks daemon control stopping, grant, selector and mount
presence. It does not independently check Workspace stopping/closed before
spawn. A retained stopped mount can still allow the shell to spawn; new
observed projection callbacks then return ENODEV. Do not replace that actual
behavior with a universal pre-spawn refusal claim.

The optimization matches verified bytes/identity behind reads and writes.
Neither shell command text nor benchmark identity chooses a range-edit
carrier. No public Workspace range API or FUSE ioctl is reintroduced.

Sources: [SDK Exec](../../../../crates/layerfs-api/sdk/src/workspace.rs#L75),
[request grammar](../../../../crates/layerfs-bridge/src/contract/workspace_request.rs#L116),
[execution bounds](../../../../crates/layerfs-bridge/src/contract/execution.rs#L4),
[dispatch](../../../../crates/layerfs-daemon/src/control.rs#L409),
[process/output lifecycle](../../../../crates/layerfs-daemon/src/execution.rs#L55).

### Syscalls and relevant callbacks

| User action | Relevant FUSE callbacks |
| --- | --- |
| Traverse/openat | LOOKUP, possibly GETATTR/ACCESS, OPEN or CREATE |
| stat/fstat | GETATTR; fh checks bind handle to inode |
| read/pread | READ after required open/path work |
| write/pwrite/append | WRITE with accepted kernel/caller flags |
| close | Possible FLUSH, eventually RELEASE |
| getdents/readdir | OPENDIR, paged READDIR, RELEASEDIR |
| mkdir/create/mknod | MKDIR/CREATE/regular MKNOD plus path resolution |
| truncate/ftruncate/open with truncate | SETATTR(size); atomic O_TRUNC not negotiated |
| chmod/portable timestamps | Supported SETATTR fields |
| unlink/rmdir | UNLINK/RMDIR |
| rename/temporary-file replacement | RENAME for old/new parents/components |
| hard link | LINK, same live regular serial |
| symlink/readlink | SYMLINK/READLINK |
| Lookup-reference retirement | FORGET; provider BATCH_FORGET delegates per inode |
| statvfs | STATFS |
| fsync/fdatasync/directory fsync | FSYNC/FSYNCDIR -> EOPNOTSUPP |

## 6. Callback admission binds owner, handle and reply custody

```text
new observation/read callback                 new mutation callback
-----------------------------                 ---------------------
Adapter::guard                                owner/stopping/RW/input checks
  stopping -> ENODEV                                   |
  wrong Workspace UID -> EACCES                        v
        |                                     exclusive ProjectionMutationPermit
        v                                       Ready notifier required
ProjectionReplyPermit                            no conflicting reply/mutation
  mounted/available + original deadline                  |
  at most2 concurrent projection replies                 v
        |                                     semantic validation/candidate
        v                                                |
select checked result                                    v
        |                                     one selected publication
        v                                                |
attempt one reply                                        v
permit survives reply boundary                 selected notification/reply attempt
        |                                                |
drop permit, decrement owner                   drop permit, clear owners, wake waiter
```

A successor WRITE can wait at a bounded narrow boundary only when the previous
healthy WRITE has already marked reply-started. This is not a general queued
mutation/retry service. Failed/Pending coherence has explicit admission and
inspection rules; local reads can still inspect selected bytes where permitted.
Release/Forget cleanup stays available while foreground admission stops.

Canonical root serial and FUSE inode 1 are exchanged reversibly. A genuine
canonical serial 1 swaps with the root serial, so other serials remain stable
without a collision. Reply conversion checks timestamp/mode/link-count
representability. FUSE `FileAttr.blocks` derives from logical size and is not
the physical private `st_blocks*512` accounting proof.

Sources: [callback guard/permit](../../../../crates/layerfs-fuse/src/adapter.rs#L33),
[projection admission](../../../../crates/layerfs-workspace/src/runtime/coherence.rs#L456),
[reply-start boundary](../../../../crates/layerfs-workspace/src/runtime/coherence.rs#L569),
[inode/attribute conversion](../../../../crates/layerfs-fuse/src/replies.rs#L61).

## 7. LOOKUP, GETATTR, ACCESS and OPEN

```text
LOOKUP(parent inode, one name)
        |
translate inode -> canonical serial
check selected directory/access; select active revision + immutable Base
        |
active N(parent,name)?
        +-- tombstone -> ENOENT
        +-- binding -> active I(child)
        +-- absent
              +-- fresh private parent -> ENOENT
              +-- same-Base inherited-name memo -> exact immutable facts
              +-- otherwise -> Service ChildAttributes(Base,parent,name)
        |
overlay selected active I(child) when present
charge/cache full child locator
recheck baseline/revision
increment Projection lookup reference
        |
checked FileAttr -> ReplyEntry(TTL0)
```

Cold inherited lookup can issue a Service query. A positive same-Base memo can
reuse exact facts; absent names are not universally negatively cached.
Tombstones are authoritative for the selected private view. The remote query
uses inode identity rather than rewalking an old canonical path after rename.

GETATTR normally uses current cached Node attributes. With fh, it checks the
handle/inode binding first. ACCESS applies owner/mode and ReadOnly write
refusal. These are observation calls, not implicit Saves.

```text
OPEN(ino, flags)
        |
validate owner/rights/kind/accepted flags
        |
insert Projection-scoped semantic handle; increment Node handle count
        |
regular Handle.view=None; directory open uses separate selected-view route
        |
RW regular reply -> FOPEN_DIRECT_IO
RO regular reply -> empty FopenFlags
```

The handle table is capped at 128. Ordinary regular FDs follow live inode
content, including writes through another hard link; opening before Commit
does not freeze old bytes. Caller O_APPEND intent is retained. The explicit
accepted flag set refuses unsupported sync/direct-I/O caller flags. The
kernel OPEN/SETATTR truncation route is separate because ATOMIC_O_TRUNC is not
negotiated. Native open-with-truncate has its own atomic handle/publication
semantics, described in the private document.

Sources: [LOOKUP](../../../../crates/layerfs-fuse/src/adapter.rs#L93),
[namespace resolution](../../../../crates/layerfs-workspace/src/filesystem/namespace.rs#L112),
[active/name memo](../../../../crates/layerfs-workspace/src/filesystem/active_view.rs#L111),
[GETATTR/ACCESS/OPEN](../../../../crates/layerfs-fuse/src/adapter.rs#L122),
[semantic open](../../../../crates/layerfs-workspace/src/filesystem/open.rs#L85),
[accepted flags](../../../../crates/layerfs-fuse/src/open_flags.rs).

## 8. READ and READLINK resolve only selected bytes

```text
READ(ino, fh, offset, requested length)
        |
owner/reply permit; verify inode/handle/read rights
requested <=128 KiB; clip at EOF
        |
charge output + selected locator; pin current read selection
        |
selected intervals
        +-- Packed  -> authenticated selected page/slot
        +-- Payload -> verified owned private payload
        +-- Zero    -> synthesized zeros
        +-- Base    -> authenticated Service ReadFile(root, range)
        |
validate exact lengths and identities
optional ONE charged ReadOrigin for actual contiguous immutable Base bytes
        |
ReplyData with copied output and operation owner alive
        |
release transient selection/reply ownership
```

The selected active lock covers interval selection and Packed/Zero copying.
Base spans and payload owners survive outside it for RPC/physical reads. A
configured range can still require mapping/chunk/group reads beyond its logical
size; lazy loading does not guarantee exact physical read amplification of 1.

Read-origin reuse is a byte/identity optimization, not a separate content cache
or command dispatch. A non-Base/mixed/Zero read clears the optional memo.
Changed inherited identities refresh through InodeAttributes. Base byte RPC
needs the primary remote slot, so it can refuse Busy during Commit.

READLINK reads selected local target bytes or canonical InodeReadlink through
metadata admission. Native target grammar allows up to 4096 non-NUL bytes;
FUSE rejects PATH_MAX bytes or more, so its returned target must be shorter.

The declared 128 KiB read bound is not new successful-read evidence. Retained
live SDK full-byte proofs use 16 KiB; the independent 32 KiB pinned SDK `Io`
observation remains separately unresolved. Public pinned reads use control
leases, not a FUSE regular-file handle.

Sources: [READ callback](../../../../crates/layerfs-fuse/src/adapter.rs#L197),
[selected read](../../../../crates/layerfs-workspace/src/filesystem/read.rs#L14),
[read-origin](../../../../crates/layerfs-workspace/src/filesystem/read_origin.rs#L15),
[READLINK](../../../../crates/layerfs-fuse/src/adapter.rs#L235),
[symlink selection](../../../../crates/layerfs-workspace/src/filesystem/symlink.rs#L39).

## 9. WRITE selects verified private bytes before the reply

```text
WRITE(ino, fh, offset, bytes, kernel flags)
        |
owner/stopping/ReadOnly/flags/<=128 KiB checks
reject writeback-cache/unsupported flags
verify projected write rights and handle identity
exclusive mutation/reply permit
        |
<=128-byte input -> charged bounded copy
larger input -> owned private Payload acquisition
        |
select current inode/Base; append validates offset==live EOF
frontier Budget + completion escrow + physical admission
        |
exact supplied bytes match actual ReadOrigin and inode/root?
        +-- yes -> Base-source extent
        +-- no -> shared tiny pack / owned Payload extent
        |
coherent candidate E/R/L/P/I/D + matching attributes/locator work
physical create -> actual blocks -> write -> readback/authentication
        |
ONE selected index revision
        |
update resident inode through node_index
checked owner retirement; selected checked inode notification
        |
mark reply-started; ReplyWrite(accepted count)
permit remains alive through reply attempt
```

WRITE changes private state. It does not run SaveFile, Stage or History Commit
per callback. Sparse growth uses Zero extents; overlap cuts final intervals and
their inverse references. Truncate uses SETATTR rather than a hidden range API.
Accepted zero-length input returns count0 and the current revision without
dirty admission, candidate publication or notification. The diagram describes
nonempty input after that branch.

Notification/cleanup failure after publication can return EIO while accepted
state and its native receipt remain owned. A FUSE error grants no blind replay,
rollback or quota refund. The provider exposes reply sending, not a checked
later kernel-completion acknowledgement.

Sources: [callback WRITE](../../../../crates/layerfs-fuse/src/adapter.rs#L496),
[ordinary active write](../../../../crates/layerfs-workspace/src/filesystem/write.rs#L187),
[publication](../../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L35),
[post-publication failure](../../../../crates/layerfs-workspace/src/filesystem/write.rs#L390),
[private verification workflow](private_backing_workflow.md#6-a-mutation-publishes-one-complete-verified-selection).

## 10. Namespace and portable attribute mutations

```text
new CREATE / regular MKNOD / MKDIR / SYMLINK
        |
parent/name/rights/creation-condition checks
cold inherited facts may require Service ChildAttributes
        |
History ReserveInodes {scope, count:1}
        |
precharge frontier, Node, handle/payload and completion owners
        |
candidate N(parent,name) + child I/D + parent I/D
        |
ONE active selection
CREATE additionally installs ready handle in same publication
Projection lookup reference retained
        |
kernel installs parent/entry from callback reply
```

Fresh identity creation reserves one canonical serial. It is not wholly local
and the reservation remains consumed if later publication refuses. LINK reuses
an existing live regular serial; nonexclusive CREATE on an existing file takes
the existing-name/open route without a new serial.

| Callback | Current semantics and refusal |
| --- | --- |
| CREATE | Regular file; mode/open/exclusive/truncate checks; FOPEN_DIRECT_IO handle |
| MKNOD | Regular kind and rdev0 only; special device/FIFO/socket kinds refused |
| MKDIR | Directory with supported mode/sticky bits; kernel already applied umask, so adapter passes zero |
| SYMLINK | Opaque non-NUL target within native bound; Payload custody even for short target |
| LINK | Live regular inode only, same serial and data; directory/symlink link refused |
| UNLINK | Tombstone/parent attributes/link decrement; held unlinked inode retains ownership |
| RMDIR | Directory/root/effective-empty checks; detach held directory instead of enabling mutation via removed ancestry |
| RENAME | Ordinary replacement or RENAME_NOREPLACE; EXCHANGE/WHITEOUT/other flags return EINVAL; both bindings publish together |
| SETATTR size | Regular file only; shrink cuts intervals, growth exposes zeros; no directory size mutation |
| SETATTR mode/time | Portable mode/mtime; unsupported ownership/platform times/flags refused |

Rename retains current full-locator work: resident Node replacement paths are
prepared, and increasing depth can visit inherited descendants. The active
aggregate limit remains 65,536 bytes/256 components. The constant number of
logical parent/name changes is not a scan-free rename proof.

SETATTR has one portable timestamp. A selected atime is accepted only when it
equals the converted mtime. General chown, independent atime/platform-time
and symlink portable-attribute mutation are not supplied by this route.

### Current notification source/comment discrepancy

The separate native projected `set_len` route uses `ProjectionSize`, and
creation uses `ProjectionCreate`; active-file delivery suppresses those
origins. Current FUSE SETATTR, including size-only requests, calls
`permit.set_attributes` and uses `ProjectionAttributes`. Namespace projection
changes avoid reverse parent notification while the kernel owns its entry
mutation. Regular WRITE selects reverse inode notification.

Adapter comments at lines 488–489 and coherence comments at 386–387 describe
projected SETATTR as not notifying. The regular-file SETATTR route actually
uses `ProjectionAttributes` even for size-only input, which is not suppressed
by active_file's
215–226 origin match; write.rs delivers that selected notifier. Directory
portable SETATTR suppresses projected delivery in active_attributes.rs 89–96.
The documents follow those branches. This discrepancy is a source observation;
no runtime failure or deadlock test was performed for this documentation task.

Sources: [SETATTR](../../../../crates/layerfs-fuse/src/adapter.rs#L387),
[create/mknod/mkdir](../../../../crates/layerfs-fuse/src/adapter.rs#L593),
[remove/rename/link](../../../../crates/layerfs-fuse/src/adapter.rs#L643),
[CREATE](../../../../crates/layerfs-fuse/src/adapter.rs#L873),
[serial reservation](../../../../crates/layerfs-workspace/src/filesystem/active_create.rs#L117),
[link restriction](../../../../crates/layerfs-workspace/src/filesystem/link.rs#L30),
[actual file notifier selection](../../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L215),
[directory notifier selection](../../../../crates/layerfs-workspace/src/filesystem/active_attributes.rs#L89),
[origin conversion](../../../../crates/layerfs-workspace/src/runtime/coherence.rs#L398).

## 11. Directory handles, paging and continuation cookies

```text
OPENDIR(inode)
        |
owner/flags/permissions; reserve Projection handle, table max128
pin effective View + charged directory full path/parent
        |
READDIR(fh, cookie)
        |
verify handle/inode and cookie belongs to THIS handle
cookie0 starts; other cookie resumes after stored name/dots state
        |
"." -> held directory serial
".." -> live Node parent or last parent after detach
        |
merge byte-ordered streams in selected View
  active N bindings/tombstones, scan batches <=128 rows
  canonical InodeList(serial), <=128 names /16 KiB per RPC page
        |
for EACH emitted child
  selected kind/attributes resolution
  listed serial == resolved serial check
        |
charged cookie indexes
  cookie ID -> {handle, after-name, dots}
  {handle, dots, name} -> reusable cookie ID
        |
up to128 semantic entries prepared
ReplyDirectory.add until kernel byte buffer is full
        |
reply; resume from last ACTUALLY DELIVERED cookie
        |
RELEASEDIR
  release selected View and Node handle pin
  remove handle-owned cookies/shrink charge
  collect unreferenced Nodes and retained ancestors
```

A fresh private directory lists local `N` facts. An inherited listing uses
canonical inode-relative pages and resolves each returned child's attributes.
A readdir name is not automatically the complete positive lookup memo.
Cold `ls` can therefore require multiple authenticated queries; this is not
READDIRPLUS or a free bulk stat.

If the kernel byte buffer fits fewer than the prepared 128 entries, the
cookie prevents skipping unreturned names, but preparation already paid for
its generated page. Full enumeration remains proportional to examined/emitted
rows with actual index and remote resolution costs. A directory handle pins
selected names; its active `..` semantics do not freeze the live parent forever.

Sources: [directory callbacks](../../../../crates/layerfs-fuse/src/adapter.rs#L282),
[handle pin](../../../../crates/layerfs-workspace/src/filesystem/open.rs#L219),
[merge/cookies](../../../../crates/layerfs-workspace/src/filesystem/directory.rs#L17),
[cookie ownership](../../../../crates/layerfs-workspace/src/runtime/state.rs#L667).

## 12. FLUSH, RELEASE, FORGET, STATFS and refused operations

| Callback | Effect |
| --- | --- |
| FLUSH | Check fh/inode and retained coherence error; no Save, Commit, fsync or handle release |
| RELEASE | Check fh/inode; drop regular semantic handle; decrement Node handle count; collect |
| RELEASEDIR | Release directory View/path/cookies/handle and collect |
| FORGET | Decrement Projection lookup refs by kernel count, collect; no reply |
| DESTROY | Set Adapter stopping; does not finish MountLease or prove refund |
| STATFS | Guard owner/stopping; static zero block/file counts, block/frsize4096, name max255 |
| FSYNC/FSYNCDIR | Explicit EOPNOTSUPP |
| READDIRPLUS | Explicit EOPNOTSUPP |
| GETXATTR/LISTXATTR | Explicit EOPNOTSUPP |
| SETXATTR/REMOVEXATTR | Guard then EOPNOTSUPP on RW, EROFS on RO |

Release/Forget and the cleanup role of Flush remain usable during stopping
so owners can drain. STATFS is not an exact private-quota report or an
inventory of maximum file capacity. No Workspace fsync is added.

Unimplemented fuser trait operations inherit version 0.18.0 defaults:
`ioctl`, `bmap`, `poll`, `fallocate`, `lseek`, `copy_file_range`, `getlk` and
`setlk` have no product-backed implementation and return ENOSYS at their
callback boundary. The kernel can handle portions of seek/locks/poll locally;
this does not mean every ordinary SEEK_SET/SEEK_CUR issues LSEEK. Ordinary
copy can use read/write; no command name selects a private bypass.

Writeback-cache writes, unsupported open flags, unsupported SETATTR fields
and special MKNOD kinds are also refused. Shared writable mmap and arbitrary
xattrs/devices/sync are outside the selected supported projection. The retired
range-edit ioctl remains absent.

Sources: [destroy/forget](../../../../crates/layerfs-fuse/src/adapter.rs#L89),
[flush/release](../../../../crates/layerfs-fuse/src/adapter.rs#L253),
[statfs/refusals](../../../../crates/layerfs-fuse/src/adapter.rs#L358),
[xattr mutations](../../../../crates/layerfs-fuse/src/adapter.rs#L955),
[semantic Flush](../../../../crates/layerfs-workspace/src/filesystem/read.rs#L223),
[provider trait](https://docs.rs/fuser/0.18.0/src/fuser/lib.rs.html).

## 13. Checked unmount stops, drains, detaches and proves absence

Public `WorkspaceApi::unmount` is a checked projection operation. The native
bound uses the original request deadline minus terminal-delivery allowance;
it does not stretch the timer to wait for retained owners.

```text
SDK unmount(id)
        |
authenticated selector/grant + lifecycle slot
        |
MountHandle::unmount
  finished -> idempotent success
  cleanup_failed -> terminal CleanupFailed
  deadline already expired -> Deadline
        |
stop_admission
  Adapter stopping=true -> new observed callbacks ENODEV
  Workspace stopping=true -> new semantic admission refused
  cleanup Flush/Release/Releasedir/Forget stay available
        |
drain within ORIGINAL deadline
  active_operations=0
  Projection handles=0
  Projection replies=0
  coherence is not Pending
        |
consume actual SessionUnmounter; ONE native detach attempt
  detach error -> cleanup_failed, retain lease/worker/custody
        |
if pending Session never started: release it/FD outside owner mutex
        |
wait for outer worker completion within deadline; join
  includes completion of its two callback loops
  worker error/panic -> cleanup_failed, retain semantic mounted state
        |
read bounded /proc/self/mountinfo; prove EXACT mountpoint absent
  still present/oversize unknown -> CleanupFailed
        |
MountLease::finish
  recheck no operation/reply/mutation/Pending/Projection handle
  mounted=false; remove ProjectionState/notifier
  clear Projection lookup refs; collect Nodes
  Workspace stopping=false
        |
finished=true; daemon clears slot.mount; completed terminal
```

Unmount does not invent FD releases. A backend READ finishing does not close
the caller's FD; Projection handles must drain. Native Local handles may
remain. Timeout keeps the worker/lease and stopped state; an explicit later
unmount can continue resumable ownership. Failed detach, worker error/panic or
proved residual mount sets terminal cleanup_failed and prevents guessed
automatic retry.

A Failed notifier is different from Pending. Checked detach may retire that
failed binding while the applied private mutation remains owned. Success
proves absence and projection teardown, not clean backing, canonical Commit,
release of every public view or refund of every physical owner.

Sources: [checked unmount](../../../../crates/layerfs-fuse/src/mount.rs#L294),
[drain/absence proof](../../../../crates/layerfs-fuse/src/mount.rs#L392),
[lease finish](../../../../crates/layerfs-workspace/src/runtime/lifecycle.rs#L158),
[daemon retained/completed result](../../../../crates/layerfs-daemon/src/control.rs#L455),
[public unmount](../../../../crates/layerfs-api/sdk/src/workspace.rs#L161).

## 14. Mount failure and Drop preserve distinct custody

```text
ATTACHMENT FAILURE
  original semantic cause recorded
        |
one checked attachment-resource cleanup
  active owner -> arena -> verified directory -> owned mount leaf
        +-- exact success/absence -> remove failed registry entry
        +-- error/unknown -> retain exact remaining resources

lifecycle selection:
  restore prior closed selection only after exact new-incarnation absence
  failed inspection != absence
```

```text
PRIMITIVE FUSE MOUNT FAILURE
  before MountLease admission:
    phase=Admission, retained=None

  after lease admission:
    Session/Binding/Worker/Deadline failure
        |
    stop both admissions
    MountFailure retains MountHandle
    primitive mount does not automatically roll back/unmount
        |
    explicit caller invokes checked unmount
    original error and cleanup result remain separate

FULL SDK OPEN
  semantic attachment can exist even if primitive mount admission failed
  daemon retains attachment/MountHandle before terminal delivery
  Retained ID or unknown result never proves native absence
```

The daemon's startup assembly can attempt checked shutdown once at the original
startup deadline after attachment/mount failure. Failed cleanup retains the
process and waits for an explicit signal before another attempt. That caller
policy is separate from primitive mount's retained-failure behavior.

Neither MountHandle nor MountLease has a semantic cleanup Drop that calls
`finish`. Dropping an outer JoinHandle detaches the thread; a running Session
still owns its Adapter/Workspace. Dropping an unstarted Session can invoke
the provider's native unmount-on-Drop and Adapter::destroy, but that attempt
does not establish absence, clear mounted admission or prove physical refunds.
The provider can log a failed Drop cleanup. Lost typed custody is not permission
to reset semantic state.

Sources: [attachment cleanup](../../../../crates/layerfs-workspace/src/runtime/attachment.rs#L43),
[lifecycle restoration](../../../../crates/layerfs-daemon/src/lifecycle.rs#L124),
[MountFailure/handle](../../../../crates/layerfs-fuse/src/mount.rs#L51),
[session custody](../../../../crates/layerfs-fuse/src/mount.rs#L190),
[startup failure custody](../../../../crates/layerfs-daemon/src/run.rs#L179),
[provider unmount-on-Drop](https://docs.rs/fuser/0.18.0/src/fuser/session.rs.html).

## 15. Unmount, clean close, graceful stop and Sandbox delete

```text
checked FUSE unmount
  callback drain + detach + absence proof + projection lease finish
        |
Workspace remains attached
  dirty bytes, Submission, Local handles and selected views can remain
        |
explicit Commit / explicit owner release as required
        |
native Workspace::close_clean_until
  refuses dirty/Submission/mount/active operations/handles/lookups/external pins
        |
stop admission; finish unused fund; clear read memo
close active backing -> arena -> eligible payloads -> verified empty directory
remove owned mount leaf; mark closed; retire registry entry
```

Public SDK exposes mount, exec, commit, status and unmount, plus its separate
held-view methods. It has no public `close_clean` method. Close does not
implicitly discard changes or force old owners to release.

```text
SIGINT/SIGTERM observed by daemon main thread
        |
one explicit shutdown attempt, bounded10 seconds
try_lock lifecycle slot
  Exec/Commit in flight -> Busy; retain process/control
        |
checked unmount
        |
native clean close
  dirty/held owner/cleanup failure -> retain owner and endpoint
        |
ONLY AFTER close succeeds:
stop control admission
shutdown retained socket/session; join acceptor
        |
daemon exits successfully

failed shutdown -> retain diagnostic/custody
wait for another explicit signal before another cleanup attempt
```

SandboxApi::delete separately destroys its owned Docker container and named
volume. Its cleanup result is not native close_clean, a successful canonical
Commit or an independently checked per-owner physical refund. No forced
process teardown is substituted for graceful owner resolution here.

Sources: [native clean close](../../../../crates/layerfs-workspace/src/runtime/lifecycle.rs#L73),
[daemon shutdown](../../../../crates/layerfs-daemon/src/lifecycle.rs#L208),
[signal loop](../../../../crates/layerfs-daemon/src/run.rs#L156),
[control stop](../../../../crates/layerfs-daemon/src/control.rs#L72),
[Sandbox delete](../../../../crates/layerfs-api/sdk/src/sandbox.rs),
[private close ownership](private_backing_workflow.md#18-close-unmount-and-deletion-have-different-outcomes).

## 16. Efficiency and retained limits

| Work | What current code saves | Work that still happens |
| --- | --- | --- |
| Mount | Loads root facts rather than materializing all files | Attachment/capability/root RPC, initial charged capacities, native mount/INIT |
| Repeated reached name | Same-Base positive memo and resident identity | Permission/selection/revision checks; cold/changed facts can need RPC |
| Identity after move | Inode-relative canonical lookup/list/readlink | Current active full-path replacement and depth-growth work remain |
| Read | Reads selected range, no eager whole-file checkout | Mapping/chunk/group amplification, charged buffer, actual Base/payload reads |
| Tiny WRITE | Shared packs avoid one full segment per tiny callback | Verified page/index publication, inverse refs, notification/accounting |
| Proven Base copy | Reuses exact immutable bytes as Base extents | Byte comparison and later Service-side canonical construction |
| Directory list | Bounded pages and owned continuation cookies | All examined/emitted rows and per-child attribute resolution |
| Live Commit | Captured G1 shares selected pages; G2 can publish successors | Admission/mutex/gate contention, actual C1/C2/History work and C5 cleanup |
| Old-view release | Visits selecting retirement cohorts | Actual identity/block/unlink proof and remaining payload-owner scans |

```text
distinct concurrency/resource limits
  one selected daemon mount and lifecycle control slot
  one live/closing authenticated control session; no waiting connection queue
  two FUSE callback event loops
  two projection reply slots
  one projected mutation owner
  shared Host primary upstream owner + conditional one metadata admission
  one authenticated-session mutex, serializing actual upstream calls
  one canonical construction worker
  max_background1 for kernel background requests
```

These numbers describe different owners. They do not impose one total POSIX
process/syscall, nor establish simultaneous throughput for every callback.
New public control calls during Commit return Busy; existing/background FUSE
calls have their own admission. Service byte reads need the primary upstream
slot; metadata inspection/serial reservation can admit a secondary call and
then wait behind the same transport mutex. See [the exact live-progress
boundaries](private_backing_workflow.md#15-live-progress-has-specific-admission-and-lock-boundaries).

Other current bounds are 128 native handles, 255-byte names, 65,536-byte/
256-component active locators, separate 4096-byte canonical LogicalPath,
4 GiB logical files, 32 public held-view leases, 4096-byte Exec commands,
8192-byte retained stdout/stderr each, 30-second requested Exec and 5-second
requested public unmount profiles. RAM Budget, physical quota, completion
credit and old-pin/failure custody remain enforced. Node capacity grows in
charged chunks; there is no new unlimited-file-count contract.

The [functional report](../PREMERGE-FUNCTIONAL-COMPLETION-20260929.md) and
[integration checkpoint](../PHASE45-INTEGRATION-CHECK-20260929.md) retain their
specific source/route scopes. This doc adds no physical benchmark or
all-callback runtime PASS. Historical numeric rows remain INELIGIBLE and the
frozen matched control remains NOT_RUN; additional memory qualification is
deferred to [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) under
the owner's direction. The separate pinned-read observation and deferred
#256/#270 scale/Commit work remain open. The source trace supplies no cold-cache
contract, constant total RAM claim, release approval or merge permission.
