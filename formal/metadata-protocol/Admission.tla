------------------------------ MODULE Admission ------------------------------
(***************************************************************************)
(* GC admission on a PostgreSQL Store (#811 V1): session advisory keys,    *)
(* gc_intent rows, mount / unmount, branch.remove, compaction, and the     *)
(* Store service writer's quarantine (variant A, ca4be0bf3c) or reopen     *)
(* (variant B, 1a7338ffa2), with the candidate fixes of the reopen race.   *)
(*                                                                         *)
(* Granularity: one action = one client turn on a writer session (one     *)
(* statement or one pipelined message) or one server event.  The mount's   *)
(* statement-by-statement admission (register_separately) is split where   *)
(* the source splits it: session_lock (own turn), the Admit transaction's  *)
(* scan, its insert + COMMIT, then hold_op outside any turn.  See README.  *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
    Variant,     \* "A" = quarantine for life, "B" = writer reopen
    Fix,         \* "none" | "F1" | "F1late" | "F2" | "F2all"
    FixOrphan,   \* B only: note an orphan in the turn that saw Unknown
    FixLate,     \* B only: release orphans only once every lost backend is
                 \* proven gone; a later call settles what a reopen could not
    Mounts,      \* mount actors on the service S1
    OneMsg,      \* "one" | "sep" | "both": a mount's first admission path
    WithR,       \* a branch.remove on the service S2
    WithC,       \* a compaction on its own handle session SC
    WithT,       \* an endless caller on S1 (another thread's calls)
    MaxKill,     \* backend kills (fault budget)
    MaxLost,     \* lost replies (fault budget)
    MaxB         \* backend ids

Svcs == {"S1", "S2", "SC"}
NoReq == <<"-", "-">>
Backends == 1..MaxB
KeyNames == {"S", "B"}
Modes == {"sh", "ex"}
ReopenSvc == IF Variant = "B" THEN {"S1", "S2"} ELSE {}
Actors == Mounts \cup (IF WithR THEN {"R"} ELSE {})
                 \cup (IF WithC THEN {"C"} ELSE {})
                 \cup (IF WithT THEN {"T"} ELSE {})
SvcOf(a) == IF a \in Mounts \/ a = "T" THEN "S1"
            ELSE IF a = "R" THEN "S2" ELSE "SC"
KeysOf(op) == IF op = "R" THEN {<<"S", "sh">>, <<"B", "ex">>}
              ELSE IF op = "C" THEN {<<"S", "ex">>}
              ELSE {<<"S", "sh">>, <<"B", "sh">>}

VARIABLES
    alive,    \* live backends
    nextB,    \* next backend id a connect gets
    conn,     \* each session's current connection (its backend)
    quar,     \* session quarantined
    lost,     \* B: backends of closed connections not yet proven gone
    adv,      \* session advisory locks: [<<backend, key, mode>> -> level]
    rows,     \* gc_intent rows (op ids; mount ops are actor names)
    removed,  \* the Branch's removal marker
    gen,      \* history_meta.catalog_incarnation
    sgen,     \* the generation each session opened at
    held,     \* held_ops per session
    orphans,  \* B: Session::orphans per session
    pc, unk,  \* per actor: program counter, "the failure was Unknown"
    turn,     \* the writer's FIFO turn, held across one transaction
    pend,     \* a request a backend received but has not run yet
    kills, losts,
    unkeyed,  \* ghost: an intent row committed without its keys held
    ropen     \* ghost: reopens during one mount's registration call

vars == <<alive, nextB, conn, quar, lost, adv, rows, removed, gen, sgen,
          held, orphans, pc, unk, turn, pend, kills, losts, unkeyed, ropen>>

----------------------------------------------------------------------------
(* Advisory locks.  A backend never conflicts with itself.                 *)
Compatible(f, b, k, m) ==
    \A c \in Backends \ {b} :
        IF m = "ex" THEN f[<<c, k, "sh">>] = 0 /\ f[<<c, k, "ex">>] = 0
                    ELSE f[<<c, k, "ex">>] = 0
GrantableIn(f, b, ops) ==
    \A op \in ops : \A km \in KeysOf(op) : Compatible(f, b, km[1], km[2])
PlusAll(f, b, ops) ==
    [x \in DOMAIN f |-> IF x[1] = b
        THEN f[x] + Cardinality({op \in ops : <<x[2], x[3]>> \in KeysOf(op)})
        ELSE f[x]]
Minus(f, b, op) ==
    [x \in DOMAIN f |-> IF x[1] = b /\ <<x[2], x[3]>> \in KeysOf(op) /\ f[x] > 0
                        THEN f[x] - 1 ELSE f[x]]
Drop(f, bs) == [x \in DOMAIN f |-> IF x[1] \in bs THEN 0 ELSE f[x]]
Holds(f, b, op) == \A km \in KeysOf(op) : f[<<b, km[1], km[2]>>] > 0

MountRows == rows \cap Mounts
StartPaths == CASE OneMsg = "one" -> {"one"} [] OneMsg = "sep" -> {"sep"}
                [] OneMsg = "both" -> {"one", "sep"}
Conflicting == "R" \in rows \/ "C" \in rows

----------------------------------------------------------------------------
(* Program counters.                                                       *)
\* Calls that take the writer's turn and pass Session::ready (may reopen).
SessionPcs == {"one", "sep", "locked", "errunlock", "mounted",
               "rlock", "rreg", "rmark", "rrel", "rerr", "tcall"}
\* A registration in progress, between its keys and its hold_op (F2 / F2all).
RegPcs == {"pre", "locked", "scanned", "admitted", "errunlock", "rreg", "rerr"}
\* One register_mount call (for the one-reopen-per-call ghost).
RegCallPcs == {"one", "sep", "locked", "errunlock"}
Terminal == {"failed", "unmounted", "umfailed", "rdone", "rfailed",
             "cdone", "cfailed"}

FailPc(a, u) ==
    CASE pc[a] = "sep"       -> IF u THEN "note" ELSE "failed"
      [] pc[a] = "one"       -> IF u THEN "note" ELSE "failed"
      [] pc[a] \in {"locked", "scanned"} -> "errunlock"
      [] pc[a] = "errunlock" -> IF unk[a] THEN "note" ELSE "failed"
      [] pc[a] = "mounted"   -> "umfailed"
      [] pc[a] = "rlock"     -> "rfailed"
      [] pc[a] = "rreg"      -> "rerr"
      [] pc[a] = "rmark"     -> "rrel"
      [] pc[a] \in {"rrel", "rerr"} -> "rfailed"
      [] pc[a] = "tcall"     -> "tidle"
      [] OTHER               -> "failed"

\* The call fails; an errunlock leaving the turn drops its op from held_ops
\* (F1 recorded it before the second turn).
Fail(a, u) ==
    /\ pc' = [pc EXCEPT ![a] = FailPc(a, u)]
    /\ unk' = [unk EXCEPT ![a] = IF pc[a] = "errunlock" THEN unk[a] ELSE u]
    /\ held' = IF pc[a] \in {"errunlock", "rerr"}
               THEN [held EXCEPT ![SvcOf(a)] = @ \ {a}] ELSE held

Quarantine(s) ==
    /\ quar' = [quar EXCEPT ![s] = TRUE]
    /\ lost' = [lost EXCEPT ![s] = IF quar[s] THEN @ ELSE @ \cup {conn[s]}]

SettleDue(s) ==
    /\ FixLate /\ s \in ReopenSvc /\ orphans[s] # {}
    /\ \A b \in lost[s] \cap alive : pend[b] = NoReq

\* A turn of a healthy session whose connection is still alive.
Ready(a) ==
    LET s == SvcOf(a) IN
    /\ turn[s] \in {"none", a}
    /\ ~quar[s]
    /\ conn[s] \in alive
    /\ ~SettleDue(s)

\* B: orphans noted at the moment the Unknown is seen (FixOrphan).
NoteNow(a, u) ==
    orphans' = IF Variant = "B" /\ FixOrphan /\ u /\ a \in Mounts
               THEN [orphans EXCEPT !["S1"] = @ \cup {a}] ELSE orphans

----------------------------------------------------------------------------
(* Session::ready: refusal (A), reopen (B), a dead connection.             *)

ReopenAllowed(a) ==
    CASE Fix = "F2"    -> pc[a] \notin RegPcs
      [] Fix = "F2all" -> \A m \in Actors : SvcOf(m) = SvcOf(a) => pc[m] \notin RegPcs
      [] OTHER         -> TRUE

\* Variant A (and SC in both): a quarantined session refuses before sending.
Refuse(a) ==
    LET s == SvcOf(a) IN
    /\ s \notin ReopenSvc /\ pc[a] \in SessionPcs
    /\ turn[s] \in {"none", a} /\ quar[s]
    /\ Fail(a, FALSE)
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, orphans, turn, pend, kills, losts, unkeyed, ropen>>

\* A send on a connection the server closed: quarantined, the call fails.
\* Inside the open Admit transaction it is Unknown (a lost reply).
SendDead(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] \in SessionPcs \cup {"scanned"}
    /\ turn[s] \in {"none", a} /\ ~quar[s] /\ conn[s] \notin alive
    /\ Quarantine(s)
    /\ Fail(a, pc[a] = "scanned")
    /\ NoteNow(a, pc[a] = "scanned")
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, rows, removed, gen, sgen, pend,
                   kills, losts, unkeyed, ropen>>

\* F2 / F2all: the call that would reopen is refused instead.
NoReopen(a) ==
    LET s == SvcOf(a) IN
    /\ s \in ReopenSvc /\ pc[a] \in SessionPcs
    /\ turn[s] = "none" /\ (quar[s] \/ conn[s] \notin alive)   \* once per turn
    /\ ~ReopenAllowed(a)
    /\ Quarantine(s)
    /\ Fail(a, FALSE)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, rows, removed, gen, sgen, orphans,
                   pend, kills, losts, unkeyed, ropen>>

\* reopen.rs Session::reopen / settle: steps 1-4 (5, prepare, is no state).
Reopen(a) ==
    LET s == SvcOf(a)
        lost1 == IF quar[s] THEN lost[s] ELSE lost[s] \cup {conn[s]}
        nb == nextB
    IN
    /\ s \in ReopenSvc /\ pc[a] \in SessionPcs
    /\ turn[s] = "none" /\ (quar[s] \/ conn[s] \notin alive)   \* once per turn
    /\ ReopenAllowed(a)
    /\ nb \in Backends
    \* step 1: a backend still running a request may not be proven gone
    /\ \E NG \in SUBSET {b \in lost1 \cap alive : pend[b] # NoReq} :
       LET gone == (lost1 \cap alive) \ NG
           adv1 == Drop(adv, gone)
           ok == sgen[s] = gen                      \* step 2
                 /\ GrantableIn(adv1, nb, held[s])   \* step 3, all or none
       IN
       /\ lost' = [lost EXCEPT ![s] = NG]
       /\ pend' = [b \in Backends |-> IF b \in gone THEN NoReq ELSE pend[b]]
       /\ IF ok
          THEN /\ conn' = [conn EXCEPT ![s] = nb]
               /\ nextB' = nb + 1
               /\ alive' = (alive \ gone) \cup {nb}
               /\ adv' = PlusAll(adv1, nb, held[s])
               /\ quar' = [quar EXCEPT ![s] = FALSE]
               \* step 4 (FixLate: only with no lost backend left running)
               /\ IF FixLate /\ NG # {}
                  THEN UNCHANGED <<rows, orphans>>
                  ELSE /\ rows' = rows \ orphans[s]
                       /\ orphans' = [orphans EXCEPT ![s] = {}]
               /\ turn' = [turn EXCEPT ![s] = a]   \* the call's work follows
               /\ ropen' = [ropen EXCEPT ![a] =
                              IF pc[a] \in RegCallPcs THEN @ + 1 ELSE @]
               /\ UNCHANGED <<pc, unk, held>>
          \* The new connection is closed again and leaves nothing on the
          \* server, so the model reuses its backend id (bounds MaxB).
          ELSE /\ nextB' = nextB
               /\ conn' = conn
               /\ alive' = alive \ gone
               /\ adv' = adv1
               /\ quar' = [quar EXCEPT ![s] = TRUE]
               /\ turn' = [turn EXCEPT ![s] = "none"]
               /\ Fail(a, FALSE)
               /\ UNCHANGED <<rows, orphans, ropen>>
    /\ UNCHANGED <<removed, gen, sgen, kills, losts, unkeyed>>

\* FixLate: the next call of a serving session whose orphans wait, once no
\* lost backend is still running a request, first ends the lost backends
\* (as pass_lane does) and releases the orphans; until then calls proceed.
Settle(a) ==
    LET s == SvcOf(a) gone == lost[s] \cap alive IN
    /\ pc[a] \in SessionPcs /\ turn[s] \in {"none", a} /\ ~quar[s]
    /\ conn[s] \in alive /\ SettleDue(s)
    /\ alive' = alive \ gone
    /\ adv' = Drop(adv, gone)
    /\ lost' = [lost EXCEPT ![s] = {}]
    /\ rows' = rows \ orphans[s]
    /\ orphans' = [orphans EXCEPT ![s] = {}]
    /\ UNCHANGED <<nextB, conn, quar, removed, gen, sgen, held, pc, unk, turn,
                   pend, kills, losts, unkeyed, ropen>>

----------------------------------------------------------------------------
(* Mount actors (service S1).                                              *)

\* One-message admission (backend/postgresql/history/mount.rs admit): fence,
\* keys all-or-none, probe, scan, insert, COMMIT in one transaction.  A
\* definite refusal falls back to register_separately with nothing held.
MOne(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "one" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ Fail(a, FALSE)
            /\ UNCHANGED <<adv, rows>>
       ELSE IF ~GrantableIn(adv, b, {a}) \/ removed \/ Conflicting
       THEN /\ pc' = [pc EXCEPT ![a] = "sep"]
            /\ UNCHANGED <<adv, rows, unk, held>>
       ELSE /\ adv' = PlusAll(adv, b, {a})
            /\ rows' = rows \cup {a}
            \* the decided turn ends; hold_op runs after it (F1: inside it)
            /\ pc' = [pc EXCEPT ![a] = IF Fix = "F1" THEN "mounted" ELSE "admitted"]
            /\ held' = IF Fix = "F1" THEN [held EXCEPT ![s] = @ \cup {a}] ELSE held
            /\ UNCHANGED unk
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, orphans,
                   pend, kills, losts, unkeyed, ropen>>

\* The message's reply is lost: it ran, or the backend still holds it.
MOneLost(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "one" /\ Ready(a) /\ losts < MaxLost
    /\ losts' = losts + 1
    /\ \/ /\ sgen[s] = gen /\ GrantableIn(adv, b, {a}) /\ ~removed /\ ~Conflicting
          /\ adv' = PlusAll(adv, b, {a})
          /\ rows' = rows \cup {a}
          /\ UNCHANGED pend
       \/ /\ pend' = [pend EXCEPT ![b] = <<a, "one">>]
          /\ UNCHANGED <<adv, rows>>
    /\ Quarantine(s)
    /\ Fail(a, TRUE)
    /\ NoteNow(a, TRUE)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, removed, gen, sgen, kills, unkeyed, ropen>>

\* register_separately, turn 1: session_lock (admission/session.rs).
MLock(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "sep" /\ Ready(a)
    /\ IF sgen[s] = gen /\ GrantableIn(adv, b, {a})
       THEN /\ adv' = PlusAll(adv, b, {a})
            /\ pc' = [pc EXCEPT ![a] = IF Fix = "F1late" THEN "pre" ELSE "locked"]
            \* F1: the keys go into held_ops inside the turn that took them
            /\ held' = IF Fix = "F1" THEN [held EXCEPT ![s] = @ \cup {a}] ELSE held
            /\ UNCHANGED unk
       ELSE /\ Fail(a, FALSE)
            /\ UNCHANGED adv
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   orphans, pend, kills, losts, unkeyed, ropen>>

\* F1late: hold_op after the lock's turn ended, before the second turn.
MPre(a) ==
    /\ pc[a] = "pre"
    /\ held' = [held EXCEPT ![SvcOf(a)] = @ \cup {a}]
    /\ pc' = [pc EXCEPT ![a] = "locked"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, orphans, unk, turn, pend, kills, losts, unkeyed, ropen>>

\* turn 2 (run_admit): BEGIN with the fence, Branch probe, conflict scan.
\* The transaction stays open (and holds the turn) until its COMMIT.
MScan(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "locked" /\ Ready(a)
    /\ IF sgen[s] # gen \/ removed \/ Conflicting
       THEN /\ Fail(a, FALSE)
            /\ turn' = [turn EXCEPT ![s] = "none"]
       ELSE /\ pc' = [pc EXCEPT ![a] = "scanned"]
            /\ turn' = [turn EXCEPT ![s] = a]
            /\ UNCHANGED <<unk, held>>
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, orphans, pend, kills, losts, unkeyed, ropen>>

\* INSERT ... RETURNING and COMMIT.
MCommit(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "scanned" /\ turn[s] = a /\ ~quar[s] /\ b \in alive
    /\ rows' = rows \cup {a}
    /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, a))
    /\ pc' = [pc EXCEPT ![a] = IF Fix \in {"F1", "F1late"} THEN "mounted" ELSE "admitted"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, removed, gen, sgen,
                   held, orphans, unk, pend, kills, losts, ropen>>

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
    /\ Fail(a, TRUE)
    /\ NoteNow(a, TRUE)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, removed, gen, sgen, kills, ropen>>

\* hold_op, outside any turn (admission.rs register_separately Ok arm;
\* admission/mount.rs one_message Admitted arm).
MHold(a) ==
    /\ pc[a] = "admitted"
    /\ held' = [held EXCEPT ![SvcOf(a)] = @ \cup {a}]
    /\ pc' = [pc EXCEPT ![a] = "mounted"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, orphans, unk, turn, pend, kills, losts, unkeyed, ropen>>

\* The Err arm's session_unlock: its own turn, through Session::ready.
MErrUnlock(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "errunlock" /\ Ready(a)
    /\ adv' = Minus(adv, b, a)
    /\ Fail(a, unk[a])
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   orphans, pend, kills, losts, unkeyed, ropen>>

\* register_mount's note_orphan, after register_separately returned.
MNote(a) ==
    /\ pc[a] = "note"
    /\ orphans' = IF Variant = "B" /\ ~FixOrphan
                  THEN [orphans EXCEPT !["S1"] = @ \cup {a}] ELSE orphans
    /\ pc' = [pc EXCEPT ![a] = "failed"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, held, unk, turn, pend, kills, losts, unkeyed, ropen>>

\* Unmount: the owner release, one message (delete then unlock).
MUnmount(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "mounted" /\ Ready(a)
    /\ IF sgen[s] # gen
       THEN /\ Fail(a, FALSE)
            /\ UNCHANGED <<rows, adv>>
       ELSE /\ rows' = rows \ {a}
            /\ adv' = Minus(adv, b, a)
            /\ held' = [held EXCEPT ![s] = @ \ {a}]
            /\ pc' = [pc EXCEPT ![a] = "unmounted"]
            /\ UNCHANGED unk
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, orphans,
                   pend, kills, losts, unkeyed, ropen>>

\* A lost release reply: the delete ran or not; the op stays held
\* (authority.rs release: end_release(kept = true)).
MUnmountLost(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "mounted" /\ Ready(a) /\ losts < MaxLost /\ sgen[s] = gen
    /\ losts' = losts + 1
    /\ \/ rows' = rows \ {a}
       \/ UNCHANGED rows
    /\ Quarantine(s)
    /\ Fail(a, TRUE)
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, adv, removed, gen, sgen, orphans, pend,
                   kills, unkeyed, ropen>>

----------------------------------------------------------------------------
(* branch.remove (service S2): register (lane write), marker, release.     *)

RLock(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "rlock" /\ Ready(a)
    /\ IF sgen[s] = gen /\ GrantableIn(adv, b, {"R"})
       THEN /\ adv' = PlusAll(adv, b, {"R"})
            /\ pc' = [pc EXCEPT ![a] = "rreg"]
            \* F1 (and F1late, which differs only for mounts): held in-turn
            /\ held' = IF Fix \in {"F1", "F1late"}
                       THEN [held EXCEPT ![s] = @ \cup {"R"}] ELSE held
            /\ UNCHANGED unk
       ELSE /\ Fail(a, FALSE)
            /\ UNCHANGED adv
    /\ turn' = [turn EXCEPT ![SvcOf(a)] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   orphans, pend, kills, losts, unkeyed, ropen>>

RReg(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] = "rreg" /\ Ready(a)
    /\ IF sgen[s] # gen \/ MountRows # {} \/ "C" \in rows
       THEN /\ Fail(a, FALSE)
            /\ UNCHANGED <<rows, unkeyed>>
       ELSE /\ rows' = rows \cup {"R"}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, "R"))
            /\ held' = [held EXCEPT ![s] = @ \cup {"R"}]
            /\ pc' = [pc EXCEPT ![a] = "rmark"]
            /\ UNCHANGED unk
    /\ turn' = [turn EXCEPT ![SvcOf(a)] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, removed, gen, sgen,
                   orphans, pend, kills, losts, ropen>>

RMark(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "rmark" /\ Ready(a)
    /\ IF sgen[s] # gen \/ MountRows # {}
       THEN /\ Fail(a, FALSE)
            /\ UNCHANGED removed
       ELSE /\ removed' = TRUE
            /\ pc' = [pc EXCEPT ![a] = "rrel"]
            /\ UNCHANGED <<unk, held>>
    /\ turn' = [turn EXCEPT ![SvcOf(a)] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, gen, sgen,
                   orphans, pend, kills, losts, unkeyed, ropen>>

RRel(a) ==
    LET s == SvcOf(a) b == conn[s] IN
    /\ pc[a] \in {"rrel", "rerr"} /\ Ready(a)
    /\ rows' = rows \ {"R"}
    /\ adv' = Minus(adv, b, "R")
    /\ held' = [held EXCEPT ![s] = @ \ {"R"}]
    /\ pc' = [pc EXCEPT ![a] = IF pc[a] = "rrel" THEN "rdone" ELSE "rfailed"]
    /\ turn' = [turn EXCEPT ![SvcOf(a)] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, orphans,
                   unk, pend, kills, losts, unkeyed, ropen>>

----------------------------------------------------------------------------
(* Compaction on its own handle session SC (never reopens, never faulted). *)

CLock(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "clock"
    /\ IF sgen["SC"] = gen /\ GrantableIn(adv, b, {"C"})
       THEN /\ adv' = PlusAll(adv, b, {"C"})
            /\ pc' = [pc EXCEPT ![a] = "creg"]
       ELSE /\ pc' = [pc EXCEPT ![a] = "cfailed"]
            /\ UNCHANGED adv
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, rows, removed, gen, sgen,
                   held, orphans, unk, turn, pend, kills, losts, unkeyed, ropen>>

CReg(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "creg"
    /\ IF MountRows # {} \/ "R" \in rows
       THEN /\ adv' = Minus(adv, b, "C")
            /\ pc' = [pc EXCEPT ![a] = "cfailed"]
            /\ UNCHANGED <<rows, unkeyed>>
       ELSE /\ rows' = rows \cup {"C"}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, "C"))
            /\ pc' = [pc EXCEPT ![a] = "cswap"]
            /\ UNCHANGED adv
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, held,
                   orphans, unk, turn, pend, kills, losts, ropen>>

\* The swap: LOCK history_meta ACCESS EXCLUSIVE NOWAIT waits out no open
\* fenced transaction (modelled as enabled only when none is open), then
\* G + 1.  The round ends: its row and key go.
OpenAdmit == (\E m \in Mounts : pc[m] = "scanned")
             \/ (\E b \in Backends : pend[b] # NoReq /\ pend[b][2] = "commit")
CSwap(a) ==
    LET b == conn["SC"] IN
    /\ pc[a] = "cswap" /\ ~OpenAdmit
    /\ gen' = gen + 1
    /\ sgen' = [sgen EXCEPT !["SC"] = gen + 1]
    /\ rows' = rows \ {"C"}
    /\ adv' = Minus(adv, b, "C")
    /\ pc' = [pc EXCEPT ![a] = "cdone"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, held, orphans, unk,
                   turn, pend, kills, losts, unkeyed, ropen>>

----------------------------------------------------------------------------
(* Another thread's calls on S1 (a status read, a Commit, ...).            *)

TStart(a) ==
    /\ pc[a] = "tidle"
    /\ pc' = [pc EXCEPT ![a] = "tcall"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, held, orphans, unk, turn, pend, kills, losts, unkeyed, ropen>>

TCall(a) ==
    LET s == SvcOf(a) IN
    /\ pc[a] = "tcall" /\ Ready(a)
    /\ pc' = [pc EXCEPT ![a] = "tidle"]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, adv, rows, removed, gen,
                   sgen, held, orphans, unk, pend, kills, losts, unkeyed, ropen>>

----------------------------------------------------------------------------
(* Server events and faults.                                               *)

\* Fault: pg_terminate_backend / a crash of a writer's backend.
Kill(b) ==
    /\ kills < MaxKill /\ b \in alive
    /\ b \in {conn[s] : s \in {"S1", "S2"}}
    /\ kills' = kills + 1
    /\ alive' = alive \ {b}
    /\ adv' = Drop(adv, {b})
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ UNCHANGED <<nextB, conn, quar, lost, rows, removed, gen, sgen, held,
                   orphans, pc, unk, turn, losts, unkeyed, ropen>>

\* A backend whose client closed the connection ends (dropping anything it
\* had not run).
Closed(b) == b \in alive /\ ~(\E s \in Svcs : conn[s] = b /\ ~quar[s])
BackendExit(b) ==
    /\ Closed(b)
    /\ alive' = alive \ {b}
    /\ adv' = Drop(adv, {b})
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ UNCHANGED <<nextB, conn, quar, lost, rows, removed, gen, sgen, held,
                   orphans, pc, unk, turn, kills, losts, unkeyed, ropen>>

\* A request whose reply the client gave up on runs after all.
LateExec(b) ==
    LET a == pend[b][1] s == SvcOf(pend[b][1]) IN
    /\ b \in alive /\ pend[b] # NoReq
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ IF pend[b][2] = "commit"
       THEN /\ rows' = rows \cup {a}
            /\ unkeyed' = (unkeyed \/ ~Holds(adv, b, a))
            /\ UNCHANGED adv
       ELSE IF sgen[s] = gen /\ GrantableIn(adv, b, {a}) /\ ~removed /\ ~Conflicting
       THEN /\ adv' = PlusAll(adv, b, {a})
            /\ rows' = rows \cup {a}
            /\ UNCHANGED unkeyed
       ELSE UNCHANGED <<adv, rows, unkeyed>>
    /\ UNCHANGED <<alive, nextB, conn, quar, lost, removed, gen, sgen, held,
                   orphans, pc, unk, turn, kills, losts, ropen>>

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
    /\ orphans = [s \in Svcs |-> {}]
    /\ pc \in {f \in [Actors -> {"one", "sep", "rlock", "clock", "tidle"}] :
                \A a \in Actors :
                    IF a \in Mounts THEN f[a] \in StartPaths
                    ELSE f[a] = (IF a = "R" THEN "rlock" ELSE IF a = "C" THEN "clock" ELSE "tidle")}
    /\ unk = [a \in Actors |-> FALSE]
    /\ turn = [s \in Svcs |-> "none"]
    /\ pend = [b \in Backends |-> NoReq]
    /\ kills = 0 /\ losts = 0
    /\ unkeyed = FALSE
    /\ ropen = [a \in Actors |-> 0]

Step(a) ==
    \/ Refuse(a) \/ SendDead(a) \/ NoReopen(a) \/ Reopen(a) \/ Settle(a)
    \/ MOne(a) \/ MOneLost(a) \/ MLock(a) \/ MPre(a) \/ MScan(a)
    \/ MCommit(a) \/ MCommitLost(a) \/ MHold(a) \/ MErrUnlock(a) \/ MNote(a)
    \/ MUnmount(a) \/ MUnmountLost(a)
    \/ RLock(a) \/ RReg(a) \/ RMark(a) \/ RRel(a)
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
(* Properties (the GC admission and writer-reopen rows of #811).           *)

\* No live mount (or removal / compaction) is left without keys: every
\* intent row commits while its own backend holds the op's keys.
KeyedAdmission == ~unkeyed

\* Compaction and branch.remove never proceed while a mount is live.
Exclusion == MountRows # {} => ("R" \notin rows /\ "C" \notin rows)
RemovedClean == removed => MountRows = {}

\* A session that serves holds the keys of every op it holds (reopen step 3).
KeysCover ==
    \A s \in {"S1", "S2"} :
        (~quar[s] /\ conn[s] \in alive) => \A op \in held[s] : Holds(adv, conn[s], op)

\* One reopen per register_mount call.
OneReopenPerCall == \A m \in Mounts : ropen[m] <= 1

\* Liveness: every call ends with an answer; orphaned rows are removed; a
\* quarantined writer serves again or is definitely stale.
Termination == <>(\A a \in Actors \ {"T"} : pc[a] \in Terminal)
OrphansRemoved == <>[](\A m \in Mounts : pc[m] = "failed" => m \notin rows)
Serves == [](quar["S1"] => <>(~quar["S1"] \/ sgen["S1"] # gen))
\* Reachability probe (expected to be violated): a mount whose registration
\* reopened the writer mid-call is admitted and mounted, so the fixes are
\* checked on the race path itself, not vacuously.
NoMountAfterReopen == ~(\E m \in Mounts : ropen[m] >= 1 /\ pc[m] = "mounted")
=============================================================================
