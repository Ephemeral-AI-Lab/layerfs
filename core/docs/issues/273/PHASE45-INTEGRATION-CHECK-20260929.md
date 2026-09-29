# #273 and #245 Phase 4.5 integration check

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Product source `11a864fc133844cae7a4247b1f243d84d5763b10` is unchanged.
> This test/docs revision follows `0ef7e801816f14c70cdb8efbb4e40c120841ef12`.

Live #245/#248 and PR heads were read on 2026-09-29. PR #269 remains draft
at `6eb7553671d3000160ad023a57b335e52dd81a26`; #263 remains open at
`ef3a310480254774d6e6966004fb5e0a4fa3b94d`, stacked on draft #260.
The owned branch already contains #269, its one-seal repair `05eb5c148`,
the C1 ordering repair `fbda0f0f1`, and the subsequent active namespace union
repairs. `git merge-base --is-ancestor <#269 head> HEAD` succeeds. A dry
`git merge-tree --write-tree HEAD <#269 head>` returns the unchanged HEAD
tree. GitHub PR #274 still points to the earlier `29fc5747d` source; the
completed owned branch has not been merged through that PR stack.

The [final edit/Commit proofs](PREMERGE-FUNCTIONAL-COMPLETION-20260929.md)
remain source-pinned. A newly run final-product Linux inherited namespace
suite using its existing locked debug binary passed **10/11**. The one
failure asserted zero completion reservation after rename, while the
approved precharged completion fund reserves exactly **851,968 bytes**.
Source `runtime/state.rs::frontier_counts` reserves this fund before the
first dirty publication. This is a stale custody assertion following the
product change at `40f03ed7f`; it does not justify reverting the reserve.

Local append-only FAIL: `core/target/issue273-integration-review-namespace-01/`.
Result SHA256 and command/binary/image identities remain in that directory.
Its stopped container and owned ext4 volume are retained for failure custody;
no cleanup/refund claim is made for that attempt. The other ten cases passed,
including deep resident/never-resident moves, held ancestry and handles,
detached-parent refusal, growing-prefix counts, quota refusal and G1/G2.

This revision updates the existing one-seal test to verify the exact reserve
before Commit, zero unused slot/byte credit after successful Commit, canonical
old/new bytes and exact allocated/reserved refund at checked close. The final
product, quotas, Budget, workload and numerical receipts are unchanged.
The changed test needs a new clean-source proof before a final recommendation.
Memory/cache qualification stays deferred under the owner's latest direction;
numeric rows remain INELIGIBLE and the frozen matched control stays NOT_RUN.
No PR is merged, no issue closed and no package-scale or release PASS claimed.
