# #231: release-only four-tier SDK Init, 2026-09-24

> **Status: three functional PASS rows, one verifier TIMEOUT; #231 remains
> open.** Owner direction replaced the active debug SDK Init benchmark with
> locked release binaries before this cohort. These are four new one-shot
> receipts under the [release-only contract](SDK-RELEASE-FOUR-TIER-20260924.md).
> Historical debug receipts remain unchanged.
> The later owner-approved [lite verifier result](SDK-VERIFIER-LITE-RESULTS-20260924.md)
> has a narrower content-proof scope and its own release-v5-lite receipts.

## Identity and operation

The clean measured source was `48282bfb877c38fdd858210c8fa03deaf0ea8d3f`
(tree `3a19b8b1c6a728b669e1a6b845165a5b4ff66436`). Product seal:
`78430b1b2cbcbda185e5995393cd519dd881b3a1e94cb93dd60c217b31dc6529`;
harness seal: `690147eeaff539dc4a438fa8112555453ce9dc75ba581213e6411c58080ac54f`;
Cargo lock SHA-256: `72919f072f26a55ed8e2d3284536fe11dd0e5149fd2f9058ed96a36df2ad1a06`.
The runner's locked `cargo ... build --release` took 6.264781416 s on the
first case, then reused exact binary hashes. Release driver/verifier SHA-256:
`e6b51b46610efc507bf77cab3f33ff216a62729a81c59cf8594ed5b3509fa63f`
and `48d9f812bbf117cffc70ef18155b5a54e13c38eb7d817128d900fc4d6df109a0`.
No debug executable or old unmarked cache supplied a row.

Each case launched one public `Client::init_project` driver on a fresh
Store/History. The seed-1 `core-sdk-init-fixture-v2` prepared masters were
identity-checked and reused outside timing; none was regenerated or used
as a previous sample's mutated Store. The timer covers source scan/read,
construction, Saves and C5 publication. The independent verifier runs after
the timed child and reopens Store/History. There was exactly one sample per
case and no timeout, worker, workload or cache-policy change after a miss.

## One-shot results

| Case | Public SDK Init | Complete command | Independent full verifier | Cleanup | Receipt status |
| --- | ---: | ---: | --- | --- | --- |
| 100 files / 5 MB | 0.031915375 s | 1.402081000 s, PASS | 0.040957625 s, PASS; 102 paths / 5 MB | PASS | Functional PASS; performance INELIGIBLE |
| 1,000 files / 20 MB | 0.105516292 s | 0.117562708 s, PASS | 0.150731083 s, PASS; 1,011 paths / 20 MB | PASS | Functional PASS; performance INELIGIBLE |
| 10,000 files / 300 MB | 1.242754125 s | 1.254954417 s, PASS | 1.449427708 s, PASS; 10,101 paths / 300 MB | PASS | Functional PASS; performance INELIGIBLE |
| 100,000 files / 500 MB | 5.467732792 s | 5.482059125 s, PASS | 5.006864083 s, TIMEOUT; no complete readback | PASS | FAIL |

The 100k public call returned a typed root and clean process exit; its
separate verifier reached the unchanged five-second limit. The timeout is
not a content mismatch, but its partial progress is not a full oracle PASS.
An earlier release diagnostic of the same Core product implementation
completed a full 100k readback in **9.179 s** under a separately declared
600-second diagnostic watchdog
([receipt](../../../issues/237/evidence/c3-inode-fusion-20260924/candidate/receipt.json)).
That receipt cannot be promoted into this source/contract's five-second
proof row. No second verifier or performance attempt was taken here.

These operation times are numerically consistent with the earlier
release-profile observations: the [same-source profile check](../../../issues/237/sdk-merge-check-20260924.md)
recorded 1.289 s for 10k release versus 6.140 s debug, and the retained
direct-inode release diagnostic recorded 5.410 s for 100k. The current
Core product files under `core/crates/` are unchanged from that direct-inode
source. This resolves the apparent 5× slowdown from comparing debug with
release; it is **not** a qualified cross-run speedup or regression test.
The v0.1.6 route, fixture/cache state and binary identity also differ.

All four receipts still declare `source-cache-uncontrolled-v1` and
`admission_eligible=false`; no whole-input cold residency proof or numeric
SDK latency target was frozen for this selection. The raw times therefore
cannot become performance PASS, even where functional verification passed.
The 100k 2.7-second cold target remains untested by this cohort.

## Custody, checks and decision

Raw Store/History and manifests are retained under
`benchmark-results/fs-bench-pro/issue231-release-four-tier-48282bfb8-{100,1000,10000,100000}/`
in this worktree. Each read-only `runner.py verify --run` returned PASS for
**retained-evidence integrity**, not the 100k oracle. The small
[curated receipts](evidence/sdk-release-four-tier-20260924/) and
[SHA-256 inventory](evidence/sdk-release-four-tier-20260924/SHA256SUMS.json)
are committed beside this report. The four optional structured-text variants
were not selected.

Focused benchmark Python tests (8), `py_compile` and `git diff --check`
passed at the release-only runner source. The runner's release build passed
under 30 s. No product source changed in this policy/profile correction, so
Core workspace tests were not repeated for this measurement identity.

Do not merge the treatment PR to remote `main` or close #231 on these rows.
The issue requires four **eligible performance PASS** rows, independent
verification PASS, cleanup PASS and exact custody. The 100k verifier failed
its fixed budget and every row remains performance-ineligible. A new
prospective cache/numeric admission contract and a faster complete 100k
oracle would be required; the historical debug rows and these release
diagnostics cannot be relabeled after the fact.
