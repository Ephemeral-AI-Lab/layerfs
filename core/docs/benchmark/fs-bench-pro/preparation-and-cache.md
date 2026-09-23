# Preparation and cache discipline

> **Status:** Proposed. No cache claim is admitted until its per-mode method is
> implemented, self-checked, and recorded in the receipt.

## Prepare once; sample from an isolated state

1. Generate or acquire the deterministic fixture outside the measured child.
   Pin its full logical manifest, generator/version, seed, sizes, modes, names,
   metadata, content digest, and expected-result recipe.
2. Validate the closed prepared master once per compatibility identity. Keep it
   immutable and outside sample-writable paths. Do not regenerate it before each
   case. A later acquisition is accepted only after identity validation.
3. For each post-initialization sample, create a new independent writable byte
   copy of the prepared master. Record copy method, source/target identity, and
   validation. This is setup reuse only; it is not an OS-cache state.
4. Use `fresh` only for initialization or cases whose contract creates a new
   output Store/workspace. Reject `clone` there. Never reuse a mutated sample.
5. Keep fixture validation, setup, daemon/runtime start, output creation, and
   oracle work outside the product timer only when they are not product work in
   the declared operation. Report all their wall/resource costs separately.

Use no APFS/reflink/COW clone under the `clone` name. Do not use links to a
mutable master. Do not clear protected caches routinely or repeat a run until a
warm variant passes. Reuse builds and inputs through identity seals, with reuse
visible in the receipt.

## Declare the cache contract per case and mode

There is no single cache bit for this pipeline. Register separately the OS page
cache for every process/filesystem that supplies measured bytes, in-process
product caches, prepared Store/workspace state, and daemon/transport session
state. State whether the measured operation reads a file-backed source, reads
the sample Store, consumes newly written bytes, or uses already-materialized
memory input.

For a case claiming a cold file read:

- Invalidate the exact source data pages before the timer, after any fixture copy
  or producer writes that would make them resident.
- Verify residency for the **whole declared input** with a non-faulting check.
  The check must not read/prime measured bytes or expected-result ranges. Run a
  self-check that proves the invalidation and detector catch known-warm pages.
- Where storage reads are part of the cold claim, require the registered device
  read/IO evidence as well; a zero-resident check alone does not prove that data
  came from the device.
- Apply the same method and acceptance threshold to every comparison arm and
  every sample. Keep acquisition/invalidation outside product time and report
  it; don't move a necessary product read outside the timer.

Use the existing
[`shared/cold.py`](../../../../benchmark/fs-bench-pro/shared/cold.py) contract
as a bounded precedent: it invalidates and checks whole-input residency for one
namespace case. It does **not** establish cold state for other cases, for the
direct-storage mode, or for another filesystem/process domain. Each new route
needs its own registered method and self-check.

When `daemon-host` and host storage use different operating systems or cache
domains, inspect the cache of the process that actually reads the measured
bytes. A host-side residency check cannot certify a daemon-side read (or vice
versa). If a route cannot expose a verifiable cold contract on every relevant
domain, its result is not admitted as cold-cache evidence.

For an operation that reads bytes it just wrote (for example, a Commit reading
its own spool): either evict and verify those pages after the writes and before
the timer, or report the row `INELIGIBLE` for a cold claim. Never let setup,
another phase, a previous sample, another arm, or a “just checking” pass supply
resident pages to the measured read. If the producer and reader are on different
hosts/filesystems and the reader's cache state cannot be inspected, the cold
claim is `INCOMPLETE`/`INELIGIBLE`; don't infer it from the host cache alone.

If the case has no measured file-backed read, say so and identify its real input
state. Do not call it cold just because it used a clone. A warm-cache row may be
retained as an explicitly labeled diagnostic only; it cannot be pooled with or
reported as the cold admission result.

## Status and evidence rules

- Unknown, asymmetric, or unverifiable cache state is never `PASS`.
- A resident page that violates a registered whole-input cold contract makes
  the row `INELIGIBLE`; preserve the raw timing as diagnostic evidence.
- Missing invalidation, residency, device-read, or identity evidence makes the
  row `INCOMPLETE` when eligibility cannot be established.
- Never drop failing, ineligible, incomplete, or unrun cases. Do not change
  worker counts, timeouts, workload sizes, cache policies, or acceptance bands
  after looking at an unfavorable result.
- Use a fresh output path for every command; keep discarded attempts, failures,
  verification, and cleanup results. The initial #231 runner takes one sample
  per case and has no comparison arm; a later campaign needs its own frozen
  contract before changing that cardinality.

Apply the existing measurement budgets and lock ownership in the root
[`AGENTS.md`](../../../../AGENTS.md). The core migration gets no exception by
virtue of using another workspace.
