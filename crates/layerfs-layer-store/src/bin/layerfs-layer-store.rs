use std::net::TcpListener;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(path) = arguments.next() else {
        return Err("usage: layerfs-layer-store <database-path> <listen-address>".into());
    };
    let Some(address) = arguments.next() else {
        return Err("usage: layerfs-layer-store <database-path> <listen-address>".into());
    };
    if arguments.next().is_some() {
        return Err("usage: layerfs-layer-store <database-path> <listen-address>".into());
    }
    let store = layerfs_layer_store::LayerStore::open(path)?;
    let listener = TcpListener::bind(address.to_string_lossy().as_ref())?;
    for stream in listener.incoming() {
        let store = store.clone();
        let mut stream = stream?;
        std::thread::spawn(move || {
            let Ok(mut output) = stream.try_clone() else {
                return;
            };
            while layerfs_layer_store::serve_once(&store, &mut stream, &mut output).is_ok() {}
        });
    }
    Ok(())
}
