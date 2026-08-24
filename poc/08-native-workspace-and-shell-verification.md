# Native APFS workspace and Bash verification

Status: **required PoC product and verification contract; the SDK/evaluator now
exists, and current proof/limitations are recorded in `poc/17`**.

## 1. Required user story

```text
open canonical Store at root R0
  -> materialize R0 into an owned ordinary APFS directory W0
  -> materialize directly as ExternalWorkspace or consume ManagedWorkspace into it
  -> run real /bin/bash and ordinary host tools with cwd=W0
  -> wait until every owned child/background writer is gone
  -> freeze and scan W0 exactly
  -> publish root R1 once
  -> close the process
  -> reopen the Store and materialize R1 into a different directory W1
  -> rerun read-only Bash assertions in W1
  -> compare W0/W1 with the canonical oracle
```

PASS means a user can inspect, execute and modify ordinary physical files. It
does not mean the SQLite/CAS object directory itself is a POSIX workspace.

## 2. Canonical store versus physical workspace

| LayerFS Store | Materialized workspace |
|---|---|
| immutable payload chunks | ordinary regular APFS files |
| file extent B+ trees | ordinary byte-addressable file descriptors |
| directory B+ trees | ordinary directories and path lookup |
| canonical symbolic-link target objects | native symbolic links |
| immutable roots/refs | derived mutable checkout |
| authoritative | disposable until a successful capture publishes a root |

Direct SDK reads do not require physical files. Materialization constructs the
physical tree only when native compatibility is requested.

## 3. Supported filesystem subset

| Native kind/metadata | PoC behavior | Capture identity |
|---|---|---|
| regular file | required | exact bytes in mode-free `FileStateV3` + mode/mtime in `PortableMetadataV1` |
| directory | required, including empty | exact child names/targets in mode-free `DirectoryStateV1` + mode/mtime in `PortableMetadataV1` |
| symbolic link | required for relative/absolute/dangling targets without NUL; never followed | exact growing-buffer `readlinkat` bytes; truncation rejected |
| executable bits | required | selected `0o111` bits plus frozen supported mode mask |
| hard link | required in Apple profile; multiple paths share one canonical `InodeId` | native `(device,inode)` groups are scan observations, never canonical identity |
| FIFO/socket/device | typed `UnsupportedNativeKind` | never opened as regular content |
| sparse allocation | logical bytes remain exact; physical-hole preservation unavailable | zero bytes are content unless a future canonical hole extent exists |
| mtime | canonical nanosecond portable metadata; still cannot prove unchanged bytes alone | exact round trip for build-tool compatibility |
| atime/ctime/birthtime/native inode | observed/noncanonical | never content or operational-root equality |
| xattr/ACL/resource fork/BSD flags | exact round trip when included in frozen `AppleMetadataV1`; typed failure otherwise | typed canonical `apple.*` extension metadata interpreted only by AppleDriver |

Canonical name rules reject empty, `.`/`..`, slash, NUL, oversize, duplicate,
unrepresentable, case-colliding and normalization-colliding siblings before a
native tree is classified Complete.

## 4. Workspace state and API

```text
Materializing
  -> ManagedWorkspace          private path, exact managed authority
      -> ManagedDirty
      -> ExternalWorkspace     consuming into_external()
  -> ExternalWorkspace         caller-known destination from creation

ManagedDirty --capture--> Captured
ExternalWorkspace --cooperative freeze/full scan/capture--> Captured
ManagedDirty/ExternalWorkspace --discard--> Discarded
any LayerFS-owned or registered child/writer --capture--> WorkspaceBusy
```

Minimal SDK surface:

```rust
impl ManagedWorkspace {
    pub fn write_at(&mut self, path: &CanonicalPath, offset: u64,
                    bytes: &[u8]) -> Result<()>;
    pub fn replace(&mut self, path: &CanonicalPath, range: Range<u64>,
                   bytes: &[u8]) -> Result<()>;

    pub fn capture(&mut self) -> Result<NamespaceRoot>;
    pub fn into_external(self) -> Result<ExternalWorkspace>;
    pub fn discard(&mut self) -> Result<()>;
}

impl ExternalWorkspace {
    pub fn path(&self) -> &Path;
    pub fn capture_quiescent(&mut self) -> Result<NamespaceRoot>;
    pub fn discard(&mut self) -> Result<()>;
}
```

ManagedWorkspace has no resolvable public path. ExternalWorkspace is the only
type that exposes one. A caller-requested materialization at a known path starts
External; conversion from Managed consumes the managed handle. Capture errors
do not consume either workspace.

No shell parser, command DSL or process abstraction is added to the SDK. Tests
and the example use `std::process::Command` directly.

## 5. Child-process and quiescence contract

```text
before spawn:
  materialization Complete
  ExternalWorkspace acquired
  no LayerFS managed writer active

spawn:
  executable = /bin/bash
  cwd = exact workspace path
  stdin/stdout/stderr = explicit test-owned handles
  environment = recorded; semantic fixture variables forbidden

before capture:
  direct child exited and was waited
  evaluator-owned process group has no surviving registered child
  no LayerFS-owned or registered writer lease remains
  all test-created descriptors are closed
  workspace directory identity revalidated
```

