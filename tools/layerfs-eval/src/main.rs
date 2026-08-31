use layerfs_sdk::{BranchId, LayerStackStore, ObjectSource};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (store, branch_id) = match arguments.as_slice() {
        [mode, store, id] if mode == "check" => {
            (LayerStackStore::connect(store)?, id.parse::<BranchId>()?)
        }
        _ => return Err("usage: layerfs-eval check <store-db> <branch-id>".into()),
    };
    let pinned = store.pin_branch(branch_id)?;
    pinned.reader.read_object(pinned.root)?;
    println!("{:?} {}", pinned.branch.head_commit_id, pinned.root);
    Ok(())
}
