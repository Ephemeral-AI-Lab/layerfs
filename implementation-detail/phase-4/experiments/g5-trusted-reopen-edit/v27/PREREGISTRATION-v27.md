# G5-1 v27 prospective matched-control RSS repair

V26 is terminal `MEASURED_REVISE`: its full gate completed all 200 arms in
97.325344041 seconds and reaped 200/200 children with terminal active children
zero, but six CompleteRoundTrip/high-work child RSS observations exceeded the
unchanged 20,971,520-byte per-child cap. V27 reuses no measured row.

The sole prospective repair is a matched SQLite cache configuration change from
2,000 to 1,280 pages in both the v27 G5 child and a patched G4 control built
from frozen G4 source SHA
`01886da1d413ce73bbeba38f1b5cbc45a939e9d50e69fa7273c1af33f65554cb`.
The frozen G4 binary/results remain baseline authority; the main v27 G4-vs-G5V
comparison uses the matched patched G4 control only after focused output, work,
timer, transaction/COMMIT, lifecycle, and Q parity with frozen G4 passes.
The cache configuration applies during Store operations. Existing 64 KiB
database preconditioning remains outside the product timer and inside protected
complete wall. Every newly explored postcommit or phase memory-release call was
removed and is preserved only as `RELEASE-EXPERIMENTS-NO-GO-v27.json`; G5's
pre-existing post-request cleanup is unchanged.

The complete schedule remains 200 gate arms and 56 fixed CompleteRoundTrip checkpoints. The screen remains strictly below 20 seconds and the gate remains at most 150 seconds total. All correctness, identity, exact error, transaction/one-COMMIT, durability, reconciliation, Q, per-child RSS, storage, cleanup, and analyzer-agreement gates remain unchanged. Preparation, freeze, dry run, screen, and gate remain unauthorized until both product hashes, patched-G4 parity, focused evidence, and independent readiness are settled.

Database preconditioning is declared before measurement: each G5 row reads the complete candidate database sequentially with a fixed 64-KiB buffer after the attempt-local clone and before the product timer. Its bytes and wall are recorded and charged inside complete campaign wall. It is not LayerFS SQL/authentication work. G4 is not reread by the wrapper because its frozen executable already hashes the database before its product timer. Results are `CacheWarmPreconditionedNotColdReopen`; no cold-reopen claim is made.

Every G4 and G5 benchmark arm runs in a fresh one-shot child with `MallocNanoZone=0`. This makes frozen-G4 versus G5-Verified process shape matched. G5-Verified versus G5-Trusted is also one-shot versus one-shot. Process startup remains outside the product decision timer but inside complete campaign wall.

The source freeze must bind the exact executed `runner.py`, both analyzers, this preregistration, limitations/review/sample documents, focused/readiness evidence, the v24 anti-cheat disposition and v27 supersession, schedules, expectations, reused operands, and the 150-second forecast equation before any benchmark lock or row.

Any repair after freeze or any measured failure is preserved; thresholds, populations, and schedule are never reduced after observation.
