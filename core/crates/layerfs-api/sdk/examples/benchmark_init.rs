//! Host-direct benchmark driver for one real public SDK project Init.
use layerfs_sdk::{Error, HistoryMode, ProjectApi, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::{fmt::Write as _, path::Path, time::Instant};

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("string write");
    }
    text
}

fn key(text: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if text.len() != 64 {
        return Err("cursor key must be 32 hex bytes".into());
    }
    let mut value = [0u8; 32];
    for (index, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    Ok(value)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("source store history project-name required".into());
    }
    let server = Server::create(ServerConfig {
        store_path: Path::new(&args[2]).to_path_buf(),
        history_path: Path::new(&args[3]).to_path_buf(),
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key: key(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?,
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })?;
    let start = Instant::now();
    let result = ProjectApi::new(&server).init(&args[4], Path::new(&args[1]));
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
