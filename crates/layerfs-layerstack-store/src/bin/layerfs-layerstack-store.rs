fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(path) = arguments.next() else {
        return Err("usage: layerfs-layerstack-store <database-path>".into());
    };
    if arguments.next().is_some() {
        return Err("usage: layerfs-layerstack-store <database-path>".into());
    }
    let store = layerfs_layerstack_store::LayerStackStore::connect(path)?;
    println!("{}", store.store_id());
    Ok(())
}
