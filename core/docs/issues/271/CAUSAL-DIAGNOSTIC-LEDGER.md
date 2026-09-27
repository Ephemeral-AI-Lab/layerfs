# Issue 271 causal diagnostic ledger

This ledger is append-only. The [research and reform proposal](../../../../docs/roadmap/0.1/0.1.7/issue271-causal-telemetry-and-reform-proposal.md)
names the cost model. Rows here retain their original identities and status;
no diagnostic elapsed time is a release speed PASS while cache state is
uncontrolled.

## C1 — 4,097 WRITE cause diagnostic, 2026-09-27

**Status: `INCOMPLETE` cause evidence; functional `PASS`; latency
`INELIGIBLE`; admission false.** This is the single attempt at scenario
`issue271-separated-4097-cause-v1`, preregistered in the
[cause specification](../../../../docs/roadmap/0.1/0.1.7/issue271-causal-diagnostic-spec.md).
The raw [receipt](evidence/causal-v1/run/receipt.json),
[stderr](evidence/causal-v1/run/driver.stderr),
[full verifier result](evidence/causal-v1/run/verifier.stdout),
[append-only hashes](evidence/causal-v1/run/SHA256SUMS), and
[public preparation identity](evidence/causal-v1/prepared-public.json) are
retained. The separate, earlier [25 s gate FAIL](FOURHOP-GATE-FAIL.md) is not
changed or re-labelled.

| Identity and contract | Value |
| --- | --- |
| Source commit/tree | `4bedea97bf8b612f4bf2ff7ff238ad110fb53113` / `fa44e955cce857c16adad74ce959fc7be1aa10c6` |
| Product/harness seal | `54fae2b1177cc849b8f8f647104c07beb91e32ba3014e171de17efd5356e98fe` / `a166dde532d43e7967713aeb46305f86a7779c4fb2e89deb60e6d10c57b0ac80` |
| Image | `sha256:4928d0a48872cec7794777e6fc51b0151b19dba7a02e661d39486fa090396087` |
| Build/reuse | Locked release SDK, verifier and daemon rebuilt; closed verified master reused as an independent writable `shutil.copyfile` byte copy; one construction worker. Binary, spec, manifest and master hashes are in the prepared identity and receipt. |
| Reproduction | `python3 core/benchmark/fs-bench-pro/separated_writes.py run --prepared benchmark-results/fs-bench-pro/issue271/fourhop-cause-prepared-v1/prepared.json --selection diagnostic4097cause --output benchmark-results/fs-bench-pro/issue271/fourhop-cause-v1` (the command was used once; do not rerun this identity). |
| Limits | 60 s complete command for this cause diagnostic only; 30 s product Exec deadline; 9 s separate verifier. The public gate remains 25 s. |

One SDK Mount → one generic Exec → explicit Commit produced **4,097 FUSE
WRITE callbacks** and **four upstream Service calls**, with no refusals.
The full old/new-head and byte oracle passed in **0.013755 s**; cleanup passed
with clean daemon close. Raw SDK Exec was **26.219312 s**, Commit **0.523337
s**, complete command **31.047519 s** and cleanup **3.228656 s**. The complete
command is below the 60 s *diagnostic* limit and above the ordinary 25 s
gate. Cache state was uncontrolled, so these values only locate work within
this attempt. They cannot be compared numerically with the older diagnostic
as a speed result.

At accepted WRITE 4,096, the existing parent FUSE timers and new cumulative
children reported:

| Exec cost center | Cumulative seconds / calls | Scope |
| --- | ---: | --- |
| `own_payload` parent | 11.442783 s / 4,096 | Includes maintenance and acquisition. |
| Acquisition metadata maintenance | 10.883235 s / 4,096 | Child of acquisition maintenance (10.886953 s). |
| Actual payload acquire | 0.554049 s / 4,096 | Separate from acquisition maintenance. |
| `write_file` parent | 14.618860 s / 4,096 | Includes publication and checked notification. |
| Publication core | 14.602965 s / 4,096 | Post-maintenance root/candidate work. |
| Publication maintenance | 0.001758 s / 4,096 | Metadata/payload children overlap it. |
| Checked notifier | 0.008451 s / 4,096 | Inside mounted publication; one checked delivery per WRITE. |

