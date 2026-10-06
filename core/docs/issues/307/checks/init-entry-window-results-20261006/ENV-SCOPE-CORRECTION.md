# Environment scope clarification

The generic run_command.py `target` field in Docker command receipts records
its outer host launcher environment. The actual Docker Cargo target is explicitly
`/work/core/target/cluster2-linux` in the retained command, with Cargo home
`/work/core/target/cluster2-linux-cargo`. Those are the paths used by the container.
The raw receipts retain their original bytes; this clarification does not relabel
any result or turn Linux compilation into global-provider runtime proof.

All candidate measurement builds instead use the new managed worktree's own
`core/target`, as pinned by each runner build command. Setup copies are ordinary
closed-input byte copies, not shared writable targets or a cold-state assertion.
