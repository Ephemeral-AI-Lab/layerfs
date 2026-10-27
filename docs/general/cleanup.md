# Development workspace cleanup

> **Status:** Current general guide.

This guide covers development worktrees, build caches, agent logs, temporary test
files, benchmark payloads, and Docker resources. It applies when the user has
authorized cleanup. Authorization and the selected scope carry through the pass;
routine removal of confirmed disposable resources does not need repeated approval.

Follow [repository instructions](../../AGENTS.md), the owning campaign's evidence
requirements, and the [measurement workflow](agent-measurement-policy.md).
Preserve unrelated work and current execution ownership. Use filesystem and
runtime metadata to inspect resources; cleanup does not require starting product
tests, benchmarks, or Store sessions.

## 1. Define the pass and protect ongoing work

Create a fresh pass directory outside the tracked source tree. Record the user's
scope, requested free-space target and unit, start time, and protected resources.
Keep machine-specific paths, task identifiers, inventories, and results in that
directory. This guide describes the procedure and remains independent of a
particular cleanup pass.

Resolve active task or thread identifiers to actual working directories. An app's
worktree field may be empty even after an agent creates an isolated checkout;
cross-check task instructions, events, Git registrations, and the filesystem.
Read application metadata without editing its state.

Protect the active worktree and its dependencies, including:

- reference checkouts and untracked helper scripts;
- original input trees, inventories, prepared fixture masters, and sealed Stores;
- binaries and images selected for build or proof reuse;
- containers, volumes, mounts, open files, and measurement locks owned by current
  work.

Search current task instructions and harness defaults for dependency paths before
classifying an older directory as disposable. Keep resources whose ownership or
dependency role remains uncertain and continue with independently safe candidates.
Avoid cleanup I/O on measured inputs during an active measurement. Record known
interference and follow the owning campaign's isolation requirements.

## 2. Inventory space and ownership

Run read-only inspection first. From the repository root, useful commands are:

```sh
df -k /System/Volumes/Data
git worktree list --porcelain
git status --porcelain=v1
docker system df -v
docker ps -a
```

The filesystem command above is for macOS; select the actual data filesystem on
other hosts. Use focused `du` scans, per-worktree Git status, process and open-file
inspection, and container metadata as needed. Record running commands, bind
mounts, volume consumers, and source identities alongside allocation estimates.

Record free bytes and derive both units:

- GB = bytes / 1,000,000,000;
- GiB = bytes / 1,073,741,824.

Treat directory allocation as a candidate estimate. Shared files, APFS clones,
hard links, Docker virtual disks, and delayed reclamation can prevent directory
totals from matching recovered physical space. Report the observed change in
filesystem free bytes. Record concurrent builds or other writes that affect that
net change; do not attribute an unisolated change entirely to one removal.

## 3. Classify each candidate

| Resource | Required check | Permitted cleanup action |
| --- | --- | --- |
| Cargo dependency, incremental, build, and fingerprint caches | Outside protected targets; no current build or measurement uses them | Remove generated caches together as appropriate; keep required executables, scripts, locks, manifests, and records |
| Registered Git worktree | Local changes, unique commits, ignored helpers, active processes, and container binds | Preserve recovery refs and verified source/record backups before removal |
| Workspace copy without Git metadata | Ownership and complete contents | Archive complete source and useful records before removing generated payloads |
| Benchmark receipts, logs, and verdicts | Owning campaign and evidence requirements | Retain in place or in a verified archive when archival is within the authorized scope |
| Historical sample databases, exports, and payloads | Closed run, no current consumer, records preserved, disposal authorized | Remove selected payloads and record that they are unavailable for future proof reuse |
| Prepared fixture masters, inventories, sealed Stores, binaries, and images | Current task, harness defaults, source identities, and reuse records | Keep resources selected for ongoing work or qualifying reuse |
| Docker sample containers and volumes | Exact mapping to closed run records, current processes, consumers, and ownership | Stop and remove the identified resources after ownership is established |
| Inactive agent transcripts and text records | No writer; no pinned file identity or current allocation requirement | Prefer verified transparent compression where supported |
| Live application databases, WAL files, active logs, and session state | Owning application and current use | Preserve |
| Download, installer, browser, and package caches | Current consumers, installation activity, and supported pruning behavior | Remove unused staging files or prune through the appropriate installed tool |
| Personal files and application data | Explicit user-selected scope | Preserve during development cleanup unless the user selects them |

