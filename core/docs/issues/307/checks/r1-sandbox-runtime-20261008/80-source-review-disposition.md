# Runtime foundation source review disposition

> **Status:** Dated implementation checkpoint; full R1 acceptance remains open.
> Main owns all edits/runs; three agents performed read-only source/primary research.

| Finding | Implemented disposition / retained gap |
| --- | --- |
| No reusable active JSON/HTTP/mux implementation | Concrete std-only streamed adapter; existing nix0.31.3 polling only. Private report/harness encoders and excluded source are not dependencies/includes |
| Failed stdin close loses custody | InputEnd preserves terminal counters; InputCloseFailure retains original input/error/attempt, prevents another half-close. Actual stdin=true2MiB proof exercises success; injected half-close failure qualification remains open |
| Output progress hidden after sink failure | Public read-only progress/pending/framing accessors; actual2-byte acknowledged delivery and3-byte pending tail test |
| Wrong/missing/duplicate stream media accepted | Exact non-TTY multiplexed media required. Positive actual Engine proof and wrong/missing/duplicate external cases |
| Loose chunk trailers/extensions/status line | Shared strict header-field grammar; framing trailers refused; extensions explicitly unsupported; malformed reason/separator/control and trailer cases |
| Inspect requested identity labeled as observed | Distinct requested/decoded IDs, retained wrong/partial actual response selectors. Lost Start uses original request selectors, never invents a decoded reply identity |
| Error HTTP status claims nonexistent diagnostics | Actual separately bounded prefix/count/complete/read-error, Debug redacts raw bytes; non-200 fields do not fabricate response IDs |
| Metadata char writes make excessive syscalls | Fixed8KiB request encoder; local metadata count pass plus one streamed send. Positive partial progress, no retry after original errors |
| Fast closed Unix peer rejects deadline clearing | Safe polling over nonblocking descriptors; switch local wait policy after101, no deadline-changing syscall on closed socket |
| Giant fixture unexpectedly EOF | Peer accepted sockets inherited nonblocking on macOS; explicitly set fixture blocking mode before its unchanged2s waits. Original8192B/WouldBlock receipt retained |
| Reuse of failed Start / output copy | Owner records terminal/attempt state before effect; second Start has no send, failed output fences once and cannot copy again |
| Missing per-Exec Engine cancellation | Typed before-effect unsupported, not client kill or broad-scope substitution. Standard external2.2.4 client prerequisites [78](78-cancellation-prerequisites.md) remain to implement/qualify |
| Lifecycle/access/SDK route missing | Full original objective preserved; real daemon application composed next with private config, named Store, SDK correlation, explicit ordinary identity/access. No placeholder Ready path/facade |

Original diagnostics incorrectly selected the prior test artifact after the graph
changed:19/22/23 do not qualify the poll implementation. Exact commands/binary
identities identify this error; records are not overwritten/relabelled as PASS.
Current tests select binaries from recorded build output. All original build,
Clippy, source-directed repairs and failed test/invalid-artifact receipts remain.
Source review is not native behavior, memory, performance or whole-goal evidence.
