//! Independent retained-tree read-back, adapted from the frozen v4 verifier.
//! Complete paths/kinds/sizes; every tenth content path plus final path hashed.
#![allow(dead_code)]
use super::canonical_memo::{Memo, Reader as MetadataReader};
use super::producer::{kind_of, OpError};
use layerfs_content::{
    filesystem::{FilesystemRead, FilesystemRootId},
    inode_leaf::InodeKind,
    ObjectId,
};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};
#[derive(Default, Clone, Copy)]
struct VerifyTally {
    compared: u64,
    sampled: u64,
    files_read: u64,
    file_bytes: u64,
    symlinks_read: u64,
}
struct Sampler<'a> {
    reader: &'a dyn layerfs_content::object::AuthenticatedObjects,
    persistence: &'a dyn layerfs_storage::port::PackPersistence,
    oracle: &'a crate::workload::history::Oracle,
    tally: VerifyTally,
    metadata_memo: &'a RefCell<Memo>,
    digests: BTreeMap<ObjectId, ([u8; 32], u64)>,
    lengths: BTreeMap<ObjectId, u64>,
}
impl Sampler<'_> {
    fn run_complete(&mut self, root: FilesystemRootId) -> Result<VerifyTally, OpError> {
        let diagnostic_start = std::time::Instant::now();
        let metadata_reader = MetadataReader::new(self.reader, self.metadata_memo);
        let mut read = FilesystemRead::new(&metadata_reader, root)
            .map_err(|error| OpError::Product(format!("{error:?}")))?;
        let root_serial = read.root().root_inode().serial();
        let root_value = read
            .resolve_inode(root_serial)
            .map_err(|error| OpError::Product(format!("{error:?}")))?
            .value;
        if root_value.kind != InodeKind::Directory {
            return Err(OpError::Io("retained root is not a directory".into()));
        }
        let mut pending = vec![(Vec::<u8>::new(), root_value.content_root)];
        let mut directory_work =
            layerfs_content::filesystem::directory::DirectoryReadWork::default();
        let mut seen = BTreeSet::new();
        let files: Vec<_> = self
            .oracle
            .iter()
            .filter(|(_, e)| !e.is_directory())
            .collect();
        let selected: BTreeSet<Vec<u8>> = files
            .iter()
            .enumerate()
            .filter(|(index, _)| index % 10 == 0 || index + 1 == files.len())
            .map(|(_, (path, _))| (*path).clone())
            .collect();
        let mut file_entries = Vec::new();
        while !pending.is_empty() {
            let mut children = Vec::new();
            let breadth = std::mem::take(&mut pending);
            for batch in breadth.chunks(128) {
                let ids: Vec<_> = batch.iter().map(|(_, root)| *root).collect();
                let pages = self.reader.read_canonical_batch(&ids).map_err(|error| {
                    OpError::Product(format!("directory root batch: {error:?}"))
                })?;
                if pages.len() != batch.len() {
                    return Err(OpError::Io("directory root batch cardinality".into()));
                }
                for ((path, directory_root), bytes) in batch.iter().zip(pages) {
                    let page =
                        layerfs_content::filesystem::directory::decode_directory_page(&bytes)
                            .map_err(|error| {
                                OpError::Product(format!("directory page: {error:?}"))
                            })?;
                    let entries = match page {
                        layerfs_content::filesystem::directory::DirectoryPage::Leaf { entries } => {
                            entries
                        }
                        layerfs_content::filesystem::directory::DirectoryPage::Branch {
                            ..
                        } => {
                            let mut entries = Vec::new();
                            let mut after = None;
                            loop {
                                let page = layerfs_content::filesystem::directory::list_after(
                                    self.reader,
                                    layerfs_content::filesystem::DirectoryRoot(*directory_root),
                                    after.as_ref(),
                                    512,
                                    65_536,
                                    &mut directory_work,
                                )
                                .map_err(|error| OpError::Product(format!("{error:?}")))?;
                                entries.extend(page.entries);
                                after = page.continuation;
                                if after.is_none() {
                                    break;
                                }
                            }
                            entries
                        }
                    };
                    for (name, serial) in entries {
                        let mut child = path.clone();
                        if !child.is_empty() {
                            child.push(b'/');
                        }
                        child.extend_from_slice(name.as_str().as_bytes());
                        children.push((child, serial));
                    }
                }
            }
            // One C1 inode lookup wave covers a whole breadth of listed names.
            // The old per-directory lookup repeated the same table navigation.
            for chunk in children.chunks(512) {
                let serials: Vec<_> = chunk.iter().map(|(_, serial)| *serial).collect();
                let values = read
                    .lookup_inodes(&serials)
                    .map_err(|error| OpError::Product(format!("{error:?}")))?;
                for ((child, _), value) in chunk.iter().zip(values) {
                    if !seen.insert(child.clone()) || seen.len() > self.oracle.len() {
                        return Err(OpError::Io(
                            "duplicate, extra or cyclic retained path".into(),
                        ));
                    }
                    let expected = self.oracle.get(child).ok_or_else(|| {
                        OpError::Io(format!(
                            "unexpected retained path: {}",
                            String::from_utf8_lossy(child)
                        ))
                    })?;
                    let value = value.ok_or_else(|| OpError::Io("listed inode missing".into()))?;
                    let kind = kind_of(expected.mode);
                    if value.kind != kind {
                        return Err(OpError::Io(
                            "retained inode kind disagrees with corpus".into(),
                        ));
                    }
                    self.tally.compared += 1;
                    if kind == InodeKind::Directory {
                        pending.push((child.clone(), value.content_root));
                    } else {
                        file_entries.push((
                            child.clone(),
                            value.content_root,
                            kind,
                            expected.size,
                            expected.digest,
                            selected.contains(child),
                        ));
                    }
                }
            }
        }
        if seen.len() != self.oracle.len() {
            return Err(OpError::Io("missing retained paths".into()));
        }
        let walk_ns = diagnostic_start.elapsed().as_nanos();
        // Classify distinct file roots in bounded Store waves. Each wave's
        // declared logical bytes plus framing stay below 16 MiB, leaving ample
        // room under the Store's 32 MiB canonical-byte bound. The previous
        // per-root FileView issued one Store wave per distinct file root.
        let selected_roots: BTreeSet<ObjectId> = file_entries
            .iter()
            .filter(|(_, _, _, _, _, selected)| *selected)
            .map(|(_, id, _, _, _, _)| *id)
            .collect();
        let ids: Vec<_> = file_entries
            .iter()
            .map(|(_, id, _, _, _, _)| *id)
            .filter(|id| !self.lengths.contains_key(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut metadata = BTreeMap::new();
        for chunk in ids.chunks(512) {
            let mut located = Vec::new();
            self.persistence
                .locate(chunk, &mut located)
                .map_err(|error| OpError::Product(error.to_string()))?;
            for row in located {
                metadata.insert(
                    row.location.object_id,
                    (
                        row.location.role.code(),
                        row.location.canonical_length as u64,
                    ),
                );
            }
        }
        for (_, id, kind, _, _, _) in &file_entries {
            if self.lengths.contains_key(id) {
                continue;
            }
            let (role, canonical) = metadata
                .get(id)
                .ok_or_else(|| OpError::Io(format!("file root {id} absent from C2 metadata")))?;
            let overhead = match (*kind, *role) {
                (InodeKind::RegularFile, 1) => Some(23),
                (InodeKind::Symlink, 13) => Some(27),
                (InodeKind::RegularFile, 5) => None,
                _ => {
                    return Err(OpError::Io(format!(
                        "file root {id} has wrong stored role {role}"
                    )))
                }
            };
            if let Some(overhead) = overhead {
                let length = canonical
                    .checked_sub(overhead)
                    .ok_or_else(|| OpError::Io("stored file length underflow".into()))?;
                self.lengths.insert(*id, length);
            }
        }
        let missing: BTreeMap<ObjectId, (InodeKind, u64)> = file_entries
            .iter()
            .filter(|(_, id, _, _, _, _)| {
                !self.lengths.contains_key(id)
                    || (selected_roots.contains(id) && !self.digests.contains_key(id))
            })
            .map(|(_, id, kind, size, _, _)| (*id, (*kind, *size)))
            .collect();
        let pending: Vec<_> = missing.into_iter().collect();
        let mut offset = 0;
        while offset < pending.len() {
            let mut end = offset;
            let mut estimated = 0u64;
            while end < pending.len() && end - offset < 512 {
                let next = pending[end].1 .1.saturating_add(128);
                if end > offset && estimated.saturating_add(next) > 16 * 1024 * 1024 {
                    break;
                }
                estimated = estimated.saturating_add(next);
                end += 1;
            }
            let chunk = &pending[offset..end];
            let ids: Vec<_> = chunk.iter().map(|(id, _)| *id).collect();
            let canonical = self
                .reader
                .read_canonical_batch(&ids)
                .map_err(|error| OpError::Product(format!("file length batch: {error:?}")))?;
            if canonical.len() != chunk.len() {
                return Err(OpError::Io("file length batch cardinality".into()));
            }
            for ((id, (kind, _)), bytes) in chunk.iter().zip(canonical) {
                let selected = selected_roots.contains(id) && !self.digests.contains_key(id);
                let (length, digest) = if *kind == InodeKind::Symlink {
                    let target = layerfs_content::filesystem::SymlinkTarget::decode(&bytes)
                        .map_err(|error| OpError::Product(format!("symlink length: {error:?}")))?;
                    (
                        target.as_bytes().len() as u64,
                        selected.then(|| crate::workload::digest::sha256(target.as_bytes())),
                    )
                } else {
                    let length = layerfs_content::file::classify(&bytes)
                        .map_err(|error| OpError::Product(format!("file length: {error:?}")))?
                        .logical_len();
                    let digest = if selected {
                        layerfs_content::file::whole_file_payload(&bytes)
                            .map_err(|error| {
                                OpError::Product(format!("whole-file bytes: {error:?}"))
                            })?
                            .map(crate::workload::digest::sha256)
                    } else {
                        None
                    };
                    (length, digest)
                };
                if self.lengths.get(id).is_some_and(|stored| *stored != length) {
                    return Err(OpError::Io(format!(
                        "stored and public file length disagree: {id}"
                    )));
                }
                self.lengths.insert(*id, length);
                if let Some(digest) = digest {
                    self.digests.insert(*id, (digest, length));
                    self.tally.files_read += 1;
                    self.tally.file_bytes += length;
                    if *kind == InodeKind::Symlink {
                        self.tally.symlinks_read += 1;
                    }
                }
            }
            offset = end;
        }
        let lengths_ns = diagnostic_start.elapsed().as_nanos() - walk_ns;
        for (path, id, kind, expected_size, expected_digest, sampled) in file_entries {
            if self.lengths.get(&id) != Some(&expected_size) {
                return Err(OpError::Io(format!(
                    "retained file size disagrees with corpus: {}",
                    String::from_utf8_lossy(&path)
                )));
            }
            if sampled {
                self.tally.sampled += 1;
                let (digest, bytes) = self.digest_of(id, &String::from_utf8_lossy(&path), kind)?;
                if Some(digest) != expected_digest || bytes != expected_size {
                    return Err(OpError::Io(
                        "retained sampled bytes disagree with corpus".into(),
                    ));
                }
            }
        }
        if std::env::var("LAYERFS_HISTORY_VERIFY_PROGRESS").as_deref() == Ok("1") {
            eprintln!(
                "verify parts: walk={walk_ns} lengths={lengths_ns} digest={}",
                diagnostic_start.elapsed().as_nanos() - walk_ns - lengths_ns
            );
        }
        Ok(self.tally)
    }

    fn digest_of(
        &mut self,
        id: ObjectId,
        path: &str,
        kind: InodeKind,
    ) -> Result<([u8; 32], u64), OpError> {
        if let Some(found) = self.digests.get(&id).copied() {
            return Ok(found);
        }
        // The oracle hashes a file's **logical bytes**, not the canonical object
        // that holds them. Reading the canonical object directly is off by the
        // encoding's own header — 23 bytes on every file in this corpus, which is
        // how this phase first reported 85,929 mismatches that were all the same
        // fixed difference. `read_all` is the product's logical read path and is
        // what the claim is about.
        let (found, length) = if kind == InodeKind::Symlink {
            self.tally.symlinks_read = self.tally.symlinks_read.saturating_add(1);
            // A symlink's logical bytes are its target string; its oracle digest
            // is over that, not over the framing `SymlinkTarget::encode` adds.
            let canonical = self
                .reader
                .read_canonical(id)
                .map_err(|error| OpError::Product(format!("{path}: reading {id}: {error:?}")))?;
            let target = layerfs_content::filesystem::SymlinkTarget::decode(&canonical)
                .map_err(|error| OpError::Product(format!("{path}: symlink target: {error:?}")))?;
            (
                crate::workload::digest::sha256(target.as_bytes()),
                target.as_bytes().len() as u64,
            )
        } else {
            let mut sink = crate::workload::oracle::HashingSink::new();
            let (result, _) = Timing::disabled("verify.read", |scope: &TimingScope<'_, Active>| {
                layerfs_content::read_all(
                    self.reader,
                    id,
                    &mut sink,
                    scope.child("content.acquire"),
                )
            });
            result.map_err(|error| OpError::Product(format!("{path}: reading {id}: {error:?}")))?;
            let bytes = sink.bytes();
            (sink.finish(), bytes)
        };
        self.tally.files_read = self.tally.files_read.saturating_add(1);
        self.tally.file_bytes = self.tally.file_bytes.saturating_add(length);
        self.digests.insert(id, (found, length));
        Ok((found, length))
    }
}

#[derive(Default)]
pub struct Reuse {
    metadata_memo: RefCell<Memo>,
    digests: BTreeMap<ObjectId, ([u8; 32], u64)>,
    lengths: BTreeMap<ObjectId, u64>,
}
impl Reuse {
    pub fn metadata_counters(&self) -> super::canonical_memo::Counters {
        self.metadata_memo.borrow().counters()
    }
    pub fn check(
        &mut self,
        reader: &dyn layerfs_content::object::AuthenticatedObjects,
        persistence: &dyn layerfs_storage::port::PackPersistence,
        oracle: &crate::workload::history::Oracle,
        root: ObjectId,
    ) -> Result<(u64, u64, u64), OpError> {
        let mut sampler = Sampler {
            reader,
            persistence,
            oracle,
            tally: VerifyTally::default(),
            metadata_memo: &self.metadata_memo,
            digests: std::mem::take(&mut self.digests),
            lengths: std::mem::take(&mut self.lengths),
        };
        let tally = sampler.run_complete(FilesystemRootId(root))?;
        self.digests = sampler.digests;
        self.lengths = sampler.lengths;
        Ok((tally.compared, tally.sampled, tally.file_bytes))
    }
}
