# Issue 271: 32-record / eight-child packing diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The source `1f009ec20` reuses the independently verified baseline master,
locked-release SDK driver and verifier, writer and cache policy. It rebuilt
only the changed daemon and its image, then ran one public 100 selection and
one public 512 selection with separate independent old/new-head oracles.
Both returned exact FUSE callback counts, verifier PASS and cleanup PASS.
Cache state remained uncontrolled, so raw wall times are `INELIGIBLE` and
cannot establish a speed comparison. The [append-only receipts and raw output](evidence/packing-32-8-v1/)
pin each exact source, binary, image, fixture and command.

| Count | Ledger reads baseline → treatment | Ledger writes baseline → treatment | Child edges added | Local custody edges added | Live metadata pages | Root height | Raw Exec observation |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 2,905 → 3,676 (+771) | 1,511 → 1,867 (+356) | 83 → 474 | 3,680 → 1,205 | 8 → 19 | 1 → 2 | 0.411 → 0.562 s, INELIGIBLE |
| 512 | 20,953 → 27,706 (+6,753) | 12,150 → 15,194 (+3,044) | 3,896 → 5,443 | 23,139 → 6,465 | 21 → 81 | 1 → 3 | 2.774 → 3.346 s, INELIGIBLE |

At 512, extent leaf writes rose 527 → 571 and branch writes 450 → 1,084.
Metadata reads rose 4,408 → 6,264. The treatment did lower custody-edge
count, but every extra copied branch and newly allocated page has its own
authenticated owner-ledger work. Those costs more than consumed the edge
savings. Both count selections are therefore **rejected as an optimization**;
their `INELIGIBLE` receipts and the product commit remain unchanged. The
isolated #266 4,097 `FAIL` is untouched and there is no 4,097 attempt at this
rejected source.

The next source keeps 124-record leaf packing and changes only branch packing
to a generic 32-child target. This leaves the 100/512 separated tree shape
unchanged: their observed final root branches hold three/sixteen leaves. A root that
grows past 32 children gains another level, capping the child-edge list
recharged by a narrow splice without multiplying leaf or Local-edge pages.
The page codec still accepts 248 child references, so old full branches
remain readable. This is a source-derived choice, not an observed win.
The next source needs its own old-root/custody proof, 100/512 count receipts,
and one 4,097 gate only if those and the structural count model make it
meaningful. No worker, cache, deadline or workload rule changes.
