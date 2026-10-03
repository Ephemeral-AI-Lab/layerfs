//! Definite database refusal versus missing acknowledgement.
use layerfs_storage::port::MetadataError;
use std::error::Error;
pub(crate) fn database(error: postgres::Error, mutating: bool) -> MetadataError {
    if let Some(database) = error.as_db_error() {
        if database.severity() == "FATAL"
            || database.severity() == "PANIC"
            || mutating && database.code().code() == "57014"
        {
            return MetadataError::Uncertain;
        }
        return MetadataError::Refused {
            status: database.code().code().to_owned(),
        };
    }
    if let Some(cause) = error
        .source()
        .and_then(|source| source.downcast_ref::<native_tls::Error>())
    {
        return MetadataError::Refused {
            status: format!("TLS_IDENTITY: {cause}"),
        };
    }
    MetadataError::Uncertain
}
