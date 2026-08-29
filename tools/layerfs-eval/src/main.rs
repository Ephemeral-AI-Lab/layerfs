use layerfs_sdk::{BranchId, Direct, Stacked};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [mode, branch, layer, id, output] if mode == "materialize-direct" => Direct::open(branch, layer)?
            .materialize(id.parse::<BranchId>()?, Path::new(output))?,
        [mode, branch, stack, layer, id, output] if mode == "materialize-stacked" => {
            Stacked::open(branch, stack, layer)?
                .materialize(id.parse::<BranchId>()?, Path::new(output))?
        }
        _ => return Err("usage: layerfs-eval materialize-direct <branch-db> <layer-db> <branch-id> <empty-output> | materialize-stacked <branch-db> <stack-db> <layer-db> <branch-id> <empty-output>".into()),
    }
    Ok(())
}
