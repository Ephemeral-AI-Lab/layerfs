# Retained-history17 v2comparison

> Status: timingFAIL and candidate proofFAIL; joint resultINCOMPLETE. No history
> qualification or durable admission. Original Init milestone remains pinned.

Frozen corrected-harness c58cd91aa, same product0d72ca72b; one sample per arm,
release/locked shared producer and same SQL/VM/VFS observer. No legacyv1clock
pooled; its census failure and retained-data proof repair remain separate.

|Arm|Lifecycle s|Complete performance s /60|Proof s /9.5|Final allocated B|Cold|
|---|---:|---:|---:|---:|---|
|Phase4.5|33.770167166|49.880221916|4.862578167 PASS|52,473,856|PASS|
|Candidate Disposable|39.745866083|57.030186666|9.506742375 TIMEOUT|49,594,368|PASS|

Candidate/reference1.176952009, exceeding1.10 (allowed37.1471838826s). Storage
strict<54,278,964B and cleanup PASS; actual candidate profile readback matches.
Producer full17root vector/canonical51689objects/380559460B matches reference;
candidate closed census completed before native proof timeout, but root/census
is not substitute for full required namespace/custody proof. No timeout increase,
unchanged speed resample or missing-row omission.

Qualified trace totals: reference524829statements/24421375executed VM; candidate
200125/9408660. Lower instruction count alone is not speed proof. Nested producer
stages reference acquisition16.017061374s/construction1.678119667s/filesystem
4.499879874s/save-custody11.386851334s; candidate16.574940833/1.640998751/
6.872864628/14.424784666s. Acquisition includes required source hash validation,
not untimed preload. State-by-state work retained. Difference grows in later
filesystem/save states; these spans overlap inner SQL/canonical work.

Candidate native transactions54074/write724, therefore53350read transactions;
COMMIT0.488418286s nested. Storage locate28106/read-packs8141,16981payload reads/
1,702,370,225B plus1,050,650,368pack-read B against45,558,472inserted pack-body B.
These are distinct work counters, not physical bytes or disjoint totals.
Trace body SELECT31691calls/1.686727s; single-pack descriptor SELECT24486calls/
0.359744s; locator JOIN27197calls/0.525666s; read BEGIN53312calls. Source cache and
buffer bounds unchanged; native VM totals are unqualified because observer resets.
This points to repeated read/snapshot/acquisition work for next attribution.

Post-run reference pin derivation initially failed because required_case_ids was
an in-memory tuple serialized to JSON list. The already-written receipt, census
and proof files agree. Offline derivation from those immutable JSON records
produced pins and manifest without rerunning product/performance/proof or changing
original receipt. Candidate uses those exact hash-bound reference pins and same
identity. Bookkeeping serialization normalization is a separate tool correction;
it does not promote a historical speed/proof failure or change budgets.

Raw complete databases/evidence remain under issue302-history17-v2-{baseline,
candidate}1; compact checks/history17-v2-comparison omit large DBs.53/157 and
Durable remainNOT_RUN. Next: optimize/attribute read work and verifier scaling,
then freeze a concrete changed treatment before new matched gates. Goal ACTIVE.
