# Verification that the F12 fix is behaviourally equivalent to the reference

Question: is the restored per-field ceiling *correct*, not merely present?
Method: a 34-probe differential corpus fed to both decoders, plus the encoder on
six value sizes. The candidate and reference crates are linked by path in separate
harnesses (the reference one read-only).

Result: **no semantic divergence on any probe.**

| Probe class | Probes | Candidate vs reference |
| --- | ---: | --- |
| Encoder accept/reject and encoded length | 6 | identical decision and length for 0, 1, 4096, 8 MiB (accepted) and 8 MiB+1, 12 MiB (rejected) |
| Valid object round trip | 4 | identical `ok:<len>` for 0, 1, 4096, 8 MiB |
| Truncated by one byte | 4 | identical `eof` |
| One trailing byte | 4 | identical `trailing` |
| Wrong magic | 4 | identical `framing` |
| Non-Bytes kind tag (`9`) | 4 | both reject; candidate `role`, reference `InvalidObjectKind { tag: 9 }` |
| Hand-built length edge cases | 8 | identical decision on every one, including `payload=0`, `payload=3`, `value_len=8 MiB+1` with a legal payload, the envelope maximum `16,777,203`, an envelope-illegal `16,777,204`, `payload_len=u32::MAX` and a declared-length mismatch |

The only differences are diagnostic vocabulary, never a decision:

1. The candidate's `ObjectLimitExceeded` carries `limit` and `actual`; the
   reference's variant carries neither. Strictly more information, same rejection.
2. For an invalid kind tag the candidate reports `WrongLogicalRole` while the
   reference reports `InvalidObjectKind`. The reference first dispatches to its
   multi-kind object decoder, which this slice does not implement. Both reject.

Coverage argument: every candidate encode path funnels through `canonical_len`
(`encode_bytes_object` line 71, `encode_bytes_object_to` line 51, and the mapping
encoders via `encode_bytes_object`), and every candidate decode path funnels
through `decode_bytes_object` (`FinalizedObject::new`, the mapping decoders, and the
storage read path), so the guard cannot be bypassed.

Scope check: the largest value any role produces is 131,081 bytes (WHOLE_FILE),
about 1.6% of the field ceiling. The ceiling would still admit a construction
cutoff of up to roughly 8 MiB, well beyond the 256 KiB and 1 MiB values the policy
document names as experiments.
