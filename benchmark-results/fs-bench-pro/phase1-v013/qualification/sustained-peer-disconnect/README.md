# Sustained peer-disconnect harness repair

The original defect was identified from source, not a completed live proof:
if a worker returned an error before an infallible Barrier wait, its peer could
remain blocked indefinitely. std::thread::scope would then wait for that peer,
preventing normal failure reporting until the outer 900-second active guard.
The 30-second cycle check ran after the blocking barrier and could not help.
The original sustained source excerpt and full-source hash are retained here.

The repair changes only sustained synchronization and error reporting. Two
standard-library channels replace the same three handoffs per cycle. Each
handoff sends its arrival and receives its peer with the remaining original
30-second cycle deadline. Endpoint ownership moves into workers, so worker
failure disconnects the peer rather than leaving an infallible barrier.
Original worker errors are printed immediately before returning. The actual
filesystem cycle, counters, independent oracle, 600-second active minimum and
900/600-second active/final guards remain unchanged. No product source or
capacity limit was changed.

Exactly one focused helper test ran: normal initial peer handoff followed by
peer exit, a silent peer with an explicitly short test deadline, and an already
expired deadline. Result: 1 passed, 0 failed, 5 filtered out; test-reported 0.02s.
The silent-peer test supplies 20 ms only to the helper; production remains 30 s.
This is a harness diagnostic, not a live sustained proof or a capacity failure.

`command.json` records the standalone rustc test build and exact selected test
command. `build.*` and `test.*` preserve real output; `test-source-before.json`
and `result.json` record matching before/after hashes. The external test command
wall is 289399541 ns, distinct from the harness-reported test duration. No other
test, Cargo/product build, benchmark, or preparation was performed.
