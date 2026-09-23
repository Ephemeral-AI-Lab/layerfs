//! Host-direct benchmark driver for one real public SDK project Init.
use layerfs_sdk::{Error, Host};
use std::{fmt::Write as _, path::Path, time::Instant};

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("string write");
    }
    text
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("source store history project-name required".into());
    }
    let host = Host::create(
        Path::new(&args[2]),
        Path::new(&args[3]),
        b"layerfs-bench-pro",
        &std::env::var("LAYERFS_PRIVATE_KEY")?,
        &std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?,
    )?;
    let client = host.client();
    let start = Instant::now();
    let result = client.init_project(&args[4], Path::new(&args[1]));
    let operation_ns = start.elapsed().as_nanos();
    match result {
        Ok(project) => {
            println!("{{\"status\":\"COMPLETE\",\"operation_ns\":{operation_ns},\"root\":\"{}\",\"stack_body\":\"{}\",\"project_id\":\"{}\",\"genesis_layer\":\"{}\",\"root_serial\":{}}}",
                hex(&project.root), hex(&project.id[1..]), hex(&project.id),
                hex(&project.genesis_layer), project.root_serial);
            Ok(())
        }
        Err(Error::Backend(failure)) => {
            println!("{{\"status\":\"FAIL\",\"operation_ns\":{operation_ns},\"code\":\"{:?}\",\"unknown\":{},\"cleanup\":\"{:?}\"}}",
                failure.code, failure.unknown, failure.cleanup);
            Err("public SDK Init failed".into())
        }
        Err(Error::Unsupported) => Err("public SDK Init is unsupported".into()),
    }
}
