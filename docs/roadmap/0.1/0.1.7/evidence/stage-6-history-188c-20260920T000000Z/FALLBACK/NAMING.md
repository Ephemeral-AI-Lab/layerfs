# Renaming "fallback" — **declared** versus **similarity**

## Why the old name was wrong

"Fallback" carries two implications, and the code contradicts both:

1. **It implies a failure preceded it.** Nothing has failed when the source is consulted; the rule is
   evaluated *before* any work is attempted. And a failed acquisition is a **hard error** —
   @select.rs:294@ propagates it — which is the opposite of falling back.
2. **It implies a degraded mode.** It is a *second candidate source*, and the alternative it actually
   faces is **no base at all** (~2.6x), not the declared base (~17.3x).

It also collides with a rule this repository enforces: @core/AGENTS.md@ forbids *"error-driven alternate
algorithm"*. A name that sounds like one invites exactly the wrong review.

## The name, and why this one

The codebase already has both words:

| source | who knows the correspondence | product precedent |
| --- | --- | --- |
| **declared** | the **caller** names the previous version of the same path | @AdvisoryPredecessors@; @file/edit/apply.rs:121@ does it |
| **similarity** | the **store** finds a content-similar object, usually at another path | the content-signature index, @encoding/delta/candidates.rs@ |

**So: declared versus similarity.** It says where the candidate came from — which is the only fact that
distinguishes the two sources — and it says nothing about failure.

Considered and rejected: *cross-path* (usually true but not definitionally — the index is keyed by
content, not by path), *discovered* (names the contrast, not the mechanism), *signature* (accurate but
jargon), *candidate cache* (understates it — this arm spans saves; the product's does not).

## What was renamed

| before | after |
| --- | --- |
| @fn fallback_predecessors_enabled()@ | @fn similarity_candidates_enabled()@ |
| @LAYERFS_HISTORY_FALLBACK_PREDECESSORS@ | @LAYERFS_HISTORY_SIMILARITY_CANDIDATES@ |
| @LAYERFS_HISTORY_SAME_PATH_ONLY@ | @LAYERFS_HISTORY_DECLARED_ONLY@ |

**Verified after the rename** — the arms are byte-identical to before it:

@```
  default (declared only)        56,049,664 B   = 1.13654x v0.1.6
  SIMILARITY_CANDIDATES=1        49,672,192 B   = 1.00723x v0.1.6
  DECLARED_ONLY=1                56,049,664 B   = 1.13654x v0.1.6   (same arm from the other side)
@```

## What was deliberately NOT renamed

**The squad reports V1-V6, E1-E4 and A1-A4 keep the old word.** They are the record of what was measured
under that name at that time, and rewriting them would be rewriting history — the repository forbids it.
Where this round's own documents are being authored now, they use the new pair and say *"formerly called
the fallback"* once, so a reader arriving from an older report can still follow the trail.
