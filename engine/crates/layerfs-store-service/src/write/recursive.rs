//! Recursive mutations use bounded directory pages and SQLite-backed hardlink maps.
use super::mutation::{failure, read_failure, Mutation, Result};
use crate::history::lookup as args;
use layerfs_content::{
    filesystem::{PathName, SymlinkTarget},
    object::inode_leaf::InodeKind,
};
use layerfs_overlay::{
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope,
    OperationRecordExpectedValue,
};
use layerfs_workspace::{Operation, OperationRecordApply, OverlayOperationRecords};
struct Frame {
    path: String,
    serial: u64,
    after: Option<Vec<u8>>,
    rows: std::vec::IntoIter<(Vec<u8>, u64)>,
    done: bool,
}
impl Frame {
    fn new(path: String, serial: u64) -> Self {
        Self {
            path,
            serial,
            after: None,
            rows: Vec::new().into_iter(),
            done: false,
        }
    }
}
fn join(path: &str, name: &[u8]) -> Result<String> {
    Ok(format!(
        "{}/{}",
        path.trim_end_matches('/'),
        std::str::from_utf8(name).map_err(args::failed)?
    ))
}
impl Mutation<'_> {
    pub fn remove(&mut self, path: &str) -> Result<()> {
        if path == "/" {
            return Err(args::invalid("cannot remove the root"));
        }
        let top = self.resolve(path)?;
        if top.kind != InodeKind::Directory {
            let (parent, name) = self.parent(path)?;
            self.apply(Operation::Unlink { parent, name })?;
            return Ok(());
        }
        let mut stack = vec![Frame::new(path.to_owned(), top.serial)];
        while let Some(frame) = stack.last_mut() {
            let next = match frame.rows.next() {
                Some(row) => Some(row),
                None if frame.done => None,
                None => {
                    let page = self
                        .view
                        .list(self.op.overlay(), frame.serial, frame.after.as_deref())
                        .map_err(read_failure)?;
                    frame.after = page.continuation;
                    frame.done = frame.after.is_none();
                    frame.rows = page.entries.into_iter();
                    continue;
                }
            };
            if let Some((name, serial)) = next {
                let child = join(&frame.path, &name)?;
                let stat = self
                    .view
                    .stat(self.op.overlay(), serial)
                    .map_err(read_failure)?;
                if stat.kind == InodeKind::Directory {
                    stack.push(Frame::new(child, serial));
                } else {
                    self.apply(Operation::Unlink {
                        parent: frame.serial,
                        name: PathName::from_bytes(&name).map_err(args::content)?,
                    })?;
                }
            } else {
                let frame = stack.pop().expect("directory");
                let (parent, name) = self.parent(&frame.path)?;
                self.apply(Operation::Rmdir { parent, name })?;
            }
        }
        Ok(())
    }
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        overlap(from, to)?;
        if self.optional(to)?.is_some() {
            return Err(args::refuse(
                "Exists",
                to,
                layerfs_bridge::control::SdkValue::Null,
            ));
        }
        let (parent, name) = self.parent(from)?;
        let (new_parent, new_name) = self.parent(to)?;
        let destination_path = Some(
            to.rsplit_once('/')
                .expect("path")
                .0
                .split('/')
                .filter(|s| !s.is_empty())
                .map(PathName::new)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(args::content)?,
        );
        self.apply(Operation::Rename {
            parent,
            name,
            new_parent,
            new_name,
            replace: false,
            destination_path,
        })?;
        Ok(())
    }
    pub fn copy(&mut self, from: &str, to: &str, index: u64) -> Result<()> {
        overlap(from, to)?;
        if self.optional(to)?.is_some() {
            return Err(args::refuse(
                "Exists",
                to,
                layerfs_bridge::control::SdkValue::Null,
            ));
        }
        let top = self.resolve(from)?;
        let owner = self
            .op
            .overlay()
            .acquire_operation(self.workspace.route(), index + 1)
            .map_err(args::failed)?;
        let scope = IndexedOperationRecordScope {
            owner,
            file_scope: index,
        };
        let result = (|| {
            self.copy_one(top.serial, to, scope)?;
            if top.kind != InodeKind::Directory {
                return Ok(());
            }
            let mut stack = vec![Frame::new(from.to_owned(), top.serial)];
            while let Some(frame) = stack.last_mut() {
                let next = match frame.rows.next() {
                    Some(row) => Some(row),
                    None if frame.done => {
                        stack.pop();
                        continue;
                    }
                    None => {
                        let page = self
                            .view
                            .list(self.op.overlay(), frame.serial, frame.after.as_deref())
                            .map_err(read_failure)?;
                        frame.after = page.continuation;
                        frame.done = frame.after.is_none();
                        frame.rows = page.entries.into_iter();
                        continue;
                    }
                };
                let Some((name, serial)) = next else {
                    continue;
                };
                let source = join(&frame.path, &name)?;
                let target = format!("{to}{}", &source[from.len()..]);
                let stat = self
                    .view
                    .stat(self.op.overlay(), serial)
                    .map_err(read_failure)?;
                self.copy_one(serial, &target, scope)?;
                if stat.kind == InodeKind::Directory {
                    stack.push(Frame::new(source, serial));
                }
            }
            Ok(())
        })();
        // Unknown operation custody is kept rather than released or replayed.
        if result
            .as_ref()
            .is_err_and(|e: &layerfs_bridge::control::SdkRefusal| e.code == "Unknown")
        {
            return result;
        }
        self.op
            .overlay()
            .release_operation(self.workspace.route(), owner)
            .map_err(args::failed)?;
        result
    }
    fn copy_one(
        &mut self,
        serial: u64,
        to: &str,
        scope: IndexedOperationRecordScope,
    ) -> Result<()> {
        let source = self
            .view
            .stat(self.op.overlay(), serial)
            .map_err(read_failure)?;
        let (parent, name) = self.parent(to)?;
        let mut raw = [0; 32];
        raw[..8].copy_from_slice(&serial.to_be_bytes());
        let key = IndexedOperationRecordKey {
            kind: 0x53444b43,
            key: raw,
        };
        if source.kind == InodeKind::RegularFile {
            if let Some(bytes) = self
                .op
                .overlay()
                .operation_record_get(scope, key)
                .map_err(failure)?
                .value
            {
                let serial = u64::from_be_bytes(bytes.as_slice().try_into().map_err(args::failed)?);
                self.apply(Operation::Link {
                    serial,
                    parent,
                    name,
                })?;
                return Ok(());
            }
        }
        let operation = match source.kind {
            InodeKind::Directory => Operation::Mkdir {
                parent,
                name,
                mode: source.metadata.mode,
            },
            InodeKind::RegularFile => Operation::Create {
                parent,
                name,
                mode: source.metadata.mode,
            },
            InodeKind::Symlink => Operation::Symlink {
                parent,
                name,
                target: SymlinkTarget::new(
                    self.view
                        .readlink(self.op.overlay(), serial)
                        .map_err(read_failure)?
                        .as_bytes()
                        .to_vec(),
                )
                .map_err(args::content)?,
            },
        };
        let copied = self
            .apply(operation)?
            .ok_or_else(|| args::failed("copy inode"))?;
        if source.kind == InodeKind::RegularFile {
            let mut offset = 0;
            while offset < source.logical_len {
                let bytes = self.read(serial, offset, 32768)?;
                if bytes.is_empty() {
                    return Err(args::failed("copy shortened"));
                }
                self.write_at(copied.serial, offset, &bytes)?;
                offset += bytes.len() as u64;
            }
            let change = IndexedOperationRecordChange {
                key,
                expected: OperationRecordExpectedValue::Missing,
                value: Some(copied.serial.to_be_bytes().to_vec()),
            };
            if !matches!(
                self.op
                    .overlay()
                    .operation_record_apply(scope, vec![change])
                    .map_err(failure)?
                    .value,
                OperationRecordApply::Applied
            ) {
                return Err(args::failed("copy hardlink index collision"));
            }
        }
        Ok(())
    }
}
fn overlap(from: &str, to: &str) -> Result<()> {
    if from == to || from == "/" || to.starts_with(&format!("{from}/")) {
        Err(args::invalid("source and destination overlap"))
    } else {
        Ok(())
    }
}