Age, a passing historical receipt, a clean Git status, or an ignored-file rule
alone does not establish that a resource is disposable. A running container may
be an abandoned sample daemon, but an idle process can also own useful mutable
state. Combine task ownership, closed-run records, volume mappings, and current
process/consumer inspection before deciding.

## 4. Preserve recovery state before removing a worktree

Record the original HEAD, branch or detached state, and Git status. Create a
valid recovery ref in the owning repository so an unmerged or detached commit
remains reachable. Preserve staged and unstaged binary patches, untracked source,
and useful ignored files. Inspect build directories for helper scripts, case
lists, journals, and measurement wrappers before treating their contents as cache.

For a copy without usable Git metadata, preserve complete source instead of
assuming a patch and an existing commit can recover it. For an independent
repository, ensure its required Git objects remain recoverable before removing
its object database.

Archive retained benchmark records and record the selected payload exclusions.
Verify archive contents against a manifest, including hashes for regular files
and targets for symlinks. Check that recovery refs resolve to the recorded commits.
An archive's existence or successful compression alone is insufficient.

Recheck local changes and current ownership immediately before removal. If work
changed during archival, preserve the new state before proceeding. Use Git's
worktree removal and pruning facilities for registered worktrees; prune stale
registrations only after confirming their working directories are gone.

## 5. Remove identified runtime and temporary resources

Use exact container, image, volume, and path identifiers from the inventory.
Reinspect current processes and consumers before acting. Protect newly created
resources even if they resemble an older selected run. Do not stop another
owner's current execution or unlink an active measurement lock.

For retired private mutable state selected for recovery, stop its owning daemon
before copying the complete volume. Verify the stopped-state backup before
removing the container and volume. Record the shutdown outcome and any uncertainty;
a partial copy of a live SQLite database does not establish a recoverable backup.

Inspect temporary test directories for ownership, age, open files, and current
task dependencies. Preserve useful records before removing selected test payloads.
Keep prepared masters and original inputs that current harnesses reference even
when they live under a temporary directory.

Prune caches using the appropriate installed tool and version where possible.
Inspect a failed launcher before choosing a corrected invocation; do not repair
unrelated application installations merely to finish a cleanup pass.

## 6. Preserve evidence meaning and accessible transcripts

Removing a historical payload does not change its original verdict. Record the
disposal in a separate cleanup ledger, with its original location and identity
where available. Mark deleted payloads and executable copies as unavailable for
new proof reuse. Preserve failed, ineligible, and unrun outcomes. Do not rewrite
or promote an original receipt after cleanup.

Transparent filesystem compression can reduce allocation while keeping text
records accessible at their existing paths. Confirm platform support first. For
each selected inactive file:

1. Capture its logical size, hash, allocation, and relevant metadata.
2. Write a compressed temporary copy and verify its logical bytes and hash.
3. Check that the original did not change during copying.
4. Replace it atomically only when the copy is verified and saves allocation.
5. Record the before/after allocation and preserved hash.

Preserve file location, ownership, mode, and modification time as required.
Inode, change time, and allocation can change during replacement; exclude files
whose identities or current measurement contracts require those properties.
Keep live databases, WAL files, active logs, measurement inputs, and fixture
masters out of this process.

## 7. Diagnose failures from the remaining state

