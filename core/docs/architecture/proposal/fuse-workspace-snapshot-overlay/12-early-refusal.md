# Preserve an early native refusal during upload teardown

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Correction from `4d6f5cd0fa4d21afe51fb1dda01db0d4a0095c88`, 2026-09-21.

R2 integration's whole-core test command exposed a failure in the existing
Status refusal test: the service correctly returned Unsupported, but its native
client observed Io. The native server wrote the terminal failure and immediately
shut both socket directions while the client's upload could still have queued
bytes. That made delivery depend on the transport-close race.

The shared `server::serve` error branch now half-closes its output after the
terminal failure, discards remaining input through the existing bounded Input
adapter and then closes the connection. Discarding uses the same declared length,
frame count, frame size, progress and absolute deadline. It never calls the
rejected handler again or supplies those bytes to it. An ordinary Client receives
the terminal failure and cancels its upload. No extra timeout, retry, worker or
queue is introduced. Cleanup/input errors do not replace the original handler
failure. A lost connection can still prevent delivery; no network reliability
or universal receipt-delivery guarantee is claimed.

The fix applies to all decoded requests rejected by a handler. Malformed BEGIN
metadata has no validated Input contract and keeps its existing immediate
best-effort refusal. Successful operations and their finality remain unchanged.

The new external bridge test queues a real upload byte, withholds END_INPUT,
receives the authoritative Denied result and confirms that final server teardown
waits for upload closure within the existing deadline. The original service
Status test's Unsupported assertion is retained. This is a focused correctness
fix prompted by an actual integration failure; no closed Pair 3 qualification
or historical transport receipt is relabelled.

The failed whole-core attempt and subsequent checks are retained in
[the refusal evidence](evidence/early-refusal-20260921/). They are functional
verification, not latency/throughput measurements. The new bridge test passes;
the whole-core integration command also passes after this fix, including the unchanged Status refusal assertion. All-target bridge Clippy, formatting, the boundary guard and six self-tests pass; the complete integration log is retained with the following R2 round.

```sh
CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-bridge --test early_refusal
```

## Production source comparison

Production LOC: **99,335 -> 99,337 (delta +2)**. Reference: 65,417 unchanged;
replacement core: 33,918 -> 33,920 (+2). The unchanged `tools/production_loc.py`
(blob `b5b9617d08204977176302311e0b2c72a811b420`) counts exact first-parent and
staged Git archives of `crates` and `core/crates`: production Rust and runtime
SQL, excluding inline tests, tests, fixtures, examples, tools, docs, manifests,
comments and blank lines. No legacy retirement or performance claim.
