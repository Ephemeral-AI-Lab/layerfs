//! Live Workspace diagnostics over bounded source/list windows, with exact release.
use crate::{
    control::Service,
    store::{BoundWorkspace, StoreOperation},
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::TreeTable, filesystem::FilesystemRootId, object::inode_leaf::InodeKind,
};
use layerfs_store_service::{
    history::lookup::{self as args, Answer},
    operator::args::{paging, text},
    read::{
        snapshot::{self, Kind, Snap},
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
    let snap = Snap {
        kind: Kind::Commit,
        label: "Workspace base".into(),
        node: binding.branch.head_commit.map_or(
            layerfs_history::NodeId::Layer(binding.branch.base_layer),
            layerfs_history::NodeId::Commit,
        ),
        root,
        tree: TreeTable::open(op.ports(), FilesystemRootId(root)).map_err(args::content)?,
    };
    let pending = workspace
        .pending_conflicts()
        .map_err(args::failed)?
        .unwrap_or_default();
    let mut base = Walker::new(op.ports(), &snap, "/")?;
    let mut live = Live::new(view, op);
    let mut work = layerfs_content::filesystem::attributes::AttributeReadWork::default();
    let (mut a, mut b) = (base.next()?, live.next()?);
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
                b = live.next()?;
                (p, None, Some(e))
            }
            std::cmp::Ordering::Equal => {
                let (p, x) = a.take().expect("base");
                let (_, y) = b.take().expect("live");
                a = base.next()?;
                b = live.next()?;
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
    let _ = service;
    Ok(())
}

pub(crate) fn changes(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    let (limit, pos) = paging(args)?;
    with_source(&workspace, |op, view| {
        let (mut seen, mut items, mut spent, mut more) = (0u64, Vec::new(), 0, false);
        scan(service, &workspace, op, view, |path, kind, markers| {
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

pub(crate) fn overlay(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    let op = workspace.operation().map_err(args::failed)?;
    let resources = op.overlay().resources().map_err(args::failed)?;
    let mut counts = [0i64; 3];
    with_source(&workspace, |op, view| {
        scan(service, &workspace, op, view, |_, kind, _| {
            counts[match kind {
                "Added" => 0,
                "Modified" => 1,
                _ => 2,
            }] += 1;
            Ok(true)
        })
    })?;
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
        (
            "changes",
            SdkValue::map([
                ("added", SdkValue::Int(counts[0])),
                ("modified", SdkValue::Int(counts[1])),
                ("removed", SdkValue::Int(counts[2])),
            ]),
        ),
    ]))
}
