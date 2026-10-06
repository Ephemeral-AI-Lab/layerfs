# Corrected local commit boundary

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first commit-boundary assertion failed: prepared S7-only staged tree
461425fc72987ec3b9f72d53e6853d7b11add192 had combined151591, while the local commit
2ac3b827c also contained S9 history paths and tree9afa3557c34b4a15ac5ebcbdf96f580d3191e8d8.
S9 staging occurred before the pending S7 preparation/commit finished. This was an
agent sequencing error, not a product/test failure. No unrelated paths entered it.

That local-only commit is superseded by an amended combined S7/S9 checkpoint.
Its first parent and final staged tree are recounted with the unchanged authoritative
counter, then the actual committed tree is verified against the new receipt.
The original mismatched S7 receipt and combined observed-tree count are retained
under core/target/cluster2-307/loc; neither is represented as a confirmed commit.
Per-milestone audits/receipts remain separate. No push or user commit is rewritten.
