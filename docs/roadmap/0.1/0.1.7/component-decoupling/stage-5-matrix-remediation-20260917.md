# Stage 5 acceptance matrix, re-derived after the remediation round (2026-09-17)

> **Status:** re-derivation of the independent reviewer's own rows. Row ids,
> criteria and "review" statuses are quoted from
> [`stages-1-5-review-20260917T160000Z.md`](stages-1-5-review-20260917T160000Z.md)
> section 4; nothing here renumbers or re-words a row. The "final" column is the
> status on this round's tree, `f1f6cee36`.

## Stage 5 matrix (denominator 71)

At the review: **50 PASS, 15 FAIL, 5 INCOMPLETE, 1 NOT_RUN**.
On `f1f6cee36`: **70 PASS, 1 NOT_RUN, zero FAIL, zero INCOMPLETE**.

| id | criterion | review | final | what closed it / why it stands |
| --- | --- | --- | --- | --- |
| CI-1 | one shared inode value codec | PASS | PASS | unchanged from the review |
| CI-2 | no duplicate leaf codec | FAIL | PASS | one leaf grammar; `filesystem_codec::both_inode_leaf_routes_are_one_grammar` (`ecec7dbe0`) |
| CI-3 | 73-vs-81 resolved with independent golden bytes | PASS | PASS | unchanged from the review |
| CI-4 | exact root/branch/leaf/symlink framing, role identity | PASS | PASS | unchanged from the review |
| CI-5 | reference/row ordering inside pages | PASS | PASS | unchanged from the review |
| CI-6 | oracle is not self-referential | PASS | PASS | the generator is gated and a read-only comparator added; the seal is reproduced in an isolated copy (`eb42c1347`) |
| PS-1 | accepted profile and actual schema/role constraints checked against on-disk state, including old stores | FAIL | PASS | an old same-version Store is refused at open (`a376acfee`) |
| PS-2 | refusal without silent mutation/migration | PASS | PASS | unchanged from the review |
| PS-3 | old canonical/pool expansion behaviour preserved | PASS | PASS | unchanged from the review |
| PS-4 | non-compact profiles refused | PASS | PASS | unchanged from the review |
| SI-1 | scope plus serial with caller allocation obligations | PASS | PASS | unchanged from the review |
| SI-2 | exhaustion and range enforced | FAIL | PASS | the inode-serial range is enforced where serials enter (`a376acfee`) |
| SI-3 | no fake `ObjectId` from a serial | PASS | PASS | unchanged from the review |
| SI-4 | unchecked caller count/retention not accepted as proof | PASS | PASS | the dead `additions`/`removals`/`declared_new` and `is_touched` surfaces are gone (`ecec7dbe0`) |
| SC-1 | optional-base build | PASS | PASS | unchanged from the review |
| SC-2 | exact page occupancy and partition | PASS | PASS | unchanged from the review |
| SC-3 | tail rebalance | PASS | PASS | unchanged from the review |
| SC-4 | height collapse | PASS | PASS | unchanged from the review |
| SC-5 | changed-path work, untouched subtree IDs | PASS | PASS | unchanged from the review |
| SC-6 | grouped reads, final-only child-first emission, no provisional seed | PASS | PASS | unchanged from the review |
| WT-1 | effective final topology | PASS | PASS | unchanged from the review |
| WT-2 | cycles created by several moves | FAIL | PASS | effective cycles refused; reviewer client `build.disconnected_cycle=REFUSED`, `T1=REFUSED` (`ef6bab19d / 8964db93e`) |
| WT-3 | duplicate names/identities | PASS | PASS | unchanged from the review |
| WT-4 | single parent for directories/symlinks | FAIL | PASS | a second parent for a Directory/Symlink is refused; reviewer client `T2`/`T3` REFUSED, `T4` control (`b262ad38c`) |
| WT-5 | root operations | PASS | PASS | unchanged from the review |
| WT-6 | disconnected dirty inputs, membership before final emission | FAIL | PASS | no pages for a directory the same batch deletes (`ef6bab19d`) |
| RA-1 | additions before removals, across directories and waves | PASS | PASS | unchanged from the review |
| RA-2 | new counts derived once | PASS | PASS | unchanged from the review |
| RA-3 | aliases outside the changed set included | PASS | PASS | unchanged from the review |
| RA-4 | newest pending value overrides base | PASS | PASS | unchanged from the review |
| RA-5 | moved-out child and externally linked file survive subtree removal | PASS | PASS | unchanged from the review |
| RA-6 | zero-count release bounded | PASS | PASS | unchanged from the review |
| RA-7 | old roots remain readable | PASS | PASS | unchanged from the review |
| OR-1 | compact typed records, malformed rejected | PASS | PASS | unchanged from the review |
| OR-2 | actual threshold crossings, tier carry/merge | PASS | PASS | unchanged from the review |
| OR-3 | tombstone precedence | PASS | PASS | unchanged from the review |
| OR-4 | overflow/truncation explicit | PASS | PASS | unchanged from the review |
| OR-5 | simultaneous old/new runs, backing quota | FAIL | PASS | the declared ceiling covers inputs plus output (`cfc6c4ae3`) |
| OR-6 | cleanup on success, error and drop | PASS | PASS | the accessors stay honest after a failed removal (`781a73661`) |
| OR-7 | no error-triggered alternate route | PASS | PASS | unchanged from the review |
| OR-8 | returned ordering counters describe the work | FAIL | PASS | charged reads and a per-tier cursor; reviewer client `rows_read` now moves with the reads (`8701eae12`) |
| RD-1 | resolve/stat/list/readlink public and usable | PASS | PASS | path-level attribute reads have callers (`781a73661`) |
| RD-2 | duplicate-demand cardinality and order | PASS | PASS | unchanged from the review |
| RD-3 | shared ancestor reads | PASS | PASS | unchanged from the review |
| RD-4 | bounded count and bytes, progressing pagination | FAIL | PASS | a byte bound that cannot fit one row refuses; reviewer client `list.max_bytes=1..15 Err` (`ef6bab19d`) |
| RD-5 | wrong summary/kind/scope/truncation rejected | PASS | PASS | unchanged from the review |
| RD-6 | no hidden full-tree collection | PASS | PASS | `attribute_keys` is bounded by `MAXIMUM_ATTRIBUTE_KEYS` and charged (`781a73661`) |
| AT-1 | portable mode/mtime grammar | PASS | PASS | unchanged from the review |
| AT-2 | generic key bounds and order | PASS | PASS | unchanged from the review |
| AT-3 | opaque untouched key AND value root preserved | PASS | PASS | unchanged from the review |
| AT-4 | bounded extent-only values | FAIL | PASS | the 1 MiB value bound covers the write path (`a376acfee`) |
| AT-5 | exact page sizing and partitions | PASS | PASS | unchanged from the review |
| AT-6 | no Apple-specific dispatch | PASS | PASS | unchanged from the review |
| AT-7 | scoped attribute parity: supplied value roots **and** independently constructed complete trees | INCOMPLETE | PASS | the patch route equals a from-scratch build of the same final set (`f1f6cee36`) |
| C2-1 | new roles through real save/finish/reopen | PASS | PASS | unchanged from the review |
| C2-2 | correct direct object references; the required objects of a reported root are persisted | FAIL | PASS | the persisted-reference contract is stated and the accepted input is pinned; the reviewer client still shows the dangling arm unreadable, which is the documented outcome (`5b138ce64`) |
| C2-3 | pooled inode interoperability, locators, dependency visibility, batch reuse | PASS | PASS | unchanged from the review |
| C2-4 | no flush per inode or directory | PASS | PASS | unchanged from the review |
| TR-1 | real independent C1/C2/integrated bodies; the case actually selects the operation | FAIL | PASS | the harness selects and composes what it names: six distinct pipeline roots, `SaveHandoff` plus a reopened Store (`b3df5461c`) |
| TR-2 | bounded reports and honest clipping | PASS | PASS | a clipped report fails the run (`b3df5461c`) |
| TR-3 | timing on/off runs the same body with equal results | INCOMPLETE | PASS | the integrated C1+C2 body has an on/off equality case (`eb42c1347`) |
| TR-4 | validation/ordering/provider/output waits attributed | INCOMPLETE | PASS | validation work is charged and provider calls carry a caller scope (`cfc6c4ae3 / ab17a6958`) |
| TR-5 | simultaneous memory and backing costs covered | FAIL | PASS | peak coexistence is sampled (`cfc6c4ae3`) |
| VF-1 | every named Stage 5 target exists | PASS | PASS | coverage is stated as tests that exist (53 test-bearing targets of 59, 406 tests) (`b3df5461c`) |
| VF-2 | source and fixture seals recorded | PASS | PASS | the seal is read-only under an ordinary run and reproduces in an isolated copy (`eb42c1347`) |
| VF-3 | meaningful assertions | FAIL | PASS | the byte-bound case is discriminating (`ef6bab19d`) |
| VF-4 | limits evidence | INCOMPLETE | PASS | the limits table is re-derived at this round (`a376acfee / 4f2e4ca9a / 781a73661`) |
| VF-5 | required comparison against the pinned reference | INCOMPLETE | PASS | an identity-matched component comparison at `eb42c1347` (`a512604f8`) |
| VF-6 | complete-operation comparison | NOT_RUN | NOT_RUN | declared `NOT_RUN` before collection under the frozen addendum with a source-backed reason: no equivalent public reference surface exists for a complete-operation comparison. It is **not** waived and is **not** promoted; the complete-operation comparison belongs to Stage 6 (#171). |
| VF-7 | LOC census and plan accounting | FAIL | PASS | the LOC tables are re-derived at this round (`781a73661`) |
| VF-8 | oracle reproduced independently | PASS | PASS | unchanged from the review |

## Cumulative Stages 0-4, telemetry and reachable settings (denominator 31)

At the review: **27 PASS, 1 FAIL, 1 INCOMPLETE, 2 owner-WAIVED**.
On `f1f6cee36`: **29 PASS, 2 owner-WAIVED**, zero FAIL and zero INCOMPLETE.

| id | criterion | review | final | what closed it / why it stands |
| --- | --- | --- | --- | --- |
| S01-1 | canonical framing/identity and authentication boundary | PASS | PASS | unchanged from the review |
| S01-2 | exact-length stable inputs, empty/small/large construction | PASS | PASS | unchanged from the review |
| S01-3 | frozen CDC and mapping partitions | INCOMPLETE | PASS | the mapping profile hashes the named partition constants (`ecec7dbe0`) |
| S01-4 | complete input validation | PASS | PASS | unchanged from the review |
| S01-5 | direct finalized output and backpressure | PASS | PASS | unchanged from the review |
| S01-6 | C1-only operation without SQLite/Workspace | PASS | PASS | unchanged from the review |
| S2-1 | exact reuse | PASS | PASS | unchanged from the review |
| S2-2 | admission and dependencies | PASS | PASS | unchanged from the review |
| S2-3 | ordinary FULL and pack locator reads | PASS | PASS | unchanged from the review |
| S2-4 | bounded batch ownership | PASS | PASS | unchanged from the review |
| S2-5 | SQLite transactions and profile | PASS | PASS | unchanged from the review |
| S2-6 | private early output and publication watermark | PASS | PASS | unchanged from the review |
| S2-7 | pending-owner reads, one acknowledged finish, reopen | PASS | PASS | dead surfaces gone and the always-true `acknowledged` field removed (`ecec7dbe0 / b3df5461c`) |
| S2-8 | one definite-failure cleanup that cannot delete successful versions | PASS | PASS | unchanged from the review |
| S2-9 | C2-only operation without construction | PASS | PASS | unchanged from the review |
| S3-1 | configurable cutoff and independent payload/metadata chain limits | PASS | PASS | unchanged from the review |
| S3-2 | exact reuse before one allowed delta trial | PASS | PASS | unchanged from the review |
| S3-3 | explicit candidate ordering, framed cost, compression/lane selection | PASS | PASS | unchanged from the review |
| S3-4 | iterative authenticated reconstruction | PASS | PASS | unchanged from the review |
| S3-5 | real value pooling, ordinals, digests, window boundaries, retained cache bounds | PASS | PASS | unchanged from the review |
| S3-6 | performance/resource claims | owner-WAIVED | owner-WAIVED | unchanged from the review |
| S4-1 | small and large edits, current-result coordinates | PASS | PASS | unchanged from the review |
| S4-2 | stored-subtree split/concat/coalesce and the exact reference partition | PASS | PASS | the base partition is asserted against the sealed fixture (`781a73661`) |
| S4-3 | local CDC convergence and bounded decoded frontier | PASS | PASS | unchanged from the review |
| S4-4 | no-op, both cutoff transitions, finality, old-root immutability | PASS | PASS | unchanged from the review |
| S4-5 | stage performance claims | owner-WAIVED | owner-WAIVED | unchanged from the review |
| TEL-1 | one real operation hierarchy | PASS | PASS | unchanged from the review |
| TEL-2 | disabled-path behaviour | PASS | PASS | unchanged from the review |
| TEL-3 | bounded retention/clipping and honest incompleteness | FAIL | PASS | one region per wave, not one node per object (`b3df5461c`) |
| TEL-4 | existing timer reused, not a second monitor | PASS | PASS | provider calls carry a caller scope (`ab17a6958`) |
| X-1 | no retry, error-driven fallback, busy retry, resend, fsync family, WAL | PASS | PASS | unchanged from the review |

## How to read the "unchanged from the review" rows

A row the review recorded `PASS` and this round did not touch is **not** re-argued
here; it stands on the review's own evidence plus this round's full run
(`cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked`:
**406 passed / 0 failed / 0 ignored**, 54 of 60 discovered targets carrying at least
one test and 6 carrying none),
the product-boundary guard and `clippy -D warnings`. Where this round's changes
touched a row's subject, the row says so in its own line. No row was promoted
without a change, and the one `NOT_RUN` row keeps its pre-declared reason rather
than being waived.

## What this matrix is not

It is not a measurement: `TR-*` rows describe which timing and resource bodies
exist and what the harness proves about them, not a qualified performance claim.
The complete-operation comparison, cold-cache rows, pack footprint and
whole-process memory remain Stage 6 (#171) and are named as unavailable rather
than read as zero.
