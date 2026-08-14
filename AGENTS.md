# LayerFS Agent Instructions

This file defines durable implementation and verification rules for the
LayerFS Rust repository. It is not a handoff, status page, progress tracker,
or substitute for the active specification.

## 1. Establish authority before changing code

Follow requirements in this order:

1. The user's current request.
2. The controlling milestone specification and incorporated addenda.
3. The milestone implementation plan and execution ledger.
4. This file.
5. Historical handoffs and audit reports, which are diagnostic only.

For L1.5.5 work, the controlling documents are under:

`../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/l1.5.5/`

The broader system and LayerFS design context is under:

`../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/`

Before substantive LayerFS implementation or architectural review, read the
current versions of:

- `index.md` and `project_structure.md`;
- `supported_platform_driver.md`;
- `layefs/ARCHITECTURE.md`;
- `layefs/SPEC.md`;
- `layefs/STORAGE_AND_PERFORMANCE.md`;
- `layefs/IMPLEMENTATION_PLAN.md`;
- `layefs/read_after_l1.5.5.md` when planning work after L1.5.5; and
- the active milestone's specification, addenda, plan, and acceptance evidence.

Read any other document in that tree that is referenced by the controlling
specification or directly relevant to the requested subsystem. The broader
documentation supplies system intent and cross-component constraints; the
active milestone specification remains authoritative for its acceptance gate.

Before implementation:

- identify the active milestone and its explicit acceptance rows;
- identify work explicitly deferred to later milestones;
- inspect the current dirty worktree and preserve unrelated changes;
- trace the complete production path, including every caller of a shared
  function being changed;
- locate the existing type, helper, error vocabulary, test seam, and ownership
  boundary before adding a new one;
- state any assumption that changes semantics, resource bounds, format, or
  portability.

If two controlling documents conflict materially, stop and report the exact
conflict. Do not silently combine their strongest requirements into a new
gate.

## 2. Implementation discipline

- Fix the root cause at the narrowest shared layer through which all affected
  callers pass.
- Reuse existing project patterns before introducing a new abstraction.
- Keep diffs small, local, and reviewable after understanding the full path.
- Do not add speculative interfaces, fallback paths, feature flags, caches,
  workers, formats, providers, or configuration.
- Do not mix a correctness fix with broad renaming, file movement, formatting,
  or performance redesign.
- Preserve the repository's required module, test-target, fixture, and feature
  shape unless the controlling specification explicitly changes it.
- Keep production visibility as narrow as possible. Test support belongs under
  `cfg(test)` unless a controlling specification requires a bounded external
  qualification seam.
- Do not expose concrete storage authority merely to make integration tests
  convenient. Prefer bounded semantic requests and immutable observations.
- Do not change canonical bytes, hashes, object identifiers, ordering,
  envelopes, or on-disk formats without explicit format authority and
  compatibility requirements.

## 3. Rust correctness rules

- Use checked arithmetic for sizes, offsets, counts, reservations, and
  observation counters. Convert overflow to the exact typed error.
- Stage multi-field state or counter updates and commit them only after every
  check succeeds.
- Do not partially mutate an observation tuple before a later fallible step.
- Avoid `unwrap`, `expect`, indexing panics, and assertion-dependent production
  behavior on inputs, files, callbacks, counters, or shared state.
- Preserve chronological error order across callbacks, observation transfer,
  cleanup, terminalization, and unwinding.
- Catch and classify unwinding at user-controlled or test-controlled callback
  boundaries when the surrounding contract requires typed terminal behavior.
- Do not let `Drop` be the only path that performs mandatory accounting,
  terminalization, or fallible cleanup. Explicitly finish owned resources on
  ordinary return paths.
- Keep unsafe code out unless the requirement cannot be satisfied safely and
  the safety invariant is documented and directly tested.
- Keep feature-gated builds warning-clean where practical; never solve a
  warning by hiding reachable incorrect code.

## 4. Typed failure rules

Preserve exact distinctions among at least:

- unsupported operation;
- missing, malformed, unequal, or replaced occupant;
- permission denial;
- short read or short write;
- generic read or write failure;
- no-space, quota, byte exhaustion, and namespace-entry exhaustion;
- cancellation and deadline;
- synchronization poison;
- cleanup failure and invalidation failure;
- checked arithmetic or resource overflow.

Required behavior:

- preserve the earliest exact typed failure;
- allow a later cause to dominate only where the lifecycle contract explicitly
  authorizes cleanup or invalidation dominance;
- retain bounded provenance for both the first and dominant causes;
- do not flatten a known error into `Integrity`, `SourceFailure`, or an opaque
  callback payload;
- centralize raw operating-system error classification and test the raw mapper
  directly when changing it;
- never infer absence from `Path::exists` or an equivalent lossy probe;
- never replace an unavailable observation with zero, logical length, a ledger
  value, or another guessed numeric value.

## 5. CAS authority and immutable publication

