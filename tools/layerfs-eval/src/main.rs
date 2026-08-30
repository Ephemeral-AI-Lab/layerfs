use layerfs_sdk::{BranchId, BranchStore, LayerStackStore};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (branch, branch_id) = match arguments.as_slice() {
        [mode, branch, layerstack, id] if mode == "check" => {
            let layerstack = LayerStackStore::connect(layerstack)?;
            (
                BranchStore::connect(branch, layerstack.store_id())?,
                id.parse::<BranchId>()?,
            )
        }
        _ => {
            return Err("usage: layerfs-eval check <branch-db> <layerstack-db> <branch-id>".into())
        }
    };
    let record = branch.branch(branch_id)?.ok_or("Branch not found")?;
    let root = branch.branch_root(branch_id)?;
    println!(
        "{:?} {} complete={}",
        record.head_commit_id,
        root,
        branch.root_complete(root)?
    );
    Ok(())
}
