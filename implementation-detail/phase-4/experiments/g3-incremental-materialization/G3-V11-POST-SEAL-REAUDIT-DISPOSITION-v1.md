# G3-v11 post-seal independent re-audit disposition

Date: 2026-08-22

Disposition: **G3-v11 historical REVISE; sealed evidence integrity PASS; G4
measured execution remains NO-GO**

This additive record does not modify, relabel, chmod, rerun, or supersede bytes
inside the read-only v11 result package. It distinguishes two facts:

1. the v11 campaign, analyses, manifest, static closure, terminal, modes, and
   hashes remain reproducible evidence of what v11 executed; and
2. independent source/contract review found defects that invalidate v11's
   unqualified G3 acceptance claim.

## Immutable v11 anchors reverified

| Artifact | SHA-256 |
|---|---|
| source set | `45a08ba60b02316bc803cca69871e773751009bb9f9196fb9c03e8c7ad705821` |
| methodology set | `e7194aa398476a06a0706e93e1af70e06a15a9fb662be7d20014117f463856d8` |
| campaign | `9227c2eb31c8d897e163aceed0e2724c5d3d7617896fcb0069207ba061e7ef16` |
| raw JSONL | `47d979b9f687be75bfbc816608678b8ea1ef43e1317a3e7f9437abf7d5b93191` |
| primary analysis | `0225e6e67411af363b8dcb1868d70572cf9dc8a4a9a76d295cd655ab29f8bbc3` |
| independent recomputation | `4f09586396e5bb35c5b758a7a91a3447283990f168edefca3d0006ecdbfb9366` |
| static closure | `6de469522152ee2adf48c05e563fbf75d52cdbc312f4bc898e3d834e8b17c2ee` |
| payload manifest | `2950a6698983718e8c386a782b975e1ef807fa7a9ecf95cd59396d2473f3b27e` |
| terminal | `222bdc2abef4cd1435c6baec82a35bf05756e1aa385b10ae206bd27f9c6c351a` |
| terminal verification | `995084a7ae284b940b951d9c67680d61d3ee56b350cac55df546dfcd883f99a8` |

The sealed v11 module source is
`5ecc718a3e0595b79910d1bc12353318dd6d55ffff2d2c856118a7cfc14691e8`.
No v11 measurement or terminal artifact is reused by v12.

## Acceptance defects

1. The lost-ack comparison owned a fixed 32768-byte buffer but v11 did not
   charge it to reconciliation Q. The reported 24081-byte reconciliation
   high-water therefore contradicted the stated buffers/state definition.
2. Temp and seed creation acquired their cleanup-capable directory handle only
   after creating the named file. Failure in that gap could leave a private
   `.g3-tmp-*` or `.g3-seed-*` name without an armed guard.
3. On a prior/old reconciliation result, cleanup failure could replace the
   original rename or directory-sync publication error, violating exact
   publication-first error precedence and losing one provenance.
4. Permit preparation checked the mutable fixtures before canonical roots were
   built, but did not prove from stable no-follow preparation descriptors that
   the authenticated canonical parent and target roots differed only inside the
   declared range. The measured private harness remained byte-exact, but the
   production-shaped locality authority claim was incomplete.

These are evidence/authority correctness defects, not a claim that v11 rows
were rerun or fabricated. The nine v11 observations—including its bounded
changed-range counters, full authenticated fallbacks, exact outputs, old-or-new
publication results, and wall/RSS/storage measurements—remain historical
descriptive observations only. They do not authorize G4.

## Required successor

v12 must freshly bind repaired source and methodology, reject lost-ack
`reconciliation_q_high_water < 32768`, acquire cleanup capability before each
name-creating syscall, retain both publication and cleanup typed details with
publication primary, and consume a canonical parent/target range proof keyed
into the permit. It retains the complete authenticated fallback, fresh
one-shot nine-row schedule, zero reruns, source/method/binary custody, durable
row cleanup, sealed G2-v5 dependency, and one final static closure. Until v12
has a new sealed terminal PASS, G3 is REVISE and G4 remains unstarted.
