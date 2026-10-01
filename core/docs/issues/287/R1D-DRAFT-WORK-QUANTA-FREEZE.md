# R1D draft reference preparation quanta

Prospective source contract, frozen before source changes on 2026-10-01. Root
owns compilation, runtime checks and retained receipts. This is not a deadline,
performance, native memory or global qualification claim.

The only production responsibility is `construction_state/draft_write.rs`.
Other draft creation, retirement, temporary-pin, adapter and schema files remain
with their existing owners. External vectors are in the new
`tests/draft_reference_quanta.rs`; existing real COMMIT Unknown vectors remain
reusable when their recorded source identities and covering scope match.

## Closed local statement lifetime

The complete before/proposed `Attempt` and its original `Vec<Effect>` remain
installed before BEGIN. All existing admission checks remain: at most128 effects
and actual Vec capacity128, encoded window65536, captured records/aggregate bytes,
job sequence, exact owner verification, native reservation and explicit engine
validation. SQL/schema, quotas, cache policy and grants are unchanged.

Inside that same transaction, walk the original slice in order. Only a maximal
contiguous run of `ReferenceInsert` or of `ReferenceDelete` shares preparation.
Insert and delete are different closed classes. Intervening effects end a run;
different runs never share a statement. No sorting, grouping across intervening
effects, combination of multiplicities, skipped effect or expected field occurs.

Each row executes once with its original full id, ordinal, child and linked flag.
Deletes retain all four exact predicates. Every row must acknowledge cardinality
one, with the existing integrity error otherwise. The statement is a local owner
of this connection and run, with no new collection. On success or error it is
dropped before the existing owner CAS and guarded COMMIT/ROLLBACK path. No
statement survives the operation or enters a persistent connection cache.

The original attempt/capsule remains retained through actual COMMIT Unknown.
Known failure still uses the existing checked rollback and owner failure path.
Native observation and credit/drop ordering are unchanged. One statement was
already live in each `Connection::execute`; reuse does not add another concurrent
statement, a persistent arena or a new funded allocation class. Native SQLite
working memory remains explicitly unqualified.

## Prospective source counts and vectors

For128 references, creation keeps four waves of32 and retirement six waves of24
(the last is8). Reference row executions remain128+128=256. Preparation for these
reference runs changes128+128 to4+6=10, removing246 repeated preparations. This
is source arithmetic, not a measured dominant term or a10-second PASS. Smaller
and partial waves retain their original limits and every acknowledgement.

New public native vectors cover maximum128 references and partial33/49 waves,
exact ordinal/value/linked readback, repeated-child count multiplicity and full
known retirement. A foreign supplied job count must refuse without consuming the
actual queued row; a malformed stored reference must fail its checksum before
retirement effects. Existing `construction_drafts` actual SHARED-reader COMMIT
Unknown tests retain the real before/proposed capsule and deny replay; their
barriers precede the first transition and do not purport to time an internal
reference run. No product test hook or invented transaction interleaving is added.

Validation is NOT_RUN at freeze. Root will run the changed source selectors and
the previously failed direct2049 case once; inputs/deadlines are unchanged.

## Genuine owner telemetry addendum, before counter source

Root authorizes four existing-`DraftStats` u64 fields: reference insert/delete
prepare calls and successfully acknowledged reference insert/delete rows. Prepare
calls are checked/incremented immediately before the actual local `prepare`, even
if that call fails. Each row counter has its next value checked before execution
and is assigned only after cardinality one. All issued/successful SQL work remains
counted if the enclosing transaction subsequently rolls back or its COMMIT is
Unknown; these counters are not known published-record counts. They do not select
an algorithm, remove work or reset within a phase. Only a new owner's default
stats reset them. Actual `sizeof(DraftStats)` is already part of the compiled
draft owner working expression, so its additional32 bytes do not create a grant.

The narrow additional source responsibility is C1 `file/edit/draft_port.rs` fields
only, coordinated with the draft owner. Public native vectors assert telemetry
differences across actual hold/retire calls:128 insert rows/four preparations and
128 delete rows/six preparations;33 gives two/two and49 gives two/three. No trace
feature, test hook, private connection API or provider substitution is added.

An additional external negative vector creates a unique `(id,value)` index on
the owned disposable reference table before a public hold. Its repeated-child
reference run acknowledges the first insert and refuses the second on the real
engine constraint. Known rollback must remove that partial run while retaining
one issued preparation and one successful SQL-row telemetry event. This schema
fence exists only in the external test, uses no trigger and does not alter product
schemas or production runtime settings.