Record the command, exit status, output, and remaining files or registrations.
Inspect what changed before selecting a corrective action. Do not automatically
retry a failed operation or assume it had no effect.

Git can partially remove files and delete a worktree registration before reporting
a filesystem failure. Determine whether the registration still exists before
choosing the next action. Any direct removal of leftover files requires verified
backups, the recorded recovery ref, and a fresh ownership check.

Read-only fixture directories can prevent deletion of their children. Adjust
owner permissions only within explicitly selected, backed-up disposable paths
after diagnosing the failure. Preserve inaccessible, immutable, or uncertain
remnants when safe removal is not established; record their paths and remaining
allocation where measurable.

## 8. Verify completion and write the report

Append progress and failures to the pass ledger. Write a fresh final report with:

- start and finish time, authorization, scope, target, and units;
- protected tasks, actual worktree paths, and required dependencies;
- free bytes before and after, GB/GiB values, and concurrent disk activity;
- removed paths and exact runtime identifiers, with disposal reasons;
- verified archives, manifests, recovery refs, and stopped-state backups;
- compression hashes and allocation changes;
- failures, partial removals, skipped resources, and remaining large resources;
- final Git state and checks that protected helpers and inputs remain present;
- explicit availability limits for deleted benchmark payloads and builds.

Reinspect the protected task's state and resources. Compare unrelated checkouts'
Git state with the initial inventory; distinguish their owners' concurrent changes
from cleanup changes. Measure final free space after removals have completed and
record the observation time, since ongoing builds can continue to consume space.

Stop after the authorized, confirmed disposable resources are cleared. Report
the achieved space and the remaining protected or uncertain resources. A disk
target guides the search; it does not expand authorization to remove needed
inputs, unrelated state, or personal data.

Follow the [documentation policy](documentation-policy.md) when maintaining this
guide. A documentation-only commit still needs the exact unchanged production
LOC comparison required by [repository instructions](../../AGENTS.md).

## 9. Parallel worktree build and E2E retention

The [parallel worktree launcher](parallel-worktrees.md) registers resource owners,
publishes pinned build bundles, records per-run outcomes, and exports exact cache
release scopes for this cleanup procedure.

Each worktree owns one writable Cargo target per platform. Keep that target warm
while its assignment is active. A Linux target volume belongs to one worktree;
several worktrees never mount one target volume writable. Seed a new target from
a completed, quiescent cache, copying only the selected build profile and its
required metadata. Do not copy an entire historical `target/` tree containing
old experiments, source archives, receipts, or unrelated build profiles.

Named Cargo volumes survive removal of their worktree or build container.
Include each completed assignment's exact target and dependency-volume names in
its release scope, then inspect all current container consumers before removal.
Inspect target volumes for direct binaries, helper scripts and records outside
generated compiler directories; preserve selected extras in verified archives.
Do not keep tens of gigabytes of disposable compiler output merely to retain a
small helper. Active assignments retain their warm volumes even between builds.
A Docker connection failure leaves the operation's outcome uncertain: inspect
whether the selected identity still exists, record the result, and keep failed
selections out of a follow-up pass of previously unattempted resources.

Every build or E2E invocation records its worktree, run id, source and binary
identities, exact paths, container ids, and volume names. Reusable caches belong
to the worktree; writable test Stores, SDK homes, mountpoints, and outputs belong
to the run. Share immutable images and read-only dependency inputs. Concurrent
daemons within a test retain the shared Store required by that test; independent
test runs use independent writable copies. Cleanup never opens a Store or changes
its persistence profile.

