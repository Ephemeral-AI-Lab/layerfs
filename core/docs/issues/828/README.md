# #828: 2.0 file, folder and crate layout

> **Status:** Owner-approved plan (2026-11-27), committed by owner ruling as an
> exception to the no-records-in-tree rule. It is the authority for the #828
> restructure of `layerfs_v2_0_0` and becomes historical once that pull request
> merges.

- [restructure-plan.md](restructure-plan.md): the execution plan (waves 0–3,
  agent groups, speed rules, checks, terminal pass).
- `mapping-{core,engine,control,api}.json`: one row `{old, new, change, note}`
  per file; `current-*.csv` is the base tree (`3203f6acbf`) with production LOC.
- [layout-style.md](layout-style.md): the owner's layout rules; §5 lists the
  settled decisions.
- `build.py`: the layout checker (`python3 -I build.py core/docs/issues/828`
  writes `site/layout.json`, ignored by Git, and prints the rule report).
- `apply_moves.py`: applies reviewer move lists to a mapping.
- `move.py`: the one move tool that applies mapping rows to the tree.
- `reviews/`: the reviewer reports behind the moves.
- `production_loc_files_3203f6acbf.txt`: per-file production LOC at the base.
