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

// Independent regular-root grammar; usable by both schema7 and schema9 builds.
// Metadata ropes continue to use the extent-only functions below.
pub(crate) fn small_bytes(canonical: &[u8]) -> layerfs_content::CoreResult<Option<&[u8]>> {
    let value = layerfs_content::decode_bytes_object(canonical)?;
    if !value.starts_with(b"LFS5SML\0") {
        return Ok(None);
    }
    if value.get(8..10) != Some(&[0, 1]) || !(11..131082).contains(&value.len()) {
        return Err(layerfs_content::CoreError::InvalidRecord(
            "SmallContent framing",
        ));
    }
    Ok(Some(&value[10..]))
}

pub(crate) fn expected_small_root(bytes: &[u8]) -> AnyResult<ObjectId> {
    if bytes.is_empty() || bytes.len() >= 131072 {
        return Err("SmallContent length".into());
    }
    let mut payload = b"LFS5SML\0\0\x01".to_vec();
    payload.extend_from_slice(bytes);
    let canonical = layerfs_content::encode_bytes_object(&payload)?;
    Ok(ObjectId::for_bytes(&canonical))
}

fn regular_length(reader: &CoreReader<'_>, root: rope::FileStateRoot) -> AnyResult<u64> {
    Ok(reader.with_authenticated_canonical(root.0, |canonical| {
        Ok(match small_bytes(canonical)? {
            Some(raw) => raw.len() as u64,
            None => extent_codec::decode_file_state(canonical)?.logical_len,
        })
    })?)
}

fn validate_regular(reader: &CoreReader<'_>, root: rope::FileStateRoot) -> AnyResult<()> {
    if !reader
        .with_authenticated_canonical(root.0, |canonical| Ok(small_bytes(canonical)?.is_some()))?
    {
        rope::validate_file(reader, root)?;
    }
    Ok(())
}

fn read_regular(
    reader: &CoreReader<'_>,
    root: rope::FileStateRoot,
    range: std::ops::Range<u64>,
    sink: &mut impl Write,
) -> AnyResult<()> {
    let small = reader.with_authenticated_canonical(root.0, |canonical| {
        Ok(small_bytes(canonical)?.map(Vec::from))
    })?;
    if let Some(bytes) = small {
        let from = usize::try_from(range.start)?;
        let to = usize::try_from(range.end)?;
        sink.write_all(bytes.get(from..to).ok_or("SmallContent range bound")?)?;
    } else {
        rope::read_range(reader, root, range, sink)?;
    }
    Ok(())
}

pub(crate) fn verify_sample(
    source: &dyn ObjectSource,
    root: ObjectId,
    sample: &common::TreeSample,
) -> AnyResult<Receipt> {
    sample.validate()?;
    let reader = CoreReader(source);
    for entry in &sample.entries {
        let path = if entry.path == "." {
            CanonicalPath::root()
        } else {
            CanonicalPath::new(&entry.path)?
        };
        let resolved =
            layerfs_content::filesystem::resolve(&reader, root, &path, &mut Default::default())?;
        resolved.record.validate(entry.path == ".")?;
        verify_metadata(&reader, resolved.record.metadata_root, entry)?;
        match &entry.kind {
            EntryKind::Directory if resolved.record.kind == inode::InodeKind::Directory => (),
            EntryKind::File(content) if resolved.record.kind == inode::InodeKind::RegularFile => {
                let file = rope::FileStateRoot(resolved.record.content_root);
                let length = regular_length(&reader, file)?;
                if length != content.len() {
                    return Err(format!("sampled canonical length: {}", entry.path).into());
                }
                for (offset, len) in sample.file_ranges(entry, content) {
                    let mut expected = vec![0; len];
                    if content.read_at(offset, &mut expected)? != len {
                        return Err("sampled oracle length".into());
                    }
                    let mut actual = Vec::with_capacity(len);
                    read_regular(&reader, file, offset..offset + len as u64, &mut actual)?;
                    if actual != expected {
                        return Err(format!("sampled canonical bytes: {}", entry.path).into());
                    }
                }
            }
            _ => return Err(format!("sampled canonical kind: {}", entry.path).into()),
        }
    }
    for path in &sample.absent {
        let mut prefix = String::new();
        let mut missing = false;
        for component in path.split('/') {
            let parent = if prefix.is_empty() {
                CanonicalPath::root()
            } else {
                CanonicalPath::new(&prefix)?
            };
            let resolved = layerfs_content::filesystem::resolve(
                &reader,
                root,
                &parent,
                &mut Default::default(),
            )?;
            if resolved.record.kind != inode::InodeKind::Directory {
                return Err("sampled absent parent kind".into());
            }
            if directory::directory_lookup(
                &reader,
                directory::DirectoryStateRoot(resolved.record.content_root),
                &layerfs_content::CanonicalName::from_bytes(component.as_bytes())?,
                &mut Default::default(),
            )?
            .is_none()
            {
                missing = true;
                break;
            }
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
        }
        if !missing {
            return Err(format!("sampled canonical absence: {path}").into());
        }
    }
    Ok(sample.receipt())
}

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
    verify_named(store, branch, entries, evidence, "canonical-verification")
}

/// Multi-Commit lifecycles verify every cycle from its own published head, so
/// each cycle keeps its own evidence directory instead of colliding with the
/// single-cycle path. `step` is the 1-based Commit ordinal.
pub(crate) fn verify_step(
    store: &LayerStackStore,
    branch: BranchId,
    entries: &[Entry],
    evidence: &Path,
    step: usize,
) -> AnyResult<SnapshotEvidence> {
    verify_named(
        store,
        branch,
        entries,
        evidence,
        &format!("canonical-verification-step-{step}"),
    )
}

fn verify_named(
    store: &LayerStackStore,
    branch: BranchId,
    entries: &[Entry],
    evidence: &Path,
    name: &str,
) -> AnyResult<SnapshotEvidence> {
    let pinned = store.pin_branch(branch)?;
    let mut result = verify_root(&pinned.reader, pinned.root, entries)?;
    persist_snapshot(entries, &mut result, evidence, name)?;
    Ok(result)
}

/// Verify a snapshot whose declared classes are not derived from hard-link
/// edges alone.
///
/// `inode_classes` maps each declared path to its declared inode-class key and
/// `class_ref_counts` gives the declared reference count for every class. This
/// is the v0.1.6 alias route: an alias may legitimately separate from its
/// target inside one declared schedule, so the two names end in different
/// classes even though the initial fixture linked them.
pub(crate) fn verify_split_classes(
    store: &LayerStackStore,
    branch: BranchId,
    entries: &[Entry],
    evidence: &Path,
    split_class: &str,
) -> AnyResult<SnapshotEvidence> {
    let pinned = store.pin_branch(branch)?;
    let mut result = verify_root_split(&pinned.reader, pinned.root, entries, split_class)?;
    persist_snapshot(entries, &mut result, evidence, "canonical-verification")?;
    Ok(result)
}

