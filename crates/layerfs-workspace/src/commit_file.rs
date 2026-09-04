//! Owned, immutable content input for both inline and parallel Commit workers.
//! Identity, namespace references, metadata and publication remain coordinator-owned.
use crate::cow_tree::{Data, FileData, NodeId, Workspace};
use crate::file_edit::{Piece, PieceTree};
use layerfs_content::file::rope::{
    self, FileMutationBatch, FilePreparationMetrics, FileStateRoot, RopeCounters,
};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::{CoreError, CoreResult, ObjectId};
use layerfs_layerstack_store::{ObjectSource, Result, SnapshotReader, StoreError};
use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::FileExt;

pub(crate) const CANONICAL_BYTES: usize = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES + 21;
const IO_BYTES: usize = layerfs_content::file::cdc::MINIMUM_CHUNK_BYTES;
/// Additional explicit compiler comparison buffers. Codec/CDC/reader buffers and
/// private preparation storage require their own shared worker reservations.
pub(crate) const IO_SCRATCH_BYTES: usize = 2 * IO_BYTES;

pub(crate) struct FrozenFile {
    reader: SnapshotReader,
    source: FrozenSource,
    before: Option<FileStateRoot>,
    len: u64,
}

enum FrozenSource {
    Base {
        root: FileStateRoot,
    },
    Edited {
        base: Option<(FileStateRoot, u64)>,
        pieces: PieceTree,
        spool: Option<File>,
    },
}

pub(crate) struct CompiledFile {
    pub(crate) root: FileStateRoot,
    pub(crate) len: u64,
    pub(crate) counters: RopeCounters,
    pub(crate) preparation: FilePreparationMetrics,
}

impl FrozenFile {
    /// Called only by the quiesced coordinator. No Workspace reference or
    /// pathname survives in the resulting owned input. Duplicate descriptors
    /// are read positionally, never through their shared OS seek offset.
    pub(crate) fn freeze(
        workspace: &Workspace,
        node: NodeId,
        before: Option<FileStateRoot>,
    ) -> Result<Self> {
        let data = &workspace
            .nodes
            .get(&node)
            .ok_or(StoreError::NotFound("node"))?
            .data;
        let (source, len) = match data {
            Data::File(FileData::Base { root, len }) => (FrozenSource::Base { root: *root }, *len),
            Data::File(FileData::Edited {
                base,
                pieces,
                spool,
                ..
            }) => {
                let file = if pieces.spool_len() != 0 {
                    Some(workspace.spool_file(node, spool)?.try_clone()?)
                } else {
                    None
                };
                (
                    FrozenSource::Edited {
                        base: *base,
                        pieces: pieces.clone(),
                        spool: file,
                    },
                    pieces.len(),
                )
            }
            _ => return Err(StoreError::InvalidInput("file")),
        };
        Ok(Self {
            reader: workspace.reader.clone(),
            source,
            before,
            len,
        })
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    pub(crate) fn scratch_bytes(&self) -> Result<u64> {
        if let Some(root) = self.before {
            if self.is_identity(root)? {
                return Ok(1024);
            }
        }
        let mut read_level = 0_u64;
        let mut has_base = false;
        let mut inspect = |root| -> Result<()> {
            has_base = true;
            read_level = read_level.max(u64::from(self.source_state(root)?.tree_level));
            Ok(())
        };
        if let Some(root) = self.before {
            inspect(root)?;
        }
        match &self.source {
            FrozenSource::Base { root } => inspect(*root)?,
            FrozenSource::Edited {
                base: Some((root, _)),
                ..
            } => inspect(*root)?,
            _ => {}
        }
        if !has_base && self.len != 0 && self.len < IO_BYTES as u64 {
            return Ok(2 * IO_BYTES as u64 + 2048);
        }
        let mut entries = self.len / layerfs_content::file::cdc::MINIMUM_CHUNK_BYTES as u64 + 1;
        let mut stream_level = 0_u64;
        while entries > layerfs_content::file::extent::MAX_ENTRIES as u64 {
            entries = entries.div_ceil(layerfs_content::file::extent::MIN_ENTRIES as u64);
            stream_level += 1;
        }
        Ok(256 * 1024 + (read_level + 1) * 32 * 1024 + (stream_level + 1) * 16 * 1024)
    }

    fn source_state(
        &self,
        root: FileStateRoot,
    ) -> Result<layerfs_content::file::extent::FileStateV3> {
        let source = BoundedSource::new(&self.reader);
        source.result(rope::state(&source, root, &mut RopeCounters::default()))
    }

    /// `store` must remain private until final-root closure and normal admission.
    /// It must support read-your-writes for incremental replacement. Immutable
    /// input spans read the frozen SnapshotReader, including reconciliation
    /// overlays; newly prepared objects are read through `store` by the batch.
    pub(crate) fn compile<S: ObjectStore>(
        &self,
        store: &mut S,
        deferred_private_budget: usize,
    ) -> Result<CompiledFile> {
        if let Some(before) = self.before {
            if self.is_identity(before)? {
                return Ok(self.unchanged(before));
            }
            if matches!(&self.source, FrozenSource::Edited {base: Some((root, _)), ..} if *root == before)
            {
                return self.compile_incremental(store, before, deferred_private_budget);
            }
            if self.matches_base(before)? {
                return Ok(self.unchanged(before));
            }
        }
        self.compile_full(store)
    }

    fn unchanged(&self, root: FileStateRoot) -> CompiledFile {
        CompiledFile {
            root,
            len: self.len,
            counters: RopeCounters::default(),
            preparation: FilePreparationMetrics::default(),
        }
    }

    fn is_identity(&self, before: FileStateRoot) -> Result<bool> {
        match &self.source {
            FrozenSource::Base { root } => Ok(*root == before),
            FrozenSource::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            } if *root == before && self.len == *base_len => {
                let mut iter = pieces.iter();
                let first = iter.next().transpose()?;
                Ok((self.len == 0 && first.is_none())
                    || (matches!(first, Some(Piece::Base {root: piece_root, offset: 0, len}) if piece_root == *root && len == *base_len)
                        && iter.next().transpose()?.is_none()))
            }
            _ => Ok(false),
        }
    }

