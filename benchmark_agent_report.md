# Benchmark agent report template

> **Status:** Current general guide.

Use the relevant table below for every new benchmark invocation and checkpoint.
Freeze case IDs, order, operation, limits, cache contract and verifier before
sampling. Fill cells from the immutable receipts; never use an illustrative
value as evidence. Keep every registered case visible. Write `NOT_RUN` for an
unrun case, `UNAVAILABLE` with a reason for a missing observation, and the
actual `PASS`, `FAIL`, `INCOMPLETE` or `INELIGIBLE` verdict for an attempted
case. The [benchmark rules](docs/general/benchmark_rules.md) and each frozen
family specification govern the limits and claim; this file governs report
layout only.

## Identity and custody for every run

| Field | Receipt value |
| --- | --- |
| Family, case IDs, profile, gate version and selected order | `<exact values>` |
| Source commit/tree; product, compilation, harness and dependency seals | `<exact values>` |
| Release/debug profile, binary SHA-256, image ID when applicable | `<exact values or N/A>` |
| Fixture/oracle identities, seed, construction workers and setup method | `<exact values>` |
| Host topology, cache contract and measured interference | `<exact values>` |
| Exact command, fresh output path, sample count and build reuse | `<exact values>` |
| Separate verifier command, scope and wall bound | `<exact values>` |

Report raw time in nanoseconds and storage in bytes in receipts. Human tables
may add rounded seconds and percentages, with their raw operands and formula
linked. Never turn a complete-command wall into SDK-call time, put verifier
wall inside a speed comparison, sum overlapping nested spans, or use a
single-sample observation as a median or repeatability claim.

## Family 1 — public SDK Project Init

The inner timer surrounds `ProjectApi::new(&server).init(...)`; Server creation,
fixture preparation, process launch and the independent verifier are distinct.
There is no daemon/FUSE scope. The current four tiers use a 15 s complete
command bound and a separate 9.5 s lite-verifier bound. The verifier covers
every path/kind and the declared selected file metadata/content, not every
payload byte. Label explicit large tiers by their actual registration status.

| Files / exact case ID | Raw SDK call ns | Complete command ns / bound | Separate verifier ns / bound | Paths; selected files / bytes | Functional / cleanup | Registration | Numeric cache eligibility |
| --- | ---: | ---: | ---: | --- | --- | --- | --- |
| 100 / `<case>` | `<ns>` | `<ns> / 15 s` | `<ns> / 9.5 s` | `<count>; <count> / <B>` | `<statuses>` | `<status>` | `<status>` |
| 1,000 / `<case>` | `<ns>` | `<ns> / 15 s` | `<ns> / 9.5 s` | `<count>; <count> / <B>` | `<statuses>` | `<status>` | `<status>` |
| 10,000 / `<case>` | `<ns or NOT_RUN>` | `<ns> / 15 s` | `<ns> / 9.5 s` | `<count>; <count> / <B>` | `<statuses>` | `<status>` | `<status>` |
| 100,000 / `<case>` | `<ns or NOT_RUN>` | `<ns> / 15 s` | `<ns> / 9.5 s` | `<count>; <count> / <B>` | `<statuses>` | `<status>` | `<status>` |

## Family 2 — InProcess retained-history storage

This is a C1/C2/C5 component selection, not an SDK or Workspace benchmark.
Report the complete driver and its separate verifier for every stride. An
internal operation value is usable only when its timer tree is complete; the
stride1 timer has previously clipped, so do not quote a partial `operation_ns`
as the whole operation. Report original-owner C2 and C5 allocation separately,
their total, the applicable profile ceiling, and the exact deviation from the
original target. Historical strict receipts keep their original verdicts.

| Stride / states | Internal operation ns or reason unavailable | Complete driver ns / bound | Separate verifier ns / bound | C2 + C5 = total allocated B | Original target; profile ceiling; deviation | Independent roots / O3 / tree / bytes | Semantic / storage / cleanup | Numeric cache eligibility |
| --- | ---: | ---: | ---: | --- | --- | --- | --- | --- |
| 10 / 17 | `<ns>` | `<ns> / 60 s` | `<ns> / 10 s` | `<B> + <B> = <B>` | `<B>; <B>; <percent>` | `<proof counts/status>` | `<statuses>` | `<status>` |
| 3 / 53 | `<ns>` | `<ns> / 170 s` | `<ns> / 20 s` | `<B> + <B> = <B>` | `<B>; <B>; <percent>` | `<proof counts/status>` | `<statuses>` | `<status>` |
| 1 / 157 | `<ns or UNAVAILABLE: timer clipped>` | `<ns> / 170 s` | `<ns> / 30 s` | `<B> + <B> = <B>` | `<B>; <B>; <percent>` | `<proof counts/status>` | `<statuses>` | `<status>` |

Deviation formula: `100 × (actual total allocated − original target) /
original target`. It describes storage, not a speedup or a relabeling of an
older strict result. SDK and daemon time are `N/A` for this family.

