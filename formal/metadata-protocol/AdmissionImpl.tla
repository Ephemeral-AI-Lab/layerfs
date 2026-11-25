---------------------------- MODULE AdmissionImpl ----------------------------
(***************************************************************************)
(* GC admission with the Store service writer's reopen as implemented at  *)
(* 2b0dee1a1f (claude/811-reopen-fix, parent 1a7338ffa2): the variant      *)
(* "B + implemented fix" of #811 V1, modelled turn by turn.                *)
(*                                                                         *)
(* Against Admission.tla's B this adds what 2b0dee1a1f changed:            *)
(*  - session_lock enters a fresh op in held_ops inside its own turn and   *)
(*    tries every key; a refused statement's partial grants and a          *)
(*    re-registration's level are given back in a later turn, on the       *)
(*    connection that took them only (session_unlock's `on`);             *)
(*  - the one-message admission holds the op (or notes the orphan) inside  *)
(*    its decided turn; a Holding admission gives its keys back with `on`; *)
(*  - a failed registration abandons its op; session_give_back drops it    *)
(*    and unlocks it in one turn; a reopen drops abandoned ops;            *)
(*  - an owner release (unmount) runs begin_release outside any turn, and  *)
(*    its message's turn runs the delete, the unlock and end_release;      *)
(*  - orphans are noted in the turn that saw Unknown and released only     *)
(*    when no lost backend is left, by a reopen or by any later turn       *)
(*    (Session::ready release_waiting_orphans);                            *)
(*  - a mounted Workspace may register its op a second time.               *)
(* Mutations: "noon" (session_unlock ignores `on`), "lateend" (the         *)
(* release's end_release after its turn, as at 1a7338ffa2).                *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
    Mut,         \* "none" | "noon" | "lateend" | "retakeabandoned"
    Mounts,      \* mount actors on S1
    OneMsg,      \* "one" | "sep" | "both": a mount's first admission path
    ReReg,       \* a mounted Workspace registers its op once more
    WithR, WithC, WithT,
    MaxKill, MaxLost, MaxB

Svcs == {"S1", "S2", "SC"}
NoReq == <<"-", "-">>
Backends == 1..MaxB
KeyNames == {"S", "B"}
Modes == {"sh", "ex"}
ReopenSvc == {"S1", "S2"}
Actors == Mounts \cup (IF WithR THEN {"R"} ELSE {})
                 \cup (IF WithC THEN {"C"} ELSE {})
                 \cup (IF WithT THEN {"T"} ELSE {})
SvcOf(a) == IF a \in Mounts \/ a = "T" THEN "S1"
            ELSE IF a = "R" THEN "S2" ELSE "SC"
KeysOf(op) == IF op = "R" THEN {<<"S", "sh">>, <<"B", "ex">>}
              ELSE IF op = "C" THEN {<<"S", "ex">>}
              ELSE {<<"S", "sh">>, <<"B", "sh">>}
StartPaths == CASE OneMsg = "one" -> {"one"} [] OneMsg = "sep" -> {"sep"}
                [] OneMsg = "both" -> {"one", "sep"}

VARIABLES
    alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
    held,     \* held_ops per session
    aband,    \* held ops marked abandoned
    rel,      \* held ops whose release is running (HeldOp::releasing)
    orphans,
    pc, phase,  \* per actor; phase: "reg" | "rereg" | "done" (mounts)
    fresh,    \* the op entered held_ops at this registration's lock
    owedB, owedK, \* levels a later turn gives back, and the connection
    turn, pend, kills, losts,
    unkeyed   \* ghost: an intent row committed without its keys

vars == <<alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
          held, aband, rel, orphans, pc, phase, fresh, owedB, owedK, turn,
          pend, kills, losts, unkeyed>>

----------------------------------------------------------------------------
Compatible(f, b, k, m) ==
    \A c \in Backends \ {b} :
        IF m = "ex" THEN f[<<c, k, "sh">>] = 0 /\ f[<<c, k, "ex">>] = 0
                    ELSE f[<<c, k, "ex">>] = 0
GrantableIn(f, b, ops) ==
    \A op \in ops : \A km \in KeysOf(op) : Compatible(f, b, km[1], km[2])
GrantsIn(f, b, op) == {km \in KeysOf(op) : Compatible(f, b, km[1], km[2])}
AddKeys(f, b, ks) ==
    [x \in DOMAIN f |-> IF x[1] = b /\ <<x[2], x[3]>> \in ks THEN f[x] + 1 ELSE f[x]]
PlusAll(f, b, ops) ==
    [x \in DOMAIN f |-> IF x[1] = b
        THEN f[x] + Cardinality({op \in ops : <<x[2], x[3]>> \in KeysOf(op)})
        ELSE f[x]]
SubKeys(f, b, ks) ==
    [x \in DOMAIN f |-> IF x[1] = b /\ <<x[2], x[3]>> \in ks /\ f[x] > 0
                        THEN f[x] - 1 ELSE f[x]]
Drop(f, bs) == [x \in DOMAIN f |-> IF x[1] \in bs THEN 0 ELSE f[x]]
Holds(f, b, op) == \A km \in KeysOf(op) : f[<<b, km[1], km[2]>>] > 0

MountRows == rows \cap Mounts
Conflicting == "R" \in rows \/ "C" \in rows

----------------------------------------------------------------------------
\* Writer turns that pass Session::ready (may reopen, may settle orphans).
SessionPcs == {"one", "hunlock", "sep", "lrefunlock", "locked", "giveback",
               "unlockowed", "urel", "rlock", "rlunlock", "rreg", "rgive",
               "rmark", "rrel", "tcall"}
Terminal == {"failed", "unmounted", "umfailed", "rdone", "rfailed",
             "cdone", "cfailed"}

\* Where a registration's call ends: a re-registration returns to mounted.
Back(a) == IF a \in Mounts /\ phase[a] = "rereg" THEN "mounted" ELSE "failed"

FailPc(a) ==
    CASE pc[a] \in {"one", "hunlock", "sep", "lrefunlock", "giveback",
                    "unlockowed"} -> Back(a)
      [] pc[a] \in {"locked", "scanned"} ->
              IF fresh[a] THEN "abandon" ELSE "unlockowed"
      [] pc[a] = "urel"       -> "umfailed"
      [] pc[a] = "rlock"      -> "rfailed"
      [] pc[a] = "rlunlock"   -> "rfailed"
      [] pc[a] = "rreg"       -> "rabandon"
      [] pc[a] = "rgive"      -> "rfailed"
      [] pc[a] = "rmark"      -> "rrel"
      [] pc[a] = "rrel"       -> "rfailed"
      [] pc[a] = "tcall"      -> "tidle"
      [] OTHER                -> "failed"

\* A call (or this turn of it) fails.  Owed levels are moot then (the
\* connection they were on is closed) except before the unlockowed turn;
\* a release never sent ends with end_release(kept = true).
Fail(a) ==
    /\ pc' = [pc EXCEPT ![a] = FailPc(a)]
    /\ IF FailPc(a) = "unlockowed" THEN UNCHANGED <<owedB, owedK>>
       ELSE /\ owedB' = [owedB EXCEPT ![a] = 0]
            /\ owedK' = [owedK EXCEPT ![a] = {}]
    /\ rel' = IF pc[a] = "urel" THEN rel \ {a} ELSE rel
    /\ phase' = IF a \in Mounts /\ phase[a] = "rereg" /\ Back(a) = "mounted"
                   /\ FailPc(a) = "mounted"
                THEN [phase EXCEPT ![a] = "done"] ELSE phase

Quarantine(s) ==
    /\ quar' = [quar EXCEPT ![s] = TRUE]
    /\ lost' = [lost EXCEPT ![s] = IF quar[s] THEN @ ELSE @ \cup {conn[s]}]

SettleDue(s) ==
    /\ s \in ReopenSvc /\ orphans[s] # {}
    /\ \A b \in lost[s] \cap alive : pend[b] = NoReq

Ready(a) ==
    LET s == SvcOf(a) IN
    /\ turn[s] \in {"none", a}
    /\ ~quar[s]
    /\ conn[s] \in alive
    /\ ~SettleDue(s)

\* Orphan noted inside the turn that saw Unknown, for a new op only.
NoteOrphan(a, s) ==
    orphans' = IF a \in Mounts /\ a \notin held[s]
               THEN [orphans EXCEPT ![s] = @ \cup {a}] ELSE orphans

LocalOnly == <<alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
               orphans, turn, pend, kills, losts, unkeyed>>

----------------------------------------------------------------------------
(* Session::ready.                                                         *)

SendDead(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] \in SessionPcs \cup {"scanned"}
    /\ turn[s] \in {"none", a} /\ ~quar[s] /\ conn[s] \notin alive
    /\ Quarantine(s)
    /\ Fail(a)
    \* inside run_admit an Unknown notes the fresh op as an orphan
    /\ orphans' = IF pc[a] = "scanned" /\ fresh[a]
                  THEN [orphans EXCEPT ![s] = @ \cup {a}] ELSE orphans
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, rows, removed, gen, sgen, held,
                   aband, fresh, pend, kills, losts, unkeyed>>

\* reopen.rs settle (steps 1-4), at most once per turn.
Reopen(a) ==
    LET s == SvcOf(a)
        lost1 == IF quar[s] THEN lost[s] ELSE lost[s] \cup {conn[s]}
        nb == nextB
        \* retake_keys drops abandoned ops ("retakeabandoned": takes them too)
        keep == IF Mut = "retakeabandoned" THEN held[s] ELSE held[s] \ aband
        aband1 == IF Mut = "retakeabandoned" THEN aband ELSE aband \ held[s]
    IN
    /\ s \in ReopenSvc /\ pc[a] \in SessionPcs
    /\ turn[s] = "none" /\ (quar[s] \/ conn[s] \notin alive)
    /\ nb \in Backends
    /\ \E NG \in SUBSET {b \in lost1 \cap alive : pend[b] # NoReq} :
       LET gone == (lost1 \cap alive) \ NG
           adv1 == Drop(adv, gone)
       IN
       /\ lost' = [lost EXCEPT ![s] = NG]
       /\ pend' = [b \in Backends |-> IF b \in gone THEN NoReq ELSE pend[b]]
       /\ IF sgen[s] = gen /\ GrantableIn(adv1, nb, keep)
          THEN /\ nextB' = nb + 1
               /\ conn' = [conn EXCEPT ![s] = nb]
               /\ alive' = (alive \ gone) \cup {nb}
               /\ adv' = PlusAll(adv1, nb, keep)
               /\ held' = [held EXCEPT ![s] = keep]
               /\ aband' = aband1
               /\ quar' = [quar EXCEPT ![s] = FALSE]
               \* step 4 only once step 1 left no lost backend
               /\ IF NG = {}
                  THEN /\ rows' = rows \ orphans[s]
                       /\ orphans' = [orphans EXCEPT ![s] = {}]
                  ELSE UNCHANGED <<rows, orphans>>
               /\ turn' = [turn EXCEPT ![s] = a]
               /\ UNCHANGED <<pc, phase, owedB, owedK, rel>>
          ELSE /\ UNCHANGED <<nextB, conn, rows, orphans>>
               /\ alive' = alive \ gone
               /\ adv' = adv1
               \* a refused re-take (step 3) has already dropped the abandoned
               /\ IF sgen[s] = gen
                  THEN /\ held' = [held EXCEPT ![s] = keep]
                       /\ aband' = aband1
                  ELSE UNCHANGED <<held, aband>>
               /\ quar' = [quar EXCEPT ![s] = TRUE]
               /\ turn' = [turn EXCEPT ![s] = "none"]
               /\ Fail(a)
    /\ UNCHANGED <<removed, gen, sgen, fresh, kills, losts, unkeyed>>

\* Session::ready release_waiting_orphans: a serving session with orphans
\* waiting ends the lost backends again; none left, it releases them
\* (generation-checked delete; a stale one deletes nothing).
Settle(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] \in SessionPcs /\ turn[s] \in {"none", a} /\ ~quar[s]
    /\ conn[s] \in alive /\ s \in ReopenSvc /\ orphans[s] # {}
    /\ \E NG \in SUBSET {b \in lost[s] \cap alive : pend[b] # NoReq} :
       LET gone == (lost[s] \cap alive) \ NG IN
       /\ alive' = alive \ gone
       /\ adv' = Drop(adv, gone)
       /\ pend' = [b \in Backends |-> IF b \in gone THEN NoReq ELSE pend[b]]
       /\ lost' = [lost EXCEPT ![s] = NG]
       /\ IF NG = {}
          THEN /\ rows' = IF sgen[s] = gen THEN rows \ orphans[s] ELSE rows
               /\ orphans' = [orphans EXCEPT ![s] = {}]
          ELSE UNCHANGED <<rows, orphans>>
    /\ UNCHANGED <<nextB, conn, quar, removed, gen, sgen, held, aband, rel, pc,
                   phase, fresh, owedB, owedK, turn, kills, losts, unkeyed>>

----------------------------------------------------------------------------
(* Mounts.                                                                 *)

\* One-message admission (history/mount.rs admit, decided turn).  The lock
\* statement gives back partial grants in the statement itself (Released).
\* An existing op's row (a re-registration) fails the insert: Holding.
MOne(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "one" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ Fail(a)
            /\ UNCHANGED <<adv, rows, held>>
       ELSE IF ~GrantableIn(adv, b, {a})
       THEN /\ pc' = [pc EXCEPT ![a] = IF phase[a] = "rereg" THEN "sep" ELSE "sep"]
            /\ UNCHANGED <<adv, rows, held, owedB, owedK, rel, phase>>
       ELSE IF removed \/ Conflicting \/ a \in rows
       THEN \* Holding: the keys stay until session_unlock(.., on) next turn
            /\ adv' = PlusAll(adv, b, {a})
            /\ owedB' = [owedB EXCEPT ![a] = b]
            /\ owedK' = [owedK EXCEPT ![a] = KeysOf(a)]
            /\ pc' = [pc EXCEPT ![a] = "hunlock"]
            /\ UNCHANGED <<rows, held, rel, phase>>
       ELSE \* Admitted: held in the decided turn (hold_new_op)
            /\ adv' = PlusAll(adv, b, {a})
            /\ rows' = rows \cup {a}
            /\ held' = [held EXCEPT ![s] = @ \cup {a}]
            /\ pc' = [pc EXCEPT ![a] = "mounted"]
            /\ UNCHANGED <<owedB, owedK, rel, phase>>
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, aband,
                   orphans, fresh, pend, kills, losts, unkeyed>>

MOneLost(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "one" /\ Ready(a) /\ losts < MaxLost /\ sgen[s] = gen
    /\ losts' = losts + 1
    /\ \/ /\ GrantableIn(adv, b, {a}) /\ ~removed /\ ~Conflicting /\ a \notin rows
          /\ adv' = PlusAll(adv, b, {a})
          /\ rows' = rows \cup {a}
          /\ UNCHANGED pend
       \/ /\ pend' = [pend EXCEPT ![b] = <<a, "one">>]
          /\ UNCHANGED <<adv, rows>>
    /\ Quarantine(s)
    /\ NoteOrphan(a, s)
    /\ Fail(a)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, removed, gen, sgen, held, aband, fresh,
                   kills, unkeyed>>

\* session_unlock(keys, Some(on)) in a later turn: sends nothing when the
\* session reopened since ("noon": unlocks on the current connection).
UnlockOwed(a, next) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ adv' = IF owedB[a] = b \/ Mut = "noon" THEN SubKeys(adv, b, owedK[a]) ELSE adv
    /\ owedB' = [owedB EXCEPT ![a] = 0]
    /\ owedK' = [owedK EXCEPT ![a] = {}]
    /\ pc' = [pc EXCEPT ![a] = next]

MHoldingUnlock(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "hunlock" /\ Ready(a)
    /\ UnlockOwed(a, "sep")
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   held, aband, rel, orphans, phase, fresh, pend, kills, losts,
                   unkeyed>>

\* register_separately, turn 1: session_lock tries every key; all granted
\* at its generation: a new op enters held_ops in this turn (fresh), a held
\* op's new level is owed; partial grants are owed to a later unlock.
MLock(a) ==
    LET s == SvcOf(a) b == conn[s] G == GrantsIn(adv, b, a) IN
    /\ pc[a] = "sep" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ Fail(a)
            /\ UNCHANGED <<adv, held, fresh>>
       ELSE /\ adv' = AddKeys(adv, b, G)
            /\ IF G = KeysOf(a)
               THEN IF a \notin held[s]
                    THEN /\ held' = [held EXCEPT ![s] = @ \cup {a}]
                         /\ fresh' = [fresh EXCEPT ![a] = TRUE]
                         /\ pc' = [pc EXCEPT ![a] = "locked"]
                         /\ UNCHANGED <<owedB, owedK>>
                    ELSE /\ fresh' = [fresh EXCEPT ![a] = FALSE]
                         /\ owedB' = [owedB EXCEPT ![a] = b]
                         /\ owedK' = [owedK EXCEPT ![a] = G]
                         /\ pc' = [pc EXCEPT ![a] = "locked"]
                         /\ UNCHANGED held
               ELSE /\ owedB' = [owedB EXCEPT ![a] = b]
                    /\ owedK' = [owedK EXCEPT ![a] = G]
                    /\ pc' = [pc EXCEPT ![a] = "lrefunlock"]
                    /\ UNCHANGED <<held, fresh>>
            /\ UNCHANGED <<rel, phase>>
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   aband, orphans, pend, kills, losts, unkeyed>>

\* A refused lock statement's grants given back (session_lock's Err arms).
MRefusedUnlock(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "lrefunlock" /\ Ready(a)
    /\ UnlockOwed(a, Back(a))
    /\ phase' = IF phase[a] = "rereg" THEN [phase EXCEPT ![a] = "done"] ELSE phase
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   held, aband, rel, orphans, fresh, pend, kills, losts, unkeyed>>

\* turn 2 (run_admit): fence, probe, conflict scan; held to its COMMIT.
\* A re-registration's insert fails on its existing row (definite).
MScan(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "locked" /\ Ready(a)
    /\ IF sgen[s] # gen \/ removed \/ Conflicting \/ ~fresh[a]
       THEN /\ Fail(a)
            /\ turn' = [turn EXCEPT ![s] = "none"]
       ELSE /\ pc' = [pc EXCEPT ![a] = "scanned"]
            /\ turn' = [turn EXCEPT ![s] = a]
            /\ UNCHANGED <<owedB, owedK, rel, phase>>
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
                   held, aband, orphans, fresh, pend, kills, losts, unkeyed>>

MCommit(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "scanned" /\ turn[s] = a /\ ~quar[s] /\ b \in alive
    /\ rows' = rows \cup {a}
    /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, a))
    /\ pc' = [pc EXCEPT ![a] = "mounted"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, removed, gen, sgen, held,
                   aband, rel, orphans, phase, fresh, owedB, owedK, pend, kills,
                   losts>>

\* A lost Admit reply: run_mode notes the fresh op as an orphan in-turn.
MCommitLost(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "scanned" /\ turn[s] = a /\ ~quar[s] /\ b \in alive
    /\ losts < MaxLost
    /\ losts' = losts + 1
    /\ \/ /\ rows' = rows \cup {a}
          /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, a))
          /\ UNCHANGED pend
       \/ /\ pend' = [pend EXCEPT ![b] = <<a, "commit">>]
          /\ UNCHANGED <<rows, unkeyed>>
    /\ Quarantine(s)
    /\ orphans' = [orphans EXCEPT ![s] = @ \cup {a}]
    /\ Fail(a)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, removed, gen, sgen, held, aband,
                   fresh, kills>>

\* session_give_back, outside any turn: abandon_op.
MAbandon(a) ==
    /\ pc[a] = "abandon"
    /\ aband' = IF a \in held[SvcOf(a)] THEN aband \cup {a} ELSE aband
    /\ pc' = [pc EXCEPT ![a] = "giveback"]
    /\ UNCHANGED <<LocalOnly, held, rel, phase, fresh, owedB, owedK>>

\* ... and in its turn take_abandoned: still held and abandoned (no reopen
\* since), its keys are on this connection; drop it and unlock them.
MGiveBack(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "giveback" /\ Ready(a)
    /\ IF a \in held[s] /\ a \in aband
       THEN /\ held' = [held EXCEPT ![s] = @ \ {a}]
            /\ aband' = aband \ {a}
            /\ adv' = SubKeys(adv, b, KeysOf(a))
       ELSE UNCHANGED <<held, aband, adv>>
    /\ pc' = [pc EXCEPT ![a] = Back(a)]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen, rel,
                   orphans, phase, fresh, owedB, owedK, pend, kills, losts, unkeyed>>

\* A re-registration's own level, given back with `on`.
MUnlockOwed(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "unlockowed" /\ Ready(a)
    /\ UnlockOwed(a, Back(a))
    /\ phase' = IF phase[a] = "rereg" THEN [phase EXCEPT ![a] = "done"] ELSE phase
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   held, aband, rel, orphans, fresh, pend, kills, losts, unkeyed>>

\* A mounted Workspace registers its op once more (register_mount).
MReReg(a) ==
    /\ pc[a] = "mounted" /\ ReReg /\ phase[a] = "reg"
    /\ phase' = [phase EXCEPT ![a] = "rereg"]
    /\ \E p \in StartPaths : pc' = [pc EXCEPT ![a] = p]
    /\ UNCHANGED <<LocalOnly, held, aband, rel, fresh, owedB, owedK>>

\* authority.rs release: begin_release outside any turn.
MBeginRelease(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "mounted" /\ phase[a] # "rereg"
    /\ rel' = IF a \in held[s] /\ a \notin rel THEN rel \cup {a} ELSE rel
    /\ pc' = [pc EXCEPT ![a] = "urel"]
    /\ UNCHANGED <<LocalOnly, held, aband, phase, fresh, owedB, owedK>>

\* The owner release's message (release.rs): delete (generation-checked),
\* unlock, and end_release in its judge, all in this turn.
EndRelease(a, s, kept) ==
    IF Mut = "lateend"
    THEN UNCHANGED <<held, rel>>
    ELSE /\ held' = IF kept THEN held ELSE [held EXCEPT ![s] = @ \ {a}]
         /\ rel' = rel \ {a}

MRelease(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "urel" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ UNCHANGED <<rows, adv>>
            /\ EndRelease(a, s, TRUE)
            /\ pc' = [pc EXCEPT ![a] = IF Mut = "lateend" THEN "uendkept" ELSE "umfailed"]
       ELSE /\ rows' = rows \ {a}
            /\ adv' = IF a \in rel THEN SubKeys(adv, b, KeysOf(a)) ELSE adv
            /\ EndRelease(a, s, FALSE)
            /\ pc' = [pc EXCEPT ![a] = IF Mut = "lateend" THEN "uend" ELSE "unmounted"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, aband,
                   orphans, phase, fresh, owedB, owedK, pend, kills, losts, unkeyed>>

\* Its reply lost: the delete ran or not; an unknown outcome ends the
\* release not kept (decided.1), the connection closed with every key.
MReleaseLost(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "urel" /\ Ready(a) /\ losts < MaxLost /\ sgen[s] = gen
    /\ losts' = losts + 1
    /\ \/ rows' = rows \ {a}
       \/ UNCHANGED rows
    /\ Quarantine(s)
    /\ IF Mut = "lateend"            \* 1a7338ffa2: kept on an unknown outcome
       THEN /\ UNCHANGED <<held, rel>>
            /\ pc' = [pc EXCEPT ![a] = "uendkept"]
       ELSE /\ EndRelease(a, s, FALSE)
            /\ pc' = [pc EXCEPT ![a] = "umfailed"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, removed, gen, sgen, aband, orphans,
                   phase, fresh, owedB, owedK, pend, kills, unkeyed>>

\* lateend: end_release after the turn ended (as at 1a7338ffa2).
MLateEnd(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] \in {"uend", "uendkept"}
    /\ held' = IF pc[a] = "uend" THEN [held EXCEPT ![s] = @ \ {a}] ELSE held
    /\ rel' = rel \ {a}
    /\ pc' = [pc EXCEPT ![a] = IF pc[a] = "uend" THEN "unmounted" ELSE "umfailed"]
    /\ UNCHANGED <<LocalOnly, aband, phase, fresh, owedB, owedK>>

----------------------------------------------------------------------------
(* branch.remove on S2: register_separately (lane write), marker, release. *)

RLock(a) ==
    LET s == SvcOf(a) b == conn[s] G == GrantsIn(adv, b, "R") IN
    /\ pc[a] = "rlock" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ Fail(a)
            /\ UNCHANGED <<adv, held, fresh>>
       ELSE /\ adv' = AddKeys(adv, b, G)
            /\ IF G = KeysOf("R")
               THEN /\ held' = [held EXCEPT ![s] = @ \cup {"R"}]
                    /\ fresh' = [fresh EXCEPT ![a] = TRUE]
                    /\ pc' = [pc EXCEPT ![a] = "rreg"]
                    /\ UNCHANGED <<owedB, owedK>>
               ELSE /\ owedB' = [owedB EXCEPT ![a] = b]
                    /\ owedK' = [owedK EXCEPT ![a] = G]
                    /\ pc' = [pc EXCEPT ![a] = "rlunlock"]
                    /\ UNCHANGED <<held, fresh>>
            /\ UNCHANGED <<rel, phase>>
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   aband, orphans, pend, kills, losts, unkeyed>>

RRefusedUnlock(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "rlunlock" /\ Ready(a)
    /\ UnlockOwed(a, "rfailed")
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   held, aband, rel, orphans, phase, fresh, pend, kills, losts,
                   unkeyed>>

RReg(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "rreg" /\ Ready(a)
    /\ IF sgen[s] # gen \/ MountRows # {} \/ "C" \in rows
       THEN /\ Fail(a)
            /\ UNCHANGED <<rows, unkeyed>>
       ELSE /\ rows' = rows \cup {"R"}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, "R"))
            /\ pc' = [pc EXCEPT ![a] = "rmark"]
            /\ UNCHANGED <<owedB, owedK, rel, phase>>
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, removed, gen, sgen, held,
                   aband, orphans, fresh, pend, kills, losts>>

RAbandon(a) ==
    /\ pc[a] = "rabandon"
    /\ aband' = IF "R" \in held[SvcOf(a)] THEN aband \cup {"R"} ELSE aband
    /\ pc' = [pc EXCEPT ![a] = "rgive"]
    /\ UNCHANGED <<LocalOnly, held, rel, phase, fresh, owedB, owedK>>

RGiveBack(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "rgive" /\ Ready(a)
    /\ IF "R" \in held[s] /\ "R" \in aband
       THEN /\ held' = [held EXCEPT ![s] = @ \ {"R"}]
            /\ aband' = aband \ {"R"}
            /\ adv' = SubKeys(adv, b, KeysOf("R"))
       ELSE UNCHANGED <<held, aband, adv>>
    /\ pc' = [pc EXCEPT ![a] = "rfailed"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen, rel,
                   orphans, phase, fresh, owedB, owedK, pend, kills, losts, unkeyed>>

RMark(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "rmark" /\ Ready(a)
    /\ IF sgen[s] # gen \/ MountRows # {}
       THEN /\ Fail(a)
            /\ UNCHANGED removed
       ELSE /\ removed' = TRUE
            /\ pc' = [pc EXCEPT ![a] = "rrel"]
            /\ UNCHANGED <<owedB, owedK, rel, phase>>
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, gen, sgen, held,
                   aband, orphans, fresh, pend, kills, losts, unkeyed>>

\* release_or_mark -> release: one turn here (begin/end around it as for a mount).
RRel(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "rrel" /\ Ready(a)
    /\ rows' = rows \ {"R"}
    /\ adv' = IF "R" \in held[s] THEN SubKeys(adv, b, KeysOf("R")) ELSE adv
    /\ held' = [held EXCEPT ![s] = @ \ {"R"}]
    /\ pc' = [pc EXCEPT ![a] = "rdone"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, aband, rel,
                   orphans, phase, fresh, owedB, owedK, pend, kills, losts, unkeyed>>

----------------------------------------------------------------------------
(* Compaction on its own handle (never reopens, never faulted).            *)

CLock(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "clock"
    /\ IF sgen["SC"] = gen /\ GrantableIn(adv, b, {"C"})
       THEN /\ adv' = PlusAll(adv, b, {"C"})
            /\ pc' = [pc EXCEPT ![a] = "creg"]
       ELSE /\ pc' = [pc EXCEPT ![a] = "cfailed"]
            /\ UNCHANGED adv
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen, held,
                   aband, rel, orphans, phase, fresh, owedB, owedK, turn, pend,
                   kills, losts, unkeyed>>

CReg(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "creg"
    /\ IF MountRows # {} \/ "R" \in rows
       THEN /\ adv' = SubKeys(adv, b, KeysOf("C"))
            /\ pc' = [pc EXCEPT ![a] = "cfailed"]
            /\ UNCHANGED <<rows, unkeyed>>
       ELSE /\ rows' = rows \cup {"C"}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, "C"))
            /\ pc' = [pc EXCEPT ![a] = "cswap"]
            /\ UNCHANGED adv
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, held, aband,
                   rel, orphans, phase, fresh, owedB, owedK, turn, pend, kills,
                   losts>>

OpenAdmit == (\E m \in Mounts : pc[m] = "scanned")
             \/ (\E b \in Backends : pend[b] # NoReq /\ pend[b][2] = "commit")
CSwap(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "cswap" /\ ~OpenAdmit
    /\ gen' = gen + 1
    /\ sgen' = [sgen EXCEPT !["SC"] = gen + 1]
    /\ rows' = rows \ {"C"}
    /\ adv' = SubKeys(adv, b, KeysOf("C"))
    /\ pc' = [pc EXCEPT ![a] = "cdone"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, held, aband, rel,
                   orphans, phase, fresh, owedB, owedK, turn, pend, kills, losts,
                   unkeyed>>

----------------------------------------------------------------------------
TStart(a) ==
    /\ pc[a] = "tidle"
    /\ pc' = [pc EXCEPT ![a] = "tcall"]
    /\ UNCHANGED <<LocalOnly, held, aband, rel, phase, fresh, owedB, owedK>>

TCall(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "tcall" /\ Ready(a)
    /\ pc' = [pc EXCEPT ![a] = "tidle"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
                   held, aband, rel, orphans, phase, fresh, owedB, owedK, pend,
                   kills, losts, unkeyed>>

----------------------------------------------------------------------------
Kill(b) ==
    /\ kills < MaxKill /\ b \in alive
    /\ b \in {conn[s] : s \in {"S1", "S2"}}
    /\ kills' = kills + 1
    /\ alive' = alive \ {b}
    /\ adv' = Drop(adv, {b})
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ UNCHANGED <<nextB, conn, quar, lost, rows, removed, gen, sgen, held, aband,
                   rel, orphans, pc, phase, fresh, owedB, owedK, turn, losts,
                   unkeyed>>

Closed(b) == b \in alive /\ ~(\E s \in Svcs : conn[s] = b /\ ~quar[s])
BackendExit(b) ==
    /\ Closed(b)
    /\ alive' = alive \ {b}
    /\ adv' = Drop(adv, {b})
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ UNCHANGED <<nextB, conn, quar, lost, rows, removed, gen, sgen, held, aband,
                   rel, orphans, pc, phase, fresh, owedB, owedK, turn, kills,
                   losts, unkeyed>>

LateExec(b) ==
    LET a == pend[b][1] s == SvcOf(pend[b][1]) IN
    /\ b \in alive /\ pend[b] # NoReq
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ IF pend[b][2] = "commit"
       THEN /\ rows' = rows \cup {a}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, a))
            /\ UNCHANGED adv
       ELSE IF sgen[s] = gen /\ GrantableIn(adv, b, {a}) /\ ~removed
               /\ ~Conflicting /\ a \notin rows
       THEN /\ adv' = PlusAll(adv, b, {a})
            /\ rows' = rows \cup {a}
            /\ UNCHANGED unkeyed
       ELSE UNCHANGED <<adv, rows, unkeyed>>
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, held,
                   aband, rel, orphans, pc, phase, fresh, owedB, owedK, turn,
                   kills, losts>>

----------------------------------------------------------------------------
Init ==
    /\ alive = {1, 2, 3}
    /\ nextB = 4
    /\ conn = [s \in Svcs |-> IF s = "S1" THEN 1 ELSE IF s = "S2" THEN 2 ELSE 3]
    /\ quar = [s \in Svcs |-> FALSE]
    /\ lost = [s \in Svcs |-> {}]
    /\ adv = [x \in Backends \X KeyNames \X Modes |-> 0]
    /\ rows = {}
    /\ removed = FALSE
    /\ gen = 1
    /\ sgen = [s \in Svcs |-> 1]
    /\ held = [s \in Svcs |-> {}]
    /\ aband = {}
    /\ rel = {}
    /\ orphans = [s \in Svcs |-> {}]
    /\ pc \in {f \in [Actors -> {"one", "sep", "rlock", "clock", "tidle"}] :
                \A a \in Actors :
                    IF a \in Mounts THEN f[a] \in StartPaths
                    ELSE f[a] = (IF a = "R" THEN "rlock" ELSE IF a = "C" THEN "clock" ELSE "tidle")}
    /\ phase = [a \in Actors |-> "reg"]
    /\ fresh = [a \in Actors |-> FALSE]
    /\ owedB = [a \in Actors |-> 0]
    /\ owedK = [a \in Actors |-> {}]
    /\ turn = [s \in Svcs |-> "none"]
    /\ pend = [b \in Backends |-> NoReq]
    /\ kills = 0 /\ losts = 0
    /\ unkeyed = FALSE

Step(a) ==
    \/ SendDead(a) \/ Reopen(a) \/ Settle(a)
    \/ MOne(a) \/ MOneLost(a) \/ MHoldingUnlock(a) \/ MLock(a)
    \/ MRefusedUnlock(a) \/ MScan(a) \/ MCommit(a) \/ MCommitLost(a)
    \/ MAbandon(a) \/ MGiveBack(a) \/ MUnlockOwed(a) \/ MReReg(a)
    \/ MBeginRelease(a) \/ MRelease(a) \/ MReleaseLost(a) \/ MLateEnd(a)
    \/ RLock(a) \/ RRefusedUnlock(a) \/ RReg(a) \/ RAbandon(a) \/ RGiveBack(a)
    \/ RMark(a) \/ RRel(a)
    \/ CLock(a) \/ CReg(a) \/ CSwap(a)
    \/ TStart(a) \/ TCall(a)

Next ==
    \/ \E a \in Actors : Step(a)
    \/ \E b \in Backends : Kill(b) \/ BackendExit(b) \/ LateExec(b)

Fairness ==
    /\ \A a \in Actors : WF_vars(Step(a))
    /\ \A b \in Backends : WF_vars(BackendExit(b)) /\ WF_vars(LateExec(b))

Spec == Init /\ [][Next]_vars
FairSpec == Spec /\ Fairness

----------------------------------------------------------------------------
KeyedAdmission == ~unkeyed
Exclusion == MountRows # {} => ("R" \notin rows /\ "C" \notin rows)
RemovedClean == removed => MountRows = {}
\* One level per key: on a serving session's connection every key's level
\* is exactly one per held op that takes it, plus the levels a call still
\* owes to give back on that connection.  (Under "lateend", an op whose
\* release already unlocked and whose end_release is pending is not counted.)
Counted(s) == {op \in held[s] : ~(op \in Mounts /\ pc[op] = "uend")}
KeysCover ==
    \A s \in {"S1", "S2"} :
        (~quar[s] /\ conn[s] \in alive) => \A op \in Counted(s) : Holds(adv, conn[s], op)
LevelsExact ==
    \A s \in {"S1", "S2"} :
        (~quar[s] /\ conn[s] \in alive) =>
            \A k \in KeyNames, m \in Modes :
                adv[<<conn[s], k, m>>] =
                    Cardinality({op \in Counted(s) : <<k, m>> \in KeysOf(op)})
                    + Cardinality({a \in Actors : SvcOf(a) = s /\ owedB[a] = conn[s]
                                                  /\ <<k, m>> \in owedK[a]})

Termination == <>(\A a \in Actors \ {"T"} : pc[a] \in Terminal)
OrphansRemoved == <>[](\A m \in Mounts : pc[m] = "failed" => m \notin rows)
Serves == [](quar["S1"] => <>(~quar["S1"] \/ sgen["S1"] # gen))
=============================================================================
