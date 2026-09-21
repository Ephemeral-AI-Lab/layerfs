# raw/07 — SaveProfile bucket definitions

Source of truth: `core/crates/layerfs-storage/src/cas/owner.rs`.

> Every measured value in this file is **NOT_MEASURED**.
> The v0.1.7 receipt
> `core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-final-20260921T031259Z/pipeline-namespace-10000/receipt.json`
> does **not** publish `SaveProfile`. A key scan of that receipt finds **no** key
> matching `save`, `accept` or `remainder`; the only key matching `profile` is the
> empty top-level `"profile": ""`. No bucket has been measured, and this campaign ran
> no build and no benchmark. These are definitions only.

## The struct

`owner.rs:30-42` (doc comment, verbatim):

> `/// Nanosecond cost split of one save operation's accept path.`
> `///`
> `/// Seven disjoint buckets, each charged at the call site that does that kind of`
> `/// work. The profile is an **aggregate over the whole operation**, never a span`
> `/// per object: a stride1 row accepts about 10^5 objects and the recorder refuses a`
> `/// node per object, so the cost accumulates into seven \`u64\` fields and is`
> `/// published once per state. Charging costs one \`Instant::now()\` pair per site.`
> `///`
> `/// Every charged interval is disjoint from every other: no bucket contains`
> `/// another, and a bucket's sites are the only places that kind of work happens.`
> `/// The instrument measures the accept path, so the caller's own per-object work`
> `/// (presence validation, group assembly outside the codec, \`raw_payload\`) is`
> `/// deliberately outside all seven and is reported as the remainder.`

This docstring is **contradicted in one place** by the code: `owner.rs:34-35` says the
profile is "never a span per object … published once per state", while
`owner.rs:135-145` (`accumulate`) adds one profile into another. The two are
reconcilable — accumulate merges *states*, it does not create a per-object node — but
the wording invites the misreading. Noted, not a defect.

## The seven buckets

| # | Field | Definition (verbatim from the source doc) | Declared at | Value |
|---|---|---|---|---|
| — | `resolve` | "The disjoint parts of resolution." `resolve_ns()` sums the five parts (`owner.rs:130-133`). | `owner.rs:45-46` | **NOT_MEASURED** |
| 1a | `resolve.eligible_ns` | "Candidate eligibility: the `depth_of` edge walk, which reads each edge through `ChainBases` and is charged to no other counter in the product." | `owner.rs:90-92` | **NOT_MEASURED** |
| 1b | `resolve.acquire_ns` | "Base acquisition: the chain rebuild that produces the offered bytes a prefix frame is taken against." | `owner.rs:93-95` | **NOT_MEASURED** |
| 1c | `resolve.cost_ns` | "The post-trial cost walk that records the admitted object's own depth." | `owner.rs:96-97` | **NOT_MEASURED** |
| 1d | `resolve.reuse_ns` | "Exact-reuse verification: stored-object reconstruction, the identity re-hash that authenticates it, and the byte comparison." | `owner.rs:98-100` | **NOT_MEASURED** |
| 1e | `resolve.pooled_ns` | "The pooled lane's value lookup, base acquisition and index synchronization." | `owner.rs:101-102` | **NOT_MEASURED** |
| 2 | `full_ns` | "FULL representation encode: the ordinary lane's tree-role and payload frames, and the pooled lane's leaf body." | `owner.rs:47-49` | **NOT_MEASURED** |
| 3 | `delta_ns` | "Delta representation encode: ordinary prefix frames and pooled COPY/INSERT program construction." | `owner.rs:50-52` | **NOT_MEASURED** |
| 4 | `group_ns` | "Group codec: ordinary group framing and pooled value-group compression." | `owner.rs:53-54` | **NOT_MEASURED** |
| 5 | `place_ns` | "Pack placement: lane selection and the write it produces." | `owner.rs:55-56` | **NOT_MEASURED** |
| 6 | `sql_ns` | "SQL: object rows, pack bodies, value-group rows, the content-signature flush and the publication watermark." | `owner.rs:57-59` | **NOT_MEASURED** |
| 7 | `commit_ns` | "Transaction cadence: `COMMIT`, `ROLLBACK`, and the `BEGIN IMMEDIATE` that restarts a bounded transaction." | `owner.rs:60-62` | **NOT_MEASURED** |

`total_ns()` is the sum of exactly these seven duration fields
(`owner.rs:147-163`), and its doc states the remainder explicitly
(`owner.rs:147-150`, verbatim):

> `/// Sum of the seven buckets.`
> `///`
> `/// This is charged work, not the accept path: the difference between it and`
> `/// \`storage.accept_loop\` is the remainder the instrument does not name.`

## Deliberately OUTSIDE all seven

From `owner.rs:40-42`, verbatim — *"the caller's own per-object work (presence
validation, group assembly outside the codec, `raw_payload`) is deliberately outside
all seven and is reported as the remainder"*.

The remainder is defined operationally, not by a constant:

```
remainder = storage.accept_loop span  -  SaveProfile::total_ns()
```

`pipeline.rs:774-789` states the span has to be measured rather than inferred, and
records it as `accept_span_ns` from a harness `Instant` pair:

> `// The denominator \`SaveProfile\` is reported against. … it runs from the moment the`
> `// operation is owned to the moment the seal returns: every one of the seven`
> `// buckets is charged inside that interval and nothing outside it is. Read`
> `// beside \`pipeline.profile_total_ns\`; the difference is the remainder the`
> `// instrument does not name.`

Remainder value: **NOT_MEASURED**. `accept_span_ns` exists in the row's scope
(`pipeline.rs:784, 789, 845`) but is not published in the receipt either.

## Not a bucket, explicitly

`reuse_repeat` (`owner.rs:63-76`) — verbatim: *"A **count**, not a duration, and
deliberately not an eighth time bucket: `total_ns` still sums the seven durations, so
the split published beside this figure is unchanged by it."* It is zero unless
`LAYERFS_STORAGE_REUSE_PROBE` is set (`owner.rs:73-75`, `owner.rs:185-189`).
Value: **NOT_MEASURED** (and, absent an enabled probe, definitionally `0`).

## Why no value exists in this campaign

`SaveProfile` is an aggregate published "once per state" (`owner.rs:34-36`). The
v0.1.7 receipt is `status: "INCOMPLETE"` and its `resources.space.incomplete` array
contains `"no reading was taken before the chain"`. Recovering any of the seven bucket
values requires running the row and reading its timing output — a benchmark run, which
this campaign was explicitly forbidden to perform.

## What a future measurement must record

For any of these values to become evidence it must be published **beside**
`accept_span_ns`, because a bucket with no denominator cannot be read: the seven sum
to charged work on the accept path, and every claim of the form "bucket X dominates"
is a claim about `X / accept_span_ns`, not about `X` alone (`owner.rs:147-150`).