    fn compile_full<S: ObjectStore>(&self, store: &mut S) -> Result<CompiledFile> {
        // This boundary belongs to the existing canonical CDC codec. It is not
        // a workload selector: below it every byte slice is exactly one chunk.
        let (root, counters) = if self.len < IO_BYTES as u64 {
            let mut bytes = [0_u8; IO_BYTES];
            let count = self.read_into(0, &mut bytes[..self.len as usize])?;
            if count != self.len as usize {
                return Err(StoreError::Integrity("frozen file length"));
            }
            rope::build_bytes(store, &bytes[..count])?
        } else {
            let mut reader = self.range_reader(0, self.len)?;
            let built = rope::build(store, &mut reader);
            let (root, counters) = built
                .map_err(|error| reader.original_error.take().unwrap_or_else(|| error.into()))?;
            if reader.offset != self.len {
                return Err(StoreError::Integrity("frozen file length"));
            }
            (root, counters)
        };
        Ok(CompiledFile {
            root,
            len: self.len,
            counters,
            preparation: FilePreparationMetrics::default(),
        })
    }

    fn compile_incremental<S: ObjectStore>(
        &self,
        store: &mut S,
        root: FileStateRoot,
        private_budget: usize,
    ) -> Result<CompiledFile> {
        let FrozenSource::Edited { pieces, .. } = &self.source else {
            unreachable!()
        };
        let original_len = self.source_state(root)?.logical_len;
        let mut batch =
            FileMutationBatch::new_private_preparation(store, Some(root), private_budget)?;
        let mut changed = false;
        let mut base_cursor = 0_u64;
        let mut final_cursor = 0_u64;
        let mut replacement_len = 0_u64;
        for piece in pieces.iter() {
            match piece? {
                Piece::Base {
                    root: piece_root,
                    offset,
                    len,
                } => {
                    if piece_root != root || offset < base_cursor {
                        return Err(StoreError::Integrity("Workspace base piece order"));
                    }
                    let delete_len = offset - base_cursor;
                    if self.replacement_differs(
                        root,
                        final_cursor,
                        base_cursor,
                        delete_len,
                        replacement_len,
                    )? {
                        let mut input = self.range_reader(final_cursor, replacement_len)?;
                        let result = batch.replace(final_cursor, delete_len, &mut input);
                        result.map_err(|error| {
                            input.original_error.take().unwrap_or_else(|| error.into())
                        })?;
                        changed = true;
                    }
                    final_cursor = final_cursor
                        .checked_add(replacement_len)
                        .and_then(|n| n.checked_add(len))
                        .ok_or(StoreError::InvalidInput("file length"))?;
                    replacement_len = 0;
                    base_cursor = offset
                        .checked_add(len)
                        .ok_or(StoreError::InvalidInput("file length"))?;
                }
                piece => {
                    replacement_len = replacement_len
                        .checked_add(piece.len())
                        .ok_or(StoreError::InvalidInput("file length"))?;
                }
            }
        }
        let delete_len = original_len
            .checked_sub(base_cursor)
            .ok_or(StoreError::Integrity("Workspace base piece order"))?;
        if self.replacement_differs(root, final_cursor, base_cursor, delete_len, replacement_len)? {
            let mut input = self.range_reader(final_cursor, replacement_len)?;
            let result = batch.replace(final_cursor, delete_len, &mut input);
            result.map_err(|error| input.original_error.take().unwrap_or_else(|| error.into()))?;
            changed = true;
        }
        if batch.logical_len()? != self.len {
            return Err(StoreError::Integrity("Workspace file mutation length"));
        }
        if !changed {
            return Ok(self.unchanged(root));
        }
        let (root, counters, preparation) = batch.finish_with_preparation_metrics()?;
        Ok(CompiledFile {
            root,
            len: self.len,
            counters,
            preparation,
        })
    }