| Resource | Retention and release |
| --- | --- |
| Active worktree Cargo target and Linux target volume | Keep one per platform. Inspect growth after build/test batches; a target over 20 GiB triggers a breakdown, not automatic deletion or a compilation refusal. |
| Completed assignment's generated compiler cache | Remove after checking consumers and preserving selected executables, helpers, and records. Keep the source checkout and recovery state. |
| Inactive cache without a current owner | Review after seven days; remove only after dependency and ownership checks. Age alone never authorizes removal. |
| Seed cache | Keep one selected, closed seed per required toolchain/platform/profile/flags combination. Replace obsolete seeds after active consumers have independent copies. |
| Successful E2E runtime resources | After teardown and receipt capture, remove the exact containers and disposable writable volumes. Keep any explicitly selected prepared fixture. |
| Failed or interrupted E2E resources | Retain diagnostics and the state needed for diagnosis. Record an owner and release condition; remove disposable state after the diagnosis releases it. |
| Comparison binaries, snapshots, and frozen failure evidence | List explicit pins and release conditions in the campaign records. A cache cleanup never implicitly releases these pins. |
| Temporary source trees used for LOC comparisons | Keep at most one before/after pair per active comparison. Record the commit/tree identities, counter identity, command, and result; remove the generated trees when that comparison finishes. Never accumulate another full-repository pair for each commit. |
| Completed worktrees | Review at handoff. Before removing a checkout, preserve its reachable commits, local edits, useful ignored helpers, and campaign records through the verified recovery procedure above; check current processes, bind mounts, and other tasks' path dependencies. A merged HEAD alone does not release the checkout. |
| Agent transcripts, experiment logs, and product traces | Keep active writers and pinned proof inputs in place. At run closure, index the source/run identity, outcome, retained paths, and release condition. Compress closed text records with byte verification where readers support it; remove redundant expanded copies only after verified archival. Retain original failures and verdicts. |
| Cargo/pnpm downloads and immutable runtime images | Reuse them across worktrees where safe. Remove only obsolete, unreferenced copies after checking current build and proof requirements. |

Cleanup runs at the owning command's completion and assignment handoff; it is
not a global build/test scheduling gate. Agents may develop concurrently in
their own resources. No agent runs a global Docker prune, deletes another
worktree's cache, clears shared dependency inputs, or uses process-name-wide
termination to reclaim space. Labels help identify ownership; exact acknowledged
ids and the run ledger decide what is removed. A still-running resource is not
disposable merely because its original shell exited.

Keep host and Docker guest headroom separate. How often to check free space, the
low-space trigger and the recovery target, for the host and inside Docker, depend
on the machine; set them in your own agent instructions, not in this repository.
Thresholds are not product limits, test verdicts, or permission to delete
protected state. Before a large new fixture, cache clone, or image build, account
for its additional allocation. If confirmed disposable data cannot meet the
recovery target you set, report the actual recovery, shortfall and remaining
protected/uncertain allocation.

After removal, record actual free-byte changes in both filesystems where
applicable. APFS clone totals and Docker object totals are not additive physical
recovery claims. Keep a small append-only disposal ledger outside source; link
it from the handoff, and leave historical verdicts unchanged.

## 10. Reusable cleanup script

Run [tools/cleanup.py](../../tools/cleanup.py) with Python 3.9 or newer, Git,
`lsof`, and an available Docker CLI/daemon. It uses the Python standard library.
No product test, build or Store session is needed. The script inventories first;
it never infers release permission from an old timestamp, clean Git status, or
unreferenced volume alone.

Check the host filesystem holding the repository:

```sh
python3 tools/cleanup.py check
```

Exit 0 means free space is at or above `--min-free-gb` (decimal GB, default 100);
exit 2 means it is below. This is a cheap check; it does not scan every worktree
or delete anything. Record its initial free-byte value when tracking recovery across more
than one pass. Do not add overlapping directory or Docker allocation estimates.

Create a fresh inventory pass, even before selecting any removals:

```sh
python3 tools/cleanup.py plan --out /absolute/path/outside-repository/new-pass
```

When the user authorizes cleanup of completed worktrees and containers, discover
exact candidates directly. A separately prepared owner-release file is optional:

