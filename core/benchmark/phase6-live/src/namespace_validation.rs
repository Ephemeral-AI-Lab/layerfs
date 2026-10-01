//! Publication composition; canonical I/O never holds the locator SQL owner.
use crate::{metadata::Authority, namespace_index as index, objects::Reader, wire::Snapshot};
use layerfs_content::{filesystem::FilesystemRoot, AuthenticatedObjects, ObjectId};
fn root(reader: &Reader, id: [u8; 32]) -> Result<FilesystemRoot, String> {
    let id = ObjectId::from_bytes(&id).map_err(|e| e.to_string())?;
    FilesystemRoot::decode(&reader.read_canonical(id).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub fn initialize(a: &Authority) -> Result<(), String> {
    let s = a.snapshot()?;
    let reader = Reader::new(a.locators.s3.clone(), a.locators.clone());
    let fs = root(&reader, s.root)?;
    crate::audit::candidate(
        &reader,
        ObjectId::from_bytes(&s.root).map_err(|e| e.to_string())?,
        fs.scope(),
        fs.profile(),
        &a.locators,
    )?;
    a.locators
        .sql_transaction(|db| index::initialize_empty(db, &s, fs))
}
pub fn prepare(a: &Authority, s: &Snapshot, candidate: ObjectId) -> Result<(), String> {
    if index::stamp(&a.snapshot()?) != index::stamp(s) {
        return Err("namespace stale C5 selection".into());
    }
    // Refuse an uncertified source before acquiring candidate payloads.
    a.locators.sql(|db| index::check_base(db, s))?;
    let reader = Reader::new(a.locators.s3.clone(), a.locators.clone());
    let old = root(&reader, s.root)?;
    let new = root(&reader, *candidate.as_bytes())?;
    let work = a.locators.sql(|db| index::prepare(db, s, old, new))?;
    let mut after = 0;
    let mut portable = 0u64;
    let mut files = 0u64;
    let mut attribute_work = Default::default();
    let mut file_work = crate::file_facts::Work::default();
    loop {
        let change = a.locators.sql(|db| index::next_changed(db, after))?;
        let Some((id, old, new)) = change else { break };
        after = id;
        let Some(new) = new else { continue };
        if old.is_none_or(|v| v.metadata_root != new.metadata_root) {
            layerfs_content::filesystem::attributes::read::read_portable(
                &reader,
                new.metadata_root,
                new.kind,
                &mut attribute_work,
            )
            .map_err(|e| e.to_string())?;
            portable += 1;
        }
        if new.kind == layerfs_content::inode_leaf::InodeKind::RegularFile
            && old.is_none_or(|v| v.content_root != new.content_root)
        {
            let w = a.locators.certify_file(new.content_root)?;
            file_work.visited += w.visited;
            file_work.reused += w.reused;
            file_work.edges += w.edges;
            file_work.newly_certified += w.newly_certified;
            files += 1;
        }
    }
    a.locators
        .sql_transaction(|db| index::seal(db, s, candidate))?;
    eprintln!("P6_INCREMENTAL_AUDIT work={work:?} portable={portable} attribute={attribute_work:?} files={files} file_cert={file_work:?}");
    Ok(())
}
pub fn install(a: &Authority, old: &Snapshot, new: &Snapshot) -> Result<(), String> {
    a.locators.sql(|db| index::install_known(db, old, new))
}
