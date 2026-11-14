# LayerFS paper draft

> **Status:** Research; informative and not a product contract.

Draft of a systems paper, *LayerFS: A Recursive Filesystem for Ephemeral
Computing in AI Agents* (2026-11-14). Every number in it comes from a published
layerfs-benchmark report, a commit message, or the agent-session analysis in
`notes/E-evaluation.md`. Red `[...]` notes in the PDF mark gaps to close
before submission; set `\draftfalse` in `layerfs.tex` to hide them.

| File | What it is |
| --- | --- |
| [layerfs.tex](layerfs.tex), [references.bib](references.bib) | Paper source |
| `layerfs.pdf` | Built draft (`latexmk -pdf layerfs.tex`) |
| [OUTLINE.md](OUTLINE.md) | Earlier outline, evaluation plan and gap list |
| [notes/](notes/) | Research notes: A website claims, B engine design, C core storage, D speed history, E evaluation tables and agent-time premise |
| [notes/scripts/](notes/scripts/) | Read-only scripts behind the premise numbers (they print only aggregates) |
| [appendix-cli.tex](appendix-cli.tex), [notes/cli-calls.md](notes/cli-calls.md) | Every 2.0 CLI call (73, plus 5 CLI-only commands), generated from the call table at `8cfb6e2734` |

Code identity: `origin/layerfs_v2_0_0` at `8cfb6e2734`. Benchmark builds
differ per table and are named in each caption or in the notes.
