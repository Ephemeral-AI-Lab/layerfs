# Retained failures

alias-initial-build failed E0609: the external test used Resolved.identity, while
the actual public type contains serial/value under the bound root's scope. The
fixture now compares serial, retaining same-scope identity and reference-count
assertions. The rebuilt memory/owning-profile bodies pass. Original output remains.
An initial read command globbed a nonexistent read/ directory; the actual API is
filesystem/read.rs and was inspected before correcting the fixture.
No timeout, failed-operation replay, product fault hook or cache/performance claim.
