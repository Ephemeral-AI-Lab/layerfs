# Observer class-index count/cause checkpoint

> **Status:** Research; informative and not a product contract.

Retained current Disposable157class keys have790classes/20251289known lookup calls.
Exact count replay estimates1775113020linear row comparisons versus20258390indexed
slot probes,98.858754920%fewer comparisons. This is count algebra, not a new
performance sample or an exclusive measured CPU effect. It preserves first-class
miss behavior and excludes unavailable/omitted events; the retained receipt has
zero omissions. Hash key is the original full SQL fingerprint plus phase/owner.

The fixed open-address index has8192uint32 slots/32768B at4096classes. It navigates
existing diagnostic counter rows only, stores no object/result bytes, and adds no
product cache or native cursor lifetime. Full SQL hashing, collision checks,
mutex protection, row order, status reset and saturation/omission behavior remain.
Lookup-call/probe observations are emitted as an additive class_lookup field;
existing sqlite-work-v1mandatory fields and class IDs remain unchanged.

Seven calibration tests pass. The new differential native probe compares original
linear and indexed observers on deliberately colliding bucket keys, reused prepared
statements, bootstrap/operation/cleanup phases and three owner categories. Exact
non-clock class arrays/VM/call/status counts and order agree at512classes and a
saturated16-class cap; omission counts match and actual SQLite values agree.
A self-contained first-party test fixture is the exact original893fac296observer
snapshot; it is not a dependency fork/vendor or product alternative. After moving
that fixture out of a Git-history requirement, the affected differential check
passes once again at its final test identity; unchanged tests are not repeated.
The existing six calibrations preserve delegated BLOB/acquisition/failure/byte
checks. Changed history registry/proof tests also pass with six indexed cases,
three strides per profile, preserving60/170/300s and12/12/30s bounds.

No Rust product source, SQL schema or product/example compilation input changed.
Core519/99targets/Clippy/fmt/boundary455/23selftests atedcfae4ac are reused at the
identical product source scope; no unrelated family sweep or aggregate gate ran.
New actual stride10paired qualification is pending; stride3/1/Durable remain
NOT_RUN at the indexed-observer artifact. A lower observer cost is instrumentation
improvement, never silently labeled a new product algorithm speedup.
