use crate::{
    construction,
    metadata::{self, Authority, LocatorDb},
    minio::{hex, Minio},
    objects::{Consumer, Reader},
};
use layerfs_bridge::{
    adapters::native::connection::{Peer, VerifiedPeer},
    contract::CommitOutcomeWire,
};
use layerfs_content::{
    filesystem::{profile_id, scope_for_seed, FilesystemRootId},
    FilesystemRead, LogicalPath,
};
use layerfs_history::{
    catalog::{HistoryCatalog, HistoryCatalogConfig},
    identity::{BranchId, HistoryName, LayerStackId},
    records::*,
    sqlite,
};
use layerfs_sandbox::{random, OwnerConfig, SandboxOwner};
use layerfs_sdk::WorkspaceApi;
use layerfs_telemetry::{runtime::Runtime, timer::Timing};
use std::{
    fs::File,
    io::Write,
    net::TcpListener,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
fn root(outcome: &CommitOutcomeWire) -> [u8; 32] {
    match outcome {
        CommitOutcomeWire::Committed(c) => c.root,
        CommitOutcomeWire::UpToDate { root, .. } => *root,
    }
}
pub fn run(out: &Path, config: &Path, image: &str) -> Result<(), String> {
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(config).map_err(|e| e.to_string())?;
    let fields: Vec<_> = text.lines().collect();
    if fields.len() != 4 {
        return Err("provider config fields".into());
    }
    let s3 = Minio {
        authority: fields[0].into(),
        bucket: fields[1].into(),
        access: fields[2].into(),
        secret: fields[3].into(),
    };
    s3.create_bucket()?;
    let db = Arc::new(LocatorDb::create(&out.join("objects.sqlite"), s3.clone())?);
    let reader = Reader {
        s3: s3.clone(),
        locators: db.clone(),
    };
    let mut consumer = Consumer::new(reader.clone())?;
    let start = Instant::now();
    let genesis = construction::genesis(&reader, &mut consumer)?;
    let genesis_ms = start.elapsed().as_secs_f64() * 1000.;
    let history = sqlite::create(
        &out.join("history.sqlite"),
        &HistoryCatalogConfig {
            binding_key: format!("phase6-live-{}", s3.bucket).into_bytes(),
            incarnation: 1,
            cursor_key: random::<32>().map_err(|e| e.to_string())?,
        },
    )
    .map_err(|e| e.to_string())?;
    let stack = LayerStackId::from_authority(random::<16>().map_err(|e| e.to_string())?);
    let branch = BranchId::from_authority(random::<16>().map_err(|e| e.to_string())?);
    let layer = history
        .initialize_layerstack(&StackInitialization {
            stack,
            name: HistoryName::new("phase6-live").map_err(|e| e.to_string())?,
            scope: scope_for_seed([6; 32]).object(),
            profile: profile_id(),
            genesis_root: genesis.root.0,
        })
        .map_err(|e| e.to_string())?;
    history
        .fork(&ForkRequest {
            stack,
            branch,
            name: HistoryName::new("main").map_err(|e| e.to_string())?,
            source: ForkSource::Layer(layer.head_layer),
        })
        .map_err(|e| e.to_string())?;
    let root_reservation = history
        .reserve_inodes(&ReserveRequest {
            scope: scope_for_seed([6; 32]).object(),
            count: 1,
        })
        .map_err(|e| e.to_string())?;
    if root_reservation.start != 1 {
        return Err("root serial reservation".into());
    }
    let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let service_private = random::<32>().map_err(|e| e.to_string())?;
    let server_private = random::<32>().map_err(|e| e.to_string())?;
    let server_public = *VerifiedPeer::from_private(&server_private)
        .map_err(|e| e.to_string())?
        .public_key();
    let peer = Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&service_private)
            .map_err(|e| e.to_string())?
            .public_key(),
        expires_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            + 3600,
    };
    let mut daemon_s3 = s3;
    let (_, minio_port) = daemon_s3
        .authority
        .rsplit_once(':')
        .ok_or("MinIO address")?;
    daemon_s3.authority = format!("host.docker.internal:{minio_port}");
    let authority = Arc::new(Authority {
        locators: db,
        history,
        branch,
        daemon_s3,
        bootstrap_taken: AtomicBool::new(false),
    });
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let a = authority.clone();
        let s = stop.clone();
        std::thread::spawn(move || metadata::serve(listener, a, server_private, peer, s))
    };
    let owner = SandboxOwner::new(OwnerConfig {
        service_endpoint: format!("host.docker.internal:{port}"),
        service_selector: 1,
        service_private,
        service_public: server_public,
        control_private: random::<32>().map_err(|e| e.to_string())?,
        store: 1,
        telemetry_run: None,
        telemetry: Runtime::disabled(),
    })
    .map_err(|e| e.to_string())?;
    let api = WorkspaceApi::new(&owner);
    let total = Instant::now();
    let sandbox = owner
        .create(image, "phase6-live-smoke")
        .map_err(|e| format!("{e:?}"))?;
    let project = layerfs_sdk::Project {
        id: stack.to_bytes(),
        genesis_layer: layer.head_layer.to_bytes(),
        root: *genesis.root.0.as_bytes(),
        root_serial: 1,
    };
    let mount = api
        .mount(sandbox, &project, branch.to_bytes(), None)
        .map_err(|e| e.to_string())?;
    let first="dd if=/dev/zero of=data bs=4096 count=1 2>/dev/null && printf phase6 | dd of=data bs=1 seek=17 conv=notrunc 2>/dev/null && test \"$(wc -c < data)\" -eq 4096";
    let second="printf SECOND | dd of=data bs=1 seek=100 conv=notrunc 2>/dev/null && test \"$(wc -c < data)\" -eq 4096";
    let mut rows = Vec::new();
    let mut roots = Vec::new();
    for (name, command) in [("create", first), ("overwrite", second)] {
        let start = Instant::now();
        let exec = api.exec(&mount.id, command).map_err(|e| e.to_string())?;
        let exec_ms = start.elapsed().as_secs_f64() * 1000.;
        if exec.exit_status != Some(0) || exec.stdout_truncated || exec.stderr_truncated {
            return Err(format!(
                "command {name} failed: {:?} {}",
                exec.exit_status,
                String::from_utf8_lossy(&exec.stderr)
            ));
        }
        let start = Instant::now();
        let commit = api.commit(&mount.id).map_err(|e| e.to_string())?;
        let commit_ms = start.elapsed().as_secs_f64() * 1000.;
        let id = root(&commit.outcome);
        roots.push(id);
        rows.push(format!("{{\"case\":\"{name}\",\"exec_ms\":{exec_ms},\"commit_ms\":{commit_ms},\"generation\":{},\"root\":\"{}\"}}",commit.generation,hex(&id)));
    }
    api.unmount(&mount.id).map_err(|e| e.to_string())?;
    let mut logs = File::create(out.join("daemon.stderr")).map_err(|e| e.to_string())?;
    owner
        .delete_with_logs(sandbox, &mut logs)
        .0
        .map_err(|e| format!("{e:?}"))?;
    let command_ms = total.elapsed().as_secs_f64() * 1000.;
    stop.store(true, Ordering::SeqCst);
    worker.join().map_err(|_| "metadata owner panic")??;
    let proof = Instant::now();
    let mut expected = vec![0; 4096];
    expected[17..23].copy_from_slice(b"phase6");
    for (index, id) in roots.iter().enumerate() {
        if index == 1 {
            expected[100..106].copy_from_slice(b"SECOND")
        }
        let mut fs = FilesystemRead::new(
            &reader,
            FilesystemRootId(layerfs_content::ObjectId::from_bytes(id).map_err(|e| e.to_string())?),
        )
        .map_err(|e| e.to_string())?;
        let resolved = fs
            .resolve(&LogicalPath::new("data").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let (result, _) = Timing::disabled("independent semantic readback", |t| {
            layerfs_content::read_all_bounded(
                &reader,
                resolved.value.content_root,
                4096,
                &mut bytes,
                t.child("read"),
            )
        });
        result.map_err(|e| e.to_string())?;
        if bytes != expected {
            return Err(format!("independent bytes mismatch at publication {index}"));
        }
        let meta = fs
            .read_portable(&LogicalPath::new("data").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if meta.mode != 0o644 {
            return Err("portable mode oracle".into());
        }
        std::fs::write(out.join(format!("publication-{index}.data")), &bytes)
            .map_err(|e| e.to_string())?;
    }
    let snapshot = authority.snapshot()?;
    if snapshot.root != roots[1] || roots[0] == roots[1] {
        return Err("successor head oracle".into());
    }
    let head = authority
        .history
        .commit(
            layerfs_history::identity::CommitId::from_bytes(snapshot.head.ok_or("head absent")?)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?
        .ok_or("head missing")?;
    let parent = authority
        .history
        .commit(head.parent.ok_or("parent absent")?)
        .map_err(|e| e.to_string())?
        .ok_or("parent missing")?;
    if parent.root.as_bytes() != &roots[0] {
        return Err("history parent oracle".into());
    }
    let proof_ms = proof.elapsed().as_secs_f64() * 1000.;
    let mut file = File::create(out.join("receipt.json")).map_err(|e| e.to_string())?;
    writeln!(file,"{{\"schema\":1,\"kind\":\"correctness diagnostic\",\"cache_status\":\"INELIGIBLE: OS/page cache unknown\",\"sqlite_version\":\"{}\",\"image\":\"{}\",\"genesis_ms\":{genesis_ms},\"complete_command_ms\":{command_ms},\"proof_ms\":{proof_ms},\"rows\":[{}],\"semantic_proof\":\"PASS\",\"canonical_reference\":\"NOT_RUN\",\"physical_resources\":\"NOT_RUN\",\"cleanup\":\"PASS\"}}",rusqlite::version(),image,rows.join(",")).map_err(|e|e.to_string())?;
    println!("first real SDK/FUSE/SQL/C1/C2/MinIO/C5 correctness path PASS; complete_command_ms={command_ms:.3} proof_ms={proof_ms:.3}; speed INELIGIBLE");
    Ok(())
}
