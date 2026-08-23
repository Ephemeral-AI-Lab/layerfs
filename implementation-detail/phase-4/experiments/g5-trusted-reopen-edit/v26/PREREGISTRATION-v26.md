# G5-1 v26 prospective authority rerun

V26 is a fresh method-only authority rerun of the unchanged, hash-verified v24 product release and sealed v10 inputs. No v24 row or decision is reused.

The complete schedule remains 200 gate arms and 56 fixed CompleteRoundTrip checkpoints. The screen remains strictly below 20 seconds and the gate remains at most 150 seconds total. All correctness, identity, exact error, transaction/one-COMMIT, durability, reconciliation, Q, per-child RSS, storage, cleanup, and analyzer-agreement gates remain unchanged.

Database preconditioning is declared before measurement: each G5 row reads the complete candidate database sequentially with a fixed 64-KiB buffer after the attempt-local clone and before the product timer. Its bytes and wall are recorded and charged inside complete campaign wall. It is not LayerFS SQL/authentication work. G4 is not reread by the wrapper because its frozen executable already hashes the database before its product timer. Results are `CacheWarmPreconditionedNotColdReopen`; no cold-reopen claim is made.

Every G4 and G5 benchmark arm runs in a fresh one-shot child with `MallocNanoZone=0`. This makes frozen-G4 versus G5-Verified process shape matched. G5-Verified versus G5-Trusted is also one-shot versus one-shot. Process startup remains outside the product decision timer but inside complete campaign wall.

The source freeze must bind the exact executed `runner.py`, both analyzers, this preregistration, limitations/review/sample documents, focused/readiness evidence, the v24 anti-cheat disposition and v26 supersession, schedules, expectations, reused operands, and the 150-second forecast equation before any benchmark lock or row.

Any repair after freeze or any measured failure is preserved; thresholds, populations, and schedule are never reduced after observation.
