# Fixed mixed development workloads

## Five commits per cycle (schedule M1)

K=10 performs two cycles; K=100 performs twenty. One branch/workspace owns its
ordered cycle; concurrent workers never mutate each other's mounts. All logical
roles, operation counts and content recipes are deterministic functions of
fixture, seed, branch tag and cycle, never elapsed time or requested depth.

| Stage | Exact requested operations before Commit | Persistent effect |
| --- | --- | --- |
| 1: refresh | Unlink 64 selected 4 KiB regular files, then create 64 files at the same paths and lengths: 32 exact recurrent, 16 local 256-byte variants, 16 unique cycle-tagged contents | Every stage changes at least the unique cohort; deleted bytes and old snapshots remain independently verifiable |
| 2: edits | Touch 16 targets with the recipes below | Exercise SDK and POSIX operations with separate declared counters, including length and representation changes |
| 3: directories | Move two populated 32-file directories across parents; remove eight scratch empty directories, then create eight alternate empty directories | Source/destination names alternate each cycle; all 64 descendants move without payload rewrite |
| 4: attrs/links | Create eight hardlinks and four symlinks; perform four 64-byte writes through hardlink aliases; chmod 16 regular-file inodes and four directories; set 16 file mtimes | Links survive Commit 4; mode/mtime changes alternate and apply to inode aliases |
| 5: inode lifetime/subtree | Delete/recreate the 32-file subtree including its root; four atomic 4 KiB saves over four linked destinations; unlink eight hardlinks and four symlinks | Test old inode lifetime, replacement identity and namespace recreation; finish at initial path/byte counts |

Perform one public full-status Commit immediately after each stage is finished.
Close writable descriptors and terminate the stage helper before Commit.
Every ordinary stage must return `Created`, with `presentation_failed=false`.
Attempted/Created/UpToDate/Busy/HeadMoved/error counts are distinct. Never insert
an unrelated dummy mutation solely to force Created: each stage already has a
specified semantic change. Generation-specific stage-5 contents prevent a
delete/recreate-identical no-op from collapsing history.

### Stage 2: sixteen main edit targets

| Count | Target class | Operation |
| --- | --- | --- |
| 8 | 4 KiB tiny | POSIX `pwrite`, 256-byte overwrite; positions rotate 0, 1920, 3840 |
| 1 | Medium | POSIX 256-byte overwrite at midpoint |
| 1 | Medium | POSIX append 4,096 bytes then truncate back; deliberate net-zero probe |
| 1 | Medium | POSIX truncate 4,096 bytes then append different 4,096-byte tail |
| 2 | Initially 1 MiB | Two public single-file SDK calls: one +256-byte insertion and one −256-byte deletion; inverse lengths next cycle; middle offsets, replacement bytes from independent generator |
| 1 | Large anchor | POSIX 4,096-byte overwrite; offset 0, aligned midpoint, then len−4096 in rotation |
| 2 | Boundary below/above | POSIX shrink before growth: (131071,131073) → (131072,131072) on odd cycles, reverse lengths on even cycles |

The SDK pair uses two `Client::edit_workspace_file_range` calls, shrink first,
after the 14-target POSIX helper has exited and before Commit. The existing
batch API is same-file only; this pair is not one failure-atomic transaction.
If either call fails, the stage fails and its partial dirty state is retained
for diagnosis/cleanup, never committed as a successful row. No shell/POSIX
reconstruction of these SDK range edits.
Both APIs remain explicitly named; the encompassing mixed stage is not an
SDK-only file-edit performance claim. The exact-131072 third boundary file is
an untouched control. The two length-changing pairs have zero aggregate
committed growth. Sixteen targets are touched, not sixteen net content changes.

The no-op append/truncate probe is counted even though its final content is
unchanged. Time its real writes and truncate; do not optimize it out. POSIX
operation-caused reads and write buffers belong to the workload, whereas
added oracle hashing belongs only in verify mode.

### Stages 4/5: inode and link semantics

Alias targets are eight disjoint 4 KiB files, included among the sixteen chmod
and mtime targets. Link before the attribute changes. Four 64-byte alias writes
are extra probe writes beyond the sixteen main edit targets. Verify propagation
to the canonical names. File modes toggle 0640/0600, directory modes 0750/0700;
explicit timestamps follow the fixture recipe.

Four relative symlinks use targets of at most 64 UTF-8 bytes: two resolvable
links, one dangling link, one self-loop. Expected live readlink/open behavior is
part of the selected workload. Missing target/ELOOP are expected outcomes, not
successful data reads. Remove them in stage 5, after their retained-state proof.

Before deleting the 32-file subtree, open one member; remove all names/root,
read and write a 64-byte range through the still-open descriptor, then close it
before recreating the subtree. Recreate 16 files with recurrent A/B contents
and 16 with generation-specific contents. Old snapshot bytes must remain intact.

For each of four hardlinked destinations, hold its descriptor, create/write/
fsync one 4 KiB temporary file, rename it over the destination, and exercise
the old descriptor and old alias. The new destination has a different inode
relationship; the old descriptor/alias still refers to the old inode. Close
before the next save. Then remove all eight aliases and four symlinks. Numeric
inode values need not be stable across remount/reopen: compare equivalence
classes, inequality and link counts within the appropriate view.

