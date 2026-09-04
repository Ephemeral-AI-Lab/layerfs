//! Verification-only typed Store graph, complete namespace and independent bytes.
use super::*;
use crate::workload_source::workspace_common::{
    self as common, Content, Entry, EntryKind, Receipt,
};
use layerfs_content::file::{extent::ExtentNodeV3, extent_codec, rope};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::tree::{directory, inode, metadata};
use layerfs_content::{CanonicalPath, ObjectId};
use layerfs_layerstack_store::{CoreReader, ObjectSource};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::Stdio;

pub(crate) fn write_gzip(
    path: &Path,
    write: impl FnOnce(&mut dyn Write) -> AnyResult<()>,
) -> AnyResult<()> {
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut child = Command::new("/usr/bin/gzip")
        .args(["-n", "-6", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output))
        .stderr(Stdio::piped())
        .spawn()?;
    let written = {
        let mut input = std::io::BufWriter::new(child.stdin.take().ok_or("gzip input pipe")?);
        write(&mut input).and_then(|()| {
            input.flush()?;
            Ok(())
        })
    };
    if written.is_err() {
        let _ = child.kill();
    }
    let finished = child.wait_with_output()?;
    if !finished.stderr.is_empty() {
        std::fs::write(path.with_extension("gz.stderr.txt"), &finished.stderr)?;
    }
    written?;
    if !finished.status.success() {
        return Err(format!("canonical artifact gzip failed: {}", finished.status).into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct Extent {
    pub id: ObjectId,
    pub source_offset: u64,
    pub len: u64,
    pub payload_len: u64,
}
#[derive(Debug)]
pub(crate) struct SnapshotEvidence {
    pub receipt: Receipt,
    pub extents: BTreeMap<String, Vec<Extent>>,
    pub file_roots: BTreeMap<String, ObjectId>,
    pub canonical_objects: BTreeMap<ObjectId, CanonicalObject>,
    independent_manifest: String,
}

pub(crate) fn verify(
    store: &LayerStackStore,
    branch: BranchId,
    entries: &[Entry],
    evidence: &Path,
) -> AnyResult<SnapshotEvidence> {
    let pinned = store.pin_branch(branch)?;
    let mut result = verify_root(&pinned.reader, pinned.root, entries)?;
    persist_snapshot(entries, &mut result, evidence)?;
    Ok(result)
}

pub(crate) fn persist_snapshot(
    entries: &[Entry],
    result: &mut SnapshotEvidence,
    evidence: &Path,
) -> AnyResult<()> {
    let evidence = evidence.join("canonical-verification");
    if evidence.exists() {
        return Err("canonical verifier evidence already exists".into());
    }
    std::fs::create_dir_all(&evidence)?;
    write_gzip(&evidence.join("payload-extents.tsv.gz"), |rows| {
        writeln!(
            rows,
            "path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length"
        )?;
        for (path, extents) in &result.extents {
            for (index, extent) in extents.iter().enumerate() {
                writeln!(
                    rows,
                    "{path}\t{index}\t{}\t{}\t{}\t{}",
                    extent.id, extent.source_offset, extent.len, extent.payload_len
                )?;
            }
        }
        Ok(())
    })?;
    write_gzip(&evidence.join("file-roots.tsv.gz"), |roots| {
        writeln!(roots, "path\tcontent_root")?;
        for (path, root) in &result.file_roots {
            writeln!(roots, "{path}\t{root}")?;
        }
        Ok(())
    })?;
    let manifest_path = evidence.join(
        if entries
            .iter()
            .any(|entry| matches!(entry.kind, EntryKind::File(Content::Digest { .. })))
        {
            "persistence-bound-manifest.tsv.gz"
        } else {
            "independent-manifest.tsv.gz"
        },
    );
    write_gzip(&manifest_path, |writer| {
        writer.write_all(result.independent_manifest.as_bytes())?;
        Ok(())
    })?;
    result
        .receipt
        .insert("artifact_encoding".into(), "gzip-v1".into());
    result.receipt.insert(
        "artifact_compressor".into(),
        "/usr/bin/gzip -n -6 -c".into(),
    );
    let mut receipt = std::fs::File::create(evidence.join("canonical-receipt.txt"))?;
    for (key, value) in &result.receipt {
        writeln!(receipt, "{key}={value}")?;
    }
    Ok(())
}

/// Read the complete authenticated global inode index once per immutable proof.
/// Directory entries already carry inode IDs, so callers need not resolve each
/// path from the root again. This does not certify or skip any file content.
struct AuthenticatedNamespaceIndex {
    root_inode: inode::InodeId,
    records: BTreeMap<inode::InodeId, inode::InodeRecordV1>,
}
impl AuthenticatedNamespaceIndex {
    fn load(source: &dyn ObjectSource, root: ObjectId) -> AnyResult<Self> {
        let reader = CoreReader(source);
        let namespace = layerfs_content::filesystem::namespace(&reader, root)?;
        let mut records = BTreeMap::new();
        let index = inode::inode_table_entries(&reader, inode::InodeTableRoot(namespace.inode_table_root), &mut Default::default())?;
        for batch in index.chunks(16) {
            let ids = batch.iter().map(|(_,id)| *id).collect::<Vec<_>>();
            // ObjectRead::get_authenticated_batch passes decoded byte-object payloads;
            // inode codecs require the complete canonical envelope. Keep that envelope.
            let objects = source.read_authenticated_objects(&ids)?;
            if objects.len() != batch.len() { return Err("canonical global inode batch cardinality".into()); }
            for ((inode_id,expected_id),object) in batch.iter().zip(objects) {
                if object.id != *expected_id { return Err("canonical global inode batch identity".into()); }
                layerfs_content::authenticate_identity(&object.bytes,object.id)?;
                let record = inode::codec::decode_inode_record(&object.bytes)?;
                if records.insert(*inode_id,record).is_some() { return Err("canonical duplicate global inode".into()); }
            }
        }
        drop(index);
        Ok(Self {root_inode:namespace.root_directory_inode,records})
    }
    fn resolve_inode(&self, id: inode::InodeId) -> AnyResult<layerfs_content::filesystem::Resolved> {
        let record = *self.records.get(&id).ok_or("namespace references missing global inode")?;
        Ok(layerfs_content::filesystem::Resolved {inode:id,record})
    }
    fn require_complete_membership(&self, reached: &BTreeSet<inode::InodeId>) -> AnyResult<()> {
        if self.records.keys().copied().ne(reached.iter().copied()) {
            return Err("canonical inode table has missing or unreachable entries".into());
        }
        Ok(())
    }
}

pub(crate) fn verify_root(
    source: &dyn ObjectSource,
    root: ObjectId,
    entries: &[Entry],
) -> AnyResult<SnapshotEvidence> {
    let logical = common::validate_entries(entries)?;
    let reader = CoreReader(source);
    let namespace = AuthenticatedNamespaceIndex::load(source,root)?;
    let expected = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut reference_counts = BTreeMap::<&str, u64>::new();
    for entry in entries {
        let class = match &entry.kind {
            EntryKind::File(_) => entry.path.as_str(),
            EntryKind::Hardlink(target) => target.as_str(),
            _ => continue,
        };
        *reference_counts.entry(class).or_default() += 1;
    }
    let mut found = BTreeSet::new();
    let mut pending = vec![(".".to_owned(),namespace.root_inode)];
    let mut inode_classes = BTreeMap::new();
    let mut class_inodes = BTreeMap::new();
    let mut namespace_inodes = BTreeSet::new();
    let mut extents = BTreeMap::new();
    let mut file_roots = BTreeMap::new();
    let mut custody_paths = 0;
    let mut validated_metadata = BTreeSet::new();
    let mut comparison_scratch = vec![0; common::SCRATCH_BYTES];
    while let Some((path,id)) = pending.pop() {
        if !found.insert(path.clone()) {
            return Err("canonical path repeated".into());
        }
        let entry = expected
            .get(path.as_str())
            .ok_or_else(|| format!("extra canonical path: {path}"))?;
        let _canonical = if path == "." {
            CanonicalPath::root()
        } else {
            CanonicalPath::new(&path)?
        };
        let resolved = namespace.resolve_inode(id)?;
        if !namespace_inodes.insert(id) && resolved.record.kind != inode::InodeKind::RegularFile {
            return Err("canonical namespace repeats a non-regular inode".into());
        }
        // Record/root rules and the independent expected binding remain per-path.
        // Reuse only successful validation of the identical immutable metadata.
        resolved.record.validate(path == ".")?;
        let metadata_key = (resolved.record.metadata_root, resolved.record.kind as u8,
            entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds);
        if !validated_metadata.contains(&metadata_key) {
            directory::validate_inode_record_metadata(&reader, resolved.record, path == ".")?;
            verify_metadata(&reader, resolved.record.metadata_root, entry)?;
            validated_metadata.insert(metadata_key);
        }
        match &entry.kind {
            EntryKind::Directory => {
                if resolved.record.kind != inode::InodeKind::Directory {
                    return Err(format!("canonical directory type: {path}").into());
                }
                let mut after = None;
                loop {
                    let page = directory::directory_page_after(
                        &reader,
                        directory::DirectoryStateRoot(resolved.record.content_root),
                        after.as_ref(),
                        127,
                        8192,
                        &mut Default::default(),
                    )?;
                    for (name, child_inode) in &page.entries {
                        let child = if path == "." {
                            name.as_str().to_owned()
                        } else {
                            format!("{path}/{}", name.as_str())
                        };
                        pending.push((child,*child_inode));
                    }
                    match page.continuation {
                        Some(next) => {
                            if after.as_ref().is_some_and(|previous| previous >= &next) {
                                return Err("canonical directory cursor did not advance".into());
                            }
                            after = Some(next);
                        }
                        None => break,
                    }
                }
            }
            EntryKind::Symlink(target) => {
                if resolved.record.kind != inode::InodeKind::Symlink
                    || reader.with_authenticated_canonical(resolved.record.content_root,
                        directory::codec::decode_symlink)?.target != target.as_bytes()
                {
                    return Err(format!("canonical symlink target/type: {path}").into());
                }
            }
            EntryKind::File(_) | EntryKind::Hardlink(_) => {
                let (content, class) = match &entry.kind {
                    EntryKind::File(content) => (content, path.as_str()),
                    EntryKind::Hardlink(target) => {
                        let EntryKind::File(content) = &expected[target.as_str()].kind else {
                            return Err("canonical hard-link oracle".into());
                        };
                        (content, target.as_str())
                    }
                    _ => unreachable!(),
                };
                if resolved.record.kind != inode::InodeKind::RegularFile {
                    return Err(format!("canonical regular-file type: {path}").into());
                }
                if inode_classes
                    .insert(resolved.inode, class.to_owned())
                    .is_some_and(|previous| previous != class)
                    || class_inodes
                        .insert(class.to_owned(), resolved.inode)
                        .is_some_and(|previous| previous != resolved.inode)
                {
                    return Err(format!("canonical hard-link class mismatch: {path}").into());
                }
                if resolved.record.namespace_ref_count != reference_counts[class] {
                    return Err(format!("canonical hard-link reference count: {path}").into());
                }
                let file_root = rope::FileStateRoot(resolved.record.content_root);
                file_roots.insert(path.clone(), resolved.record.content_root);
                rope::validate_file(&reader, file_root)?;
                let state = rope::state(&reader, file_root, &mut Default::default())?;
                if state.logical_len != content.len() {
                    return Err(format!("canonical length: {path}").into());
                }
                let mut sink = CompareSink {
                    expected: content,
                    offset: 0,
                    scratch: &mut comparison_scratch,
                    custody_hash: matches!(content, Content::Digest { .. })
                        .then(workload_source::Sha256::new),
                };
                custody_paths += usize::from(sink.custody_hash.is_some());
                rope::read_all(&reader, file_root, &mut sink)?;
                if sink.offset != content.len() {
                    return Err(format!("canonical short file: {path}").into());
                }
                if let (Some(hash), Content::Digest { sha256, .. }) = (sink.custody_hash, content) {
                    if workload_source::hex(&hash.finish()) != *sha256 {
                        return Err(format!("canonical persistence digest mismatch: {path}").into());
                    }
                }
                let mut file_extents = Vec::new();
                rope::visit_extents(&reader, file_root, |page| {
                    for extent in page {
                        let payload_len = reader.with_authenticated_canonical(
                            extent.payload_object_id,
                            |canonical| {
                                Ok(extent_codec::decode_chunk_payload(
                                    layerfs_content::decode_bytes_object(canonical)?,
                                )?
                                .len() as u64)
                            },
                        )?;
                        let source_offset = u64::from(extent.source_offset);
                        let len = u64::from(extent.logical_length);
                        if source_offset
                            .checked_add(len)
                            .is_none_or(|end| end > payload_len)
                        {
                            return Err(layerfs_content::CoreError::InvalidRecord(
                                "extent payload bound",
                            ));
                        }
                        file_extents.push(Extent {
                            id: extent.payload_object_id,
                            source_offset,
                            len,
                            payload_len,
                        });
                    }
                    Ok(())
                })?;
                extents.insert(path, file_extents);
            }
        }
    }
    if found.iter().map(String::as_str).collect::<BTreeSet<_>>()
        != expected.keys().copied().collect()
    {
        return Err("complete canonical path-set mismatch".into());
    }
    namespace.require_complete_membership(&namespace_inodes)?;
    drop(namespace);
    let (mut receipt, canonical_objects) = typed_census(source, root)?;
    receipt.insert("verification_status".into(), "pass".into());
    receipt.insert("canonical_root".into(), root.to_string());
    receipt.insert("verified_paths".into(), entries.len().to_string());
    receipt.insert("verified_regular_paths".into(), extents.len().to_string());
    receipt.insert("logical_bytes".into(), logical.to_string());
    receipt.insert(
        "persistence_custody_paths".into(),
        custody_paths.to_string(),
    );
    receipt.insert(
        "independent_content_paths".into(),
        (extents.len() - custody_paths).to_string(),
    );
    receipt.insert(
        "oracle_scope".into(),
        if custody_paths == 0 {
            "independent-source"
        } else {
            "independent-source-plus-precommit-persistence-custody"
        }
        .into(),
    );
    let independent_manifest = common::manifest(entries)?;
    receipt.insert(
        "oracle_identity".into(),
        workload_source::sdk_edit_common::sha256_hex(independent_manifest.as_bytes()),
    );
    Ok(SnapshotEvidence {
        receipt,
        extents,
        file_roots,
        canonical_objects,
        independent_manifest,
    })
}

struct CompareSink<'a> {
    expected: &'a Content,
    offset: u64,
    scratch: &'a mut [u8],
    custody_hash: Option<workload_source::Sha256>,
}
impl Write for CompareSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Some(hash) = &mut self.custody_hash {
            let end = self
                .offset
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| std::io::Error::other("canonical custody length overflow"))?;
            if end > self.expected.len() {
                return Err(std::io::Error::other("canonical custody length exceeded"));
            }
            hash.update(bytes);
            self.offset = end;
            return Ok(bytes.len());
        }
        let mut cursor = 0;
        while cursor < bytes.len() {
            let amount = self.scratch.len().min(bytes.len() - cursor);
            let count = self
                .expected
                .read_at(self.offset, &mut self.scratch[..amount])
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            if count != amount || self.scratch[..count] != bytes[cursor..cursor + count] {
                return Err(std::io::Error::other(format!(
                    "canonical independent content mismatch at {}",
                    self.offset
                )));
            }
            cursor += count;
            self.offset += count as u64;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn verify_metadata(reader: &CoreReader<'_>, root: ObjectId, expected: &Entry) -> AnyResult<()> {
    let entries = metadata::metadata_tree_entries(reader, root)?;
    let mut observed = BTreeMap::new();
    for entry in entries {
        if entry.key.domain != "portable" || !matches!(entry.key.key.as_slice(), b"mode" | b"mtime")
        {
            return Err(format!("unexpected canonical metadata: {}", expected.path).into());
        }
        let maximum = if entry.key.key == b"mode" { 4 } else { 12 };
        let state = rope::state(
            reader,
            rope::FileStateRoot(entry.value_file_root),
            &mut Default::default(),
        )?;
        if state.logical_len != maximum {
            return Err("canonical metadata length".into());
        }
        let mut value = Vec::new();
        rope::read_all(
            reader,
            rope::FileStateRoot(entry.value_file_root),
            &mut value,
        )?;
        if observed.insert(entry.key.key, value).is_some() {
            return Err("duplicate canonical metadata".into());
        }
    }
    let mut timestamp = expected.mtime_seconds.to_be_bytes().to_vec();
    timestamp.extend(expected.mtime_nanoseconds.to_be_bytes());
    if observed.len() != 2
        || observed.get(b"mode".as_slice()).map(Vec::as_slice)
            != Some(expected.mode.to_be_bytes().as_slice())
        || observed.get(b"mtime".as_slice()) != Some(&timestamp)
    {
        return Err(format!("canonical metadata mismatch: {}", expected.path).into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Role {
    Namespace,
    InodeTable,
    InodeRecord,
    DirectoryState,
    DirectoryNode,
    Metadata,
    FileState,
    FileNode,
    Chunk,
    Symlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalObject {
    pub role: Role,
    pub canonical_bytes: u64,
    pub regular_file: bool,
    pub metadata_value: bool,
}
impl CanonicalObject {
    pub(crate) fn regular_payload(&self) -> bool {
        self.role == Role::Chunk && self.regular_file
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Origin {
    Structure,
    RegularFile,
    MetadataValue,
}

pub(crate) fn typed_census(
    source: &dyn ObjectSource,
    root: ObjectId,
) -> AnyResult<(Receipt, BTreeMap<ObjectId, CanonicalObject>)> {
    let mut pending = vec![(root, Role::Namespace, Origin::Structure)];
    let mut visits = BTreeSet::new();
    let mut seen = BTreeMap::<ObjectId, CanonicalObject>::new();
    while !pending.is_empty() {
        // Existing ObjectSource batching; bounded even for a malformed maximum-size
        // object (16 * MAX_OBJECT_BYTES). Every object is still authenticated.
        let mut batch = Vec::with_capacity(16);
        while batch.len() < 16 {
            let Some(item) = pending.pop() else { break };
            if visits.insert(item) { batch.push(item); }
        }
        if batch.is_empty() { continue }
        let ids = batch.iter().map(|item| item.0).collect::<Vec<_>>();
        let objects = source.read_authenticated_objects(&ids)?;
        if objects.len() != batch.len() { return Err("canonical authenticated batch cardinality".into()); }
        for ((id, role, origin), object) in batch.into_iter().zip(objects) {
        if object.id != id { return Err("canonical authenticated batch identity".into()); }
        let bytes = &object.bytes;
        layerfs_content::authenticate_identity(bytes, id)?;
        let observed = CanonicalObject {
            role,
            canonical_bytes: bytes.len() as u64,
            regular_file: origin == Origin::RegularFile,
            metadata_value: origin == Origin::MetadataValue,
        };
        match seen.entry(id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(observed);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let previous = entry.get_mut();
                if previous.role != role || previous.canonical_bytes != observed.canonical_bytes {
                    return Err(
                        "canonical object referenced with incompatible roles/lengths".into(),
                    );
                }
                previous.regular_file |= observed.regular_file;
                previous.metadata_value |= observed.metadata_value;
            }
        }
        match role {
            Role::Namespace => pending.push((
                directory::codec::decode_namespace_root(bytes)?.inode_table_root,
                Role::InodeTable,
                Origin::Structure,
            )),
            Role::InodeTable => match inode::codec::decode_inode_table_node(bytes)? {
                inode::codec::InodeTableNodeV1::Leaf(entries) => pending.extend(
                    entries
                        .into_iter()
                        .map(|(_, id)| (id, Role::InodeRecord, Origin::Structure)),
                ),
                inode::codec::InodeTableNodeV1::Branch { children, .. } => pending.extend(
                    children
                        .into_iter()
                        .map(|(_, id)| (id, Role::InodeTable, Origin::Structure)),
                ),
            },
            Role::InodeRecord => {
                let record = inode::codec::decode_inode_record(bytes)?;
                pending.push((record.metadata_root, Role::Metadata, Origin::Structure));
                let (content_role, content_origin) = match record.kind {
                    inode::InodeKind::RegularFile => (Role::FileState, Origin::RegularFile),
                    inode::InodeKind::Directory => (Role::DirectoryState, Origin::Structure),
                    inode::InodeKind::Symlink => (Role::Symlink, Origin::Structure),
                };
                pending.push((record.content_root, content_role, content_origin));
            }
            Role::DirectoryState => pending.push((
                directory::codec::decode_directory_state(bytes)?.mapping_root,
                Role::DirectoryNode,
                Origin::Structure,
            )),
            Role::DirectoryNode => {
                if let directory::codec::DirectoryNodeV1::Branch { children, .. } =
                    directory::codec::decode_directory_node(bytes)?
                {
                    pending.extend(
                        children
                            .into_iter()
                            .map(|(_, id)| (id, Role::DirectoryNode, Origin::Structure)),
                    );
                }
            }
            Role::Metadata => match metadata::codec::decode_metadata_node(bytes)? {
                metadata::codec::MetadataNodeV1::Leaf { entries, .. } => {
                    pending.extend(entries.into_iter().map(|entry| {
                        (
                            entry.value_file_root,
                            Role::FileState,
                            Origin::MetadataValue,
                        )
                    }))
                }
                metadata::codec::MetadataNodeV1::Branch { children, .. } => pending.extend(
                    children
                        .into_iter()
                        .map(|(_, id)| (id, Role::Metadata, Origin::Structure)),
                ),
            },
            Role::FileState => pending.push((
                extent_codec::decode_file_state(bytes)?.mapping_root,
                Role::FileNode,
                origin,
            )),
            Role::FileNode => match extent_codec::decode_node(bytes)? {
                ExtentNodeV3::Leaf { extents, .. } => pending.extend(
                    extents
                        .into_iter()
                        .map(|extent| (extent.payload_object_id, Role::Chunk, origin)),
                ),
                ExtentNodeV3::Branch { children, .. } => pending.extend(
                    children
                        .into_iter()
                        .map(|child| (child.child_object_id, Role::FileNode, origin)),
                ),
            },
            Role::Chunk => {
                extent_codec::decode_chunk_payload(layerfs_content::decode_bytes_object(bytes)?)?;
            }
            Role::Symlink => {
                directory::codec::decode_symlink(bytes)?;
            }
        }
    }
    }
    let mut totals = BTreeMap::<Role, (u64, u64)>::new();
    let mut canonical_bytes = 0u64;
    for object in seen.values() {
        let total = totals.entry(object.role).or_default();
        total.0 = total.0.checked_add(1).ok_or("canonical count overflow")?;
        total.1 = total
            .1
            .checked_add(object.canonical_bytes)
            .ok_or("canonical role bytes overflow")?;
        canonical_bytes = canonical_bytes
            .checked_add(object.canonical_bytes)
            .ok_or("canonical bytes overflow")?;
    }
    let mut receipt = Receipt::new();
    for (role, (count, bytes)) in totals {
        receipt.insert(format!("canonical_{role:?}_objects"), count.to_string());
        receipt.insert(format!("canonical_{role:?}_bytes"), bytes.to_string());
    }
    receipt.insert("canonical_unique_objects".into(), seen.len().to_string());
    receipt.insert("canonical_unique_bytes".into(), canonical_bytes.to_string());
    receipt.insert("canonical_role_status".into(), "pass".into());
    Ok((receipt, seen))
}

/// Certificate references a fully verified pristine input, not reads in this run.
pub(crate) struct FastCertificate {
    pub binding: String,
    root: ObjectId,
    file_roots: BTreeMap<String, ObjectId>,
}
fn read_gzip_text(path: &Path) -> AnyResult<String> {
    let output = Command::new("/usr/bin/gzip").args(["-dc"]).arg(path).output()?;
    if !output.status.success() { return Err("fast certificate gzip failed".into()); }
    Ok(String::from_utf8(output.stdout)?)
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn require_bound_bytes(bytes: &[u8], expected: &str, scope: &str) -> AnyResult<()> {
    if !hex_digest(expected) || workload_source::sdk_edit_common::sha256_hex(bytes) != expected {
        return Err(format!("fast certificate {scope} hash mismatch").into());
    }
    Ok(())
}
fn require_bound_file(path: &Path, expected: &str, scope: &str) -> AnyResult<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    require_bound_bytes(&bytes,expected,scope)?;
    Ok(bytes)
}
impl FastCertificate {
    fn require_pristine_root(&self, root: ObjectId) -> AnyResult<()> {
        if self.root != root { return Err("fast certificate pristine root mismatch".into()); }
        Ok(())
    }
    pub(crate) fn load(seed: u8, pristine_root: ObjectId, fixture: &[Entry]) -> AnyResult<Self> {
        let projection = require_bound_file(
            Path::new(&std::env::var("LAYERFS_V013_FAST_CERTIFICATE")?),
            &std::env::var("LAYERFS_V013_FAST_CERTIFICATE_SHA256")?, "TSV projection",
        )?;
        let text = String::from_utf8(projection)?;
        let mut fields = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line.split_once('\t').ok_or("fast certificate TSV columns")?;
            if value.is_empty() || fields.insert(key, value).is_some() { return Err("fast certificate duplicate/empty field".into()); }
        }
        let get = |key| fields.get(key).copied().ok_or("missing fast certificate field");
        if get("profile")? != "fast-verify-v1" || get("seed")?.parse::<u8>()? != seed
            || get("input_plan_sha256")? != std::env::var("LAYERFS_V013_FAST_INPUT_PLAN_SHA256")? {
            return Err("fast certificate profile/seed/input mismatch".into());
        }
        let binding = get("certificate_sha256")?.to_owned();
        if !hex_digest(&binding) || workload_source::sdk_edit_common::sha256_hex(
            &std::fs::read(get("certificate_json")?)?) != binding {
            return Err("fast certificate binding mismatch".into());
        }
        for key in ["source_attempt", "source_revision", "product_seal", "certificate_manifest_sha256"] { get(key)?; }
        let root = get("root")?.parse()?;
        if root != pristine_root { return Err("fast certificate pristine root mismatch".into()); }
        require_bound_file(Path::new(get("certificate_manifest")?), get("certificate_manifest_file_sha256")?, "compressed manifest")?;
        require_bound_file(Path::new(get("certificate_file_roots")?), get("certificate_file_roots_sha256")?, "compressed file roots")?;
        let manifest = read_gzip_text(Path::new(get("certificate_manifest")?))?;
        if workload_source::sdk_edit_common::sha256_hex(manifest.as_bytes()) != get("oracle_identity")? {
            return Err("fast certificate independent manifest identity".into());
        }
        let certified = common::decode_manifest(&manifest)?;
        let expected = fixture.iter().map(|e| (e.path.as_str(), e)).collect::<BTreeMap<_, _>>();
        if certified.len() != expected.len() { return Err("fast certificate input membership".into()); }
        for entry in &certified {
            let wanted = expected.get(entry.path.as_str()).ok_or("fast certificate extra input path")?;
            let kinds_match = match (&entry.kind, &wanted.kind) {
                (EntryKind::File(a), EntryKind::File(b)) => a.len() == b.len(),
                (EntryKind::Directory, EntryKind::Directory) => true,
                (EntryKind::Symlink(a), EntryKind::Symlink(b)) | (EntryKind::Hardlink(a), EntryKind::Hardlink(b)) => a == b,
                _ => false,
            };
            if !kinds_match || (entry.mode,entry.mtime_seconds,entry.mtime_nanoseconds)
                != (wanted.mode,wanted.mtime_seconds,wanted.mtime_nanoseconds) {
                return Err("fast certificate independent input descriptor mismatch".into());
            }
        }
        let roots = read_gzip_text(Path::new(get("certificate_file_roots")?))?;
        let mut lines = roots.lines();
        if lines.next() != Some("path\tcontent_root") { return Err("fast certificate file-root header".into()); }
        let mut file_roots = BTreeMap::new();
        for line in lines {
            let (path, root) = line.split_once('\t').ok_or("fast certificate file-root columns")?;
            if file_roots.insert(path.to_owned(), root.parse()?).is_some() { return Err("fast certificate duplicate file root".into()); }
        }
        let regular = fixture.iter().filter(|e| matches!(e.kind,EntryKind::File(_)|EntryKind::Hardlink(_)))
            .map(|e| e.path.clone()).collect::<BTreeSet<_>>();
        if regular != file_roots.keys().cloned().collect() { return Err("fast certificate regular membership".into()); }
        let certificate = Self { binding, root, file_roots };
        certificate.require_pristine_root(pristine_root)?;
        Ok(certificate)
    }
}

/// Full current namespace/global inode authentication, selected current bytes,
/// and explicitly certificate-bound unchanged content references. Not a full proof.
pub(crate) fn verify_fast_root(
    source: &dyn ObjectSource,
    root: ObjectId,
    entries: &[Entry],
    delta: &common::FastDelta,
    certificate: &FastCertificate,
) -> AnyResult<Receipt> {
    if !hex_digest(&certificate.binding) { return Err("fast certificate binding format".into()); }
    if entries.iter().any(|e| matches!(e.kind,EntryKind::File(Content::Digest {..}))) {
        return Err("fast expected bytes require independent source".into());
    }
    common::validate_entries(entries)?;
    let reader = CoreReader(source);
    let namespace = AuthenticatedNamespaceIndex::load(source,root)?;
    let expected_by_path = entries.iter().map(|entry| (entry.path.as_str(), entry)).collect::<BTreeMap<_,_>>();
    let selected = common::fast_selected_paths(entries,delta)?;
    let mut reference_counts = BTreeMap::<&str,u64>::new();
    for entry in entries {
        let class = match &entry.kind { EntryKind::File(_) => entry.path.as_str(), EntryKind::Hardlink(target) => target, _ => continue };
        *reference_counts.entry(class).or_default() += 1;
    }
    let mut pending = vec![(".".to_owned(), namespace.root_inode)];
    let mut found = BTreeSet::new();
    let mut visited_inodes = BTreeSet::new();
    let mut inode_classes = BTreeMap::new();
    let mut class_inodes = BTreeMap::new();
    let mut validated_metadata = BTreeSet::new();
    let mut scratch = vec![0; common::SCRATCH_BYTES];
    let mut actual_paths = 0u64;
    let mut actual_bytes = 0u64;
    let mut skipped_paths = 0u64;
    let mut skipped_bytes = 0u64;
    while let Some((path, id)) = pending.pop() {
        if !found.insert(path.clone()) { return Err("fast namespace duplicate path".into()); }
        let expected = expected_by_path.get(path.as_str()).ok_or("fast namespace extra path")?;
        let record = namespace.resolve_inode(id)?.record;
        record.validate(path == ".")?;
        if !visited_inodes.insert(id) && record.kind != inode::InodeKind::RegularFile {
            return Err("fast namespace repeats a non-regular inode".into());
        }
        let metadata_key = (record.metadata_root, record.kind as u8, expected.mode, expected.mtime_seconds, expected.mtime_nanoseconds);
        if validated_metadata.insert(metadata_key) {
            directory::validate_inode_record_metadata(&reader,record,path == ".")?;
            verify_metadata(&reader, record.metadata_root, expected)?;
        }
        match &expected.kind {
            EntryKind::Directory => {
                if record.kind != inode::InodeKind::Directory { return Err("fast directory type".into()); }
                directory::visit_directory_entries(&reader, directory::DirectoryStateRoot(record.content_root), &mut Default::default(), |page| {
                    for (name, inode) in page {
                        pending.push((if path == "." {name.as_str().to_owned()} else {format!("{path}/{}",name.as_str())}, *inode));
                    }
                    Ok(())
                })?;
            }
            EntryKind::Symlink(target) => {
                if record.kind != inode::InodeKind::Symlink || reader.with_authenticated_canonical(record.content_root,
                    directory::codec::decode_symlink)?.target != target.as_bytes() { return Err("fast symlink target/type".into()); }
            }
            EntryKind::File(_) | EntryKind::Hardlink(_) => {
                let (content, class) = match &expected.kind {
                    EntryKind::File(content) => (content, path.as_str()),
                    EntryKind::Hardlink(target) => match &expected_by_path.get(target.as_str()).ok_or("fast alias target")?.kind {
                        EntryKind::File(content) => (content, target.as_str()), _ => return Err("fast alias content".into()),
                    }, _ => unreachable!(),
                };
                if record.kind != inode::InodeKind::RegularFile || record.namespace_ref_count != reference_counts[class]
                    || inode_classes.insert(id,class.to_owned()).is_some_and(|old| old != class)
                    || class_inodes.insert(class.to_owned(),id).is_some_and(|old| old != id) {
                    return Err("fast regular type/alias/reference count".into());
                }
                if !delta.changed_paths.contains(&path) && certificate.file_roots.get(&path) != Some(&record.content_root) {
                    return Err("fast unchanged content reference differs from certificate".into());
                }
                if selected.contains(&path) {
                    let file_root = rope::FileStateRoot(record.content_root);
                    rope::validate_file(&reader,file_root)?;
                    let state = rope::state(&reader,file_root,&mut Default::default())?;
                    if state.logical_len != content.len() { return Err("fast changed/witness length".into()); }
                    let mut sink = CompareSink { expected:content, offset:0, scratch:&mut scratch, custody_hash:None };
                    rope::read_all(&reader,file_root,&mut sink)?;
                    if sink.offset != content.len() { return Err("fast changed/witness short read".into()); }
                    actual_paths += 1; actual_bytes += content.len();
                } else { skipped_paths += 1; skipped_bytes += content.len(); }
            }
        }
    }
    if found.iter().map(String::as_str).collect::<BTreeSet<_>>() != expected_by_path.keys().copied().collect()
        || delta.absent_paths.iter().any(|path| found.contains(path)) {
        return Err("fast exact namespace membership/absence".into());
    }
    namespace.require_complete_membership(&visited_inodes)?;
    Ok(Receipt::from([
        ("verification_status".into(),"fast_iteration_verified".into()),
        ("verification_profile".into(),"fast-verify-v1".into()),
        ("fully_verified".into(),"false".into()),
        ("full_canonical_census_performed".into(),"false".into()),
        ("certificate_binding".into(),certificate.binding.clone()),
        ("certificate_root".into(),certificate.root.to_string()),
        ("canonical_root".into(),root.to_string()),
        ("authenticated_namespace_paths".into(),found.len().to_string()),
        ("authenticated_global_inodes".into(),namespace.records.len().to_string()),
        ("actual_read_regular_paths".into(),actual_paths.to_string()),
        ("actual_read_logical_bytes".into(),actual_bytes.to_string()),
        ("skipped_current_store_regular_paths".into(),skipped_paths.to_string()),
        ("skipped_current_store_logical_bytes".into(),skipped_bytes.to_string()),
        ("scope".into(),"full current namespace/global inode/metadata/aliases; selected changed+witness bytes; unchanged file-state/extent/payload subgraph references certified, those current subgraph bytes not read".into()),
    ]))
}

/// One small aggregate verifier check; caller schedules it separately from samples.
pub(crate) fn fast_qualification(root: &Path) -> AnyResult<Receipt> {
    if root.exists() { return Err("fast qualification output already exists".into()); }
    std::fs::create_dir_all(root)?;
    let fixture = root.join("input");
    let entries = common::native_qualification_entries();
    common::create_fixture(&fixture,&entries)?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(EntityName::new("fast-qualification")?,LayerStackInitialization::Directory(fixture))?;
    let branch = client.fork_branch(EntityName::new("main")?, LocalForkSource::Layer { layer_id: initialized.genesis_layer_id })?;
    drop(client);
    let pinned = store.pin_branch(branch)?;
    let full = verify_root(&pinned.reader,pinned.root,&entries)
        .map_err(|error| format!("fast qualification exhaustive positive: {error}"))?;
    // Root and the empty directory deliberately share metadata. Reject a later
    // path's different expectation even after the root has warmed the memo.
    let reader = CoreReader(&pinned.reader);
    let root_record = layerfs_content::filesystem::resolve(&reader,pinned.root,&CanonicalPath::root(),&mut Default::default())?.record;
    let empty_record = layerfs_content::filesystem::resolve(&reader,pinned.root,&CanonicalPath::new("empty")?,&mut Default::default())?.record;
    if root_record.metadata_root != empty_record.metadata_root { return Err("qualification requires shared directory metadata".into()); }
    let mut wrong_metadata = entries.clone();
    wrong_metadata.iter_mut().find(|entry|entry.path=="empty").ok_or("qualification empty directory")?.mode ^= 1;
    let metadata_rejection = verify_root(&pinned.reader,pinned.root,&wrong_metadata).err()
        .ok_or("exhaustive verifier reused shared metadata despite different expected mode")?;
    let certificate = FastCertificate { binding:"ab".repeat(32), root:pinned.root, file_roots:full.file_roots };
    let delta_for = |oracle: &[Entry]| common::FastDelta {
        changed_paths:oracle.iter().map(|e|e.path.clone()).collect(), absent_paths:BTreeSet::new(), witness_paths:BTreeSet::new(),
    };
    let mut receipt = verify_fast_root(&pinned.reader,pinned.root,&entries,&delta_for(&entries),&certificate)
        .map_err(|error| format!("fast qualification canonical positive: {error}"))?;
    receipt.insert("exhaustive_shared_metadata_expected_rejection".into(),metadata_rejection.to_string());
    let mut rejections = 0;
    let mut reject = |name: &str, oracle: &[Entry]| -> AnyResult<()> {
        let error = verify_fast_root(&pinned.reader,pinned.root,oracle,&delta_for(oracle),&certificate).err().ok_or_else(||format!("fast canonical accepted {name}"))?;
        receipt.insert(format!("rejected_{name}"),error.to_string()); rejections += 1; Ok(())
    };
    let mut wrong = entries.clone();
    let file = wrong.iter_mut().find(|e|e.path=="payload").ok_or("qualification payload")?;
    let EntryKind::File(content) = &file.kind else { return Err("qualification file kind".into()) };
    file.kind = EntryKind::File(content.xor(17,1,1)?);
    reject("changed_bytes",&wrong)?;
    let mut extra = entries.clone(); extra.push(Entry::directory("extra")); reject("missing_namespace",&extra)?;
    let missing = entries.iter().filter(|e|e.path!="empty").cloned().collect::<Vec<_>>(); reject("extra_namespace",&missing)?;
    let mut wrong = entries.clone(); wrong[0].mode ^= 1; reject("metadata",&wrong)?;
    let mut wrong = entries.clone();
    let content = match &entries[2].kind { EntryKind::File(c)=>c.clone(),_=>return Err("qualification content".into()) };
    wrong.iter_mut().find(|e|e.path=="alias").ok_or("qualification alias")?.kind=EntryKind::File(content);
    reject("alias_class",&wrong)?;
    drop(reject);
    let wrong_root = "11".repeat(32).parse()?;
    if certificate.require_pristine_root(wrong_root).is_ok() { return Err("fast certificate accepted wrong pristine root".into()); }
    rejections += 1;
    let reference_delta = common::FastDelta { changed_paths:BTreeSet::new(), absent_paths:BTreeSet::new(), witness_paths:BTreeSet::new() };
    verify_fast_root(&pinned.reader,pinned.root,&entries,&reference_delta,&certificate)?;
    let mut wrong_certificate = FastCertificate { binding:certificate.binding.clone(),root:certificate.root,file_roots:certificate.file_roots.clone() };
    wrong_certificate.file_roots.insert("payload".into(),wrong_root);
    if verify_fast_root(&pinned.reader,pinned.root,&entries,&reference_delta,&wrong_certificate).is_ok() {
        return Err("fast verifier accepted wrong certified content root".into());
    }
    rejections += 1;
    let projection = b"profile\tfast-verify-v1\nroot\tqualified-root\n";
    let projection_hash = workload_source::sdk_edit_common::sha256_hex(projection);
    require_bound_bytes(projection,&projection_hash,"TSV projection")?;
    if require_bound_bytes(b"profile\tfast-verify-v1\nroot\tdrifted-root\n",&projection_hash,"TSV projection").is_ok() {
        return Err("fast certificate accepted modified TSV projection".into());
    }
    rejections += 1;
    let artifact = root.join("bound-artifact-negative.bin");
    let artifact_bytes = b"compressed artifact bytes";
    let artifact_hash = workload_source::sdk_edit_common::sha256_hex(artifact_bytes);
    std::fs::write(&artifact,artifact_bytes)?;
    require_bound_file(&artifact,&artifact_hash,"compressed file roots")?;
    std::fs::write(&artifact,b"modified compressed artifact bytes")?;
    if require_bound_file(&artifact,&artifact_hash,"compressed file roots").is_ok() {
        return Err("fast certificate accepted modified bound artifact".into());
    }
    rejections += 1;
    receipt.insert("canonical_negative_checks".into(),rejections.to_string());
    receipt.insert("qualification_status".into(),"pass".into());
    let native = common::fast_qualification(&root.join("native"))?;
    for (key,value) in native {receipt.insert(format!("native_{key}"),value);}
    let mut output=std::fs::File::create(root.join("qualification-receipt.txt"))?;
    for (key,value) in &receipt {writeln!(output,"{key}={value}")?;}
    Ok(receipt)
}

/// One tiny, explicitly selected native/Store verifier qualification.
pub(crate) fn qualification(root: &Path) -> AnyResult<Receipt> {
    if root.exists() {
        return Err("verifier qualification output already exists".into());
    }
    std::fs::create_dir_all(root)?;
    let fixture = root.join("native");
    let mut receipt = common::native_qualification(&fixture)?;
    let entries = common::native_qualification_entries();
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new("verifier-qualification")?,
        LayerStackInitialization::Directory(fixture),
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    drop(client);
    let pinned = store.pin_branch(branch)?;
    let verified = verify_root(&pinned.reader, pinned.root, &entries)?;
    let mut wrong = entries.clone();
    let file = wrong
        .iter_mut()
        .find(|entry| entry.path == "payload")
        .ok_or("qualification file")?;
    let EntryKind::File(content) = &file.kind else {
        return Err("qualification file type".into());
    };
    file.kind = EntryKind::File(content.xor(17, 1, 1)?);
    let rejection = verify_root(&pinned.reader, pinned.root, &wrong)
        .err()
        .ok_or("canonical verifier accepted incorrect expected bytes")?;
    receipt.extend(verified.receipt);
    receipt.insert("canonical_expected_rejection".into(), rejection.to_string());
    receipt.insert("qualification_status".into(), "pass".into());
    let mut output = std::fs::File::create(root.join("qualification-receipt.txt"))?;
    for (key, value) in &receipt {
        writeln!(output, "{key}={value}")?;
    }
    Ok(receipt)
}

/// Qualify only the new Digest branch; earlier recipe-verifier proofs stay valid.
pub(crate) fn digest_qualification(root: &Path) -> AnyResult<Receipt> {
    common::digest_self_check()?;
    if root.exists() {
        return Err("digest qualification output exists".into());
    }
    std::fs::create_dir_all(root)?;
    let fixture = root.join("native");
    let sources = common::native_qualification_entries();
    common::create_fixture(&fixture, &sources)?;
    let manifest = common::manifest(&sources)?;
    let custody = common::decode_manifest(&manifest)?;
    std::fs::write(root.join("precommit-custody.tsv"), &manifest)?;
    let mut receipt = common::verify_native(&fixture, &custody)?;
    let forbidden = root.join("digest-as-source");
    if common::create_fixture(&forbidden, &custody).is_ok() || forbidden.exists() {
        return Err("custody descriptor created source fixture state".into());
    }
    let mut wrong = custody.clone();
    let target = wrong
        .iter_mut()
        .find(|entry| entry.path == "payload")
        .ok_or("digest qualification file")?;
    let EntryKind::File(Content::Digest { sha256, .. }) = &mut target.kind else {
        return Err("digest qualification content".into());
    };
    sha256.replace_range(..1, if sha256.starts_with('0') { "1" } else { "0" });
    let native_error = common::verify_native(&fixture, &wrong)
        .err()
        .ok_or("native verifier accepted wrong custody SHA")?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new("digest-qualification")?,
        LayerStackInitialization::Directory(fixture),
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    drop(client);
    let pinned = store.pin_branch(branch)?;
    let canonical = verify_root(&pinned.reader, pinned.root, &custody)?;
    let canonical_error = verify_root(&pinned.reader, pinned.root, &wrong)
        .err()
        .ok_or("canonical verifier accepted wrong custody SHA")?;
    receipt.extend(canonical.receipt);
    receipt.insert(
        "native_digest_expected_rejection".into(),
        native_error.to_string(),
    );
    receipt.insert(
        "canonical_digest_expected_rejection".into(),
        canonical_error.to_string(),
    );
    receipt.insert("digest_qualification_status".into(), "pass".into());
    let mut output = std::fs::File::create(root.join("digest-qualification-receipt.txt"))?;
    for (key, value) in &receipt {
        writeln!(output, "{key}={value}")?;
    }
    Ok(receipt)
}