## Family 3 — full Workspace write matrix

Each cell uses the public SDK and ordinary POSIX/FUSE writes. The headline is
the **external driver launch-to-exit wall**, covering lifecycle, Mount, Exec,
Commit, Status and checked unmount/deletion. The 100/512-write cells are bounded
by 15 s, the 4,097-write cells by 25 s, with a separate 9 s verifier. Require
known successful Commit and independent exact old/new bytes. Show all nine
cells even when some fail or have not run.

| Pattern | Writes | End-to-end ns / bound | SDK Exec ns | SDK Commit ns | Host Server span ns¹ | Daemon/FUSE span ns¹ | Verifier ns / 9 s | Old/new bytes; known Commit | Cache eligibility | Correctness / command / cleanup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- |
| Append | 100 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Append | 512 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Append | 4,097 | `<ns> / 25 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Dispersed | 100 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Dispersed | 512 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Dispersed | 4,097 | `<ns> / 25 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Repeated | 100 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Repeated | 512 | `<ns> / 15 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |
| Repeated | 4,097 | `<ns> / 25 s` | `<ns>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns>` | `<proof>` | `<status>` | `<statuses>` |

¹ Host Server and daemon spans are attribution **inside** the end-to-end
command. They may be nested or overlap; never add them to manufacture the
headline or compare them as competing arms. An absent observer stays
`UNAVAILABLE` with its reason.

## Families 4–7 — route-aware Workspace reports

Use one row per registered case with that case's own frozen limits. Full
Workspace SDK/daemon rows and component controls (notably Family5 deep
namespace access) go in **separate** tables. Do not promote a component
control to an end-to-end Workspace result.

| Family / exact case ID | Public route and acknowledgement | End-to-end ns / case bound | SDK phase ns | Host Server ns¹ | Daemon/FUSE ns¹ | Separate verifier ns / bound | Correctness / storage / cleanup | Cache eligibility | Result |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- |
| `<full Workspace case>` | `<Mount/Exec/Commit/Status/cleanup as declared>` | `<ns> / <bound>` | `<ns>` | `<ns or UNAVAILABLE>` | `<ns or UNAVAILABLE>` | `<ns> / <bound>` | `<statuses>` | `<status>` | `<status>` |

| Family / exact component case ID | Component operation | Operation ns / bound | Separate verifier ns / bound | Correctness / resources / cleanup | Cache eligibility | Result |
| --- | --- | ---: | ---: | --- | --- | --- |
| `<component control>` | `<exact public component API>` | `<ns> / <bound>` | `<ns> / <bound>` | `<statuses>` | `<status>` | `<status>` |

## Server, transport and daemon attribution beneath each Workspace row

For a registered concurrent Workspace selection, also report per-command and
per-Workspace/Commit identities and timings, actual overlap/progress evidence,
makespan, protected-control responsiveness, aggregate admission and retained
owners. Do not sum overlapping operation spans. The proposed
[concurrency evaluation tables](core/docs/architecture/proposal/bounded-workspace-implementation-20260930/EVALUATION.md#7-reports-and-evaluation-order)
provide a draft extension; that research plan does not register cases or replace
their prospectively frozen gates.

| Scope | Time/count evidence | Resource evidence | Verdict or availability |
| --- | --- | --- | --- |
| Host Server / C1–C5 / SQLite | `<measured nested spans, calls, objects and bytes>` | `<host CPU/RSS, C2+C5 allocated B>` | `<status or UNAVAILABLE: reason>` |
| Transport | `<bytes sent/received, measured transfer interval, failures>` | `<bounded buffers/spool attribution if available>` | `<status or UNAVAILABLE: reason>` |
| Linux daemon / FUSE | `<requests, bytes handled, measured intervals>` | `<cgroup anonymous/file cache, spool disk B>` | `<status or UNAVAILABLE: reason>` |
| Independent verification | `<verification wall and scope>` | `<verification-only memory/disk>` | `<status>` |

The campaign's host SDK/Server/SQLite and Linux Docker daemon/FUSE/workload
topology is declared in the benchmark rules. Report actual clone/copy method,
cache state, host and daemon resources in their own domains. A lifetime peak
cannot stand in for a phase peak. Keep correctness, command budget, storage,
cleanup and numeric cache eligibility as separate verdicts. An under-budget
`INELIGIBLE` row is not a numeric speed PASS.

## Round disposition and source size

| Item | Value |
| --- | --- |
| Every non-passing and unrun row, with cause | `<case, verdict, evidence>` |
| Reproduction command and append-only raw/compact receipt links | `<exact paths/URLs>` |
| Next concrete fix or selection | `<source-backed action; no unchanged-arm retry>` |
| Production LOC for each commit | `<first-parent before → committed after (signed delta); reference/Core/combined; same counter and scope>` |

Only make relative speed, throughput or Server/daemon attribution claims when
the frozen method supplies eligible, comparable evidence. Otherwise publish
the raw observations and their limits without inventing a control or missing
counter.
