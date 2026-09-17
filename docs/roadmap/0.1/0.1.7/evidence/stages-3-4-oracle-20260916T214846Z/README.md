# Stages 3–4 reference oracle fixtures

Produced by `crates/layerfs-content/examples/rope_edit_oracle.rs` running the sealed
v0.1.6 reference revision `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` in a separate
process. `git diff 44cf748 -- crates/layerfs-content/src` is empty, so these
fixtures are the pinned reference's output, not a later revision's. (The claim was
written without the `/src` suffix and was therefore false for the whole path: the
path also carries the two oracle examples, which were added after the sealed
revision. The sealed *source* is what the fixtures depend on, and it is identical.
Corrected 2026-09-17, after the independent review recorded the overstatement.)

Each JSON file records one frozen case: the base root and its page partition, the
edited root and its page partition, the edited logical length, and the reference's
node counters. `candidate-comparison.txt` is the raw output of
`core/crates/layerfs-content/tests/edit_reference.rs`, which builds the same base
with the candidate constructor and applies the same edit. It **passes**: all nine
cases match the reference root, partition and surviving identities. (This paragraph
said it failed, which was true when the fixtures were cut and false once the
stored-node work landed. Corrected 2026-09-17.) The review re-executed the oracle
itself and reproduced all nine fixtures byte-identically; that independent
provenance is in
`evidence/stages-3-4-review-20260916T233008Z/oracle-replay/`.

| Case | Shape |
| --- | --- |
| `join-80-100` | two independently built files joined, then an insert at the seam |
| `untouched-sibling` | one 1 000-byte overwrite in the first quarter of a 300-extent file |
| `interior-multi-level` | a 40 000-byte overwrite in the middle of a 400-extent file |
| `height-growth` | a 3 MB append at end of file |
| `root-collapse` | a deletion leaving one 400 000-byte file |
| `batch-normalized` | three ordered edits (insert, overwrite, delete) in original coordinates |

Interpretation: these are correctness oracles. They are not performance evidence,
not a benchmark campaign, and no timing claim is made from them.
