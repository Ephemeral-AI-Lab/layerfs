//! Real direct Service legacy-profile fixtures; no process hardcap is required.
//! Native executable eligibility is separately qualified by engine_startup.
#![allow(dead_code)]

use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    cell::Cell,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

pub const ORIGINAL: &[u8] = b"prepared binding original bytes";
pub type Directory = (u64, Vec<(Vec<u8>, Option<u64>)>);

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Fixture {
    temp: Temp,
    pub service: Service,
    pub peer: VerifiedPeer,
    pub branch: [u8; 17],
    pub base_layer: [u8; 33],
    pub root: Root,
    pub scope: Root,
    pub root_serial: u64,
    pub file_serial: u64,
    pub file_content: Root,
    pub file_metadata: Root,
    next: Cell<u64>,
}

impl Fixture {
    pub fn new(label: &str) -> Self {
        Self::with_directories(label, 0)
    }

    /// Real immutable imported directory fixtures, prepared before the operation.
    pub fn with_directories(label: &str, directories: usize) -> Self {
        Self::with_shape(label, directories, false, 16 * 1024 * 1024)
    }

    pub fn with_scratch(label: &str, scratch_bytes: u64) -> Self {
        Self::with_shape(label, 0, false, scratch_bytes)
    }

    pub fn with_chain(label: &str, inner_directories: usize) -> Self {
        Self::with_shape(label, inner_directories, true, 16 * 1024 * 1024)
    }

