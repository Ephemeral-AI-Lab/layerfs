# Retained failures and build ordering

consumer-tests-build failed E0433: external runtime fixture omitted std::sync::Arc.
consumer-port-functional was incorrectly launched after that failed no-run result;
it recompiled and failed with the same error, executing no test bodies. This was
an invocation-order error, not a runtime result. Both original logs are retained.
The fixture now uses its qualified std type; a passing rebuilt no-run is required
before any test execution. No wall timeout or operation replay occurred.
