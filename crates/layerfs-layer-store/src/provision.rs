use crate::LayerStore;
use layerfs_storage::{
    empty_root, InitializedLayer, LayerHistoryId, LayerHistoryRecord, LayerId, LayerInitialization,
    LayerRecord, Result, StorageError, StorageId,
};

impl LayerStore {
    pub fn initialize(&self, source: LayerInitialization) -> Result<InitializedLayer> {
        let _operation = self.db.enter_operation()?;
        let history_id = LayerHistoryId::new();
        let seed = *blake3::hash(history_id.as_slice()).as_bytes();
        let root = match source {
            LayerInitialization::Empty => empty_root(seed)?,
            LayerInitialization::Directory(path) => {
                if !path.is_dir() {
                    return Err(StorageError::InvalidInput("Layer initialization directory"));
                }
                crate::provision::directory_root(&path, seed)?
            }
        };
        let layer = LayerRecord {
            id: LayerId::derive(history_id, None, root.root_id),
            history_id,
            parent_id: None,
            root_id: root.root_id,
        };
        let history = LayerHistoryRecord {
            id: history_id,
            head_layer_id: layer.id,
        };
        self.db
            .provision_layer_history(history, layer, &root.objects)?;
        Ok((history, layer))
    }
}

fn directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<layerfs_storage::BuiltRoot> {
    use layerfs_content::filesystem;
    use layerfs_content::CanonicalPath;
    use layerfs_storage::ObjectBuffer;
    use std::collections::HashMap;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut objects = ObjectBuffer::empty()?;
    let mut root = filesystem::empty_root(&mut objects, seed)?;
    let metadata = std::fs::symlink_metadata(path)?;
    root = filesystem::set_mode(
        &mut objects,
        root,
        &CanonicalPath::root(),
        metadata.permissions().mode(),
    )?
    .root();
    root = filesystem::set_mtime(
        &mut objects,
        root,
        &CanonicalPath::root(),
        metadata.mtime(),
        metadata.mtime_nsec() as u32,
    )?
    .root();
    let mut hard_links = HashMap::new();
    let mut scanned = 0;
    import_directory(
        path,
        &CanonicalPath::root(),
        seed,
        &mut objects,
        &mut root,
        &mut hard_links,
        &mut scanned,
    )?;
    objects.finish(root, scanned)
}

#[allow(clippy::too_many_arguments)]
fn import_directory(
    native: &std::path::Path,
    logical: &layerfs_content::CanonicalPath,
    seed: [u8; 32],
    objects: &mut layerfs_storage::ObjectBuffer<'_>,
    root: &mut layerfs_content::ObjectId,
    hard_links: &mut std::collections::HashMap<(u64, u64), layerfs_content::CanonicalPath>,
    scanned: &mut u64,
) -> Result<()> {
    use layerfs_content::filesystem;
    use layerfs_content::CanonicalName;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    for entry in entries {
        let name = CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        let path = child(logical, &name)?;
        let path_text = path.as_str().to_owned();
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let hard_link = (metadata.nlink() > 1)
            .then(|| hard_links.get(&(metadata.dev(), metadata.ino())).cloned())
            .flatten();
        if let Some(source) = hard_link {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::HardLink {
                    source: source.as_str().to_owned(),
                    target: path_text,
                }],
                seed,
            )?
            .root_id;
            continue;
        }
        if metadata.file_type().is_dir() {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::Mkdir {
                    path: path_text,
                    mode: metadata.permissions().mode(),
                }],
                seed,
            )?
            .root_id;
            import_directory(
                &entry.path(),
                &path,
                seed,
                objects,
                root,
                hard_links,
                scanned,
            )?;
        } else if metadata.file_type().is_file() {
            *root = filesystem::write_file(
                objects,
                *root,
                &path,
                std::fs::File::open(entry.path())?,
                metadata.permissions().mode(),
                seed,
            )?
            .root();
            *scanned = scanned.saturating_add(metadata.len());
        } else if metadata.file_type().is_symlink() {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::Symlink {
                    path: path_text,
                    target: std::fs::read_link(entry.path())?
                        .as_os_str()
                        .as_bytes()
                        .to_vec(),
                }],
                seed,
            )?
            .root_id;
        } else {
            return Err(StorageError::InvalidInput("unsupported Layer entry"));
        }
        *root = filesystem::set_mtime(
            objects,
            *root,
            &path,
            metadata.mtime(),
            metadata.mtime_nsec() as u32,
        )?
        .root();
        if metadata.nlink() > 1 {
            hard_links.insert((metadata.dev(), metadata.ino()), path);
        }
    }
    Ok(())
}

fn child(
    parent: &layerfs_content::CanonicalPath,
    name: &layerfs_content::CanonicalName,
) -> Result<layerfs_content::CanonicalPath> {
    let mut bytes = parent.as_bytes().to_vec();
    if !bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name.as_bytes());
    Ok(layerfs_content::CanonicalPath::from_bytes(&bytes)?)
}
