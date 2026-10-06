# S9 local history checkpoint failures

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

- `runtime-final.log` remains the original failed output (0/8 passed). Making the
  listener nonblocking bounded accept, but on macOS accepted streams inherited
  nonblocking mode and native handshake reads returned WouldBlock. The external
  fixture now restores blocking mode on the accepted stream before its existing
  two-second read/write deadlines. Accept itself retains its two-second deadline.
  No production handshake retry or timeout change was introduced.
- `build-conflict.log`: the new fixture passed owned String to PathName::new(&str)
  and passed an active timing scope where accept requires a pending child scope.
  `build-conflict-repaired.log` fixes only PathName; the timing error remains there.
  `build-conflict-timing-repaired.log` fixes the remaining caller type and passes.
- Earlier import/result-shape compiler failures are retained in the S7 checks
  folder because the first combined no-run build encountered them.

The final SDK covering output has 9 passes through the real host provider. No test
hits its wall ceiling. These checks qualify this local handler boundary only;
unknown-history transport/disconnect resolution remains unimplemented and unrun.
