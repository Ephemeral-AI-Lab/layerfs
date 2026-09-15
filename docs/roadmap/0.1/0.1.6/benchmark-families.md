# Families and tests to produce

All IDs are new versioned additions. Preserve existing IDs and their fixtures;
do not relabel v0.1.3-v0.1.5 evidence. `cases.json` expands every row below into
an exact selected case. Every regular row supports separate perf and verify.

| Family | Action | New regular cases | Scope |
| --- | --- | ---: | --- |
| dedup_branch_history | Extend | 6 | Three missing history profiles × K10/K100 |
| file_size_transition | New | 7 | Five fixed sizes + two transition/inode sequences |
| mixed_load_bearing | Extend | 4 | M1 sequential × L100/L500 × K10/K100 |
| multi_workspace_development | New | 4 | M1 concurrent × L100/L500 × K10/K100 |
| branch_development | New | 6 | Four load-bearing forks + two compact graph controls |
| historical_access | Extend | 6 | Before/after transitions, inode replacement, fork point, branch head |
| **Total** | **Three new families, three extensions** | **33** | **12 mixed load-bearing cases included** |

S/L100/L500/B/BA refer to the exact [fixture definitions](fixtures.md).
K counts local new `Created` commits, excluding imported state and inherited
ancestry. Every case records local counts, total distinct commits and longest
ancestry separately. All new-family code should follow existing family
entrypoints and shared runner/oracle layout, not add an independent framework.
`retained_graph_roots` in the registry counts initial/commit state references,
not distinct root ObjectIds: recurring content may reuse an earlier root ID.

## dedup_branch_history: six additions

Existing `distributed`, `hotset`, `recurring`, `metadata`, `unrelated` profiles
already contain depth 10 and 100. Keep them as inherited coverage. Do not create
duplicate renamed small-hotset/distributed scenarios without changed semantics.

| ID template | Fixture | Branches / live workspaces | New commits | Schedule |
| --- | --- | --- | --- | --- |
| v016-history-large-hotset-k{10,100}-v1 | S | 1 / 1 | K | One 256-byte SDK edit per commit, alternate the two 1 MiB files, cycle head/middle/tail; revisit A/B values with fixed hot regions |
| v016-history-namespace-inode-k{10,100}-v1 | S | 1 / 1 | K | Five-stage compact schedule HN below |
| v016-history-boundary-cycle-k{10,100}-v1 | S | 1 / 1 | K | Two single-file SDK calls per commit exchange one byte between 131071/131073 files, shrinking first, then reverse next commit; exact128KiB control unchanged |

HN repeats five stages: (1) unlink/recreate two tiny files, one recurring and
one generation-specific; (2) two single-file SDK calls overwrite 256 bytes in two other tiny
files; (3) rename the populated `tiny` directory to its alternate name;
(4) create one alias to a fifth tiny file, chmod two files plus this directory,
set both files' explicit mtimes; (5) one 4 KiB atomic save over the aliased
destination, observe old inode through alias, remove alias. Unique replacement
and toggled metadata make every stage Created. All other S contents remain
unchanged. Maximum names S+2, logical bytes S+8192, directories unchanged.
HN has four POSIX helper executions and two SDK calls per five-stage cycle;
helpers finish before each Commit. Perf does not run oracle reads.

Large-hotset/boundary profiles have no mutation helper Exec: SDK edits are host
public calls. Visibility/reopen proof is separate verify work. Histories have
K+1 states and all parent edges checked; S is small enough to target full
content verification at every state, subject to the same honest deadline.

Large-hotset commit j (1-based) selects file `(j-1) mod 2`, region
`floor((j-1)/2) mod 3` (head/middle/tail), and alternates B/A by visit count to
that exact (file,region). Initialize these regions to A. Global commit parity
must not always write the same value to a revisited region. HN SDK target bytes
likewise alternate by per-target visit count; every new edit differs from its
immediate predecessor. SDK cross-file sequences are not advertised as atomic
batches; stop the case on any member failure.

## file_size_transition: seven additions

| ID | Fixture/initial target | New commits | Operations |
| --- | --- | ---: | --- |
| v016-boundary-small-control-v1 | B, 4096 B | 2 | Two public SDK 256-byte overwrites |
| v016-boundary-below-v1 | B, 131071 B | 2 | Same |
| v016-boundary-exact-v1 | B, 131072 B | 2 | Same |
| v016-boundary-above-v1 | B, 131073 B | 2 | Same |
| v016-boundary-large-control-v1 | B, 1048576 B | 2 | Same |
| v016-boundary-roundtrip-v1 | B, 131071 B | 4 | SDK append1, append1, remove1, remove1; lengths131071→131072→131073→131072→131071 |
| v016-boundary-alias-roundtrip-v1 | BA, 131071 B | 5 | POSIX append1 via alias; append1 via target; truncate1 via alias; truncate1 via target; atomic replace target using new same-length content |

All use one branch/workspace. Fixed-overwrite offsets are 0 then 2048, identical
across sizes; replacement construction is untimed. BA closes handles before
each commit, preserves the original alias after final replacement, and verifies
shared inode identity before replacement and separated identity afterward.
Its maximum temporary pathname count is four, maximum alias-charged payload
<400 KiB. SDK-only and alias/POSIX results must not be pooled into one API ratio.

