# H11 retained-control preregistration v9

Status: **FROZEN BEFORE ANY V9 MEASURED ROW**. V8 remains byte-preserved with diagnostic PASS labels; fresh source audit controls its effective `REVISE` disposition.

## V8 blockers

- `env::args().collect::<Vec<_>>()` owns a Vec and argument Strings across the whole sample without either Q ledger.
- Historical verification drops its capacity charge immediately before dropping the owned operations Vec.

V8 gate anchors: terminal `39e49eade31dd3506504a81240cab2ef27e7f4b70a155cfe2069c9ed7f8356b8`; final manifest `3f23af7b0cfd882308fa83323856596998b03653d13bfac437023253f3b44514`; raw `8a849b15e6f33a86b588bbf0936f76df775ada7369f22741b7f89b1e703831c2`.

## V9 source repair

1. Borrow Darwin process arguments directly from the already-installed `libc::_NSGetArgc/_NSGetArgv` interface. Bounds, nulls, exact argc, and UTF-8 are checked; `CStr::to_str` borrows without allocation. No argument Vec/String exists.
2. Drop historical `operations` before `operations_charge`.
3. Add a focused borrowed-argv bounds test.
4. Retain exact manifest/vector, reachability, history, edit-point, historical operation, formatting/report, and terminal-drop charges from v8.
5. Build a fresh v9 executable with native v9 sample/Q schemas.

No new dependency, LayerFS format/profile/schema/algorithm change, expectation change, schedule change, threshold change, or row reuse.

## Ladder

Focused borrowed-argv, edit-point/historical-Q, whole-harness-Q, Store timer, and H11 clippy; build once; zero-row dry-run; fresh N=1,000 screen; frozen-source v9 tests/static closure; fresh eight-row gate; fresh independent source/performance/custody audit. Any failure is preserved in v10.

