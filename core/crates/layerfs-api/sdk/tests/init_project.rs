//! Public SDK import proofs with independent Store/history reads.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_history::{sqlite, HistoryCatalog, LayerStackId};
use layerfs_sdk::{Error, HistoryMode, Project, ProjectApi, Server, ServerConfig};
use layerfs_server::Service;
use layerfs_telemetry::runtime::Runtime;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

const BINDING: &[u8] = b"issue236-sdk-proof";
const CURSOR_KEY: [u8; 32] = [0x36; 32];
static NEXT_PROOF_ROOT: AtomicUsize = AtomicUsize::new(0);

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap()
        .to_path_buf()
}

fn proof_root() -> PathBuf {
    let parent = repo().join("core/target/issue236-proof");
    std::fs::create_dir_all(&parent).unwrap();
    let root = parent.join(format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_PROOF_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    root
}

fn server(store_path: &Path, history_path: &Path, create: bool) -> Server {
    let config = ServerConfig {
        store_path: store_path.to_path_buf(),
        history_path: history_path.to_path_buf(),
        binding_key: BINDING.to_vec(),
        incarnation: 1,
        cursor_key: CURSOR_KEY,
        history: if create {
            HistoryMode::Create
        } else {
            HistoryMode::OpenWritable
        },
        service_host: "host.docker.internal".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    };
    if create {
        Server::create(config).unwrap()
    } else {
        Server::open(config).unwrap()
    }
}

fn call(
    service: &Service,
    peer: &VerifiedPeer,
    id: u64,
    operation: Operation,
    bytes: u64,
) -> (Response, Vec<u8>) {
    let request = Request {
        id,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: 60_000,
        response_bytes: bytes,
        operation,
    };
    let mut output = Vec::new();
    let result = service
        .handle(peer, &request, &mut Cursor::new(&[][..]), &mut output)
        .0
        .unwrap();
    (result, output)
}

fn fixture(case: &str, root: &Path) -> (PathBuf, PathBuf, String) {
    let module = repo().join("core/benchmark/fs-bench-pro");
    let script = "import sys; from pathlib import Path; sys.path.insert(0,sys.argv[1]); from families import init_namespace as f; f.PROFILE='core-sdk-init-fixture-v1'; f.ROUTE='host-direct-sdk-v1'; r=f.prepare(f.CASES[sys.argv[2]],Path(sys.argv[3])); print(r['source']); print(r['manifest']); print(r['manifest_sha256']); print(r['reused'])";
    let result = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(module)
        .arg(case)
        .arg(root.join("prepared"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let lines: Vec<_> = std::str::from_utf8(&result.stdout)
        .unwrap()
        .lines()
        .collect();
    assert_eq!(lines[3], "False", "proof must use a fresh sealed fixture");
    (
        PathBuf::from(lines[0]),
        PathBuf::from(lines[1]),
        lines[2].to_string(),
    )
}

fn verify_inventory(
    service: &Service,
    peer: &VerifiedPeer,
    project: &Project,
    source_manifest: &Path,
    expected_digest: &str,
) -> (usize, u64) {
    let manifest = std::fs::read(source_manifest).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&manifest)), expected_digest);
    let mut directories: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut files = 0;
    let mut total = 0;
    let mut request_id = 100;
    for line in std::str::from_utf8(&manifest).unwrap().lines() {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 6);
        let (path, kind) = (columns[0], columns[1]);
        let path_bytes = if path == "." {
            Vec::new()
        } else {
            path.as_bytes().to_vec()
        };
        let (result, _) = call(
            service,
            peer,
            request_id,
            Operation::Inspect {
                root: project.root,
                query: Inspect::Attributes { path: path_bytes },
            },
            0,
        );
        request_id += 1;
        let Response::Attributes {
            kind: actual_kind,
            mode,
            mtime,
            nanoseconds,
            size,
            content,
            serial,
            ..
        } = result
        else {
            panic!("missing attributes: {path}")
        };
        assert_eq!(actual_kind, if kind == "f" { 1 } else { 2 }, "{path}");
        assert_eq!(mode, columns[2].parse::<u32>().unwrap(), "{path}");
        let mtime_ns = columns[3].parse::<i64>().unwrap();
        assert_eq!(
            (mtime, nanoseconds),
            (
                mtime_ns.div_euclid(1_000_000_000),
                mtime_ns.rem_euclid(1_000_000_000) as u32
            ),
            "{path}"
        );
        assert_eq!(size, columns[4].parse::<u64>().unwrap(), "{path}");
        if path == "." {
            assert_eq!(serial, project.root_serial);
        }
        if kind == "f" {
            let (_, bytes) = call(
                service,
                peer,
                request_id,
                Operation::ReadFile {
                    root: content,
                    start: 0,
                    end: size,
                },
                size,
            );
            request_id += 1;
            assert_eq!(bytes.len() as u64, size, "{path}");
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                columns[5],
                "{path}"
            );
            files += 1;
            total += size;
        } else {
            directories.insert(path.to_string(), Vec::new());
        }
        if path != "." {
            let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
            directories
                .entry(parent.to_string())
                .or_default()
                .push(name.to_string());
        }
    }
    for (directory, mut expected) in directories {
        expected.sort();
        let mut listed = Vec::new();
        let mut after = Vec::new();
        loop {
            let (result, _) = call(
                service,
                peer,
                request_id,
                Operation::Inspect {
                    root: project.root,
                    query: Inspect::List {
                        path: if directory == "." {
                            Vec::new()
                        } else {
                            directory.as_bytes().to_vec()
                        },
                        after,
                        entries: 128,
                        bytes: 16384,
                    },
                },
                0,
            );
            request_id += 1;
            let Response::List {
                entries,
                continuation,
            } = result
            else {
                panic!("missing list: {directory}")
            };
            listed.extend(
                entries
                    .into_iter()
                    .map(|(name, _)| String::from_utf8(name).unwrap()),
            );
            match continuation {
                Some(next) => after = next,
                None => break,
            }
        }
        assert_eq!(listed, expected, "{directory}");
    }
    (files, total)
}

