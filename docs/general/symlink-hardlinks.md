# Workspace symlink hard links

> **Status:** Current general guide. Implemented source extension; no release candidate is claimed.

The owner approved symlink hard-link support on 2026-11-06 after the portable
fsstress profile exposed the regular-file-only restriction at reference commit
`b919f50eaf9cce79671f8501a580279d23cc5f3c`. The original Docker/SQLite result
remains FAIL; its six symlink-link EPERM outcomes are not reclassified.

Workspace LINK now binds either a regular-file or symlink inode to a new name.
Aliases share the serial, metadata, exact opaque symlink target and reference
count. Unlinking one name preserves the other; the last unlink removes the
committed inode. Directory hard links remain refused. Core initial construction,
resident/indexed update and qualification apply the same count rules. Bindings
under a dropped unreachable fresh directory are rejected before they can inflate
a retained inode's count; empty dropped headers remain harmless.

The object encoding and database schema do not change. Accepted symlink reference
counts do expand: older binaries may reject trees containing multiple bindings
to a symlink. Use matching CLI, worker, daemon and bundle source identities.
No mixed-version or downgrade support is claimed. Project Init and recursive
copy still preserve aliases only for regular files; this extension concerns
Workspace inode links and their Commit representation. Pathname-following
behavior remains the operating system/client's choice; inode LINK never resolves
the stored target.

## Scoped checks

- Core: 106 passing tests in inode_leaf, filesystem_hardlinks, filesystem_topology,
  filesystem_topology_backed and filesystem_qualify. Exact reachable counts,
  directory/root rules, opaque targets, final unlink and unreachable bindings.
- Workspace: 21 passing namespace and captured-namespace tests. The first new
  capture test failed because it released the reader without installing the
  completed capture before another capture; the corrected test installs through
  the existing lifecycle. The initial failed receipt remains local.
- NFS: six passing write tests, including source-filehandle symlink LINK through
  Commit and a fresh view, both alias handles, count transitions and directory
  refusal. This does not assume Darwin pathname-following semantics.
- Real Linux FUSE: the new symlink Commit/remount proof and the existing kernel
  mount/drain/refusal proof pass. Both ran in exact owned containers with
  /dev/fuse and SYS_ADMIN, under explicit bounded test commands; containers were
  inspected and removed after success.
- Selected Linux packages compile tests/examples with --all-targets --no-run;
  native Workspace/NFS tests/examples compile too. Warning-denying Clippy passes
  for selected Linux and native packages. Rust formatting, the production boundary
  guard and all 169 tooling self-tests pass.

No tests were deleted. Obsolete symlink-refusal assertions were replaced by
positive alias/Commit proofs while directory, malformed-count and reachability
refusals remain. Kernel refusal accounting removes the one newly supported
symlink LINK refusal: three ENOSYS replies plus two remaining mutation refusals
establish the lower bound of five.

Final matched release artifacts and CLI-only qualification are recorded by the
[fsstress benchmark task](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/57)
and its compact benchmark report. Timings remain INELIGIBLE diagnostics; this
source extension makes no performance or full-upstream-suite claim.