The shell test must not daemonize. A controlled background-writer case proves
only owned/registered writer rejection. Independently launched, escaped or
hostile same-UID writers are outside AppleWorkspaceV1; the caller attests the
tree remains unchanged during `capture_quiescent`. A nonzero command exit never
triggers implicit publication.

## 6. Deterministic native fixture

Approximately 3 MiB keeps the integrated run fast while structural unit tests
exercise deep file and directory trees separately.

```text
project/
├── README.md                    4 KiB deterministic text
├── src/
│   ├── main.rs                 64 KiB deterministic text
│   ├── lib.rs                 256 KiB deterministic text
│   └── obsolete.rs              1 KiB deterministic text
├── assets/
│   └── blob.bin                 2 MiB deterministic binary
├── data/
│   └── repeated.bin           768 KiB repeated blocks
├── scripts/
│   └── check.sh                 1 KiB executable script
├── current-readme -> README.md
├── hardlink-readme              hard link to README.md
└── empty/
```

`scripts/check.sh` uses only deterministic read operations:

```bash
#!/bin/bash
set -euo pipefail

test -f README.md
test -f src/main.rs -o -f src/cli.rs
test -x scripts/check.sh
test "$(readlink current-readme)" = "README.md"
test "$(stat -f '%i' README.md)" = "$(stat -f '%i' hardlink-readme)"
grep -q "layerfs-poc" README.md
test -d empty
```

## 7. Required Bash mutation script

The evaluator writes the script outside the workspace, hashes it into the test
receipt, then executes it with the workspace as `cwd`:

```bash
#!/bin/bash
set -euo pipefail

# Complete-file read through normal host file descriptors.
grep -q "layerfs-poc" README.md

# Same-size in-place native overwrite.
printf 'LFS1' | dd of=assets/blob.bin bs=1 seek=4096 conv=notrunc 2>/dev/null

# Length-changing append and complete truncate.
printf '\nshell-update\n' >> README.md
: > data/repeated.bin

# Namespace and complete-file replacement behavior.
mkdir -p generated/nested
printf 'generated-by-bash\n' > generated/nested/result.txt
mv src/main.rs src/cli.rs
rm src/obsolete.rs

# Mode and symbolic-link behavior.
chmod +x scripts/check.sh
ln -s ../src/cli.rs generated/cli-link
test "$(readlink generated/cli-link)" = "../src/cli.rs"

# Hard-link topology: both paths must name one native inode.
ln README.md generated/readme-hardlink
printf 'linked-update\n' >> generated/readme-hardlink
tail -n 1 README.md | grep -q 'linked-update'

# One deterministic Apple metadata mutation.
/usr/bin/xattr -w com.layerfs.poc shell-metadata README.md
test "$(/usr/bin/xattr -p com.layerfs.poc README.md)" = "shell-metadata"

# Run a workspace script after the mutations.
/bin/bash ./scripts/check.sh
```

The script intentionally covers both same-size and count-changing physical
updates. Because Bash is an arbitrary external writer, the subsequent capture
is not allowed to reuse the managed edit descriptor path.

## 8. Capture algorithm after Bash

```text
acquire exclusive workspace admission
revalidate workspace directory device/inode and ownership marker
enumerate every supported relative path without following links
for each directory:
  record exact canonical UTF-8 child name, kind and supported metadata
for each symbolic link:
  readlink exact target bytes; never traverse it
for each regular file:
  group opaque native hard-link keys in an owned temporary SQLite scratch table
  require stable native link count == aliases observed inside workspace
  otherwise fail ExternalHardLinkBoundary
  with live into_external topology provenance, reuse an InodeId only for an
    exactly surviving group key; split keeps the surviving original group's ID,
    merge keeps the current native target group's prior ID, and new groups allocate
  without that live topology map, allocate one new InodeId per complete group
  stream ContentDigest with bounded buffers
  if prior content digest matches, retain prior FileStateRoot
  otherwise stream FastCDC/CAS and construct the new FileStateRoot
  independently retain or replace MetadataRoot from exact mode/mtime/extension equality
enumerate one exact Apple xattr set (including FinderInfo/resource fork), ACL
and supported flags into typed extension metadata
reject unsupported special kinds or metadata that cannot be represented exactly
construct persistent directory roots bottom-up
compare the complete canonical oracle
publish objects/delta/root/ref through one expected-head transaction/COMMIT
```

Worst-case work remains:

```text
Theta(total supported paths
    + unique current regular-file bytes for digest
    + changed current regular-file bytes reread for CDC/CAS
    + uncached prior bytes compared
    + represented metadata bytes
    + indexed hard-link grouping)
```

This is a compatibility cost, not the complexity of managed or future
write-intercepted edits.

## 9. Oracle and exact comparison

