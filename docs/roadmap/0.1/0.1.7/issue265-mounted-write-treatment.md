# Issue 265: tiny payload and extent ownership treatment

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Prospective workload, format, counter, resource and oracle specification.
> Commit this before changing the runner or taking any new sample.

## Frozen public workload and evidence

The parent [three-pattern contract](issue261-three-pattern-100-spec.md) fixes
the 10,485,760-byte old head, writer bytes and offsets, one Mount, one public
Exec of one generic shell/writer/fd with 100 one-byte syscalls, one public
Commit, independent full old/new-head verifier, and append/dispersed/repeated
order. Reuse those exact commands and the closed, verified Store/history master
through validated independent writable byte copies. This treatment changes no
writer, fixture, deadline, cache rule, or construction worker count. Label the
new attempts `issue265-{append,dispersed,repeated}-100-10m-v1`; retain the
historical #261 rows under their original identities.

Build the affected SDK driver, verifier and aarch64 daemon from this worktree
with locked Cargo **release** and its local target. Reuse the sealed static
writer only after its source and binary hashes match; rebuild the daemon image
for a changed daemon. Reprove the old master with the changed verifier before
using it. Pin source/tree, product, compilation, dependency and harness seals,
`.cargo/config.toml`, spec/writer/master/clone hashes, binary archive hashes,
image ID, clone method and reuse in every receipt. Use this worktree's
`benchmark-results/.run.lock` and fresh result directories. Keep one attempt
per case/arm/source, all failures and ineligible rows, and 25/50/75/100
within-run checkpoints. The complete command limit remains 15 s and the
separate verifier limit 9 s; each focused test command stays under 30 s.

Ordinary host/container cache is uncontrolled and never primed. Every raw
Exec, Commit and complete wall is `INELIGIBLE` for a numerical speed claim;
count reductions are separate diagnostic evidence. Build overlap from another
worktree during a timed phase is recorded as interference. No failed attempt
is rerun to obtain a favorable number.

## Versioned tiny private payload profile

One immutable accepted write keeps one payload ID, private file and owner.
For a payload of **1..=4,016 bytes**, its only segment is exactly one aligned
4,096-byte page: the existing 80-byte identity fields with a distinct
`LFSWPLD2` magic and version 2, followed by the logical bytes at offset 80
and zeros through the page end. This is one payload per page, never a pool of
writes. A larger payload keeps the current `LFSWPLD1` version-1 header page,
aligned data pages, segment sizing and bounded windows. The transition at
4,016/4,017 is a format boundary, not a change to `Source` or Workspace API.
The expected one-byte profile is one create, one 4,096-byte observed allocation
and one aligned write instead of 8,192 bytes and two writes; 100 accepted
callbacks predict 409,600 cumulative payload-write bytes. These are API
transfer counts, not device traffic or a latency claim.

Admission reserves the exact profile before create; observed `st_blocks`
reconciles the reservation. Exclusive create, identity/collision refusal,
partial-failure retention, quota and cleanup charges stay per owner. The reader
authenticates the declared version and identity, checks inline zero padding,
serves arbitrary ranges up to EOF, and refuses incompatible or truncated pages.
Both versions retain G1/G2 custody and cannot adopt a foreign private file.
Old roots stay immutable; one accepted FUSE write becomes visible atomically.
The independent native proof covers one byte, 4,016, 4,017 and a larger
multisegment input; reads, bounds, quota, collision, partial failure, padding,
cleanup and retained ownership are checked through the public Workspace API.

## Count before changing the extent algorithm

Record bounded, saturating product counters for extent leaf/branch page
visits and writes, leaf record occupancy and branch child occupancy, root
height, and child/custody edge increments and decrements. Separate these
from keyed metadata page reads/writes and 4 KiB ledger reads/writes. Report
the live extent page-kind and occupancy snapshot at 25/50/75/100, with its
bounded diagnostic read work excluded from any latency claim. Reconcile page
counts with the arena's live-page total and per-phase edge totals before
claiming fragmentation or altering the B+ splice. Any local merge or reference
transfer must preserve path-local work, immutable old roots, atomic publication,
ledger identity/checksum/quarantine and exact resource charging. If counts do
not establish avoidable churn, leave the algorithm intact and report that
decision.

For dispersed final-run Commit, split C1 `load_node` counts into stored-page
and draft loads; record `FileInput` record lookups and Store provider work.
The prior 2,028 logical loads do not imply 2,028 Store reads. Change the C1
algorithm only if those counts isolate substantial avoidable stored work in
the 100 final runs. Preserve the frozen extent stream, exact canonical root,
one History publication and no replay of superseded writes.

## Gates and reporting

Each public case must prove exactly 100 observed FUSE WRITE callbacks, one
Exec and one Commit, expected final runs (1/100/1), full old/new file bytes,
head parent and path inventory, quota/resource outcomes, clean Unmount/Delete,
and G1/G2 custody. Report actual private payload allocations and aligned
read/write calls alongside metadata page, ledger, C1/Store and resource counts;
unavailable fields remain explicitly unavailable. A focused native failure
proof and Core boundary/test/Clippy/fmt checks cover the changed paths once at
the frozen final source, with every gap stated.

Attempt the separate #248 4,097-write public gate once at a frozen source only
if the #266 FUSE outcome and the 30-second Exec timer owner make it meaningful;
otherwise record `NOT_RUN` and why. Its timer is not part of this diagnostic's
15-second complete-command budget. No benchmark limit, cache policy or worker
count is relaxed to produce a pass.
