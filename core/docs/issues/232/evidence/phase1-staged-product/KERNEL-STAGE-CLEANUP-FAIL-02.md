# Mounted stage lifecycle selection 02: cleanup Busy

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

At source `03d85272eecbfdbfb8769bcb6ecbffa2db7e8a79`, the source-corrected
mounted `stage_lifecycle` test executed BEGIN, wrong-order and wrong-FD
refusals, one DATA, ABORT, post-ABORT refusal and unchanged STATE. It then
panicked at `Workspace::close_clean()` with `Busy`. The test had used
`Workspace::set_len` to make an 8 KiB file before the mount and did not
Commit that preparation before clean close. The raw
[attempt](kernel-stage-lifecycle-02/) has exit 101 and a `NOT_RUN` final
check. It is a failed lifecycle proof despite the prior assertions. The
retained Docker container and volume were removed after log capture.

The correction adds one explicit public Workspace Commit after unmount and
before `close_clean`, matching the existing mounted test cleanup. No stage,
wire, Workspace product code or workload changes. The prior receipt remains
failed; the corrected test gets a fresh output at its new test identity.
