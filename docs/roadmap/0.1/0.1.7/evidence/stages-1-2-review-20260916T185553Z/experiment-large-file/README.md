# Reviewer experiment: is 16 MiB a file-size limit?

Question raised after the review: does 'Largest canonical object 16 MiB' cap the
size of a file that can be accepted?

Method: construct, store, and read back a 20 MiB file (larger than 16 MiB) through
the real public C1 and C2 entry points. Release build, single sample, exploratory
diagnostic only - not admission-eligible and not a benchmark.
Source: experiment-large-file/bigfile.rs (harness lives outside the repository and
path-depends on the candidate crates; no product source is modified).

```
input: 20971520 bytes = 20 MiB
C1: root=3dad99502d3e44d64f988ee5da746ae165fc662f7a24336b8194d4c33f8eb84c logical_len=20971520 objects=1086 accepted_in=31.3ms
roles: {"Chunk": 1075, "ExtentBranch": 1, "ExtentLeaf": 9, "FileState": 1}
largest single canonical object: 32789 bytes (32.0 KiB) = 0.1954% of 16 MiB
C2 save: inserted=1086 reused=0 packs_created=88 commits=23 in 127.8ms
C2 reopen + C1 readback: 20971520 bytes in 45.8ms, byte-identical=true
store file: 21495808 bytes (20.5 MiB), overhead vs input = 2.50%
```

Reading: the 16 MiB value is a per-canonical-object envelope ceiling, not a
file-size limit. The largest object in a 20 MiB file is a 32,789-byte chunk,
0.20% of that ceiling. No file-size constant exists anywhere in either crate;
the only arithmetic ceiling on logical length is u64, enforced by checked
addition (LengthOverflow) rather than by a policy limit.
