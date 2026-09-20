# Parent lookup correctness and work tests

Scope: external public-API tests in
`core/crates/layerfs-content/tests/filesystem_parent_lookup.rs`. No product hooks,
new dependencies, wall-time thresholds or product test code.

The fixture creates five empty directory parents with distinct metadata. Its six
inode records occupy one authenticated inode leaf; the test asserts that fact.
Every operation starts from the same immutable fixture.

## Regression evidence to collect

The first test applies empty binding updates to all five parents, first omitting
and then supplying their existing typed values, at base-read batches 1, 3 and 64.

- Every result must have the fixture's exact canonical root identity.
- Omitted values must demand the same object count as explicit values: the parent
  record already fetched for its content must also serve its metadata.
- Provider object demands must decrease as the three batch widths increase.
- A full batch must require at most six object acquisitions in this fixture.
  This is a structural bound, not a latency or physical-I/O claim.

Expected from source, **not yet measured**: the baseline omitted-value counts are
23/17/15 versus explicit-value counts 18/12/10. An optimized one-leaf path should
need 18/9/6 for either input representation. Baseline and candidate execution logs
must replace prediction with actual observations; unexpected counts are evidence
for investigation, not a reason to inflate the bound.

## Correctness edges

The second test combines an actual parent binding change, a removed empty
directory, unchanged directories, caller metadata precedence, a newly bound
directory, and a declared-new but unreachable directory. Batch widths 1/3/64 must
produce the same canonical root. Readback checks metadata, counts, deleted paths
and absent inode records. Seven directory updates make batch3 finish with a
partial batch.

The third test uses `maximum_pending_records=2` and an external recording ordering
backing. It requires actual spilling, successful cleanup, and the unchanged root
for a no-op update. This catches incorrect accounting caused by inserting final
directory values earlier in the reducer's stream.

Existing `filesystem_failure`, `filesystem_topology`, `filesystem_bounds`,
`filesystem_limits`, `filesystem_reference` and `filesystem_ordering*` tests cover
missing/corrupt records, invalid parents, refusal/resource boundaries, canonical
fixtures and backing behavior. The root coordinator owns running those tests and
all required workspace checks under its exclusive resource schedule.

## Run command

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked \
  -p layerfs-content --test filesystem_parent_lookup -- --nocapture
```

Test preparation status: authored; no build or test process was started by the
test subagent. The root coordinator records baseline/candidate outcomes.

Production LOC contribution of these test/document changes: 0. They are external
tests and investigation documents, excluded from the product counting scope.

## Baseline observed

The root coordinator executed the command above before the product change;
[baseline log](../checks/baseline-structural-test.log) records two correctness
PASS results and the intended structural regression failure. The external provider
observed omitted-value demands **23/17/15**, explicit-value demands **18/12/10**
for batch widths **1/3/64**. The reuse assertion failed because supplying otherwise
identical metadata avoids exactly five object acquisitions. These are counted
public provider calls/objects in a deterministic test, not benchmark timings.

## Constrained ordering follow-up

An independent review identified that inserting final parent values earlier can
change reducer spill cadence. A fourth test, authored before product edits,
adds one shared existing regular file and binds it under all five parents. The
shared child's repeated binding effects can interleave with parent values.

It prints an exact success-root or typed-error row for 56 combinations:
`maximum_pending_records` 1/2, `base_read_batch` 1/64, and `ordering_bytes`
96 × {1,2,3,4,5,6,7,8,9,10,12,16,24,32}. Every successful root must equal an
unconstrained update; refusals must be explicit resource errors with no published
root, and all owned ordering resources must be released. The matrix must contain
both successes and refusals. It does not guess baseline outcomes: the coordinator
will retain and compare the emitted baseline/candidate rows, including exact error
variants and fields.

## V1 rejection and bounded-window V2 regression contract

The coordinator found eight baseline-success cells regressing to resource refusal
in candidate V1. The test now requires successful canonical results for the
recorded baseline boundary cells: pending1 with bytes864/960, and pending2 with
bytes960/1152, each at batch1/64. The error row prints before the assertion so any
future regression retains its exact typed failure. No quota is raised.

V2 keeps the baseline order of all binding effects followed by final parent
values. It retains only the final bounded parent-read batch across that boundary;
it does not retain every parent record for the whole operation. Therefore the
structural assertion of identical omitted/explicit metadata acquisition counts
applies when all five parents fit in batch64. At batch1/3, earlier parent batches
may legitimately be read again. The same canonical roots, strictly lower demand
counts for larger batches and the six-acquisition batch64 ceiling remain required.
This revises an overbroad full-operation caching assumption to preserve bounded
memory and existing quota behavior; it is not a timing-driven threshold change.

Only the owned test file was formatted with `rustfmt +1.85.1 --edition 2021`.
No build/test/benchmark commands were run by this subagent.

## Existing materialization test accounting

The full workspace run exposed an existing total-provider assertion in
`filesystem_bounds::a_branch_materialization_reads_children_in_one_wave`.
That fixture has one directory-parent update, no supplied parent inode value,
and a level-2 inode root. Previously its second parent lookup acquired the root,
a level-1 branch, and a leaf. The retained parent batch now eliminates precisely
that lookup: total provider waves **107 -> 104**, objects **303 -> 300**.
The test now asserts the base inode-root level through the public decoder and
pins these new exact totals. Its large-wave widths **[50,64,64]**, inode-engine
waves **5**, inode-engine pages **134**, and golden canonical root are unchanged.
This is accounting for three eliminated point acquisitions, not a wider threshold
or removal of the materialization proof. The coordinator runs validation.
