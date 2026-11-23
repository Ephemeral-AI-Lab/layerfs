//! A bound Workspace changed in place and read back through its live view,
//! for Commit tests that change a Workspace between and during Commits.
use super::support::{Change, Fixture};
use layerfs_content::filesystem::PathName;
use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_store_service::store::{BindRequest, BoundWorkspace};
use layerfs_workspace::{Operation, Outcome, Position, Time};

/// The test Store's project.
pub fn branch(name: &str) -> BranchId {
    BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new(name).unwrap(),
    )
}

/// Binds Workspace `id` on `name`.
pub fn bind(fixture: &Fixture, id: u8, name: &str) -> BoundWorkspace {
    fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
                branch: branch(name),
            },
        )
        .unwrap()
        .workspace
}

/// Applies `changes` in turn to the Workspace's live view, at time 123 s,
/// with each reply attempt recorded. A refused change panics.
pub fn change(workspace: &BoundWorkspace, changes: &[Change<'_>]) {
    for change in changes {
        try_change(workspace, *change).unwrap_or_else(|error| panic!("change: {error}"));
    }
}

/// One change; its refusal is returned as text.
pub fn try_change(workspace: &BoundWorkspace, change: Change<'_>) -> Result<(), String> {
    let operation = workspace.operation().unwrap();
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, 9101)
        .unwrap();
    let view = operation.workspace().view_for_source(source).unwrap();
    let lookup = |path: &str| -> Result<u64, String> {
        let mut serial = view.root_serial();
        for name in path.split('/').filter(|part| !part.is_empty()) {
            serial = view
                .lookup(operation.overlay(), serial, &PathName::new(name).unwrap())
                .map_err(|error| format!("{path}: {error:?}"))?
                .serial;
        }
        Ok(serial)
    };
    let split = |path: &str| -> Result<(u64, PathName), String> {
        let (parent, name) = path.rsplit_once('/').unwrap();
        Ok((lookup(parent)?, PathName::new(name).unwrap()))
    };
    let apply = |op: Operation| -> Result<(), String> {
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
            .map_err(|error| format!("{error:?}"))?;
        let Outcome::Applied { publication, .. } = applied else {
            return Err(format!("{applied:?}"));
        };
        operation.overlay().reply_attempted(publication).unwrap();
        Ok(())
    };
    let done = (|| match change {
        Change::Mode(path, mode) => apply(Operation::SetAttributes {
            serial: lookup(path)?,
            mode: Some(mode),
            mtime: None,
            size: None,
        }),
        Change::Mtime(path, seconds) => apply(Operation::SetAttributes {
            serial: lookup(path)?,
            mode: None,
            mtime: Some(Time {
                seconds,
                nanoseconds: 0,
            }),
            size: None,
        }),
        Change::Write(path, bytes) => apply(Operation::Write {
            serial: lookup(path)?,
            position: Position::At(0),
            data: bytes.to_vec().into(),
        }),
        Change::Create(path, bytes) => {
            let (parent, name) = split(path)?;
            apply(Operation::Create {
                parent,
                name,
                mode: 0o644,
            })?;
            apply(Operation::Write {
                serial: lookup(path)?,
                position: Position::At(0),
                data: bytes.to_vec().into(),
            })
        }
        Change::Mkdir(path) => {
            let (parent, name) = split(path)?;
            apply(Operation::Mkdir {
                parent,
                name,
                mode: 0o755,
            })
        }
        Change::Unlink(path) => {
            let (parent, name) = split(path)?;
            apply(Operation::Unlink { parent, name })
        }
        Change::Rmdir(path) => {
            let (parent, name) = split(path)?;
            apply(Operation::Rmdir { parent, name })
        }
        _ => Err("change not used by these tests".into()),
    })();
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
    done
}

/// The bytes the Workspace's live view shows at `path`, or why not.
pub fn live(workspace: &BoundWorkspace, path: &str) -> Result<Vec<u8>, String> {
    let operation = workspace.operation().unwrap();
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, 9102)
        .unwrap();
    let view = operation.workspace().view_for_source(source).unwrap();
    let read = (|| {
        let mut serial = view.root_serial();
        for name in path.split('/').filter(|part| !part.is_empty()) {
            serial = view
                .lookup(operation.overlay(), serial, &PathName::new(name).unwrap())
                .map_err(|error| format!("{path}: {error:?}"))?
                .serial;
        }
        let mut bytes = Vec::new();
        loop {
            let read = view
                .read(
                    operation.overlay(),
                    serial,
                    bytes.len() as u64,
                    64 * 1024,
                    &mut bytes,
                )
                .map_err(|error| format!("{path}: {error:?}"))?;
            if read == 0 {
                return Ok(bytes);
            }
        }
    })();
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
    read
}

/// The text `branch` holds at `path` in the Store, or `None` when absent.
pub fn committed(fixture: &Fixture, name: &str, path: &str) -> Option<String> {
    fixture
        .try_call(
            "fs.read",
            vec![
                ("branch", layerfs_bridge::control::SdkValue::text(name)),
                ("path", layerfs_bridge::control::SdkValue::text(path)),
            ],
        )
        .ok()
        .map(|value| value.get_text("text").unwrap_or_default().to_owned())
}