## Twelve mixed load-bearing cases

| Family / ID template | Fixture variants | Local depth variants | Branches / simultaneous workspaces | Total new commits | Longest ancestry |
| --- | --- | --- | --- | --- | --- |
| mixed_load_bearing / v016-mixed-development-{100mb-5000,500mb-30000}-k{10,100}-v1 | L100/L500 | 10/100 | 1 / 1 | 10/100 | 10/100 |
| multi_workspace_development / v016-workspace-mixed-{100mb-5000,500mb-30000}-k{10,100}-v1 | L100/L500 | 10/100 per worker | 2 / 2 | 20/200 | 10/100 |
| branch_development / v016-branch-mixed-{100mb-5000,500mb-30000}-k{10,100}-v1 | L100/L500 | 10/100 per child, trunk10 | 3 / 2 | 30/210 | 15/105 |

The exact [M1 stages and topology schedules](workloads.md) are shared and
versioned once. Two-workspace cases include one discard/reopen after commit5;
branch-fork cases use trunk commit5 and have no discard probe. No merged/rebased
history is claimed; the current supported operation is historical Fork.

Pairwise shared/private contents exercise cross-branch dedup and independence
inside the same case. Independent worker clocks record actual overlap. A
same-branch second lease is expected to fail; it is not an extra successful
workspace. Reuse the existing `workspace_reliability/lease-lifecycle` proof as
an implementation regression dependency, not a duplicate performance family.

## Two compact branch controls

| ID | Fixture | Graph | New commits / longest ancestry | Maximum live workspaces |
| --- | --- | --- | --- | ---: |
| v016-branch-convergent-content-v1 | S | Trunk10; A10/B10 fork from trunk5 | 30 / 15 | 1 |
| v016-branch-fork-descendant-v1 | S | Same, then C10 forks from A's local commit5 | 40 / 20 | 1 |

Every local commit performs one 256-byte SDK overwrite on the first medium
file. Let j be the ancestral schedule ordinal: trunk1..10, children6..15 after
forking trunk5, and descendant11..20 after inheriting trunk5+A5. Offset is
`4096*((j-1) mod 2)`; payload alternates A/B by `floor((j-1)/2) mod 2`.
Initialize those regions to a distinct Z. Thus every write differs from the
inherited value, rather than becoming a no-op on its third visit. Convergent
children use identical content changes and must have equal file-content IDs;
automatic metadata and whole-tree roots may differ across separate executions.
Descendant control salts B/C changes by branch and preserves all sibling heads.
HN/M1 are not run on these controls. Fork C inherits trunk5+A5, then adds10.
No idle-workspace concurrency claim is made by these sequential graph controls.

## historical_access: six additions

Access creates no commits. Each invocation mounts one selected retained state
with one live workspace. Producer identity, graph size and selected ordinal
are explicit inputs, never an automatic history-building setup step.

| ID | Sealed producer (seed-matched) | Exact state / operation |
| --- | --- | --- |
| v016-access-boundary-before-v1 | history-boundary-cycle-k100, S,101 roots | Commit48; full131071B read of below-role file |
| v016-access-boundary-after-v1 | Same | Commit49; full131072B read of same role |
| v016-access-inode-before-v1 | history-namespace-inode-k100, S,101 roots | Commit94 (stage4 cycle19); full4KiB reads of target and alias, stat/nlink equivalence |
| v016-access-inode-after-v1 | Same | Commit95; target full4KiB, alias ENOENT, target identity/content at replacement |
| v016-access-fork-point-v1 | compact convergent branch control, S,31 roots | Trunk commit5; full64KiB read of first medium file and branch metadata |
| v016-access-divergent-head-v1 | compact descendant control, S,41 roots | B local commit10; full64KiB medium read, compare expected B content without altering A/C |

Default reader profile is fresh-session/application-cold, uncontrolled OS cache,
with no pre-read. The full producer commits are not attributed to read latency.
The access verifier repeats the declared reads independently with original
oracles. Existing warm/cold access cases remain inherited; these six do not
multiply every operation by another cache-state matrix.

## Explicit extended work: three additions, never selected by regular default

| ID | Mode | Fixed work | Complete watchdog |
| --- | --- | --- | --- |
| v016-mixed-exhaustive-100mb-5000-k100-v1 | Verify only | Reopen sealed M1 sequential K100 Store; every namespace, file byte and metadata state across all101 roots | 120s |
| v016-mixed-exhaustive-500mb-30000-k100-v1 | Verify only | Same at L500 | 300s |
| v016-workspace-four-100mb-5000-k100-v1 | Perf and verify separately | Four live branches/workspaces,100 M1 commits each,400 total; regular affected-set oracle for its verify mode | 60s each |

These are finite workloads and maximum cancellation budgets, not durations to
fill. Exhaustive proofs have no perf distribution and no replay construction
hidden before their clock; input custody/open/read/cleanup all count. Required
input must be explicitly supplied. Extended proofs are required to claim their
exhaustive scope, not to relabel the regular affected-set proof as exhaustive.
Unselected extended cases are NOT_RUN_EXTENDED, never PASS. Existing optional
repository_history profiles stay unchanged; no new timed soak is planned.
