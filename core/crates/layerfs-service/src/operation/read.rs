//! Local bounded reads and the frozen inspection variants.
use super::failure::content;
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::root::FilesystemRootId;
use layerfs_content::{read_range, FileView, FilesystemRead, LogicalPath, ObjectId, PathName};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::io::Write;
pub fn id(root: &Root) -> ObjectId {
    ObjectId::from_bytes(root).expect("fixed identity width")
}
pub fn read(
    store: &Store,
    r: &Request,
    out: &mut dyn Write,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    let provider = StoreProvider::new(store);
    match &r.operation {
        Operation::ReadFile { root, start, end } => {
            read_range(
                &provider,
                id(root),
                *start..*end,
                out,
                scope.child("service.read"),
            )
            .map_err(content)?;
            Ok(Response::Read {
                length: end - start,
            })
        }
        Operation::Inspect {
            root,
            query: Inspect::File,
        } => {
            let view = FileView::open(&provider, id(root), scope.child("service.inspect"))
                .map_err(content)?;
            let representation = match view.content() {
                layerfs_content::FileContent::WholeFile { .. } => 1,
                layerfs_content::FileContent::Chunked(_) => 2,
            };
            Ok(Response::File {
                length: view.logical_len(),
                representation,
            })
        }
        Operation::Inspect { root, query } => {
            let mut fs =
                FilesystemRead::new(&provider, FilesystemRootId(id(root))).map_err(content)?;
            match query {
                Inspect::Stat { path } => {
                    let path = LogicalPath::from_bytes(path).map_err(content)?;
                    let value = fs.resolve(&path).map_err(content)?;
                    let meta = fs.read_portable(&path).map_err(content)?;
                    Ok(Response::Stat {
                        serial: value.serial,
                        kind: value.value.kind.code(),
                        references: value.value.namespace_ref_count,
                        content: *value.value.content_root.as_bytes(),
                        metadata: *value.value.metadata_root.as_bytes(),
                        mode: meta.mode,
                        mtime: meta.mtime_seconds,
                        nanoseconds: meta.mtime_nanoseconds,
                    })
                }
                Inspect::List {
                    path,
                    after,
                    entries,
                    bytes,
                } => {
                    let path = LogicalPath::from_bytes(path).map_err(content)?;
                    let after = if after.is_empty() {
                        None
                    } else {
                        Some(PathName::from_bytes(after).map_err(content)?)
                    };
                    let page = fs
                        .list(&path, after.as_ref(), *entries as usize, *bytes as usize)
                        .map_err(content)?;
                    Ok(Response::List {
                        entries: page
                            .entries
                            .into_iter()
                            .map(|(name, serial)| (name.as_bytes().to_vec(), serial))
                            .collect(),
                        continuation: page.continuation.map(|name| name.as_bytes().to_vec()),
                    })
                }
                Inspect::Readlink { path } => {
                    let path = LogicalPath::from_bytes(path).map_err(content)?;
                    Ok(Response::Link(
                        fs.readlink(&path).map_err(content)?.as_bytes().to_vec(),
                    ))
                }
                Inspect::File => Err(Code::InvalidInput.into()),
            }
        }
        _ => Err(Code::Unsupported.into()),
    }
}