```sh
python3 tools/cleanup.py plan --discover --authorization "user authorized cleanup of completed worktrees and containers" --protect-path /absolute/path/to/active-worktree --protect-container active-daemon --out /absolute/path/new-pass
python3 tools/cleanup.py apply --plan /absolute/path/new-pass/plan.json
```

Discovery selects idle linked worktrees of this repository and stopped LayerFS
containers identified by their Store mount, repository bind or launcher label.
It preserves the invoking checkout, locked worktrees, protected resources,
running containers and worktrees with open consumers. For explicitly authorized
retirement of completed agent checkouts, add `--retire-idle-sessions`: idle
Claude sessions, their wrappers and childless terminal shells may retain only a
cwd handle. The script rechecks their status before removal and still protects
open files and active commands. Docker Desktop's cached host handles are ignored
only in this retirement mode; every current container bind is checked separately.
Idle agents are left running, and their removed checkout paths require restoration
or reattachment before reuse. Record active assignments
and their dependency paths with `--protect-path`; a process check alone cannot
identify future reuse. Inspect the saved exact plan before applying it, and apply
it only with the user's authorization for that cleanup. Whether an authorization
carries over to later passes belongs in the user's own agent instructions.

For a cache pass, use `--discover-caches --authorization "..."` instead. It finds
real Cargo targets below each linked checkout's target roots, including nested
Linux target directories. It selects generated profile children, preserving direct
executables, helpers, dependency downloads and records. Run this pass before
worktree archival so disposable compiler output does not fill recovery storage.
To limit cache discovery to specific registered checkouts, repeat
`--cache-worktree /absolute/path/to/checkout`. This also permits explicit cache
inspection of the primary checkout, which broad discovery skips. Protect each
active assignment's warm target using `--protect-path`; an idle process alone
does not establish that its build cache is released. Docker Desktop host bind
aliases are normalized before every ownership comparison.

An exact released `cargo-cache` action may set `retire_idle_sessions: true`
for a completed assignment whose profile is held by an idle session cwd or
cached Docker Desktop host descriptors. Planning and final apply checks use the
same retirement filter. Current container binds and ordinary open consumers
still protect the profile; retirement remains an explicit selection. A macOS VM
process is recognized as Docker Desktop only when its open handles include
Docker Desktop's `Docker.raw` backing file; other virtual machines remain owners.

Create a fresh worktree plan after the cache pass because directory identities
have changed. A profile is one checked job: Cargo hard-links files between
`incremental` and `deps`, so all sibling fingerprints are validated before any
sibling is removed. This avoids treating cleanup's own unlink effects as writes
by another owner. Full `du` scans are optional with `--sizes`; ordinary planning
uses candidate fingerprints and avoids repeatedly scanning all build trees.

For narrower selections or explicitly retired running daemons, copy
[the empty scope template](../../tools/cleanup-scope.example.json)
to a file outside the repository. List protected paths and volumes, optional
`scan_roots` for large-log/experiment discovery, and exact released actions.
The template selects nothing. Each action requires `owner`, `reason` and the
literal boolean `released: true`; this records an established owner release,
or the user authorization recorded by discovery. Example selections:

```json
{
  "version": 1,
  "protected_paths": ["/absolute/path/to/pinned-daemon"],
  "protected_volumes": ["exact-active-cargo-volume"],
  "scan_roots": ["/absolute/path/to/experiment-records"],
  "actions": [
    {
      "kind": "cargo-cache",
      "path": "/absolute/path/to/completed-worktree/target/debug",
      "target_root": "/absolute/path/to/completed-worktree/target",
      "owner": "completed assignment owner",
      "reason": "assignment handed off; cache released",
      "released": true
    },
    {
      "kind": "archive-tree",
      "path": "/absolute/path/to/finished-loc-comparison/before",
      "owner": "comparison owner",
      "reason": "comparison recorded; expanded source no longer needed",
      "released": true
    },
    {
      "kind": "archive-file",
      "path": "/absolute/path/to/closed-run/daemon.trace",
      "owner": "run owner",
      "reason": "closed record released for verified archival",
      "released": true
    },
    {
      "kind": "worktree",
      "path": "/absolute/path/to/completed-linked-worktree",
      "owner": "assignment owner",
      "reason": "handoff complete; source and helpers released for archival",
      "released": true
    },
    {
      "kind": "docker-volume",
      "name": "exact-closed-disposable-volume",
      "owner": "run owner",
      "reason": "closed run; receipts retained; payload released",
      "released": true
    }
  ]
}
```

