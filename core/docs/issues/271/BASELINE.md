# Issue 271: combined-source ownership-cost baseline

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective selection](../../../../docs/roadmap/0.1/0.1.7/issue271-root-edge-workload-spec.md)
was committed at `ebbd5606fb5087c2b5934e156279eaca3de158e9` before these
attempts. Both use product seal
`caa60189727a0a57d18a24f4e787d47e13949e50ed6c74d8b0bdf1b2ea5428da`,
locked release binaries, one public Mount → Exec → Commit, the unchanged
one-process/one-fd positional writer, an independently verified master and a
writable byte-copy clone. Each selection ran once. Host/container cache was
uncontrolled, so latency is `INELIGIBLE`; the raw times are observations.
The [raw receipts, stderr, verifier output and hashes](evidence/combined-baseline-v1/)
retain the complete command and source/image/binary/fixture identities.

| Actual WRITEs | Exec / Commit / complete command | Ledger reads / writes | Leaf / branch writes | Child edges added / removed | Custody edges added / removed | Result |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 100 | 0.410623 / 0.035478 / 1.326981 s | 2,905 / 1,511 | 102 / 38 | 83 / 77 | 3,680 / 3,544 | oracle PASS, cleanup PASS, latency INELIGIBLE |
| 512 | 2.774466 / 0.071214 / 4.095102 s | 20,953 / 12,150 | 527 / 450 | 3,896 / 3,864 | 23,139 / 22,589 | oracle PASS, cleanup PASS, latency INELIGIBLE |

The verifier checked exact parent/old/new heads, inventory and full 8,194-byte
file bytes with 100 or 512 changed runs. Its separate wall was 0.013660 /
0.013667 s. The public driver counted 100 / 512 actual FUSE WRITE callbacks
and reported four upstream calls in each row. The last backing snapshot counted
100 / 512 retained private payloads; allocated backing was 450,560 / 2,220,032
bytes, including 40,960 / 122,880 metadata bytes. Store size was 618,496 →
897,024 bytes in each independent clone; history file size stayed 86,016
bytes. Sampled process RSS maxima were 30,097,408 / 33,619,968 bytes, not
phase-local memory peaks. The zero registry `lookup_scans` and 99 / 511
`routine_scans` do not support the historical registry-wide cause.

Within the 512 run, the 128-write checkpoints reported ledger reads
`3,977 → 9,127 → 14,768 → 20,953` and writes
`2,080 → 4,928 → 8,267 → 12,150`. Each successive 128-write block did more
ledger work. The root remained height 1, while branch-child additions reached
170, 892, 2,134 and 3,896. At 100, the root first became height 1 after the
single leaf filled. The final child-addition count of 3,896 from 450 branch
pages means each copied branch named about 8.7 children on average; its old
root cleanup released nearly the same set. The splice also re-encodes a
nearly full touched leaf and updates each Local custody edge. This creates a
large **linear** per-write term: 23,139 additions by 512, beside the growing
branch-child term. The isolated #266 row counted 22,015 / 13,212 at 512;
the combined row has 1,062 fewer reads and writes after #265 changed both
owner finalization and leaf packing. That cross-source difference is not an
isolated attribution, and the branch cost still grows. These counts are
logical 4 KiB ledger API operations, not physical storage traffic or a global
time-complexity proof.

## Treatment choice

The shared page owner charges every edge of a copied leaf or branch at write,
then releases the previous root's edges. The page codec can hold 124 extent
records and 248 child references, but filling pages to those physical maxima
is expensive for a frequently edited tree. The proposed first treatment uses
lower **generic packing targets** for new extent leaves and branches, keeping
the same page format, validation and old-root reader. It bounds the number of
Local and child edges recharged by one copied page and creates another tree
level before a growing root lists every leaf. Its cost is more metadata pages,
branch writes and Commit visits. Count and charge those costs in the one-shot
treatment receipts; do not infer a speed win from the source model. Existing
older full pages remain readable and are repacked only when touched. Any test
that checks physical codec capacity must stay distinct from the chosen packing
target.

No combined 4,097 row has been attempted at this baseline identity: its
isolated #266 25.006537 s FAIL remains the only public gate result so far.
