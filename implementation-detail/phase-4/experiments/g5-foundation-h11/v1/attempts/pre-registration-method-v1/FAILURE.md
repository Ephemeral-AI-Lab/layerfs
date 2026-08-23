# Preserved pre-registration method failures

These are not measured H11 rows. They occurred before `PREREGISTRATION-v1.md` and before any result root existed.

1. The oracle stopped after generation 2 with `DeltaConflict`. Root cause: revision 3 tried to verify the current non-genesis transition as genesis because same-count publication intentionally carries no authority. Repair: every new transaction now supplies the actual parent root and exact prior replace operation; the 1,001-revision oracle passes.
2. The first revision-1 smoke panicked because eager `then_some` evaluated `1 - 2`. Repair: lazy genesis-parent selection.
3. The repaired smoke reached object accounting and returned `WrongLogicalRole`. Root cause: the reachability helper decoded a namespace's canonical bytes wrapper directly instead of using the retained Store's canonical `get` path. Repair: reuse `Store::get`; the same smoke then passed with exact cleanup.

No semantic gate, timing limit, workload, or retained control was weakened. The final frozen executable includes only those smallest method repairs.
