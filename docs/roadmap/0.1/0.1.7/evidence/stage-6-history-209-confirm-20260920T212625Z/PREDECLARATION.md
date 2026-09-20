# Pre-declaration — a same-window confirmation of the **kept** treatment

> Status: Research; diagnostic. Written before the rows were sampled.

**What is being confirmed.** `704580673` — *a step commits the policy state it changed,
not the policy state it re-asserted* — is the change that is **kept**. This round adds no
treatment and changes no product line; it re-measures the kept change in a fresh window,
because the machine's level moves by more than the effect (16.5 s → 22.5 s for identical
work between two consecutive rounds).

**No source is edited to run this.** Two already-archived binaries are used exactly as
recorded:

- `binary-archive/shipped-treatment` (sha256 `3a6c1c20…`), which the current tree rebuilds
  to **byte for byte** — the confirmation that the shipped default is the treatment;
- the kept round's lever binary (`…/stage-6-history-209-commit-20260920T194819Z/binary-archive/lever-pair`,
  sha256 `106f181b…`), whose arm is selected by the measurement-only
  `LAYERFS_STORAGE_POLICY_REASSERT` lever: **unset = treatment**, `=0` = the behaviour the
  treatment replaced.

Both carry the harness source seal `04bcfab573ab…`, and `collect.py` / `with_locks.py` here
are byte-identical to the retained ones (`diff -q` clean), so no harness change can
invalidate the pair.

**Design.** One **A B B A** window on the lever binary (control = lever `0`, treatment =
unset), then one row from the shipped binary. Every row one sample, fresh `--output`, both
global flocks, quiet preflight per `collect.py`.

**What must hold.** The saved Store must hash
`7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358` in every row with
`commits` 48,446, and `commit_ns` must be lower in the treatment rows than in the control
rows. **A window's absolute level is not a claim**; only the within-window contrast is, and
the shipped row is a confirmation that the default reproduces the treatment arm, not an
effect size.