- Authenticate canonical bytes before trusting identifiers or semantic shape.
- Treat installed CAS objects as immutable.
- Use no-replace publication. Never overwrite an incumbent in place.
- Classify a successful incumbent as authenticated equal/reusable, unequal,
  malformed, missing, replaced, or inaccessible as appropriate.
- Bind cleanup authority to the exact operation, generation, receipt, bytes,
  and file identity required by the format.
- Immediately before unlink, replace, adopt, or publish, revalidate the exact
  authority under the required short lock discipline.
- Do not invoke callbacks, waits, or fallible unrelated work between final
  authority validation and the namespace transition it authorizes.
- If authority becomes unavailable or ambiguous, retain custody and fail
  closed. Never delete a pathname using stale validation.
- A hard-link `Unsupported` or `CrossesDevices` result remains typed and has no
  copy, reflink, memory, provider, or whole-pack fallback.

## 6. CDC, object, and COW semantics

- Preserve the frozen CDC profile, canonical object encoding, hash domains, and
  fragmentation-independent results.
- Source payload processing must be streaming and bounded; do not stage a
  source-sized buffer.
- A small edit may reuse authenticated unchanged chunks, but reuse must never
  bypass base authentication or exact rejoin validation.
- COW mutations must preserve canonical order, authenticated roots, exact
  object identity, and declared reuse/creation accounting.
- Do not label a detached pair of mutations as one atomic Move, ReplacePair, or
  cross-directory handoff.
- A logical Remove changes reachability in the authenticated tree. It does not
  imply native recursive deletion, physical CAS reclamation, GC, or compaction.
- Keep honest asymptotic behavior. Do not claim logarithmic or edit-sized work
  where the frozen format requires suffix or closure traversal.

## 7. Resource and accounting invariants

- Preflight and reserve bounded resources before invoking a supplier or making
  irreversible namespace state visible.
- Keep byte and namespace-entry admission independent and return the matching
  typed exhaustion cause.
- Every admitted operation must terminate with exact byte and namespace
  equations:

  `requested = released + committed + retained`

- Release every capability, queue ticket, active slot, reservation, lock
  authority, preparation file, and spool, or retain it as explicit typed
  terminal custody.
- If exact custody cannot be represented numerically, record typed unavailable
  or quarantined state. Never fabricate a total.
- File-backed spools are required where metadata grows with input and cannot
  fit the admitted in-process memory plan.
- Logical LayerFS accounting is not allocator usage, RSS, PSS, stack, mapped
  libraries, operating-system page cache, or physical filesystem allocation.
- Optional host observations must be `Observed`, `Unavailable`,
  `NotApplicable`, or another explicit status; unavailable values remain
  absent rather than zero.

## 8. Concurrency and control

- Production operations run synchronously on caller threads unless a
  controlling specification explicitly says otherwise.
- Do not add internal workers, Rayon fan-out, hidden retries, redispatch, or
  capacity multiplication.
- Enforce the declared admission cap and queue bound exactly.
- Cancellation and deadlines must remain observable while queued, waiting on
  semantic ownership, performing long I/O, comparing, hashing, sorting, and
  cleaning up.
- Use the project's controlled wait and polling mechanisms; do not busy-spin or
  use arbitrary sleeps as synchronization.
- Preserve the documented lock acquisition order.
- Keep visibility and publication critical sections short. Do not perform full
  payload I/O, hashing, decoding, incumbent comparison, sorting, or callback
  work while holding them.
- After out-of-lock work, reacquire and exactly revalidate the prerequisite
  snapshot before the authoritative transition.
- Record lock and semantic-owner wait/hold observations directly and
  transactionally. Do not manufacture wait time from elapsed operation time.
- On callback unwind during lock acquisition or release, drop the guard,
  balance the observation state, and preserve the typed terminal contract.

## 9. Filesystem and portability rules

- Use actual fallible filesystem operations and classify their exact outcomes.
- Distinguish missing, permission, wrong type, symlink/dangling path, short I/O,
  no-space/quota, and generic I/O.
- Open and revalidate regular files without following an unexpected symlink.
- Use stable file identity only where the platform provides the required
  semantics. Otherwise return typed `Unsupported` or fail closed.
- Keep platform-specific mechanics behind narrow `cfg` sections and
  platform-neutral semantic interfaces.
- Do not claim support for an operating system, filesystem, provider, or race
  model that has not passed its own direct qualification.
- Current fail-closed behavior on another platform is not proof of functional
  portability there.
- Do not introduce native materialization, projection, clone/reflink behavior,
  or provider switching unless it belongs to the active milestone.

## 10. Module ownership

Keep responsibilities in their semantic owners:

- object modules own canonical object framing, encoding, decoding, lengths,
  and physical identifiers;
- CDC modules own chunk-boundary and resynchronization mechanics;
- content modules own logical file/tree content semantics and streaming ports;
- COW modules own authenticated view and mutation semantics;
- pack modules own pack layout, index, carrier, and pack-port mechanics;
- CAS modules own immutable storage mechanics, locators, publication,
  authentication, and filesystem authority;
