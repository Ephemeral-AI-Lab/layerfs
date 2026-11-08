//! The pinned snapshot and its depth-first walk, encoded as bounded
//! `tree::Record` parts for the writer (here or in a worker).
use crate::project::tree::{header, Record};
use layerfs_bridge::control::SdkRefusal;
use layerfs_bridge::sdk_values::{hex, node_text, short};
use layerfs_content::compare::{memo::Memo, tree::TreeTable, EntryFacts};
use layerfs_content::file::FileView;
use layerfs_content::filesystem::{
    attributes::{read_portable, AttributeReadWork},
    directory::read::{list_after, DirectoryReadWork},
    path::PathName,
    sorted::DirectoryRoot,
    symlink::SymlinkTarget,
    FilesystemRootId,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError};
use layerfs_history::NodeId;
use layerfs_home::outcome::refused;
use layerfs_store_service::history::{lookup, MAIN};
use layerfs_store_service::read::cached::Cached;
use layerfs_store_service::store::StorePorts;
use layerfs_store_service::StoreService;
use layerfs_telemetry::timer::Timing;
use std::io::Write;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Maximum size of one streamed `tree::Record` part (`2 MiB`).
const PART_LIMIT: usize = 2 << 20;

/// Maximum size of one `Record::Write` piece (`256 KiB`).
const PIECE_LIMIT: usize = 256 << 10;

/// Smallest room left in a part worth filling before flushing (`64 KiB`).
const MIN_ROOM: usize = 64 << 10;

/// Directory entries read per B-tree page wave.
const DIRECTORY_WINDOW: usize = 64;
const DIRECTORY_BYTES: usize = 8192;

pub(super) struct PinnedSnapshot {
    reader: std::sync::Arc<StorePorts>,
    pub(super) label: String,
    pub(super) node: String,
    pub(super) root: String,
    tree: TreeTable,
}

/// Resolves `(project, branch)` once into an immutable `(NodeId, ObjectId,
/// TreeTable)` snapshot.
pub(super) fn pin_snapshot(
    service: &StoreService,
    project: &str,
    branch: &str,
) -> Result<PinnedSnapshot> {
    let stack = lookup::project(&service.store, project)?;
    let node = if branch == MAIN {
        NodeId::Layer(stack.head_layer)
    } else {
        let branch_rec = lookup::branch(&service.store, &stack, branch)?;
        lookup::branch_node(&branch_rec)
    };
    let label = format!("{branch}@{}", short(node));
    let root = lookup::root(&service.store, node)?;
    let reader = service.store.ports(stack.scope);
    let tree = TreeTable::open(reader.as_ref(), FilesystemRootId(root)).map_err(lookup::content)?;
    Ok(PinnedSnapshot {
        reader,
        label,
        node: node_text(node),
        root: hex(root.as_bytes()),
        tree,
    })
}

#[derive(Default)]
pub(super) struct Counts {
    pub(super) files: u64,
    pub(super) directories: u64,
    pub(super) symlinks: u64,
    pub(super) bytes: u64,
}

struct DirFrame {
    rel: Vec<u8>,
    entry: EntryFacts,
    after: Option<PathName>,
    pending: std::vec::IntoIter<(PathName, u64, Option<InodeValue>)>,
    exhausted: bool,
}

