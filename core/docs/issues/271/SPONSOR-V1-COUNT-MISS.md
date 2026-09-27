# Issue 271: first sponsored-page count miss

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective custody trial](../../../../docs/roadmap/0.1/0.1.7/issue271-sponsored-page-treatment-spec.md)
was committed before source `aa82ffc3556e2e72470561d419009acc12350ff5`.
One 100 and one 512 public Mount → one-process/one-fd Exec → explicit Commit
selection reused the sealed master, locked release SDK/verifier and writable
byte-copy clones. Both full old/new-head oracles, exact FUSE WRITE callbacks
(100/512), four upstream calls, Commit and cleanup were **PASS**. Cache was
uncontrolled, so raw latency remains **INELIGIBLE**. The [append-only raw
receipts, stderr, prepared identities, Store clones and hashes](evidence/sponsor-v1/)
pin the complete commands, product/harness/image/binary/fixture identities.

| WRITEs | Baseline ledger reads / writes | Sponsored ledger reads / writes | Charged child / Local / sponsor edges added | Root height / final children | Result |
| ---: | ---: | ---: | ---: | ---: | --- |
| 100 | 2,905 / 1,511 | **2,916 / 1,446** (+11 / −65) | 83 / 1,882 / 50 | 1 / 3 | count predicate FAIL; functional PASS; latency INELIGIBLE |
| 512 | 20,953 / 12,150 | **21,030 / 10,988** (+77 / −1,162) | 2,223 / 11,809 / 457 | 1 / 16 | count predicate FAIL; functional PASS; latency INELIGIBLE |

The 512 within-run 128-WRITE ledger checkpoints were 3,949/1,964,
9,370/4,701, 15,051/7,696 and 21,030/10,988 reads/writes. The four blocks
thus cost 3,949, 5,421, 5,681 and 5,979 reads, and 1,964, 2,737, 2,995 and
3,292 writes. Each row still charged its changed path; the final branch named
the same 3/16 children as the combined baseline. The new owner counter
separates charged Local/child edges from the encoded edge-field counts.
Sponsor edges removed were 48/447 at the final WRITE checkpoint; later clean
close completed the remaining positive custody cleanup.

Exec / Commit / complete-command observations were 0.395905 / 0.033147 /
1.437821 s and 2.550237 / 0.071235 / 3.882959 s. Verifier wall was
0.016691 / 0.014477 s. The Store clone was 618,496 → 897,024 bytes in both
rows and history remained 86,016 bytes. At the last WRITE, backing allocation
was 454,656 / 2,252,800 bytes including 45,056 / 155,648 metadata bytes.
Shared-process sampled RSS maxima were 31,801,344 / 29,868,032 bytes, not
phase-local peaks. These values describe only the exact cache-INELIGIBLE rows.

The read-count miss has a source-level cause: `write_raw_page` reads the old
owner to decide whether it may sponsor, then `sponsored_edges` calls
`Arena::load_raw`, which reads that owner again before loading the authenticated
body. There were 50/457 successful sponsors; removing one redundant owner
ledger read per successful sponsor would more than cover the observed +11/+77
read-count misses. This is a count inference, not an unchanged-arm rerun or a
latency prediction. A new source and prospective identity are needed to test
it. The 4,097 gate and 100-write sibling patterns were **NOT_RUN** at this
source because the declared dual read/write predicate failed.
