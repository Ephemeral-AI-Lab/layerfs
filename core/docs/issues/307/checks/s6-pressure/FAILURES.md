# S6 pressure slice: retained outcomes and repairs

These are append-only outcomes from the dirty completion slice after `be651a048`.
They are component correctness/work/resource diagnostics, not eligible cold-speed,
RSS or release evidence. No failed product operation was replayed. Every executed
Rust test command has an explicit <=120s wall ceiling and a prior no-run build.
No command in this slice reached that ceiling. Original raw logs remain unchanged.

| Receipt / outcome | Cause and resulting action |
| --- | --- |
| build-accounting-1: compiler FAIL | Namespace getter already returns i64; removed an incorrect unsigned conversion. No test executed from this failed build |
| build-source-wait-2: compiler FAIL | New wait_refs decode field omitted from StoredCounts initializer; supplied its actual SQL column |
| device-ext4-first: fixture FAIL | Initial filler assumed one allocation of every reported free block would succeed. ext4 retained metadata blocks and returned ENOSPC before the product action. Diagnostic now records original filler ENOSPC/residual blocks; no failed fill is retried |
| overlay-integrated-first: compound FAIL | Accounting's IN(0,ns) generated persistent iterator opcodes. Replaced all count updates with two explicit primary-key updates; VM scan opcodes are now zero |
| compound-range: count assertion FAIL | Exact changed rows rose8->14 from two backed-count updates for each of three affected insert/delete records. Updated the stated arithmetic, preserving all runtime work and scale equality checks |
| overlay-final-first: fragmentation FAIL | Statement/change counts remained identical; old NULL-mask accounting differed by64 VM instructions over32 cells. The fragmented arm was cheaper. Keep equality of statements/changes and require fragmented VM work not to grow; report both values. Edge DML includes its accounting trigger, with exactly two additional edge reads |
| core-final: FAIL after passing earlier packages | Pre-BEGIN page_count caused one Startup fullscan counter step in compound profiling. Main qualification alone did not repair it (overlay-main-final remains FAIL). page_count's supported VM contains Expire; removed it from hot admission. Nonexpiring freelist cookie plus owned dense aligned descriptor length bounds available capacity. Later overlay-cookie-final and real owner profiles pass every scan/sort/autoindex/reprepare guard |
| device-owner-ext4-final: fixture expectation FAIL | One metadata operation was legally Published from previously reserved capacity, contrary to the fixture expectation of immediate refusal. Revised proof uses distinct new data windows and stops on its first actual allocation refusal. It accepted one new window, refused the next before BEGIN, then idle cleanup reclaimed all accepted state. No failed operation is resubmitted |

Source review also corrected two defects before selected exit execution: releasing
one reader no longer wakes an unbounded generation population in one transaction;
a smaller cleanup allocation retains a prior larger guaranteed range. Final
resource snapshots prepare expiring page_count afresh once, rather than caching
it and automatically repreparing it. Eight snapshots prove zero reprepare.

The provisional64/32 MiB reservation plan was incomplete source arithmetic. Final
128/128 MiB bounds include indexes, triggers, eight conservative shrink/orphan
allowances and all bounded cleanup paths. The fixture grows128->512 MiB to hold
that declared256 MiB startup reservation. This is an explicit source/fixture
selection amendment, not a changed frozen performance gate or a claimed storage
improvement. All old128 MiB fixture outcomes remain retained.

Existing source assertion updates in source.rs count the two new backed updates
on source insertion/deletion (2->4 rows). No altered byte/root/ownership/error
contract or ignored failing test is used to obtain acceptance.

Full core invocation remains FAIL, with unrun later binaries retained by its log.
Coverage is completed through scoped later commands, and unchanged earlier
packages are reused at their source scope; it is not relabelled aggregate PASS.
The Linux four-package build/test selection emits an existing unused
BackendError::Filesystem warning from unchanged Persistence's macOS-only allocation
backend. Linux SDK owning provider cases remain cfg-disabled; macOS executes them.

No performance campaign is selected. Functional package invocations can exceed
10s while staying below the120s test ceiling; their wall values are diagnostic,
not independent performance proofs. Both selected ext4 resource proofs include
fixture preparation and normal teardown and finish under1s. Build/cache preparation
is separate. macOS Clippy and Linux compilation overlapped once with separate Cargo
targets; no speed/cache/throughput result is inferred. Four unrelated containers
were preserved; owned loop/mount fixtures use exact identity and normal teardown.

Final default telemetry tests also overlapped final macOS Clippy. Their functional
assertions pass; no timing eligibility is inferred. Final resource-observation and
device receipts follow the noncached page-count repair and show zero reprepare.
