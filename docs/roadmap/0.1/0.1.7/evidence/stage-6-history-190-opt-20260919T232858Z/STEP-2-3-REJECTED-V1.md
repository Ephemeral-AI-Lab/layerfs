> Status: Research; informative and not a product contract.

## Steps 2–3 checkpoint — first implementation rejected by quota parity

Implemented bounded parent batching plus immediate reuse of fetched inode values in one product file. The focused shared-leaf case improved from15 to6 object acquisitions, and canonical/correctness assertions passed. However, comparing all56 quota cells against baseline exposed **8 previously successful configurations now refusing with ObjectLimitExceeded**:

- pending1, ordering bytes864/960, batch1/64;
- pending2, ordering bytes960/1152, batch1/64.

The change inserted directory values into the reducer earlier, increasing transient spill demand. **This candidate is rejected before any performance sample.** No ceiling was increased. The generic matrix test passed because it allowed explicit resource refusals; baseline-supported cells are now being locked into regression assertions.

Revision in progress: preserve original global reducer insertion order and contents map, batch both parent acquisition passes, retain only a bounded final parent-record window for reuse. This intentionally permits batched rereads beyond the retained window; it does not add an unbounded all-parent cache.

Rejected-v1 production LOC: reference65,417 unchanged; core20,116→20,132 (+16); combined85,533→85,549 (+16). Method `tools/production_loc.py`, tests/harness/docs excluded. This is an intermediate rejected snapshot, not the final LOC delta.

Baseline stride3 completed: operation125,277,281,254ns; filesystem99,950,581,082ns; provider reads98,406,450,952ns,210,380waves/230,323objects. Baseline read-back: stride10 work8,124,985,000ns meets10s; stride3 work29,241,424,750ns is TARGET_MISS against20s. Sampled read-back and absent pinned counters do not yield admission.

Evidence includes `candidate-v1-rejected.patch`, `candidate-v1-quota-differential.json`, all check logs and baseline raw files under `stage-6-history-190-opt-20260919T232858Z/`. Independent reviewer confirmed the quota regression. No candidate performance run yet.
