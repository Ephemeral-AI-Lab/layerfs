# Stage 5 ordering diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source: `0979fbd5bcd46352f36dcc5f4a8ffd23786111328`. Public-API diagnostic only.
See the [investigation and recovery order](../../component-decoupling/stage-5-blocker-investigation-20260917.md).

- Root files: first diagnostic attempt, whose client compile failed because the
  rustc dependency lookup path omitted `deps/`; preserved unchanged.
- [attempt-2/probe.stdout](attempt-2/probe.stdout): reproduced zero backing byte
  counters, successful C1 completion without checked cleanup, and missing returned
  ordering counters. Assertions confirm those defects, not correct product behavior.
- [attempt-2/run.py](attempt-2/run.py): locked build/compile/run procedure, executed
  under the shared measurement lock. Run only from a fresh copied evidence directory;
  output logs use exclusive creation and must never be overwritten.
- [attempt-2/identity.json](attempt-2/identity.json) and
  [attempt-2/manifest.json](attempt-2/manifest.json): source/library/binary and local
  evidence identities. No product files were changed.

The three-inode case uses opaque placeholder content IDs to isolate tree handling;
it makes no C2 admission/readback claim. Its files are explicitly removed after the
observations. No benchmark, cold-cache claim or full test-suite claim is made.
