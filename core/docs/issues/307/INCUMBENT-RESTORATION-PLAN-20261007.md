# Incumbent restoration and regression selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner direction 2026-10-07: stop the streaming run, restore the best 9.75 s
version, then run namespace Init and history stride 10, 3 and 1.

## Restoration

Bounded streaming Init (`5a9704610`, `890a144ff`, `1d3b6d3ae`) is withdrawn. Its
final source measured Durable100000 at 10.132066834 s against the incumbent's
9.749380917 s and missed all eight speed and allocation gates; see
[streaming results](INIT-STREAMING-RESULTS-20261007.md). Every path under
`core/crates` is restored byte-for-byte to `4a207cea1` (also the tree of evidence
commit `6567b6fda`): product Rust, shipped SQL and the owning public tests. The
candidate source stays on local branch `codex/init-streaming-candidate`; its
receipts, closed-copy manifests and the 24 streaming case registrations remain,
and those cases now refuse before any product attempt. Nothing is selected by an
error or benchmark switch.

## Prospective selection

[Selection record](checks/incumbent-restoration-20261007/selection.json). One
sample per case per arm, fixed order, no rerun of an unchanged arm. Source is
committed and sealed before sampling.

| Family | Cases | Arms | Speed gate | Allocation gate | Complete command / proof bound |
| --- | --- | --- | --- | --- | --- |
| Namespace Init, Monolithic with acquisition tables | Durable and Disposable × 100 / 1,000 / 10,000 / 100,000 files, `-incumbent-restored-v1` | candidate only; cluster-one-end control at `197d2fb7d` is reused, never executed | `10 × candidate ≤ 11 × control` | final database+WAL+SHM ≤ same-profile control | 30 s / 19 s |
| History, GroupRowsIndexed | Durable `-group-rows-indexed-v2` and Disposable `-group-rows-indexed-v1` × stride 10 / 3 / 1 (17 / 53 / 157 states) | pinned reference `7edddbdb8` then candidate, at one harness identity | `10 × candidate ≤ 11 × reference` | allocated ≤ 54,278,964 / 70,427,034 / 92,342,273 B | Durable 120 / 340 / 600 s and 24 / 24 / 60 s; Disposable 60 / 170 / 300 s and 12 / 12 / 30 s |

The restored product equals the work-reduction source that already has one
retained sample per Init case. The new Init rows are a second, separately
identified observation at a new commit requested by the owner; neither sample
replaces the other and no lower value is selected. The history limits are the
existing owner-approved family limits, not general defaults or test timeouts;
history uses one construction worker and Init its four constructors. History has
not been measured since product freeze `ba6499a61`; these rows test whether the
later acquisition, reclamation and Storage/Persistence changes moved it.

Init runs in the `init-entry-performance` measurement worktree and history in
the `save-vfs-amplification` worktree that owns the pinned reference checkout,
each detached at the sealed commit with worktree-local targets, locks and
outputs. Runs are sequential; no build overlaps a sample.
