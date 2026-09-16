# Reviewer finding F12: the per-field envelope ceiling is missing

Question that surfaced it: if a CDC chunk is at most 32 KiB, why is the envelope
ceiling 16 MiB? Answering that required comparing both envelope guards with the
reference, which showed the candidate implements only half of them.

The reference applies **two** independent size guards to a Bytes object
(`crates/layerfs-content/src/object/codec.rs`):

```text
payload_len <= MAX_OBJECT_BYTES - HEADER_LEN     (16 MiB - 9)
value_len   <= MAX_OBJECT_FIELD_BYTES            (8 MiB)
```

The candidate implements the first only: neither `canonical_len` /
`encode_bytes_object_to` nor `decode_bytes_object` bounds `value_len` at 8 MiB.

Harness sources are retained beside this file; both link the real crates by path
(the reference one from `crates/layerfs-content`, read-only). No product source was
modified.

```text
value   1048576 B (  1.0 MiB): encoded=  1048589 B  decode=ACCEPTED (1048576 B)
value   4194304 B (  4.0 MiB): encoded=  4194317 B  decode=ACCEPTED (4194304 B)
value   8388608 B (  8.0 MiB): encoded=  8388621 B  decode=ACCEPTED (8388608 B)
value   8388609 B (  8.0 MiB): encoded=  8388622 B  decode=ACCEPTED (8388609 B)
value  12582912 B ( 12.0 MiB): encoded= 12582925 B  decode=ACCEPTED (12582912 B)
value  16777202 B ( 16.0 MiB): encoded= 16777215 B  decode=ACCEPTED (16777202 B)

reference decode_bytes_object rejects value_len > MAX_OBJECT_FIELD_BYTES = 8 MiB
candidate decode_bytes_object has no equivalent field check

### reference (crates/layerfs-content), same value sizes
reference MAX_OBJECT_FIELD_BYTES = 8388608
value   1048576 B (  1.0 MiB): encoded=  1048589 B  reference decode=ACCEPTED (1048576 B)
value   8388608 B (  8.0 MiB): encoded=  8388621 B  reference decode=ACCEPTED (8388608 B)
value 8388609: reference encode rejected: ObjectLimitExceeded
value 12582912: reference encode rejected: ObjectLimitExceeded
value 16777202: reference encode rejected: ObjectLimitExceeded
```

Reading: the candidate encodes and decodes an 8 MiB+1, 12 MiB and 16 MiB−14 Bytes
value; the reference refuses to *encode* anything above 8 MiB and would reject it on
decode. The divergence is two-sided — the candidate can produce an object the
reference cannot produce, and accepts one the reference rejects.

**Reachability: not through the product API.** Every role the candidate produces is
far below the field limit (whole file 131,094 B, chunk 32,789 B, mapping page
8,192 B, file state ~116 B), and a hand-built object is refused one step later by
`owner.record_body` (`GROUP_LIMIT` 65,536). It is an envelope and public-API
contract divergence plus a foreign-bytes acceptance difference, not a
Store-corruption path.

**This was missed by the review.** `object_identity.rs` asserts payload-overflow
rejection (`u32::MAX`) but never probes the 8 MiB field boundary, so criterion 1.7
was recorded PASS too generously.

Smallest fix: add `MAX_OBJECT_FIELD_BYTES = 8 MiB` and apply it in
`canonical_len`/`encode_bytes_object_to` and `decode_bytes_object`, plus a boundary
test at 8 MiB (accepted) and 8 MiB + 1 (rejected).
