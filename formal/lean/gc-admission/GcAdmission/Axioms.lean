import GcAdmission.Proofs

/-!
  GcAdmission/Axioms.lean

  The axiom receipt for the main theorem, the §9.4 corollaries of the shipped
  scope, the Inv7/EntriesSound/base-case lemmas, the Inv3/Inv4 witness
  preservation and the force-remove end.  There are 23 receipts.
  `lake env lean GcAdmission/Axioms.lean` prints every closure; the workflow
  feeds that output to `tools/check_lean_axioms.py`, which requires every
  listed declaration and accepts only `propext`, `Classical.choice` and
  `Quot.sound`.  `sorryAx`, a user axiom or a native-decide proof fails the
  check.
-/
open GcAdmission

#print axioms GcAdmission.step_preserves
#print axioms GcAdmission.no_reclaim_live
#print axioms GcAdmission.compact_barrier
#print axioms GcAdmission.reconcile_only_dead
#print axioms GcAdmission.done_is_operator
#print axioms GcAdmission.place_fallback_conservative
#print axioms GcAdmission.conservative_no_death
#print axioms GcAdmission.unordered_never_dead
#print axioms GcAdmission.restore_is_uncertain
#print axioms GcAdmission.ordered_rule
#print axioms GcAdmission.ordered_rule_sequential
#print axioms GcAdmission.compareMachine_uncertain_cases
#print axioms GcAdmission.unordered_place_never_dead
#print axioms GcAdmission.recorded_newer_is_uncertain
#print axioms GcAdmission.done_survives_compact
#print axioms GcAdmission.preview_is_done
#print axioms GcAdmission.Inv7_holds
#print axioms GcAdmission.Inv7_preserves
#print axioms GcAdmission.Inv3_preserves
#print axioms GcAdmission.Inv4_preserves
#print axioms GcAdmission.EntriesSound_preserves
#print axioms GcAdmission.empty_inv
#print axioms GcAdmission.force_remove_ends_mounts