The two large non-overlapping child cost centers, acquisition metadata
maintenance and publication core, total **25.486200 s**, about **97.2%** of
raw Exec at WRITE 4,096. Ledger counters reported **220,377 4 KiB reads** and
**119,492 4 KiB writes**; their cache state is unknown. Other overlapping
cost centers were **274,418 ledger-file opens/validation calls** in **0.503335
s**, **15,419 metadata page creations** in **1.975102 s**, and 10,174 sponsor
attempts (8,143 accepted, 2,031 depth-four fallbacks). These source-owned
timers are nested within the phase parents and must not be added to them.
The final WRITE 4,097 has no checkpoint, so these counts stop at 4,096.

The host Commit SaveFile was **0.457514 s**, including `service.pre_save_input`
**0.437165 s**. Parsing/spooling reported 8,194 descriptors, 4,097 edits,
16,388 eight-byte spool writes, 131,104 spool bytes and **0.037087 s** in those
write calls. The daemon source cause line collided with a concurrent LFT1
line on stderr. Its counts/times cannot satisfy the preregistered complete
cause record; the parser returned `INCOMPLETE`. The raw tail contains partial
clues about Local reads, but is not promoted to validated cause evidence.
The [v2 specification](../../../../docs/roadmap/0.1/0.1.7/issue271-causal-diagnostic-v2-spec.md)
predeclares a separate atomic-log diagnostic at a new source identity.

**Non-passing lines and checks:** cause `INCOMPLETE` due malformed/interleaved
`LFS_COMMIT_SOURCE_CAUSE`; cache/performance `INELIGIBLE`; 25 s gate remains
FAIL. Workspace `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace
--locked` stopped at two untouched `layerfs-content/tests/filesystem_ordering.rs`
ceiling assertions; the separate tests for the four changed packages passed.
Warning-denying Core Clippy, Core examples, product boundary scan and its nine
self-tests, harness self-check and changed-file rustfmt passed. Workspace fmt
still reports the untouched `runtime/state.rs:267` layout. No other performance
arm was sampled for this source.

## C2 — atomic Commit cause diagnostic, 2026-09-27

**Status: causal `PASS`, functional `PASS`, latency/row `INELIGIBLE`, admission
false.** This is the single attempt at the separately preregistered
[`issue271-separated-4097-cause-v2`](../../../../docs/roadmap/0.1/0.1.7/issue271-causal-diagnostic-v2-spec.md)
scenario. It does not replace C1's `INCOMPLETE` record. The [raw
receipt](evidence/causal-v2/run/receipt.json), [stderr](evidence/causal-v2/run/driver.stderr),
[verifier](evidence/causal-v2/run/verifier.stdout), [run hashes](evidence/causal-v2/run/SHA256SUMS),
[public preparation identity](evidence/causal-v2/prepared-public.json) and
[evidence manifest](evidence/causal-v2/EVIDENCE.json) are retained.

| Identity and contract | Value |
| --- | --- |
| Source commit/tree | `70863270093d0726e0c4e1912ae2f7c405268483` / `01c3b26ab7e01337f839d4b05f8706fc630ece18` |
| Product/harness seal | `c5b8ca57a93e6716db80e1702c4b0b648b32144a428277be9fa00314f25154c7` / `af720b3b48125eb352acee79246cd049cf25a25713e144e590303b0fb1779060` |
| Image | `sha256:755bd5a2650c2f6442c1fce88eb3c6ea3af0a4a81ee0f3cea55ebeaa62674090` |
| Build/reuse | Locked release SDK, verifier and daemon rebuilt; the closed master was validated with the new verifier and given an independent writable byte-copy clone. One construction worker; binary, workload, manifest, compilation and spec hashes are in the prepared identity and receipt. |
| Reproduction | `python3 core/benchmark/fs-bench-pro/separated_writes.py run --prepared benchmark-results/fs-bench-pro/issue271/fourhop-cause-v2-prepared-v1/prepared.json --selection diagnostic4097causev2 --output benchmark-results/fs-bench-pro/issue271/fourhop-cause-v2-run-v1` (used once; do not rerun this identity). |
| Limits | 60 s complete cause command, 30 s product Exec deadline, 9 s separate verifier; the unchanged public gate is 25 s. |

