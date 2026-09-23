//! Host-direct benchmark driver for one real public SDK project Init.
use layerfs_bridge::adapters::native::{connection::VerifiedPeer, pipe::key};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_sdk::{Client, Error};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{fmt::Write as _, path::Path, sync::Arc, time::Instant};

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
    let private = key(&std::env::var("LAYERFS_PRIVATE_KEY")?)?;
    let cursor_key = key(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?;
    let peer = VerifiedPeer::from_private(&private)?;
    let store = Timing::disabled("create", |scope| {
        Store::create(&args[2], Store::default_policy(), scope.child("store"))
    })
    .0?;
    let history: Arc<dyn HistoryCatalog> = Arc::new(sqlite::create(
        Path::new(&args[3]),
        &HistoryCatalogConfig {
            binding_key: b"layerfs-bench-pro".to_vec(),
            incarnation: 1,
            cursor_key,
        },
    )?);
    let service = Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            history: Some(history),
            grants: vec![Grant {
                public_key: *peer.public_key(),
                operations: 127,
                expires_unix: u64::MAX,
            }],
        }],
        OperationRecorder::disabled(),
    )?;
    let client = Client::new(&service, &peer, 1);
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
