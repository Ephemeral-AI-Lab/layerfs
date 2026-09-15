# v0.1.6 fixture and environment configuration

## Units and budget scope

MB means 1,000,000 bytes; KiB/MiB mean 1,024/1,048,576 bytes. The repository
limit applies to each live or retained namespace view, not the sum of all
branches/history and not a physical Store/RSS limit.

Count every non-directory name toward the path cap: regular files, hardlink
aliases, symlinks and temporary save files. Charge a hardlink name its referent
length in `logical_path_bytes`, and a symlink its UTF-8 target length. Also
report distinct regular inodes and distinct-inode logical bytes separately.
Open-unlinked file allocations are separate spool/inode resource measurements;
they cannot be hidden just because the pathname has disappeared.

## Load-bearing fixtures L100 and L500

The caps are 5,000/30,000 paths. Initial regular-path counts are deliberately
16 lower, reserving names for real atomic saves and links. No initial aliases
or symlinks. All workload files and witnesses are already inside these counts.

| Class | L100 | L500 |
| --- | --- | --- |
| Empty files | 34 × 0 B | 284 × 0 B |
| Tiny files | 3,950 × 4,096 B | 23,700 × 4,096 B |
| Medium, lower length | 706 × 28,146 B | 3,736 × 22,566 B |
| Medium, higher length | 244 × 28,147 B | 2,064 × 22,567 B |
| Boundary below | 1 × 131,071 B | 1 × 131,071 B |
| Boundary exact | 1 × 131,072 B | 1 × 131,072 B |
| Boundary above | 1 × 131,073 B | 1 × 131,073 B |
| Moderate large | 46 × 1,048,576 B | 195 × 1,048,576 B |
| Anchors | 1 × 8,388,608 B | 2 × 33,554,432 B |
| **Initial regular paths / inodes** | **4,984** | **29,984** |
| **Initial logical bytes** | **99,934,464** | **499,934,464** |
| **Maximum non-directory paths** | **5,000** | **30,000** |
| **Maximum logical path bytes** | **100,000,000** | **500,000,000** |
| **Reserved names / bytes** | **16 / 65,536** | **16 / 65,536** |

Threshold semantics are exact: nonempty files <131,072 B use SmallContent in
the current format; exactly 131,072 B enters the chunked path. Empty files are
their own class. Native physical owners can also have a 2 MiB ceiling; the
1 MiB files and 8/32 MiB anchors intentionally exercise different large-file
sizes without equating logical representation with physical encoding.

### Namespace and role assignment

Freeze a manifest with stable role IDs before implementation qualification.
Canonical initial paths are `dNNN/fNNNNN` with at most 100 regular names in a
data directory. Use **52/302 data directories**. The two movable 32-file
subtrees and one 32-file deletion subtree occupy three dedicated data
directories; their remaining 68 slots stay unused. The remaining directories
have sufficient capacity for all other files. Two additional empty move
destination parents and eight scratch empty directories give initial total
directories excluding root of **62/312**.
Eight scratch directories are removed before their eight replacement names
are created; subtree roots are removed before recreation. Directory moves do
not create parents. The maximum directory count is therefore also **62/312**.
Ordinary data directories remain capped at 100 regular names; the global path
cap governs aliases/temporary names. Maximum path depth is three components
after moving a data directory beneath a destination parent.

The first 1,024 tiny-file ordinals form sixteen disjoint 64-file refresh cohorts.
The next 32 tiny files belong to the deletion subtree, the next 64 to the two
move subtrees, the next eight to hardlink targets, and the next eight to edit
targets. None of these pools overlaps. Other tiny/medium/large files are
background or explicit witnesses; attribute targets intentionally include the
eight canonical hardlink targets plus eight additional tiny files.

Roles bind to logical IDs, so a move changes the path resolved for a role,
not the content generator or target identity. The exact manifest must record
every role-to-path mapping; no path selection depends on directory iteration
order or on observed benchmark timings.

## Compact fixture S

Sixteen regular files, **2,658,304 B**, five initial directories excluding root
(`tiny`, `medium`, `boundary`, `large`, `witness`). Distribution: eight 4 KiB,
two 64 KiB, the three threshold sizes, two 1 MiB, and one 4 KiB witness.
Caps: 20 non-directory paths, 4 MiB logical path bytes, eight directories.
This fixture supports new focused history/branch controls. It is not the
12-file fixture from the preliminary conversation, and gets a new identity.
S denotes a structural envelope: the profile-specific A/B/Z region overrides
in the family plan are part of the actual fixture/cache identity. Different
overrides must not reuse a master solely because their file sizes match.

For namespace/inode history, `tiny` contains all eight tiny files and moves
between `tiny` and `tiny-moved`. Use its first two files for refresh, next two
for edits, and next file for alias/replacement. Relative roles follow the move.

## Boundary fixtures B and BA

B has one target at the selected size plus a 4,096 B witness, two regular
paths and one data directory excluding root. Static target lengths are
4,096; 131,071; 131,072; 131,073; 1,048,576 B. Roundtrip starts at 131,071 B.
Cap: three paths, 2 MiB logical path bytes, two directories.

BA starts with a 131,071 B target, a hardlink alias to it and a 4,096 B witness:
three paths, two distinct regular inodes, 266,238 logical path bytes and
135,167 distinct-inode bytes. Cap: four paths, 1 MiB logical path bytes, two
directories. The alias replacement uses one temporary file at a time.

## Content, seeds and metadata

Seeds are 1, 2, 3; selected development defaults to seed 1. Even ordinary
role ordinals use deterministic structured text; odd ordinals use the existing
seeded pseudorandom content generator. Empty files are empty. Content class,
seed and role are part of fixture identity. No all-zero padding shortcut.

For exact/local/unique reuse comparisons in mixed refresh, each index group of
four uses the same base content class and length: indices 0/1 are recurring,
2 is a local variant, 3 is unique. Recurring A/B bytes omit cycle and branch
salt; local variants change a fixed 256-byte region; unique bytes include
seed, role, cycle and divergent branch tag. Publish actual logical, reused and
encoded byte numerators separately; mixed compressibility is not a speedup arm.

Fixture generation is independent of requested depth: K10 is the exact first
ten commits of K100 for the same family/size/seed/branch role. Refresh cohort
sequence for cycle c starting at 1 is 0 for odd c, and `(c/2 - 1) mod 16` for even c.
The first two cycles both visit cohort0, so K10 already includes temporal
recurrence. Initialize recurrent refresh/subtree bytes to A; first visit writes
B, second writes A. Recurrence uses per-cohort visit parity inherited at a fork,
not wall-clock time or a fresh child-local reset. Half the cycles hit a hot
cohort and half rotate. Other role pools are fixed; edit offsets
rotate head/middle/tail by cycle. Shared branch content omits branch salt;
divergent content includes it. Never mask a threshold case with compression
or use a generation tag in content declared exactly recurrent.

Initial file modes 0640, directory modes 0750, mtime 1,700,000,000 seconds,
nanoseconds 0. Explicit attribute-stage modes alternate 0600/0640 for files,
0700/0750 for directories; explicit mtimes are epoch + `10*cycle + branch_tag`
seconds and 123,456,789 ns (branch_tag 0 trunk/A, 1 B). Automatic write/rename
timestamps are observed with their actual semantics, not falsely equated to
the explicit deterministic attribute values. Atime/ctime are not persistence
oracles. File ownership follows the supported runtime; no chown/ACL claim.

See [workload high-water equations](workloads.md) and
[execution environment](execution-and-verification.md).
