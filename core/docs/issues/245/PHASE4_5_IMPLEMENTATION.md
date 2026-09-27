# #264 Phase 4.5 implementation checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source basis: `46bac18e6` on `codex/issue264-phase45`, first parent
`6115dfcd2`, itself based exactly on `ef3a31048` from PR #263. The private
page-write telemetry and this checkpoint are in the same follow-up commit.
PR #263 remains stacked on unmerged PR #260; neither is merged here. The
[Phase 4.5 design](PHASE4_5_IDENTITY_RELATIVE_NAMESPACE.md) is the planning
baseline, and [Phase 4](PHASE4_INHERITED_RENAME.md) preserves its own prior
evidence. No performance sample or release admission is claimed.

## Implemented mounted route

Resident Nodes hold one budget-charged component name, stable serial, parent
serial and attached state, rather than a 4,096-byte absolute-path array.
`State::collect` retains the complete ancestor closure of every pinned live
directory. Detached held directories remain readable through a pinned handle
view, but lookup and mutation reject them as parents. Each live ancestor edge
was created by a checked child lookup or directory create, moved with the
same-root rename publication, or detached by removal/replacement. Rename
rechecks the destination's attached chain under the final publication lock;
forget cannot silently remove an edge between its earlier check and that lock.

Lookup/list/create/remove/readdir/rename take a parent serial and one checked
component. Stale file reads and `serial_original` use canonical
`InodeAttributes`; symlink readlink uses additive Bridge/Service
`InodeReadlink` subtag 8 and C1's existing `readlink_inode`. Direct whole-path
Inspect, source import and one pathname syscall keep their own limits. Readdir
uses the handle's frozen directory view for children; `..` presents the moved
Node's current parent, or its last parent after detachment.

Rename edits the existing one or two COW parent deltas and, when the moved
directory is resident, its one parent/name edge. It has no inherited-descendant
scan, resident-descendant rewrite, subtree copy-up or new parent-index key.
An unresident source directory succeeds: the growing-prefix count case obtains
its serial from the independent Service oracle without looking it up in the
Workspace before rename. The retained attached-chain walk uses `node_index`,
so its source-derived local cost is `O(c·h + d·log P)` for a constant `c` edited
keys, private height `h`, actual depth `d` and `P` resident nodes, independent
of inherited descendants `N`. This is an algorithmic bound, not a measured
latency. Forget/collection work is separate and may walk each pinned directory's
ancestors; its worst local bound is `O(P·d·log P)`. The retained names and
ancestors stay charged to the host budget.

## Focused release-binary proof

| Selection | Observed result |
| --- | --- |
| Linux `inherited_workspace` initial full selection | 9 passed, 0 failed, 0.59 s test time. Covered inherited and fresh moves, stale/held reads, frozen G1/G2, nested move/back, invalid replacement, cycle refusal, ancestor forget, detached held parent, canonical symlink and cached/uncached deep traversal. |
| Changed focused budget and count selections | Both passed at the final test source, 0.05 s and 0.08 s respectively. No unaffected passing selection was rerun merely for a fresh number. |
| Native Bridge `attributes` | 2 passed, 0 failed; subtag 8 roundtrip and invalid serial refusal included. |
| Public `WorkspaceApi::exec` deep move | 1 passed, 1.14 s test time. One shell Exec made 13 component-relative destination directories, moved a base directory with an inherited leaf to a 4,123-byte logical descendant, read that leaf through relative `/proc/self/fd` traversal, and replaced a shallow temp file. One explicit Commit followed zero exit. An independent serial-based Service oracle checked moved identities/modes, both heads' deep bytes and content root, old-path absence, replacement bytes and absence of `.next`. |
| Existing public inherited move Exec | 1 passed, 1.01 s test time, including its separate old/new full-tree oracle and explicit Commit. |

