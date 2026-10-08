# R2 admission notification and worker exit component

> Status: Verified component; full R2–R5 remains in progress. No new approval gate.

The original Pending Future now has a before-admission counterpart. An original
command can wait on a fixed notification slot until job credit is released, then
yield its one admitted Pending. Caller cancellation returns only an unattempted
command. Retained results continue to hold credit. Oversized inputs and exhausted
registration tables are explicit before-effect refusals. The configured table is
charged as fixed scheduler state; native callers still own ingress/task/payload
accounting. No native scheduler or admission fairness claim is made here.

The SQL worker's exit guard fences admission and returns queued original inputs
on unwind. A previously published result remains published; explicit stop retains
the exact original panic payload inside Sync error custody. There is no replay,
new worker, provider reopen or guessed outcome after loss.

## Verification

- macOS ARM64 and Linux ARM64 locked Daemon all-target builds PASS.
- 34 selected public tests PASS on each platform: admission7, completion futures5,
  Owner9, completion ownership3, completion storage1, charge1 and Store Commit8.
  Actual test invocations are explicitly limited to100s; no outer limit expired.
- Host/Linux warning-denying all-target Clippy, workspace formatting,762-file
  product boundary and48 tooling tests PASS.
- Source/dependency/config hashes22, exact commands25/26/29/35 and host/Linux
  test binary hashes28/38 are retained. Root Cargo config retains the ARMv8
  AES/POLYVAL/ChaCha/target-feature flags. Linux uses the unchanged acknowledged
  image recorded in26; the one created container was removed by exact ID30.
- Store Commit fixtures explicitly select Disposable/WAL/OFF; Overlay remains
  MEMORY/OFF/EXCLUSIVE. Durable is NOT_RUN — disabled by owner until explicit
  reauthorization. Native FUSE, cold eligibility, speed/storage and residency
  measurements were not selected for this component.

## Retained failures

04/05 is the expected pre-fix oversized-input refusal regression.08/09 preserves
three incorrect fixture saturation assertions (State is a lifecycle job; Inode
uses ordinary slots), corrected in13/14.16/17 preserves the genuine worker-loss
failure at its3s internal wait: the original queue could admit into a departed
worker.19 preserves the intermediate Send/Sync compile failure when retaining a
bare Send-only panic payload.10 records the source causes and corrections.
None of these receipts is overwritten or reclassified as a passing run.

R2 still requires the replacement Fuse service/session/handlers, admitted Store
readers, resumable semantic steps, indexed native lookup/cookies, Attach/Ready,
security and complete normal drain. R3/R5 are not begun as native routes; R4 has
only its captured namespace page port. Completion of this component is not a
stopping point for the dispatched R2–R5 assignment.

Production LOC: 170795 -> 170986 (delta +191). Core105378→105569;
active62504→62695; reference65417, excluded predecessors37431 and excluded
integration5443 unchanged. Exact parent/staged classification and counter pin:
[37](37-exact-production-loc.json). No migration or retirement.