    fn replacement_differs(
        &self,
        root: FileStateRoot,
        final_cursor: u64,
        base_cursor: u64,
        deleted: u64,
        replacement: u64,
    ) -> Result<bool> {
        if deleted == 0 && replacement == 0 {
            return Ok(false);
        }
        if deleted != replacement || final_cursor != base_cursor {
            return Ok(true);
        }
        self.range_matches_base(root, final_cursor, replacement)
            .map(|same| !same)
    }

    fn range_matches_base(&self, root: FileStateRoot, start: u64, len: u64) -> Result<bool> {
        let mut final_bytes = [0_u8; IO_BYTES];
        let mut base_bytes = [0_u8; IO_BYTES];
        let end = start
            .checked_add(len)
            .ok_or(StoreError::InvalidInput("file range"))?;
        let mut offset = start;
        while offset < end {
            let count = (end - offset).min(IO_BYTES as u64) as usize;
            if self.read_into(offset, &mut final_bytes[..count])? != count {
                return Err(StoreError::Integrity("frozen file length"));
            }
            read_base_into(&self.reader, root, offset, &mut base_bytes[..count])?;
            if final_bytes[..count] != base_bytes[..count] {
                return Ok(false);
            }
            offset += count as u64;
        }
        Ok(true)
    }

    fn matches_base(&self, root: FileStateRoot) -> Result<bool> {
        let base_len = self.source_state(root)?.logical_len;
        if base_len != self.len {
            return Ok(false);
        }
        // Exact chunked comparison preserves no-op semantics without a pair of
        // full-file digest passes or whole-file buffers.
        self.range_matches_base(root, 0, self.len)
    }

