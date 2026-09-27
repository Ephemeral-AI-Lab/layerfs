# Issue 261: five public mounted-write rows after #266 and #265 merge

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Product source is draft #262's `6bcfa464f74ae9ca3859df31c678985ec69ba098`,
the merge of #267 (#266) followed by #268 (#265). All five rows carry the
same product seal `caa60189727a0a57d18a24f4e787d47e13949e50ed6c74d8b0bdf1b2ea5428da`.
The 8,194-byte separated 100/512 rows were retained at docs-only source
`ebbd5606fb5087c2b5934e156279eaca3de158e9` on the
[#271 evidence branch](https://github.com/Ephemeral-AI-Lab/layerfs/blob/3c3343e0b116f99b1e4abe1a3e9795fff28b3f79/core/docs/issues/271/BASELINE.md).
The three 10 MiB pattern rows were selected by the [prospective merged report
specification](../../../../docs/roadmap/0.1/0.1.7/issue261-merged-three-pattern-report-spec.md)
and taken once each at reporting source `95d28d0590e70d763b5ad2e8dc3a52fe7bb55eb4`.
Its only changes since the merge are a workload specification and the reviewed
prepared-master source allowlist; all product code and build flags match the
merge. It copied the closed, independently verified #265 master without
modifying that owner's worktree and rebuilt locked release SDK, verifier and
daemon binaries in this worktree. Every case used one public Mount, one Exec
of the generic one-process/one-fd writer, one explicit Commit and a separate
full old/new-head verifier. Every row observed its exact 100 or 512 FUSE
WRITE callbacks, passed the oracle and cleanup, and retained one attempt.

## Raw time and private backing

All rows have `functional=PASS`, `verifier=PASS`, `cleanup=PASS`, and
`cache/performance=INELIGIBLE`. Ordinary host/container cache was not
controlled, so the raw times below are **not** qualified speed comparisons
between cases or sources. Complete command includes launch and teardown;
verifier runs separately.

| Case | Exec / Commit / complete command | Verifier | Final private payload + metadata = allocated backing | Sampled host / daemon RSS |
| --- | ---: | ---: | ---: | ---: |
| 100 separated positions in 8,194 B | 410.623 / 35.478 / 1,326.981 ms | 13.660 ms | 400 + 40 = **440 KiB** | 28.70 / 35.01 MiB |
| 512 separated positions in 8,194 B | 2,774.466 / 71.214 / 4,095.102 ms | 13.667 ms | 2,048 + 120 = **2,168 KiB** | 32.06 / 39.09 MiB |
| 100 true EOF appends to 10 MiB | 359.775 / 33.760 / 4,258.153 ms | 98.752 ms | 400 + 24 = **424 KiB** | 30.00 / 34.89 MiB |
| 100 dispersed edits to 10 MiB | 419.071 / 41.882 / 1,413.663 ms | 104.528 ms | 400 + 36 = **436 KiB** | 30.27 / 35.04 MiB |
| 100 repeated edits at one position | 411.194 / 25.689 / 1,320.225 ms | 98.376 ms | 8 + 20 = **28 KiB** | 33.17 / 34.59 MiB |

The append complete-command row includes about 3.285 s outside its reported
Mount, Exec/Commit and cleanup spans, versus 0.365–0.389 s in the other two
pattern rows. The retained events do not isolate that extra wall; this one
sample must not be replaced or used to rank append against the other cases.
RSS is a **sampled process-shared operation maximum**, not a phase-local
cgroup peak or page-cache/anonymous split. The product's 851,968-byte
reservation in each last backing snapshot is a separate quota reservation,
not additional allocated payload in the table. The 10 MiB old file is a
closed prepared Store input, not private backing created by the edits.

For the identical workloads on earlier, separate source identities, final
private backing was **836 KiB** for separated 100
([#261 receipt](evidence/100-v3-ledger/receipt.json)), **4,188 KiB** for
separated 512 ([#266 receipt](../266/evidence/treatment-512/receipt.json)),
and **824 / 960 / 36 KiB** for append/dispersed/repeated
([#261 pattern report](THREE-PATTERN-DIAGNOSIS.md)). The merged snapshots are
440 / 2,168 / 424 / 436 / 28 KiB respectively. These are final allocated
backing comparisons across source identities, not cache-qualified latency
claims. The new one-page tiny format also changes the source-derived aligned
payload-write work from two 4 KiB writes to one per accepted one-byte write:
800 → 400 KiB per 100 callbacks, or 4,096 → 2,048 KiB for 512. These are
product I/O API bytes, not measured physical-device traffic.

Each separated clone's Store file was 618,496 → 897,024 bytes. The 10 MiB
pattern clones were 626,688 → 913,408 bytes for append/repeated and
626,688 → 917,504 for dispersed. History stayed 86,016 bytes in every row.
These are SQLite file sizes, not Store operation counts or device-write bytes.

The 100 and 512 separated receipts are [here](https://github.com/Ephemeral-AI-Lab/layerfs/blob/3c3343e0b116f99b1e4abe1a3e9795fff28b3f79/core/docs/issues/271/evidence/combined-baseline-v1/100/receipt.json)
and [here](https://github.com/Ephemeral-AI-Lab/layerfs/blob/3c3343e0b116f99b1e4abe1a3e9795fff28b3f79/core/docs/issues/271/evidence/combined-baseline-v1/512/receipt.json).
The [append](evidence/merged-patterns-v1/append/receipt.json),
[dispersed](evidence/merged-patterns-v1/dispersed/receipt.json) and
[repeated](evidence/merged-patterns-v1/repeated/receipt.json) receipts, raw
logs, independent verifier outputs, release preparation record and hashes
are [retained together](evidence/merged-patterns-v1/).

## What changed in the architecture

| Before the two child PRs | After their merge |
| --- | --- |
| A one-byte private payload used an 8 KiB allocation and two aligned 4 KiB payload writes. | #265 stores a 1..=4,016-byte payload and identity in one 4 KiB page, making one aligned write per one-byte callback. Larger payloads retain the earlier format. |
| Narrow edits could leave many sparse extent leaves; the earlier 100-dispersed source had 32 reachable leaves and 37 aggregate live metadata pages. | #265 balances only touched leaves. Its source-equivalent 100-dispersed tree had two leaves; the merged public row has seven aggregate live metadata pages. Untouched subtrees remain shared. |
| New page ownership finalization made an extra authenticated ledger read and write to set its completed-edge flag. | #265 sets that flag in the first owner ledger write and tracks the acknowledged prefix for definite partial-failure cleanup. The native old-root/shared-custody proof passed on its source. |
| A following WRITE could receive `EBUSY` while a healthy preceding WRITE still held the reply permit during a post-reply status snapshot. | #266 waits only for that Ready, bound reply-gap permit until the original callback deadline, then rechecks state. Failed/unbound and other Busy paths still refuse. There is no mutation replay or extra per-write Service call. |

The common public route still performs **one FUSE WRITE request/reply per
one-byte syscall**, with one LOOKUP, one OPEN, three GETATTRs and zero READs
in each retained case. It reports four upstream Workspace Service calls for
the whole operation, not one per WRITE. Each successful projected WRITE
publishes a new immutable Workspace root and invokes inode invalidation.
The old full-registry payload scan is absent on the routine path; candidate
reclaim remains. Commit uses the frozen **final** extent sequence, rather
than replaying 100 or 512 intermediate versions.

The exact first-parent production LOC comparisons on the two merge commits
are #267 **123,949 → 124,057 (+108)** and #268 **124,057 → 124,362 (+305)**
combined lines; all +413 lines are in replacement Core, with reference
source unchanged at 65,417. This is source-size accounting, not performance.

## Work counts and complexity at these sizes

| Case | Final extents / changed runs | Final extent root height | Extent leaf / branch writes during Exec | Child / Local custody edges added | Ledger 4 KiB reads / writes | Live metadata pages |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Separated 100 | 200 / 100 | 1 | 102 / 38 | 83 / 3,680 | 2,905 / 1,511 | 8 |
| Separated 512 | 1,024 / 512 | 1 | 527 / 450 | 3,896 / 23,139 | 20,953 / 12,150 | 21 |
| True append 100 | 101 / 1 | 0 | 100 / 0 | 0 / 5,050 | 2,447 / 1,270 | 4 |
| Dispersed 100 | 201 / 100 | 1 | 101 / 39 | 78 / 3,512 | 2,916 / 1,516 | 7 |
| Repeated 100 | 3 / 1 | 0 | 100 / 0 | 0 / 100 | 2,859 / 1,289 | 4 |

For `N` accepted one-byte writes, the public route necessarily has `N`
kernel FUSE WRITE round trips, payload acquisitions and atomic root
publications. It has a fixed number of public/control and host Service calls
per Exec/Commit in these tests. The extent B+ page codec holds at most 124
records per leaf and 248 child refs per branch. A narrow splice reads the
touched leaf and ancestor path, then writes a copied leaf and ancestors;
publication and old-root reclaim visit the Local and child custody edges on
those copied pages. The ledger batches adjacent references within a 62-owner
ledger page, but a copied branch still names every current child.

This gives bounded per-write work of `O(124 + 248H)` for the page/edge path
at fixed page format and tree height `H`, plus acquisition and cleanup. The
fixed constants are costly. Within the observed range, **leaf and root
occupancy grow with write count**: one full append leaf caused 5,050 Local
edge additions across 100 appends, while separated writes grew from three
root children at write 100 to sixteen at write 512 and made 3,896 cumulative
child-edge additions by 512. Thus the cumulative page-local edge work can
look quadratic over this finite range. It is not a claim that global runtime
is `O(N²)` for all file sizes: page fanout is fixed, the tree gains levels,
and the file has a 4 GiB bound. #265 removes sparse-leaf and finalization
costs; the copied root/leaf ownership term remains for #271.

The retained private payload space is one 4 KiB page per still-owned tiny
write, plus extent leaves and branch/root/custody metadata growing with `E`
final extents. The 124-record ceiling sets a lower bound on leaf count;
actual half-split occupancy determines the charged page count. Append,
dispersed and separated rows retain `O(N)` payload
pages at their final snapshots. Repeated editing retains only two payload
pages and one leaf (`O(1)` retained private content here), though it still
allocates and writes 100 payload pages cumulatively. Commit streams the
final `E` extents and applies the `R` final changed runs. This explains
small Commit walls for repeated and append despite their 100 Exec writes;
the dispersed row has `R=100`. Its 2,028 C1 node loads comprise **two
stored-provider requests and 2,026 in-memory draft loads**, not 2,028 Store
or device reads. Exact C2 SQLite page I/O and physical-device bytes are not
exported by these receipts; Store file-size growth is not a substitute.

The separate #266 4,097 gate was `FAIL` at its 25 s bound, with no Commit;
the later #271 experiments and FAILs are on **different source identities**
and do not change these five merged-product rows. #249 owns the product's
30-second Exec timer. The combined product has no qualifying 4,097 PASS.
