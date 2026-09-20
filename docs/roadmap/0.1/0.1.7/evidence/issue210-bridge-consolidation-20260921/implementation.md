# Bridge codec consolidation

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Parent: `a4a144af8af9b46c3ce3100047466f4817332808` on
`codex/pair2-history-remediation`, draft PR #213. The owner requested consolidation
of repeated bridge codec mechanics after reviewing pair 2's LOC growth.

Four bridge production files change. Optional fixed-width values share the
existing metadata helpers. The five history page encoders and decoders share
one bounded envelope each; explicit tags, per-record minimum widths, record
validation, allocation/check order and result limits stay unchanged. Prepared
changes and manifest entries are constructed directly in wire-field order.
The client loses an unreachable duplicate Fork result match. No DTO, protocol
version, dependency, limit, source scope or C1/C2/C5 transition changes.

Before production edits, the existing fixture builders encoded 46 request,
response and failure cases through the a4a144af production codecs. Their exact
hex bytes are checked by a new external regression alongside existing malformed,
minimum/maximum width, exact-budget, legacy and delivery tests. Focused tests
passed. A new test formatting lint was corrected without changing product code;
its original failed log is retained in the later validation record.

The independent read-only reviewer found no actionable issue. It checked wire
order, presence flags, failure categories, all five count prechecks before record
allocation, per-record validation and the Fork early-return logic. It did not
independently execute tests. Reviewed product diff SHA-256:
`8e8106eace053fd27ffa2b2e1f50cea13b0035c878df5e1e1eed780fb9797ffe`.
The full committed-head checks and route receipts are appended separately.
Existing H04/H06/H08/H14 qualification gaps are unaffected and not waived.

Production LOC: 96344 -> 96268 (delta -76).
Core: 30927 -> 30851 (-76); reference: 65417 -> 65417 (+0).
Bridge: 3734 -> 3658 (-76). Per-file deltas: metadata -23, response -51,
client -3, history_failure +1. Pair 2's net production addition from a02168ad
becomes 5457 (previously 5533). No relocation or reference retirement.

Method: export first parent and final staged trees with `git archive`, then run
`python3 tools/production_loc.py --root <export> --detail --json` on each. Count
first-party Rust/runtime SQL; exclude comments/blanks, legacy inline tests,
external tests, docs, tooling, manifests and artifacts. Counter SHA-256:
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
Committed tree equality is confirmed after committing. This is source size,
not a performance claim.
