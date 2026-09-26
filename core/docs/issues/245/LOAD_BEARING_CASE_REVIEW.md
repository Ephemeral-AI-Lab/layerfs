# #245 load-bearing case feasibility review

> **Status:** Research; informative and not a product contract.
>
> Case source: [`LOAD_BEARING_CASES.md`](LOAD_BEARING_CASES.md) at
> `d63379b955d1af4656cbfe23ebefc05ebf36ef8c`; its SHA-256 is unchanged
> in this worktree. Product source pin:
> `f74dbe77da12fa533587be8a578375bce3f19373` (after #252 and PR #255).
> Three independent read-only source reviews covered file, namespace and Exec
> paths. No case was run or benchmarked.

## Answer and qualification

Eight of the nine proposed *rows* have a source-plausible route or a named
#248/#256/#249 lift path, subject to resource and public-API proof. The ninth,
**Move and replace**, is ambiguous about whether the moved directory is
inherited from the Store. A fresh upper directory can move, but a base-resident
directory with no local record is explicitly refused as `Unsupported`. That
missing semantic path is [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258),
a separate #245 subissue. Nothing here is a functional or speed PASS for the
proposed fixtures. [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
sets the number of live Workspaces a daemon may create; every proposed row
uses one Workspace, so #219 changes none of their file/namespace behavior.

The case document's “current” 1,024-piece, 256-interval and 8 MiB *final
replacement* claims predate the path-copied tree and generic `SaveFile` route.
Those aggregate checks no longer describe the pinned product. The remaining
8 MiB guard is on one internal Workspace write, while the mounted FUSE route
negotiates at most 128 KiB per WRITE. The source still has a 4 GiB logical-file
bound, an 8 GiB `SaveFile` input bound, 128 dirty identities/names per
generation, a 30-second whole-Exec deadline, and resource budgets. See
[write admission](../../../crates/layerfs-workspace/src/filesystem/write.rs),
[FUSE WRITE](../../../crates/layerfs-fuse/src/adapter.rs),
[SaveFile request](../../../crates/layerfs-bridge/src/contract/request.rs),
and [frontier admission](../../../crates/layerfs-workspace/src/runtime/state.rs).

## Case-by-case assessment

“Current” means the pinned product source, not the older limits stated in the
case proposal. “Target” means after the named issues actually implement and
pass their own gates; it is not a prediction of benchmark admission.

| Proposed case | Current source assessment | Target owner and remaining proof |
| --- | --- | --- |
| **Many packages:** ~160 updated/new/removed packages | **Blocked at aggregate namespace admission.** More than 128 dirty identities/names meets Workspace and Bridge checks before quota is known to bind. | #256 must stream the full final namespace and scale directory/cache/Store validation. Freeze exact package count and run one SDK Exec + Commit with a complete new/old tree oracle. #249 matters only if the command exceeds today's Exec lifetime. |
| **One large package:** ~160 files + 192 MiB distribution | **Blocked by namespace count.** The 192 MiB file itself is below the logical-file and nominal 1 GiB private-backing bounds; actual disk, cgroup cache and time are unknown. | #256 handles names/files; #248 handles repeated file writes/Commit; #249 removes the whole-Exec timer. Verify every mode/path/hash, deep path, manifest replacement, old head, private disk and 512 MiB container memory high water. |
| **Move and replace:** cross-parent directory move plus temp-file replacement; whole-directory variant | **Fixture provenance unresolved.** Small file temp-over-base replacement and fresh-upper directory move have focused proof. A base-resident moved directory lacking a private record returns `Unsupported`. | [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258) must preserve inherited descendants under a stable canonical origin. Freeze base-resident and fresh-upper variants separately; verify moved grandchildren, old/new heads, held handles and atomic overlong-path refusal. #256 handles wide namespace count, not this missing lookup semantic. |
| **Remove and copy:** recursive `rm -r`, then `cp -R` | Bottom-up unlink/tombstone/empty `rmdir` and rebind paths exist. A large tree can meet the 128-change and 4,096-binding validation ceilings. No proposed full-tree run is recorded. | #256 must remove the aggregate ceilings and bound work. Verify every removed path, copied byte/mode, untouched canonical roots, delete-then-recreate, and old head. |
| **Tiny edit in 500 MiB existing file** | A small Local extent over canonical Base is structurally supported. Source inspection does not establish zero untouched-base reads or cold performance. | #248 must count actual canonical bytes read, private page visits, spool bytes, RSS/cache and writer-gate hold while proving exact old/new content. A base-size sweep distinguishes sparse work from hidden whole-base work. |
| **Large existing-file edit:** >8 MiB contiguous replacement; separate >256 disjoint edits | The old aggregate 8 MiB/256 refusals are gone on `SaveFile`. FUSE sends bounded callbacks; current repeated payload maintenance and per-run construction remain performance risks. No public-route scale proof exists. | #248 owns 4,097 separated-final-run gate, monotone cursors and write-time scaling. Freeze contiguous and disjoint variants as separate cases; record actual FUSE callbacks, final runs, old/new bytes and resource highs. |
| **Large fresh file and append:** 256 MiB copy; separate existing-file append | At least 2,048 max-sized FUSE WRITEs for the fresh copy. Fresh content and existing-base append use different Commit routes. Nominal backing quota can hold the payload, but high water, cumulative write cost and 30-second Exec outcome are unproven. | #248 removes work that grows with earlier writes and proves both Commit routes; #249 removes product whole-Exec expiry. Verify exact bytes/modes and whether append sends only final appended bytes. Measure backing, spool, cgroup cache and complete wall. |
| **Shell-driven prepend/insert:** 64 MiB temp-file copy then rename; separate future in-place shift | The shown `cat prefix file > file.next` necessarily reads and rewrites the full resulting file through FUSE; a local extent tree cannot erase those POSIX bytes. The fresh-file and same-parent replacement routes exist. | #248 helps bulk write cost; #249 removes the command timer. This case proves temp-file semantics, not efficient in-place insertion. Freeze the true in-place variant separately if pursued. Use `&&`/`set -e`, then check temp absence, held handle, modes, old/new heads and actual read/write bytes. |
| **Printed logs versus Workspace log file** | Printing 16 KiB to each stream uses no FUSE WRITE, returns the first 8 KiB of each with independent truncation flags, and needs no Commit. Appending a multi-MiB file uses FUSE and Commit. The two are distinct scenarios. | Printing's bounded-output route already exists; verify exact prefixes, flags, exit and unchanged head. For the file log, freeze `logs/` and initial file state, verify bytes, FUSE writes, empty Exec output and old head. #248 scales the file variant; #249 addresses long/quiet Exec. |

## Source-backed gaps and resource risks

1. A base-resident directory move is a definite current refusal in
   [`moved_identity`](../../../crates/layerfs-workspace/src/filesystem/rename.rs).
   Inherited children still use path-based Store inspection in
   [namespace resolution](../../../crates/layerfs-workspace/src/filesystem/namespace_view.rs).
   A move of a fresh upper directory does not prove the inherited case. The
   proposed fixture says the base has `packages/old/` but does not explicitly
   state whether `old/subtree` is canonical; the manifest must fix this.
   Deep directory moves also need to reject or correctly remap a resulting
   cached descendant path beyond 4,096 bytes before publication.
2. #256 must address the separate C1
   [4,096-binding cycle/reachability walk](../../../crates/layerfs-content/src/filesystem/limits.rs)
   for wide base-tree rebinds. Lifting Workspace's 128-name checks alone would
   move that refusal downstream. Recursive remove/copy and wide directory
   operations need public proof across that boundary.
3. The current sandbox has 1 GiB Workspace disk, 16 MiB Workspace memory and
   512 MiB container memory in its
   [launcher](../../../crates/layerfs-sandbox/src/docker.rs). A 192 or 256 MiB
   copied payload fits the nominal private-disk quota but also creates source
   and destination page-cache pressure. No current receipt proves the cgroup or
   backing high water for these cases. A 500 MiB canonical base with one 4 KiB
   edit has a very different private-payload cost; do not pool those claims.
4. Until #249, SDK Exec carries a 30-second deadline and the
   [daemon](../../../crates/layerfs-daemon/src/execution.rs) kills the process
   group on expiry. A quiet command can encounter the native transport's
   five-second progress rule. Each FUSE callback still has its own ten-second
   deadline. Removing the product whole-Exec timer does not waive the
   repository's normal 15-second complete-command benchmark budget (or a
   prospectively declared 25-second exception).
5. The current FUSE configuration has two worker threads and
   `max_background=1`. Overlapping Exec process lifetimes alone do not prove
   overlapping filesystem progress under #249; instrument accepted callback
   ordering. The proposed rows use one Exec each and do not require #249's
   multi-Exec capability, apart from its whole-Exec lifetime change.

## Freeze before sampling

The nine table rows contain more than nine possible measurement cells:
directory provenance/move variant, contiguous versus disjoint edits, fresh
versus append, temp-file prepend versus future in-place shift, and printed
versus file logs are distinct operations. Register each chosen case separately
with exact image and Store manifests, command bytes, old/new hashes and modes,
cache state, route/callback counts, quotas, timers, and independent verifier.
Use a short frozen script path if an inline many-package command would exceed
the present 4,096-byte Exec request. Make multi-command shells fail on their
first failed step (`set -e` or `&&`); a final successful `mv` or `cat` must not
hide an earlier copy failure. Prepare an independent writable fixture clone
outside each timed child; do not let its cache warmth credit the measured
phase. Preserve every failed/ineligible/unrun receipt.

Only a registered selection with a cache-qualified receipt, exact public
route, independent verifier and required complete-command wall can support a
performance claim. A long payload case that cannot meet the normal budget
remains a functional/resource probe or needs a prospectively specified
extended qualification; do not shrink it, relax the budget or repeat an arm to
select a faster sample. See the [benchmark rules](../../../../docs/general/benchmark_rules.md).
