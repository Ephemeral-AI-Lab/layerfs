# Localized read final qualification — issue302

Product/implementation freeze: `7230d62f12348ed0ca2ec6ca0be972ecf3d9210b`.
Local implementation and evidence; no push/PR, Durable/Init/all-seven release
admission, transport or daemon/FUSE claim. Measurement order10,3,1; one sample
per eligible arm at this frozen source/harness identity. Scope is SQLite-only
host product, compared with unchanged Phase4.5 `7edddbdb8e8512627aed0ed42533ef099d802384`.
Candidate uses supported Disposable MEMORY/OFF; reference retains its disclosed
native profile. Locked release binaries, compilation/dependency/observer/corpus
seals and all exact identities are in raw receipts and the [sealed index](checks/localized-final-history1/comparison.json).

## Clocks and separate gates

Product time is complete inner lifecycle. Command wall additionally includes
whole-source cold attestation/process overhead. Verification is a separate12s
command; it is excluded from the speed comparison. The cold helper checks
180,444source files/2,254,684,026B; its roughly17–19s wall is harness work, not a
new20s product slowdown. Both arms use the same declared invalidation/residency
contract; source and per-state database residency after invalidation is zero.

| Stride | Reference product | Candidate product | Complete command reference/candidate | Proof reference/candidate | Result |
| --- | ---: | ---: | --- | --- | --- |
| 10 | 35.250872542s | 32.017440125s | 53.974006375/50.184649042s | 3.302526458/3.952038583s PASS | PASS |
| 3 | 69.865540167s | 64.646124250s | 87.608914959/82.398316709s | 6.763287041/6.587442083s PASS | PASS |
| 1 | 187.341269417s | NOT_RUN | 205.451676833s / NOT_RUN | 12.006836416s TIMEOUT / NOT_RUN | INCOMPLETE admission |

Performance budgets60/170/300s, separate proof12s. Candidate product ratios
0.908273691292393 and0.925293414972188 (9.17%/7.47% less time in these matched
pairs). Integer gates:10*32,017,440,125<=11*35,250,872,542 and
10*64,646,124,250<=11*69,865,540,167. These are overall candidate-versus-reference
results, not an isolated time attribution to one cache/read change or a spread
estimate. Earlier windows/failed scopes are not pooled or selected as best-of.

Storage reference/candidate:52,473,856/49,594,368B atstride10 (ceiling54,278,964B),
65,142,784/62,611,456B atstride3 (ceiling70,427,034B). Stride1 reference closed
86,179,840B stays below92,342,273B; no candidate claim. Every sampled final arm
has cold eligibility and cleanup PASS. Stride1 lacks qualified independent proof
pins, so candidate is explicitly NOT_RUN before build/setup/sample. No time,
storage or cache gate waives that missing proof. Prior190s failures remain;
300s resolves the reference performance bound only.

## Acquired bytes and integrity scope

Versioned proof keeps every state's schema/custody/canonical inventory, roots,
paths/kinds/declared sizes and bounded content anchors. Owner approves canonical
objects/dependencies with separate whole-pack audit; unread pack bytes are not
claimed audited by localized reads. All pooled nodes, including Save's physical
root base, are canonical-authenticated before use. Existing complete-pack audit
APIs remain; exhaustive whole-pack audit is NOT_RUN in this lite scope.

| Stride | Same sampled logical bytes | Reference acquired bytes | Candidate acquired bytes | Acquired ratio |
| --- | ---: | ---: | ---: | ---: |
| 10 | 970,326 | 16,578,959 | 3,355,533 | 0.202397086572203 |
| 3 | 921,174 | 14,474,270 | 3,754,649 | 0.259401614036494 |

These are actual successful physical SQL-pack extraction/BLOB bytes during
proof file-root/content phases, including dependencies and conservatively
included mapping metadata:79.76%/74.06% fewer acquired bytes under identical
content scope. They are not device-byte counts or whole-workflow read
amplification. Returned logical bytes, materialized units, full-pack hash work,
VFS bytes and retained cache sizes remain distinct. Complete encoded groups are
the implemented granularity; raw-record-only acquisition remains future work.

## Resources, reuse and omitted domains

One construction worker. Existing2MiB/4096body entries,512KiB decoded/value
caches,32MiB/4096output bound, singleton exception and chain/work/private/
publication/transaction/visibility bounds are unchanged. No new dependency or
base cache, retries, fallback, schema/format change or third-party patch.

| Stride/arm | Measured driver CPU | Driver lifetime peak RSS |
| --- | ---: | ---: |
| 10/baseline | 27.218647000s | 265,879,552B |
| 10/candidate | 24.324232000s | 262,504,448B |
| 3/baseline | 52.507690000s | 276,365,312B |
| 3/candidate | 47.002437000s | 266,780,672B |
| 1/baseline | 135.107553000s | 343,752,704B |

Host child CPU/RSS are recorded as wait4 lifetime data, not phase-only peaks;
proof wrapper lifetime excludes unreaped grandchildren on timeout and is not
native verifier peak evidence. No cgroup/daemon/FUSE/transport results exist in
this SQLite-only selection; these domains are NOT_RUN, not zero cost.

Prepared corpus and compilation-sealed release executables are reused visibly;
fresh database output is measured, not cloned from a prior mutated sample.
No performance receipt replay, warm start, input prefill or cache-policy change.
All raw receipts/manifests and nonpassing attempts are preserved. The earlier
766ec016f launch failures, TSV binding diagnostic failure, supervision-loss157
INCOMPLETE and corrected diagnostic proofs are separately [indexed](checks/localized-launch-repair1/comparison.json);
none is relabeled as admission.157cause reaches132states before12s, late
namespace traversal dominates. The final157miss is reported as a remaining gate;
no repeated unchanged arm or deadline inflation follows.

## Checks, reproduction and source size

Final Core501tests/95targets; locked all-target Clippy-Dwarnings, fmt,
449-file boundary/23selftests PASS. Actual SQLite counter/native environment
calibration and generated-reference release builds PASS. Save-base negative
fixture is retained before/after. No CI or aggregate pre-push gate ran.

Reproduce each declared arm with a fresh owned output (never replay these
receipts or resample this unchanged identity):

```
python3 core/benchmark/fs-bench-pro/runner.py run --case phase7-sqlite-disposable-history-stride10-v3 --arm baseline --out <fresh-output> --baseline-root <owned-pinned-reference>
python3 core/benchmark/fs-bench-pro/runner.py run --case phase7-sqlite-disposable-history-stride10-v3 --arm candidate --out <fresh-output> --reference-pins <qualified-matched-reference/root-pins.json>
```

Stride3 uses`stride3-v3`;stride1 uses`stride1-v5`, candidate requires qualifying
same-identity reference pins. The final raw output prefix is
`benchmark-results/fs-bench-pro/issue302-history{17,53,157}-localized-final-*1`.
Source/harness changes require a new frozen matched identity, not an old-row
promotion. Current release/all-seven qualification remains incomplete.

Production LOC, exact first-parent snapshots, same counter/scope/exclusions:
cddb16f20 139412->139412(delta+0);766ec016f 139412->139572(delta+160);
7230d62f1 139572->139588(delta+16). Reference stays65417; Core ends74171.
Counter SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Tests/examples/benchmark/tooling/docs are excluded; migration totals are separate.
This evidence-only follow-up has unchanged139588 production LOC(delta0).