    pub(crate) fn read_into(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        if offset > self.len {
            return Err(StoreError::InvalidInput("file range"));
        }
        let count = output
            .len()
            .min(usize::try_from(self.len - offset).unwrap_or(usize::MAX));
        let output = &mut output[..count];
        match &self.source {
            FrozenSource::Base { root } => read_base_into(&self.reader, *root, offset, output)?,
            FrozenSource::Edited { pieces, spool, .. } => {
                let mut written = 0;
                for piece in pieces.iter_range(offset, offset + count as u64)? {
                    let piece = piece?;
                    let len = usize::try_from(piece.len())
                        .map_err(|_| StoreError::InvalidInput("file range"))?;
                    let part = output
                        .get_mut(written..written + len)
                        .ok_or(StoreError::Integrity("frozen piece length"))?;
                    match piece {
                        Piece::Base { root, offset, .. } => {
                            read_base_into(&self.reader, root, offset, part)?
                        }
                        Piece::Inline { bytes, offset, .. } => {
                            let start = usize::try_from(offset)
                                .map_err(|_| StoreError::InvalidInput("file range"))?;
                            part.copy_from_slice(
                                bytes
                                    .get(start..start + len)
                                    .ok_or(StoreError::Integrity("frozen inline range"))?,
                            );
                        }
                        Piece::Zero { .. } => part.fill(0),
                        Piece::Spool { offset, .. } => spool
                            .as_ref()
                            .ok_or(StoreError::Integrity("frozen spool descriptor"))?
                            .read_exact_at(part, offset)?,
                    }
                    written += len;
                }
                if written != count {
                    return Err(StoreError::Integrity("frozen piece length"));
                }
            }
        }
        Ok(count)
    }

    fn range_reader(&self, offset: u64, len: u64) -> Result<FrozenRangeReader<'_>> {
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= self.len)
            .ok_or(StoreError::InvalidInput("file range"))?;
        Ok(FrozenRangeReader {
            source: self,
            offset,
            end,
            original_error: None,
        })
    }
}

struct FrozenRangeReader<'a> {
    source: &'a FrozenFile,
    offset: u64,
    end: u64,
    original_error: Option<StoreError>,
}
impl Read for FrozenRangeReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output
            .len()
            .min(usize::try_from(self.end - self.offset).unwrap_or(usize::MAX));
        match self.source.read_into(self.offset, &mut output[..count]) {
            Ok(read) => {
                self.offset += read as u64;
                Ok(read)
            }
            Err(error) => {
                self.original_error = Some(error);
                Err(std::io::Error::other("frozen file source"))
            }
        }
    }
}

fn read_base_into(
    reader: &SnapshotReader,
    root: FileStateRoot,
    offset: u64,
    output: &mut [u8],
) -> Result<()> {
    if output.is_empty() {
        return Ok(());
    }
    let end = offset
        .checked_add(output.len() as u64)
        .ok_or(StoreError::InvalidInput("file range"))?;
    let mut sink = SliceSink { remaining: output };
    let source = BoundedSource::new(reader);
    let counters = source.result(rope::read_range(&source, root, offset..end, &mut sink))?;
    if !sink.remaining.is_empty() {
        return Err(StoreError::Integrity("frozen base length"));
    }
    reader.note_rope_read(counters)?;
    Ok(())
}

pub(crate) fn portable_metadata_bounded(
    reader: &SnapshotReader,
    root: ObjectId,
    kind: layerfs_content::tree::inode::InodeKind,
) -> Result<layerfs_content::tree::metadata::PortableMetadataV1> {
    let source = BoundedSource::new(reader);
    match crate::cow_tree::portable_metadata(&source, root, kind) {
        Ok(value) => Ok(value),
        Err(error) => Err(source.error.borrow_mut().take().unwrap_or(error)),
    }
}