The deep fixture stays within macOS source pathname limits. Its mounted read
uses ordinary Linux directory file descriptors and component-sized `cd` paths;
the shell does not pass a 4,123-byte pathname argument. The first fixture
attempt exceeded the macOS source pathname limit, a later command exceeded the
Exec wire bound, and ordinary shell `cd` reached Linux pathname refusal. Those
were test setup/command failures before the final component traversal; none
was retained as a passing product result. Existing release daemon image layers
were reused with the new locked-release musl daemon binary. Final deep-Exec
image ID: `sha256:9d51a960b3ac8dd8b5879ba94205574a0e64fe48b6988e7f751875911c0bcf8b`;
daemon SHA-256:
`fc7e5d2c3673fb47cac08d51e7814b9f390114c0d9b4f2812fb886e8c87ee84f`.
The host release `agent_route` binary SHA-256 was
`41a41863abfe96d9f0e27713d108c8bc9433291c3fd8f3614be5fbcb9988e0d0`.

### Untimed growing-prefix count diagnostic

One release test compared a successful move with 3 versus 67 inherited
descendants. The source directory was not resident before either rename. These
are operation counters, not cache-qualified speed samples:

| Counter during rename | 3 descendants | 67 descendants |
| --- | ---: | ---: |
| Upstream Store calls | 2 | 2 |
| Private metadata page reads | 2 | 2 |
| Private immutable page-file writes | 4 | 4 |
| Net newly allocated private pages | 3 | 3 |
| Added metadata backing bytes | 16,384 | 16,384 |
| Added total backing bytes | 16,384 | 16,384 |
| Added charged host memory bytes | 2,199 | 2,199 |
| Resident Nodes before rename | 4 | 4 |
| Explicit Commit and clean close | PASS | PASS |

Four page files were written while three pages remained newly allocated; the
write counter includes a candidate page later retired. The same test compared
the unchanged inherited sibling file's serial and content root across the old
and new canonical heads. Equal content roots preserve its chunk graph; the
test did not enumerate each chunk ID or directly record `saved_files`.
The separate atomic-refusal case set private backing quota to **8,192 bytes**
from a zero-byte baseline allocation. Growing rename refused before revision
publication, the old binding remained, the destination stayed absent, and
cleanup passed.

## Checks and remaining bounds

Locked release builds only: host Bridge/SDK tests, Linux SDK external test and
musl daemon. The changed-package warning-denying release Clippy, Core format
check, `core/tools/check_product_boundary.py` (317 files), its 9 self-tests,
and `git diff --check` passed. Each focused test command completed below 30 s.
The aggregate Core test suite and examples were not run; the selected external
cases cover the changed route. The full-size public benchmark was not run per
owner direction.

C1's full-base parent-alias validation and moved-effective-subtree cycle scan
remain inside explicit Commit. **C1 internal page visits and file-save counts
were not instrumented in this diagnostic**; the 2 upstream calls and 4 private
page writes are not substitutes. No path-local Commit, latency, cold-cache or
release PASS follows. The 128-handle bound, private level/slot and directory
count encodings, direct whole-path Inspect, source import, span and C1 tree
ceilings remain separate. The #264 issue stays open for any owner-required C1
visit accounting or broader closure review.

## Per-commit production LOC

`tools/production_loc.py --json` counted exact first-parent and staged-tree
snapshots from `git archive`, excluding tests/docs/tooling and legacy inline
tests. The reference implementation remained **65,417** production lines:

| Commit | Core before → after | Combined before → after | Signed delta |
| --- | ---: | ---: | ---: |
| `6115dfcd2` ancestor closure | 58,779 → 58,841 | 124,196 → 124,258 | +62 |
| `46bac18e6` identity-relative route | 58,841 → 58,704 | 124,258 → 124,121 | −137 |

The follow-up telemetry/report commit records its own exact comparison in its
commit message. Core and reference coexist: the combined decrease is mounted
path-state removal and responsibility relocation, not a claim about C1 Commit
complexity or retirement of reference source.