/// Walks the pinned snapshot depth-first through `Cached` + `Memo`, encoding
/// `tree::Record`s into bounded parts and passing each part to `on_part`.
pub(super) fn stream_snapshot(
    pinned: &PinnedSnapshot,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<Counts> {
    let cached = Cached::new(&pinned.reader)?;
    let memo = Memo::new(&cached);
    let root_serial = pinned.tree.root_serial();
    let root_value = pinned
        .tree
        .value(&memo, root_serial)
        .map_err(lookup::content)?
        .ok_or_else(|| lookup::failed("missing root inode"))?;
    if root_value.kind != InodeKind::Directory {
        return Err(lookup::failed("snapshot root is not a directory"));
    }
    let mut part = Vec::with_capacity(PART_LIMIT.min(256 << 10));
    let mut counts = Counts::default();
    let mut stack = vec![DirFrame {
        rel: Vec::new(),
        entry: EntryFacts {
            serial: root_serial,
            value: root_value,
        },
        after: None,
        pending: Vec::new().into_iter(),
        exhausted: false,
    }];

    while let Some(frame) = stack.last_mut() {
        if frame.pending.len() == 0 && !frame.exhausted {
            let page = list_after(
                &memo,
                DirectoryRoot(frame.entry.value.content_root),
                frame.after.as_ref(),
                DIRECTORY_WINDOW,
                DIRECTORY_BYTES,
                &mut DirectoryReadWork::default(),
            )
            .map_err(lookup::content)?;
            frame.exhausted = page.continuation.is_none();
            frame.after = page.continuation;
            let serials: Vec<u64> = page.entries.iter().map(|(_, s)| *s).collect();
            let values = pinned
                .tree
                .values(&memo, &serials)
                .map_err(lookup::content)?;
            frame.pending = page
                .entries
                .into_iter()
                .zip(values)
                .map(|((name, serial), val)| (name, serial, val))
                .collect::<Vec<_>>()
                .into_iter();
        }
        let Some((name, serial, value)) = frame.pending.next() else {
            let finished = stack.pop().expect("active directory frame");
            emit_meta(
                &memo,
                &finished.rel,
                &finished.entry.value,
                &mut part,
                on_part,
            )?;
            continue;
        };
        let value = value.ok_or_else(|| lookup::failed("missing bound inode"))?;
        let mut rel = frame.rel.clone();
        if !rel.is_empty() {
            rel.push(b'/');
        }
        rel.extend_from_slice(name.as_bytes());
        match value.kind {
            InodeKind::Directory => {
                counts.directories += 1;
                push_record(Record::Directory { path: &rel }, &mut part, on_part)?;
                stack.push(DirFrame {
                    rel,
                    entry: EntryFacts { serial, value },
                    after: None,
                    pending: Vec::new().into_iter(),
                    exhausted: false,
                });
            }
            InodeKind::Symlink => {
                counts.symlinks += 1;
                let canonical = memo
                    .read_canonical(value.content_root)
                    .map_err(lookup::content)?;
                let target = SymlinkTarget::decode(&canonical).map_err(lookup::content)?;
                push_record(
                    Record::Link {
                        path: &rel,
                        target: target.as_bytes(),
                    },
                    &mut part,
                    on_part,
                )?;
                emit_meta(&memo, &rel, &value, &mut part, on_part)?;
            }
            InodeKind::RegularFile => {
                counts.files += 1;
                let file_bytes = emit_file(&cached, &rel, &value, &mut part, on_part)?;
                counts.bytes += file_bytes;
                emit_meta(&memo, &rel, &value, &mut part, on_part)?;
            }
        }
    }
    flush_part(&mut part, on_part)?;
    Ok(counts)
}

fn emit_meta(
    reader: &dyn AuthenticatedObjects,
    rel: &[u8],
    value: &InodeValue,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let meta = read_portable(
        reader,
        value.metadata_root,
        value.kind,
        &mut AttributeReadWork::default(),
    )
    .map_err(lookup::content)?;
    push_record(
        Record::Meta {
            path: rel,
            mode: meta.mode,
            seconds: meta.mtime_seconds,
            nanoseconds: meta.mtime_nanoseconds,
        },
        part,
        on_part,
    )
}

fn emit_file(
    reader: &dyn AuthenticatedObjects,
    rel: &[u8],
    value: &InodeValue,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<u64> {
    let canonical = reader
        .read_canonical(value.content_root)
        .map_err(lookup::content)?;
    let view = FileView::from_canonical(value.content_root, canonical).map_err(lookup::content)?;
    let len = view.logical_len();
    if len == 0 {
        push_record(
            Record::Write {
                path: rel,
                offset: 0,
                bytes: &[],
            },
            part,
            on_part,
        )?;
        return Ok(0);
    }
    if let Some(payload) = view.whole_file_bytes().map_err(lookup::content)? {
        write_pieces(rel, 0, payload, part, on_part)?;
        return Ok(len);
    }
    let mut sink = PieceSink {
        rel,
        offset: 0,
        part,
        on_part,
        error: None,
    };
    let (read_res, _) = Timing::disabled("export.file.read", |scope| {
        view.read_range(reader, 0..len, &mut sink, scope)
    });
    if let Some(err) = sink.error {
        return Err(err);
    }
    read_res.map_err(|e| match e {
        ContentError::Io => refused("Failed", "writing exported file stream failed"),
        other => lookup::content(other),
    })?;
    Ok(len)
}

struct PieceSink<'a, 'b> {
    rel: &'a [u8],
    offset: u64,
    part: &'a mut Vec<u8>,
    on_part: &'a mut (dyn FnMut(&[u8]) -> Result<()> + 'b),
    error: Option<SdkRefusal>,
}

impl Write for PieceSink<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if let Err(err) = write_pieces(self.rel, self.offset, buf, self.part, self.on_part) {
            self.error = Some(err);
            return Err(std::io::Error::other("export sink failed"));
        }
        self.offset += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn write_pieces(
    rel: &[u8],
    mut offset: u64,
    mut bytes: &[u8],
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let fixed = header(rel) + 8 + 4;
    while !bytes.is_empty() {
        let mut room = PART_LIMIT.saturating_sub(part.len() + fixed);
        if room < MIN_ROOM.min(bytes.len()) {
            flush_part(part, on_part)?;
            room = PART_LIMIT.saturating_sub(fixed);
        }
        let take = bytes.len().min(PIECE_LIMIT).min(room.max(1));
        let (piece, rest) = bytes.split_at(take);
        push_record(
            Record::Write {
                path: rel,
                offset,
                bytes: piece,
            },
            part,
            on_part,
        )?;
        offset += take as u64;
        bytes = rest;
    }
    Ok(())
}

fn push_record(
    record: Record<'_>,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    if !part.is_empty() && part.len() + record.size() > PART_LIMIT {
        flush_part(part, on_part)?;
    }
    record.encode(part);
    if part.len() >= PART_LIMIT {
        flush_part(part, on_part)?;
    }
    Ok(())
}

fn flush_part(part: &mut Vec<u8>, on_part: &mut dyn FnMut(&[u8]) -> Result<()>) -> Result<()> {
    if !part.is_empty() {
        on_part(part)?;
        part.clear();
    }
    Ok(())
}