#[test]
fn public_sdk_imports_fresh_100_and_1000_file_fixtures() {
    let root = proof_root();
    for (case, expected_files, expected_bytes) in [
        ("namespace-100-compact-v3", 100, 5_000_000),
        ("namespace-1000-compact-v3", 1_000, 20_000_000),
    ] {
        let run = root.join(case);
        std::fs::create_dir(&run).unwrap();
        let (source, manifest, digest) = fixture(case, &run);
        let store_path = run.join("store.sqlite");
        let history_path = run.join("history.sqlite");
        let writer = server(&store_path, &history_path, true);
        let project = ProjectApi::new(&writer).init(case, &source).unwrap();
        assert_eq!(project.id[0], 0x31);
        assert_eq!(project.genesis_layer[0], 0x32);
        drop(writer);

        let history = sqlite::open_read_only(&history_path, BINDING, CURSOR_KEY).unwrap();
        let stack = history
            .layer_stack(LayerStackId::from_bytes(project.id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(stack.head_layer.to_bytes(), project.genesis_layer);
        let layer = history.layer(stack.head_layer).unwrap().unwrap();
        assert_eq!(*layer.root.as_bytes(), project.root);
        drop(history);
        let reader = server(&store_path, &history_path, false);
        let (files, bytes) = verify_inventory(
            reader.service(),
            &reader.peer().unwrap(),
            &project,
            &manifest,
            &digest,
        );
        assert_eq!((files, bytes), (expected_files, expected_bytes));
        let tree = std::env::var("LAYERFS_PROOF_TREE").unwrap_or_else(|_| "unbound".to_string());
        let receipt = format!("route=host-direct-sdk-v1\nfixture_profile=core-sdk-init-fixture-v1\ncase={case}\nsource_tree={tree}\nmanifest_sha256={digest}\nfiles={files}\nbytes={bytes}\nproject_id={}\ngenesis_layer={}\nroot={}\nroot_serial={}\nstatus=PASS\n",
            hex(&project.id), hex(&project.genesis_layer), hex(&project.root), project.root_serial);
        std::fs::write(run.join("proof.txt"), receipt).unwrap();
        println!(
            "proof={} tree={tree} manifest={digest} files={files} bytes={bytes}",
            run.display()
        );
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}

#[test]
fn host_setup_exposes_the_same_sdk_init() {
    let root = proof_root();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("one"), b"A").unwrap();
    let server = server(
        &root.join("store.sqlite"),
        &root.join("history.sqlite"),
        true,
    );
    let project = ProjectApi::new(&server)
        .init("host-project", &source)
        .unwrap();
    assert_eq!((project.id[0], project.genesis_layer[0]), (0x31, 0x32));
}

#[test]
fn source_refusals_duplicates_and_concurrent_bindings() {
    let root = proof_root();
    let a = root.join("a");
    let b = root.join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("only-a"), b"A").unwrap();
    std::fs::write(b.join("only-b"), b"B").unwrap();
    let link = root.join("link");
    std::os::unix::fs::symlink(&a, &link).unwrap();
    let server = server(
        &root.join("store.sqlite"),
        &root.join("history.sqlite"),
        true,
    );
    let service = server.service();
    let peer = server.peer().unwrap();
    let projects = ProjectApi::new(&server);
    for invalid in [&link, &a.join("only-a"), &root.join("missing")] {
        assert!(matches!(
            projects.init("bad", invalid),
            Err(Error::Backend(Failure {
                code: Code::InvalidInput,
                ..
            }))
        ));
    }
    let (one, two) = std::thread::scope(|scope| {
        let left = scope.spawn(|| projects.init("left", &a).unwrap());
        let right = scope.spawn(|| projects.init("right", &b).unwrap());
        (left.join().unwrap(), right.join().unwrap())
    });
    assert!(matches!(
        projects.init("left", &a),
        Err(Error::Backend(Failure {
            code: Code::Integrity,
            unknown: false,
            ..
        }))
    ));
    for (project, present, absent) in [
        (&one, b"only-a".as_slice(), b"only-b".as_slice()),
        (&two, b"only-b".as_slice(), b"only-a".as_slice()),
    ] {
        call(
            service,
            &peer,
            1,
            Operation::Inspect {
                root: project.root,
                query: Inspect::Attributes {
                    path: present.to_vec(),
                },
            },
            0,
        );
        let request = Request {
            id: 2,
            generation: 1,
            store: 1,
            profile: 1,
            deadline_ms: 60_000,
            response_bytes: 0,
            operation: Operation::Inspect {
                root: project.root,
                query: Inspect::Attributes {
                    path: absent.to_vec(),
                },
            },
        };
        let missing = service
            .handle(&peer, &request, &mut Cursor::new(&[][..]), &mut Vec::new())
            .0
            .unwrap_err();
        assert_eq!(missing.code, Code::PathNotFound);
    }
}
