### Milestone 1 repaired exit

- Candidate commit: `88415105459431971974dc8fa421441a63f73e16`
- Validation date: 2026-08-10
- Sequential predecessor: repaired M0 candidate
  `786b418d002a0bf086386bd84d053a20054ec3fd`
- Checklist complete: yes; M0 and M1 only
- Primary environment: Windows 11 64-bit `10.0.26200`, Node `24.11.1`, pnpm `10.32.1`
- Primary command: `pnpm validate:m1`
- Primary result: pass in 54.007 seconds; 4 M0 tests, 13 M1 Node tests, and 9 workerd
  checks passed with zero failures
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the pure-algorithm milestone
- Hosted-CI deviation: no GitHub Actions run exists for this unpushed branch. The exact
  candidate was instead validated locally across Windows and Linux with Node 22 and
  Node 24.
- Approval boundary: stop after M1. M2 is paused and may not resume until an independent
  audit and explicit user approval.

The exact candidate passed `pnpm validate:m1` on Windows Node 24.11.1 in 54.007 seconds
and Windows Node 22.23.2 in 53.006 seconds. Clean Debian bookworm containers also passed
the same sequential gate on Node 22 and Node 24. Every run included the complete
repaired M0 gate, 13 Node algorithm tests, and the workerd golden-vector gate.

Manifest encoding and construction now validate the embedded FastCDC parameters before
writing bytes, enforce maximum chunk length and non-final minimum chunk length, reject
empty internal nodes, and retain a single-entry lookahead rather than the complete
input. Full-tree validation proves exact child totals, canonical leaf/internal grouping
boundaries, balanced leaf depth, the empty-root special case, and absence of a unary
root wrapper. Lookup and sequential cursor construction validate every affected node's
grouping, chunk constraints, declared totals, and depth before returning an entry.

Fixed vectors now cover the 68-byte root envelope, leaf encoding and digest, internal
encoding and digest, grouping states and boundaries, and a complete manifest root in
both Node and workerd. Regression tests reproduce all four audit defects and add
unbalanced-tree, empty-internal, maximum-capacity, maximum-depth, safe-integer overflow,
truncation, extension, reserved-header, impossible-count, and zero-length-record cases
without depending only on digest flips.

The 100,001-entry durable builder case remains bounded to a 17-record keyset page and a
group-sized retained window. Local rebuild, streamed fallback, COW, patch, SHA-256,
FastCDC partitioning, and deterministic full-rebuild comparisons remain green.

`validate:accepted` advances to `pnpm validate:m1` only in the evidence commit after the
candidate passed the full four-cell local matrix. M2 source remains provisional and is
not validated or accepted by this record.
