# Native regular-file identity during initial acquisition

> **Status:** Current general guide. S9 checkpoint; backed complete-root acceptance remains open.

Project/import now groups its already retained regular-file job vector by native
(device,inode,source index), using in-place unstable sorting. The first source
index supplies the logical inode; its aliases bind that same serial. Equal payload
bytes on different native inodes stay distinct, even when CAS reuses their content
root. Links outside the imported root do not increase its namespace reference count.

Each group constructs its regular-file payload once. Four existing Namespace Init
workers borrow disjoint job slices from one short claimed-range cursor, never a
second alias map or an input-sized per-group collection. Files are checked before
read, through the original descriptor afterward and once at each original path
before the group is acknowledged. Device/inode/type/length/mode/mtime/ctime fields
must match the captured source; a changed path/metadata is refused without reread
or failed-operation replay. Source stability remains the operator's initial-input
obligation; these checks do not claim an atomic native filesystem snapshot.

The catalog still consumes its original path-count reservation. Alias serials stay
unused and are never recycled; emitted new-inode records exclude aliases and remain
strictly sorted, with sparse serial positions where necessary. Directory bindings
use root_serial plus canonical source index. Existing canonical constructors derive
namespace reference counts and validate topology. Formats, public Init API and
Store policy are unchanged; no root format, custom mutable index, third-party,
retry, synchronization or Commit algorithm is introduced.

For N regular paths, U distinct native regular files and B bytes in those U files,
group preparation isO(N log N), source metadata workO(N), and construction pays B
rather than rereading a large payload for every alias. Claimed-group boundary search
addsO(log N) per group. The original scan/job/frontier/child/input collections remain
input-sized; grouping does not qualify a bounded importer. NamespaceWork separately
records unique_files and regular_aliases and continues charging actual retained
entry/job/vector capacities. No heap/RSS/page-cache/speed claim follows.

Public complete-root tests include ignored data, Git index, dependencies, caches,
outputs, exact opaque symlinks, a regular hard link to .git/index, an outside-root
link and a separate equal-byte copy. Memory and both real macOS Store profiles
qualify shared serial/refcount2 versus separate copied serial/refcount1. Native
source is removed before readback. Linux Docker qualifies the portable acquisition
path; it does not replace the owning macOS global provider.

Append-only checks under issues/307/checks/s9-native-aliases retain the initial test
field-name compilation failure and its source diagnosis, builds, explicit120-second
functional checks, cache ineligibility and source/binary identities. This closes the
regular-file identity defect, not S9. Backed scan/namespace construction, full context,
consumer/application/restart custody and greater-than4GiB native qualification remain
open. P3/P6/P7/P13/P14 remain explicit later Commit prerequisites.