The evaluator builds an independent native oracle using no LayerFS encoder:

```text
NativeEntry {
  relative_path_bytes
  kind
  native_hardlink_group              // oracle-only device/inode grouping
  supported_mode
  typed_extension_metadata_digest
  regular_file_length + BLAKE3(bytes), or
  symbolic_link_target_bytes
}
```

Rules:

- enumerate with no-follow operations;
- sort by exact relative path bytes;
- stream regular files; no source-sized allocation;
- include empty directories;
- compare kinds before content;
- compare complete hard-link alias groups and native link counts;
- ignore only metadata explicitly excluded by the frozen policy;
- compare the native oracle before capture, canonical reconstruction after
  capture, and a fresh rematerialization after process reopen.

## 10. Realistic correctness sequence

| Step | Action | Required result |
|---|---|---|
| W0 | import fixture as root `R0` | exact canonical root/profile |
| W1 | cold materialize `R0` | ordinary APFS oracle exact |
| W2 | run read-only `check.sh` | exit 0; no root change |
| W3 | one managed 4 KiB write/capture | local structural bounds; `R1` exact |
| W4 | rematerialize as ExternalWorkspace | caller-visible path; no managed fast authority claimed |
| W5 | run Bash mutation script | exit 0; physical oracle records all content/path/mode/symlink/hard-link changes |
| W5m | run tiny Apple writable-mmap helper, `msync`, unmap and exit | mapped mutation present before cooperative capture |
| W6 | keep registered writer alive and attempt capture | exact `WorkspaceBusy`; head unchanged; no claim about unregistered writers |
| W7 | reap writer and capture | full-workspace class; one transaction/COMMIT; root `R2` |
| W8 | close/reopen and materialize `R2` elsewhere | independent physical oracle exact |
| W9 | rerun read-only assertions | exit 0 against reopened result |
| W10 | run a nonzero command | no automatic publication; dirty workspace inspectable |
| W11 | discard or explicitly capture | terminal temp/child/descriptor state zero |
| W12 | fork `R2`, run divergent shell changes, rollback one ref | every retained root exact |
| W13 | close all workspaces/readers and run offline compaction | retained roots rematerialize and pass Bash assertions; authenticated-unreachable objects are absent |

## 11. Failure cases

| Failure | Required disposition |
|---|---|
| shell executable missing | typed process-launch error; no publication |
| shell exits nonzero | record exit/stderr; no implicit capture |
| child remains alive | `WorkspaceBusy`; no scan/publication |
| path becomes symlink during regular-file scan | identity conflict; no followed target |
| hard-link group changes during scan | identity/link-count conflict; head unchanged |
| stable native link count exceeds aliases inside workspace | `ExternalHardLinkBoundary`; incomplete topology is not canonicalized |
| FIFO/socket/device appears | `UnsupportedNativeKind`; never block/open as file |
| case/normalization collision | `NativeNameCollision`; no Complete authority |
| file changes during scan | identity/length/digest revalidation conflict; no exact result claimed |
| expected head changes | conflict before visible publication |
| COMMIT acknowledgement lost | fresh requested/prior/different/indeterminate reconciliation |
| process crashes before capture | canonical head unchanged; workspace reopens `Unknown` |
| process crashes after COMMIT | fresh Store authority determines the accepted root |

## 12. Small diagnostic measurement

One release invocation runs W0–W13 three times in fresh sibling workspaces and
stops. Complete-wall target remains `<=30 s`, including command launch, capture,
oracle, reopen and cleanup.

Report:

```text
shell executable + argv + exit status
fixture/script hashes
paths by kind; bytes scanned/hashed/written
file/namespace nodes read/created/reused
managed versus external capture class
child/process/descriptor high-water and terminal state
transactions and COMMITs
RSS, owned Q, largest buffer
APFS clone route/outcome and native logical writes
complete wall, user CPU, system CPU when observable
```

This measurement proves that the integrated product path is usable and bounded.
It does not authorize production throughput, p95, multi-user security, live
database snapshot consistency, or arbitrary POSIX metadata claims.

## 13. PASS boundary

- [x] ordinary files/directories/symlinks and supported modes round-trip;
- [x] a real Bash child reads, executes and mutates the physical workspace;
- [x] writable mmap mutation is flushed/unmapped and captured after process exit;
- [x] capture refuses LayerFS-owned/registered writers and succeeds after caller-attested cooperative quiescence;
- [x] Bash changes become one exact immutable root through full-scan capture;
- [x] unchanged history-shaped files retain their prior `FileStateRoot`;
- [x] process reopen plus fresh rematerialization reproduces the physical oracle;
- [x] managed edits still retain their local structural path;
- [x] persistent namespace structural tests are independent of small fixture size;
- [x] hard-link topology and common Apple metadata round-trip; unsupported special files fail typed without head movement;
- [x] one compact campaign passes with terminal owned state zero.
- [x] offline compaction preserves every retained workspace and removes only authenticated-unreachable objects.
