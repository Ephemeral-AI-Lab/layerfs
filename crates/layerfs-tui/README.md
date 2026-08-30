# LayerFS V2 TUI mock

This isolated worktree builds the TUI against the concrete, in-memory
layerfs_cli::CliSession seam. The fixture models one LayerStackStore, one
BranchStore, three named LayerStacks, Reference and Replica pull boundaries,
remote and local Branches, a depth-four rollout tree, Commit-anchored
Workspaces, operations, receipts, storage, and the three Diff forms.

Run the interactive TUI:

    cargo run -p layerfs-tui

Render a deterministic headless screen:

    cargo run -p layerfs-tui -- --dump topology --width 200 --height 60 --no-color

Run one refined mock CLI operation:

    cargo run -p layerfs-cli -- branch pull B-main --through C-B-main-42 --replica

The mock has no socket, database, HTTP service, FUSE mount, or backend
compatibility adapter. Replacing its CliSession implementation with the V2
backend keeps the frontend contract and TUI unchanged.