One failed `O_CREAT|O_EXCL` attempt on an existing refresh path per cycle is a
bounded negative POSIX probe; expect EEXIST and unchanged state. It adds zero
successful creations. Existing xattr/lease negative proofs are reused outside
M1; no successful xattr/chown/ACL persistence is implied.

### Exact per-branch operation totals

| Receipt | Per cycle | K10 | K100 |
| --- | ---: | ---: | ---: |
| Created commits | 5 | 10 | 100 |
| Explicit regular unlinks | 96 | 192 | 1,920 |
| Ordinary regular creates | 96 | 192 | 1,920 |
| Temporary regular creates | 4 | 8 | 80 |
| Rename-overwrite saves | 4 | 8 | 80 |
| Main edit target operations | 16 | 32 | 320 |
| SDK range-edit calls / members | 2 / 2 | 4 / 4 | 40 / 40 |
| Hardlink creates / unlinks | 8 / 8 | 16 / 16 | 160 / 160 |
| Symlink creates / unlinks | 4 / 4 | 8 / 8 | 80 / 80 |
| Extra alias writes | 4 | 8 | 80 |
| Open-unlinked descriptor writes | 1 | 2 | 20 |
| Populated-directory moves | 2 | 4 | 40 |
| Directory creates / removes (including subtree root) | 9 / 9 | 18 / 18 | 180 / 180 |
| File chmod / directory chmod | 16 / 4 | 32 / 8 | 320 / 80 |
| Explicit file mtime calls | 16 | 32 | 320 |
| Expected EEXIST probes | 1 | 2 | 20 |
| POSIX stage helper executions | 5 | 10 | 100 |

Main-target operations are not a syscall total: append/truncate, temporary
saves and descriptor probes each contain multiple syscalls. Freeze and check
the exact helper syscall receipt against these recipes; never label sixteen
targets as sixteen total syscalls. Count fsync, open/close, readlink, reads,
ftruncate and probe calls separately. Rename replacement is not an explicit
unlink syscall and cannot be double-counted as one.

### Live high-water bounds

- Stage 1 deletes before creation; stage 3 removes before mkdir.
- Stage 2 adds at most 4,096 transient bytes and no names; length pairs shrink
  before growth, finish aggregate-neutral.
- Stage 4 adds twelve names and at most 8×4096 + 4×64 = 33,024 path bytes.
- Stage 5 one-at-a-time temporary save adds at most thirteen names and 37,120
  bytes over initial. Open-unlinked allocations are measured separately.
- Stage 5 finishes with the initial count/aggregate length. Eight scratch
  directories alternate names; move parents are pre-existing. Directory count
  never exceeds the initial 62/312.

Thus the sixteen-name/65,536-byte reserve holds at every step, not only after
Commit. Per-worker byte caps are not multiplied into a physical Store promise.

## Topology schedules

### Sequential: mixed_load_bearing

One initialized Store, one branch, one live Workspace, K Created commits. One
session persists through all K stages; helper processes do not persist across
Commit. Initial state + K unique newly created commits = K+1 retained roots.

### Concurrent: multi_workspace_development

Fork two branches from the same initial Layer (no created trunk commits).
Two host workers share one Store/client and one container, distinct FUSE roots.
Common start barrier; both independently execute K M1 stages. Total Created
commits 2K; each branch depth K; total retained roots 1+2K. Half the replacement
cohorts are common across branches, half use branch-specific bytes.

At local commit 5 on both workers, exercise one fixed discard/reopen event:
B completes the mutation of stage 6 and holds dirty state; A performs one
extra 256-byte POSIX mutation on the witness, exits its helper and discards its
session without Commit. A reopens the same branch, then both continue. A's
discarded mutation is absent and B's dirty state survives. This is an extra
mutation/helper/session lifecycle, not an extra Created commit. Locate the
event at 5 for both K10 and K100, preserving the common prefix.

Successful Commit requests should overlap in at least one separately recorded
round. Use an additional pre-Commit barrier for stage 1 (and no timed sleeps)
so both requests are released together; record actual public-call intervals.
If actual overlap is absent, report concurrency coverage failure rather than
claim internal parallelism. SQLite publication may legitimately serialize.
The measurement lock is per selected test; do not serialize its workers with it.

### Historical fork: branch_development

Trunk executes ten M1 commits. Fork A and B from trunk commit 5 using
`Client::fork_branch(LocalForkSource::Branch { ... })`. Each child executes K
stages with local cycle numbering starting at 2, the correct successor to the
inherited stage-5 state. Inherited content, names and modes are the fork-state
oracle, not a pristine reset. Continue for K stages, i.e. two/twenty whole cycles.

Total Created commits = 10+2K (30/210); total graph roots = 11+2K (31/211).
Each child's ancestry depth = 5+K (15/105), while its local Created count = K.
Trunk remains at commit 10. Children run concurrently without a discard event.
Use the first common stage's pre-Commit barrier as above. Content groups 0/1
are branch-common, groups 2/3 branch-divergent; metadata tags distinguish
branches where specified. Commit IDs may differ even when content is reused.

### Explicit four-workspace extension

One L100 Store; four branches from initial Layer; four live workers in the same
2 CPU/2 GiB container, 100 local M1 commits each (400 total, 401 graph roots).
Same shared/private content rule (two pairs of common cohorts); no discard
probe. Fixed work with a 60-second outer watchdog and three seconds reserved
for cleanup. No target duration, no sleep, no automatic default execution.
