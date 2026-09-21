//! Direct semantic parity, using the same authorized handler and explicit fixture.
use layerfs_bridge::{
    adapters::native::{connection::VerifiedPeer, pipe::key},
    contract::*,
};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::io::Cursor;
fn req(id: u64, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation,
    }
}
fn hex(root: &Root) -> String {
    layerfs_content::ObjectId::from_bytes(root)
        .unwrap()
        .to_string()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("store, prepared root, scope and metadata required".into());
    }
    let peer = VerifiedPeer::from_private(&key(&std::env::var("LAYERFS_PRIVATE_KEY")?)?)?;
    let store = Timing::disabled("open", |s| Store::open(&args[1], s.child("open"))).0?;
    let service = Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            history: None,
            grants: vec![Grant {
                public_key: *peer.public_key(),
                operations: 31,
                expires_unix: u64::MAX,
            }],
        }],
        OperationRecorder::disabled(),
    )?;
    let body: Vec<u8> = (0..500000).map(|n| (n % 251) as u8).collect();
    let saved = service
        .handle(
            &peer,
            &req(
                1,
                Operation::ConstructFile {
                    length: body.len() as u64,
                },
            ),
            &mut Cursor::new(&body),
            &mut std::io::sink(),
        )
        .0?;
    let Response::Saved {
        root,
        length,
        inserted,
        reused,
    } = saved
    else {
        return Err("expected saved file".into());
    };
    let mut bytes = Vec::new();
    service
        .handle(
            &peer,
            &req(
                2,
                Operation::ReadFile {
                    root,
                    start: 0,
                    end: length,
                },
            ),
            &mut std::io::empty(),
            &mut bytes,
        )
        .0?;
    assert_eq!(bytes, body);
    let saved = service
        .handle(
            &peer,
            &req(
                3,
                Operation::EditFile {
                    root,
                    base_length: length,
                    edits: vec![Edit {
                        start: 10,
                        end: 15,
                        replacement: 3,
                    }],
                },
            ),
            &mut Cursor::new(b"new"),
            &mut std::io::sink(),
        )
        .0?;
    let Response::Saved {
        root: edited,
        inserted: edit_inserted,
        reused: edit_reused,
        ..
    } = saved
    else {
        return Err("expected edited file".into());
    };
    let update = Operation::UpdatePreparedFilesystem {
        directory_metadata: Vec::new(),
        new_directories: Vec::new(),
        base: key(&args[2])?,
        scope: key(&args[3])?,
        root_serial: 1,
        directories: vec![],
        inodes: vec![InodeChange {
            serial: 2,
            kind: 1,
            content: edited,
            metadata: key(&args[4])?,
        }],
    };
    let saved = service
        .handle(
            &peer,
            &req(6, update),
            &mut std::io::empty(),
            &mut std::io::sink(),
        )
        .0?;
    let Response::FilesystemSaved {
        root: tree,
        inserted: tree_inserted,
        reused: tree_reused,
    } = saved
    else {
        return Err("expected filesystem".into());
    };
    let stat = service
        .handle(
            &peer,
            &req(
                7,
                Operation::Inspect {
                    root: tree,
                    query: Inspect::Stat {
                        path: b"f".to_vec(),
                    },
                },
            ),
            &mut std::io::empty(),
            &mut std::io::sink(),
        )
        .0?;
    assert!(matches!(stat,Response::Stat{content,..}if content==edited));
    println!("{{\"file\":\"{}\",\"edited\":\"{}\",\"tree\":\"{}\",\"file_counts\":[{inserted},{reused}],\"edit_counts\":[{edit_inserted},{edit_reused}],\"tree_counts\":[{tree_inserted},{tree_reused}]}}",hex(&root),hex(&edited),hex(&tree));
    Ok(())
}
