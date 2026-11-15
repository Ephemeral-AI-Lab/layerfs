//! Live Workspace diagnostics over bounded source/list windows, with exact release.
use crate::{
    control::Service,
    store::{BoundWorkspace, StoreOperation},
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{EntryFacts, TreeTable},
    filesystem::FilesystemRootId,
    object::inode_leaf::InodeKind,
};
use layerfs_store_service::{
    history::lookup::{self as args, Answer},
    operator::args::{paging, text},
    read::{
        snapshot::{self, Kind, Snap},
        visibility::Visibility,
        window::Walker,
    },
};
use layerfs_workspace::{OverlayRead, SourceView, ViewStat};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;
struct Frame {
    path: String,
    serial: u64,
    after: Option<Vec<u8>>,
    page: std::vec::IntoIter<(Vec<u8>, u64)>,
    done: bool,
}
struct Live<'a> {
    view: &'a SourceView,
    op: &'a StoreOperation,
    frames: Vec<Frame>,
}
impl<'a> Live<'a> {
    fn new(view: &'a SourceView, op: &'a StoreOperation) -> Self {
        Self {
            view,
            op,
            frames: vec![Self::frame("/".into(), view.root_serial())],
        }
    }
    fn frame(path: String, serial: u64) -> Frame {
        Frame {
            path,
            serial,
            after: None,
            page: Vec::new().into_iter(),
            done: false,
        }
    }
    fn next(&mut self) -> Result<Option<(String, ViewStat)>> {
        while let Some(frame) = self.frames.last_mut() {
            let (name, serial) = match frame.page.next() {
                Some(row) => row,
                None if frame.done => {
                    self.frames.pop();
                    continue;
                }
                None => {
                    let page = self
                        .view
                        .list(self.op.overlay(), frame.serial, frame.after.as_deref())
                        .map_err(args::failed)?;
                    frame.after = page.continuation;
                    frame.done = frame.after.is_none();
                    frame.page = page.entries.into_iter();
                    continue;
                }
            };
            let name = std::str::from_utf8(&name).map_err(args::failed)?;
            let path = format!("{}/{}", frame.path.trim_end_matches('/'), name);
            let stat = self
                .view
                .stat(self.op.overlay(), serial)
                .map_err(args::failed)?;
            if stat.kind == InodeKind::Directory {
                self.frames.push(Self::frame(path.clone(), serial));
            }
            return Ok(Some((path, stat)));
        }
        Ok(None)
    }

    /// The next live entry that is visible at the base node. A path the markers
    /// hide is absent unless the overlay holds a local inode there: a
    /// re-creation after the bound node is an ordinary addition, as a Commit's
    /// is, while an inherited entry of a mount older than the marker is hidden.
    fn next_visible(&mut self, seen: &Visibility) -> Result<Option<(String, ViewStat)>> {
        loop {
            let Some((path, stat)) = self.next()? else {
                return Ok(None);
            };
            if seen.hidden(&path).is_some()
                && self
                    .op
                    .overlay()
                    .inode(self.view.source(), stat.serial)
                    .map_err(args::failed)?
                    .is_none()
            {
                continue;
            }
            return Ok(Some((path, stat)));
        }
    }
}

/// The base walk, less every path the markers hide at the base node: a removed
/// path is absent from `changes`, like a read of it, and so is its subtree.
struct VisibleBase<'a> {
    walker: Walker<'a>,
    seen: &'a Visibility,
}
impl VisibleBase<'_> {
    fn next(&mut self) -> Result<Option<(String, EntryFacts)>> {
        loop {
            match self.walker.next()? {
                Some((path, _)) if self.seen.hidden(&path).is_some() => continue,
                other => return Ok(other),
            }
        }
    }
}

fn selected(service: &Service, args: &SdkValue) -> Result<Arc<BoundWorkspace>> {
    service
        .operator_workspace(text(args, "path")?)
        .map_err(|e| args::failed(format!("{e:?}")))
}

