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
