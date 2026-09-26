# #241 silent Exec control v1 result

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This is a synthetic functional diagnostic, not a performance row.

The single frozen [attempt](attempt-01/result.tsv) at source
`f3d51dda7099fe024a39de8a88df49b40c76735f` returned the expected
control-path error pair. Public SDK `WorkspaceApi::exec("sleep 6")` returned
`Failure(Unknown, unknown=true)` after **5.003852042 s** of host context wall.
The immediately attempted SDK unmount returned `Failure(Io, unknown=false)`.
SDK Sandbox deletion succeeded and the final list was empty. The complete
test command took 12.38 s, including Project initialization, container
lifecycle and cleanup. No range EDIT or Commit ran.

The [identity](attempt-01/IDENTITY.json) pins the clean source tree, test
binary SHA-256 `93aa3ca1735558c094e8cd8ea31ab05394ff857a8464d45687817687f9eefded`,
and immutable Linux image
`sha256:03d791bd1c133c438f621eb713ef933a33adf00e0ac5c79eb37c9436bfbc1a27`.
The [raw console](attempt-01/console.txt.gz) is losslessly compressed, and
the [SHA manifest](attempt-01/SHA256SUMS) verifies the retained files. Cache
state was uncontrolled; these walls are diagnostic context, not latency PASS.

This shows that a silent Exec can exhaust the native five-second progress
clock before its 30-second operation deadline and that a fresh unmount can
receive Io while the old daemon control session is occupied. It **does not**
show that the historical small `printf` command stalled for five seconds,
that its FUSE write reached the kernel, or where its terminal response was
lost. The earlier [integrated campaign](../position-sweep-integrated/REPORT.md)
remains 243 PASS / 1 FAIL / 20 NOT_RUN and Phase 3 remains incomplete. Do
not repeat this control, widen the timeout, or relabel the historical case to
pass. A product fix must target an observed cause of that baseline stall or
response loss.
