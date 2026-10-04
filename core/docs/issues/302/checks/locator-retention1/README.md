# Positive-locator pressure admission

The existing cache holds at most 4,096 positive locator rows over the same
reader/save lifetime. Former pressure admission retained only the current
requested identities, discarding all other rows even when one slot was needed.
A public-reader count diagnostic with 4,097 real saved canonical objects proves
that one miss followed by a prior unrequested positive hit caused two singleton
locator requests. The same fixture after selective eviction records one request
and exactly one capacity eviction, with exact canonical results.

Admission now drops only enough unrequested entries, in existing CID key order.
No extra map/index, recency history or input/result allocation is introduced.
Requested hits remain protected; the existing full-cache guard handles oversized
frontiers. Locator lifetime, negative-cache reset, explicit invalidation, row and
descriptor validation, snapshot scope and output/cache bounds stay unchanged.
An actual capacity-removal u64 adds 8 bytes per Diagnostics value. This can share
previously acquired positive locators across already permitted demands; it does
not merge transactions or promise that scoped pack acquisition counts decrease.

[Checks](checks.json) record the cause and final covering commands. No speed
prediction is made from the fixture. The final Disposable ladder tests the real
history workload in stride 10, then 3, then 1 order. Durable is separate.

SQLite units_read::Input::load still validates each complete mapping once per
acquisition against that same request's full control directory. No duplicated
mapping query within a request was found. Cross-acquisition reuse would require
an explicit new trust-lifetime/API design; no such memo is introduced here.
