# #245 Phase 4: inherited directory rename (#258)

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source: `bdf03d8c552c5807670f0fa69f7f8bd83f89cb8c` on
`codex/issue258-phase4`, stacked on the unmerged #260 repair commit
`6af2c5c59a48d0b6c85d656e55aecc353e346728`. Phase 3 is
`7499d6d5632126273dd15b1a07e85ae8fba4d825`. This note records functional
checks and a count diagnostic only. It is not a registered performance row.

## Implementation

The public route remains `WorkspaceApi::exec(command)` through mounted POSIX
operations and an explicit Commit. The Workspace publishes the two parent
deltas and one canonical-origin record for a previously unrecorded inherited
directory through one candidate root. The record retains the stable directory
serial and immutable origin root. `FilesystemRead` resolves a child and lists a
directory by serial inside that root; Bridge Inspect subtags 5–7 carry those
queries. Neither rename nor lookup copies up the inherited descendants.

Before publication, rename checks the destination, replacement type and
emptiness, descendant cycle, and all resident child paths. It checks resident
paths again under the final state lock because lookup can add a pin without
changing the namespace revision. A longer or deeper destination prefix also
walks the effective inherited subtree to prove the 4,096-byte and 256-component
path limits. Every frame is charged and the scan has no fixed descendant-count
ceiling. A refused path leaves the revision and namespace unchanged. The
serial-ordered Commit cursor retains one later record on a kind miss, and a
same-generation move-back clears the corresponding tombstone row charge.

## Frozen-source functional checks

All binaries below are locked Cargo **release** builds. The public SDK test
used Linux daemon image
`sha256:5a6d820416cec32e2cb43ea0bf7c5db46b3915bf40b55cc109387d168955e61b`
(`layerfs-daemon` SHA-256
`7d14bd80680cb9cf4e715fefe8ce8ae2164b682c89821322b88bec0c03536a47`).
The host release `agent_route` binary SHA-256 was
`148a693436ad9d6121382eb1517f2be4130e1faa615c642a3cdb840568119889`.
The Linux release `inherited_workspace` test binary SHA-256 was
`3847e2ea73ae6abaaacba9f52859bde982f97726cecbfd4011d49ccedccf6fda`.

The [SDK external test](../../../crates/layerfs-api/sdk/tests/agent_route.rs)
prepared a base with `packages/old/subtree/child/grand.txt` and
`sibling.txt`, made one B0 Commit to retain an old head, then ran **one**
mutation Exec:

```sh
umask 022 && mv packages/old/subtree packages/new/subtree && printf grand-new > packages/new/subtree/child/grand.txt.next && mv -f packages/new/subtree/child/grand.txt.next packages/new/subtree/child/grand.txt
```

Exec exited zero and one following explicit Commit published B1. Direct
Service `Inspect::List`/`Attributes` and `ReadFile` calls, independent of the
mount's read path, checked the complete small tree: root names, both package
parents, moved subtree and child listings, stable directory/child serials and
modes, replacement and unchanged file bytes, no old or `.next` name, and B0's
original bytes and bindings. The observed mounted projection counts were
`lookup=30`, `getattr=42`, `read=0`, `write=4`, `readdir=0`, `open=0`,
`setattr=0`, `rename=2`, `other=0`. The test and Docker cleanup passed;
complete external command wall was 2.783 s. These are functional observations,
not a latency gate or cache-qualified sample.

The [Linux external Workspace tests](../../../crates/layerfs-api/sdk/tests/inherited_workspace.rs)
ran against the production Service with private backing on an owned ext4 Docker
volume. One release test command completed in 0.55 s: **6 passed, 0 failed**.
It covered inherited child lookup/list, held file and directory handles and
lookup pins across move and Commit, invalid directory replacement, descendant
cycle refusal, nested move and move-back, a temp-file replacement under the
destination, fresh-upper control, a base file move from a previously unmodified
root, a frozen G1 move with a later G2 file write and two sequential heads, and
an exactly 4,096-byte cached descendant path followed by an atomic 4,097-byte
refusal. The owned ext4 volume was empty after the run and removed.

| Untimed count diagnostic | 3 inherited descendants | 67 inherited descendants |
| --- | ---: | ---: |
| Upstream Store calls during equal-length rename | 2 | 2 |
| Workspace metadata page reads | 2 | 2 |
| Added metadata backing bytes | 16,384 | 16,384 |
| Added payload backing bytes, as reported by `BackingStatus` | 16,384 | 16,384 |
| Added accounted host memory bytes | 2,190 | 2,190 |
| Resident nodes before rename | 5 | 5 |
| Explicit Commit and clean close | PASS | PASS |

This diagnostic observes the unchanged-prefix-length rename path. The private
page count and bytes come from Workspace status, and Store *calls* are the
Workspace's upstream call counter. C1's internal Store page visits were not
measured; none are inferred from the call count. No cache state, timer, resource
peak or source arm was registered, so the table cannot support a speedup or
release admission claim.

## Remaining limits

- A growing-prefix move currently scans its inherited descendants before
  publication. A canonical maximum-relative-path summary or another sound
  index is needed to make that safety check independent of subtree size. The
  equal-length count diagnostic does not prove that stronger claim.
- C1 `validate::check_parent_aliases` still scans the full base tree for a
  rebound stored directory, and `check_effective_cycles` walks the effective
  moved subtree. Commit is **not** proved path-local. These reads remain inside
  the explicit Commit and its resource/deadline limits.
- The full-size public #258 benchmark was not run, per owner direction. No
  performance receipt, cold-cache eligibility or release admission is claimed.
  #258 and parent #245 remain open for the indexed-growth and broader
  package-scale/performance decisions.

Two older `namespace_route.py` bootstrap attempts were retained locally as
functional setup failures, not counted as test runs: its default pointed at a
debug daemon (refused under the release-only rule), and with an explicit
release daemon its historical raw prepared command returned `InvalidInput`.
The final tests used the public SDK and production in-process Service instead.
No historical benchmark receipt was changed or promoted.

## Checks and production size

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`: PASS.
- `cargo +1.85.1 clippy --release --locked --manifest-path core/Cargo.toml -p layerfs-bridge -p layerfs-content -p layerfs-server -p layerfs-workspace -p layerfs-sdk --all-targets -- -D warnings`: PASS.
- Locked release host builds of the changed packages/tests and locked release
  Linux builds of the daemon and external test: PASS. No debug binary was used.
- `python3 core/tools/check_product_boundary.py` and
  `python3 -m unittest discover -s core/tools -p 'test_*.py'`: PASS (316
  production files scanned; 9 guard tests).
- `git diff --check`: PASS. The aggregate Core test suite and full benchmark
  were not run; focused test commands each finished under 30 s.

`adfe93ee9` recorded production LOC `123848 -> 124047` (`+199`): Core
`58431 -> 58630`, reference `65417 -> 65417`. `bdf03d8c5` recorded
`124047 -> 124196` (`+149`): Core `58630 -> 58779`, reference unchanged.
Both comparisons use `tools/production_loc.py` on the exact first-parent and
staged tree with tests, docs and tooling excluded.
