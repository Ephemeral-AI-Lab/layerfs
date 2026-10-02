//! Scoped C1/C2 preparation after a captured Exec, before thin C5 publication.
use crate::{
    construction::{self, ConstructionWork},
    engine::Engine,
    minio::Minio,
    strict_catalog::SaveContext,
    strict_client::Catalog,
    strict_scope::{FileConsumer, FileReader, MetadataConsumer, MetadataReader},
    strict_writer::{Counts, Writer},
};
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    FilesystemResult,
};
use layerfs_storage::{
    cas::PoolCounters, encoding::delta::select::DeltaCounters, StorageCapacities,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct Prepared {
    pub save: i64,
    pub candidate: FilesystemResult,
    pub construction: ConstructionWork,
    pub storage: Counts,
    pub delta: DeltaCounters,
    pub pool: PoolCounters,
}

/// The caller supplies a frozen live-row view with no SQL/ownership lock held
/// across this call. S2 owns that capture and its persistent scoped engine;
/// this function does not allocate a live database or replace that authority.
pub fn prepare(
    engine: &Engine,
    scope: InodeScope,
    base: FilesystemRootId,
    catalog: Arc<dyn Catalog>,
    provider: Minio,
    capacities: StorageCapacities,
) -> Result<Prepared, String> {
    finish_prepare(
        engine,
        scope,
        base,
        Writer::new(catalog, provider, capacities)?,
    )
}
pub fn prepare_owned(
    engine: &Engine,
    base: FilesystemRootId,
    catalog: Arc<dyn Catalog>,
    provider: Minio,
    capacities: StorageCapacities,
    context: &SaveContext,
) -> Result<Prepared, String> {
    if layerfs_content::filesystem::profile_id().as_bytes() != &context.profile {
        return Err("captured construction profile mismatch".into());
    }
    if base.0.as_bytes() != &context.base_root {
        return Err("captured construction base mismatch".into());
    }
    let scope = InodeScope::from_object(
        layerfs_content::ObjectId::from_bytes(&context.scope).map_err(|e| e.to_string())?,
    );
    finish_prepare(
        engine,
        scope,
        base,
        Writer::new_owned(catalog, provider, capacities, context)?,
    )
}
fn finish_prepare(
    engine: &Engine,
    scope: InodeScope,
    base: FilesystemRootId,
    writer: Writer,
) -> Result<Prepared, String> {
    let owner = Rc::new(RefCell::new(writer));
    let metadata_reader = MetadataReader {
        owner: owner.clone(),
    };
    let file_reader = FileReader {
        owner: owner.clone(),
    };
    let mut metadata_consumer = MetadataConsumer {
        owner: owner.clone(),
    };
    let mut file_consumer = FileConsumer {
        owner: owner.clone(),
    };
    let (candidate, construction) = match construction::build_scoped(
        engine,
        scope,
        base,
        &metadata_reader,
        &mut metadata_consumer,
        &file_reader,
        &mut file_consumer,
    ) {
        Ok(prepared) => prepared,
        Err(cause) => {
            return match owner.borrow_mut().stop_preparation(&cause) {
                Ok(()) => Err(cause),
                Err(custody) => Err(format!("{cause}; {custody}")),
            };
        }
    };
    let mut writer = owner.borrow_mut();
    writer.finish_storage()?;
    Ok(Prepared {
        save: writer.save,
        candidate,
        construction,
        storage: writer.counts.clone(),
        delta: writer.delta,
        pool: writer.pool_counts,
    })
}
