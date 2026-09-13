# Host disk range tree component

Status: **four distinct component tests PASS**, reusing the two unaffected passes
from attempt 01 and repairing/rerunning only its two failures in attempts 02–03.
Runtime FUSE/SDK installation, publication coverage and public capacity/benchmark
qualification remain open. No benchmark or #122 workload was executed.

## Implemented behavior

`overlay_ranges.rs` provides immutable implicit-offset `Tree` handles over the
existing shared `Index` and `Payload` arenas. Split, merge, unequal replacement,
truncate, append and range reads follow the existing core PieceTree scheme and
SplitMix64 priority sequence. Fresh split-piece priorities are merged back through
ancestors to preserve heap order. Logical offsets derive from subtree lengths;
small insertions do not rekey or serialize the remaining suffix.

Each range node occupies one ordinary Index leaf: fixed summary under key 0,
owned child-root links under 1/2, fixed provenance under 3 and bounded backing
locator or inline bytes under 4. Values stay within the Index's 64-byte inline
value limit, avoiding overflow pages. A node's summary stores length, count,
height, priority and left-subtree length. Root restoration reads that bounded
header. There is no per-file root registry or complete piece vector in production.

This initial layout costs **one 4 KiB metadata leaf per range node**, plus shared
Index catalogs, temporary COW paths, old snapshots and cleanup slack. This known
space/write-amplification ceiling is charged by the Index; it is not a claim of
full million-file capacity or final performance acceptance. Default file length
and piece-count limits reuse the existing core constants, and tree depth is bounded.

Pieces represent Base(root, offset), exact owned Payload tokens, Zero, or at most
64 inline bytes. All nonzero pieces retain explicit logical OriginIds and
coordinates. Splitting a payload creates an independently owned `Payload::subrange`
token, so retaining one byte does not retain its original source tree or unrelated
blocks. Linked Index roots retain the graph and tokens after all per-file runtime
Root handles disappear. Node preparation and failed operations leave the input
Tree independently readable.

The owned cursor retains a consistent root while iterating and returns payload
slices with their own exact interval ownership. Returned pieces contain no old
Tree root. `next_descriptor` streams provenance directly for V3 without allocating
payload subrange tokens or reading payload. The cursor has one bounded tree path;
its bounded metadata reclaim assist prevents lookup-owner accumulation during long
scans. `read_piece` routes base reads through the supplied existing canonical reader.

Sequential `append_from` reuses lineage only while appending on the origin's
original, never-rewound payload source frontier. `Payload::join_tokens` produces
one independently owned union range, leaving retained readers unchanged. After
truncation moves the live end behind that frontier, append falls back before
consuming input and assigns a fresh source/origin, preventing coordinate reuse.
Physical canonical substitution cannot create a new extension frontier for an old
origin. No canonical encoder or second content pipeline was added.

## Evidence

| Case | Valid result | Evidence |
| --- | --- | --- |
| Unequal edits, split/merge, truncate, zero growth, input bounds, retained snapshot versus independent byte oracle | PASS retained | `attempt-01-tests.log` |
| 1024 one-byte pieces, unequal edit in middle, bounded path visits, owned partial reads, unchanged snapshot | PASS retained | `attempt-01-tests.log` |
| Drop inode link/tree/cursor; retain one-byte payload reader; metadata live pages 0, live payload 4 KiB; release reader then live payload 0 | PASS after repair | `attempt-02-slice.log` |
| 80 × 17-byte appends after initial 3 bytes, one final node covering 1363 bytes; original snapshot unchanged; truncate to 2 then append `z` yields `abz` with fresh tail origin | PASS after repair | `attempt-03-append.log` |

These are correctness/locality checks, not benchmark measurements or new wall-clock
performance gates. The 1024-piece check detects traversal/rewrite of the full suffix
using operation counters; it does not claim a speedup. Production exposes node read,
write, split, merge and cursor visit counters for later integration measurements.

## Preserved failures and targeted repairs

- Shared payload attempt 05 failed to compile the new range decoder because its
  index closure inferred `i32`; explicit `usize` repaired it. No range test ran in
  that attempt. See `../phase2-payload/attempt-05.log`.
- Range attempt 01: two PASS and two FAIL. The append case exposed that shared
  `Payload::append` accepted a truncated owner even when its end was behind the
  source frontier, consumed input there, then failed to join noncontiguous owners.
  The shared guard now returns `None` before reading; unchanged range code performs
  the existing fresh-source/origin fallback. The exact failed case passes in 03.
- Range attempt 01's final slice release exposed that `Payload::stats.cleanup_pending`
  omitted queued drops and disk release jobs. The cleanup loop saw false completion
  between queue transfer and interval release. The shared predicate now includes
  those obligations; the unchanged exact case passes in 02.

No range implementation/test changes were needed for those shared fixes. The two
other cases exercise Base/Inline/Zero operations and unchanged metadata ownership;
Payload append/cleanup predicates are not their relevant dependency paths, so their
passing evidence was retained instead of rerunning all four.

## Integration needs

Create one `Ranges` context per live Workspace, sharing an Index whose external
owner is that Workspace's Payload arena. Persist `Tree::root()` through the inode
record's owned Index link, and restore through the same context. Create fresh
occurrence ids for ordinary replacements; preserve ids/coordinates for slices.
Feed `Cursor::next_descriptor` into the V3 description/plan and use `read_piece`
for snapshot-owned replacement reads. The host coordinator must still atomically
install the resulting inode/range root with binding/change indexes and replay
results, and account Index/payload/retained-root resources in aggregate.