Make a new plan from that scope and then apply the reviewed plan:

```sh
python3 tools/cleanup.py plan --scope /absolute/path/to/scope.json --out /absolute/path/to/new-pass --reclaim-gb 300
python3 tools/cleanup.py apply --plan /absolute/path/to/new-pass/plan.json
```

Read `plan.json` and `inventory.json` before apply. Change selections in the scope
and generate a new pass instead of editing a saved plan. During ordinary cleanup
without a low-space episode, omit `--reclaim-gb` if there is no recovery target.
If more confirmed disposable resources are needed, use fresh, nonoverlapping
selections in a new pass and compare final free bytes with the episode's initial
check. Do not automatically retry an operation that failed or partially removed
something.

An exact `discard-tree` action may release reproducible generated payloads such
as inactive installer staging without making a redundant archive. It requires
`disposable_generated: true` in addition to the usual owner/reason/release
fields. The full tree fingerprint and all activity/protection guards still
apply. Select source, helper and evidence recovery through `archive-tree` instead;
this explicit payload-discard action is never selected by broad discovery.

A released named Cargo target can use `docker-cargo-cache` with its exact volume
`name`. Unlike a `docker-volume` action, this retains the volume and direct
binaries/helpers while clearing only generated debug/release children. It needs
the installed `node:24.21.0-bookworm` helper image; the plan pins its immutable ID
and never pulls an image. Every current container consumer protects the volume.
The helper validates all child fingerprints before any unlink and verifies direct
file hashes afterward. Failed helpers are stopped and retained for inspection;
they are not replayed. A successful helper is removed without deleting volumes.
For a nested Cargo target, set `target_subdir` to its exact single directory
name, for example `"plain"`. Docker mounts only that volume subdirectory at the
helper's target root; its own `.rustc_info.json` and debug/release profiles are
required. Parent caches and sibling directories stay outside the helper mount.
Absolute paths, traversal, multi-level paths and mount-option delimiters are
rejected before container creation. The entire volume must still be idle and
unprotected. Docker must support the `volume-subpath` mount option; an unsupported
mount is refused without falling back to the volume root.

The script enforces these boundaries:

- Cargo cleanup selects only `incremental`, `deps`, `build`, `.fingerprint` and
  `examples` under a direct `debug` or `release` profile with Cargo target metadata.
  It preserves direct executables and helpers. Nested target roots require their
  own Cargo metadata; arbitrary custom profile names are not guessed.
- Archive payloads and generated caches are fingerprinted at plan time. A
  worktree plan pins its root device/inode, HEAD and staged state; current working
  files are captured and verified when applying, so legitimate ignored-file or
  helper changes before capture are included in the recovery archive. Apply rechecks open files/cwds, protected paths and Docker binds.
  Exited/zombie inspectors in an earlier `lsof` snapshot are not current owners.
  Incomplete process/container inspection blocks deletion. Every container,
  including a stopped one, retains its mounts.
- A volume must be explicitly released, unprotected, have the same recorded
  identity, and have no container consumer. Removal uses its exact name. The
  script does not remove images, prune globally or run FITRIM.