The one public Mount → Exec → Commit route produced exactly **4,097 FUSE
WRITE callbacks**, **four upstream Service calls**, 8,194 final extents,
4,097 separated replacements and a committed new head. The independent full
old/new-head and byte verifier passed in **0.015106 s**; cleanup and daemon
close passed. Raw SDK Exec was **26.496683 s**, Commit **0.449202 s**,
complete command **31.390640 s** and cleanup **3.355971 s**. Those raw
durations describe this attempt only: its ordinary host/container cache was
uncontrolled, so they establish neither a speedup over C1 nor a release
latency PASS. The 25 s gate remains the earlier FAIL.

At accepted WRITE 4,096, acquisition metadata maintenance took **10.912227
s** of the **11.463375 s** `own_payload` parent, while actual payload
acquisition took **0.545151 s**. Publication core took **14.853132 s** of
the **14.870282 s** `write_file` parent; checked notifier delivery took
**0.009056 s**. The two large, non-overlapping child buckets total **25.765359
s**, or **97.2%** of Exec through that checkpoint. They are source-owned
diagnostic durations, not cache-qualified latency comparisons.

| Accepted WRITE block | Metadata maintenance s | Publication core s | Ledger reads | Ledger writes | Ledger-file calls | Page creates |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1–512 | 1.003 | 1.360 | 19,426 | 9,807 | 24,589 | 1,489 |
| 513–1,024 | 1.193 | 1.560 | 22,356 | 12,071 | 27,951 | 1,552 |
| 1,025–1,536 | 1.422 | 1.845 | 28,173 | 15,190 | 34,975 | 2,051 |
| 1,537–2,048 | 1.404 | 1.940 | 29,575 | 16,653 | 36,263 | 2,066 |
| 2,049–2,560 | 1.422 | 1.973 | 29,518 | 15,870 | 36,879 | 2,065 |
| 2,561–3,072 | 1.451 | 2.011 | 30,285 | 16,500 | 37,746 | 2,065 |
| 3,073–3,584 | 1.468 | 2.066 | 30,513 | 16,740 | 37,962 | 2,065 |
| 3,585–4,096 | 1.550 | 2.098 | 30,531 | 16,661 | 38,053 | 2,066 |

The 4,096-WRITE totals are **220,377 ledger reads, 119,492 writes, 274,418
ledger-file calls and 15,419 new metadata pages**. Ledger-file open/validation
time was **0.520671 s** and page creation time **1.955163 s**, both nested
within the large phase buckets and too small alone to explain a 2× Exec
target. There were 10,174 sponsor attempts, 8,143 accepted and 2,031
depth-four fallbacks. This row's block counters rise around the height-two
transition, then approach a stable per-block range; there is no observed
sustained quadratic multiplier over 512–4,096.

**Commit attribution.** Host SaveFile pre-input was **0.372094 s**. Its
parser processed exactly 8,194 descriptors and 4,097 edits and issued
16,388 eight-byte spool writes (131,104 bytes) in **0.028451 s**. The daemon
made 13 descriptor-source calls for 196,656 bytes in **0.033197 s**, then
two replacement-source calls for 4,097 bytes in **0.307810 s**. The latter
includes 8,195 ordered cursor-next calls (**0.030493 s**), 4,097 Local
reader creations (**0.000317 s**), 4,097 one-byte Local reads (**0.272190
s**), 4,097 payload opens (**0.009589 s**) and 4,097 authenticated aligned
4 KiB reads, **16,781,312 physical bytes in 0.254309 s**. These are
overlapping parent/child durations in daemon and host clock domains; the
counts are actual source calls, not extra Service RPCs or observed Bridge
frames. Bridge frame count remains unavailable. The host pre-input span
contains transport wait and parsing as well as its 0.028451 s spool writes;
it cannot be assigned to spool alone.

**Non-passing lines and checks:** cache/performance/row `INELIGIBLE`; no
release admission, and the prior 25 s gate FAIL remains. Full Core workspace
tests again stopped at two untouched `layerfs-content/tests/filesystem_ordering.rs`
ceiling assertions; tests for the four changed packages, Core examples,
warning-denying Clippy, boundary guard and nine guard self-tests, harness
self-check and changed-file formatting passed. Workspace fmt still reports
the untouched `runtime/state.rs:267` layout. No other performance arm was
sampled at this source.
