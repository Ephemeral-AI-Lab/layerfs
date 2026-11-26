# Round trips follow depth

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** store-access · **Applies when:** code awaits a Store read or write once per changed item · **From:** [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810), [#815](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815), [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816)

## Rule

A remote Store call's round trips must grow with tree depth, in bounded windows,
not with how many files, directories or pages the change touches.

## What happened

The Commit read path was written when the Store was SQLite in the same process,
where a lookup is a function call. It located and read the old tree one item at a
time: each changed directory's page, its inode values, each changed file's base,
then the bindings walk. Each phase (construct, finish, logical count, bindings)
read the same pages again, because a large change overflowed the caches.

Against PostgreSQL every lookup became a network round trip. Adding 10,000 files
in 3,942 directories (C4s4) took 26,740 round trips and 17.8 s on loopback, an
estimated 270 s at 10 ms of latency.

The reads did not depend on each other. The captured change list already names
every changed directory and file; only tree depth, about 12 levels, orders them.
Asking per level in bounded windows, sharing pages across phases, and sending the
Save's writes in one fenced message took C4s4 to 203 round trips and B1 (npm
install, 10,002 files) from 5,309 to 270.

## How to spot it

- A loop that awaits one Store read per item: directory, file, inode page, pack.
- A call that returns a location, followed by a second call to read it.
- The same page or value read again in a later phase of the same operation.
- One write round trip per batch where the batches could travel together.
- PostgreSQL round trips that grow with the number of changed files. Count round
  trips, not only time: on loopback each costs under a millisecond and hides the
  shape until the Store is remote.

## What to do

- Collect the IDs the operation already knows, then ask for all of them in one
  statement (`= ANY($1)`) or one pipelined message per tree level.
- Cut each batch into bounded windows by rows or bytes, never sized to the whole
  change.
- Read each page once per operation and hand it to later phases, within a bounded
  cache.
- Send dependent writes as one fenced message instead of one trip per batch.
- Leave the SQLite path as it is; take the batched path only when the provider
  reports that a lookup costs a round trip.

## Limits

- Depth is the floor. Without a format change, level d+1's IDs are known only
  after level d is read.
- Joining locate and read into one statement saved nothing on its own
  (#815 Stage 1). The gain comes from fewer sequential waits, not fewer statements.
- A large statement leaves its buffer on the connection: tokio-postgres keeps its
  request and write buffers at their largest size. Split large inserts into
  bounded statements inside one message
  ([#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822)).
- Reads by a cold Workspace still cost one trip each. Batching them per window
  (plan-then-fetch) was proposed in #815 and set aside as a small gain
  ([#824](https://github.com/Ephemeral-AI-Lab/layerfs/issues/824)).

## Evidence

PostgreSQL round trips per Commit:

| Cell | Before #810 | After #810 | After #816 |
| --- | ---: | ---: | ---: |
| B1 npm install | 5,309 | 729 | 270 |
| C4s4 add 10k files | 26,740 | 611 | 203 |
| B2 delete `node_modules` | 1,723 | 137 | 114 |
| C4 1,000 deep edits | 4,851 | — | 91 |

"Before" is from the [#810 issue](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810),
measured with an earlier harness; compare it by order of magnitude. The other
columns are cold local PostgreSQL runs at `7e6caa86b2` and `056e920166`, from the
closing comments of [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418)
and [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7203083110).
