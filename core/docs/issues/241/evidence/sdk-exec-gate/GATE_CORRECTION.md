# #241 SDK Exec→Commit gate correction — 2026-09-24

The first mounted [receipt](attempt-001/receipt.json) belongs to source commit
`8493d8f8b59216438468758b51e4c5d3b296e9d4`. Its tool returned exit zero
and untruncated canonical `PASS` output, and the caller committed successfully.
The test-only gate at that source identity checked **only** exit zero before
Commit; the test checked truncation and output content afterwards. Thus that
receipt proves this successful mounted route, **not** that a zero-exit Exec with
truncated, malformed or non-PASS output would omit Commit. The original raw
receipt, identities and report remain unchanged.

The follow-up gate checks the complete `ExecResult` before calling the public
SDK Commit API: exit status `Some(0)`, neither output stream truncated, and
exactly the frozen edit tool's one-line splice JSON `PASS` output with the
caller's expected `final_bytes` and `shifted_bytes:0`. An absent newline, extra
output, wrong operation, wrong length or nonzero shifted count is ineligible.
The helper now accepts `expected_final_bytes: u64`; callers must calculate it
from the declared pre-edit length, deletion length and replacement length.
The canonical byte check is specific to this frozen test tool, whose output
format is fixed in `core/benchmark/fs-bench-pro/workload/src/main.rs`.

The focused pure test checks eligible output and rejects nonzero/absent exits,
zero exit with either truncation flag, malformed output, non-PASS status,
wrong final length and nonzero shifted count. The helper's Commit branch uses
that same predicate. No mounted case was repeated after this source change;
the corrected gate has **not** itself been exercised in a new live receipt.
The separate position sweep can exercise it with fresh Branches and its own
old/new Commit oracle. This correction makes no latency or cache claim.
