# #286 r026: allocation diagnostic invalidated by stale binary path

> **Status: INCOMPLETE diagnostic; no candidate sample and no gate result.**
> The source declaration named `2aaaf8d8f`, but the invocation used a binary
> from the wrong worktree-local Cargo target directory. Preserve the attempt.

The frozen method was a count-driven `history-stride1` run with
`LAYERFS_HISTORY_BLOCK_DIAGNOSTIC=1`, the original v3 corpus/root ledger,
157 states, one construction worker and the existing 170 s command bound.
The release build made from the benchmark manifest produced SHA-256
`8f52b440238a548d117716759adc28794b580404b7421172ba85ceaf1bf1b661`
under `core/benchmark/fs-bench-pro-storage-content/target/release/`. The
invocation script instead selected `core/target/release/fs-bench-storage-content`
with SHA-256
`d0b04009e6a01cc00a8f02316cc561aba2d4ed877780b786b3d7dcfc41c2107f`.
Its trace contains **zero** `diagnostic.state.*` counters, proving that the new
probe did not run. The command exited zero in **167.802 s**, but neither its
time nor its C2/C5 allocation is a current-source gate sample. Its closed
files occupy 84,561,920 and 196,608 B according to this diagnostic's original
owner metadata; that number cannot supersede r021 or r025.

The [SHA-indexed raw declaration, logs and original files](20260930-history-block-diagnostic-r026.json)
remain append-only in the local worktree. The next command uses the **actual**
benchmark-manifest target binary by its validated SHA in a new output directory,
then checks for 157 complete C2/C5 allocation and apparent-size observations.
Family 2 remains storage FAIL at its prior official r021/r025 receipts; families
3–7 remain NOT_RUN. The family-3 registry added in source `2aaaf8d8f` collects
no sample.
