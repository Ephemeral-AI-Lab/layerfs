# Is an append issued once per open lane tail per transaction? — YES, with one condition

Squad C answer to the coordinator's update 2, question 1. Sources are file:line in
`source-citations.md`; counts are the ones in `commit-decomposition.txt`.

## 1. The answer

**Yes: every step that places a group places it in exactly one transaction, and the pack
write happens inside that transaction.** The chain is:

1. `cas/placement.rs:111-200` (`seal_group`) takes the arbitration lock (`132`), opens the
   write transaction if it is closed (`133-135`, `cas/lifecycle.rs:129-139`), frames the
   lane's pending group (`122`), places it with
   `select_many(lane, vec![group], ...)` (`137`) — **one group** — writes the pack
   (`161` → `write_pack`, `cas/placement.rs:202-245`), inserts the group's object rows
   (`179`), and then commits inside the same guard (`199` → `cas/lifecycle.rs:160-178`).
   The guard is still alive at `199`: the commit happens **under** the lock, and the lock is
   released at `200`.
2. `pack/placement.rs:71-80` emits **exactly one write per pack that receives a group in
   that call** ("Every pack that receives a group in this call produces exactly one write,
   assembled once, after the last group that landed in it", `73-74`), and
   `cas/placement.rs:202-245` issues exactly one `insert_pack`/`append_pack` statement for it.
3. `sqlite/write.rs:80-89` (`append_pack`) binds the **entire** new pack body
   (`UPDATE object_packs SET data = ?2 ...`), which is what `cas/placement.rs:203-205`
   states as the reason: "Writing a pack moves every body in it (the directory grows)".
4. `cas/lifecycle.rs:163-167` fixes the transaction's lifetime to the step:
   "A write transaction therefore never outlives the step that opened it under the
   arbitration lock, so every step commits before that lock is released. Batching stays
   inside a step; it cannot span steps."

So a pack is rewritten once per **step that places a group into it**, and a step is one
transaction. The two counters in the receipt that look like coincidences are therefore the
same event counted twice:

| | count | source |
| --- | ---: | --- |
| pack writes (groups placed) | 16,802 | `store-geometry.txt`: 16,595 object groups + 207 value groups; Squad B's pack parse agrees exactly |
| of which object-group seals | 16,595 | `cas/placement.rs:199` |
| of which pooled value-group writes | 207 | `cas/pool_lane.rs:355` |
| transactions (`BEGIN IMMEDIATE`) | 17,378 | receipt `pipeline.commits`; Squad B's statement table |
| of which place no group at all | 576 | 207 ordinal reservations + 367 candidate flushes + acquisition + publication |
| ring insertions | 9,444 | `max(content_signatures.stamp)`, one per WholeFile FULL winner (`select.rs:297-303` etc.) |

## 2. The condition on "one append per transaction"

It is **one append per pack touched per step**, not one per step, and not every step
appends:

- A step *can* place several groups: `cas/pool_lane.rs:294-296` passes a **vector** of
  groups to `select_many`. Today that vector is length 1 for every leaf, because a leaf's
  fresh set is bounded by `POOLED_LEAF_ROWS_LIMIT = 100` (`policy.rs:139`,
  `cas/pool_lane.rs:139-141`) and a group chunks at `VALUES_PER_GROUP = 165`
  (`policy.rs:135`). The batched shape is nevertheless **already shipped** here and in
  `cas/lifecycle.rs:228-230` (`finish_inner` seals all five lanes inside one transaction).
- 576 of the 17,377 `begin_write` steps place nothing: the 207 ordinal reservations
  (`cas/pool_lane.rs:122`), the candidate-signature flush steps
  (`cas/lifecycle.rs:201`), and the publication step.
- `flush_candidates` is the one step that can open a transaction and roll it back instead
  of committing (`cas/lifecycle.rs:196-200`); 367 of the ~577 waves committed.

## 3. Why this makes cadence and amplification the same lever — and what caps it

Because the write count per pack equals the number of steps that place into it, **reducing
the number of steps reduces the rewritten volume in exactly the same proportion**, at
constant objects, constant packs and constant bytes. That is the mechanism the
coordinator's synthesis was looking for, and it is why the treatments below can be larger
than the §9 fixed-cost bound (3.9–6.4 %), which modelled a replica that wrote each blob
once and therefore could not see `append_pack` at all.

Two format-enforced ceilings cap how far the step can be coarsened in *this* row:

1. **A group body is at most `GROUP_LIMIT` = 65,536 bytes** for the ordinary, native and
   whole-file lanes (`pack/assemble.rs:21-22` → `frame_group_bounded(records, GROUP_LIMIT)`).
   The ordinary/native seal target is `GROUP_TARGET` = 48 KiB (`policy.rs:93`,
   `cas/selection.rs:65-69`), i.e. already 73 % of the hard ceiling.
2. **The whole-file lane's group body is one unframed record**:
   `pack/assemble.rs:117-120` refuses `records.len() != 1` with
   `Integrity("compact group record count")`, the body is stored `GroupCodec::Raw` with no
   count or offsets, and `pack/layout.rs:114-117` marks that lane as the only one whose
   directory "stores only group starts". Coarsening 9,444 whole-file groups into fewer,
   multi-record groups is therefore a **Store-format change** and needs an owner ruling —
   Squad C does not propose one.

That second ceiling is the reason this row's amplification is where it is. Per lane
(`pack-rewrite-estimate-2.txt`, DERIVED by the equal-increment formula
`L*(k+1)/2`; Squad B's independent parse of the same packs is 4.1 % higher in total):

| lane | packs | writes | final bytes | bytes handed to the pager | amplification |
| --- | ---: | ---: | ---: | ---: | ---: |
| WholeFile | 102 | 9,444 | 24,613,232 | 1,169,771,205 | **47.5×** |
| Native | 1,142 | 7,130 | 276,058,548 | 1,001,675,136 | 3.63× |
| PooledMetadata | 3 | 207 | 726,096 | 25,670,723 | 35.4× |
| Ordinary | 3 | 21 | 625,356 | 2,690,731 | 4.30× |
| **TOTAL** | **1,250** | **16,802** | **302,023,232** | **2,199,807,795** | **7.28×** |

102 packs — 8 % of the packs — carry **53 %** of the rewritten bytes, because each of their
9,444 groups holds one 2.6 KB record and each of those 9,444 placements rewrites the whole
growing pack. Every one of those writes is a transaction boundary (`cas/lifecycle.rs:160-178`).

## 4. What this does NOT say

- It does not say the amplification is *fully* present in `commit_ns`: Squad A's own
  synthesis notes that 2.34 GB at the 1.687 s residual implies ~1385 MB/s, above its own
  677.5 MB/s synthetic floor for this geometry. The volume term is an upper bound on the
  page-flush share, not a measurement of it. `SQLITE_DBSTATUS_CACHE_WRITE` on the
  product's own connection (never read for this row) is what would settle it.
- It does not make the append count a *cause* of the step count or vice versa: both follow
  from the step definition at `cas/lifecycle.rs:163-167`. Removing the commit per group is
  not available (the transaction must not outlive the step), so the only lever is **what a
  step contains**.
- It does not license holding a transaction across steps. That was measured and failed:
  `COMMIT_EVERY` = 8, 64 and 100000 all died with
  `CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`
  (`issue-commit-time-rca-handoff.md:67-70`), and the standing instruction is
  "do not serialise writers, widen the step, or batch across steps" (`:222-223`).

## 5. What a cadence treatment must NOT break (coordinator question 3)

1. **Pack-cache invalidation is load-bearing.** `cas/placement.rs:203-217`: "Writing a pack
   moves every body in it (the directory grows), so every cache that holds those bytes is
   invalidated whenever this save writes" — `208` `pool_reader.release_packs()`, `217`
   `pack_cache.remove(&write.pack_id)`. `cas/owner.rs:360-370` states the same lifetime for
   `read_batch`. A treatment that places several groups per step must keep **exactly one
   invalidation per pack write** and must not retain any pack body across it: a stale entry
   is refused as `Integrity("group ordinal")` (`cas/placement.rs:209-216`).
2. **The pack format must not move.** No change to `VERSION_*`, `DIRECTORY_ENTRY_LEN`,
   `WHOLE_FILE_ENTRY_LEN`, `directory_is_starts_only`, or the whole-file one-record-per-group
   grammar. A directory that does not grow with the body — the obvious way to kill the
   append rewrite — is a Store-format change ("A change that moves the Store is a different
   operation — say so and start again", `issue-commit-time-rca-handoff.md:181-185`; format
   changes are "an owner decision, not an agent's", `:159-163`).
3. **The step invariant itself.** No transaction may outlive the step that opened it under
   the arbitration lock (`cas/lifecycle.rs:163-167`), and a refused second writer must not
   lose its save (`issue-commit-time-rca-handoff.md:67-70, 167-170, 222-223`).
4. **The watermark at every step boundary.** `tests/pack_watermark.rs:42-72` asserts, for
   every accepted object, that `next_pack_id` is ahead of every committed pack id, read
   through an independent connection; `74-116` does it with two interleaved writers.
   `cas/lifecycle.rs:141-150` says deferring the watermark to publication "is exactly the
   change that would let a second writer collide, and `tests/pack_watermark.rs` fails on it".
   Every step that allocates a pack must still advance it inside its own transaction.
5. **Publication scoping.** Early bounded commits must not publish
   (`tests/visibility.rs:109-160`), and a pooled value group committed above the publication
   ceiling must still be refused with `VisibilityCeiling` (`tests/visibility.rs:342-394`).
6. **The two fixed commits.** `tests/cas_reuse.rs:51-54` and
   `tests/persistence_failure.rs:260-263` pin `commits == 2` for a save that writes nothing
   ("slot acquisition and publication"): the acquisition commit (`sqlite/ownership.rs:133`)
   and the publication commit (`cas/lifecycle.rs:256-258`) stay.
7. **The declared bounds stay bounds.** `cas/placement.rs:154-160`
   (`transaction_rows`, the only transaction bound actually read — `capacities.transaction_bytes`
   is declared at `policy.rs:352` and never used), `cas/batch.rs:48-69` (512 objects /
   512 KiB per wave), `pack/layout.rs:204-219` (`append_fits`), `pack/assemble.rs:21-22`
   (`GROUP_LIMIT`). A step that grows must fail closed at these, not raise them.
8. **The writer budget is not a lever.** It is 2 by default and #216 measured that raising it
   buys no throughput (`docs/roadmap/0.1/0.1.7/concurrency-controls.md:30-31`;
   `evidence/issue216-writer-budget-20260921T004651Z/README.md:96-100`). No treatment may
   raise it or add workers (`AGENTS.md` §3.8).
