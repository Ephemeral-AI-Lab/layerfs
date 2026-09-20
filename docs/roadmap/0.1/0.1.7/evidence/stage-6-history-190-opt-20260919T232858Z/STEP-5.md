> Status: Research; informative and not a product contract.

## Step 5 — deeper subtree/streaming changes considered and deferred

The measured batching/bounded-reuse change is sufficient to deliver the requested targeted optimization; no additional product mechanism is added in this campaign.

**Subtree summaries:** the candidate inode spans are4,482,754,167ns (stride10) and15,739,979,668ns (stride3), but include lazy reference processing as well as sorted inode work. They do not isolate avoidable sibling reads. Both versions already acquire siblings before pruning; current parents lack all authenticated child facts needed for fill/count/rebalancing. The evidence does not yet justify a format/trust-contract change. Keep existing validation.

**Save streaming:** candidate accept-loop time remains9,448,569,166ns /19,247,494,540ns, but those intervals include actual Store admission, encoding/index work and transactions. The measured object-clone handoff is only49,171,738ns /87,294,823ns. That does not justify assuming streaming removes the multi-second save cost. No added workers, enlarged buffers, reordered persistence or shifted timer boundary.

Remaining filesystem residual is explicitly retained:5,210,486,416ns /15,511,671,619ns. Its read/zero-count/release components need finer evidence before another optimization claim. We do not assign it to a full scan by guesswork.

Independent reviewer reproduced both raw comparisons, all70 roots, exact56 quota parity, identical Store hashes across each pair, source/binary/lock custody and production LOC85,533→85,582(+49). Issue stays open for its remaining qualification gaps (stride3 verification TARGET_MISS, O3 pins missing, uncontrolled cache/no admission). Steps1–4 deliver measured diagnostics and validated code; step5 is the recorded decision to defer unsupported deeper changes. No commit/push or release claim.
