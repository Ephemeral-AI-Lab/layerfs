# Issue 261: authenticated-owner reuse in old-root cleanup

> **Status:** One labelled public diagnostic per three-pattern case at source
> `ad9f91bf48187588f4735cccff1efa8e47475d52`. Functional and independent
> oracle PASS; cleanup PASS by SDK Unmount/Delete and absence of a retained
> shutdown report. All latency rows are cache-`INELIGIBLE`, with no numerical
> speed admission. Raw data and the retained evidence-packaging failure are in
> [`patterns-100-cleanup-v1`](evidence/patterns-100-cleanup-v1/).

The [prospective treatment](../../../../docs/roadmap/0.1/0.1.7/issue261-ownership-io-treatment.md)
was committed at `e2d8cb2de` before the source commit `52dc157d4`. That
product commit changes `cleanup_step` to pass its already authenticated owner
to the raw metadata page load, which still revalidates ledger pathname identity
and page file identity/checksum. A later phase or retry reads the owner again.
The runner-lock commit `ad9f91bf4` changes benchmark tooling only; all three
changed-source attempts use that final clean source identity. No payload
format, workload, deadline, worker count, cache policy or public route changed.

Preparation reused the prior closed, verified 10 MiB Store/history master by
identity, and the unchanged sealed static writer by SHA-256. The host SDK
driver, independent verifier and aarch64 daemon were rebuilt with locked Cargo
release against the changed product; the image was rebuilt. The rebuilt
verifier reproved the old master before any case. Each case copied Store and
history to its own independent writable byte copy. The three attempts ran in
append/dispersed/repeated order under the worktree-local lock. The old rows
at source `7361312e6` were not rerun.

## Count result

| Case | Baseline ledger reads / writes | Changed reads / writes | Read reduction | Cumulative reduction at writes 25/50/75/100 |
| --- | ---: | ---: | ---: | --- |
| True EOF append | 2,941 / 1,470 | 2,647 / 1,470 | **294** | 69 / 144 / 219 / 294 |
| Dispersed 10 MiB edit | 3,696 / 1,929 | 3,328 / 1,929 | **368** | 69 / 144 / 243 / 368 |
| Repeated one-place edit | 3,451 / 1,489 | 3,059 / 1,489 | **392** | 92 / 192 / 292 / 392 |

These are 1,204,224 / 1,507,328 / 1,605,632 fewer bytes read through the
4 KiB ledger API. They are not measured physical-device bytes. The source
removes one read for each edge-bearing page cleanup that uses the newly loaded
owner; the receipts do not export that page-kind count separately. Unchanged
ledger writes, metadata page reads (591 / 703 / 689), final extents and
changed runs (101/1, 201/100, 3/1), live metadata pages (4/37/4), payload
counts (100/100/2), final private allocated bytes (843,776 / 983,040 /
36,864), and routine record scans (99/99/197) agree with a read-only cleanup
change. All cases still acquired 100 one-byte, 8,192-byte private payloads
during Exec. The historical registry-wide scan is absent.

| Case | Changed Exec ms | Changed Commit ms | Changed complete command ms | Independent verifier ms | Functional / cleanup / latency |
| --- | ---: | ---: | ---: | ---: | --- |
| Append | 353.717 | 31.782 | 1,967.751 | 94.299 | PASS / PASS / INELIGIBLE |
| Dispersed | 475.587 | 72.029 | 1,507.095 | 96.903 | PASS / PASS / INELIGIBLE |
| Repeated | 422.836 | 25.746 | 1,316.366 | 96.602 | PASS / PASS / INELIGIBLE |

The raw Exec values are below the previous 433.382 / 518.925 / 467.413 ms,
but both source identities ran under uncontrolled ordinary host/container
cache. The append complete-command wall increased. Neither difference is a
qualified speedup or regression. The within-run count reduction, same ledger
writes and unchanged shape are the evidence for the specific mechanism.

