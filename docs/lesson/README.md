# Lessons

> **Status:** Current general guide.

A lesson is one short rule learned the hard way in merged or closed work. It tells
you what to do and how to recognize the situation, so the next change doesn't
repeat the mistake. A lesson is informative: it never overrides
[AGENTS.md](../../AGENTS.md), a policy or an owner decision, and it is not a
product contract.

## Read the lessons for your task

AGENTS.md routes each task to a category. Read every lesson in that category
before you start.

| Category | Read when you are… |
| --- | --- |
| [store-access](store-access/) | changing code that reads or writes the Store: persistence, storage, store-service, Commit construction |
| [measurement](measurement/) | setting a target, running a benchmark, or judging a result against a cap |

## Index

### store-access

| Lesson | Rule |
| --- | --- |
| [Round trips follow depth](store-access/round-trips-follow-depth.md) | A remote Store call's round trips must grow with tree depth, not with how many items changed. |

### measurement

| Lesson | Rule |
| --- | --- |
| [State the cache behind a cap](measurement/state-the-cache-behind-a-cap.md) | Record the cache state and counting method with every target, and judge only like against like. |

## Writing a lesson

**When.** Only from merged or closed work, and only for a mistake or insight that
cost real time and is likely to recur. A one-off bug fix belongs in its pull
request, not here.

**One rule per file.** Name the file after the rule, in kebab case
(`round-trips-follow-depth.md`). If you need "and" to state it, write two lessons.

**Place.** `docs/lesson/<category>/`. Use an existing category if one fits. A new
category needs a row in the table above and in AGENTS.md's Read first table, in
the same commit. A lesson that spans two categories lives in its main one and is
listed under both in the index.

**Shape.** At most about one page, in this order:

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
