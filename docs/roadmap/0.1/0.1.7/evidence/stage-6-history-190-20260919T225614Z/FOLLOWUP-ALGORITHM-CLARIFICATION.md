# #190 follow-up — confirmed work amplification versus unmeasured cause

> Status: Research; informative and not a product contract.

Source rechecked at the unchanged product snapshot `9f35c49ad62956f131dc2676787f99d69659686e`. This addendum corrects an overstatement in S2 and identifies concrete traversal/acquisition patterns. No product code or benchmark was run or changed.

## Confirmed source behavior

- `core/crates/layerfs-content/src/filesystem/sorted/merge.rs:239` loops over every immediate child of an entered branch. It fetches the child batch at line 247, decodes/checks each child at lines 259/266, and only then calls `edit` at line 278. The unchanged-subtree shortcut is inside that callee at line 180. Therefore an unchanged subtree's root page has already been acquired and checked before the shortcut. An updated level-1 root reads all its leaf children even for one changed key. This is source-proven sibling-page read amplification; it is not proof that every history state fully traversed every tree.
- `core/crates/layerfs-content/src/filesystem/update.rs:441` implements `lookup_base` by calling `lookup_many` with exactly one serial (line 447). The per-directory loop calls it at line 214 (or 203), and can call it again when recovering an omitted inode value at line 288. `filesystem/inode/read.rs:101` starts each lookup_many call at the table root. Shared ancestors are shared inside one call, not across these singleton calls. Exact per-state call counts and cost remain unmeasured.
- `core/crates/layerfs-storage/src/cas/read.rs:180` reads the publication ceiling per wave; line 61 obtains locators and line 77 starts a wave-local pack map. The operation retains its connection, decode workspace and decoded-group cache, so repeated lookup does not imply repeated connection opens or an uncached decode every time. It does imply repeated acquisition/control work. These are local SQLite/provider operations, not network/container RPCs.
- `core/crates/layerfs-content/src/filesystem/validate.rs:609` iterates changed directory bindings, and its effective-cycle walk resets its pending/seen state at lines 634–636 for each starting directory. Overlapping subtrees can be visited by separate checks. Existing bounds and validation semantics remain necessary; this source fact does not quantify its contribution to the observed tree envelope.

## Correction to S2's prefix-recursion statement

S2 says the upper-bound-only `Changes::in_range` can cause recursion into prefix subtrees before a changed key. That conclusion is **not supported by the cited condition** and must not be used in synthesis. At `sorted/merge.rs:32–35`, a next changed key greater than the child's upper bound makes `in_range` false. For example, next key 250 and child upper bound 100 does not enter that subtree's descendants. The sorted change stream is consumed in order (`take`, lines 39–43); the absence of an explicit lower bound is not itself evidence of incorrect traversal.

The valid amplification finding is the earlier **fetch/decode of sibling root pages before this check**, which remains true. S2's original document is retained unchanged so the disagreement is visible. This addendum supersedes its prefix-recursion assertion, not its sibling-fetch finding.

## Evidence limit

Archived build/update envelope: 23,520,347,667 ns. Archived harness input assembly, which does scan the complete current path tree: 65,188,166 ns. The latter cannot directly explain the 21,195,388,457-ns descriptive cross-generation excess. No arithmetic error in the product, exact query-count regression, unconditional full-base scan, or per-mechanism time saving has been established. Fresh detailed phase/counter evidence remains NOT_RUN under the unresolved diagnostic ceiling. Algorithm repair remains outside authorization.
