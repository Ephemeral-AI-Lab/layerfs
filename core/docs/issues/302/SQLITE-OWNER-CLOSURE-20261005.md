# Owner-directed SQLite closure qualification, 2026-10-05

The owner explicitly requested simpler proof/validation, doubled time caps, a fresh Durable stride 10 → 3 → 1 round, all namespace Init tiers for Durable and Disposable, statistics, and issue closure. This prospectively supersedes the earlier deadline policy for this campaign only. Historical failures and qualified Disposable receipts remain unchanged.

History uses creation-only GroupRowsIndexed schema3. New Durable `phase7-sqlite-history-stride{10,3,1}-group-rows-indexed-v2` caps respectively are 120/340/600 seconds complete performance command and 24/24/60 seconds separate proof (exactly twice previous 60/170/300 and 12/12/30). Storage ceilings remain 54,278,964 / 70,427,034 / 92,342,273 bytes; original targets and misses are reported. Relative time gate stays 10*candidate <= 11*reference.

Init uses the public default Monolithic layout, four tiers 100/1000/10000/100000 in each profile: Durable `phase7-sqlite-init-{tier}-v3`, Disposable `phase7-sqlite-disposable-init-{tier}-v2`. Both arms have 30-second complete performance and 19-second separate verification caps, twice 15/9.5. Internal operation deadlines match the new cap. Allocation remains candidate final main/WAL/SHM <= matched reference total. Init retains its legitimate four-worker exception; history has one construction worker.

One release/locked sample per new frozen case and arm, fresh append-only outputs, matched original pinned reference source, prepared fixture/corpus reuse, full input invalidation plus residency checks, real measured work and cleanup. Copy cost belongs inside proof. Closed WAL proof permits only exclusive regular main plus exact empty WAL/32768-byte SHM, immutable census, bounded independent byte copy, zero copied-input residency, unchanged original identities/hashes. Journals, pending frames, aliases, changes and excess copies fail closed. Existing all-state structure, independent roots, canonical counts, bounded selected content and authentication limits remain.

The proof becomes one active path using the reviewed correction; no production/dependency/durability change. Validate covering helper tests and changed release example once at frozen source. Record every result, identity, reproduction command and gap. SDK/server/daemon/FUSE are N/A for this direct component lane. PostgreSQL/MinIO remains paused; SQLite qualification and owner-requested closure do not claim those original service milestones or release admission.

Close #302 after this requested round and a concrete report; failures cannot be renamed PASS. Checkpoint commits record exact first-parent/staged production LOC and update the issue.

## Checkpoint: Durable stride10 and stride3

Source702f7a38f, exact matched release/locked arms. Stride10 PASS: product32630223375 /32356111208ns (−0.8400560543%); complete46596506375 /44885041583ns; proof3018135167 /3332632167ns; candidate50724864B <=54278964B. Stride3 PASS: product69063050500 /69114259292ns (+0.0741478861%, within unchanged10% margin); complete82421191000 /81654476917ns; proof6765155625 /6761100583ns; candidate64278528B <=70427034B. Both cold, custody, independent roots, cleanup and original preservation PASS. Original storage targets remain missed. [Compact evidence](checks/owner-closure1/) pins raw receipts. Stride1 and both-profile Init remain NOT_RUN at this checkpoint.

Validation:37 covering harness tests PASS; changed Init example locked Clippy/fmt PASS; boundary456 PASS. Historical failed helper checks retained with corrections; product523unique tests and23 unchanged guard self-tests retained at identical product source. No aggregate preflight/CI. Production LOC remains reference65417/core74887/combined140304.
