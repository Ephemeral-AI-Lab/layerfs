# Canonical-v2 compact closure — prospective contract

Date: 2026-08-21. This is a fresh, non-retroactive closure attempt. The sealed
`final-candidate-v1` REVISE bundle and its clarification remain historical
authority and are not relabeled.

## Global boundary

One monotonic supervisor starts before incremental check and ends only after
focused tests, one release build, custody, preparation, rows, analysis,
disposition, manifest generation, and manifest verification. The hard limit is
119 seconds (strictly below 120). A timeout is `CANONICAL-V2 REVISE / TIME-BUDGET`.
Every child is limited to the smaller of 59 seconds or the remaining global
time. There is one lock and one fresh result namespace. Started evidence is
never deleted or rerun.

The validation commands are exactly one incremental `cargo check`, one filtered
test command selecting exactly the three `compact_v2_*` tests, and one release
build. The sealed independent oracle is rehashed, not rerun, because the codec
and oracle bytes are unchanged.

## Preparation and rows

The 1/10/100 MiB sources are generated once by the frozen CP-0009 operand and
must match their sealed identities. One empty full-create master is prepared per
arm and size. One published 100-MiB edit/read master is prepared per arm. All
rows are physical byte copies. Operation expectations are batch-derived from
those immutable masters; no 100-MiB published base is rebuilt per operation.

The exact row order is:

1. global 100-MiB full-create warmup `A B`;
2. 1-MiB full-create scaling `A B`;
3. 10-MiB full-create scaling `B A`;
4. 100-MiB full-create pair 0 `A B`;
5. 100-MiB full-create pair 1 `B A`;
6. 100-MiB comparable guards, alternating order:
   `same-middle AB`, `plus1-early BA`, `plus1-middle AB`,
   `materialize-warm BA`, `materialize-fresh AB`, `reopen BA`, `range1m AB`;
7. candidate-only guards:
   `one-byte-early`, `one-byte-middle`, `one-byte-late`,
   `first-edit-after-reopen`, `scrub-only`.

This is 29 rows: 2 warmup, 8 full-create measured/scaling rows, 14 comparable
guard rows, and 5 candidate-only guard rows. The candidate-only rows make no v1
speed claim. Fresh-process materialization is OS/filesystem cache
warm-or-unknown, never cold.

## Frozen decisions

Correctness is hard: exact operand/source/profile/CDC/canonical identities;
strict v2 head/receipt; successful rows; terminal Q zero and high-water at most
131,072 bytes for full create or 4,194,304 bytes for bounded CDC/read guards;
one writer transaction and one publication COMMIT for mutations/full create;
zero transactions for reads; DELETE/FULL; timer equations; no journal/WAL/SHM
residue; exact one-byte offset/old/new/fingerprint/reference-count evidence; and
the first-edit equation

`total = reopen/head + authority establishment/full scrub + edit publication`.

Both adjacent 100-MiB full-create comparisons must favor canonical-v2. The
position-balanced center is the arithmetic mean of the two arm observations.
The 1/10 rows are scaling checks. A comparable lifecycle guard is a material
regression only when candidate wall exceeds control wall by both 20 ms and 50%;
otherwise its result is descriptive. Candidate-only guards have no performance
gate. Instructions/cycles and physical I/O are unavailable unless a stable
unprivileged observer actually reports them; wall, user/system CPU, RSS,
logical/apparent/allocated storage, SQL/BLOB/pager/Q are reported without
inferring unavailable quantities.

PASS means fresh-store canonical-v2 is eligible for promotion with automatic v1
migration deferred. Any hard/custody/timer/build/budget failure is REVISE and
CP-0009 remains accepted. No later optimization or commit is authorized.