    fn with_shape(label: &str, directories: usize, chain: bool, scratch_bytes: u64) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-prepared-bindings-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let store = Timing::disabled("create", |scope| {
            Store::create(
                path.join("store.sqlite"),
                Store::default_policy(),
                scope.child("create"),
            )
        })
        .0
        .unwrap();
        assert_eq!(store.max_concurrent_writes().unwrap(), 2);
        let catalog: Arc<dyn HistoryCatalog> = Arc::new(
            sqlite::create(
                &path.join("history.sqlite"),
                &HistoryCatalogConfig {
                    cursor_key: [71; 32],
                    binding_key: b"binding-proof-authority".to_vec(),
                    incarnation: 1,
                },
            )
            .unwrap(),
        );
        let peer = VerifiedPeer::from_private(&[7; 32]).unwrap();
        let mut service = Service::with_construction_scratch(
            vec![StoreAccess {
                id: 1,
                store,
                history: Some(catalog),
                grants: vec![Grant {
                    public_key: *peer.public_key(),
                    operations: 0xff,
                    expires_unix: u64::MAX,
                }],
            }],
            OperationRecorder::disabled(),
            scratch_bytes,
        )
        .unwrap();
        let source = path.join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("a"), ORIGINAL).unwrap();
        let mut nested = source.clone();
        for index in 0..directories {
            if chain {
                nested.push("d");
                std::fs::create_dir(&nested).unwrap();
            } else {
                std::fs::create_dir(source.join(format!("d{index:04}"))).unwrap();
            }
        }
        service.set_import_root(&source).unwrap();
        let created = dispatch(
            &service,
            &peer,
            1,
            Operation::HistoryCommand(HistoryCommand::ImportNativeDirectory {
                stack: [51; 16],
                name: b"main".to_vec(),
                scope_seed: [42; 32],
            }),
            &mut io::empty(),
        )
        .unwrap();
        let Response::History(created) = created else {
            panic!("history")
        };
        let HistoryResult::StackCreated(created) = *created else {
            panic!("stack")
        };
        let forked = dispatch(
            &service,
            &peer,
            2,
            Operation::HistoryCommand(HistoryCommand::Fork {
                stack: created.stack.stack,
                branch: [61; 16],
                name: b"work".to_vec(),
                source: HistoryForkSource::Layer(created.stack.head_layer),
            }),
            &mut io::empty(),
        )
        .unwrap();
        let Response::History(forked) = forked else {
            panic!("history")
        };
        let HistoryResult::BranchSnapshot(forked) = *forked else {
            panic!("branch")
        };
        let entry = dispatch(
            &service,
            &peer,
            3,
            Operation::Inspect {
                root: forked.effective_root,
                query: Inspect::Stat {
                    path: b"a".to_vec(),
                },
            },
            &mut io::empty(),
        )
        .unwrap();
        let Response::Stat {
            serial,
            content,
            metadata,
            ..
        } = entry
        else {
            panic!("file")
        };
        Self {
            temp: Temp(path),
            service,
            peer,
            branch: forked.branch.branch,
            base_layer: forked.branch.base_layer,
            root: forked.effective_root,
            scope: forked.scope,
            root_serial: created.root_serial,
            file_serial: serial,
            file_content: content,
            file_metadata: metadata,
            next: Cell::new(4),
        }
    }

    pub fn path(&self) -> &Path {
        &self.temp.0
    }
    pub fn call(&self, operation: Operation, body: &mut dyn Read) -> Result<Response, Failure> {
        let id = self.next.get();
        self.next.set(id + 1);
        dispatch(&self.service, &self.peer, id, operation, body)
    }
    pub fn stage(
        &self,
        header: PreparedChanges,
        body: &mut dyn Read,
    ) -> Result<StageWire, Failure> {
        let Response::History(result) = self.call(
            Operation::HistoryCommand(HistoryCommand::StageChanges(header)),
            body,
        )?
        else {
            panic!("history")
        };
        let HistoryResult::Stage(stage) = *result else {
            panic!("stage")
        };
        Ok(stage)
    }
    pub fn current_stage(&self, workspace: [u8; 32]) -> StageWire {
        let Response::History(result) = self
            .call(
                Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
                &mut io::empty(),
            )
            .unwrap()
        else {
            panic!("history")
        };
        let HistoryResult::Stage(stage) = *result else {
            panic!("stage")
        };
        stage
    }
    pub fn reserve(&self) -> u64 {
        let Response::History(result) = self
            .call(
                Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                    scope: self.scope,
                    count: 1,
                }),
                &mut io::empty(),
            )
            .unwrap()
        else {
            panic!("history")
        };
        let HistoryResult::Reservation {
            scope,
            start,
            count,
        } = *result
        else {
            panic!("reservation")
        };
        assert_eq!(scope, self.scope);
        assert_eq!(count, 1);
        start
    }
    pub fn stat(&self, root: Root, path: &[u8]) -> Response {
        self.call(
            Operation::Inspect {
                root,
                query: Inspect::Stat {
                    path: path.to_vec(),
                },
            },
            &mut io::empty(),
        )
        .unwrap()
    }
    pub fn list(&self, root: Root, path: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let mut after = Vec::new();
        let mut rows = Vec::new();
        loop {
            let Response::List {
                entries,
                continuation,
            } = self
                .call(
                    Operation::Inspect {
                        root,
                        query: Inspect::List {
                            path: path.to_vec(),
                            after: after.clone(),
                            entries: 64,
                            bytes: 8192,
                        },
                    },
                    &mut io::empty(),
                )
                .unwrap()
            else {
                panic!("list")
            };
            rows.extend(entries);
            if let Some(next) = continuation {
                assert!(next > after);
                after = next;
            } else {
                return rows;
            }
        }
    }
    pub fn original_bytes(&self) -> Vec<u8> {
        let id = self.next.get();
        self.next.set(id + 1);
        let request = Request {
            id,
            generation: 1,
            store: 1,
            profile: 1,
            deadline_ms: 30_000,
            response_bytes: MAX_FILE,
            operation: Operation::ReadFile {
                root: self.file_content,
                start: 0,
                end: ORIGINAL.len() as u64,
            },
        };
        let mut bytes = Vec::new();
        self.service
            .handle(&self.peer, &request, &mut io::empty(), &mut bytes)
            .0
            .unwrap();
        bytes
    }
    pub fn prepared(
        &self,
        workspace: [u8; 32],
        generation: u64,
        directories: &[Directory],
        declarations: &[(u64, u32)],
    ) -> (PreparedChanges, Vec<u8>) {
        let mut body = vec![1]; // Independent frozen wire-v1 transcription.
        let mut totals = PreparedTotals {
            directories: directories.len() as u64,
            identities: declarations.len() as u64,
            declarations: declarations.len() as u64,
            fresh: declarations.len() as u64,
            ..PreparedTotals::default()
        };
        for (parent, bindings) in directories {
            body.extend_from_slice(&parent.to_be_bytes());
            body.extend_from_slice(&(bindings.len() as u32).to_be_bytes());
            for (name, child) in bindings {
                totals.names += 1;
                totals.name_bytes += 10 + name.len() as u64;
                body.extend_from_slice(&(name.len() as u16).to_be_bytes());
                body.extend_from_slice(name);
                body.extend_from_slice(&child.unwrap_or(0).to_be_bytes());
            }
        }
        for (serial, mode) in declarations {
            body.push(6);
            body.extend_from_slice(&serial.to_be_bytes());
            body.extend_from_slice(&mode.to_be_bytes());
            body.extend_from_slice(&0i64.to_be_bytes());
            body.extend_from_slice(&0u32.to_be_bytes());
        }
        let header = PreparedChanges {
            workspace,
            branch: self.branch,
            expected_head: None,
            expected_base: self.base_layer,
            generation,
            base: self.root,
            scope: self.scope,
            root_serial: self.root_serial,
            totals,
        };
        assert_eq!(header.stream_bytes().unwrap(), body.len() as u64);
        (header, body)
    }
}

fn dispatch(
    service: &Service,
    peer: &VerifiedPeer,
    id: u64,
    operation: Operation,
    body: &mut dyn Read,
) -> Result<Response, Failure> {
    let request = Request {
        id,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryCommand(_) | Operation::HistoryQuery(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: 30_000,
        response_bytes: MAX_FILE,
        operation,
    };
    service.handle(peer, &request, body, &mut io::sink()).0
}
