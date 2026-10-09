# R8 original startup refusal and prospective preparation correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Registered source35971624f/registration3748e37ce invocation
[047](047-full-mounted-proof/result.json) FAILED before Ready/mount/oracle.
[Original container log048](048-failed-startup-inspection/stderr.txt) says
`daemon I/O: protected file ownership/mode/type`.

`application/config.rs::protected_file` requires a regular file owned by the
daemon with no group/other permission bits. The streaming harness copy used the
shell's default umask and produced0644; the metadata was not checked by preparation.
This is a harness preparation defect. The product correctly refused it. No Store
open, Mount or full comparator invocation occurred; full-byte oracle attempts0.

The copy now declares umask077, checks0600/root ownership and exact byte length
alongside the same sealed digest, and records that backing metadata. These are
protected backing permissions; the captured root's portable metadata, content,
workload, oracle, cache contract, resource profile and all stops are unchanged.
There is no checker/test/product weakening or source optimization. A fresh named
volume will be prepared from the immutable host seal and a new controller/source
identity registered before one new invocation. The failed container
`c4f1d18cd912a7fd0d5d9b3e788847b81431198d194c6ae203dafb13f3206819`
and original volume remain retained; neither is modified, replayed or removed.
The container identity comes from the original SandboxFailure in047, not a guess.
