#!/bin/sh
# Writes the Admission configs (adm_*.cfg). Commit_* configs are hand-written.
cd "$(dirname "$0")" || exit 2
mk() { # name spec kind variant fix fixorphan mounts paths R C T kills losts maxb comment props...
  n=$1 sp=$2 kd=$3 v=$4 f=$5 fo=$6 m=$7 o=$8 r=$9; shift 9
  c=$1 t=$2 k=$3 l=$4 b=$5 cm=$6; shift 6
  { echo "\\* $cm"; echo "SPECIFICATION $sp"; echo CONSTANTS
    echo "  Variant = \"$v\""; echo "  Fix = \"$f\""; echo "  FixOrphan = $fo"
    echo "  FixLate = ${FIXLATE:-FALSE}"; echo "  Mounts = $m"; echo "  OneMsg = \"$o\""; echo "  WithR = $r"
    echo "  WithC = $c"; echo "  WithT = $t"; echo "  MaxKill = $k"
    echo "  MaxLost = $l"; echo "  MaxB = $b"; echo "$kd"
    for p in "$@"; do echo "  $p"; done; } > "$n.cfg"
}
S="KeyedAdmission Exclusion RemovedClean KeysCover"
M2='{"m1", "m2"}'
M1='{"m1"}'
# Safety, full actor set.
mk adm_A        Spec INVARIANTS A none   FALSE "$M2" both TRUE TRUE TRUE 1 1 6 "A (ca4be0bf3c): quarantine for life; two mounts, remove, compaction, another caller." $S
mk adm_B_F1     Spec INVARIANTS B F1     TRUE  "$M2" both TRUE TRUE TRUE 1 1 6 "B + F1 (keys into held_ops in the turn that took them) + orphan noted in that turn." $S
mk adm_B_F2all  Spec INVARIANTS B F2all  TRUE  "$M2" both TRUE TRUE TRUE 1 1 6 "B + F2all (no reopen by any call while a registration is between its keys and hold_op) + orphan fix." $S
# Counterexamples of B as shipped and of the weaker fixes (small actor sets).
mk adm_B_sep    Spec INVARIANTS B none   FALSE "$M1" sep  FALSE FALSE TRUE 1 0 6 "B as shipped: the known race (reopen between session_lock and run_admit)." $S
mk adm_B_one    Spec INVARIANTS B none   FALSE "$M1" one  FALSE FALSE TRUE 1 0 6 "B as shipped: one-message mount, reopen by another call before hold_op." $S
mk adm_B_rm     Spec INVARIANTS B none   FALSE "$M1" one  TRUE FALSE FALSE 1 0 6 "B as shipped: branch.remove's register_separately on S2 meets the same race." $S
mk adm_B_excl   Spec INVARIANTS B none   FALSE "$M1" sep  TRUE TRUE FALSE 1 0 6 "B as shipped: the consequence, removal or compaction proceeding while a mount is live." Exclusion RemovedClean
mk adm_B_F1late Spec INVARIANTS B F1late TRUE  "$M1" sep  FALSE FALSE TRUE 1 0 6 "B + F1 done after the lock's turn ended (not inside it)." $S
mk adm_B_F2     Spec INVARIANTS B F2     TRUE  "$M1" sep  FALSE FALSE TRUE 1 0 6 "B + F2 per call (only the registering call refuses to reopen)." $S
# Liveness under fairness.
L="Termination OrphansRemoved Serves"
mk adm_A_live     FairSpec PROPERTIES A none  FALSE "$M2" both FALSE TRUE TRUE 1 1 6 "A, liveness: every call answers (a quarantined writer refuses definitely)." Termination
mk adm_A_orphans  FairSpec PROPERTIES A none  FALSE "$M1" both FALSE FALSE TRUE 0 1 6 "A, liveness: an orphaned mount row is never removed while the service lives (documented at A)." OrphansRemoved
mk adm_B_live     FairSpec PROPERTIES B none  FALSE "$M1" both FALSE FALSE TRUE 0 1 6 "B as shipped, liveness." $L
mk adm_B_F1_live  FairSpec PROPERTIES B F1    TRUE  "$M1" both FALSE FALSE TRUE 0 1 6 "B + F1 + orphan noted in-turn, liveness (late commit of a not-proven-gone backend)." $L
FIXLATE=TRUE mk adm_B_F1_fixed_live FairSpec PROPERTIES B F1 TRUE "$M2" both FALSE TRUE TRUE 1 1 6 "B + F1 + both orphan fixes, liveness." $L
FIXLATE=TRUE mk adm_B_F1_fixed Spec INVARIANTS B F1 TRUE "$M2" both TRUE TRUE TRUE 1 1 6 "B + F1 + both orphan fixes, safety." $S
mk adm_B_term     FairSpec PROPERTIES B none  FALSE "$M2" both FALSE TRUE TRUE 1 1 6 "B as shipped, liveness: every call answers; a quarantined writer serves again or is definitely stale." Termination Serves
# One reopen per register_mount call (several session turns).
mk adm_B_reopen1  Spec INVARIANTS B none  FALSE "$M1" sep FALSE FALSE FALSE 1 1 6 "B as shipped: reopens within one register_mount call." OneReopenPerCall
FIXLATE=TRUE mk adm_B_F1_reopen1 Spec INVARIANTS B F1 TRUE "$M1" sep FALSE FALSE FALSE 1 1 6 "B + F1 + orphan fixes: reopens within one register_mount call." OneReopenPerCall

