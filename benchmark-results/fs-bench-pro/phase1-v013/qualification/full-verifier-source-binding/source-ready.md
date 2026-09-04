# Exact full-verifier source compatibility

Source ready; no test, build, report or product execution performed by the compatibility worker. The coordinator qualifies the changed full-verifier path separately.

The historical7948 verifier retains SHA256 `c0bdb1d9e2faef6efe7f542f2a7a1cd35fe1c1ba1c21991c16ec22f34b9bd4e4`. The successor verifier is pinned to `346bcc35e0db2df1975193563aaa46669daddd4da882f9bca360776b2322b320`. The old hash is accepted only for exact revision7948; old source/preparation receipts are never regenerated using the successor hash.

The pair requires identical product/build inputs, unchanged normative contracts, and no benchmark changes beyond workspace_verify.rs and runner/report compatibility maintenance. Full metadata, aliases, content reads, extents and typed graph census remain exhaustive. Performance samples remain measurements of their original7948 VM8 source. The report retains matching input/environment requirements and keeps fast assurance outside the full gate.

After the successor build passes, run `python3 target/phase1-bind-full-verifier-assets.py /absolute/path/to/assets-SUCCESSOR`. This uses the existing source-map validator, binds all active7948 performance slots, preserves37f5 full proofs and the600-second standalone proof, and retains the7948 fast demonstration separately. It snapshots the prior map and restores it if source-map validation fails. It updates only source selection and the future full-verification plan, then reports the receipt path; it does not generate a report or execute workloads.

The verification driver accepts `--start-ordinal` and delegates successful-slot reuse to the existing runner. Cache eviction remains an explicit coordinator-owned existing-schema plan at family boundaries; metadata and producer evidence are archived, cache limits unchanged.