// Default authenticated batching on this adapter reads one bounded canonical
// value at a time. It never invokes SnapshotReader's bulk materialization path.
struct BoundedSource<'a> {
    source: &'a dyn ObjectSource,
    error: RefCell<Option<StoreError>>,
}
impl<'a> BoundedSource<'a> {
    fn new(source: &'a dyn ObjectSource) -> Self {
        Self {
            source,
            error: RefCell::new(None),
        }
    }
    fn result<T>(&self, result: CoreResult<T>) -> Result<T> {
        result.map_err(|error| {
            self.error
                .borrow_mut()
                .take()
                .unwrap_or_else(|| error.into())
        })
    }
}
impl ObjectRead for BoundedSource<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.source
            .read_object_bounded(id, CANONICAL_BYTES)
            .map_err(|error| {
                *self.error.borrow_mut() = Some(error);
                CoreError::Io
            })
    }
}

struct SliceSink<'a> {
    remaining: &'a mut [u8],
}
impl Write for SliceSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining.len() {
            return Err(std::io::Error::other("frozen base overflow"));
        }
        let remaining = std::mem::take(&mut self.remaining);
        let (head, tail) = remaining.split_at_mut(bytes.len());
        head.copy_from_slice(bytes);
        self.remaining = tail;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROOT;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource, ObjectBuffer,
    };

    fn fixture(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-frozen-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new(label).unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace)
    }

    fn contents(store: &impl ObjectStore, root: FileStateRoot) -> Vec<u8> {
        let mut bytes = Vec::new();
        rope::read_all(store, root, &mut bytes).unwrap();
        bytes
    }

    #[test]
    fn bounded_source_rejects_oversize_and_preserves_original_error() {
        struct Failure(bool);
        impl ObjectSource for Failure {
            fn read_object(&self, _: ObjectId) -> Result<Vec<u8>> {
                panic!("unbounded source path used")
            }
            fn read_object_bounded(&self, _: ObjectId, maximum: usize) -> Result<Vec<u8>> {
                assert_eq!(maximum, CANONICAL_BYTES);
                if self.0 {
                    Err(StoreError::Core(CoreError::ObjectLimitExceeded))
                } else {
                    Err(StoreError::Io(std::io::Error::from_raw_os_error(28)))
                }
            }
        }
        let id = ObjectId::for_bytes(b"not loaded");
        let oversized = Failure(true);
        let source = BoundedSource::new(&oversized);
        assert!(matches!(
            source.result(source.get(id)),
            Err(StoreError::Core(CoreError::ObjectLimitExceeded))
        ));
        let failed = Failure(false);
        let source = BoundedSource::new(&failed);
        let Err(StoreError::Io(error)) = source.result(source.get(id)) else {
            panic!("original OS error erased")
        };
        assert_eq!(error.raw_os_error(), Some(28));
    }

    #[test]
    fn frozen_input_owns_mixed_pieces_and_positional_spool_descriptor() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FrozenFile>();
        let (root, mut workspace) = fixture("mixed-owned");
        let node = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        let original = (0..40000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        workspace.write(node, 0, &original).unwrap();
        workspace.commit().unwrap();
        let before = match workspace.nodes[&node].data {
            Data::File(FileData::Base { root, .. }) => root,
            _ => unreachable!(),
        };
        workspace.write(node, 3, b"SPOOL").unwrap();
        workspace
            .edit_many(
                node,
                vec![
                    (
                        12,
                        4,
                        crate::WorkspaceFileReplacement::Inline(b"INLINE".to_vec()),
                    ),
                    (30, 3, crate::WorkspaceFileReplacement::Zero(5)),
                ],
            )
            .unwrap();
        let expected = workspace
            .read(node, 0, workspace.attr(node).unwrap().size as usize)
            .unwrap();
        let frozen = FrozenFile::freeze(&workspace, node, Some(before)).unwrap();
        let old_path = match &workspace.nodes[&node].data {
            Data::File(FileData::Edited { spool, .. }) => spool.clone(),
            _ => unreachable!(),
        };
        let held_path = old_path.with_extension("held");
        std::fs::rename(&old_path, &held_path).unwrap();
        // A copied snapshot must not follow later immutable PieceTree replacement.
        let FrozenSource::Edited { pieces, .. } = &frozen.source else {
            unreachable!()
        };
        assert!(pieces
            .iter()
            .any(|piece| matches!(piece.unwrap(), Piece::Base { .. })));
        assert!(pieces
            .iter()
            .any(|piece| matches!(piece.unwrap(), Piece::Inline { .. })));
        assert!(pieces
            .iter()
            .any(|piece| matches!(piece.unwrap(), Piece::Zero { .. })));
        assert!(pieces
            .iter()
            .any(|piece| matches!(piece.unwrap(), Piece::Spool { .. })));
        for start in [0, 2, 3, 7, 11, 12, 17, 29, 30, 35, 39990, expected.len()] {
            let mut output = [0x55; 37];
            let count = frozen.read_into(start as u64, &mut output).unwrap();
            assert_eq!(
                &output[..count],
                &expected[start..expected.len().min(start + 37)]
            );
            assert!(output[count..].iter().all(|byte| *byte == 0x55));
        }
        let mut objects = ObjectBuffer::new(&frozen.reader).unwrap();
        let compiled = frozen.compile(&mut objects, 512).unwrap();
        assert_eq!(compiled.len, expected.len() as u64);
        assert_eq!(contents(&objects, compiled.root), expected);
        assert!(compiled.preparation.deferred_peak_bytes <= 512);
        drop(objects);
        std::fs::rename(held_path, old_path).unwrap();
        workspace
            .edit_many(
                node,
                vec![(0, 1, crate::WorkspaceFileReplacement::Inline(vec![255]))],
            )
            .unwrap();
        let mut first = [0];
        frozen.read_into(0, &mut first).unwrap();
        assert_eq!(first[0], expected[0]);
        drop(frozen);
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frozen_compiler_preserves_exact_reverted_edit_noop() {
        let (root, mut workspace) = fixture("reverted-noop");
        let node = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        let original = vec![b'a'; 20000];
        workspace.write(node, 0, &original).unwrap();
        workspace.commit().unwrap();
        let before = match workspace.nodes[&node].data {
            Data::File(FileData::Base { root, .. }) => root,
            _ => unreachable!(),
        };
        workspace.write(node, 128, b"changed!").unwrap();
        workspace.write(node, 128, &original[128..136]).unwrap();
        let frozen = FrozenFile::freeze(&workspace, node, Some(before)).unwrap();
        let mut objects = ObjectBuffer::new(&frozen.reader).unwrap();
        let compiled = frozen.compile(&mut objects, 512).unwrap();
        assert_eq!(compiled.root, before);
        assert_eq!(compiled.counters.cdc_bytes_scanned, 0);
        assert_eq!(compiled.preparation.private_puts, 0);
        assert_eq!(contents(&objects, compiled.root), original);
        drop(objects);
        drop(frozen);
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frozen_full_build_matches_canonical_cdc_boundaries() {
        let (root, mut workspace) = fixture("canonical-boundaries");
        for len in [0, IO_BYTES - 1, IO_BYTES, IO_BYTES + 1, 32768] {
            let node = workspace
                .create_file(ROOT, format!("file-{len}").as_bytes(), 0o640)
                .unwrap()
                .node;
            let expected = (0..len).map(|i| (i % 251) as u8).collect::<Vec<_>>();
            workspace.write(node, 0, &expected).unwrap();
            let frozen = FrozenFile::freeze(&workspace, node, None).unwrap();
            let mut objects = ObjectBuffer::new(&frozen.reader).unwrap();
            let compiled = frozen.compile(&mut objects, 512).unwrap();
            let mut reference = ObjectBuffer::new(&frozen.reader).unwrap();
            let (canonical, _) = rope::build(&mut reference, expected.as_slice()).unwrap();
            assert_eq!(compiled.root, canonical);
            assert_eq!(compiled.len, len as u64);
            assert_eq!(compiled.counters.cdc_bytes_scanned, len as u64);
            assert_eq!(contents(&objects, compiled.root), expected);
        }
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
}
