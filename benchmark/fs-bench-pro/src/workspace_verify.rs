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

fn write_gzip(path: &Path, write: impl FnOnce(&mut dyn Write) -> AnyResult<()>) -> AnyResult<()> {
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
}

pub(crate) fn verify(
    store: &LayerStackStore,
    branch: BranchId,
    entries: &[Entry],
    evidence: &Path,
) -> AnyResult<SnapshotEvidence> {
    let pinned = store.pin_branch(branch)?;
    let mut result = verify_root(&pinned.reader, pinned.root, entries)?;
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
        writer.write_all(common::manifest(entries)?.as_bytes())?;
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
    Ok(result)
}

pub(crate) fn verify_root(
    source: &dyn ObjectSource,
    root: ObjectId,
    entries: &[Entry],
) -> AnyResult<SnapshotEvidence> {
    let logical = common::validate_entries(entries)?;
    let reader = CoreReader(source);
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
    let mut pending = vec![".".to_owned()];
    let mut inode_classes = BTreeMap::new();
    let mut class_inodes = BTreeMap::new();
    let mut namespace_inodes = BTreeSet::new();
    let mut extents = BTreeMap::new();
    let mut file_roots = BTreeMap::new();
    let mut custody_paths = 0;
    while let Some(path) = pending.pop() {
        if !found.insert(path.clone()) {
            return Err("canonical path repeated".into());
        }
        let entry = expected
            .get(path.as_str())
            .ok_or_else(|| format!("extra canonical path: {path}"))?;
        let canonical = if path == "." {
            CanonicalPath::root()
        } else {
            CanonicalPath::new(&path)?
        };
        let resolved = layerfs_content::filesystem::resolve(
            &reader,
            root,
            &canonical,
            &mut Default::default(),
        )?;
        namespace_inodes.insert(resolved.inode);
        directory::validate_inode_record_metadata(&reader, resolved.record, path == ".")?;
        verify_metadata(&reader, resolved.record.metadata_root, entry)?;
        match &entry.kind {
            EntryKind::Directory => {
                if resolved.record.kind != inode::InodeKind::Directory {
                    return Err(format!("canonical directory type: {path}").into());
                }
                let mut after = None;
                loop {
                    let (page, _) = layerfs_content::filesystem::list(
                        &reader,
                        root,
                        &canonical,
                        after.as_ref(),
                        127,
                        8192,
                    )?;
                    for (name, _) in &page.entries {
                        let child = if path == "." {
                            name.as_str().to_owned()
                        } else {
                            format!("{path}/{}", name.as_str())
                        };
                        pending.push(child);
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
                    || layerfs_content::filesystem::readlink(&reader, root, &canonical)?.0
                        != target.as_bytes()
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
                    scratch: vec![0; common::SCRATCH_BYTES],
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
    let namespace = layerfs_content::filesystem::namespace(&reader, root)?;
    let mut table_inodes = BTreeSet::new();
    inode::visit_inode_table_entries(
        &reader,
        inode::InodeTableRoot(namespace.inode_table_root),
        &mut Default::default(),
        |page| {
            for (id, _) in page {
                if !table_inodes.insert(*id) {
                    return Err(layerfs_content::CoreError::InvalidRecord(
                        "duplicate inode table entry",
                    ));
                }
            }
            Ok(())
        },
    )?;
    if table_inodes != namespace_inodes {
        return Err("canonical inode table has missing or unreachable entries".into());
    }
    let mut receipt = typed_census(source, root)?;
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
    receipt.insert(
        "oracle_identity".into(),
        workload_source::sdk_edit_common::sha256_hex(common::manifest(entries)?.as_bytes()),
    );
    Ok(SnapshotEvidence {
        receipt,
        extents,
        file_roots,
    })
}

struct CompareSink<'a> {
    expected: &'a Content,
    offset: u64,
    scratch: Vec<u8>,
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
enum Role {
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

pub(crate) fn typed_census(source: &dyn ObjectSource, root: ObjectId) -> AnyResult<Receipt> {
    let mut pending = vec![(root, Role::Namespace)];
    let mut seen = BTreeMap::new();
    let mut totals = BTreeMap::<Role, (u64, u64)>::new();
    while let Some((id, role)) = pending.pop() {
        if let Some(previous) = seen.insert(id, role) {
            if previous != role {
                return Err("canonical object referenced with incompatible roles".into());
            }
            continue;
        }
        let objects = source.read_authenticated_objects(&[id])?;
        if objects.len() != 1 || objects[0].id != id {
            return Err("canonical authenticated batch identity".into());
        }
        let bytes = &objects[0].bytes;
        layerfs_content::authenticate_identity(bytes, id)?;
        let total = totals.entry(role).or_default();
        total.0 += 1;
        total.1 += bytes.len() as u64;
        match role {
            Role::Namespace => pending.push((
                directory::codec::decode_namespace_root(bytes)?.inode_table_root,
                Role::InodeTable,
            )),
            Role::InodeTable => match inode::codec::decode_inode_table_node(bytes)? {
                inode::codec::InodeTableNodeV1::Leaf(entries) => {
                    pending.extend(entries.into_iter().map(|(_, id)| (id, Role::InodeRecord)))
                }
                inode::codec::InodeTableNodeV1::Branch { children, .. } => {
                    pending.extend(children.into_iter().map(|(_, id)| (id, Role::InodeTable)))
                }
            },
            Role::InodeRecord => {
                let record = inode::codec::decode_inode_record(bytes)?;
                pending.push((record.metadata_root, Role::Metadata));
                pending.push((
                    record.content_root,
                    match record.kind {
                        inode::InodeKind::RegularFile => Role::FileState,
                        inode::InodeKind::Directory => Role::DirectoryState,
                        inode::InodeKind::Symlink => Role::Symlink,
                    },
                ));
            }
            Role::DirectoryState => pending.push((
                directory::codec::decode_directory_state(bytes)?.mapping_root,
                Role::DirectoryNode,
            )),
            Role::DirectoryNode => {
                if let directory::codec::DirectoryNodeV1::Branch { children, .. } =
                    directory::codec::decode_directory_node(bytes)?
                {
                    pending.extend(
                        children
                            .into_iter()
                            .map(|(_, id)| (id, Role::DirectoryNode)),
                    );
                }
            }
            Role::Metadata => match metadata::codec::decode_metadata_node(bytes)? {
                metadata::codec::MetadataNodeV1::Leaf { entries, .. } => pending.extend(
                    entries
                        .into_iter()
                        .map(|entry| (entry.value_file_root, Role::FileState)),
                ),
                metadata::codec::MetadataNodeV1::Branch { children, .. } => {
                    pending.extend(children.into_iter().map(|(_, id)| (id, Role::Metadata)))
                }
            },
            Role::FileState => pending.push((
                extent_codec::decode_file_state(bytes)?.mapping_root,
                Role::FileNode,
            )),
            Role::FileNode => match extent_codec::decode_node(bytes)? {
                ExtentNodeV3::Leaf { extents, .. } => pending.extend(
                    extents
                        .into_iter()
                        .map(|extent| (extent.payload_object_id, Role::Chunk)),
                ),
                ExtentNodeV3::Branch { children, .. } => pending.extend(
                    children
                        .into_iter()
                        .map(|child| (child.child_object_id, Role::FileNode)),
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
    let mut receipt = Receipt::new();
    for (role, (count, bytes)) in totals {
        receipt.insert(format!("canonical_{role:?}_objects"), count.to_string());
        receipt.insert(format!("canonical_{role:?}_bytes"), bytes.to_string());
    }
    receipt.insert("canonical_unique_objects".into(), seen.len().to_string());
    receipt.insert("canonical_role_status".into(), "pass".into());
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
