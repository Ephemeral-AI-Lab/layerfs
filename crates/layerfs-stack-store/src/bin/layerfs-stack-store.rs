use layerfs_stack_store::{RemoteEndpoint, StackStore};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let Some(path) = arguments.next() else {
        return Err(usage().into());
    };
    let Some(parent) = arguments.next() else {
        return Err(usage().into());
    };
    let Some(listen) = arguments.next() else {
        return Err(usage().into());
    };
    if arguments.next().is_some() {
        return Err(usage().into());
    }
    let parent = Arc::new(RemoteEndpoint::connect(parent.parse::<SocketAddr>()?)?);
    let store = StackStore::connect(path, parent)?;
    let listener = TcpListener::bind(listen)?;
    for stream in listener.incoming() {
        let store = store.clone();
        let mut stream = stream?;
        std::thread::spawn(move || {
            let Ok(mut output) = stream.try_clone() else {
                return;
            };
            while layerfs_stack_store::serve_once(&store, &mut stream, &mut output).is_ok() {}
        });
    }
    Ok(())
}

fn usage() -> &'static str {
    "usage: layerfs-stack-store <database-path> <parent-address> <listen-address>"
}
