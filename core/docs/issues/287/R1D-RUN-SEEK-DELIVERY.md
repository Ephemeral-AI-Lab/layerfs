# R1d-run-seek delivery and exact proof scope

> Scoped exits PASS; the larger R1 milestone remains PARTIAL.
> First parent43c977c00e342d207757cd8bf5f6e2cd15041650; the actual resulting
> commit/publication is recorded by the #287 checkpoint, not guessed here.

The existing ReferenceReducer caller now keeps every immutable tier's forward
cursor and one inline overshoot independently of backward fixed96-byte binary
points. Pending rows and newest tiers keep exact complete-row precedence; errors
have no older-tier/sequential/consolidation rescue. Spill key equality/positive
order precedes backing creation. Finalized count/length/bounds and sequential
row order are checked. Both adopted inputs and growing carry intermediates
remain charged through the next output reservation; known checked release
precedes logical credit return. No public signature, canonical v1 grammar,
ordering quota, construction worker or dependency changes.

The frozen responsibility is [R1D-RUN-SEEK-FREEZE](R1D-RUN-SEEK-FREEZE.md).
[Source/check evidence](evidence/r1d-run-seek/01-FROZEN-SOURCE.json) records the
owned8-file SHA2562c9d5a3c5fb46c116e78740e175c93391e5aa1eca804b2b732cb038a25818a7a.
Its method is sorted relative UTF8 path, NUL, u64BE byte length, raw contents.
Real FileBacking plus bounded scalar observation measures read_at calls and
requested/successful bytes, not kernel physical storage or warm-cache speed.
Expected complete rows/effects derive from independent input snapshots/events.

## Owning checks

The exact command below passed41 tests: hardlinks6, ordering18, consolidation1,
scan3, independent sealed reference2, new run-seek11. No failures were hidden,
no repeated passing suite or benchmark sample was taken.

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_ordering_scan --test filesystem_ordering_consolidate --test filesystem_ordering --test filesystem_run_seek --test filesystem_reference --test filesystem_hardlinks -- --nocapture
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-content --all-targets -- -D warnings
cargo +1.85.1 build --manifest-path core/Cargo.toml --locked -p layerfs-content --examples
```

[Raw test output](evidence/r1d-run-seek/02-content-tests.log),
[Clippy](evidence/r1d-run-seek/03-content-clippy.log) and
[examples](evidence/r1d-run-seek/04-content-examples.log) retain exact command
outcomes. Complete command walls10.057746667/7.304949709/3.129091916s are
correctness/build wall observations, never product speed/performance gates.
Examples compiled only; no workload was run.

The exact staged-source snapshot excludes independent unstaged R1e engine/
policy/binary changes. Whole-Core fmt passed, product boundary scanned380
production files, and its selected guard module passed7 tests. The unrelated
SDK telemetry self-tests were unaffected and not repeated. The governing
staged policy is the published parent policy; developing R1e's additional unsafe
allowlist is not substituted for it. [Snapshot commands/results](evidence/r1d-run-seek/05-staged-snapshot-checks-result.json)
record source tree73eee2f8e99e017910200fdc63bd789ea4e343b0 and Core tree
8c9d576b977ffc4f43016c63c569f751c43418a2. Documentation additions leave that
counted/checked product subtree unchanged. Repository-root ARMv8 flags and the
owned worktree target apply. Linux/strict physical proof and full final Core
checks remain unrun.

## Count evidence and custody

- Fresh103-row sparse ascending sweep:15 read_at calls,9888 exact requested/
  successful bytes,103 decodes, maximum buffer672. No output creation/append.
- Prime1021 rows after an explicit forward pass:3063 alternating/descending
  backward queries used27583 exact96-byte reads/2647968 bytes, below the
  prospective33693-read bound. No prefix restart, creation, append or flush.
- Consolidation: live288-byte inputs plus proposed288-byte output exceeds
  operation480 before output creation/read/append, even though backing capacity
  is8388608. Checked release returns the retained charge.
- Eighth carry: actual768-byte inputs +96 pending +768 output =1632 exceeds
  operation1536. Incoming1/intermediate2/intermediate4 are the only3 creates;
  the last8-row output is refused before creation. Intermediate reads/writes
  and cleanup are charged, not hidden outside a timer.
- Real reducer uses pending3,67 independently modeled serials,267 spilled rows,
  88 merges; sampled state queries create/append/flush nothing. The recorded
 3470 read_at calls/523104 bytes include actual spill/merge/query work.

These are labeled count diagnostics, not benchmark collection or admission.
Unchanged-tier work is<=96*n forward bytes plus
96*Q*(ceil(log2(n+1))+1) backward bytes; replaced tiers/merges are counted
separately. Real short/corrupt newest reads propagate their original error with
one attempted96-byte point read and no older fallback. Real malformed supplied
metadata/order and pre-effect map-key/zero refusal are externally checked.

Cached handle length is not fstat/native identity. The legacy OrderingBacking's
partial-append/refund, exact removal identity and release/Drop retry custody are
still open. Read/allocation counters do not prove RSS, file residency, kernel
I/O or global/engine/native physical containment. No strict/concurrent
capability, larger profile, benchmark or release admission is advertised.

## Exact production LOC and next responsibility

Production LOC:138994 ->139180 (delta+186).
Reference:65417 ->65417 (delta+0).
Core:73577 ->73763 (delta+186).
The identical revised root counter, blob a1cb6c064dd150dbc3aa19648a3748b5aeb741b5,
SHA2560d798f53263c0f18636d96ccdfdc3728f501f53b066e90577d251859f1f117b2,
counts exact first-parent/final staged Git archives of crates and core/crates.
[Per-file classification/totals](evidence/r1d-run-seek/PRODUCTION-LOC.json)
include required package/src SQL and exclude inline/test-only/test/example/
fixture/tool/doc/manifest/generated/third-party source. Physical ceiling checks
are separate. No legacy retirement, production-scope change or duplicated
canonical algorithm occurred. The committed archive must reproduce the staged
result before confirmed publication.

Next coordinated checkpoint is R1e-bootstrap's exact native provider decision;
its genuine Darwin readback failure is retained in its separate evidence and
cannot become a32MiB PASS. Independent binding-input planning may continue.
The larger R1d paged graph/draft/reference/release populations, R1e healthy
engine/control envelope, R2–R7 and #288 qualification remain open. No #288
campaign, issue edit or receipt replacement occurs.
