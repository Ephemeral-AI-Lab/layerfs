# Observations after a failed deferred write

The original `workspace-short-spool-write-proof` failed at source
`03d4914ee36da6d303ab268e9102519d1755a8e4`:
`attempts/workspace-short-spool-write-proof-s1-verify-65caef92bbd4/` under the
Phase 1 campaign. This was a required native correctness failure.

The workload acknowledged a 4096-byte write and then received the required
`EIO` from fsync. The injected ShortAppend fault was reached exactly once.
Backend logical spool bytes and peak remained 4096 and mutation generation
remained 29, satisfying rollback accounting. However, fresh native verification
reported the wrong type/length for `work/b/fail.dat`, whose expected length was
zero. The proxy had optimistically cached length 4096 when accepting the write
and did not clear its observations when deferred synchronization failed.

The repair in `f5d1c3036eb501d415031fb3b4b625be423c346f` invalidates attributes,
directory observations and read-ahead only on the synchronization error branch.
It preserves the first error, pending mutation queues, counter and fence
semantics; the successful branch is unchanged. One protocol regression passed
for both I/O and NoSpace, including a subsequent backend attribute read of
length zero and a successful fence.

The real short-spool proof then passed in 3.758256917 seconds
(`workspace-short-spool-write-proof-s1-verify-f07f2574a5ec`). The separate
required deferred-NoSpace proof passed in 3.791484875 seconds
(`workspace-deferred-nospace-proof-s1-verify-ba4a3648a712`). Each was independently
validated once with no issues or resource violations, including fresh native
bytes/metadata, exact errno and deferred-error boundary, rollback accounting,
canonical state and final cleanup. Qualification is in
`qualification/deferred-cache-f5d1c303/`.

The old failed outcome and its secondary pre-recovery spool observation remain
unchanged. Its explicit failure Discard and post-Client-drop cleanup passed;
that does not make the failed native verification pass. Historical successful
results are reused only through exact source and unaffected-error-site proofs.
Short-spool and deferred-NoSpace evidence from the older product are excluded
from that reuse. No fixture, metadata check, deadline or resource gate changed.