pub(crate) fn persist_snapshot(
    entries: &[Entry],
    result: &mut SnapshotEvidence,
    evidence: &Path,
    name: &str,
) -> AnyResult<()> {
    let evidence = evidence.join(name);
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

/// The dedicated v0.1.6 alias oracle: one declared path pair is proved to share
/// exactly one inode before the replacement Commit and to hold two distinct
/// inodes afterwards, each with the reference count its declared class requires.
///
/// This is deliberately separate from the generic class verifier: the generic
/// route derives one reference count per declared class for the whole snapshot
/// and therefore cannot express "one shared inode before, two classes after"
/// inside a single schedule. The reference-count check itself is unchanged and
/// still applies to both states.
pub(crate) struct AliasClassProof {
    pub(crate) before_inode: inode::InodeId,
    pub(crate) before_ref_count: u64,
    pub(crate) after_inode: inode::InodeId,
    pub(crate) alias_inode: inode::InodeId,
    pub(crate) target_ref_count: u64,
    pub(crate) alias_ref_count: u64,
    pub(crate) before_content_root: ObjectId,
    pub(crate) target_content_root: ObjectId,
    pub(crate) alias_content_root: ObjectId,
}

/// One inode's declared proof material, read from a retained root.
struct AliasBinding {
    inode: inode::InodeId,
    ref_count: u64,
    content_root: ObjectId,
    length: u64,
}

fn alias_binding(source: &dyn ObjectSource, root: ObjectId, path: &str) -> AnyResult<AliasBinding> {
    let view = namespace_view(source, root)?;
    let inode = view
        .inodes
        .get(path)
        .copied()
        .ok_or_else(|| format!("v0.1.6 alias oracle: path absent: {path}"))?;
    let record = view
        .paths
        .get(path)
        .ok_or_else(|| format!("v0.1.6 alias oracle: record absent: {path}"))?;
    if record.kind != inode_regular() {
        return Err(format!("v0.1.6 alias oracle: {path} is not a regular file").into());
    }
    Ok(AliasBinding {
        inode,
        ref_count: record.namespace_ref_count,
        content_root: record.content_root,
        length: declared_regular_length(source, record)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_alias_classes(
    before_source: &dyn ObjectSource,
    before_root: ObjectId,
    after_source: &dyn ObjectSource,
    after_root: ObjectId,
    target: &str,
    alias: &str,
    declared_before_len: u64,
    declared_after_len: u64,
    declared_alias_len: u64,
) -> AnyResult<AliasClassProof> {
    let shared = alias_binding(before_source, before_root, target)?;
    let before_alias = alias_binding(before_source, before_root, alias)?;
    // (a) both names hold one shared inode before the replacement.
    if shared.inode != before_alias.inode {
        return Err(format!(
            "v0.1.6 alias oracle: {target} and {alias} are not one shared inode before the replacement"
        )
        .into());
    }
    if shared.ref_count != 2 {
        return Err(format!(
            "v0.1.6 alias oracle: shared inode reference count {}, declared 2",
            shared.ref_count
        )
        .into());
    }
    if shared.content_root != before_alias.content_root {
        return Err("v0.1.6 alias oracle: shared inode content roots differ".into());
    }
    if shared.length != declared_before_len {
        return Err(format!(
            "v0.1.6 alias oracle: pre-replacement target length {}, declared {declared_before_len}",
            shared.length
        )
        .into());
    }
    // (b) and (c) two distinct inodes after, each with its declared class count.
    let after_target = alias_binding(after_source, after_root, target)?;
    let after_alias = alias_binding(after_source, after_root, alias)?;
    if after_target.inode == after_alias.inode {
        return Err(format!(
            "v0.1.6 alias oracle: {target} and {alias} still share inode {:?} after the replacement",
            after_target.inode
        )
        .into());
    }
    if after_target.ref_count != 1 || after_alias.ref_count != 1 {
        return Err(format!(
            "v0.1.6 alias oracle: reference counts {} and {} after the replacement, declared 1 and 1",
            after_target.ref_count, after_alias.ref_count
        )
        .into());
    }
    if after_target.length != declared_after_len {
        return Err(format!(
            "v0.1.6 alias oracle: replacement target length {}, declared {declared_after_len}",
            after_target.length
        )
        .into());
    }
    if after_alias.length != declared_alias_len {
        return Err(format!(
            "v0.1.6 alias oracle: surviving alias length {}, declared {declared_alias_len}",
            after_alias.length
        )
        .into());
    }
    // The surviving alias keeps the pre-replacement inode and its content.
    if after_alias.inode != shared.inode {
        return Err(format!(
            "v0.1.6 alias oracle: surviving alias inode {:?} is not the pre-replacement inode {:?}",
            after_alias.inode, shared.inode
        )
        .into());
    }
    if after_alias.content_root != shared.content_root {
        return Err("v0.1.6 alias oracle: surviving alias content root changed".into());
    }
    Ok(AliasClassProof {
        before_inode: shared.inode,
        before_ref_count: shared.ref_count,
        after_inode: after_target.inode,
        alias_inode: after_alias.inode,
        target_ref_count: after_target.ref_count,
        alias_ref_count: after_alias.ref_count,
        before_content_root: shared.content_root,
        target_content_root: after_target.content_root,
        alias_content_root: after_alias.content_root,
    })
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
        inode::visit_inode_records(
            &reader,
            inode::InodeTableRoot(namespace.inode_table_root),
            &mut Default::default(),
            |id, record| {
                if records.insert(id, record).is_some() {
                    return Err(layerfs_content::CoreError::InvalidRecord(
                        "duplicate global inode",
                    ));
                }
                Ok(())
            },
        )?;
        Ok(Self {
            root_inode: namespace.root_directory_inode,
            records,
        })
    }
    fn resolve_inode(
        &self,
        id: inode::InodeId,
    ) -> AnyResult<layerfs_content::filesystem::Resolved> {
        let record = *self
            .records
            .get(&id)
            .ok_or("namespace references missing global inode")?;
        Ok(layerfs_content::filesystem::Resolved { inode: id, record })
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
    verify_root_split(source, root, entries, "")
}

/// One path's explicit metadata from the live namespace view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewMetadata {
    pub(crate) permission_mode: u32,
    pub(crate) mtime_seconds: i64,
    pub(crate) mtime_nanoseconds: u32,
}

/// One complete namespace snapshot: every canonical path with its resolved
/// inode record and its explicit metadata. Reading the namespace is bounded; no
/// file byte, content root or payload is read here.
pub(crate) struct NamespaceView {
    pub(crate) paths: BTreeMap<String, inode::InodeRecordV1>,
    pub(crate) inodes: BTreeMap<String, inode::InodeId>,
    pub(crate) metadata: BTreeMap<String, ViewMetadata>,
}

impl NamespaceView {
    pub(crate) fn inode(&self, path: &str) -> Option<inode::InodeId> {
        self.inodes.get(path).copied()
    }
}

/// Read the explicit mode and mtime of one inode from its metadata tree.
fn read_view_metadata(
    reader: &CoreReader<'_>,
    root: ObjectId,
    path: &str,
) -> AnyResult<ViewMetadata> {
    let entries = metadata::metadata_tree_entries(reader, root)?;
    let mut mode = None;
    let mut timestamp = None;
    for entry in entries {
        if entry.key.domain != "portable" || !matches!(entry.key.key.as_slice(), b"mode" | b"mtime")
        {
            return Err(format!("unexpected canonical metadata: {path}").into());
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
        if entry.key.key == b"mode" {
            mode = Some(u32::from_be_bytes(value.as_slice().try_into()?));
        } else {
            timestamp = Some((
                i64::from_be_bytes(value[..8].try_into()?),
                u32::from_be_bytes(value[8..].try_into()?),
            ));
        }
    }
    let (mtime_seconds, mtime_nanoseconds) =
        timestamp.ok_or_else(|| format!("canonical metadata mtime: {path}"))?;
    Ok(ViewMetadata {
        permission_mode: mode.ok_or_else(|| format!("canonical metadata mode: {path}"))?,
        mtime_seconds,
        mtime_nanoseconds,
    })
}

/// Read the complete authenticated namespace of one snapshot. Every path the
/// snapshot contains is visited through the directory tree, so a missing or
/// extra path is observable without trusting a cached manifest.
pub(crate) fn namespace_view(
    source: &dyn ObjectSource,
    root: ObjectId,
) -> AnyResult<NamespaceView> {
    let reader = CoreReader(source);
    let namespace = AuthenticatedNamespaceIndex::load(source, root)?;
    let mut paths = BTreeMap::new();
    let mut inodes = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut pending = vec![(".".to_owned(), namespace.root_inode)];
    while let Some((path, id)) = pending.pop() {
        if !seen.insert(path.clone()) {
            return Err("canonical path repeated".into());
        }
        let resolved = namespace.resolve_inode(id)?;
        resolved.record.validate(path == ".")?;
        let is_directory = resolved.record.kind == inode::InodeKind::Directory;
        if inodes.insert(path.clone(), id).is_some() {
            return Err("canonical namespace repeated an inode binding".into());
        }
        let value = read_view_metadata(&reader, resolved.record.metadata_root, &path)?;
        metadata.insert(path.clone(), value);
        if is_directory {
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
                    pending.push((child, *child_inode));
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
        paths.insert(path, resolved.record);
    }
    if paths.len() != seen.len() {
        return Err("canonical namespace size".into());
    }
    Ok(NamespaceView {
        paths,
        inodes,
        metadata,
    })
}

/// Independently recompute the persisted content root of one regular file from
/// its declared bytes. This proves the Store's own content identity without
/// reading the payload back through the Store.
pub(crate) fn declared_content_root(entry: &Entry) -> AnyResult<Option<ObjectId>> {
    let content = match &entry.kind {
        EntryKind::File(content) => content,
        _ => return Ok(None),
    };
    // SmallContent carries 1..131,071 bytes; empty files have their own
    // representation and are proved by their declared length and type.
    if content.len() == 0 || content.len() >= 131_072 {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(content.len() as usize);
    content.write_to(&mut bytes)?;
    Ok(Some(expected_small_root(&bytes)?))
}

/// One commit's identity and parent edge, observed through the public query
/// surface after a reopen.
pub(crate) struct CommitRecord {
    pub(crate) parent_commit_id: Option<layerfs_sdk::CommitId>,
}

pub(crate) fn commit_records(
    client: &Client,
    wanted: &[layerfs_sdk::CommitId],
) -> AnyResult<BTreeMap<layerfs_sdk::CommitId, CommitRecord>> {
    let expected: BTreeSet<layerfs_sdk::CommitId> = wanted.iter().copied().collect();
    let mut records = BTreeMap::new();
    let mut query = Query::new(QueryKind::Commits).limit(127);
    loop {
        let page = client.query(query.clone())?;
        for item in &page.items {
            if let QueryItem::Commit(commit) = item {
                if expected.contains(&commit.id) {
                    records.insert(
                        commit.id,
                        CommitRecord {
                            parent_commit_id: commit.parent_commit_id,
                        },
                    );
                }
            }
        }
        if records.len() == expected.len() {
            break;
        }
        let Some(next) = page.into_next_query(&query) else {
            break;
        };
        query = next;
    }
    if records.len() != expected.len() {
        return Err(format!(
            "reopened Store does not expose every retained commit: {} of {}",
            records.len(),
            expected.len()
        )
        .into());
    }
    Ok(records)
}

pub(crate) fn inode_regular() -> inode::InodeKind {
    inode::InodeKind::RegularFile
}

pub(crate) fn inode_directory() -> inode::InodeKind {
    inode::InodeKind::Directory
}

/// The declared logical length of one persisted regular file. SmallContent
/// carries its bytes inline; chunked files declare a logical length in their
/// file-state record. Reading the file-state header performs no payload read.
pub(crate) fn declared_regular_length(
    source: &dyn ObjectSource,
    record: &inode::InodeRecordV1,
) -> AnyResult<u64> {
    if record.kind != inode::InodeKind::RegularFile {
        return Err("declared regular length requires a regular inode".into());
    }
    let reader = CoreReader(source);
    Ok(
        reader.with_authenticated_canonical(record.content_root, |canonical| {
            Ok(match small_bytes(canonical)? {
                Some(raw) => raw.len() as u64,
                None => extent_codec::decode_file_state(canonical)?.logical_len,
            })
        })?,
    )
}

/// Read one declared byte range of a persisted regular file through the Store
/// reader and compare it with the independent recipe value.
///
/// The caller passes the file-state root it already resolved from this
/// snapshot's inventory. Resolving the path here instead would rebuild the
/// complete O(persisted paths) namespace traversal once per range — a
/// verification pass checks many ranges of one snapshot, so that repeats
/// identical work without proving anything extra (#154). The comparison itself
/// is unchanged: the same bytes are read from the Store and compared with the
/// same independent recipe value.
pub(crate) fn verify_declared_range_at(
    source: &dyn ObjectSource,
    content_root: ObjectId,
    path: &str,
    range: std::ops::Range<u64>,
    expected: &[u8],
) -> AnyResult<()> {
    let reader = CoreReader(source);
    let mut observed = Vec::with_capacity(expected.len());
    read_regular(
        &reader,
        rope::FileStateRoot(content_root),
        range,
        &mut observed,
    )?;
    if observed != expected {
        return Err(format!("declared range mismatch: {path}").into());
    }
    Ok(())
}

/// Verify one snapshot. `split_class` names a declared path whose inode class
/// is declared to be its own rather than its hard-link target's: the v0.1.6
/// alias plan replaces the target name while the alias keeps the previous
/// inode, so the two names hold one class each even though the fixture linked
/// them. Every other class keeps the hard-link-derived reference count.
pub(crate) fn verify_root_split(
    source: &dyn ObjectSource,
    root: ObjectId,
    entries: &[Entry],
    split_class: &str,
) -> AnyResult<SnapshotEvidence> {
    let logical = common::validate_entries(entries)?;
    let reader = CoreReader(source);
    let namespace = AuthenticatedNamespaceIndex::load(source, root)?;
    let expected = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut reference_counts = BTreeMap::<&str, u64>::new();
    for entry in entries {
        let class = if !split_class.is_empty() && entry.path == split_class {
            entry.path.as_str()
        } else {
            match &entry.kind {
                EntryKind::File(_) => entry.path.as_str(),
                EntryKind::Hardlink(target) => target.as_str(),
                _ => continue,
            }
        };
        *reference_counts.entry(class).or_default() += 1;
    }
    let mut found = BTreeSet::new();
    let mut pending = vec![(".".to_owned(), namespace.root_inode)];
    let mut inode_classes = BTreeMap::new();
    let mut class_inodes = BTreeMap::new();
    let mut namespace_inodes = BTreeSet::new();
    let mut extents = BTreeMap::new();
    let mut file_roots = BTreeMap::new();
    let mut custody_paths = 0;
    let mut validated_metadata = BTreeSet::new();
    let mut comparison_scratch = vec![0; common::SCRATCH_BYTES];
    while let Some((path, id)) = pending.pop() {
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
        let metadata_key = (
            resolved.record.metadata_root,
            resolved.record.kind as u8,
            entry.mode,
            entry.mtime_seconds,
            entry.mtime_nanoseconds,
        );
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
                        pending.push((child, *child_inode));
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
                    || reader
                        .with_authenticated_canonical(
                            resolved.record.content_root,
                            directory::codec::decode_symlink,
                        )?
                        .target
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
                    return Err(format!(
                        "canonical hard-link reference count: {path} observed {} declared {}",
                        resolved.record.namespace_ref_count, reference_counts[class]
                    )
                    .into());
                }
                let file_root = rope::FileStateRoot(resolved.record.content_root);
                file_roots.insert(path.clone(), resolved.record.content_root);
                validate_regular(&reader, file_root)?;
                let length = regular_length(&reader, file_root)?;
                if length != content.len() {
                    return Err(format!(
                        "canonical length: {path} observed {length} expected {}",
                        content.len()
                    )
                    .into());
                }
                let mut sink = CompareSink {
                    expected: content,
                    offset: 0,
                    scratch: &mut comparison_scratch,
                    custody_hash: matches!(content, Content::Digest { .. })
                        .then(workload_source::Sha256::new),
                };
                custody_paths += usize::from(sink.custody_hash.is_some());
                read_regular(&reader, file_root, 0..content.len(), &mut sink)?;
                if sink.offset != content.len() {
                    return Err(format!("canonical short file: {path}").into());
                }
                if let (Some(hash), Content::Digest { sha256, .. }) = (sink.custody_hash, content) {
                    if workload_source::hex(&hash.finish()) != *sha256 {
                        return Err(format!("canonical persistence digest mismatch: {path}").into());
                    }
                }
                let file_extents = read_file_extents(&reader, file_root)?;
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
    receipt.insert(
        "regular_content_schema".into(),
        "authenticated-filecontent-v2".into(),
    );
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

fn read_file_extents(
    reader: &CoreReader<'_>,
    file_root: rope::FileStateRoot,
) -> AnyResult<Vec<Extent>> {
    if let Some(length) = reader.with_authenticated_canonical(file_root.0, |canonical| {
        Ok(small_bytes(canonical)?.map(|raw| raw.len() as u64))
    })? {
        return Ok(vec![Extent {
            id: file_root.0,
            source_offset: 0,
            len: length,
            payload_len: length,
        }]);
    }
    let mut file_extents = Vec::new();
    rope::visit_extents(reader, file_root, |page| {
        for extent in page {
            let payload_len =
                reader.with_authenticated_canonical(extent.payload_object_id, |canonical| {
                    Ok(
                        extent_codec::decode_chunk_payload(layerfs_content::decode_bytes_object(
                            canonical,
                        )?)?
                        .len() as u64,
                    )
                })?;
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
    Ok(file_extents)
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
        let absent: &[u8] = b"<absent>";
        return Err(format!(
            "canonical metadata mismatch: {} (expected mode {:02x?} mtime {:02x?}; observed mode {:02x?} mtime {:02x?})",
            expected.path,
            expected.mode.to_be_bytes(),
            timestamp,
            observed.get(b"mode".as_slice()).map(Vec::as_slice).unwrap_or(absent),
            observed.get(b"mtime".as_slice()).map(Vec::as_slice).unwrap_or(absent),
        )
        .into());
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
    SmallContent,
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
        matches!(self.role, Role::Chunk | Role::SmallContent) && self.regular_file
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Origin {
    Structure,
    RegularFile,
    MetadataValue,
}

fn census_inode(pending: &mut Vec<(ObjectId, Role, Origin)>, record: inode::InodeRecordV1) {
    pending.push((record.metadata_root, Role::Metadata, Origin::Structure));
    let (role, origin) = match record.kind {
        inode::InodeKind::RegularFile => (Role::FileState, Origin::RegularFile),
        inode::InodeKind::Directory => (Role::DirectoryState, Origin::Structure),
        inode::InodeKind::Symlink => (Role::Symlink, Origin::Structure),
    };
    pending.push((record.content_root, role, origin));
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
            if visits.insert(item) {
                batch.push(item);
            }
        }
        if batch.is_empty() {
            continue;
        }
        let ids = batch.iter().map(|item| item.0).collect::<Vec<_>>();
        let objects = source.read_authenticated_objects(&ids)?;
        if objects.len() != batch.len() {
            return Err("canonical authenticated batch cardinality".into());
        }
        for ((id, role, origin), object) in batch.into_iter().zip(objects) {
            if object.id != id {
                return Err("canonical authenticated batch identity".into());
            }
            let bytes = &object.bytes;
            layerfs_content::authenticate_identity(bytes, id)?;
            let role = if role == Role::DirectoryState
                && layerfs_content::decode_bytes_object(bytes)?.starts_with(b"LFS6NSP\0")
            {
                Role::DirectoryNode
            } else if role == Role::FileState && small_bytes(bytes)?.is_some() {
                if origin != Origin::RegularFile {
                    return Err("SmallContent is not a metadata rope".into());
                }
                Role::SmallContent
            } else {
                role
            };
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
                    if previous.role != role || previous.canonical_bytes != observed.canonical_bytes
                    {
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
                Role::InodeTable
                    if layerfs_content::decode_bytes_object(bytes)?.starts_with(b"LFS6INT\0") =>
                {
                    match layerfs_content::tree::compact::decode_inode(bytes)? {
                        layerfs_content::tree::compact::InodeNode::Leaf(records) => {
                            for (_, record) in records {
                                census_inode(&mut pending, record);
                            }
                        }
                        layerfs_content::tree::compact::InodeNode::Branch { children, .. } => {
                            pending.extend(
                                children
                                    .into_iter()
                                    .map(|(_, id)| (id, Role::InodeTable, Origin::Structure)),
                            );
                        }
                    }
                }
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
                    census_inode(&mut pending, inode::codec::decode_inode_record(bytes)?);
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
                    extent_codec::decode_chunk_payload(layerfs_content::decode_bytes_object(
                        bytes,
                    )?)?;
                }
                Role::SmallContent => {
                    small_bytes(bytes)?.ok_or("SmallContent role mismatch")?;
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
    extents: BTreeMap<String, Vec<Extent>>,
    pub assurance: String,
    native_reference_readback: bool,
}
fn read_gzip_text(path: &Path) -> AnyResult<String> {
    let output = Command::new("/usr/bin/gzip")
        .args(["-dc"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err("fast certificate gzip failed".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn require_bound_bytes(bytes: &[u8], expected: &str, scope: &str) -> AnyResult<()> {
    if !hex_digest(expected) || workload_source::sdk_edit_common::sha256_hex(bytes) != expected {
        return Err(format!("fast certificate {scope} hash mismatch").into());
    }
    Ok(())
}
fn require_bound_file(path: &Path, expected: &str, scope: &str) -> AnyResult<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    require_bound_bytes(&bytes, expected, scope)?;
    Ok(bytes)
}
fn verify_component_mapping(
    mapping: &str,
    expected: &BTreeMap<&str, &Entry>,
    regular: &BTreeSet<String>,
) -> AnyResult<()> {
    let mut lines = mapping.lines();
    if lines.next() != Some("target_path\tsource_path\tcontent_recipe_sha256") {
        return Err("component mapping header".into());
    }
    let mut mapped = BTreeSet::new();
    for line in lines {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 || columns[1].is_empty() || !mapped.insert(columns[0].to_owned()) {
            return Err("component mapping row".into());
        }
        let entry = expected
            .get(columns[0])
            .ok_or("component target outside independent input")?;
        let content = match &entry.kind {
            EntryKind::File(c) => c,
            EntryKind::Hardlink(target) => match &expected
                .get(target.as_str())
                .ok_or("component alias target")?
                .kind
            {
                EntryKind::File(c) => c,
                _ => return Err("component alias content".into()),
            },
            _ => return Err("component mapped nonregular path".into()),
        };
        if common::content_recipe_identity(content)? != columns[2] {
            return Err("component independent recipe mismatch".into());
        }
    }
    if &mapped != regular {
        return Err("component mapping exact membership".into());
    }
    Ok(())
}

impl FastCertificate {
    pub(crate) fn independent(root: ObjectId, binding: String) -> Self {
        Self {
            binding,
            root,
            file_roots: BTreeMap::new(),
            extents: BTreeMap::new(),
            assurance: "independent_current_content".into(),
            native_reference_readback: false,
        }
    }
    pub(crate) fn covered_paths(&self, entries: &[Entry], need_extents: bool) -> BTreeSet<String> {
        entries
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File(_) | EntryKind::Hardlink(_)))
            .filter(|entry| {
                self.file_roots.contains_key(&entry.path)
                    && (!need_extents || self.extents.contains_key(&entry.path))
            })
            .map(|entry| entry.path.clone())
            .collect()
    }
    fn require_pristine_root(&self, root: ObjectId) -> AnyResult<()> {
        if self.root != root {
            return Err("fast certificate pristine root mismatch".into());
        }
        Ok(())
    }
    pub(crate) fn load(seed: u8, pristine_root: ObjectId, fixture: &[Entry]) -> AnyResult<Self> {
        let projection = require_bound_file(
            Path::new(&std::env::var("LAYERFS_V013_FAST_CERTIFICATE")?),
            &std::env::var("LAYERFS_V013_FAST_CERTIFICATE_SHA256")?,
            "TSV projection",
        )?;
        let text = String::from_utf8(projection)?;
        let mut fields = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line
                .split_once('\t')
                .ok_or("fast certificate TSV columns")?;
            if value.is_empty() || fields.insert(key, value).is_some() {
                return Err("fast certificate duplicate/empty field".into());
            }
        }
        let get = |key| {
            fields
                .get(key)
                .copied()
                .ok_or("missing fast certificate field")
        };
        if !matches!(get("profile")?, "fast-verify-v1" | "fast-verify-v2")
            || get("seed")?.parse::<u8>()? != seed
            || get("input_plan_sha256")? != std::env::var("LAYERFS_V013_FAST_INPUT_PLAN_SHA256")?
        {
            return Err("fast certificate profile/seed/input mismatch".into());
        }
        let binding = get("certificate_sha256")?.to_owned();
        if !hex_digest(&binding)
            || workload_source::sdk_edit_common::sha256_hex(&std::fs::read(get(
                "certificate_json",
            )?)?)
                != binding
        {
            return Err("fast certificate binding mismatch".into());
        }
        for key in [
            "source_attempt",
            "source_revision",
            "product_seal",
            "certificate_manifest_sha256",
        ] {
            get(key)?;
        }
        let root = get("root")?.parse()?;
        let assurance = fields
            .get("reference_assurance")
            .copied()
            .unwrap_or("fully_verified");
        if !matches!(
            assurance,
            "fully_verified" | "canonical_input_qualified" | "qualified_content_components"
        ) {
            return Err("unsupported fast reference assurance".into());
        }
        let components = assurance == "qualified_content_components";
        let native_reference_readback = fields
            .get("reference_native_readback")
            .copied()
            .unwrap_or(if assurance == "fully_verified" {
                "true"
            } else {
                "false"
            })
            .parse::<bool>()?;
        if !components && root != pristine_root {
            return Err("fast certificate pristine root mismatch".into());
        }
        require_bound_file(
            Path::new(get("certificate_manifest")?),
            get("certificate_manifest_file_sha256")?,
            "compressed manifest",
        )?;
        require_bound_file(
            Path::new(get("certificate_file_roots")?),
            get("certificate_file_roots_sha256")?,
            "compressed file roots",
        )?;
        let manifest = read_gzip_text(Path::new(get("certificate_manifest")?))?;
        if workload_source::sdk_edit_common::sha256_hex(manifest.as_bytes())
            != get("oracle_identity")?
        {
            return Err("fast certificate independent manifest identity".into());
        }
        let certified = common::decode_manifest(&manifest)?;
        let expected = fixture
            .iter()
            .map(|e| (e.path.as_str(), e))
            .collect::<BTreeMap<_, _>>();
        if !components && certified.len() != expected.len() {
            return Err("fast certificate input membership".into());
        }
        for entry in &certified {
            let wanted = expected
                .get(entry.path.as_str())
                .ok_or("fast certificate extra input path")?;
            let kinds_match = match (&entry.kind, &wanted.kind) {
                (EntryKind::File(a), EntryKind::File(b)) => a.len() == b.len(),
                (EntryKind::Directory, EntryKind::Directory) => true,
                (EntryKind::Symlink(a), EntryKind::Symlink(b))
                | (EntryKind::Hardlink(a), EntryKind::Hardlink(b)) => a == b,
                _ => false,
            };
            if !kinds_match
                || (entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds)
                    != (wanted.mode, wanted.mtime_seconds, wanted.mtime_nanoseconds)
            {
                return Err("fast certificate independent input descriptor mismatch".into());
            }
        }
        let roots = read_gzip_text(Path::new(get("certificate_file_roots")?))?;
        let mut lines = roots.lines();
        if lines.next() != Some("path\tcontent_root") {
            return Err("fast certificate file-root header".into());
        }
        let mut file_roots = BTreeMap::new();
        for line in lines {
            let (path, root) = line
                .split_once('\t')
                .ok_or("fast certificate file-root columns")?;
            if file_roots.insert(path.to_owned(), root.parse()?).is_some() {
                return Err("fast certificate duplicate file root".into());
            }
        }
        let regular = certified
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::File(_) | EntryKind::Hardlink(_)))
            .map(|e| e.path.clone())
            .collect::<BTreeSet<_>>();
        if regular != file_roots.keys().cloned().collect() {
            return Err("fast certificate regular membership".into());
        }
        if components {
            let mapping = String::from_utf8(require_bound_file(
                Path::new(get("certificate_mapping")?),
                get("certificate_mapping_sha256")?,
                "component mapping",
            )?)?;
            verify_component_mapping(&mapping, &expected, &regular)?;
        }
        let mut extents = BTreeMap::<String, Vec<Extent>>::new();
        if let Some(path) = fields.get("certificate_extents") {
            require_bound_file(
                Path::new(path),
                get("certificate_extents_sha256")?,
                "compressed extents",
            )?;
            let text = read_gzip_text(Path::new(path))?;
            let mut lines = text.lines();
            if lines.next()
                != Some("path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length")
            {
                return Err("certificate extent header".into());
            }
            for line in lines {
                let c = line.split('\t').collect::<Vec<_>>();
                if c.len() != 6 || !regular.contains(c[0]) {
                    return Err("certificate extent path/columns".into());
                }
                let rows = extents.entry(c[0].into()).or_default();
                if c[1].parse::<usize>()? != rows.len() {
                    return Err("certificate extent ordinal".into());
                }
                let extent = Extent {
                    id: c[2].parse()?,
                    source_offset: c[3].parse()?,
                    len: c[4].parse()?,
                    payload_len: c[5].parse()?,
                };
                if extent
                    .source_offset
                    .checked_add(extent.len)
                    .is_none_or(|end| end > extent.payload_len)
                {
                    return Err("certificate extent range".into());
                }
                rows.push(extent);
            }
            for entry in &certified {
                let len = match &entry.kind {
                    EntryKind::File(c) => c.len(),
                    EntryKind::Hardlink(target) => match &expected[target.as_str()].kind {
                        EntryKind::File(c) => c.len(),
                        _ => return Err("certificate alias length".into()),
                    },
                    _ => continue,
                };
                let rows = extents.entry(entry.path.clone()).or_default();
                let sum = rows
                    .iter()
                    .try_fold(0u64, |sum, e| sum.checked_add(e.len))
                    .ok_or("certificate extent total overflow")?;
                if sum != len {
                    return Err("certificate extent length".into());
                }
            }
        }
        let certificate = Self {
            binding,
            root,
            file_roots,
            extents,
            assurance: assurance.into(),
            native_reference_readback,
        };
        if !components {
            certificate.require_pristine_root(pristine_root)?;
        }
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
    Ok(verify_fast_snapshot(source, root, entries, delta, certificate, false)?.receipt)
}

pub(crate) struct FastSnapshotEvidence {
    pub receipt: Receipt,
    pub extents: BTreeMap<String, Vec<Extent>>,
    pub file_roots: BTreeMap<String, ObjectId>,
}

pub(crate) fn verify_fast_snapshot(
    source: &dyn ObjectSource,
    root: ObjectId,
    entries: &[Entry],
    delta: &common::FastDelta,
    certificate: &FastCertificate,
    collect_extents: bool,
) -> AnyResult<FastSnapshotEvidence> {
    if !hex_digest(&certificate.binding) {
        return Err("fast certificate binding format".into());
    }
    if entries
        .iter()
        .any(|e| matches!(e.kind, EntryKind::File(Content::Digest { .. })))
    {
        return Err("fast expected bytes require independent source".into());
    }
    common::validate_entries(entries)?;
    let reader = CoreReader(source);
    let namespace = AuthenticatedNamespaceIndex::load(source, root)?;
    let expected_by_path = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let covered = certificate.covered_paths(entries, collect_extents);
    let mut selected_delta = common::FastDelta {
        changed_paths: delta.changed_paths.clone(),
        absent_paths: delta.absent_paths.clone(),
        witness_paths: delta.witness_paths.clone(),
    };
    for entry in entries {
        if matches!(entry.kind, EntryKind::File(_) | EntryKind::Hardlink(_))
            && !covered.contains(&entry.path)
        {
            selected_delta.witness_paths.insert(entry.path.clone());
        }
    }
    let selected = common::fast_selected_paths(entries, &selected_delta)?;
    let mut file_roots = BTreeMap::new();
    let mut extents = BTreeMap::new();
    let mut reference_counts = BTreeMap::<&str, u64>::new();
    for entry in entries {
        let class = match &entry.kind {
            EntryKind::File(_) => entry.path.as_str(),
            EntryKind::Hardlink(target) => target,
            _ => continue,
        };
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
        if !found.insert(path.clone()) {
            return Err("fast namespace duplicate path".into());
        }
        let expected = expected_by_path
            .get(path.as_str())
            .ok_or("fast namespace extra path")?;
        let record = namespace.resolve_inode(id)?.record;
        record.validate(path == ".")?;
        if !visited_inodes.insert(id) && record.kind != inode::InodeKind::RegularFile {
            return Err("fast namespace repeats a non-regular inode".into());
        }
        let metadata_key = (
            record.metadata_root,
            record.kind as u8,
            expected.mode,
            expected.mtime_seconds,
            expected.mtime_nanoseconds,
        );
        if validated_metadata.insert(metadata_key) {
            directory::validate_inode_record_metadata(&reader, record, path == ".")?;
            verify_metadata(&reader, record.metadata_root, expected)?;
        }
        match &expected.kind {
            EntryKind::Directory => {
                if record.kind != inode::InodeKind::Directory {
                    return Err("fast directory type".into());
                }
                directory::visit_directory_entries(
                    &reader,
                    directory::DirectoryStateRoot(record.content_root),
                    &mut Default::default(),
                    |page| {
                        for (name, inode) in page {
                            pending.push((
                                if path == "." {
                                    name.as_str().to_owned()
                                } else {
                                    format!("{path}/{}", name.as_str())
                                },
                                *inode,
                            ));
                        }
                        Ok(())
                    },
                )?;
            }
            EntryKind::Symlink(target) => {
                if record.kind != inode::InodeKind::Symlink
                    || reader
                        .with_authenticated_canonical(
                            record.content_root,
                            directory::codec::decode_symlink,
                        )?
                        .target
                        != target.as_bytes()
                {
                    return Err("fast symlink target/type".into());
                }
            }
            EntryKind::File(_) | EntryKind::Hardlink(_) => {
                let (content, class) = match &expected.kind {
                    EntryKind::File(content) => (content, path.as_str()),
                    EntryKind::Hardlink(target) => match &expected_by_path
                        .get(target.as_str())
                        .ok_or("fast alias target")?
                        .kind
                    {
                        EntryKind::File(content) => (content, target.as_str()),
                        _ => return Err("fast alias content".into()),
                    },
                    _ => unreachable!(),
                };
                if record.kind != inode::InodeKind::RegularFile
                    || record.namespace_ref_count != reference_counts[class]
                    || inode_classes
                        .insert(id, class.to_owned())
                        .is_some_and(|old| old != class)
                    || class_inodes
                        .insert(class.to_owned(), id)
                        .is_some_and(|old| old != id)
                {
                    return Err(format!(
                        "fast regular type/alias/reference count: {path} observed {} declared {}",
                        record.namespace_ref_count, reference_counts[class]
                    )
                    .into());
                }
                if !delta.changed_paths.contains(&path)
                    && certificate
                        .file_roots
                        .get(&path)
                        .is_some_and(|old| *old != record.content_root)
                {
                    return Err("fast unchanged content reference differs from certificate".into());
                }
                if collect_extents {
                    file_roots.insert(path.clone(), record.content_root);
                }
                if selected.contains(&path) {
                    let file_root = rope::FileStateRoot(record.content_root);
                    validate_regular(&reader, file_root)?;
                    let length = regular_length(&reader, file_root)?;
                    if length != content.len() {
                        return Err("fast changed/witness length".into());
                    }
                    let mut sink = CompareSink {
                        expected: content,
                        offset: 0,
                        scratch: &mut scratch,
                        custody_hash: None,
                    };
                    read_regular(&reader, file_root, 0..content.len(), &mut sink)?;
                    if sink.offset != content.len() {
                        return Err("fast changed/witness short read".into());
                    }
                    actual_paths += 1;
                    actual_bytes += content.len();
                    if collect_extents {
                        extents.insert(path.clone(), read_file_extents(&reader, file_root)?);
                    }
                } else {
                    skipped_paths += 1;
                    skipped_bytes += content.len();
                    if collect_extents {
                        extents.insert(
                            path.clone(),
                            certificate
                                .extents
                                .get(&path)
                                .ok_or("missing certified extent reference")?
                                .clone(),
                        );
                    }
                }
            }
        }
    }
    if found.iter().map(String::as_str).collect::<BTreeSet<_>>()
        != expected_by_path.keys().copied().collect()
        || delta.absent_paths.iter().any(|path| found.contains(path))
    {
        return Err("fast exact namespace membership/absence".into());
    }
    namespace.require_complete_membership(&visited_inodes)?;
    let mut receipt=Receipt::from([
        ("verification_status".into(),"fast_iteration_verified".into()),
        ("verification_profile".into(),"fast-verify-v2".into()),
        ("reference_assurance".into(),certificate.assurance.clone()),
        ("reference_native_readback".into(),certificate.native_reference_readback.to_string()),
        ("has_reused_reference".into(),(skipped_paths>0).to_string()),
        ("covered_regular_paths".into(),covered.len().to_string()),
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
    ]);
    if certificate.assurance == "independent_current_content" {
        receipt.remove("certificate_root");
        receipt.insert("reference_root_scope".into(), "none".into());
    } else if certificate.assurance == "qualified_content_components" {
        receipt.remove("certificate_root");
        receipt.insert(
            "source_certificate_root".into(),
            certificate.root.to_string(),
        );
        receipt.insert(
            "reference_root_scope".into(),
            "mapped content components only; target pristine namespace not certified".into(),
        );
    } else {
        receipt.insert("reference_root_scope".into(), "exact pristine input".into());
    }
    if collect_extents {
        receipt.insert(
            "extent_assurance".into(),
            "selected-current-reads-and-certified-unchanged-root-references".into(),
        );
    }
    Ok(FastSnapshotEvidence {
        receipt,
        extents,
        file_roots,
    })
}

/// One small aggregate verifier check; caller schedules it separately from samples.
pub(crate) fn fast_qualification(root: &Path) -> AnyResult<Receipt> {
    if root.exists() {
        return Err("fast qualification output already exists".into());
    }
    std::fs::create_dir_all(root)?;
    let fixture = root.join("input");
    let entries = common::native_qualification_entries();
    common::create_fixture(&fixture, &entries)?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new("fast-qualification")?,
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
    let full = verify_root(&pinned.reader, pinned.root, &entries)
        .map_err(|error| format!("fast qualification exhaustive positive: {error}"))?;
    // Root and the empty directory deliberately share metadata. Reject a later
    // path's different expectation even after the root has warmed the memo.
    let reader = CoreReader(&pinned.reader);
    let root_record = layerfs_content::filesystem::resolve(
        &reader,
        pinned.root,
        &CanonicalPath::root(),
        &mut Default::default(),
    )?
    .record;
    let empty_record = layerfs_content::filesystem::resolve(
        &reader,
        pinned.root,
        &CanonicalPath::new("empty")?,
        &mut Default::default(),
    )?
    .record;
    if root_record.metadata_root != empty_record.metadata_root {
        return Err("qualification requires shared directory metadata".into());
    }
    let mut wrong_metadata = entries.clone();
    wrong_metadata
        .iter_mut()
        .find(|entry| entry.path == "empty")
        .ok_or("qualification empty directory")?
        .mode ^= 1;
    let metadata_rejection = verify_root(&pinned.reader, pinned.root, &wrong_metadata)
        .err()
        .ok_or("exhaustive verifier reused shared metadata despite different expected mode")?;
    let certificate = FastCertificate {
        binding: "ab".repeat(32),
        root: pinned.root,
        file_roots: full.file_roots,
        extents: full.extents,
        assurance: "fully_verified".into(),
        native_reference_readback: true,
    };
    let delta_for = |oracle: &[Entry]| common::FastDelta {
        changed_paths: oracle.iter().map(|e| e.path.clone()).collect(),
        absent_paths: BTreeSet::new(),
        witness_paths: BTreeSet::new(),
    };
    let mut receipt = verify_fast_root(
        &pinned.reader,
        pinned.root,
        &entries,
        &delta_for(&entries),
        &certificate,
    )
    .map_err(|error| format!("fast qualification canonical positive: {error}"))?;
    receipt.insert(
        "exhaustive_shared_metadata_expected_rejection".into(),
        metadata_rejection.to_string(),
    );
    let mut rejections = 0;
    let mut reject = |name: &str, oracle: &[Entry]| -> AnyResult<()> {
        let error = verify_fast_root(
            &pinned.reader,
            pinned.root,
            oracle,
            &delta_for(oracle),
            &certificate,
        )
        .err()
        .ok_or_else(|| format!("fast canonical accepted {name}"))?;
        receipt.insert(format!("rejected_{name}"), error.to_string());
        rejections += 1;
        Ok(())
    };
    let mut wrong = entries.clone();
    let file = wrong
        .iter_mut()
        .find(|e| e.path == "payload")
        .ok_or("qualification payload")?;
    let EntryKind::File(content) = &file.kind else {
        return Err("qualification file kind".into());
    };
    file.kind = EntryKind::File(content.xor(17, 1, 1)?);
    reject("changed_bytes", &wrong)?;
    let mut extra = entries.clone();
    extra.push(Entry::directory("extra"));
    reject("missing_namespace", &extra)?;
    let missing = entries
        .iter()
        .filter(|e| e.path != "empty")
        .cloned()
        .collect::<Vec<_>>();
    reject("extra_namespace", &missing)?;
    let mut wrong = entries.clone();
    wrong[0].mode ^= 1;
    reject("metadata", &wrong)?;
    let mut wrong = entries.clone();
    let content = match &entries[2].kind {
        EntryKind::File(c) => c.clone(),
        _ => return Err("qualification content".into()),
    };
    wrong
        .iter_mut()
        .find(|e| e.path == "alias")
        .ok_or("qualification alias")?
        .kind = EntryKind::File(content);
    reject("alias_class", &wrong)?;
    let wrong_root = "11".repeat(32).parse()?;
    if certificate.require_pristine_root(wrong_root).is_ok() {
        return Err("fast certificate accepted wrong pristine root".into());
    }
    rejections += 1;
    let reference_delta = common::FastDelta {
        changed_paths: BTreeSet::new(),
        absent_paths: BTreeSet::new(),
        witness_paths: BTreeSet::new(),
    };
    verify_fast_root(
        &pinned.reader,
        pinned.root,
        &entries,
        &reference_delta,
        &certificate,
    )?;
    let mut wrong_certificate = FastCertificate {
        binding: certificate.binding.clone(),
        root: certificate.root,
        file_roots: certificate.file_roots.clone(),
        extents: certificate.extents.clone(),
        assurance: certificate.assurance.clone(),
        native_reference_readback: true,
    };
    wrong_certificate
        .file_roots
        .insert("payload".into(), wrong_root);
    if verify_fast_root(
        &pinned.reader,
        pinned.root,
        &entries,
        &reference_delta,
        &wrong_certificate,
    )
    .is_ok()
    {
        return Err("fast verifier accepted wrong certified content root".into());
    }
    rejections += 1;
    let projection = b"profile\tfast-verify-v1\nroot\tqualified-root\n";
    let projection_hash = workload_source::sdk_edit_common::sha256_hex(projection);
    require_bound_bytes(projection, &projection_hash, "TSV projection")?;
    if require_bound_bytes(
        b"profile\tfast-verify-v1\nroot\tdrifted-root\n",
        &projection_hash,
        "TSV projection",
    )
    .is_ok()
    {
        return Err("fast certificate accepted modified TSV projection".into());
    }
    rejections += 1;
    let artifact = root.join("bound-artifact-negative.bin");
    let artifact_bytes = b"compressed artifact bytes";
    let artifact_hash = workload_source::sdk_edit_common::sha256_hex(artifact_bytes);
    std::fs::write(&artifact, artifact_bytes)?;
    require_bound_file(&artifact, &artifact_hash, "compressed file roots")?;
    std::fs::write(&artifact, b"modified compressed artifact bytes")?;
    if require_bound_file(&artifact, &artifact_hash, "compressed file roots").is_ok() {
        return Err("fast certificate accepted modified bound artifact".into());
    }
    rejections += 1;
    // No-reference coverage cannot silently omit an unchanged body.
    let independent = FastCertificate::independent(pinned.root, "cd".repeat(32));
    let no_reference = verify_fast_snapshot(
        &pinned.reader,
        pinned.root,
        &entries,
        &reference_delta,
        &independent,
        true,
    )?;
    if no_reference
        .receipt
        .get("skipped_current_store_regular_paths")
        .map(String::as_str)
        != Some("0")
        || no_reference.extents.len() != 2
        || no_reference.file_roots.len() != 2
    {
        return Err("no-reference fast coverage omitted current content/extents".into());
    }
    let mut wrong = entries.clone();
    let file = wrong
        .iter_mut()
        .find(|entry| entry.path == "payload")
        .ok_or("qualification payload")?;
    let EntryKind::File(content) = &file.kind else {
        return Err("qualification content".into());
    };
    file.kind = EntryKind::File(content.xor(23, 1, 1)?);
    if verify_fast_root(
        &pinned.reader,
        pinned.root,
        &wrong,
        &reference_delta,
        &independent,
    )
    .is_ok()
    {
        return Err("no-reference fast verifier accepted wrong unchecked content".into());
    }
    rejections += 1;
    let by_path = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let covered = ["payload".to_owned()].into_iter().collect::<BTreeSet<_>>();
    let content = match &entries[2].kind {
        EntryKind::File(c) => c,
        _ => return Err("qualification mapping content".into()),
    };
    let mapping = format!(
        "target_path\tsource_path\tcontent_recipe_sha256\npayload\tsource-payload\t{}\n",
        common::content_recipe_identity(content)?
    );
    verify_component_mapping(&mapping, &by_path, &covered)?;
    let wrong_mapping = format!(
        "target_path\tsource_path\tcontent_recipe_sha256\npayload\tsource-payload\t{}\n",
        "0".repeat(64)
    );
    if verify_component_mapping(&wrong_mapping, &by_path, &covered).is_ok() {
        return Err("component mapping accepted incompatible independent content recipe".into());
    }
    rejections += 1;
    receipt.insert("canonical_negative_checks".into(), rejections.to_string());
    receipt.insert("qualification_status".into(), "pass".into());
    let native = common::fast_qualification(&root.join("native"))?;
    for (key, value) in native {
        receipt.insert(format!("native_{key}"), value);
    }
    let mut output = std::fs::File::create(root.join("qualification-receipt.txt"))?;
    for (key, value) in &receipt {
        writeln!(output, "{key}={value}")?;
    }
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

#[cfg(test)]
mod sampled_tests {
    use super::*;

    #[test]
    fn bounded_workspace_samples_match_recipes_and_reject_observed_corruption() -> AnyResult<()> {
        for case in workload_source::tiny_file_churn::cases() {
            let sample = workload_source::ordinary_workloads::tiny_sample(&case, 1)?;
            sample.validate()?;
            if case.tier <= 10 {
                let expected = workload_source::tiny_file_churn::expected(&case, 1, 1)?;
                for entry in &sample.entries {
                    let matching = expected
                        .iter()
                        .find(|candidate| candidate.path == entry.path)
                        .ok_or("sample not in oracle")?;
                    assert_eq!(format!("{entry:?}"), format!("{matching:?}"));
                }
                for absent in &sample.absent {
                    assert!(!expected.iter().any(|entry| &entry.path == absent));
                }
            }
        }
        for case in workload_source::workspace_change_locality::cases()
            .into_iter()
            .filter(|case| workload_source::ordinary_workloads::mixed_v4(case))
        {
            let sample = workload_source::ordinary_workloads::workspace_sample(&case, 1)?;
            sample.validate()?;
            let large = workload_source::workspace_common::mixed_v4_large_sizes(case.tier)?;
            assert_eq!(sample.ranges.len(), large.len());
            for ordinal in 0..large.len() {
                let path =
                    workload_source::ordinary_workloads::shard_path(ordinal / 200, ordinal % 200);
                assert_eq!(sample.ranges.get(&path).map(Vec::len), Some(3));
            }
        }
        for case in workload_source::dedup_branch_history::cases()
            .into_iter()
            .filter(|case| workload_source::dedup_workloads::history_unrelated_mixed_v2(case))
        {
            let sample = workload_source::ordinary_workloads::workspace_sample(&case, 1)?;
            sample.validate()?;
            let large = workload_source::dedup_workloads::mixed_v2_path(0)?;
            assert_eq!(sample.ranges.len(), 1);
            assert_eq!(sample.ranges.get(&large).map(Vec::len), Some(3));
        }
        let root = std::env::temp_dir().join(format!(
            "layerfs-sampled-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        let entries = vec![
            Entry::directory("."),
            Entry::file(
                "payload",
                Content::Seed {
                    seed: 71,
                    len: 131072,
                },
            ),
        ];
        let native = root.join("native");
        common::create_fixture(&native, &entries)?;
        let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
        let client = Client::connect(store.clone())?;
        let initialized = client.initialize_layerstack(
            EntityName::new("sample-check")?,
            LayerStackInitialization::Directory(native.clone()),
        )?;
        let branch = client.fork_branch(
            EntityName::new("main")?,
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )?;
        let pinned = store.pin_branch(branch)?;
        let mut sample = common::TreeSample {
            entries: entries.clone(),
            absent: vec!["missing/child".into()],
            ranges: BTreeMap::new(),
        };
        verify_sample(&pinned.reader, pinned.root, &sample)?;
        common::verify_native_sample(&native, &sample)?;
        // Untouched native state is neither enumerated nor read by selection.
        std::fs::File::create(native.join("untouched"))?.set_len(500 * common::MIB)?;
        common::set_metadata(&native, &entries[0])?;
        common::verify_native_sample(&native, &sample)?;
        sample.entries[1].mode ^= 1;
        assert!(verify_sample(&pinned.reader, pinned.root, &sample).is_err());
        assert!(common::verify_native_sample(&native, &sample).is_err());
        sample.entries[1] = Entry::file(
            "payload",
            Content::Seed {
                seed: 72,
                len: 131072,
            },
        );
        assert!(verify_sample(&pinned.reader, pinned.root, &sample).is_err());
        assert!(common::verify_native_sample(&native, &sample).is_err());
        // A non-prefix range must be checked by both readers.
        sample.entries = entries.clone();
        sample
            .ranges
            .insert("payload".into(), vec![(65536, 64), (131008, 64)]);
        verify_sample(&pinned.reader, pinned.root, &sample)?;
        common::verify_native_sample(&native, &sample)?;
        sample.entries[1] = Entry::file(
            "payload",
            Content::Xor {
                source: std::sync::Arc::new(Content::Seed {
                    seed: 71,
                    len: 131072,
                }),
                offset: 65536,
                len: 1,
                mask: 1,
            },
        );
        assert!(verify_sample(&pinned.reader, pinned.root, &sample).is_err());
        assert!(common::verify_native_sample(&native, &sample).is_err());
        sample.ranges.clear();
        sample.entries = vec![entries[0].clone()];
        sample.absent = vec!["payload".into()];
        assert!(verify_sample(&pinned.reader, pinned.root, &sample).is_err());
        assert!(common::verify_native_sample(&native, &sample).is_err());
        drop(pinned);
        drop(client);
        drop(store);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}

#[cfg(test)]
mod small_content_checks {
    use super::*;
    #[test]
    fn small_content_grammar_and_identity_are_bounded() {
        for length in [1, 8192, 131071] {
            let raw = vec![42; length];
            let mut value = b"LFS5SML\0\0\x01".to_vec();
            value.extend_from_slice(&raw);
            let canonical = layerfs_content::encode_bytes_object(&value).unwrap();
            assert_eq!(small_bytes(&canonical).unwrap(), Some(raw.as_slice()));
            assert_eq!(
                expected_small_root(&raw).unwrap(),
                ObjectId::for_bytes(&canonical)
            );
            value[9] = 2;
            assert!(small_bytes(&layerfs_content::encode_bytes_object(&value).unwrap()).is_err());
        }
        for raw in [vec![], vec![0; 131072]] {
            assert!(expected_small_root(&raw).is_err());
        }
        assert!(
            small_bytes(&layerfs_content::encode_bytes_object(b"LFS5SML\0\0\x01").unwrap())
                .is_err()
        );
    }
}
