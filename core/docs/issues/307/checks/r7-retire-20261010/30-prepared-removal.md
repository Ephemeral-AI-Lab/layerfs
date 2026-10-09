# R7-retire: prepared removal of the four crates kept for O-10

> **Status:** Executed 2026-10-10. Written as a planning checklist; kept as written below.
>
> **Addendum 2026-10-10:** the owner authorized the removal ("yes, do the full cleanup") whatever the
> answer to O-10. The four steps were carried out as commits `d6b7539f7` (−505), `b56950220` (−2151),
> `eeeffb6b5` (−6834) and `e6794521a` (−26835), each matching its expected delta. O-10 stays open as a
> product question. See the
> [completion record](../../R7-RETIRE-COMPLETION-20261010.md#addendum-full-cleanup-by-owner-authorization).

Four excluded crates stay in the tree because they carry pinned read-only view
code, and whether the product keeps pinned views is the unanswered owner
question O-10 ([scope and method](00-scope-and-method.md#the-one-open-owner-question-that-blocks-removal-o-10)).
Their audits found nothing else that blocks removal: every other behaviour and
test is class A or C.

```
core/crates/                        [36325]  (part: the four kept crates)
  layerfs-bridge-legacy/             [6834]
  layerfs-daemon-legacy/             [2151]
  layerfs-sdk-legacy/                 [505]
  layerfs-workspace-legacy/         [26835]
```

Scope of the listing: `cf5bd62ce`, pinned `tools/production_loc.py`, whole
production total of each directory. Tests are not counted.

## What the owner's answer changes

| Answer to O-10 | Then |
| --- | --- |
| Defer or drop pinned views | Carry out the steps below. Nothing else is owed first |
| Keep pinned views | Pinned views need a design on the SQLite overlay first (303/08 K20: "They change the version bound"). The predecessor's implementation rests on the private backing, which is removed by owner requirement, so it cannot be activated. The owner then decides whether the four crates wait for that design as reading material or are removed at once and read from Git |

## Steps, once authorized

One commit per crate, each with its own count from `core/target/rx-count.py
staged`, labelled retirement:

1. `git rm -r core/crates/layerfs-sdk-legacy`; remove its `exclude` entry.
   Expected production delta −505.
2. `git rm -r core/crates/layerfs-daemon-legacy`; remove its entry. Expected
   −2151.
3. `git rm -r core/crates/layerfs-bridge-legacy`; remove its entry. Expected
   −6834.
4. `git rm -r core/crates/layerfs-workspace-legacy`; remove its entry.
   Expected −26835. `core/Cargo.toml`'s `exclude` then holds only
   `vendor/fuser-0.18.0`.

With each: add the directory to `RETIRED_PACKAGES` in
`core/tools/check_product_boundary.py` (its test iterates the tuple), and
replace the "preserved … S11 must remove it" paragraph in the matching
architecture document with the removal and its recovery commit:

| Crate | Architecture document |
| --- | --- |
| `layerfs-sdk-legacy` | `22-sdk-runtime.md` |
| `layerfs-daemon-legacy` | `21-daemon-owner.md` |
| `layerfs-bridge-legacy` | `23-native-bridge.md` |
| `layerfs-workspace-legacy` | `20-workspace-base.md` |

Then `core/AGENTS.md` (the sentence that four predecessors remain), the
R7-retire ledger row and the completion record. Checks: `fmt --check`, the
guard and its self-tests, and a workspace build; the removed directories are
in no build, so no test result can change, but the stage's rule is to run the
suites at the final commit.

Recovery of any removed file: `git show <parent of its removal commit>:<path>`.
Until then the four directories are recoverable from `HEAD`.
