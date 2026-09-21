# Structural proof and rejected-candidate test disposition

The focused test passed for both source snapshots. The candidate reduced actual
provider acquisitions and preserved the fixture roots and semantic refusals, but
the retained history diagnostic did not meet the owner's revised one-second
operation-saving criterion. The product treatment and its active test are not
being shipped. No full-suite or stride3 campaign follows this rejected treatment.

## Retained structural results

Both scenarios start with 200 existing even-numbered file inodes in a branched
inode table. Each update adds 129 absent file inodes, changes metadata on existing
inodes 2 and 4, and removes inode 400's binding. The suffix case allocates above
the previous maximum; the interior case uses reserved, previously unbound odd
serials. Both legally satisfy the allocator's absence precondition.

| Scenario | Batch | Baseline pages | Candidate pages | Baseline waves | Candidate waves |
| --- | ---: | ---: | ---: | ---: | ---: |
| Suffix new serials | 1 | 418 | 289 | 415 | 286 |
| Suffix new serials | 32 | 283 | 279 | 278 | 274 |
| Interior new serials | 1 | 813 | 555 | 806 | 548 |
| Interior new serials | 32 | 556 | 547 | 546 | 540 |

Zero-count base-record demands fall from **133 to 4** in all four cells. Pages
are demanded canonical objects and waves are provider method calls, not physical
disk I/O. The seven distinct demanded identities remain the same; the change
reduces repeat acquisitions. The raw identity sets remain in both logs.

The suffix root is identical across both arms and both batch sizes:
`e38505039e540b699e3f975027e906f3a086a8d98d7144e6afa415d776e9095a`.
The interior root is likewise identical:
`7ef4db05cbfdf08670ba5d2d6259317724c7cbb3669082e9cee0720fd734485d`.

Assertions also verify new and modified existing reference counts, supplied
metadata, disappearance of the released existing inode, and exact
`InvalidRecord("reused inode serial")` rejection of an existing serial falsely
declared new. Provider demand waves remain within the public maximum.

The batch-32 suffix case demonstrates why serial-demand savings cannot be
converted into proportional latency savings: removing 129 serial demands avoids
only four root-page acquisitions. Interior misses can remove more pages, but the
same batch size still avoids only nine pages and six waves in this fixture.

## Verification already performed

Both archived sources were checked with the same command:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked \
  -p layerfs-content --test filesystem_zero_count_lookup -- --nocapture
```

Baseline: [source](baseline-structural-test.rs),
[receipt](checks/baseline-structural.json), [output](checks/baseline-structural.log).
Candidate: [source](candidate-structural-test.rs),
[receipt](checks/candidate-structural.json), [output](checks/candidate-structural.log).
Each output records one test passed and zero failed. The candidate complete test
command wall was 3,548,686,458 ns. This is test-command wall, not product operation
time. This disposition did not rerun either command.

## Why the candidate was rejected

The coordinator's retained stride10 result reports **156,679,583 ns** less total
operation time and **53,181,962 ns** less zero-count phase time, with 570 fewer
provider waves/requested objects and unchanged pooled work. The operation saving
is **843,320,417 ns short** of the revised 1,000,000,000 ns criterion. See
[source disposition](candidate-source-disposition.md) for the exact baseline and
candidate totals and their diagnostic qualifications. Passing these structural
checks does not establish the required latency benefit or release admission.

Before removal, the owning test worker compared the active test with
`candidate-structural-test.rs` using direct byte equality: both were exactly
6,474 bytes and identical. The worker then removed only the active
`core/crates/layerfs-content/tests/filesystem_zero_count_lookup.rs`. Archived
sources, outputs and receipts remain unchanged. No product source was edited by
this test worker, and no tests or builds were run during disposition. Removing
the external test changes production LOC by zero.
