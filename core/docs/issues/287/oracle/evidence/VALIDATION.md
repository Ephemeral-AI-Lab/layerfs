# R0 independent reference validation

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> 2026-09-30. Product parent `7edddbdb8e8512627aed0ed42533ef099d802384`.
> Owner scope: oracle/ and R0-CANONICAL-CONTRACT.md only.

All commands ran from the owned implementation worktree
`/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs`.
No product implementation, candidate comparison, benchmark or provider test ran.

## Commands and actual outcomes

1. A manually reduced Core dependency closure followed by
   `cargo +1.85.1 build --release --locked --offline --manifest-path core/docs/issues/287/oracle/hash/Cargo.toml`
   exited101: the generated helper lock needed an update. No hash-helper build
   completed in that attempt.
2. `cargo +1.85.1 generate-lockfile --offline --manifest-path core/docs/issues/287/oracle/hash/Cargo.toml`
   succeeded. The dependency equality audit refused newer cached cc1.4.7,
   cfg-if1.0.5 and find-msvc-tools0.1.13. A shell sequence continued to build that
   preliminary helper despite the audit's assertion failure; that preliminary
   binary was not used for final vectors. Its build completed in4.08s. These
   setup timings are command outputs, not performance evidence.
3. Explicit offline `cargo update -p cc --precise 1.4.5` succeeded. An unnecessary
   `cargo update -p shlex --precise 1.3.0` refused with exit101 because cc1.4.5
   requires shlex^2.0.1; the retained Core shlex2.0.1 was already correct.
4. Explicit offline `cargo update -p cfg-if --precise 1.0.4` and
   `cargo update -p find-msvc-tools --precise 0.1.12` succeeded. The external
   audit confirmed every helper dependency name/version/checksum exists in
   Core/Cargo.lock. The final locked/offline build command from step1 completed
   successfully, output `Finished release profile ... in2.22s`. This version
   supplied all final-reference hashes. Product dependencies were untouched.
5. The first reference generation internally passed8 file/6 namespace/9 parent/
   9 rejection vectors and261 objects. Subsequent source review found a malformed
   reference symlink missing its inner role/flags. That draft is retained as
   [draft-vectors-symlink-invalid.json](draft-vectors-symlink-invalid.json), with
   disposition **INELIGIBLE**. Its internal PASS cannot certify its namespace
   child-role closure or be used as an expected result for implementation.
6. The source-demonstrated correction added the existing symlink inner role5,
   flags0 and length field, plus explicit regular/symlink content decoding during
   certification. It also preserved all7 existing sealed v1 codec ObjectIds,
   added an authenticated mismatched parent-index rejection and made v2's
   delete-only finalization boundary explicit. Then:

   ```sh
   python3 core/docs/issues/287/oracle/verify.py --write core/docs/issues/287/oracle/vectors.json
   ```

   exited0 and returned:

   ```json
   {"status":"PASS","files":8,"namespaces":6,"parent_transitions":9,"rejections":10,"objects":262,"source":"7edddbdb8e8512627aed0ed42533ef099d802384","claim":"independent reference identities and internal decode/certification only"}
   ```

The final vector ledger carries exact contract/method/helper-lock SHA-256 hashes.
contract.json pins20 inspected source/compilation inputs and7 unchanged v1 codec
files. It captures the CDC table as a format input; no product table is read to
derive new expected roots at run time. Existing v1 fixtures are read only to
confirm their retained bytes/identities. The golden ledger is new evidence,
separate from the ineligible draft; nothing was overwritten or relabeled.

## Limits of this checkpoint

No v2 product exists at R0. The executed edit vectors contain at most22 extents;
larger mapping split/unequal-height-join matrices are unrun although their
independent reference operations are implemented. Namespace certification is a
finite initial full-closure reference, not a Server-issued validity lease or
incremental cross-move proof. V1 fixed/codec identity checks do not constitute
the full v1 campaign. All product tests/examples/fmt/Clippy/boundary/provider/
resource/liveness/performance checks are unrun in this owner slice and remain
root-owned or later milestone gates. Linux/Darwin provider behavior is unproven.
The reference's resident fixture maps are development state and support no
bounded production-memory claim. #288 benchmark qualification remains unrun.
