# Verify R2-F6 / AT-4 / R2-F25 — the attribute-value bound is the chunk maximum

Independent verification of the round-3 claims (fixed in commit `2fe2a4642`),
performed read-only on the frozen tree. Every finding below was reproduced by me
on this tree, not taken from a report.

## Verdicts

| Row | Claim | Verdict |
| --- | --- | --- |
| R2-F6 / AT-4 | the attribute-value bound is 32,768 bytes (the chunk maximum), derived from `cdc::MAXIMUM_CHUNK_BYTES`, §6 states 32,768, no current doc says 1 MiB; boundary proves 32,768 accepted (written and read back whole) and 32,769 refused with the declared bound reported | **PASS** |
| R2-F25 | the boundary case exercises the real bound (the old case wrote only 4,096 bytes and discriminated nothing) | **PASS** (one nuance recorded in §6) |

## Repository identity

- `git rev-parse HEAD` → `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (exit 0).
  The task cited frozen commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40`; that
  exact hash **does not exist as an object** (`git cat-file -t` → exit 128,
  "could not get object info"). The cited hash and HEAD share the 9-character
  prefix `99743b2cf`, so I treated it as a typo and verified HEAD. See UNVERIFIED.
- Round-3 commit `2fe2a4642` exists: `git log --oneline -1 2fe2a4642` →
  `2fe2a4642 fix(core): bound the read wave, right-size the value limit, validate policies` (exit 0).
- Working tree clean before and after my work (`git status --porcelain`, exit 0;
  the only non-clean entry is a sibling verifier's untracked
  `verify-R2-F12-F13.md`, not mine). My only writes were gitignored build
  artifacts (`core/target`, `s5check/target`) and `/tmp`.

## 1. The constant derives from `cdc::MAXIMUM_CHUNK_BYTES` — verified

`core/crates/layerfs-content/src/filesystem/limits.rs:53`:

```rust
pub const MAXIMUM_ATTRIBUTE_VALUE_BYTES: usize = crate::file::cdc::MAXIMUM_CHUNK_BYTES;
```