Each changed-source receipt records exactly 100 actual FUSE WRITE callbacks,
one LOOKUP, three GETATTRs, one OPEN, zero READs and four upstream Service
calls. Commit lowered the final extents, creating 1/100/1 changed runs and
100/100/1 replacement bytes. The independent release verifier checked exact
full old/new file bytes, old-head immutability, new-head parent and path
inventory. All three rows have one attempt and an independent oracle PASS;
each complete command met 15 s and each verifier met 9 s. FLUSH/RELEASE and
transport-frame/notification acknowledgement counts remain unexported.

| Case | Driver cleanup ms | Consumer accounted bytes | Host operation sampled max RSS bytes | Store bytes before → after | History bytes before → after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Append | 575.861 | 1,748,116 | 31,637,504 | 626,688 → 913,408 | 86,016 → 86,016 |
| Dispersed | 569.978 | 1,748,148 | 35,536,896 | 626,688 → 917,504 | 86,016 → 86,016 |
| Repeated | 496.650 | 1,717,212 | 35,045,376 | 626,688 → 913,408 | 86,016 → 86,016 |

The RSS field is sampled host-process operation data, not a reset cgroup
lifetime peak or a phase-local anonymous-memory bound. Store file-size growth
does not count Store requests or physical writes. Cleanup includes SDK
Status/Unmount and sandbox removal; a positive `workspace closed` daemon line
was not present in the captured logs, so it is not claimed as a separate
assertion.

The first evidence-copy script stopped after copying the raw files because it
looked for a new master inside the reuse-preparation output. Reuse correctly
kept the old sealed master. No workload or sample was repeated; the
`copy-fail.json` and corrected index in the new evidence folder retain that
packaging failure. Raw receipts, build logs, manifests, verifier logs and
SHA-256s are copied; Store/history SQLite files and binaries remain in the
ignored result directories with their hashes in `EVIDENCE.json`. The
`prepared-public.json` copy redacts the cursor key.

## Verification and remaining work

The three public Exec/Commit cases, their independent oracles, the rebuilt
old-master proof, the runner self-check and Python compile, the Core product
boundary guard, its nine self-tests, and focused locked-release Workspace
Clippy `-D warnings` passed. `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all --check` still fails on an unchanged formatting item in
`runtime/state.rs:267`; the changed files produced no fmt diff. The first
`cargo fmt` invocation lacked `--all` for this virtual workspace and failed
before formatting. A native metadata failure-injection selection was
`NOT_RUN`: its existing route requires a separately prepared 64 MiB fixture
and has a 60 s command bound, outside this issue's under-30 s focused-test
rule. The public tests prove successful reclaim, exact heads and ordinary
custody, but not that narrower corrupt-ledger failure path; do not claim it.

The independent [scaling audit](OPTIMIZATION-AUDIT.md) ranks the remaining
costs. The one-page tiny-payload format is a separate proposal because #261
bars speculative packed-page formats. Extent page-kind/occupancy counters are
needed before changing the B+ splice, and the earlier 512-write `EBUSY`
branch is not identified. The #248 4,097 gate remains `NOT_RUN` at this
identity: the earlier 512 failure is unlocalized and #249 still owns the
30-second product Exec timer. A new gate attempt is meaningful after those
specific risks are resolved or the owner freezes it as a fail-likely diagnostic;
no timeout/workload/cache adjustment is authorized.

Production LOC, first-parent staged-tree method `tools/production_loc.py
--root . --detail`: prospective spec `e2d8cb2de` combined 123,926 → 123,926
(delta +0), Core 58,509 → 58,509; product/audit/runner commit `52dc157d4`
combined 123,926 → 123,949 (delta +23), Core 58,509 → 58,532; benchmark-lock
commit `ad9f91bf4` combined 123,949 → 123,949 (delta +0), Core 58,532 →
58,532. Reference stayed 65,417 and adapter stayed 0 in every commit.
