The catalogue-only candidate passes its first matched diagnostic. Operation23,491,957,417→22,180,444,124ns: delta−1,311,513,293ns, **5.582818279974068% reduction**. Invocation CPU user+system33,510,728,000→32,304,434,000ns. Roots/save decisions/complete Store hashes match; both sampled verifiers completed.

**Complete command regressed:**39,267,932,708→39,909,975,875ns (+642,043,167ns). Outside-operation time increased; no end-to-end speedup is claimed. This is a modest one-sample operation result and cannot establish repeatability.

Actual preparation reduction is independently proven in the public-helper fixture (five compilations→one for common five queries, with current rows/parameters/errors preserved). Production implementation is net−1 LOC, no new cache or telemetry. Stride3 confirmation is running before the retain/reject decision. No further optimization is being added. Cache admission remains INELIGIBLE and history pins INCOMPLETE.
