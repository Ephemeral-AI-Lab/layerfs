# Mounted stage lifecycle selection 01: test not selected

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first `kernel_range_ioctl_route.py --case stage_lifecycle` attempt at
source `9a42709add7ae19ac05ae21be892077a696a51c8` exited its Docker test
with code zero but selected **zero tests**. The driver formed the exact Rust
filter `linux::kernel_range_stage_lifecycle`, while the test function was
named `kernel_range_staging_lifecycle`. The driver therefore reported FAIL
with its check `NOT_RUN`; no mounted ioctl, stage lifecycle or product result
was observed. The raw [result and test output](kernel-stage-lifecycle-01/)
retain that discrepancy. The retained container and volume were removed
after copying the logs.

The only correction renames the external Rust test function to match the
frozen route selector. Product code and the 64 MiB prepared fixture remain
unchanged. The stage lifecycle case will receive one fresh output under the
new test-source identity. This receipt is not relabelled or replaced.
