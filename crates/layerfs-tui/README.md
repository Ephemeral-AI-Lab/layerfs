# LayerFS V2 TUI mock

This isolated worktree builds the TUI against the concrete
`layerfs_cli::CliSession` seam. Store state lives in two real SQLite databases
using the LayerFS V2 schema-version, application-ID, table, column, index, and
immutable-parent contract. Store transfer/topology operations remain mock
logic. Workspace directories, Bash execution, final-tree capture, Commit
objects, and Branch head updates are real. Workspaces, output, progress, and
receipts remain runtime-only and are never added to either Store schema.

The seeded fixture models one LayerStackStore, one BranchStore, three named
LayerStacks, Reference and Replica pull boundaries, remote and local Branches,
a depth-four rollout tree, Commit-anchored Workspaces, operations, receipts,
storage, and the three Diff forms.

Run the interactive TUI:

    cargo run -p layerfs-tui

Run it against an inspectable context profile:

    cargo run -p layerfs-tui -- --context /tmp/layerfs-demo/context

Use the TUI command line to create the two Stores, select the pair, and then
initialize a LayerStack:

    db create layerstack /tmp/layerfs-demo/layerstack.sqlite
    db create branch /tmp/layerfs-demo/branch.sqlite --parent /tmp/layerfs-demo/layerstack.sqlite
    context use --layerstack /tmp/layerfs-demo/layerstack.sqlite --branch /tmp/layerfs-demo/branch.sqlite
    layerstack init --name npm-demo --empty

Render a deterministic headless screen:

    cargo run -p layerfs-tui -- --dump topology --width 200 --height 60 --no-color

Run one refined mock CLI operation:

    cargo run -p layerfs-cli -- branch pull B-main --through C-B-main-42 --replica

The mock has no socket, HTTP service, FUSE mount, or backend compatibility
adapter. Replacing its mock semantic executor with the V2 SDK keeps the
frontend contract and TUI unchanged; the database topology and schema are
already real.
