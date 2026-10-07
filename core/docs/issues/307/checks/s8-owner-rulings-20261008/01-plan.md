# S8 owner rulings P-1 to P-7: record

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Parent `77cf51686`. Documentation only; no product, dependency, harness or
measurement change, and no runtime check.

- On 2026-10-08 the owner asked in the main chat for suggestions on the seven
  pending choices of [specification](../../S8-SPECIFICATION-20261008.md)
  section 15.2. The recommendations were presented, refined on P-3 (a shared
  sealed Store only for samples with no serial reservation) and P-4 (authorize
  the passthrough arm as well as native). The owner replied "proceed" and then
  "just apply fix". The rulings transcribe what was presented.
- Changed: specification 15.2 pointer and new 15.3, plus the three lines that
  cited a pending choice (8.2, 10.1, 12); proof plan section 8 pointer and new
  8.1 with the P-1 oracle; implementation plan (serials row, C2, C6);
  mechanism ledger (rank 3 gate); implementation handoff section 5.
- Not changed: the retained review reports and the original finding ledger,
  which keep their recorded identity.
- Concurrent work: another owner-requested task was editing the same documents
  in this checkout (plan under `../s8-spec-review-fixes-20261008/`). This
  commit contains the parent's text plus the ruling edits only. That task's
  uncommitted changes were left in the working tree, unstaged and unmodified
  except where a ruling edit touched the same file.
- Checks: `02-document-checks.json` (links, anchors, status banners,
  whitespace, identifiers, run on the exact tree committed);
  `03-production-loc.json` (exact first-parent and staged trees).
