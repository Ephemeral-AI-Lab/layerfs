# Lessons

> **Status:** Current general guide.

A lesson is one short rule learned the hard way in merged work or a measured
finding. It tells
you what to do and how to recognize the situation, so the next change doesn't
repeat the mistake. A lesson is informative: it never overrides
[AGENTS.md](../../AGENTS.md), a policy or an owner decision, and it is not a
product contract.

## Read the lessons for your task

AGENTS.md routes each task to a category. Read every lesson in that category
before you start.

| Category | Read when you are… |
| --- | --- |
| [store-access](store-access/) | changing or reviewing code that reads or writes the Store: Commit, Init, file and history calls, persistence, storage, store-service |
| [measurement](measurement/) | measuring a call's cost, setting a target, running a benchmark, or judging a result against a cap |
| [testing](testing/) | running test binaries outside CI, especially PostgreSQL-backed ones or a full local pass |

## Start from a symptom

| You see… | Read |
| --- | --- |
| PostgreSQL far slower than SQLite on the same call | [Round trips follow depth](store-access/round-trips-follow-depth.md), then [costly patterns](store-access/costly-patterns.md) |
| Round trips grow with the number of files, directories or versions | [Costly patterns](store-access/costly-patterns.md) |
| A call is fast at 1k items and slow at 10k | [Count round trips above every threshold](measurement/count-round-trips-above-every-threshold.md) |
| A small call (`stat`, `read`) costs tens of round trips | [Costly patterns](store-access/costly-patterns.md) (fixed cost per call) |
| A batching change is ready to merge | [Rules to preserve](store-access/rules-to-preserve.md) |
| A result misses a cap that its baseline met | [State the cache behind a cap](measurement/state-the-cache-behind-a-cap.md) |
| Tests fail with Busy, OpConflict or advisory locks in a local parallel run | [One database, one test binary at a time](testing/one-database-one-test-binary.md) |

## Index

### store-access

| Lesson | Rule |
| --- | --- |
| [Round trips follow depth](store-access/round-trips-follow-depth.md) | A remote Store call's round trips must grow with tree depth, not with how many items changed. |
| [Costly Store access patterns](store-access/costly-patterns.md) | Check a Store path against the patterns that have cost hundreds to tens of thousands of round trips, in Commit, Init and per-call reads. |
| [Rules to preserve when optimizing the Store path](store-access/rules-to-preserve.md) | An optimization may change how many messages are sent and when, never what is stored, what is guaranteed or how much memory it may use. |

### measurement

| Lesson | Rule |
| --- | --- |
| [State the cache behind a cap](measurement/state-the-cache-behind-a-cap.md) | Record the cache state and counting method with every target, and judge only like against like. |
| [Count round trips above every threshold](measurement/count-round-trips-above-every-threshold.md) | Count round trips, not only time, at sizes above every cache and threshold, with latency added inside the database container. |

### testing

| Lesson | Rule |
| --- | --- |
| [One database, one test binary at a time](testing/one-database-one-test-binary.md) | Run PostgreSQL-backed test binaries that share a database one at a time, or give each its own. |

## Writing a lesson

**When.** From merged or closed work, or from a measured finding on an open issue,
and only for a mistake or insight that cost real time and is likely to recur. A
finding that is not fixed yet says so (`Status: Open`, with its issue) wherever it
appears. A one-off bug fix belongs in its pull request, not here.

**Link back.** When an open issue's work matches a lesson, link the lesson from the
issue, so the session that picks the issue up finds it.

**One rule per file.** Name the file after the rule, in kebab case
(`round-trips-follow-depth.md`). If you need "and" to state it, write two lessons.

**Place.** `docs/lesson/<category>/`. Use an existing category if one fits. A new
category needs a row in the table above and in AGENTS.md's Read first table, in
the same commit. A lesson that spans two categories lives in its main one and is
listed under both in the index.

**Shape.** Short by default. A worked lesson may run several screens when its
cases need the detail, but the rule and the spotting signs come first. Use this
order, adding a catalog or checklist section where it helps:

1. Header:

   ```text
   > **Status:** Lesson; distilled from merged work, informative and not a product contract.
   >
   > **Category:** <category> · **Applies when:** <the situation, in one line> · **From:** <issues or PRs>
   ```

2. **Rule:** one sentence a reader can act on.
3. **What happened:** the concrete case, in a few lines.
4. **How to spot it:** signs in code, a review, or a measurement.
5. **What to do:** the pattern that fixes it.
6. **Limits:** where the rule stops applying, and the traps in the fix itself.
7. **Evidence:** links to the issues, pull requests or receipts. Link numbers to
   where they were measured instead of copying many of them. A small before/after
   table is fine if it names its source.

**Keep it current.** When the code a lesson describes changes, update the lesson
or mark it `Status: Archived; retained for historical evidence only.` and move its
index row to an "Archived" list. A lesson never contradicts current AGENTS.md,
policy or architecture pages: if they disagree, they win and the lesson is wrong.
