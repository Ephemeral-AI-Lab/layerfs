import LayerfsGc.Objects
import LayerfsGc.Capture
import LayerfsGc.Delta

/-!
# A worked example

One live project (LayerStack 1) and one deleted project (LayerStack 5).

* `main` (Branch 2) is live. Its head Commit 22 merged Commit 23 from
  `feature`.
* `feature` (Branch 3) is retired, but Commit 23 names it in `branch_id`,
  so `feature` and its head stay (#42 item 29).
* `old` (Branch 4) is retired and nothing names it or its Commit 24, so
  both go.
* LayerStack 5 is deleted: all of its rows go.

`expandRef` is a reference model of `kept_node_expand`: a breadth-first
walk that skips kept nodes, truncated to the row limit. The driver runs it
with a batch of 2 and a row limit of 4, so most statements stop at their
limit and the re-follow path is exercised.

`specEdges` is the edge list in the spec text of §6.1 before #42 items 28
and 29: no `merged_from_id`, no `displaced_id`, no `commit.branch_id` and
no `layer.source_branch_id`. Pruning to its closure fails the doctor, which
is code map findings 2 and 3.
-/

namespace LayerfsGc.Examples

open LayerfsGc LayerfsGc.History LayerfsGc.Objects

def bfs {α : Type} [BEq α] (edges : α → List α) (kept : List α) :
    Nat → List α → List α → List α
  | 0, _, acc => acc
  | _ + 1, [], acc => acc
  | n + 1, x :: q, acc =>
    let next := (edges x).eraseDups.filter (fun y => !(kept.contains y) && !(acc.contains y))
    bfs edges kept n (q ++ next) (acc ++ next)

/-- `kept_node_expand` with `LIMIT limit`: the batch rows count toward the
limit, and only nodes that were not kept are returned. -/
def expandRef {α : Type} [BEq α] (edges : α → List α) (limit : Nat)
    (kept batch : List α) : List α :=
  (bfs edges kept 1000 batch []).take (limit - batch.length)

/-- `ExpandOk` as a Boolean check, without `reached`. -/
def expandOkB {α : Type} [BEq α] (edges : α → List α) (kept batch : List α)
    (limit : Nat) (new : List α) : Bool :=
  new.all (fun y => !(kept.contains y)) &&
  (!(new.length + batch.length < limit) ||
    (batch ++ new).all (fun x => (edges x).all (fun y => kept.contains y || new.contains y)))

/-- Runs the driver and checks every statement it issues against the
contract; also reports whether some statement stopped at its limit. -/
def runChecked {α : Type} [BEq α] (edges : α → List α) (bn br : Nat) :
    Nat → State α → Bool × Bool
  | 0, _ => (false, false)
  | fuel + 1, s =>
    match s.cur with
    | [] => match s.found with
      | [] => (true, false)
      | f => runChecked edges bn br fuel ⟨s.kept, f, []⟩
    | c =>
      let batch := c.take bn
      let new := expandRef edges br s.kept batch
      let short := new.length + batch.length < br
      let next : State α := if short then ⟨s.kept ++ new, c.drop bn, s.found⟩
        else ⟨s.kept ++ new, c.drop bn, s.found ++ new ++ batch⟩
      let (ok, cut) := runChecked edges bn br fuel next
      (ok && expandOkB edges s.kept batch br new, cut || !short)

def store : Store where
  rows := [
    { kind := .stack, id := 1, stack := 1, head := some 11 },
    { kind := .layer, id := 10, stack := 1, root := some 100 },
    { kind := .layer, id := 11, stack := 1, root := some 101, parent := some 10,
      srcCommit := some 21, srcBranch := some 2 },
    { kind := .branch, id := 2, stack := 1, base := some 10, head := some 22 },
    { kind := .branch, id := 3, stack := 1, base := some 10, head := some 23,
      fork := some (.commit 21) },
    { kind := .branch, id := 4, stack := 1, base := some 10, head := some 24 },
    { kind := .commit, id := 21, stack := 1, root := some 200, base := some 10,
      branch := some 2 },
    { kind := .commit, id := 22, stack := 1, root := some 201, parent := some 21,
      base := some 10, branch := some 2, merged := some (.commit 23) },
    { kind := .commit, id := 23, stack := 1, root := some 202, parent := some 21,
      base := some 10, branch := some 3 },
    { kind := .commit, id := 24, stack := 1, root := some 203, parent := some 21,
      base := some 10, branch := some 4 },
    { kind := .stack, id := 5, stack := 5, head := some 50 },
    { kind := .layer, id := 50, stack := 5, root := some 300 },
    { kind := .branch, id := 6, stack := 5, base := some 50, head := some 60 },
    { kind := .commit, id := 60, stack := 5, root := some 301, base := some 50,
      branch := some 6 } ]
  retiredBranches := [3, 4]
  retiredStacks := [5]

def presentInB (s : Store) (t : Node) (k : Nat) : Bool :=
  s.rows.any (fun r => r.node == t && r.stack == k)

/-- `Doctor` as a Boolean check. -/
def doctorB (s : Store) : Bool :=
  s.rows.all (fun r => (doctorRequired r).all (fun t => presentInB s t r.stack)) &&
  s.rows.all (fun r => r.kind == .stack || presentInB s (.stack, r.stack) r.stack) &&
  s.rows.all (fun r => r.kind != .stack || r.stack == r.id)

def markWith (edges : Node → List Node) : List Node :=
  (run (expandRef edges 4) 2 4 1000 (State.start (seed store))).getD []

def kept : List Node := markWith (sqlEdges store)

def keptIds (k : Kind) (K : List Node) : List Nat :=
  ((K.filter (fun n => n.1 == k)).map Prod.snd).mergeSort

#guard doctorB store
#guard keptIds .stack kept == [1]
#guard keptIds .branch kept == [2, 3]
#guard keptIds .commit kept == [21, 22, 23]
#guard keptIds .layer kept == [10, 11]
#guard doctorB (prune store kept)
-- Every statement met the contract, and at least one stopped at its limit.
#guard runChecked (sqlEdges store) 2 4 1000 (State.start (seed store)) == (true, true)

/-- The §6.1 edge list as first written: Branch head/base/fork, Layer
parent/source Commit, Commit parent/second/base. -/
def specTargets (r : Row) : List Node :=
  match r.kind with
  | .branch => inTable .commit (col .commit r.head ++ ref r.fork) ++
      inTable .layer (col .layer r.base ++ ref r.fork)
  | .layer => col .layer r.parent ++ col .commit r.srcCommit
  | .commit => col .commit r.parent ++ ref r.second ++ col .layer r.base
  | .stack => []

def specEdges (s : Store) (n : Node) : List Node :=
  (s.rows.filter (fun r => r.node == n)).flatMap
    (fun r => (specTargets r).filter (present s))

-- Without the merge and Branch edges, Commit 23 and `feature` are
-- collected while Commit 22 still names them: the doctor fails.
#guard keptIds .commit (markWith (specEdges store)) == [21, 22]
#guard !(doctorB (prune store (markWith (specEdges store))))

/-! Objects: roots 100, 101, 200–202 are kept. 203 (Commit 24) and
300–301 (LayerStack 5) are not. 400 is shared by a kept and a collected
root; 401 is kept only as the delta base of 402. -/

def objs : List Obj := [
  { id := 100, bytes := 1, refs := [400] },
  { id := 101, bytes := 2, refs := [400, 402] },
  { id := 200, bytes := 3, refs := [] },
  { id := 201, bytes := 4, refs := [] },
  { id := 202, bytes := 5, refs := [] },
  { id := 203, bytes := 6, refs := [400, 403] },
  { id := 300, bytes := 7, refs := [] },
  { id := 301, bytes := 8, refs := [] },
  { id := 400, bytes := 9, refs := [] },
  { id := 401, bytes := 10, refs := [] },
  { id := 402, bytes := 11, refs := [], base := some 401 },
  { id := 403, bytes := 12, refs := [] } ]

def keptObjs : List Nat :=
  ((run (expandRef (objEdges objs) 4) 2 4 1000
    (State.start (objectRoots store kept))).getD []).mergeSort

#guard keptObjs == [100, 101, 200, 201, 202, 400, 401, 402]
#guard (retain objs keptObjs).all (fun o =>
  (o.refs ++ o.base.toList).all (fun j => keptObjs.contains j))

