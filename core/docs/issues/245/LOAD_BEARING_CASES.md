# #245 Load-bearing ordinary-shell cases beyond Phase 1

> **Status:** Research; informative and not a product contract.

These deferred cases challenge the proposed scalable range-based COW Workspace
after the earlier [#243 shell selection](../243/PHASE1_CONTRACT.md) is repaired
and verified. The sizes and commands below are proposals, not a frozen test
selection or measured result. No success, speed, or capacity claim follows.

The [post-#252 source review](LOAD_BEARING_CASE_REVIEW.md) evaluates every row
against product source `f74dbe77da12fa533587be8a578375bce3f19373` and
records one uncovered base-resident directory-move case as
[#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258). The “current”
1,024-piece, 256-interval and 8 MiB final-replacement figures below belong to
this earlier proposal, not the later generic `SaveFile` implementation. The
case definitions remain proposed and have not been sampled.

The caller remains the public `WorkspaceApi::exec(command)`, which runs
`/bin/sh -c` in the mounted Workspace. Inputs live in the Linux image under
`/fixtures/stress/`, outside the mount; no registry, network, LayerFS edit tool,
or ioctl participates. Each case should start from its own validated prepared
Store/Branch master, cloned outside the timed child, and use one ordinary Exec.
Mutating success cases then call Commit only after a confirmed zero exit. The independent oracle
must reopen the published Store and History, compare the **entire** expected
tree (paths, types, modes, sizes, hashes, absence), and prove the old Commit
still resolves. For a failed Exec, verify that no Commit was called and classify
its private state and cleanup separately. Commands below show the intended POSIX
shape; image contents, exact command bytes, expected hashes, limits, cache
state, receipt schema, and case registry must be frozen **before** measurement.

## Separate pressures to expose

- The current file representation rebuilds at most 1,024 pieces, counts at most
  256 changed intervals in an existing file, and caps its final non-base
  replacement at 8 MiB. These are file/Commit-route limits, not shell-command
  limits. See [piece checks](../../../crates/layerfs-workspace/src/overlay/pieces.rs)
  and [Bridge request checks](../../../crates/layerfs-bridge/src/contract/request.rs).
- A generation currently accepts at most 128 dirty identities and 128 changed
  names, with a 32 KiB metadata request. The live node table is 256 entries,
  the handle table 128, and the directory-cookie table 1,024; those runtime
  limits are separate from piece count and require their own observations.
  See [frontier and tables](../../../crates/layerfs-workspace/src/runtime/state.rs).
- The current sandbox sets a **1 GiB Workspace disk budget**, **16 MiB Workspace
  memory budget**, and **512 MiB container memory**. Record physical private
  backing, page cache, cgroup memory, and retained generation space rather than
  assuming that bounded process heap proves bounded resource use. See
  [sandbox configuration](../../../crates/layerfs-sandbox/src/docker.rs).
- `ExecResult` retains at most **8 KiB of stdout and 8 KiB of stderr**, with
  separate truncation flags. That is an output-result limit, not a file-write
  or piece limit. The 8 MiB cap on Docker **daemon stderr diagnostic capture**
  is another, separate channel; it is not the Workspace log-file limit. See
  [Exec result contract](../../../crates/layerfs-bridge/src/contract/execution.rs)
  and [daemon output drain](../../../crates/layerfs-daemon/src/execution.rs).

## Candidate cases

All sizes and counts in this table are **proposed probes**, not acceptance
thresholds or proven capability. Use deterministic, manifest-listed bytes in
the image, not a package manager whose upstream content or network can change.
The notation `/fixtures/stress/...` names image-local input, never a Workspace
destination.

| Case | Proposed prepared input and ordinary-shell shape | Independent oracle | Main pressure and question |
| --- | --- | --- | --- |
| Many packages | Base has roughly 160 small packages, each with a manifest and source file; image has a matching `packages-v2` tree. One Exec uses ordinary `cp` for refreshed files, `mkdir`/`cp` for new packages, and `rm` for obsolete files. | Exact version-2 package and lockfile tree; no obsolete or temp names; old package tree preserved by old Commit. | More than 128 changed names/identities across many directories. Can the generation, Commit namespace encoding, lookup table, and directory cursors scale without skipping a package? |
| One large package | One package contains roughly 160 distinct small files under several subdirectories plus a proposed 192 MiB distribution file; the image has its replacement tree. Shell creates new subdirectories, copies the files, then replaces its manifest by `cp ... package.json.next; mv -f ...`. | Every file and directory, including the distribution file, a deep file and the replaced manifest, matches the image manifest. | Concentrated width/depth and large payload in one package; distinguish namespace entry/cookie pressure from fresh-file streaming and disk use. |
| Move and replace | Base has `packages/old/`, `packages/new/` and an existing destination file. Shell uses `mv packages/old/subtree packages/new/subtree`, then `cp /fixtures/stress/file-v2 packages/new/subtree/file.next; mv -f packages/new/subtree/file.next packages/new/subtree/file`; a separate variant moves a whole directory. | No old names; all moved descendants and modes present under the new names; replacement has the new bytes; old Commit retains original bindings. | Cross-directory rename, overwrite, parent accounting, and live open-handle identity. Distinguish file rename from directory rename; record the exact failing syscall and underlying Workspace error. |
| Remove and copy | Base has a modest package tree and unrelated untouched files. Shell runs `rm -r packages/obsolete` and `cp -R /fixtures/stress/replacement/. packages/replacement/`. | Tombstoned base paths stay absent; copied tree is complete; unrelated files remain byte-identical and reuse their canonical roots where applicable. | Whiteout/namespace deletion, recursive traversal, fresh-file streaming, and cleanup of abandoned upper payloads. Count identities and names independently. |
| Tiny edit in large existing file | Base holds a proposed 500 MiB file. `dd if=/fixtures/stress/patch-4k.bin of=large.bin bs=4096 seek=<middle> conv=notrunc 2>/dev/null`. | Exact full-file hash/size and bounded-range byte checks; original Commit's large file unchanged. | Does a local write and Commit avoid reading, copying, scanning, or spooling untouched 500 MiB? Record piece/page visits, payload bytes, RSS, cache and disk high water. |
| Large existing-file edit | Base has a proposed 64 MiB file; image has a deterministic replacement larger than 8 MiB. `dd if=/fixtures/stress/replacement.bin of=large.bin bs=<fixed> seek=<fixed> conv=notrunc 2>/dev/null`. A separate pattern makes more than 256 disjoint small edits. | Full resulting bytes and old Commit; no partial publication on a capacity failure. | Separate replacement-byte and interval ceilings; streaming Commit must eliminate both without one huge request or unbounded memory. Actual FUSE callback count is measured, not inferred from `dd` arguments. |
| Large fresh file and append | Image has a proposed 256 MiB file. `cp /fixtures/stress/fresh.bin packages/big/new.bin`; a separate case uses `cat /fixtures/stress/append.bin >> packages/big/existing.bin`. | Complete new/extended bytes and modes, old root unchanged, no partial file in the published tree. | Thousands of ordinary writes may exceed 1,024 pieces; bounded private backing and one-pass Commit construction. New-file and existing-file append use different Commit routes. |
| Shell-driven prepend/insert | Base has a proposed 64 MiB file. Shell builds `file.next` with `cat /fixtures/stress/prefix.bin file > file.next` and `mv -f file.next file`; a separate in-place-shift algorithm may be registered later. | Exact shifted bytes, length, replacement semantics, and old Commit. | This *command* sends the full new file through FUSE; a local index cannot erase that I/O. Test temp-file write/rename throughput and namespace correctness separately from true in-place shifting. |
| Printed logs versus a Workspace log file | Image has deterministic 16 KiB stdout and stderr files plus a proposed multi-MiB log payload. One Exec runs `cat /fixtures/stress/stdout.txt; cat /fixtures/stress/stderr.txt >&2`; another runs `cat /fixtures/stress/log.bin >> logs/build.log`. | For printing: exact retained first 8 KiB of each stream, both truncation flags, exit status, and unchanged Branch head. For file logging: complete committed `logs/build.log` bytes and old root. | Printing exercises bounded Exec output and pipe draining, **not** FUSE writes. Logging to `/workspace` exercises FUSE append, piece growth, backing disk, and Commit. Diagnostic daemon stderr is a third channel. |

For each case, capture read/lookup/write/size/create/unlink/rename callbacks and
bytes where available, per-call piece and index-page work, Commit input/scan and
Store reuse, complete command wall, Exec/Commit/cleanup separately, peak private
disk and memory, and explicit field availability. Status `write` is an aggregate
that includes namespace mutations; it cannot substitute for a FUSE WRITE
callback count. No throughput or asymptotic claim follows from a successful
full-tree oracle alone.

## Promotion rule

These cases are an **extension**, not replacements for any #243 Phase 1 failure
or registered #232 shape. Promote a small, named selection only after the
architecture and earlier rename/cleanup/count gaps are understood. Freeze the
exact per-case image and Store manifests, command bytes, oracle, expected case
cardinality, source and build identities, cache contract, timer boundaries,
numerical targets, resource limits, and admission fields prospectively. Keep
all attempted, failed, ineligible, and unrun cases in the resulting report; do
not shrink a valid failing workload, raise a limit, or omit a cell after seeing
its result. Follow the [benchmark rules](../../../../docs/general/benchmark_rules.md)
and preserve the [#243 baseline](../243/evidence/phase2-ordinary-shell-v1/REPORT.md).
