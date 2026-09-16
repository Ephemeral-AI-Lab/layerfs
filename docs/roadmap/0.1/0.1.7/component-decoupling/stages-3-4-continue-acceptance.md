# Prompt: integrate and finish Stages 3–4 acceptance

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

## Copy/paste assignment

Integrate and complete acceptance of [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168)
and [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.
Follow the [shared rules](stages-3-4-continuation-prompt.md) and the full original
handoff/reviewer contract. Consume the [D report](stages-3-4-continue-d.md) and
[pooling report](stages-3-4-continue-pooling.md) produced by those assignments.
Those links identify assignments; locate and read their actual dated reports and
raw evidence. Do not infer completion from the existence of a report file.

You own shared API/policy/manifests, integration fixes, the final case contract,
combined checks/report and issue acceptance. You are not alone: preserve others'
changes and coordinate commits; never stage unfinished unrelated work. Do not start
resource-sensitive checks while another worker is building/measuring.

### 1. Establish what is ready

Require D's corrected identical-input oracle, actual stored-node algorithm, exact
roots/partitions, finality/live-memory argument and localization evidence before
qualifying the integrated edit path. Require actual pooling boundary/reopen/chain
evidence and a precise disposition for collision coverage. An unmet mandatory gate
stays incomplete; do not downgrade it because other tests pass.

Pool-only preparation/qualification can proceed before D. Combined edit measurements
cannot qualify a known-wrong edit result. With available reports, fix only genuine
integration conflicts/defects and rerun their affected regressions. Do not restart
completed components or continue into Stage 5.

### 2. Freeze the final source and comparison contract

Identify the final source tree, dependencies, product/harness/oracle/workload seals,
dirty state, actual policy/schema/format support and all accepted overrides. Validate
that prior evidence matches these identities and operations. Relevant changes make
prior evidence stale for final acceptance; reuse only under the declared policy.

Complete a committed versioned specification/addendum before new campaign work,
including exact cases, physically realized sizes, seeds, cache/index state, single
worker, acknowledgement, numerical/resource gates and budgets. Preserve historical
smoke/failure receipts. Use matching successful v0.1.6 public operations; unmatched
surfaces are non-comparative diagnostics. One observation cannot establish a
distribution. No warm-cache credit, gate relaxation or aggregate benchmark framework.

### 3. Run the final checks and required evidence

Run applicable focused regressions first, then these explicit final checks once the
implementation is stable; do not repeat unchanged passing resource-sensitive cases
without relevant changes. No CI or aggregate preflight wrapper.

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
git diff --check
```

Run the real C1-only, C2-only and integrated examples using their inspected CLI.
Keep example wiring evidence separate from registered performance receipts. Do not
hide or ignore the currently failing edit_reference target; it must pass honestly.

Qualify actual save/read and edit-to-ack costs, required source/base/SQL/codec/pack
work, retained storage and scoped memory. Use the original handoff's cases, including
default/non-default cutoffs, transitions, chains, low/high metadata reuse, boundaries,
reopen and failure cleanup. Audit same-save candidate/cache deviations and #174
timing attribution where they affect new claims. Do not attribute C2 read work to
pure C1 CPU or subtract overlapping spans into invented durations.

### 4. Produce the final reviewer deliverables

Use [stages-3-4-reviewer-handoff.md](stages-3-4-reviewer-handoff.md):

1. Actual ASCII file tree, production LOC before/after/delta per file/directory,
   estimates versus actual, physical caps and exact per-commit migration accounting.
2. Every original #168/#169 criterion mapped to source, tests, raw evidence and
   PASS/FAIL/INCOMPLETE/NOT_RUN, with independent Stage 3/4 verdicts.
3. Concrete remaining simplifications with preserved invariants and measured or
   explicitly estimated effects. No optimization claim from omitted functionality.
4. Component/integrated timing and real-work/storage metrics; separate bounded-memory
   and memory-safety conclusions with explicit instrumentation limits.
5. Supported-limit table: retained revisions versus chain depth; file/object/chunk
   sizes; operation file counts versus workspace membership; directory entry/name/
   path/depth dimensions; logical/history/unique/physical workspace and Store sizes.
   Distinguish enforced/configurable/theoretical/verified/deferred limits. Do not
   invent Workspace/Stage 5 guarantees or call wide integers unlimited support.

Create an additive dated acceptance report and immutable evidence directory. List
every unresolved criterion, including authentic collision coverage if still absent,
without treating the existence of source checks as an executed test. Existing
owner-closed #166/#167 and separate #174 residuals retain their scope.

Close #168 and #169 individually only when each entire acceptance contract has
linked evidence. If a required criterion remains incomplete, retain it and state the
exact next technical action; do not close by description change or silently defer it
to Stage 6. Keep #165 and Stages 5–7 open. Do not release, tag, retire reference code
or implement runtime/cloud work in this assignment.
