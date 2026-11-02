//! SDK Store calls on the direct route (#352 F-020): `layerfs_api::call`, the
//! public method table, over a `StoreService` on a disposable installed Store,
//! with exact demand counts. Moved with the method table from
//! layerfs-store-service.
//! Moved with the code from the daemon's `support/sdk_compare.rs`; a call no
//! longer crosses a control connection.
use super::installed;
use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_content::filesystem::PathName;
use layerfs_history::{BranchId, CommitOutcome, HistoryName, LayerStackId, WorkspaceId};
use layerfs_store_service::{
    bootstrap::open_store_observed,
    store::{BindRequest, CommitOptions, CommitSuccess},
    Owner, OwnerConfig, StoreService,
};
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::path::Path;

/// One change [`Fixture::workspace_commit`] applies at a path.
#[derive(Clone, Copy)]
#[allow(dead_code)] // each test crate uses some of the variants
pub enum Change<'a> {
    /// chmod to the mode.
    Mode(&'a str, u32),
    /// Set only the mtime, in whole seconds.
    Mtime(&'a str, i64),
    /// Overwrite the bytes from offset 0.
    Write(&'a str, &'a [u8]),
    /// Save as git and editors do: the bytes go to a new sibling
    /// `<name>.save` in 64 KiB writes, which is then renamed over the path.
    SaveOver(&'a str, &'a [u8]),
    /// Create a regular file with the bytes.
    Create(&'a str, &'a [u8]),
    /// Create a directory.
    Mkdir(&'a str),
    /// Hard-link the existing path (first) at the new path (second).
    Link(&'a str, &'a str),
    /// Remove a file's name.
    Unlink(&'a str),
    /// Remove an empty directory.
    Rmdir(&'a str),
}

pub struct Fixture {
    pub host: installed::Fixture,
    pub owner: Owner,
    pub service: StoreService,
}

#[derive(Clone, Copy, Debug)]
pub struct Work {
    pub object_batches: u64,
    pub object_ids: u64,
    pub statements: u64,
    /// SQL rows returned, writer and readers together.
    pub rows: u64,
    /// SQLite VM steps, writer and readers together.
    pub vm_steps: u64,
}

impl Fixture {
    pub fn new(label: &str, build: impl FnOnce(&Path) -> usize) -> Self {
        Self::open(installed::Fixture::built(label, build))
    }

    /// Opens an installed Store that no other owner holds.
    pub fn open(host: installed::Fixture) -> Self {
        let config = OwnerConfig::default();
        let opened = open_store_observed(
            host.config.clone(),
            installed::BINDING,
            installed::CURSOR,
            config.namespaces,
            64 * 1024,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &host.directory.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            config,
        )
        .unwrap();
        let service = StoreService::new(opened, &owner);
        Self {
            host,
            owner,
            service,
        }
    }

    pub fn try_call(
        &self,
        method: &str,
        mut fields: Vec<(&str, SdkValue)>,
    ) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
        fields.push(("project", SdkValue::text("installed")));
        let call = SdkCall {
            method: method.into(),
            args: SdkValue::Map(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        };
        layerfs_api::call(&self.service, &call)
    }

    pub fn call(&self, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
        self.try_call(method, fields)
            .unwrap_or_else(|refusal| panic!("{method} refused: {refusal:?}"))
    }

    pub fn fork(&self, name: &str) {
        self.call("branch.fork", vec![("name", SdkValue::text(name))]);
    }

    pub fn apply(&self, branch: &str, changes: Vec<SdkValue>) -> SdkValue {
        self.call(
            "fs.apply",
            vec![
                ("branch", SdkValue::text(branch)),
                ("changes", SdkValue::List(changes)),
            ],
        )
    }

    pub fn diff(&self, from: &str, to: &str, limit: i64, position: i64) -> SdkValue {
        self.call(
            "branch.diff_branches",
            vec![
                ("from", SdkValue::text(from)),
                ("to", SdkValue::text(to)),
                ("pageSize", SdkValue::Int(limit)),
                ("pagePos", SdkValue::Int(position)),
            ],
        )
    }

    /// A real Workspace chmod and product Commit prepare a metadata-only
    /// snapshot: one private Workspace bound on the Store, mutated, committed
    /// with the captured producer and closed.
    pub fn chmod(&self, branch: &str, path: &str, mode: u32) -> String {
        self.chmods(branch, &[(path, mode)], 77)
    }

    /// [`Self::chmod`] of several paths in turn, in one Workspace `id` and
    /// one Commit; a path may appear more than once.
    pub fn chmods(&self, branch: &str, changes: &[(&str, u32)], id: u8) -> String {
        let changes: Vec<Change> = changes
            .iter()
            .map(|&(path, mode)| Change::Mode(path, mode))
            .collect();
        let committed = self.workspace_commit(branch, &changes, id, || {});
        let Some(CommitOutcome::Committed { record, .. }) = committed.history else {
            panic!("chmod Commit: {:?}", committed.history);
        };
        record.id.to_string()
    }

    /// One private Workspace `id` bound on `branch` applies `changes` in
    /// turn; `before` runs after the changes and before the product Commit
    /// (so it may move the head: the Commit then merges), and the Workspace
    /// closes after it. Mutations carry the time 123 s.
    pub fn workspace_commit(
        &self,
        branch: &str,
        changes: &[Change<'_>],
        id: u8,
        before: impl FnOnce(),
    ) -> CommitSuccess {
        let bound = self
            .service
            .store
            .bind(
                self.owner.client(),
                BindRequest {
                    workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
                    branch: BranchId::named(
                        LayerStackId::from_authority([23; 16]),
                        &HistoryName::new(branch).unwrap(),
                    ),
                },
            )
            .unwrap();
        let workspace = bound.workspace;
        let operation = workspace.operation().unwrap();
        let route = operation.workspace().route();
        let source = operation
            .overlay()
            .acquire_base_source(route, 9001)
            .unwrap();
        let view = operation.workspace().view_for_source(source).unwrap();
        let lookup = |path: &str| {
            let mut serial = view.root_serial();
            for name in path.split('/').filter(|part| !part.is_empty()) {
                serial = view
                    .lookup(operation.overlay(), serial, &PathName::new(name).unwrap())
                    .unwrap()
                    .serial;
            }
            serial
        };
        let apply = |op: Operation| {
            let applied = operation
                .workspace()
                .mutate(
                    operation.overlay(),
                    operation.ports(),
                    &view,
                    op,
                    Time {
                        seconds: 123,
                        nanoseconds: 0,
                    },
                )
                .unwrap();
            let Outcome::Applied { publication, .. } = applied else {
                panic!("Workspace mutation: {applied:?}");
            };
            operation.overlay().reply_attempted(publication).unwrap();
        };
        let split = |path: &str| {
            let (parent, name) = path.rsplit_once('/').unwrap();
            (lookup(parent), PathName::new(name).unwrap())
        };
        let attributes = |path: &str, mode: Option<u32>, mtime: Option<i64>| {
            apply(Operation::SetAttributes {
                serial: lookup(path),
                mode,
                mtime: mtime.map(|seconds| Time {
                    seconds,
                    nanoseconds: 0,
                }),
                size: None,
            })
        };
        for change in changes {
            match change {
                Change::Mode(path, mode) => attributes(path, Some(*mode), None),
                Change::Mtime(path, seconds) => attributes(path, None, Some(*seconds)),
                Change::Write(path, bytes) => apply(Operation::Write {
                    serial: lookup(path),
                    position: Position::At(0),
                    data: bytes.to_vec().into(),
                }),
                Change::SaveOver(path, bytes) => {
                    let (parent, name) = split(path);
                    let save = PathName::new(&format!("{}.save", name.as_str())).unwrap();
                    apply(Operation::Create {
                        parent,
                        name: save.clone(),
                        mode: 0o644,
                    });
                    let serial = lookup(&format!("{path}.save"));
                    // in 64 KiB writes, as the kernel sends them
                    for (index, piece) in bytes.chunks(64 * 1024).enumerate() {
                        apply(Operation::Write {
                            serial,
                            position: Position::At(index as u64 * 64 * 1024),
                            data: piece.to_vec().into(),
                        });
                    }
                    apply(Operation::Rename {
                        parent,
                        name: save,
                        new_parent: parent,
                        new_name: name,
                        replace: true,
                        destination_path: None,
                    });
                }
                Change::Create(path, bytes) => {
                    let (parent, name) = split(path);
                    apply(Operation::Create {
                        parent,
                        name,
                        mode: 0o644,
                    });
                    apply(Operation::Write {
                        serial: lookup(path),
                        position: Position::At(0),
                        data: bytes.to_vec().into(),
                    });
                }
                Change::Mkdir(path) => {
                    let (parent, name) = split(path);
                    apply(Operation::Mkdir {
                        parent,
                        name,
                        mode: 0o755,
                    });
                }
                Change::Link(existing, path) => {
                    let (parent, name) = split(path);
                    apply(Operation::Link {
                        serial: lookup(existing),
                        parent,
                        name,
                    });
                }
                Change::Unlink(path) => {
                    let (parent, name) = split(path);
                    apply(Operation::Unlink { parent, name });
                }
                Change::Rmdir(path) => {
                    let (parent, name) = split(path);
                    apply(Operation::Rmdir { parent, name });
                }
            }
        }
        drop(view);
        operation
            .overlay()
            .release_base_source(route, source)
            .unwrap();
        drop(operation);
        before();
        let committed = workspace
            .commit_captured(&CommitOptions {
                force: false,
                message: Some("Workspace changes".into()),
            })
            .unwrap_or_else(|failure| panic!("Workspace Commit: {failure:?}"));
        workspace.close().unwrap();
        committed
    }

    pub fn work(&self) -> Work {
        let store = self.service.store.work();
        let sql = self.service.opened.diagnostics().unwrap();
        Work {
            object_batches: store.object_batches,
            object_ids: store.object_ids,
            statements: sql.writer.statements
                + sql
                    .readers
                    .iter()
                    .map(|reader| reader.statements)
                    .sum::<u64>(),
            rows: sql.writer.returned_rows
                + sql
                    .readers
                    .iter()
                    .map(|reader| reader.returned_rows)
                    .sum::<u64>(),
            vm_steps: sql.writer.vm_steps
                + sql
                    .readers
                    .iter()
                    .map(|reader| reader.vm_steps)
                    .sum::<u64>(),
        }
    }

    pub fn since(&self, before: Work) -> Work {
        let after = self.work();
        Work {
            object_batches: after.object_batches - before.object_batches,
            object_ids: after.object_ids - before.object_ids,
            statements: after.statements - before.statements,
            rows: after.rows - before.rows,
            vm_steps: after.vm_steps - before.vm_steps,
        }
    }

    /// Closes the Store and its owner, keeping the installed files.
    pub fn close(self) -> installed::Fixture {
        drop(self.service);
        self.owner.stop().unwrap();
        self.host
    }

    pub fn cleanup(self) {
        self.close().cleanup();
    }
}

pub fn write(path: &str, bytes: impl Into<Vec<u8>>) -> SdkValue {
    SdkValue::map([
        ("op", SdkValue::text("write")),
        ("path", SdkValue::text(path)),
        ("bytes", SdkValue::Bytes(bytes.into())),
    ])
}

pub fn operation(kind: &str, path: &str) -> SdkValue {
    SdkValue::map([("op", SdkValue::text(kind)), ("path", SdkValue::text(path))])
}

pub fn items(value: &SdkValue) -> &[SdkValue] {
    match value.get("items") {
        Some(SdkValue::List(items)) => items,
        _ => panic!("missing items: {value:?}"),
    }
}

pub fn paths(value: &SdkValue) -> Vec<&str> {
    items(value)
        .iter()
        .map(|item| item.get_text("path").unwrap())
        .collect()
}