# Commit configs (Commit.tla).
mkc() { # name variant mutation Ws Force Ks commits krounds kills lanekills losts bumps maxb comment
  n=$1 v=$2 mu=$3 ws=$4 fo=$5 ks=$6 mc=$7 kr=$8 k=$9; shift 9
  lk=$1 l=$2 bm=$3 b=$4 cm=$5
  { echo "\\* $cm"; echo "SPECIFICATION Spec"; echo CONSTANTS
    echo "  Variant = \"$v\""; echo "  Mutation = \"$mu\""; echo "  Ws = $ws"
    echo "  Force = $fo"; echo "  Ks = $ks"; echo "  MaxCommits = $mc"
    echo "  KRounds = $kr"; echo "  Blk = 2"; echo "  MaxKill = $k"
    echo "  MaxLaneKill = $lk"; echo "  MaxLost = $l"; echo "  MaxBump = $bm"
    echo "  MaxB = $b"; echo INVARIANTS
    for p in NoLostUpdate ForceOnlyDisplaces TotalOrder AttemptOnce ReportsExact \
             FenceHolds LaneLiveness IdsUnique; do echo "  $p"; done; } > "$n.cfg"
}
W='{"w1", "w2"}'
K='{"k1", "k2"}'
mkc commit_A         A none        "$W" '{"w2"}' '{}' 3 1 1 1 2 1 8 "A: two Workspaces (w2 forces), three Commits each; a writer kill, a lane kill, two lost replies, a generation bump."
mkc commit_B         B none        "$W" '{"w2"}' '{}' 3 1 1 1 2 1 8 "B: the same with writer reopen."
mkc commit_B_lost2   B none        "$W" '{}'     '{}' 2 1 1 0 2 0 7 "B: two lost replies and a kill, no force."
mkc ids_A            A none        '{}' '{}' "$K" 1 2 1 0 1 0 6 "A: two allocator handles, two rounds each, a kill and a lost reply."
mkc ids_B            B none        '{}' '{}' "$K" 1 2 1 0 1 0 6 "B: the same with writer reopen."
mkc ids_B_lost2      B none        '{}' '{}' "$K" 1 2 0 0 2 0 6 "B: two lost replies (late reservations and rewinds)."
# Mutations: each removes one mechanism and must produce a counterexample.
mkc commit_mut_nobarrier   B nobarrier   "$W" '{}' '{}' 2 1 0 1 0 0 6 "Mutation: no lane barrier (a new lane holder does not wait for nested writes)."
mkc commit_mut_nolanecheck B nolanecheck "$W" '{}' '{}' 2 1 0 1 0 0 6 "Mutation: no pg_xact_status lane check in the nested COMMIT."
mkc commit_mut_noend       B noend       "$W" '{}' '{}' 2 1 0 0 1 0 6 "Mutation: resolution does not end the old backend before the lane pass."
mkc commit_mut_nofence     B nofence     "$W" '{}' '{}' 2 1 0 0 0 1 6 "Mutation: no generation fence."
mkc ids_mut_nocas          B nocas       '{}' '{}' "$K" 1 2 0 0 0 0 6 "Mutation: the rewind sets the highwater without compare-and-set."
FIXLATE=TRUE mk adm_B_F1_reach Spec INVARIANTS B F1 TRUE "$M1" sep FALSE FALSE TRUE 1 0 6 "Reachability probe (must be violated): under F1 a registration that reopened mid-call is mounted." NoMountAfterReopen
