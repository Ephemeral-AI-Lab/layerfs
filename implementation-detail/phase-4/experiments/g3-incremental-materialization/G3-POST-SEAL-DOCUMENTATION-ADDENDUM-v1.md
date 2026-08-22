# G3 post-seal documentation addendum v1

Status: **v1/v2/v3 REVISE; v4 controls only if its linked artifacts report PASS**

Date: 2026-08-22

The immutable v11 evidence package was correctly sealed before the active
human-facing report, baseline, index, and roadmap were refreshed from their
pre-seal wording. This addendum defines a separate, additive documentation
closure outside that read-only package. It does not rewrite the v11 manifest,
terminal, verification, campaign, measurements, or payload files.

The controlling sealed identities are:

| Artifact | SHA-256 |
|---|---|
| Static closure | `6de469522152ee2adf48c05e563fbf75d52cdbc312f4bc898e3d834e8b17c2ee` |
| 66-entry payload manifest | `2950a6698983718e8c386a782b975e1ef807fa7a9ecf95cd59396d2473f3b27e` |
| Terminal record | `222bdc2abef4cd1435c6baec82a35bf05756e1aa385b10ae206bd27f9c6c351a` |
| Terminal verification | `995084a7ae284b940b951d9c67680d61d3ee56b350cac55df546dfcd883f99a8` |

The documentation closure covers exactly the active G3 report, G3 baseline,
baseline index, Phase-4 roadmap, and this addendum. It runs no build, test,
benchmark, campaign, or finalizer. Its commands are limited to rustfmt
verification, document-scoped `git diff --check`, and a read-only verifier for
document hashes, links, stage language, repository custody, and sealed-package
hash/mode/manifest continuity.

The first additive attempt is preserved as
[`G3-POST-SEAL-DOCUMENTATION-CLOSURE-v1.json`](G3-POST-SEAL-DOCUMENTATION-CLOSURE-v1.json)
with its
[`v1 verification`](G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v1.txt). Rustfmt and
document-scoped diff checks passed, but the custody command exited on one
combined document identity/mode/newline assertion that did not identify which
input predicate it observed. The closure is therefore `REVISE`; it is not a
documentation PASS and is not discarded or rewritten. Its closure SHA-256 is
`a2b3e79a5c2652a1fad55184b0993b6cf909cb18bafe195aa6beea1000dbe15a`,
and its verification SHA-256 is
`c5d8d0abceb15144711cd766570de517b386f8819743eb5f9cb86851f300247c`.
The sealed v11 package remained unchanged.

The v2 attempt is preserved as
[`G3-POST-SEAL-DOCUMENTATION-CLOSURE-v2.json`](G3-POST-SEAL-DOCUMENTATION-CLOSURE-v2.json),
with its
[`G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v2.txt`](G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v2.txt).
Its field-specific error exposed the shared verifier defect: the expected G3
baseline digest had one extra trailing `a` and was therefore 65 hexadecimal
characters, while the unchanged file's correct SHA-256 ends in `...569d3`.
v2 is also `REVISE`; its closure SHA-256 is
`ef2edf74a0b9d6fb1d4a37d0a7d563966de5c4f42ec58f4a6aa69a147a808dfc`,
and its verification SHA-256 is
`a3a54b37114d2af1ad292d7be2da54a9aec1dc69c78738012858ed4c27fba9e4`.
The sealed v11 package again remained unchanged.

The v3 attempt is preserved as
[`G3-POST-SEAL-DOCUMENTATION-CLOSURE-v3.json`](G3-POST-SEAL-DOCUMENTATION-CLOSURE-v3.json),
with its
[`G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v3.txt`](G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v3.txt).
It passed every document hash, mode, newline, whitespace, and digest-length
gate, then rejected the baseline because the status predicate required the
words `Phase 4 remains incomplete` on one physical line while the Markdown
correctly wraps them across a newline. v3 is `REVISE`; its closure SHA-256 is
`2576ae384aadcc8d66cc9c88939c7c358dec78a7e1d2bbd3efac57e8010f99f0`,
and its verification SHA-256 is
`65136862e398622ac3dbe7c5759e753729fdb3060eb625bcb1e0505fcd3a0bab`.
The sealed v11 package again remained unchanged.

The controlling fresh attempt is
[`G3-POST-SEAL-DOCUMENTATION-CLOSURE-v4.json`](G3-POST-SEAL-DOCUMENTATION-CLOSURE-v4.json),
with an independent summary in
[`G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v4.txt`](G3-POST-SEAL-DOCUMENTATION-VERIFICATION-v4.txt).
Those v4 artifacts are valid only if they report `PASS`, preserve all three
earlier failures, use whitespace-tolerant stage-language predicates, and prove
the sealed identities above remain unchanged.

This documentation closure records **G3 PASS / G4 READY**. G4 remains
planning-only and **UNSTARTED**; Phase 4 remains incomplete, G5 and G6 remain
pending, and no production or integration acceptance is implied.
