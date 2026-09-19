# Are the overheads from large→small or small→large transitions?

**Answer: neither.** The overheads were never from transitions, and the transition axis contributes
**0 B** to the gap against v0.1.6.

## D1 · The original gap — every item is SAME-ROLE

````text
  the 8,433,664 B gap, as measured at T1's best (before the fallback and L4)

   whole-file lane      4,890,202   58.0 %   WholeFile -> WholeFile, cross-save, SAME ROLE
   native (chunk) lane  1,737,621   20.6 %   Chunk -> Chunk, in-place edits, SAME ROLE
   row grammar          1,474,560   17.5 %   the objects table, NOT a transition
   ordinary lane          344,456    4.1 %   tree roles, NOT a transition
   pooled-metadata       -221,539   -2.6 %   we win
   framing                118,252    1.4 %
   ---------------------------------------------------------------------
   of which TRANSITIONS       0 B    0.0 %   <- see D3
````

## D2 · The transition population, if you did want it

````text
   small -> large     17 events    2,523,964 B result    ceiling 355,827 B   (75 %)
   large -> small      7 events      478,194 B result    measured 120,490 B  (25 %)
   ---------------------------------------------------------------------------
   24 events on 16 distinct paths / 22 distinct (path, direction) pairs
                                                          total <= 476,317 B
````

**small → large is the bigger half, about 3:1** — a file grows past 128 KiB, becomes chunked, and its
previous version is one whole-file object that cannot serve a chunk.

## D3 · Why it is 0 B against v0.1.6 — both trees refuse it

````text
   ours,    native lane   448 FULL + 650 PREFIX  =  3,957,829 B
   v0.1.6,  native lane   448 FULL + 650 PREFIX  =  3,958,057 B
                          ^^^^^^^^^^^^^^^^^^^^^
                          BYTE-IDENTICAL SELECTION

   "Neither tree admits a cross-role base"   (deferred/01-size-transition-delta-hints.md, state 3)

     a WholeFile base cannot serve a Chunk target     (eligible() requires role equality)
     a FileState base cannot serve a WholeFile target

   so BOTH generations store the crossing objects FULL.
   -> the transition axis is a SHARED LIMITATION, not a regression.
   -> serving it would beat v0.1.6, not match it.
````

## D4 · Where the remaining 356,352 B actually is — it closes exactly

````text
                                    ours         v0.1.6         delta
   pack total                  44,558,708     46,056,732    -1,498,024   <- WE WIN
     whole-file                37,347,557     38,983,278    -1,635,721   <- WE WIN
     native                     3,957,829      3,958,057          -228   <- WE WIN
     pooled-metadata            2,019,419      2,240,958      -221,539   <- WE WIN
     ordinary                   1,022,795        678,339      +344,456   <- we lose
     framing                      211,108        196,100       +15,008
   non-pack                     5,113,484      3,259,108    +1,854,376   <- we lose
     objects                    4,284,416      2,650,112    +1,634,304   <- THE dominant item
     metadata groups + autoindex   118,784         24,576       +94,208
     14 LayerStack tables                0         57,344       -57,344   (v0.1.6 has them, we do not)
     sqlite_schema + store_policy    8,192         12,288        -4,096
   -------------------------------------------------------------------------
   apparent                    49,672,192     49,315,840      +356,352
   check: 1,854,376 - 1,498,024 = 356,352   residual 0
````

**We are 1,498,024 B BETTER than v0.1.6 on the packed content, and 1,854,376 B WORSE on the row
grammar — and the row grammar is dominated by one table: `objects`, +1,634,304 B.**

## D5 · One line

````text
   the overheads were:  whole-file bases, chunk bases, and the objects table
   NOT:                 transitions

   the transition axis is worth <= 476,317 B and costs ZERO today,
   because v0.1.6 has exactly the same limitation.
````
