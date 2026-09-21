# Correction: the transport numbers here were built without the ARMv8 flags

**The receipts in this directory are unchanged and remain what they measured.**
What is corrected is the *scope* of their transport figures: every binary used here
was built with `cargo … --manifest-path core/Cargo.toml` from the repository root,
which does not pick up `core/.cargo/config.toml`, so the bridge fell back from
ARMv8-accelerated AES-GCM to ChaCha20-Poly1305.

Measured head to head on the same source and machine:

| | this directory's build (no flags) | rebuilt with the flags |
| --- | ---: | ---: |
| transport, 1 stream | 0.212-0.222 GB/s | **0.783 GB/s (802 MiB/s)** |
| transport, 2 streams | 0.441 GB/s | **1.520 GB/s (1556 MiB/s)** |
| route, 1 x 512 MiB | 73.7-77.3 MiB/s | **95.8-104.7 MiB/s** |
| route, 64 x 8 MiB | 68.4-70.8 MiB/s | **91.2-92.3 MiB/s** |

Consequences for the claims made here:

- any sentence of the form "no measured product path reaches multi-GB/s" is
  **wrong for the flagged build** and applies only to the unconfigured one; the
  flagged build reaches 1.556 GiB/s on two streams;
- the transport and route **rates and core counts** are depressed by the slow
  suite and are superseded by the numbers above;
- the *relative* findings are unaffected and stand: the 64 x 8 MiB batch costs
  about +9% against one 512 MiB operation in both builds, concurrency still pays
  (1.80x at eight operations, measured in the unconfigured build - not re-measured
  with flags), and admission/refusal behaviour is identical because it does not
  touch the AEAD;
- the in-process C1/C2 numbers (store rate, drift, shape, memory) never enter the
  AEAD path and stand as measured, with a small unquantified codegen delta.

Root cause, the committed-manifest provenance and the proposed guards:
[the AEAD build-config record](../issue216-aead-build-config-20260921T024000Z/README.md).

## Second correction: the +9% penalty is not resolvable

A third sequence, under the enforced build profile, measured the batch **-6.7%**
against the single operation (4.859 s against 5.210 s). Three sequences now give
+9.0%, +9.1% and -6.7%; pooled over the two configured-profile sequences the
difference is +3.8% with a range of -6.7% to +9.1%. The batch is therefore within
this host's noise of one big operation, and only the few milliseconds of
per-operation fixed cost (5.45 ms of which is this harness spawning a process per
operation) are established. See [the re-check](../issue216-recheck-enforced-profile-20260921T031500Z/README.md) section 3.
