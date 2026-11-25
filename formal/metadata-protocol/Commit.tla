------------------------------- MODULE Commit -------------------------------
(***************************************************************************)
(* Commit and id metadata on a PostgreSQL Store (#811 V1): the store_lane  *)
(* row and the lane barrier, optimistic Branch heads (fused Commit, lane   *)
(* merge, force), per-attempt tokens and the resolution of a lost reply,   *)
(* the generation fence, the lane-liveness check of nested writes, and the *)
(* id allocator (block reservation, compare-and-set rewind); with the      *)
(* writer's quarantine (A, ca4be0bf3c) or reopen (B, 1a7338ffa2).          *)
(*                                                                         *)
(* Each server transaction runs atomically when the locks it waits for are *)
(* free; a request whose reply was lost may run later, until its backend   *)
(* ends.  Mutation switches remove one mechanism to show the properties   *)
(* are not vacuous.  See README.                                           *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS
    Variant,      \* "A" | "B"
    Mutation,     \* "none" | "nobarrier" | "nolanecheck" | "noend" | "nofence" | "nocas"
    Ws,           \* Workspace actors, subset of {"w1", "w2"}
    Force,        \* Workspaces whose Commits pass force
    Ks,           \* id-allocator handles, subset of {"k1", "k2"}
    MaxCommits,   \* Commit calls per Workspace
    KRounds,      \* reserve / use / rewind rounds per handle
    Blk,          \* ids per reserved block
    MaxKill, MaxLaneKill, MaxLost, MaxBump, MaxB

Svcs == {"S1", "S2"}
Backends == 1..MaxB
NoReq == [kind |-> "-"]
Actors == Ws \cup Ks
SvcOf(a) == IF a \in {"w1", "k1"} THEN "S1" ELSE "S2"
Atts == Ws \X (1..MaxCommits)
Root == <<"root", 0>>
ReopenSvc == IF Variant = "B" THEN Svcs ELSE {}

VARIABLES
    alive, nextB,
    wconn,     \* writer connection backend per service
    lconn,     \* lane connection backend per service (0: none yet)
    laneRow,   \* backend holding store_lane FOR UPDATE (0: free)
    barSh,     \* backends holding the barrier key shared (open nested writes)
    quar, lost, sgen, gen, turn,
    head,      \* the Branch head (an attempt token or Root)
    hist,      \* the Commit rows in install order: [att, parent, disp]
    pend,      \* a request a backend received and has not run yet
    wpc, cnt, bound, retained, lhead, report,
    hw,        \* scope_allocator highwater
    kpc, kround, kblock, used,
    kills, lkills, losts, bumps,
    bad        \* ghost: the names of violated guarantees

vars == <<alive, nextB, wconn, lconn, laneRow, barSh, quar, lost, sgen, gen,
          turn, head, hist, pend, wpc, cnt, bound, retained, lhead, report,
          hw, kpc, kround, kblock, used, kills, lkills, losts, bumps, bad>>

srv == <<alive, nextB, wconn, lconn, laneRow, barSh, quar, lost, sgen, gen,
         turn, pend, kills, lkills, losts, bumps>>
wsv == <<wpc, cnt, bound, retained, lhead, report>>
ksv == <<kpc, kround, kblock, used>>

Att(w) == <<w, cnt[w]>>
HistAtts == {hist[i].att : i \in 1..Len(hist)}

----------------------------------------------------------------------------
(* Server side.                                                            *)

\* A lane holder's FOR UPDATE plus the exclusive barrier pass.
LaneFree(b) ==
    /\ laneRow = 0
    /\ (Mutation = "nobarrier" \/ barSh \subseteq {b})

\* The lane transaction a nested write relies on is in progress.
LaneLive(s) == lconn[s] # 0 /\ lconn[s] \in alive /\ laneRow = lconn[s]

\* Installing one Commit row and moving the head, with the ghost checks of
\* the head, fence and lane-liveness guarantees.
Install(att, parent, disp, g, nested, s) ==
    /\ hist' = Append(hist, [att |-> att, parent |-> parent, disp |-> disp])
    /\ head' = att
    /\ bad' = bad \cup
         (IF parent # head /\ ~disp THEN {"LostUpdate"} ELSE {})
         \cup (IF g # gen THEN {"StaleWrite"} ELSE {})
         \cup (IF nested /\ ~LaneLive(s) THEN {"ClosedWrite"} ELSE {})
         \cup (IF att \in HistAtts THEN {"AttemptTwice"} ELSE {})
         \cup (IF disp /\ att[1] \notin Force THEN {"DisplacedWithoutForce"} ELSE {})

\* The fused Commit's one transaction: fence, lane row and barrier, head
\* check (force displaces), insert with its attempt token, COMMIT.  Answers
\* "Committed", "Displaced", "Moved" (nothing written) or "Stale".
FusedOutcome(exp, frc, g) ==
    IF g # gen /\ Mutation # "nofence" THEN "Stale"
    ELSE IF head = exp THEN "Committed"
    ELSE IF frc THEN "Displaced"
    ELSE "Moved"

RunFused(att, exp, frc, g, s) ==
    LET o == FusedOutcome(exp, frc, g) IN
    IF o \in {"Committed", "Displaced"}
    THEN Install(att, head, o = "Displaced", g, FALSE, s)
    ELSE UNCHANGED <<hist, head, bad>>

\* A nested write's lane check and COMMIT (#768: one round trip).
NestedOk(s) == Mutation = "nolanecheck" \/ LaneLive(s)

Quarantine(s) ==
    /\ quar' = [quar EXCEPT ![s] = TRUE]
    /\ lost' = [lost EXCEPT ![s] = IF quar[s] THEN @ ELSE @ \cup {wconn[s]}]

----------------------------------------------------------------------------
(* Session::ready for top-level calls.                                     *)

TopPcs == {"commit", "kres", "krew"}

Ready(a) ==
    LET s == SvcOf(a) IN
    turn[s] \in {"none", a} /\ ~quar[s] /\ wconn[s] \in alive

FailCall(a, outcome) ==
    IF a \in Ws
    THEN /\ wpc' = [wpc EXCEPT ![a] = "idle"]
         /\ report' = [report EXCEPT ![Att(a)] = outcome]
         /\ UNCHANGED <<cnt, bound, retained, lhead, kpc, kround, kblock, used>>
    ELSE /\ kpc' = [kpc EXCEPT ![a] = "kdone"]      \* the handle closes
         /\ UNCHANGED <<wpc, cnt, bound, retained, lhead, report, kround, kblock, used>>

\* A: a quarantined writer refuses before sending anything.
Refuse(a) ==
    LET s == SvcOf(a) IN
    /\ (a \in Ws => wpc[a] \in TopPcs) /\ (a \in Ks => kpc[a] \in TopPcs)
    /\ s \notin ReopenSvc /\ turn[s] = "none" /\ quar[s]
    /\ FailCall(a, "Refused")
    /\ UNCHANGED <<srv, head, hist, hw, bad>>

\* A send on a connection the server closed.
SendDead(a) ==
    LET s == SvcOf(a) IN
    /\ (a \in Ws => wpc[a] \in TopPcs) /\ (a \in Ks => kpc[a] \in TopPcs)
    /\ turn[s] = "none" /\ ~quar[s] /\ wconn[s] \notin alive
    /\ Quarantine(s)
    /\ FailCall(a, "Closed")
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, sgen, gen, turn,
                   pend, kills, lkills, losts, bumps, head, hist, hw, bad>>

\* B: one new connection; the closed ones' backends ended first (one that
\* is still running a request may not be proven gone), the generation read.
Reopen(a) ==
    LET s == SvcOf(a)
        lost1 == IF quar[s] THEN lost[s] ELSE lost[s] \cup {wconn[s]}
        nb == nextB
    IN
    /\ (a \in Ws => wpc[a] \in TopPcs) /\ (a \in Ks => kpc[a] \in TopPcs)
    /\ s \in ReopenSvc /\ turn[s] = "none" /\ (quar[s] \/ wconn[s] \notin alive)
    /\ nb \in Backends
    /\ \E NG \in SUBSET {b \in lost1 \cap alive : pend[b] # NoReq} :
       LET gone == (lost1 \cap alive) \ NG IN
       /\ lost' = [lost EXCEPT ![s] = NG]
       /\ pend' = [b \in Backends |-> IF b \in gone THEN NoReq ELSE pend[b]]
       /\ barSh' = barSh \ gone
       /\ laneRow' = IF laneRow \in gone THEN 0 ELSE laneRow
       /\ IF sgen[s] = gen
          THEN /\ alive' = (alive \ gone) \cup {nb}
               /\ nextB' = nb + 1
               /\ wconn' = [wconn EXCEPT ![s] = nb]
               /\ quar' = [quar EXCEPT ![s] = FALSE]
               /\ turn' = [turn EXCEPT ![s] = a]
               /\ UNCHANGED <<wsv, ksv>>
          ELSE /\ alive' = alive \ gone
               /\ UNCHANGED <<nextB, wconn, turn>>
               /\ quar' = [quar EXCEPT ![s] = TRUE]
               /\ FailCall(a, "Stale")
    /\ UNCHANGED <<lconn, sgen, gen, kills, lkills, losts, bumps, head, hist, hw, bad>>

----------------------------------------------------------------------------
(* Workspace Commits.                                                      *)

\* A Commit call: a retained uncertain Commit is resolved first.
WStart(w) ==
    /\ wpc[w] = "idle" /\ cnt[w] < MaxCommits
    /\ cnt' = [cnt EXCEPT ![w] = @ + 1]
    /\ wpc' = [wpc EXCEPT ![w] = IF retained[w] # Root THEN "resolve" ELSE "commit"]
    /\ UNCHANGED <<srv, head, hist, bound, retained, lhead, report, hw, ksv, bad>>

\* resolve_retained: pass_lane (end the lost backends by pid and start,
\* bounded; take and release the lane), then the attempt lookup.
\* A: the quarantined session's one backend; B: State::lost.
WResolve(w) ==
    LET s == SvcOf(w)
        t == retained[w]
        targets == IF Variant = "A" THEN (IF quar[s] THEN {wconn[s]} ELSE {})
                   ELSE lost[s]
    IN
    /\ wpc[w] = "resolve"
    /\ \E NG \in SUBSET {b \in targets \cap alive : pend[b] # NoReq} :
       LET gone == IF Mutation = "noend" THEN {} ELSE (targets \cap alive) \ NG
           pend1 == [b \in Backends |-> IF b \in gone THEN NoReq ELSE pend[b]]
           bar1 == barSh \ gone
           row1 == IF laneRow \in gone THEN 0 ELSE laneRow
       IN
       /\ alive' = alive \ gone
       /\ pend' = pend1
       /\ barSh' = bar1
       /\ lost' = IF Variant = "B" THEN [lost EXCEPT ![s] = @ \ gone] ELSE lost
       /\ IF NG = {}
          THEN \* the lane pass waits for every holder (lane row, barrier)
               /\ row1 = 0 /\ (Mutation = "nobarrier" \/ bar1 = {})
               /\ laneRow' = row1
               /\ report' = [report EXCEPT ![t] =
                               IF t \in HistAtts THEN "Committed" ELSE "NotCommitted"]
               /\ bound' = [bound EXCEPT ![w] = IF t \in HistAtts THEN t ELSE @]
               /\ retained' = [retained EXCEPT ![w] = Root]
               /\ wpc' = [wpc EXCEPT ![w] = "commit"]
          ELSE \* not proven gone: undetermined; the record stays, this call ends
               /\ laneRow' = row1
               /\ wpc' = [wpc EXCEPT ![w] = "idle"]
               /\ report' = [report EXCEPT ![Att(w)] = "Unresolved"]
               /\ UNCHANGED <<bound, retained>>
    /\ UNCHANGED <<nextB, wconn, lconn, quar, sgen, gen, turn, kills, lkills,
                   losts, bumps, head, hist, cnt, lhead, hw, ksv, bad>>

\* The fused Commit, one pipelined message; it waits for the lane row.
WFused(w) ==
    LET s == SvcOf(w) b == wconn[s]
        o == FusedOutcome(bound[w], w \in Force, sgen[s])
    IN
    /\ wpc[w] = "commit" /\ Ready(w) /\ LaneFree(b)
    /\ RunFused(Att(w), bound[w], w \in Force, sgen[s], s)
    /\ IF o = "Moved"
       THEN /\ wpc' = [wpc EXCEPT ![w] = "hold"]        \* merge in the lane
            /\ UNCHANGED <<report, bound>>
       ELSE /\ wpc' = [wpc EXCEPT ![w] = "idle"]
            /\ report' = [report EXCEPT ![Att(w)] = o]
            /\ bound' = [bound EXCEPT ![w] = IF o \in {"Committed", "Displaced"}
                                                THEN Att(w) ELSE @]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, quar, lost, sgen,
                   gen, pend, kills, lkills, losts, bumps, cnt, retained, lhead,
                   hw, ksv>>

\* Its reply is lost: it ran, or the backend still holds it.  The Commit
\* is retained (Unknown); nothing is resent.
WFusedLost(w) ==
    LET s == SvcOf(w) b == wconn[s] IN
    /\ wpc[w] = "commit" /\ Ready(w) /\ losts < MaxLost
    /\ losts' = losts + 1
    /\ \/ /\ LaneFree(b)
          /\ RunFused(Att(w), bound[w], w \in Force, sgen[s], s)
          /\ UNCHANGED pend
       \/ /\ pend' = [pend EXCEPT ![b] = [kind |-> "fused", att |-> Att(w),
                        exp |-> bound[w], frc |-> w \in Force, g |-> sgen[s]]]
          /\ UNCHANGED <<hist, head, bad>>
    /\ Quarantine(s)
    /\ wpc' = [wpc EXCEPT ![w] = "idle"]
    /\ report' = [report EXCEPT ![Att(w)] = "Unknown"]
    /\ retained' = [retained EXCEPT ![w] = Att(w)]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, sgen, gen, kills,
                   lkills, bumps, cnt, bound, lhead, hw, ksv>>

\* hold_lane: a lane transaction on the lane connection (opened on first
\* use or after it died), FOR UPDATE plus the barrier pass; the head is
\* read under the lane and the merge done.  The writer's turn is kept for
\* the whole hold.
WHold(w) ==
    LET s == SvcOf(w)
        nb == nextB
        lb == IF lconn[s] # 0 /\ lconn[s] \in alive THEN lconn[s] ELSE nb
    IN
    /\ wpc[w] = "hold" /\ turn[s] \in {"none", w}
    /\ lb \in Backends /\ LaneFree(lb)
    /\ nextB' = IF lb = nb THEN nb + 1 ELSE nextB
    /\ alive' = alive \cup {lb}
    /\ lconn' = [lconn EXCEPT ![s] = lb]
    /\ laneRow' = lb
    /\ lhead' = [lhead EXCEPT ![w] = head]
    /\ turn' = [turn EXCEPT ![s] = w]
    /\ wpc' = [wpc EXCEPT ![w] = "nbegin"]
    /\ UNCHANGED <<wconn, barSh, quar, lost, sgen, gen, pend, kills, lkills,
                   losts, bumps, head, hist, cnt, bound, retained, report, hw,
                   ksv, bad>>

\* The nested Publish: BEGIN with the fence and the barrier key shared.
\* Inside a held lane a quarantined writer never reopens.
WNBegin(w) ==
    LET s == SvcOf(w) b == wconn[s] IN
    /\ wpc[w] = "nbegin" /\ turn[s] = w
    /\ IF quar[s] \/ b \notin alive \/ (sgen[s] # gen /\ Mutation # "nofence")
       THEN /\ wpc' = [wpc EXCEPT ![w] = "release"]
            /\ report' = [report EXCEPT ![Att(w)] =
                            IF quar[s] \/ b \notin alive THEN "Closed" ELSE "Stale"]
            /\ UNCHANGED barSh
       ELSE /\ barSh' = barSh \cup {b}
            /\ wpc' = [wpc EXCEPT ![w] = "ncommit"]
            /\ UNCHANGED report
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, quar, lost, sgen, gen,
                   turn, pend, kills, lkills, losts, bumps, head, hist, cnt,
                   bound, retained, lhead, hw, ksv, bad>>

\* The lane check and COMMIT in one round trip: the server runs the check
\* statement, then the COMMIT (two steps another backend can fall between).
WNCheck(w) ==
    LET s == SvcOf(w) b == wconn[s] IN
    /\ wpc[w] = "ncommit" /\ turn[s] = w /\ b \in alive
    /\ IF NestedOk(s)
       THEN /\ wpc' = [wpc EXCEPT ![w] = "nfinish"]
            /\ UNCHANGED <<report, barSh, bad>>
       ELSE /\ report' = [report EXCEPT ![Att(w)] = "Closed"]
            /\ barSh' = barSh \ {b}
            /\ wpc' = [wpc EXCEPT ![w] = "release"]
            /\ UNCHANGED bad
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, quar, lost, sgen, gen,
                   turn, pend, kills, lkills, losts, bumps, head, hist, cnt,
                   bound, retained, lhead, hw, ksv>>

\* The COMMIT after a passed check.  ("ClosedWrite" is judged at the check:
\* the lane may die between the check and this COMMIT, and the barrier
\* keeps the next holder out until it ends.)
WNFinish(w) ==
    LET s == SvcOf(w) b == wconn[s] IN
    /\ wpc[w] = "nfinish" /\ turn[s] = w /\ b \in alive
    /\ Install(Att(w), lhead[w], FALSE, sgen[s], FALSE, s)
    /\ report' = [report EXCEPT ![Att(w)] = "Committed"]
    /\ bound' = [bound EXCEPT ![w] = Att(w)]
    /\ barSh' = barSh \ {b}
    /\ wpc' = [wpc EXCEPT ![w] = "release"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, quar, lost, sgen, gen,
                   turn, pend, kills, lkills, losts, bumps, cnt, retained,
                   lhead, hw, ksv>>

\* Its reply lost, or the writer's backend died under it: Unknown.
WNCommitLost(w) ==
    LET s == SvcOf(w) b == wconn[s] IN
    /\ wpc[w] \in {"ncommit", "nfinish"} /\ turn[s] = w
    /\ \/ /\ b \notin alive                       \* the transaction died
          /\ UNCHANGED <<losts, hist, head, bad, pend, barSh>>
       \/ /\ b \in alive /\ losts < MaxLost /\ wpc[w] = "ncommit"
          /\ losts' = losts + 1
          /\ \/ /\ IF NestedOk(s)
                   THEN Install(Att(w), lhead[w], FALSE, sgen[s], TRUE, s)
                   ELSE UNCHANGED <<hist, head, bad>>
                /\ barSh' = barSh \ {b}
                /\ UNCHANGED pend
             \/ /\ pend' = [pend EXCEPT ![b] = [kind |-> "nested", att |-> Att(w),
                              parent |-> lhead[w], g |-> sgen[s], s |-> s]]
                /\ UNCHANGED <<hist, head, bad, barSh>>
    /\ Quarantine(s)
    /\ report' = [report EXCEPT ![Att(w)] = "Unknown"]
    /\ retained' = [retained EXCEPT ![w] = Att(w)]
    /\ wpc' = [wpc EXCEPT ![w] = "release"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, sgen, gen, turn, kills,
                   lkills, bumps, cnt, bound, lhead, hw, ksv>>

\* Dropping LaneHold rolls the lane transaction back and frees the turn.
WRelease(w) ==
    LET s == SvcOf(w) IN
    /\ wpc[w] = "release"
    /\ laneRow' = IF laneRow = lconn[s] THEN 0 ELSE laneRow
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ wpc' = [wpc EXCEPT ![w] = "idle"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, barSh, quar, lost, sgen, gen,
                   pend, kills, lkills, losts, bumps, head, hist, cnt, bound,
                   retained, lhead, report, hw, ksv, bad>>

----------------------------------------------------------------------------
(* Id allocation: a block reserved by compare-and-set on the highwater,    *)
(* ids handed out from it, the unused tail rewound by compare-and-set.     *)

KRes(k) ==
    LET s == SvcOf(k) b == wconn[s] IN
    /\ kpc[k] = "kres" /\ Ready(k) /\ LaneFree(b)
    /\ \/ /\ hw' = hw + Blk
          /\ kblock' = [kblock EXCEPT ![k] = <<hw, hw + Blk>>]
          /\ kpc' = [kpc EXCEPT ![k] = "kuse"]
          /\ UNCHANGED <<losts, pend, quar, lost>>
       \/ /\ losts < MaxLost                \* Unknown: the block is never used
          /\ losts' = losts + 1
          /\ \/ /\ hw' = hw + Blk /\ UNCHANGED pend
             \/ /\ pend' = [pend EXCEPT ![b] = [kind |-> "res", from |-> hw]]
                /\ UNCHANGED hw
          /\ Quarantine(s)
          /\ kpc' = [kpc EXCEPT ![k] = "kdone"]
          /\ UNCHANGED kblock
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, sgen, gen,
                   kills, lkills, bumps, head, hist, wsv, kround, used, bad>>

KUse(k) ==
    LET id == kblock[k][1] IN
    /\ kpc[k] = "kuse"
    /\ used' = used \cup {id}
    /\ bad' = IF id \in used THEN bad \cup {"IdReused"} ELSE bad
    /\ kpc' = [kpc EXCEPT ![k] = "krew"]
    /\ UNCHANGED <<srv, head, hist, wsv, hw, kround, kblock>>

\* Rewind: UPDATE highwater = first unused WHERE highwater = block end.
KRew(k) ==
    LET s == SvcOf(k) b == wconn[s]
        exp == IF Mutation = "nocas" THEN hw ELSE kblock[k][2]
        to == kblock[k][1] + 1
        next == IF kround[k] < KRounds THEN "kres" ELSE "kdone"
    IN
    /\ kpc[k] = "krew" /\ Ready(k) /\ LaneFree(b)
    /\ \/ /\ hw' = IF hw = exp THEN to ELSE hw
          /\ UNCHANGED <<losts, pend, quar, lost>>
       \/ /\ losts < MaxLost                  \* Unknown: the kept ids are lost
          /\ losts' = losts + 1
          /\ \/ /\ hw' = (IF hw = exp THEN to ELSE hw) /\ UNCHANGED pend
             \/ /\ pend' = [pend EXCEPT ![b] = [kind |-> "rew", exp |-> exp, to |-> to]]
                /\ UNCHANGED hw
          /\ Quarantine(s)
    /\ kpc' = [kpc EXCEPT ![k] = next]
    /\ kround' = [kround EXCEPT ![k] = @ + 1]
    /\ turn' = [turn EXCEPT ![s] = "none"]
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, sgen, gen,
                   kills, lkills, bumps, head, hist, wsv, kblock, used, bad>>

----------------------------------------------------------------------------
(* Server events and faults.                                               *)

\* A request whose reply was lost runs after all.
LateExec(b) ==
    LET r == pend[b] IN
    /\ b \in alive /\ r # NoReq
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ CASE r.kind = "fused" ->
              /\ LaneFree(b)
              /\ RunFused(r.att, r.exp, r.frc, r.g, SvcOf(r.att[1]))
              /\ UNCHANGED <<hw, barSh>>
         [] r.kind = "nested" ->
              /\ IF NestedOk(r.s) THEN Install(r.att, r.parent, FALSE, r.g, TRUE, r.s)
                 ELSE UNCHANGED <<hist, head, bad>>
              /\ barSh' = barSh \ {b}
              /\ UNCHANGED hw
         [] r.kind = "res" ->
              /\ LaneFree(b)
              /\ hw' = IF hw = r.from THEN hw + Blk ELSE hw
              /\ UNCHANGED <<hist, head, bad, barSh>>
         [] r.kind = "rew" ->
              /\ LaneFree(b)
              /\ hw' = IF hw = r.exp THEN r.to ELSE hw
              /\ UNCHANGED <<hist, head, bad, barSh>>
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, quar, lost, sgen, gen,
                   turn, kills, lkills, losts, bumps, wsv, ksv>>

\* A backend's end: its locks, open transaction and unsent work go.
Gone(b) ==
    /\ alive' = alive \ {b}
    /\ pend' = [pend EXCEPT ![b] = NoReq]
    /\ barSh' = barSh \ {b}
    /\ laneRow' = IF laneRow = b THEN 0 ELSE laneRow

\* Fault: a writer's backend is killed (terminated, crashed, dropped).
KillWriter(b) ==
    /\ kills < MaxKill /\ b \in alive /\ b \in {wconn[s] : s \in Svcs}
    /\ kills' = kills + 1
    /\ Gone(b)
    /\ UNCHANGED <<nextB, wconn, lconn, quar, lost, sgen, gen, turn, lkills,
                   losts, bumps, head, hist, wsv, hw, ksv, bad>>

\* Fault: a lane connection dies mid-lane (idle timeout, failover, admin).
KillLane(b) ==
    /\ lkills < MaxLaneKill /\ b \in alive /\ b \in {lconn[s] : s \in Svcs}
    /\ lkills' = lkills + 1
    /\ Gone(b)
    /\ UNCHANGED <<nextB, wconn, lconn, quar, lost, sgen, gen, turn, kills,
                   losts, bumps, head, hist, wsv, hw, ksv, bad>>

\* A closed connection's backend ends (after or without running what it had).
BackendExit(b) ==
    /\ b \in alive
    /\ ~(\E s \in Svcs : (wconn[s] = b /\ ~quar[s]) \/ lconn[s] = b)
    /\ Gone(b)
    /\ UNCHANGED <<nextB, wconn, lconn, quar, lost, sgen, gen, turn, kills,
                   lkills, losts, bumps, head, hist, wsv, hw, ksv, bad>>

\* Fault: compaction swap, restore or upgrade.  The swap takes the lane,
\* then LOCK ... ACCESS EXCLUSIVE NOWAIT, which no open fenced
\* transaction (a nested write) allows.
GenBump ==
    /\ bumps < MaxBump /\ laneRow = 0 /\ barSh = {}
    /\ bumps' = bumps + 1
    /\ gen' = gen + 1
    /\ UNCHANGED <<alive, nextB, wconn, lconn, laneRow, barSh, quar, lost, sgen,
                   turn, pend, kills, lkills, losts, head, hist, wsv, hw, ksv, bad>>

----------------------------------------------------------------------------
Init ==
    /\ alive = {1, 2}
    /\ nextB = 3
    /\ wconn = [s \in Svcs |-> IF s = "S1" THEN 1 ELSE 2]
    /\ lconn = [s \in Svcs |-> 0]
    /\ laneRow = 0
    /\ barSh = {}
    /\ quar = [s \in Svcs |-> FALSE]
    /\ lost = [s \in Svcs |-> {}]
    /\ sgen = [s \in Svcs |-> 1]
    /\ gen = 1
    /\ turn = [s \in Svcs |-> "none"]
    /\ head = Root
    /\ hist = << >>
    /\ pend = [b \in Backends |-> NoReq]
    /\ wpc = [w \in Ws |-> "idle"]
    /\ cnt = [w \in Ws |-> 0]
    /\ bound = [w \in Ws |-> Root]
    /\ retained = [w \in Ws |-> Root]
    /\ lhead = [w \in Ws |-> Root]
    /\ report = [t \in Atts |-> "none"]
    /\ hw = 0
    /\ kpc = [k \in Ks |-> "kres"]
    /\ kround = [k \in Ks |-> 1]
    /\ kblock = [k \in Ks |-> <<0, 0>>]
    /\ used = {}
    /\ kills = 0 /\ lkills = 0 /\ losts = 0 /\ bumps = 0
    /\ bad = {}

Next ==
    \/ \E a \in Actors : Refuse(a) \/ SendDead(a) \/ Reopen(a)
    \/ \E w \in Ws : WStart(w) \/ WResolve(w) \/ WFused(w) \/ WFusedLost(w)
                     \/ WHold(w) \/ WNBegin(w) \/ WNCheck(w) \/ WNFinish(w)
                     \/ WNCommitLost(w)
                     \/ WRelease(w)
    \/ \E k \in Ks : KRes(k) \/ KUse(k) \/ KRew(k)
    \/ \E b \in Backends : LateExec(b) \/ KillWriter(b) \/ KillLane(b) \/ BackendExit(b)
    \/ GenBump

Spec == Init /\ [][Next]_vars

----------------------------------------------------------------------------
(* Properties (the lane, head, attempt, fence, lane-liveness and id rows).  *)

\* History writes are totally ordered and none is lost: every Commit row's
\* parent is the head it replaced, unless forced (recorded displaced).
NoLostUpdate == "LostUpdate" \notin bad
ForceOnlyDisplaces == "DisplacedWithoutForce" \notin bad
TotalOrder ==
    \A i \in 1..Len(hist) :
        hist[i].disp \/ hist[i].parent = (IF i = 1 THEN Root ELSE hist[i - 1].att)
\* An attempt is installed at most once; a Commit reported committed is
\* installed; one reported definitely not committed never is.
AttemptOnce == "AttemptTwice" \notin bad
ReportsExact ==
    \A t \in Atts :
        /\ report[t] \in {"Committed", "Displaced"} => t \in HistAtts
        /\ report[t] \in {"Moved", "Stale", "Closed", "Refused", "NotCommitted"}
               => t \notin HistAtts
\* No write of a session fenced on an old generation lands (StaleStore).
FenceHolds == "StaleWrite" \notin bad
\* No nested write commits after its lane connection died (Closed).
LaneLiveness == "ClosedWrite" \notin bad
\* No id is handed out twice.
IdsUnique == "IdReused" \notin bad
=============================================================================
