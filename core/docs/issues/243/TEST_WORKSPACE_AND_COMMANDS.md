# #243 Generic Workspace shell test: prepared tree and commands

> **Status:** Current planning checklist; no release candidate exists.

This is a proposed **functional** scenario for the public
`WorkspaceApi::exec(command)` route. The daemon runs `/bin/sh -c` with the mounted
Workspace as its working directory. The package names are local test data; no
package manager, registry, network access, or LayerFS-specific edit tool is
involved. See the [#232 route correction](../232/SHELL_ROUTE_CORRECTION.md) and
the [#243 rollout plan](IMPLEMENTATION_PLAN.md). Freeze this specification,
the source and binary seals, image identity, fixture hashes, output path, cache
contract, and numerical targets before any live performance collection.

## Prepared inputs

Prepare one closed, validated version-1 Store/Branch master outside the timed
child. Give each case an independent writable byte copy of that master and
record the clone method; a clone is not a cold-cache claim. Put version-2 files
at `/fixtures/v2/` **in the sandbox image, outside the FUSE mount**. The image
contents are immutable during the run. All destinations below are relative to
the mounted Workspace working directory. Record a manifest with path, type,
portable mode, byte length, and actual SHA-256 for both inputs; do not invent
hashes in this plan.

Every file starts in mode `0644`; directories start in mode `0755`. All text is
UTF-8 with LF endings and no implicit extra newline. The fixture builder writes
the exact bytes below; `pad(header, size)` means `header` followed by ASCII
spaces to exactly `size` bytes (reject a header longer than `size`). This simple
padding is for a functional screen, not a representative compression or speed
claim.

| Path relative to Workspace | Version 1 in Workspace | Version 2 in image |
| --- | --- | --- |
| `node_modules/@fixture/core/package.json` | `{"name":"@fixture/core","version":"1.0.0"}\n` | `{"name":"@fixture/core","version":"2.0.0"}\n` |
| `node_modules/@fixture/core/index.js` | `export const core = 1;\n` | No replacement; must stay unchanged. |
| `node_modules/@fixture/core/BUILD_ID` | `1.0.0\n` | No copied file; the command changes byte 0 to `2`, yielding `2.0.0\n`. |
| `node_modules/@fixture/parser/package.json` | `{"name":"@fixture/parser","version":"1.0.0"}\n` | `{"name":"@fixture/parser","version":"2.0.0"}\n` |
| `node_modules/@fixture/parser/lib/parse.js` | `pad("export const parse = () => 1;\n", 65536)` | `pad("export const parse = () => 2;\n", 65536)` |
| `node_modules/@fixture/parser/lib/legacy.js` | `export const legacy = true;\n` | Absent. |
| `node_modules/@fixture/parser/generated/tokenize.js` | Absent; `generated/` is absent too. | `export const tokenize = s => s.split(' ');\n` |
| `node_modules/@fixture/ui/package.json` | `{"name":"@fixture/ui","version":"1.0.0"}\n` | `{"name":"@fixture/ui","version":"2.0.0"}\n` |
| `node_modules/@fixture/ui/dist/ui.js` | `pad("export const ui = 1;\n", 1048576)` | `pad("export const ui = 2;\n", 1048576)` |
| `package-lock.json` at Workspace root | `{"name":"fixture-app","lockfileVersion":3,"packages":{"node_modules/@fixture/core":{"version":"1.0.0"},"node_modules/@fixture/parser":{"version":"1.0.0"},"node_modules/@fixture/ui":{"version":"1.0.0"}}}\n` | The same exact JSON byte pattern with all three versions changed to `2.0.0`. |

For each package row, the version-2 image path drops
`node_modules/@fixture/`: for example, parser `lib/parse.js` lives at
`/fixtures/v2/parser/lib/parse.js`. The lockfile lives at
`/fixtures/v2/package-lock.json`.

The version-1 tree contains exactly these nine files and their parent
directories. Neither `*.next` file exists initially. The image's
`/fixtures/v2/` tree contains exactly the seven replacement files used by `cp`
below: three package manifests, parser `lib/parse.js`, parser
`generated/tokenize.js`, UI `dist/ui.js`, and the root lockfile. The version-2
manifest records their actual bytes and hashes. The
image may contain required runtime programs, but those are not Workspace input.

## One mixed package-refresh Exec

Pass the following string unchanged to **one** public `WorkspaceApi::exec`
call. `/bin/sh` parses it. The command uses ordinary shell utilities and POSIX
file operations. `umask 022` makes new file and directory modes predictable;
the verifier still checks the observed portable modes.

```sh
set -eu
umask 022
base=node_modules/@fixture

printf '2' | dd of="$base/core/BUILD_ID" bs=1 seek=0 conv=notrunc 2>/dev/null
cp /fixtures/v2/core/package.json "$base/core/package.json"

cp /fixtures/v2/parser/package.json "$base/parser/package.json"
cp /fixtures/v2/parser/lib/parse.js "$base/parser/lib/parse.js"
mkdir "$base/parser/generated"
cp /fixtures/v2/parser/generated/tokenize.js "$base/parser/generated/tokenize.js"
rm "$base/parser/lib/legacy.js"

cp /fixtures/v2/ui/package.json "$base/ui/package.json"
cp /fixtures/v2/ui/dist/ui.js "$base/ui/dist/ui.js.next"
mv -f "$base/ui/dist/ui.js.next" "$base/ui/dist/ui.js"

cp /fixtures/v2/package-lock.json package-lock.json.next
mv -f package-lock.json.next package-lock.json
```

On exit status zero, issue **one explicit public SDK Commit**. Do not count
the Exec result as publication. The expected final tree has nine files:
`core/package.json`, `core/index.js`, `core/BUILD_ID`,
`parser/package.json`, `parser/lib/parse.js`,
`parser/generated/tokenize.js`, `ui/package.json`, `ui/dist/ui.js`, and the
root `package-lock.json`. It has no `parser/lib/legacy.js`, `ui.js.next`, or
`package-lock.json.next`. `core/index.js` is byte-identical to version 1;
`core/BUILD_ID` is `2.0.0\n`; every copied or renamed file equals its declared
version-2 fixture bytes. The exact directory set is the root,
`node_modules`, `node_modules/@fixture`, `core`, `parser`, `parser/lib`,
`parser/generated`, `ui`, and `ui/dist` under that prefix.

The route gate is **no accepted range ioctl and no LayerFS-specific edit tool**.
While the carrier exists, require zero `range_state` and `range_edit` Status
counters and zero accepted range-ioctl payloads. If retirement removes those
fields, prove the carrier is absent at the pinned source identity and retain
an equivalent negative-capability check (the former range request returns
an unsupported-operation error such as `ENOTTY`, with the exact code frozen
at retirement) outside the measured command; a missing counter is not a
measured zero. Record the published Workspace Status counters, bytes and piece-index
work. Its current `write` counter aggregates several
namespace mutations, so it does not prove a count of FUSE WRITE calls by itself.
If a future diagnostic provides operation-specific FUSE trace, retain its WRITE,
SETATTR, CREATE, MKDIR, RENAME and UNLINK counts separately; do not invent them
from Status. The script should exercise writes, creation, rename and unlink;
exact callback counts depend on the utilities' legal syscall sequences. Any
range ioctl requires investigation, not reinterpretation as an optimized pass.

## Separate focused cases

Do not hide small-write costs inside the mixed batch. Each focused case has
its **own sealed prepared-master identity** containing the nine-file package
tree plus the additional file named below; it does not add setup writes to a
sample clone. Give each attempt a fresh independent writable copy, and
register the cases with their own identities and byte oracles:

| Case | Additional prepared bytes | Oracle |
| --- | --- | --- |
| 4 KiB positional overwrite | `large.bin` is 10 MiB of ASCII `A`; immutable `/fixtures/v2/patch-4k.bin` is 4096 ASCII `P` bytes. | Exactly 10 MiB: 5 MiB `A`, 4 KiB `P`, then the original `A` suffix. |
| Repeated one-byte writes | `repeated.bin` is 64 KiB of ASCII `.`. | Exactly 64 KiB: sixteen `X` bytes followed by the original dots; record per-callback and aggregate piece visits. |

The exact commands, each passed as its own Exec string, are:

```sh
dd if=/fixtures/v2/patch-4k.bin of=large.bin bs=4096 seek=1280 conv=notrunc 2>/dev/null
```

```sh
i=0; while [ "$i" -lt 16 ]; do printf X | dd of=repeated.bin bs=1 seek="$i" conv=notrunc 2>/dev/null; i=$((i + 1)); done
```

Both cases require zero range ioctls. Add append, truncate, and shell-driven
insert/delete as separately frozen cases if their distinct behaviors become
part of this issue's gate; they are not implicit successes of the package batch.

## Independent oracle, failure, and custody

Use a separate identity-matched, read-only verifier after the measured command.
It reopens the **published Store and History**, enumerates the entire final
root (files and directories), and compares every file's size, SHA-256, and
portable mode against a manifest derived from the fixture recipe plus the
declared byte edit. It checks exact path presence and absence, the new Commit
and Branch head, and the old Commit/root still resolving to the complete
version-1 tree and unchanged hashes. The verifier must not use the mounted
Workspace as its oracle or repair expected data from the result. Record its
wall separately from Exec and Commit.

Run this failure case on **another fresh copy**, without a Commit:

```sh
set -eu
printf 'BAD' > node_modules/@fixture/core/package.json
false
```

Require a definite nonzero Exec exit, no public Commit invocation, the Branch
head and historical root unchanged, and the uncommitted private edit discarded
with the Workspace. An unknown/deadline result is a failure requiring its own
classification; do not assume the command or its side effects stopped merely
from a timeout. Unmount and delete the Sandbox through the public SDK for every
case; record cleanup and any retained Store disposition independently.

The first live run needs a prospective case registry and receipt contract:
source/tree/compilation/dependency seals, binary and image IDs, workload and
fixture hashes, exact command, clone method, complete command wall, raw
substeps and callback counts, worker limit 1, cache contract, verifier identity,
and fresh output path. Keep one sample per case per arm, all failures and
ineligible attempts, and append-only receipts. Do not promote cache-uncontrolled
raw timing to a latency PASS, relabel the [historical opt-in ioctl campaign](../232/evidence/phase2-all-ioctl/REPORT.md), raise product limits or deadlines,
or shrink the workload to make a cell pass. This plan makes no performance or
release claim.
