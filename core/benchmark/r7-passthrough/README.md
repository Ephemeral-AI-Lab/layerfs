# R7 native passthrough control

Status: new exploratory harness arm P, not a product implementation or
qualification receipt. It requires Linux and the pinned, already authorized
fuser 0.18.0 source plus existing libc/nix versions. It introduces no dependency
family and edits no third-party file.

Build with the pinned toolchain, repository ARM flags and a sealed lockfile under
the single checkout lock. Before the first native build run the existing fuser
provenance guard. The lead owns lockfile preparation, build receipts and all
Docker commands.

`layerfs-r7-passthrough BACKING_DIRECTORY EMPTY_MOUNTPOINT` opens `/dev/fuse`,
makes one direct native mount and runs two receive loops. The orchestrator must
wait for the JSON `negotiated` record followed by actual all-loop `ready`
(configured = created = entered = 2, exited = 0), and verify mount readiness before
launching an ordinary external command. The orchestrator makes exactly one plain
unmount; session exit prints a JSON `drain` record with implemented callback
counts indexed by native opcode. fuser's private ForgetOne type prevents a
first-party batch-forget override; its default delivers individual FORGET
callbacks with exact ownership release. The actual batch kernel-request count,
and counts for other default callbacks, remain explicitly unavailable. A zero
in an unsupported slot is never a measured zero kernel request. The
harness never writes a fusectl entry, drops global caches, launches commands,
implicitly unmounts, retries an operation, or calls a backend fsync-family function.
It uses the authorized fuser `Session::into_runner` lifecycle. The main thread
observes startup and the first exit, while one session owner consumes every
created receiver's original join. `startup_not_ready` never permits a command;
the external owner must make its one explicit detach. A `joined` record carries
exact created/entered/exited/joined counts and original outcome details. Startup,
receiver, cleanup and output failures remain failures; no extra receive loop is
added and no failure detaches or replays automatically.

The selected kernel profile copies L's current settings: ASYNC_READ and
BIG_WRITES plus offered MAX_PAGES only; 128 KiB max write and readahead; background
and congestion depth 1; 1 ns time granularity; 60 s entry and attribute TTL;
cached reads; no writeback; allow_other with default_permissions; nosuid/nodev/
noatime. Negotiated bits and ABI are recorded, rather than treating offered bits
as selected. P and L must match these fields before comparison.

The backing directory is an independent native byte copy on one filesystem,
never the preserved fixture checkout or repository bind mount. Mutations change
that backing copy. O_PATH descriptors preserve inode custody after rename/unlink;
native device/inode associations preserve hard links. Inode numbers are stable
across remounts of the same backing copy. Live lookup associations and file
handles are reclaimed by FORGET and RELEASE; residual associations at terminal
drain are reported. This control's live ownership map is separately attributed
harness state; it is not a new product cache or an optimization of L.

Extended attributes and optional unsupported opcodes retain fuser's ENOSYS
responses. FSYNC and FSYNCDIR acknowledge successful no-ops, matching L under
the Disposable policy; neither calls a physical synchronization function.
The ordinary read/write, names, directories, hard links, symlinks and metadata
paths must pass the selected public filesystem oracle before a timing is taken.
Release build receipt 037 proves compilation with existing pinned dependencies.
No current receipt proves mounting or the public filesystem oracle.

The scoped independent functional oracle is `proof.py --mount MOUNT --stage
mutation`. Retain its JSON outside the mount, unmount once, mount the same
backing copy again and run `proof.py --mount MOUNT --stage remount --original
MUTATION_JSON`. It checks bytes across a native request window, localized writes,
truncate/regrow holes, hard links, symlinks, rename across parents, open-unlink
custody, permissions, ownership, and stable remount inode/mtime/ctime identity.
It also sends FSYNC/FSYNCDIR through the mount and verifies both replies, and
uses native directory entries to check that an immediate child's `..` carries
the root inode 1 rather than the backing root's native inode plus 1.
`proof.py --mount MOUNT --stage wide` is a separate count/functional diagnostic
at 64, 128 and 256 files with 211-byte names, crossing reply windows and checking
complete names, dots and unique identity. It records no timings and does not
claim a huge-namespace qualification. Readdir reopens one directory per reply,
seeks the original native cookie and uses libc's bounded DIR buffer; no directory
population is collected inside P. Its live lookup/handle maps remain explicitly
attributed control-arm state.
Run it with its prospectively declared independent proof wall stop and one
attempt. Final drain must report zero held open handles; leftover kernel lookups
are closed by destruction of the terminal session, not claimed to have received
FORGET callbacks. Live association counts in the JSON are harness resource
observations, rather than product resident bounds.