with the doc comment at `limits.rs:45-52` explaining the derivation ("An
attribute value is stored as **one** extent-only root whose payload is one
canonical chunk object … The constant is derived from that maximum rather than
restated, so the two cannot drift apart."). The referenced maximum,
`core/crates/layerfs-content/src/file/cdc/gear.rs:17`:

```rust
pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;
```

Write-path enforcement is real, not read-side only —
`core/crates/layerfs-content/src/filesystem/attributes/value.rs:26-31`:

```rust
if bytes.len() > crate::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES {
    return Err(ContentError::ObjectLimitExceeded {
        limit: crate::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES,
        actual: bytes.len(),
    });
}
```

## 2. The boundary test runs, writes 32,768, reads it back whole, refuses 32,769

Command (exit 0, `1 passed; 0 failed; 0 ignored`):

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content \
  --test filesystem_failure the_attribute_value_bound --locked -- --nocapture
```

Output: `test the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary ... ok`.
Test names were first listed with `-- --list` (exit 0, 8 tests in the binary).

Test source `core/crates/layerfs-content/tests/filesystem_failure.rs:547-592`.
The case takes `limit` from the constant and asserts the derivation itself
(`:554-560`):

```rust
let limit = layerfs_content::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES;
assert_eq!(
    limit,
    layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES,
    "the declared value bound is the chunk maximum, ..."
);
```

Acceptance side — writes exactly `limit` bytes through the public write path
(`emit_value` over `FilesystemObjects` via the `with_objects` helper,
`tests/support/filesystem.rs:234`) and reads the whole value back through the
public `read_value` (`:562-579`):

```rust
let at_limit = vec![0x5a_u8; limit];
let outcome = with_objects(&mut store, |objects| emit_value(objects, &at_limit));
let root = match outcome {
    Ok(root) => root,
    Err(error) => panic!("the value exactly at the declared bound must be accepted: {error}"),
};
let read_back = layerfs_content::filesystem::attributes::value::read_value(&store, root, limit)
    .expect("the value at the bound is read back whole");
assert_eq!(read_back.len(), limit, "the read path returns exactly what was written");
assert!(read_back.iter().all(|byte| *byte == 0x5a),
    "the read path returns the written bytes, not a prefix");
```

Refusal side — one byte over, refused **by the declared bound** (`:581-591`):

```rust
let over = vec![0x5a_u8; limit + 1];
let outcome = with_objects(&mut store, |objects| emit_value(objects, &over));
assert!(
    matches!(
        outcome,
        Err(ContentError::ObjectLimitExceeded { limit: refused, actual })
            if refused == limit && actual == limit + 1
    ),
    "one byte over the declared bound must be refused by the declared bound: {outcome:?}"
);
```

Because `limit` is asserted equal to `MAXIMUM_CHUNK_BYTES` (32,768 at
`gear.rs:17`), the case writes exactly 32,768 bytes, reads them back whole, and
refuses 32,769 with 32,768 as the reported limit.

## 3. `stage-5-report.md` §6 states 32,768 — verified

Section heading `## 6. Limits` is at
`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:395`; the
attribute-value row at `:406`:

```markdown
| Attribute value | enforced | ≤ 32,768 bytes (the chunk maximum), always extent-backed |
```

The dated correction (§"Corrections to this document", `:511-525`) states "The
enforced bound is **32,768 bytes**, not 1 MiB … Reproduced through the public
API: a 1 MiB value is refused with `object limit 32768 exceeded by 1048576`."
and records the round-3 remediation. The round-3 row table (`:579` heading
"Rows this round implemented…") carries at `:586`:

```markdown
| `R2-F6`, `AT-4`, `R2-F25` | the value bound is the chunk maximum (32,768), derived rather than restated; boundary case writes 32,768 and refuses 32,769 | `attribute-boundary.log` |
```

No other "MiB" mention in the report concerns the attribute value (`:360`,
`:411` are the 4 MiB operation scratch; `:513-516` is the correction itself).

## 4. Falsification — is any 1 MiB attribute-value claim still presented as current fact?

Sweep: `grep -rn "1 MiB\|1MiB\|1,048,576\|1048576\|1024 \* 1024"
docs/roadmap/0.1/0.1.7/component-decoupling/*.md` (exit 0). **No document
presents a 1 MiB attribute-value bound as current fact.** Every hit is dated,
historical, a correction, or unrelated:

- `stage-5-matrix-remediation-20260917.md:66` — the **old AT-4 claim verbatim**:
  `| AT-4 | bounded extent-only values | FAIL | PASS | the 1 MiB value bound covers the write path (a376acfee) |`.
  This is a dated snapshot ("status on this round's tree, `f1f6cee36`", header)
  whose last commit `53f9ad70a` is an ancestor of the round-3 fix
  (`git merge-base --is-ancestor 53f9ad70a 2fe2a4642` → exit 0), i.e. untouched
  since. It is explicitly retracted at
  `stages-1-5-review-20260917T230700Z.md:246-251` ("row `AT-4` was promoted to
  PASS on 'the 1 MiB value bound covers the write path' … The `AT-4` PASS is not
  supported by its stated reason.") and superseded by `stage-5-report.md:586`.
  The old text survives only in this dated, retracted document.
- `stage-5-remediation-handoff-20260917.md:191,195` — dated R10 remediation todo
  ("enforce the 1 MiB attribute value bound on the write path"), predating the fix.
- `stages-1-5-review-20260917T160000Z.md:29,266,1023,1152,1375,1590,1610,1784,1837`
  — dated round-1 review describing the pre-fix state.
- `stages-1-5-review-20260917T230700Z.md:229-250,500,562,618,929,1068,1357,1362`
  — dated round-2 review; the F6/F25 corrections quoting the old error (allowed
  historical corrections).
- `stage-5-report.md:513,514,516` — the correction itself ("not 1 MiB").
- `stage-5-terminal-handoff-20260917.md:156,173,244` — the verification tasking
  matrix describing the FAIL state and decision options, not a bound claim.
- `implementation-issues.md:55` — states the correct figure: "a wrong
  attribute-value limit (32,768 bytes, not 1 MiB)".
- All remaining "1 MiB" hits (`content-storage-*.md`, `cluster-1-2-*.md`,
  `admission-and-persistence.md`, `object-storage.md:181`,
  `stages-0-2/1-2/3-4-*.md`, `stage-5-remediation-wp4-wp7-handoff-20260917.md:119`)
  concern whole-file cutoffs, decode workspaces, SQL BLOB limits, native chains
  or "1 MiB/50 … experimental candidates" — none is an attribute-value bound.

## 5. Independent public-API reproduction (s5check, re-run by me)

Command (exit 0): `cd docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T020000Z/diagnostics/s5check && cargo run --quiet`.
The client uses only public entry points (`emit_value`, `read_value`, the
limits constants). Probe A output, quoted verbatim:

```
A1 declared attribute bound 32768 bytes
A2 bound equals the chunk maximum: true
A3 at 32768 bytes: read back 32768 bytes, all 0x5a: true
A4 at 32769 bytes: Err(ObjectLimitExceeded { limit: 32768, actual: 32769 })
A5 refused by the declared bound: true
```

This reproduces the boundary live from the public API, independent of the test
binary: 32,768 written and read back whole; 32,769 refused with the declared
bound (32,768) as the reported limit.

## 6. R2-F25 — the old case wrote 4,096 bytes and discriminated nothing

`git show 2fe2a4642 -- core/crates/layerfs-content/tests/filesystem_failure.rs`
(exit 0) shows the removed case
`an_attribute_value_over_the_declared_bound_is_refused_on_write`. Its
acceptance arm was:

```rust
// Exactly at the bound is still an accepted value.
let at_limit = vec![0x5a_u8; 4096];
let outcome = with_objects(&mut store, |objects| emit_value(objects, &at_limit));
assert!(outcome.is_ok(), "a small value is emitted: {outcome:?}");
```

4,096 bytes, `is_ok` only — no read-back, no probe at the then-declared bound.
The same commit's `limits.rs` diff replaced
`pub const MAXIMUM_ATTRIBUTE_VALUE_BYTES: usize = 1024 * 1024;` with the derived
form now at `limits.rs:53`. Nuance, recorded for honesty: the old case's
**refusal** arm did write `limit + 1` against the old 1 MiB constant, so it
discriminated at the (wrong, partly dead) 1 MiB figure on the refusal side; what
it never did was probe acceptance at a bound — 4,096 proves nothing about any
limit, and per F6 the 1 MiB acceptance branch was unreachable. The handoff's
characterization ("writes 4,096 bytes", "does not discriminate at the stated
bound", `stages-1-5-review-20260917T230700Z.md:500`) is accurate in substance.

## Commands and exit codes

| # | Command | Exit |
| --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 |
| 2 | `git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` | 128 (no such object) |
| 3 | `git log --oneline -1 2fe2a4642` | 0 |
| 4 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_failure --locked -- --list` | 0 (8 tests) |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_failure the_attribute_value_bound --locked -- --nocapture` | 0 (1 passed, 0 failed) |
| 6 | `cd …/stage-5-terminal-20260918T020000Z/diagnostics/s5check && cargo run --quiet` | 0 |
| 7 | `git show 2fe2a4642 -- core/crates/layerfs-content/tests/filesystem_failure.rs` | 0 |
| 8 | `git show 2fe2a4642 -- core/crates/layerfs-content/src/filesystem/limits.rs` | 0 |
| 9 | `grep -rn "1 MiB\|1MiB\|1,048,576\|1048576\|1024 \* 1024" docs/roadmap/0.1/0.1.7/component-decoupling/*.md` | 0 (hits characterized in §4) |
| 10 | `git log --oneline -3 -- …/stage-5-matrix-remediation-20260917.md` | 0 (newest `53f9ad70a`) |
| 11 | `git merge-base --is-ancestor 53f9ad70a 2fe2a4642` | 0 (true) |
| 12 | `git status --porcelain` | 0 (clean apart from a sibling verifier's untracked file) |

Read-only file inspection used the read/grep tools on:
`core/crates/layerfs-content/src/filesystem/limits.rs`,
`core/crates/layerfs-content/src/file/cdc/gear.rs`,
`core/crates/layerfs-content/src/filesystem/attributes/value.rs`,
`core/crates/layerfs-content/tests/filesystem_failure.rs`,
`core/crates/layerfs-content/tests/support/filesystem.rs`,
`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md` and the other
component-decoupling documents, and the round-3 receipt
`…/stage-5-terminal-20260918T020000Z/attribute-boundary.log` (its recorded run
matches my re-run byte-for-byte in the test-result lines).

## UNVERIFIED

1. **The frozen-commit identity.** The cited hash
   `99743b2cf3a869b7d8897a1f16b82d742aeedc40` does not exist in this repository;
   all findings above are for HEAD `99743b2cff2470e6634874d7ee14b9d37d0ba16e`
   (shared 9-char prefix `99743b2cf`). If a different tree was intended, these
   findings do not transfer.
2. **No full-suite run.** I ran only the filtered boundary case (and the s5check
   client), not the whole `layerfs-content` suite; no claim is made about any
   other row or test.
3. **Receipt identity fields.** The round-3 receipt `attribute-boundary.log` was
   read and its output matches my re-run, but I did not independently validate
   its recorded source-commit identity against `2fe2a4642` beyond the report's
   own linkage at `stage-5-report.md:570-586`.
4. **Docs-policy judgment left open.** The dated
   `stage-5-matrix-remediation-20260917.md:66` still carries the old AT-4
   promotion text verbatim with no inline retraction marker; it is retracted
   only in the later dated review. Whether the owner requires an inline marker
   there is a documentation-policy decision I did not make — as presented, it is
   a dated historical snapshot, not a current-fact claim.