- lifecycle modules own complete operation coordination, admission,
  terminalization, cleanup, and validated handoff;
- read modules adapt authenticated content/CAS ports to extraction and range
  operations without duplicating object/content parsing.

Do not duplicate canonical layout arithmetic or semantic validation across
owners. Move the shared rule to its owner and call it from consumers.

## 11. Test design

Every non-trivial behavior change needs the smallest direct regression that
would fail without the fix, plus a real load-bearing path when adapters or
lifecycle composition could alter the result.

Tests should assert, as applicable:

- exact typed first and dominant causes;
- exact canonical bytes, identifiers, and outcomes;
- byte and namespace terminal equations;
- resource, queue, slot, preparation, and authority baselines after return;
- exact retained residue when publication preceded failure;
- stale-handle and reopen behavior after invalidation;
- direct counters and zero forbidden fallback/retry/fan-out work;
- destination bytes when real I/O completed before an observation failure;
- current and reopened handle usability after a healthy non-invalidating error.

Test rules:

- use deterministic, minimal fixtures;
- use barriers, channels, controlled fault boundaries, and bounded watchdogs for
  races; do not prove races with timing sleeps alone;
- inject a fault at the real semantic boundary being claimed;
- include a clean sibling when a repair changes normal behavior;
- test arithmetic boundaries and all-or-none counter updates directly;
- test raw OS error classification separately from already-typed injected
  failures;
- preserve historical material assertions, fixtures, fault points, and race
  schedules when migrating tests;
- do not replace substantive tests with dispatch-by-index wrappers, tautologies,
  assertion padding, snapshots, or token-presence checks;
- a command that discovers or runs zero tests is not evidence.

## 12. Verification workflow

Use the smallest verification tier that can establish the change, then expand
only at stable checkpoints.

### Edit loop

1. Run the exact affected test.
2. Run the narrow module or owner test if the change crosses an adapter.
3. Run `git diff --check`.

### Owner or subsystem checkpoint

1. Run the complete affected test target with a nonzero test count.
2. Run affected architecture, compile-fail, or custody checks.
3. Run formatting and package check.
4. Record the exact source fingerprint if the result will be used as evidence.

### Cross-cutting or final checkpoint

On unchanged source:

1. Verify Cargo metadata and the exact intended targets/features.
2. Run default and all-feature package checks as required.
3. Run every affected substantive owner exactly once.
4. Run architecture and custody gates once.
5. Run formatting, diff checks, and warnings-denied/clippy when required by the
   active milestone.
6. Perform the required independent read-only audit.

Typical commands, adjusted to the active specification:

```sh
cargo test -p layerfs-storage --offline --all-features --test <target> <test> -- --exact --nocapture
cargo test -p layerfs-storage --offline --all-features --test <target>
cargo check -p layerfs-storage --offline --all-features
cargo fmt --all -- --check
git diff --check
```

Do not run a broad wall after every edit. Do not recursively run the same owner
wall from another verifier when the closure workflow already runs it.

## 13. Build and multi-agent coordination

- Use one Cargo writer/process at a time against the shared target directory.
- Agents auditing an active implementation should remain read-only and avoid
  competing Cargo workloads.
- Do not perform final audits on a moving source fingerprint.
- Do not create a target directory per agent.
- Do not run `cargo clean` as routine troubleshooting; measure before deleting
  reusable build artifacts.
- Keep the toolchain, feature set, `RUSTFLAGS`, and target directory stable
  during an iteration cycle.
- Compile multiple affected targets in one Cargo invocation when this reuses
  work.
- Run deliberately slow concurrency/load rows without unrelated compiler or
  audit contention.

## 14. Performance claims

- Correctness counters and asymptotic inspection are not timing evidence.
- Concurrency overlap does not imply linear speedup.
- Reopened-handle behavior does not establish operating-system-cache cold/warm
  behavior.
- Streaming and logical memory admission do not establish RSS/PSS limits.
- CAS extraction is not native materialization or product startup.
- Report current work amplification honestly, including repeated metadata,
  closure, range, or suffix work.
- Make speed, throughput, latency, CPU, RSS/PSS, cold/warm, and scale claims only
  from the milestone and benchmark evidence that owns them.

## 15. Worktree and Git safety

- Preserve all user and coordinated changes in a dirty worktree.
- Do not use destructive reset, checkout, clean, or broad deletion.
- Do not commit, push, rebase, or rewrite history unless explicitly requested.
- Use `apply_patch` for manual source edits.
- Do not remove or weaken an assertion merely to obtain a green run.
- Do not refresh a frozen digest without inspecting and approving the exact
  semantic delta it represents.

## 16. Completion report

Report:

- the behavior implemented or repaired;
- the root cause and the shared layer changed;
- files materially changed;
- exact verification commands and nonzero outcomes;
- any test not run and why;
- the exact fingerprint when claiming a stable gate result;
- remaining work owned by later milestones, stated separately.

Never declare completion from compilation alone, test names/counts alone, a
refreshed digest, a partial verifier, or results produced from changing source.
