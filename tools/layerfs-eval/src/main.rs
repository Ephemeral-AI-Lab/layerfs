use layerfs_sdk::{BranchId, BranchStore, LayerStore, StackStore};
use std::sync::Arc;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (branch, branch_id) = match arguments.as_slice() {
        [mode, branch, layer, id] if mode == "check-direct" => {
            let layer = Arc::new(LayerStore::connect(layer)?);
            (
                BranchStore::connect(branch, layer)?,
                id.parse::<BranchId>()?,
            )
        }
        [mode, branch, stack, layer, id] if mode == "check-stacked" => {
            let layer = Arc::new(LayerStore::connect(layer)?);
            let stack = Arc::new(StackStore::connect(stack, layer)?);
            (
                BranchStore::connect(branch, stack)?,
                id.parse::<BranchId>()?,
            )
        }
        _ => {
            return Err(
                "usage: layerfs-eval check-direct <branch-db> <layer-db> <branch-id> | \
                 check-stacked <branch-db> <stack-db> <layer-db> <branch-id>"
                    .into(),
            )
        }
    };
    let (record, root) = branch.branch_snapshot(branch_id)?;
    println!("{} {}", record.head_commit_id, root);
    Ok(())
}
