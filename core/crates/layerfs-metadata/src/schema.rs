//! Explicit schema creation; existing product tables are refused, never migrated.
use crate::{client::Client, config::PgConfig};
use layerfs_storage::{policy::StoragePolicy, port::MetadataError};
pub(crate) fn create(
    client: &Client,
    config: &PgConfig,
    policy: StoragePolicy,
) -> Result<(), MetadataError> {
    let policy = policy.validated().map_err(|_| MetadataError::Malformed)?;
    let sql = config
        .sql(include_str!("../sql/storage.sql"))
        .replace("${format}", &policy.format_profile().to_string())
        .replace(
            "${threshold}",
            &policy.small_file_threshold_bytes().to_string(),
        )
        .replace(
            "${whole_depth}",
            &policy.whole_file_delta_max_depth().to_string(),
        )
        .replace(
            "${chunk_depth}",
            &policy.chunk_delta_max_depth().to_string(),
        )
        .replace(
            "${metadata_depth}",
            &policy.metadata_delta_max_depth().to_string(),
        );
    client.bootstrap(sql)
}
