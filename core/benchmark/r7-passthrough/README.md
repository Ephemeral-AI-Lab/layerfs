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
wait for the JSON `negotiated` record and verify actual mount readiness before
launching an ordinary external command. The orchestrator makes exactly one plain
unmount; session exit prints a JSON `drain` record with opcode counts. The
harness never writes a fusectl entry, drops global caches, launches commands,
implicitly unmounts, retries an operation, or calls fsync.

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
responses. Fsync is intentionally not implemented under the Disposable policy.
The ordinary read/write, names, directories, hard links, symlinks and metadata
paths must pass the selected public filesystem oracle before a timing is taken.
No current receipt proves this implementation compiled or mounted.

The scoped independent functional oracle is `proof.py --mount MOUNT --stage
mutation`. Retain its JSON outside the mount, unmount once, mount the same
backing copy again and run `proof.py --mount MOUNT --stage remount --original
MUTATION_JSON`. It checks bytes across a native request window, localized writes,
truncate/regrow holes, hard links, symlinks, rename across parents, open-unlink
custody, permissions, ownership, and stable remount inode/mtime/ctime identity.
Run it with its prospectively declared independent proof wall stop and one
attempt. Final drain must report zero held open handles; leftover kernel lookups
are closed by destruction of the terminal session, not claimed to have received
FORGET callbacks. Live association counts in the JSON are harness resource
observations, rather than product resident bounds.