fn with_source<T>(
    workspace: &BoundWorkspace,
    work: impl FnOnce(&StoreOperation, &SourceView) -> Result<T>,
) -> Result<T> {
    let op = workspace.operation().map_err(args::failed)?;
    let source = op
        .overlay()
        .acquire_diagnostic_source(workspace.route())
        .map_err(args::failed)?;
    let result = op
        .workspace()
        .view_for_source(source)
        .map_err(args::failed)
        .and_then(|view| work(&op, &view));
    let released = op
        .overlay()
        .release_base_source(workspace.route(), source)
        .map_err(args::failed);
    match (result, released) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn marker(op: &StoreOperation, view: &SourceView, stat: &ViewStat) -> Result<bool> {
    if stat.kind != InodeKind::RegularFile {
        return Ok(false);
    }
    let (mut offset, mut prefix, mut state) = (0, Vec::new(), 0);
    while offset < stat.logical_len {
        let mut bytes = Vec::new();
        let n = view
            .read(op.overlay(), stat.serial, offset, 16384, &mut bytes)
            .map_err(args::failed)?;
        if n == 0 {
            break;
        }
        offset += n;
        for byte in bytes {
            if prefix.len() < 256 {
                prefix.push(byte);
            }
            if byte == b'\n' {
                let line = String::from_utf8_lossy(&prefix);
                if state == 0 && line.starts_with("<<<<<<< ") {
                    state = 1;
                } else if state == 1 && line.trim_end() == "=======" {
                    state = 2;
                } else if state == 2 && line.starts_with(">>>>>>> ") {
                    return Ok(true);
                }
                prefix.clear();
            }
        }
    }
    Ok(false)
}

fn scan(
    service: &Service,
    workspace: &BoundWorkspace,
    op: &StoreOperation,
    view: &SourceView,
    mut visit: impl FnMut(String, &'static str, bool) -> Result<bool>,
) -> Result<()> {
    let binding = workspace.snapshot().map_err(args::failed)?;
    let root = binding.effective_root;
    let node = binding.branch.head_commit.map_or(
        layerfs_history::NodeId::Layer(binding.branch.base_layer),
        layerfs_history::NodeId::Commit,
    );
    // The base node `history_serial`, as every `Snap` carries it for the
    // `remove_file` visibility rule; 0 is the same unknown fallback that
    // `snapshot::resolve` takes.
    let serial = match service
        .store
        .published(|history| history.node_history_serial(node))
    {
        Ok(Some(serial)) => serial,
        _ => 0,
    };
    let snap = Snap {
        kind: Kind::Commit,
        label: "Workspace base".into(),
        node,
        root,
        tree: TreeTable::open(op.ports(), FilesystemRootId(root)).map_err(args::content)?,
        serial,
    };
    let pending = workspace
        .pending_conflicts()
        .map_err(args::failed)?
        .unwrap_or_default();
    // §4.3: the base walk is path-aware, so a path the markers hide at the
    // base node is absent from `changes` instead of a `Removed` row against
    // the mount's whiteout. The live side keeps a local re-creation.
    let seen = Visibility::for_snap(&service.store, &snap);
    let mut base = VisibleBase {
        walker: Walker::new(op.ports(), &snap, "/")?,
        seen: &seen,
    };
    let mut live = Live::new(view, op);
    let mut work = layerfs_content::filesystem::attributes::AttributeReadWork::default();
    let (mut a, mut b) = (base.next()?, live.next_visible(&seen)?);
    while a.is_some() || b.is_some() {
        let order = match (&a, &b) {
            (Some((a, _)), Some((b, _))) => a.split('/').cmp(b.split('/')),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        let (path, before, after) = match order {
            std::cmp::Ordering::Less => {
                let (p, e) = a.take().expect("base");
                a = base.next()?;
                (p, Some(e), None)
            }
            std::cmp::Ordering::Greater => {
                let (p, e) = b.take().expect("live");
                b = live.next_visible(&seen)?;
                (p, None, Some(e))
            }
            std::cmp::Ordering::Equal => {
                let (p, x) = a.take().expect("base");
                let (_, y) = b.take().expect("live");
                a = base.next()?;
                b = live.next_visible(&seen)?;
                (p, Some(x), Some(y))
            }
        };
        let kind = match (&before, &after) {
            (None, _) => "Added",
            (_, None) => "Removed",
            (Some(old), Some(new)) => {
                let local = op
                    .overlay()
                    .inode(view.source(), new.serial)
                    .map_err(args::failed)?;
                let local_content = match snapshot::entry(
                    op.ports(),
                    &TreeTable::open(
                        op.ports(),
                        FilesystemRootId(
                            layerfs_content::ObjectId::from_bytes(&view.source().root())
                                .map_err(args::content)?,
                        ),
                    )
                    .map_err(args::content)?,
                    &path,
                )? {
                    Some(e) => !layerfs_content::compare::tree::same_inode(
                        op.ports(),
                        &e.value,
                        &old.value,
                        &mut work,
                    )
                    .map_err(args::content)?,
                    None => true,
                };
                if old.serial == new.serial
                    && old.value.kind == new.kind
                    && !local_content
                    && (local.is_none() || new.kind == InodeKind::Directory)
                {
                    continue;
                }
                "Modified"
            }
        };
        let markers = after
            .as_ref()
            .filter(|_| pending.contains(&path))
            .map(|stat| marker(op, view, stat))
            .transpose()?
            .unwrap_or(false);
        if !visit(path, kind, markers)? {
            break;
        }
    }
    Ok(())
}

pub(crate) fn changes(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    changes_of(service, &workspace, args)
}

/// `changes` over an already selected Workspace; a test binds one directly.
fn changes_of(service: &Service, workspace: &BoundWorkspace, args: &SdkValue) -> Answer {
    let (limit, pos) = paging(args)?;
    with_source(workspace, |op, view| {
        let (mut seen, mut items, mut spent, mut more) = (0u64, Vec::new(), 0, false);
        scan(service, workspace, op, view, |path, kind, markers| {
            if seen < pos {
                seen += 1;
                return Ok(true);
            }
            if items.len() == limit
                || spent + path.len() + 96 > layerfs_store_service::read::window::BYTES
            {
                more = true;
                return Ok(false);
            }
            spent += path.len() + 96;
            items.push(SdkValue::map([
                ("path", SdkValue::text(path)),
                ("kind", SdkValue::text(kind)),
                ("markers", SdkValue::Bool(markers)),
            ]));
            Ok(true)
        })?;
        let next = args::next_page(pos, items.len(), more);
        Ok(SdkValue::map([
            ("items", SdkValue::List(items)),
            ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
            ("nextPagePos", next),
        ]))
    })
}

/// The overlay's own row counts, read from its database alone (#508). Path
/// change counts are `null`: naming them walks the base tree, which costs
/// minutes on a large tree, and state reads call this for every Workspace.
/// `list_changes` pages the changed paths on request.
pub(crate) fn overlay(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    let op = workspace.operation().map_err(args::failed)?;
    let resources = op.overlay().resources().map_err(args::failed)?;
    let c = resources.counts;
    Ok(SdkValue::map([
        ("inodes", SdkValue::Int(c.inode_rows as i64)),
        ("entries", SdkValue::Int(c.directory_entry_rows as i64)),
        ("payload", SdkValue::Int(c.payload_bytes as i64)),
        ("metadata", SdkValue::Null),
        (
            "metadataRows",
            SdkValue::Int((c.inode_rows + c.directory_entry_rows + c.operation_record_rows) as i64),
        ),
        ("changes", SdkValue::Null),
    ]))
}

#[cfg(test)]
mod tests {
    //! `workspace.changes` over a bound Workspace, without a kernel mount: the
    //! path-awareness of its base walk.
    use super::*;
    use crate::control::Service;
    use crate::{Owner, OwnerConfig};
    use layerfs_bridge::control::SdkValue;
    use layerfs_history::{PathRemovalKind, PathRemovalRequest, WorkspaceId};
    use layerfs_storage::ReservationBlocks;
    use layerfs_store_service::{
        bootstrap::open_store_observed, store::BindRequest, write::mutation::Mutation,
    };
    use std::fs;

    use crate::operator::installed_store as installed;

    fn listed(answer: &SdkValue) -> Vec<(String, String)> {
        let Some(SdkValue::List(items)) = answer.get("items") else {
            panic!("no items: {answer:?}");
        };
        items
            .iter()
            .map(|item| {
                (
                    item.get_text("path").expect("path").to_owned(),
                    item.get_text("kind").expect("kind").to_owned(),
                )
            })
            .collect()
    }

    /// A file and a directory removed by `project.remove_file` markers at the
    /// bound node are absent from `changes`, subtree and all; a visible
    /// change is still listed, and a post-bound re-creation is an addition.
    #[test]
    fn a_removed_file_and_a_hidden_directory_are_absent_from_changes() {
        let fixture = installed::Fixture::built("changes-visibility", |source| {
            fs::write(source.join("visible.txt"), b"visible\n").unwrap();
            fs::write(source.join("hidden.txt"), b"hidden\n").unwrap();
            fs::create_dir(source.join("hidden-dir")).unwrap();
            fs::write(source.join("hidden-dir/inner.txt"), b"inner\n").unwrap();
            4
        });
        let opened = open_store_observed(
            fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
            64,
            8 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        let stack = opened
            .store
            .history()
            .branch_snapshot(fixture.branch)
            .unwrap()
            .unwrap()
            .branch
            .stack;
        for path in ["hidden.txt", "hidden-dir"] {
            let outcome = opened
                .store
                .history()
                .remove_path(&PathRemovalRequest {
                    stack,
                    path: path.as_bytes().to_vec(),
                    removed_ms: 1,
                })
                .unwrap();
            assert_eq!(outcome.kind, PathRemovalKind::Removed, "{path}");
        }
        let owner = Owner::start(
            &fixture.directory.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let bound = opened
            .store
            .bind(
                owner.client(),
                BindRequest {
                    branch: fixture.branch,
                    workspace: WorkspaceId::from_authority([57; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace;
        // Bind wrote the whiteouts; a visible change and one re-creation follow.
        let mut mutation = Mutation::new(&bound).unwrap();
        mutation.write("/visible.txt", b"after\n", None).unwrap();
        mutation.finish().unwrap();
        let mut mutation = Mutation::new(&bound).unwrap();
        mutation.write("/hidden.txt", b"again\n", None).unwrap();
        mutation.finish().unwrap();

        let service = Service::new(opened.store.clone(), &owner);
        let answer = changes_of(
            &service,
            &bound,
            &SdkValue::map([("pageSize", SdkValue::Int(50))]),
        )
        .unwrap();
        assert_eq!(
            listed(&answer),
            [
                ("/hidden.txt".to_owned(), "Added".to_owned()),
                ("/visible.txt".to_owned(), "Modified".to_owned()),
            ]
        );
        assert_eq!(answer.get("hasMore"), Some(&SdkValue::Bool(false)));

        drop(service);
        bound.close().unwrap();
        owner.stop().unwrap();
        drop(opened);
        fixture.cleanup();
    }
}
