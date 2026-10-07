# Six S8 specification review corrections

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner request 2026-10-08: fix all six review findings against `77cf51686`.
Documentation only; no product, dependency, harness or measurement change.

- Change S8-SPECIFICATION-20261008.md: separate Exec process/stream/result completion;
  restore indexed lookup custody; require a complete daemon-work drain before
  owner retirement; keep filesystem service during a reversible unmount probe;
  use one explicit connection-abort step and one plain detach attempt; specify
  the actual fuser callback admission wait and receive-slot accounting.
- Reconcile S8-IMPLEMENTATION-PLAN-20261008.md, S8-PROOF-PLAN-20261008.md,
  S8-MECHANISM-EVIDENCE-20261008.md and HANDOFF-S8-IMPLEMENTATION-20261008.md.
  Add targeted counterexample oracles and honest ownership SQL/count costs.
- Preserve original review receipts/decisions at their recorded identity; write
  a new correction ledger here and link the supersession from current documents.
- Reuse the pinned fuser source, S6 lookup/open/processing custody and Linux 6.12
  abort semantics. No new subagents or external messages are needed.
- Check links/anchors, cross-document requirements, source scope and whitespace;
  retain exact first-parent/staged production LOC, then one local commit.
  Preserve protected notes and all earlier evidence; no runtime checks or push.

The concurrent owner-rulings checkpoint `ac576a5fb` became the commit parent
while these corrections were in progress. Preserve its P-1 through P-7 rulings
and their companion-plan edits; they do not replace the six corrections.
