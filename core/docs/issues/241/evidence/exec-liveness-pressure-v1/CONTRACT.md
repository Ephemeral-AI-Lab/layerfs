# #241 controlled host slot pressure diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Frozen before the sole synthetic attempt at source `d3bae7c75213d1cb151a84146e3a0a8b0c210c5d`.

Use a fresh validated 1 MiB master copy and one public SDK
`printf baseline > .position-baseline` Exec. After Mount, hold three host TCP
sessions with only the selector sent; they occupy handshake slots without a
Service request. The fourth slot remains available to the daemon until its
existing session closes. Retain host admission/drop/exit counts, daemon
TCP/Noise/Hello spans, typed Exec result, Docker state and cleanup. The
[plan](plan.json) pins the source, image, binary, master, copy and paths.

This deliberately creates slot pressure to test whether capacity refusal has
the same five-second signature as the frozen v3 failure. It is a separate
functional diagnostic with uncontrolled cache, no latency admission and no
retry. It cannot replace the frozen v3 row or the unpressured one-shot control.
