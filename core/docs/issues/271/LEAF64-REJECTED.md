# Issue 271: 64-record leaf packing rejected

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective treatment](../../../../docs/roadmap/0.1/0.1.7/issue271-moderate-leaf-treatment-spec.md)
preceded product source `e255e688bdac46f57c9ccbadb859869ea1fedf4e`.
That source packs newly written extent leaves to 64 records, keeps 32-child
branches and the full old-page codec, and changes no ownership rule. One
`diagnostic100v3` and one `fuse512` selection used the same sealed prepared
master, independent writable byte-copy clones, locked release SDK/verifier,
one-process/one-fd positional writer, one public Mount → Exec → Commit, and
separate full old/new-head oracle as the [combined baseline](BASELINE.md).
The [raw receipts, logs, Store/history clones, prepared identities and hashes](evidence/leaf64-v1/)
are append-only. Each selection ran once. Cache was uncontrolled in both
sources, so both raw wall observations remain latency **INELIGIBLE**.

| Actual WRITEs | Ledger reads baseline → trial | Ledger writes baseline → trial | Child edges added baseline → trial | Local edges added baseline → trial | Leaf / branch writes baseline → trial | Live metadata pages baseline → trial |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 2,905 → 3,203 (+298) | 1,511 → 1,629 (+118) | 83 → 244 | 3,680 → 2,212 | 102 / 38 → 105 / 68 | 8 → 11 |
| 512 | 20,953 → 21,570 (+617) | 12,150 → 12,587 (+437) | 3,896 → 7,710 | 23,139 → 12,408 | 527 / 450 → 542 / 480 | 21 → 36 |

The 512 trial's cumulative 128-WRITE ledger checkpoints were 4,263/2,186,
9,476/5,097, 15,267/8,586 and 21,570/12,587 reads/writes. Each successive
128-WRITE block cost 4,263, 5,213, 5,791 and 6,303 reads, and 2,186, 2,911,
3,489 and 4,001 writes. At its last WRITE the root was still height 1 and
named **31** children, versus the baseline's **16**. The 64-record target
reduced Local edge additions but created more leaves, copied more branch
children, and increased both ledger I/O totals. These are 4 KiB ledger API
operations, not physical storage bytes or a cache-qualified time exponent.
The trial fails the prospectively declared count condition in **both** rows.

Both rows returned exact 100/512 FUSE WRITE callbacks, four upstream calls,
explicit Commit, full old/new-head and byte oracle **PASS**, and cleanup
**PASS**. The trial's new heads were
`1224f72a3d0ab9aa5c4be0db6a9902fa1289ab1b1e032cb2fa70756e849a49ca8d`
and `12c8cf260fc73f0cb8a2bb8a57b4be176c67a4c1e6d77cac81e122dd3a74e3688f`;
both began at old head
`12ef44464e764a34399e5be721a9d14ab714813471949c06bf20413d2ca6f9a098`.
Exec / Commit / complete-command observations were 0.454161 / 0.035318 /
1.453932 s and 2.594780 / 0.086366 / 3.919574 s. Independent verifier
walls were 0.019620 / 0.015419 s. No speed claim follows from these
uncontrolled-cache observations.

At the last WRITE, backing allocated 462,848 / 2,281,472 bytes, including
53,248 / 184,320 metadata bytes; the Store clone was 618,496 → 897,024 bytes
in each case and history stayed 86,016 bytes. The sampled shared-process RSS
maxima were 30,621,696 / 33,112,064 bytes, **not phase-local peaks**.
`LFS_PIECE_LOWER` independently reported 100/512 changed runs. Both receipts
record successful unmount and sandbox deletion, no admission stop, and no
unexplained write refusal. The focused Linux extent suite passed 37/37 at
this source, including older 124-record leaves and 248-child branches.

The 4,097 gate and 100-write append/dispersed/repeated siblings are
**NOT_RUN** at this rejected source: the declared count predicate failed before
the gate decision. The retained [32/8 count regression](PACKING-32-8-REJECTED.md),
[32-child 4,097 FAIL](BRANCH32-GATE-FAIL.md) and
[2,048 FAIL](BRANCH32-2048-FAIL.md) keep their original status. More packing
tuning cannot establish the requested touched-path ownership cost; a later
source needs a separate prospective ownership change and its own custody proof.

Reproduction commands, with fresh output paths and the retained prepared
master, are `python3 core/benchmark/fs-bench-pro/separated_writes.py prepare
--reuse-prepared <sealed-prepared.json> --reuse-selection diagnostic100v3
--output <fresh-prepared-100>` followed by `run --prepared
<fresh-prepared-100>/prepared.json --selection diagnostic100v3 --output
<fresh-100>`, then the same `prepare`/`run` pair with `fuse512`. The exact
commands, source/product/harness seals, image and binary hashes, fixture,
writer and oracle identities are in the prepared files and row receipts.
