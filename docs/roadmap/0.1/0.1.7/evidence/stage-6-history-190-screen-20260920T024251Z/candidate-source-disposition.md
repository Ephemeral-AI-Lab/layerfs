# Zero-count candidate source disposition

The candidate is rejected against the owner's revised **1,000,000,000 ns**
stride10 operation-saving threshold, not the earlier two-second screen.

The experiment preserved original touched-serial batch boundaries and omitted
`PendingState::New` from base inode lookups because its final count is already
carried by the reducer. Existing rows consumed the filtered base results in the
same order. The original allocator-absence validation, resource checks and final
zero-count result order remained in place. Both arms included the identical
`zero_count` diagnostic phase wrapper.

| Quantity | Baseline | Candidate | Reduction |
| --- | ---: | ---: | ---: |
| Operation ns | 22,615,178,250 | 22,458,498,667 | 156,679,583 |
| Zero-count phase ns | 1,699,077,376 | 1,645,895,414 | 53,181,962 |
| Provider waves | 66,616 | 66,046 | 570 |
| Provider requested objects | 74,279 | 73,709 | 570 |

These retained one-sample diagnostic results were supplied by the coordinating
agent. Pooled read metrics are unchanged. The operation reduction is
843,320,417 ns short of the one-second threshold. The local phase result and
unchanged pooled work support the structural finding: removing new high-serial
lookups mostly avoids root-only demand waves; the existing-record leaf work
remains. This is not a claim of a portable maximum saving or a separate quantified
indirect cache effect.

Before restoring source, this worker compared `candidate-source.patch` byte for
byte with the current `git diff -- core/crates/layerfs-content/src/filesystem/update.rs`;
they matched exactly. Only that owned source file was then restored to current
HEAD `ca13fb170`, preserving the previously merged improvements. The subsequent
file diff was empty. The retained patch, binaries, identities, tests and run
artifacts were not changed. No further tests, builds or measurements were run
as part of restoration.

No further one-second C1 saving has been established by this bounded experiment.
That is an evidence limit, not a claim that no such optimization can exist. The
failed candidate is retained as evidence rather than shipped or broadened into a
cache redesign. Product source delta after restoration: zero.
