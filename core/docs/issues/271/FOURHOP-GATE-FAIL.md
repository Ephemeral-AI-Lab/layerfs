# Issue 271: four-hop custody source and retained 4,097 failure

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective four-hop custody treatment](../../../../docs/roadmap/0.1/0.1.7/issue271-four-hop-custody-spec.md)
preceded frozen source `933b3457c916519f458137e4c4f19662c8e9228e`.
It retains authenticated page bodies, four charged sponsor hops at most,
full ownership on the fifth copy, exact old-root/G1/G2 custody and bounded
cleanup. The focused native test passed its depth, abandoned-candidate and
zero-allocation cleanup assertions. The [raw receipts, separate verifier,
prepared identities, logs, Store clones and hashes](evidence/fourhop-v1/)
retain every selected attempt. The same locked release SDK/verifier, generic
one-process/one-fd writer, sealed master, writable byte-copy clones,
one-worker and uncontrolled-cache contract applied. Raw latency is
**INELIGIBLE** in all rows.

| Public selection | Actual WRITEs | 4 KiB ledger reads / writes | Exec / Commit / complete command | Full oracle | Cleanup | Row |
| --- | ---: | ---: | ---: | --- | --- | --- |
| separated 100 | 100 | 2,833 / 1,404 | 0.412395 / 0.031369 / 1.333013 s | PASS | PASS | INELIGIBLE |
| separated 512 | 512 | 19,426 / 9,807 | 2.366790 / 0.074215 / 3.722882 s | PASS | PASS | INELIGIBLE |
| 10 MiB append 100 | 100 | 2,464 / 1,217 | 0.350983 / 0.024887 / 2.005786 s | PASS | PASS | INELIGIBLE |
| 10 MiB dispersed 100 | 100 | 2,824 / 1,403 | 0.362646 / 0.034576 / 1.331919 s | PASS | PASS | INELIGIBLE |
| 10 MiB repeated 100 | 100 | 2,663 / 1,289 | 0.379498 / 0.021397 / 1.221577 s | PASS | PASS | INELIGIBLE |
| separated 4,097 gate | unavailable | unavailable | unavailable / unavailable / **25.006973 s** | NOT_RUN | FAIL | **FAIL** |

The separated rows satisfy both frozen count comparisons: baseline
2,905/1,511 → **2,833/1,404** at 100 (−72/−107), and
20,953/12,150 → **19,426/9,807** at 512 (−1,527/−2,343).
Against the one-hop source they save another 33/42 and 1,147/1,181
reads/writes. At 512, cumulative 128-WRITE ledger checkpoints were
3,819/1,911, 8,871/4,401, 14,074/7,042 and 19,426/9,807.
The successive blocks still rose: 3,819, 5,052, 5,203 and 5,352 reads;
1,911, 2,490, 2,641 and 2,765 writes. Charged child, Local and sponsor
additions were 1,225, 5,030 and 730 by write 512, with 1,207, 4,158 and
695 acknowledged removals at that checkpoint. The root remained height 1
with 16 children. These are API counts, not physical device bytes or a global
time-complexity proof.

The separated oracles checked 100/512 changed runs and full old/new bytes.
Their new heads were
`1294c31bb6cae0e979a9b0f4955f1123b381b9bf28c20d4ba9615e5d024486933e`
and `12d8f197b68cd568d1d1ceb3ab45bb12fc030bacefda698d31e5734c68feb27461`;
both used old head
`12ef44464e764a34399e5be721a9d14ab714813471949c06bf20413d2ca6f9a098`.
The 10 MiB sibling receipts pin their own exact parent and new heads.
Each passing row had four upstream calls, clean unmount and sandbox deletion.
Independent verifier walls were 0.013364/0.013251 s for separated 100/512,
and 0.103435/0.098964/0.098847 s for append/dispersed/repeated.
The separated Store clones were 618,496 → 897,024 bytes; history remained
86,016 bytes. At write 512, backing allocation was 2,359,296 bytes,
including 262,144 metadata bytes and 54 live metadata pages. Its sampled
shared-process RSS maximum was 29,818,880 bytes, not a phase-local peak.

The **one new-source 4,097 gate** expired at the unchanged 25 s
complete-command bound before a driver receipt, callback count, Commit,
verifier or clean product close. The Store/history clones remained
618,496/86,016 bytes. The owned daemon later logged a successful
`WorkspaceExec` of **26.117643 s** (`daemon.exec_output` 26.107585 s),
but this postmortem result cannot promote the timed-out host row. There is
no exact host-observed callback count or new Commit root. The daemon then
reported `sandbox shutdown retained: Busy`; deliberate stop ended exit 137.
A read-only postmortem saw 18,592 KiB and 4,582 files in the private volume,
and the sampled daemon RSS reached 42,631,168 bytes. Those are broader or
post-timeout observations, not phase-local memory or clean-close proof.
The owned container and volume were removed after preserving the logs;
forced removal remains **cleanup FAIL**.

The count reduction is real and correctness passed at the selected smaller
sizes. The public 4,097 target remains **FAIL**, and this source supplies no
cache-qualified speed or gate-memory PASS. The one-hop 4,097 FAIL and its
separate 2,048 diagnostic also retain their original statuses. Further work
needs a distinct mechanism and source identity, not a larger timeout, more
workers, a warmer cache or an unchanged-arm rerun.

A later [user-requested 60 s count diagnostic](FOURHOP-4097-EXTENDED-DIAGNOSTIC.md)
finished and exposed per-512-WRITE growth. It is a separate instrumented
selection and does not alter this gate's FAIL status or 25 s limit.
