### Milestone 1 status: reopened

The previous M1 exit at `2270870a91e2b07822c7e7b0e6b6c35cb2d75dd3` is superseded and is
not acceptance evidence. An independent audit demonstrated that manifest validation and
lookup accepted noncanonical tree shapes, grouping boundaries, entry lengths, and
builder parameters. M1 remains open until those defects, golden vectors, malformed
encodings, and property cases are repaired and the exact candidate passes
`pnpm validate:m1` across the supported runtime matrix.

M2 remains paused. No M2 validation or acceptance work may resume without explicit
approval after the repaired M1 evidence is independently audited.
