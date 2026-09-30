# #276 deferred optimization experiment log

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

Append-only checkpoints for the optimization subset of #276. Broader Core
streaming, reservation batching, dirty-discard/close and numeric cache admission
remain separately owned or deferred.

## C1-001 — baseline counts and candidate algorithm freeze

Prospective [specification](C1-CYCLE-CHECKPOINT-SPEC-20260930.md), baseline source
`eb3da4be63d2378df2fc47855e421a7a4c8407be`, tree
`2e14eb7e008231e004452a9c03e6fc28df29850f`, product identical to published
`7edddbdb8e8512627aed0ed42533ef099d802384`. Locked release diagnostic build
9,614,142,041 ns, within preferred30s; full count command718,693,125ns /15s.
One invocation, ordered depth16 fresh/inherited then depth270 fresh/inherited.
In-memory resident authenticated fixtures: numeric INELIGIBLE, admission false.

| Input | Directory row lookups | Value lookups | Inode demands | Inode pages | Directory pages | Stored entries examined | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
|16 fresh |136 |186 |187 |2 |0 |0 |PASS |
|16 inherited/restated |184 |0 |220 |2 |153 |169 |PASS |
|270 fresh |36585 |37397 |37398 |6 |0 |0 |PASS |
|270 inherited/restated |UNAVAILABLE |UNAVAILABLE |UNAVAILABLE |UNAVAILABLE |UNAVAILABLE |UNAVAILABLE |FAIL: cycle check work limit |

The fourth check exhausts the unchanged64MiB ordering allowance (65536 work
entries): repeated inherited walks charge both stored and effective entries.
Its panic prevents final counters; absent values remain unavailable, not zero.
No baseline retry or larger bound is authorized. The original prospective
both-arm-success expectation was disproved, not relaxed into a passing baseline.
Candidate counts still face the original fresh<=depth/inherited<=4*depth+2
lookup gates; inherited270 is additionally an explicit failure-to-completion
result, with no paired numeric speed claim. All four cases remain visible.

Implement the one operation-local active/completed DFS in `validate/cycles.rs`.
Alias/allocation/root/build/count checks, formats and resources stay intact.
The new proof-map entry ceiling includes empty directories and can explicitly
refuse formerly zero-edge work that exceeds resident state; this tighter
boundary is documented rather than hidden.

Focused correctness initially failed to compile due to two external-test API
mistakes; after those corrections,17 existing bounds tests passed and the new
oracle found a fixture error (omitted empty directory rows left synthetic
content roots). Correcting that fixture, the3 new tests pass, covering all64
mixed stored/new final-parent graphs, independent base-less canonical root
identity, old-root reads, no publication on cycle/alias refusal and proof-state
bound. No product algorithm changed in response to these test fixture mistakes.
Raw attempts remain under worktree-local
`benchmark-results/phase5-deferred-opt/{baseline-build,baseline-count,focused-correctness,focused-correctness-repaired,graph-oracle-repaired}`.

Next: commit this algorithm, run candidate count diagnostic once, then the
frozen selected public SDK deep270 operation and separate exact-byte verifier,
and required owning Core checks. No Family2 or unchanged family sweep.
