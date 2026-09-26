# #237: fresh C1 validation state treatment

> **Status: Research; informative and not a product contract.** Frozen
> before source edit or candidate sampling. One distinct C1 memory question;
> no repeat speed arm and no claim of end-to-end bounded native import.

## Cause and treatment

On the kept direct-inode Core source at `ef59cabc652824e6508ec9fc3e49c0a43114b65a`,
the 100k/500-MB native Init still reaches 127,631,360 B peak RSS during
namespace Save, 22,298,624 B above file-loop high-water. C1's generic
fresh validation allocates a `BTreeMap` entry for every bound child after
its duplicate-parent check, although `run` never reads the returned
`CheckedInput.additions`. Its build-reachability walk also copies every
directory's child serials and enqueues/marks regular files that have no
child bindings. `unreachable_parents` first records every bound child to
identify only the unbound new directory parents.

Keep the public `validate::check` output unchanged. For the internal
`base: None` C1 run only, omit the zero-count additions materialization
that no later C1 step consumes. Traverse fresh directory bindings directly
from their already-sorted `FilesystemInput` updates, and enqueue only
directory children for reachability. Rewrite `unreachable_parents` to
start from candidate new directory parents and remove those that any
binding reaches. Preserve every existing rejection, work count,
canonical object, resource ceiling and generic update behavior. Add no
file, spool, worker, dependency, Save or cache policy.

## Decision and proof

Use the retained direct-inode [control receipt](evidence/c3-inode-fusion-20260924/candidate/receipt.json)
only if the same release diagnostic runner, driver, phase markers, source
manifest, build flags and cache label can be reconstructed exactly; else
freeze one new control/candidate pair before either sample. Run one
candidate public `Client::init_project` call on a fresh independent byte
copy and Store/History with 0 resident payload pages, then the separate
full reopened 101,001-path, 500,000,000-byte oracle. Directory/inode
metadata cache remains unqualified; raw times are diagnostic only.

The local adoption gate is **at least 8 MiB less whole-call peak RSS** than
the identity-matched control, with full readback PASS, no canonical or
Store ownership regression, and no new failed Core tests. Report explicit
live collection cardinalities and phase high-waters. A miss is retained
and the source rejected without resampling. The registered debug SDK 100k
row and #229 sparse-history gate remain separate. Even a pass is a bounded
C1 scratch improvement, not a bounded source or input representation.
