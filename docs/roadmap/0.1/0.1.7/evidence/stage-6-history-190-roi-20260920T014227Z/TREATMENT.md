# Selected local experiment

> Status: Research; informative and not a product contract.

Native sample captured one main-thread graph with7,307 samples;1,759 descend from StoreProvider. Disjoint catalogue-query sample counts: preparation204, execution312, reset87, finalization/drop12, row decode5, other4. Preparation+finalization/drop216 is a credible local target, not a measured exact time saving. The87 reset samples are still required with cached statements; do not count them as removable. Pack preparation43 does not justify broadening the treatment. Source and independent profile review agree.

Freeze one treatment before matched runs: sqlite::pool::group_for reuses the existing16-entry connection statement cache through prepare_cached, retaining exactly the same SQL, binding, row decoding, invalid-ordinal/missing/range errors and fresh-result behavior. No pack_bytes change, data/value cache, lifetime extension, timer/counter, new dependency, worker/policy or capacity change.

A safe external rusqlite authorizer callback counts actual SELECT preparation events on the normal helper: baseline two repeated calls compile twice, candidate once. A row update between calls must be visible, parameters rebound, and deletion/invalid input must preserve errors. Tests first; one unprofiled pair on stride10, confirmstride3 only if the result justifies retaining this tiny change. If it regresses or fails semantics, retain rejection and stop; no second optimization.