end LayerfsGc.Examples

/-! ## #2, #4, #5 on small inputs: the hypotheses can all be met -/

namespace LayerfsGc.Examples2

open LayerfsGc

-- #5: cap 2 for role 0. FULL 1, then 2 on 1, then 3 on 2 (depth 2 = cap).
-- Every record decodes with depth + 1 records in its chain.
def deltaOps : List Delta.Op :=
  [.publish ⟨1, 0, none⟩, .publish ⟨2, 0, some 1⟩, .publish ⟨3, 0, some 2⟩,
   .publish ⟨2, 0, none⟩,            -- a second copy of 2 loses (first wins)
   .compact [3, 2, 1]]
def deltaStore : Delta.DStore := Delta.applyAll Delta.DStore.empty deltaOps

#guard deltaStore.rows.map (fun x => (x.record.id, x.depth)) == [(1, 0), (2, 1), (3, 2)]
#guard deltaStore.rows.all (fun x =>
  (Delta.read deltaStore (fun _ => 2) x).map List.length == some (x.depth + 1))

-- #4: generation 1 was captured over floor 0; generation 2 holds a later
-- write to key 7. The view before install equals the view after.
def orows : List (Capture.ORow Nat Nat) :=
  [⟨7, 1, 70⟩, ⟨8, 1, 80⟩, ⟨7, 2, 71⟩]
def base0 : Nat → Nat := fun k => k
#guard (List.range 10).all (fun k =>
  Capture.view orows base0 0 2 k ==
    Capture.view orows (Capture.construct orows base0 0 1) 1 2 k)

end LayerfsGc.Examples2
