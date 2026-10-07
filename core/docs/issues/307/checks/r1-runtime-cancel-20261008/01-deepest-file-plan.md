# R1e exact external runtime command cancellation

> Status: implementation/qualification plan; input e5e95e76c16da65172d17b6a52bb9a25613547ec.
> Full R0–R9 Goal remains active. Main alone edits/builds/tests/Docker/proof/cleanup;
> existing read-only agents review cancellation and subsequent native FUSE work.

| Deepest source | Concrete responsibility/custody |
| --- | --- |
| Sandbox backend/docker admin command/client/types | Distinct fixed-command trusted root ctr client; no public arbitrary root Exec or daemon process supervisor. Only Version/ContainerInfo/exact signal. Same Engine context, exact acknowledged CID/ExecID, explicit signal, actual pinned server/client version and CNI observation. Caller owns isolated admin container; no Store/Overlay/config/FUSE/host PID mount |
| Existing request/streams/mux/JSON | Reuse one ordinary Engine transport/Exec protocol, root privilege private to concrete admin command vocabulary. Stream stdout for metadata parsing with exact stderr/progress/pending/error and root-exit snapshots. No whole metadata/output collection or fake qualification receipt |
| Actual owned runtime proofs | Verify published unchanged ctr artifact, reported VM socket, actual server/version and owned target metadata; then one exact command signal, independent matching Engine exit observation, surviving sibling/descendants distinctions. Lost/failed original operations retained; no replay or broad container-stop cancellation |
| Architecture/ledger/guard/checks/LOC | Existing deps and byte format, scoped source/proof limits, exact first-parent/staged/committed production source comparison for each local commit. No reference retirement before R8/R9 |

Global Store execution remains explicit Disposable/WAL/OFF; Durable NOT_RUN disabled
by owner. This runtime-only prerequisite probe creates no Store. Ordinary command
identity remains nonroot and NNP. Administrative RPC deadlines do not bound ordinary
Bash. ctr's version-specific CNI helper and transparent unprocessed gRPC transport
behavior are disclosed; one LayerFS/ctr signal invocation is never resent after an
unknown/error. Signal acknowledgment is independent of root exit, descendants,
streams and filesystem lifetime. FUSE Ready/drain source and proof remain separate.
