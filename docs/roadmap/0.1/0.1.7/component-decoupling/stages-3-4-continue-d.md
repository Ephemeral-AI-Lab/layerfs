# Prompt D: correct the oracle and complete stored-tree edits

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

## Copy/paste assignment

Complete checkpoint D for [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169)
in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.
Read and follow the [shared continuation rules](stages-3-4-continuation-prompt.md),
the [completion details](stages-3-4-completion-handoff.md) and the original file
content contract. Pooling is implemented; its remaining coverage does not block D.
Do not restart E or stop this assignment after fixing the test harness.

You own C1 `src/file/edit/`, necessary `file/mapping/` and `file/view.rs` changes,
external edit tests, and the external reference oracle/fixtures. You are not alone
in this repository. Preserve other edits; coordinate shared public/error/policy
changes with the integration owner. Do not edit C2 pooling or shared reports while
another owner is working there. Keep independent task evidence.

### 1. Correct the compared operations before diagnosing the product

Source inspection at `2b2dbc028` found this mismatch:

- The reference `FileMutationBatch::replace` applies each replacement to its current
  root, then updates the root. `rope_edit_oracle.rs` passes starts unchanged.
- Candidate `tests/edit_reference.rs::fixture_inputs` adds accumulated length delta
  based on a comment that incorrectly calls the reference coordinates original-file.
- In `batch-normalized`, after the initial 700-byte insertion, the reference uses
  600,000 and 1,200,000; the candidate uses 600,700 and 1,200,700.

Recheck against the actual source snapshot. Make both sides execute the same
current-result edit stream; do not change product semantics to match a mismatched
test. Record the normalized tuples, replacement lengths/byte identities, final-byte
model and base root for each side. Assert equal operations before comparing roots.
Add a small insertion-followed-by-replacement/deletion regression for coordinate
translation. Compare after each edit if that isolates a remaining difference.

Preserve historical fixtures and failed receipts. If oracle inputs/outputs or
interpretation change, produce new identified evidence and mark old comparison
claims inapplicable through an additive correction. Audit fixture paths, source
identity and actual fixture contents; a file containing a revision string alone
does not seal the oracle. Use separate reference execution, never a candidate
production dependency on old crates.

Run the corrected reference target. Report which mismatches remain; do not assume
the harness correction fixes the mapping algorithm or invalidate unrelated cases.

### 2. Implement the missing algorithm through the same edit entry point

Read the pinned reference rope edit/state/build/read/codec implementation and all
affected callers completely. Replace the current large->large retained-extent walk
plus fresh mapping builder with actual stored-node split/concat/coalesce:

```text
Stored subtree ID + checked summary     Owned decoded unfinished node
                    \                  /
                   affected paths and join boundaries
                                |
             preserve reference partitions and reusable IDs
                                |
              prove final -> seal children -> encode/hash -> emit
```

Use one private chosen representation, no generic COW framework. Preserve root/
non-root checks, cumulative offsets, same/different-height joins, coalescing,
half partition, fanout/height bounds, collapse, CDC segmentation, old-root
immutability, no-op semantics and transitions. Keep complete construction for
complete input and necessary representation conversions. Remove the replaced
large->large full mapping walk; no runtime old/new switch, encoded-overlay backup
or error-driven rebuild.

For every emission rule, state why later admitted edits and joins on either side
cannot alter the node. Include exact 80+100 -> 90+90 repartition and unequal-height
cases; a fixture named join-80-100 is not itself proof of that precise precondition.
Retain only unresolved boundaries. Prove a hard simultaneous-capacity bound over
both boundaries, active paths, replacement-builder levels, read ownership and
blocked output. No per-edit draft tree or speculative unreachable emission.

### 3. Verification and completion gates

Required evidence:

- Corrected oracle: identical inputs, exact reference roots/partitions and expected
  surviving subtree IDs for single/multiple edits, joins, height growth/collapse.
- Old roots remain readable; newly emitted objects are reachable from the final
  root; equal-byte/empty edits retain required identities.
- External provider/consumer accounting demonstrates affected-path/boundary work
  for localized cases rather than all retained extents being visited/re-encoded.
  Count necessary no-op reads and replacement scans honestly.
- Bounded many-edit/many-file state, slow consumer and late source/output failure.
- C1-only and integrated routes invoke the same implementation with enabled/disabled
  timer behavior preserved. Do not invent pure CPU time by subtracting wall spans.

Use focused commands as the relevant changes become ready:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_reference
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_single --test edit_batch --test edit_model --test edit_bounds --test edit_noop --test edit_transitions --test edit_timing
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test edit_pipeline --test core_pipeline
python3 core/tools/check_product_boundary.py
```

Do not weaken, ignore or delete a failing equivalence case to get a pass. Keep model
byte equality distinct from reference partition equality. Reuse passed evidence only
under the exact identity rules. Final workspace formatting/Clippy/combined checks
belong to the integration owner; coordinate any local build with active measurements.

Save `stages-3-4-d-report-<UTC>.md` beside this prompt with source identities,
oracle correction, actual algorithm changes, proof/counterexamples, command outputs,
node-work/capacity evidence, per-file LOC and per-commit production accounting.
The completion criterion is implemented D with its correctness/localization/finality
proof, not an oracle alone. Do not close #169 here: final qualification/acceptance
follows in the [integration prompt](stages-3-4-continue-acceptance.md).