- Container actions save inspect/diff records, verified compressed logs and a
  tagged recovery image before removal. Bind mounts and named volumes remain
  intact. Discovery never stops running containers. An exact `docker-container`
  scope action may specify `retire_running: true` for an owner-released abandoned
  daemon; both plan and apply require it to contain only one `layerfs-daemon`
  process, with no Exec descendants. Protected containers and binds are refused.
  The daemon is stopped before recovery capture. A changed identity or newly
  active command blocks removal. Recovery images retain changed filesystem layers;
  removing a container alone may reclaim little space. For aggressive retirement,
  an exact action may set `discard_rootfs: true` and list `preserve_paths`, such as
  `/layerfs-local/config` and `/usr/local/bin/layerfs-daemon`. Those files are
  copied from the stopped container and verified in deduplicated recovery objects;
  other ephemeral rootfs data is explicitly released. Logs, inspect/diff records,
  original images and every mounted Store remain available. This avoids archiving
  disposable npm/temp caches as whole recovery images.
- Source trees and closed files are stored as raw deduplicated blobs/trees in
  the repository's Git object database, using a private index. Every archived raw
  file is rehashed and its recovery tree is verified before deletion. Hashing
  batches are bounded by file count and total bytes; a large file is processed
  alone. Fixed command limits still apply, and a timeout preserves the source. Worktree
  format v2 reuses the pinned HEAD for unchanged tracked source; it stores raw
  working edits, untracked files, ignored helpers and all nested Git metadata.
  Compressed manifests retain original path/mode/time/ownership and empty-directory
  records, marking unchanged tracked files with `from_head`. The full worktree
  metadata fingerprint is still rechecked before removal.
- Linked worktree archival also pins HEAD and preserves staged/unstaged binary
  patches, including a staged version different from the working file. Primary,
  locked and unmerged-index checkouts are refused. Nested linked worktrees below
  the invoking checkout can be removed; the invoking checkout and its ancestors
  cannot. Nested repository metadata inside a retired worktree is byte-preserved
  under encoded recovery paths, with original-to-archive mappings in the manifest.
  Removal uses `git worktree remove`; it never prunes other registrations.
- Extended attributes, where the Python runtime supports them, are recorded as
  base64 values in each manifest row. Restore them with `os.setxattr` after raw
  file restoration. This is a source/text recovery format: ACLs are not captured,
  and hard-link identity is only reflected in recorded inode metadata. Preserve
  artifacts requiring those properties or use a verified filesystem backup.
  Every action needs recorded release authority and fresh activity checks;
  process checks cannot prevent a new writer racing after inspection.

Keep `actions.jsonl`, `report.json`, recovery manifests/patches, and the Git
`refs/cleanup/...` references and objects together. The ledger records archival
before removal. Raw archived bytes can be exported with `git archive --format=tar
<recovery_ref>`; manifest metadata and empty directories need separate restoration.
For a v2 worktree recovery, first check out its retained HEAD, apply the staged
patch to the index and working tree, then apply the unstaged patch to the working
tree. Overlay the raw recovery tree afterward. Restore nested metadata files from
`archive_path` to their original row `path`, then restore manifest metadata and
extended attributes. A v1 worktree archive contains the entire raw tree. Retain
both the HEAD reference and patches in either format. Test restoration into a
fresh destination before discarding recovery material.

Apply is single-attempt per pass: an exclusive `apply.started` marker rejects
reruns. The default is sequential. `apply --jobs 4` runs up to four independent
archive, worktree or container actions per batch; paths are checked for overlap first.
The first refused/failed batch stops further batches while already-running jobs
settle and retain their outcomes. Nothing is automatically retried. Its report
reconciles completed outcomes from the journal after settling active jobs,
including on Ctrl-C. Its report distinguishes untouched selections from uncertain
started actions; only untouched selections belong in a fresh follow-up scope.
It records pins, actual net free-byte
change, and whether the recovery target was met. Exit 1 means a refusal/failure;
exit 2 means removal completed but the requested space recovery was not reached.
Concurrent writes and APFS/Docker block sharing can affect the observed delta.
