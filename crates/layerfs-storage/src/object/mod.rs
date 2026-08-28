//! Authenticated content-addressed object persistence.

mod authenticate;
mod page;
mod read;
mod write;

pub(crate) use authenticate::{
    authenticate_borrowed_unaccounted, payload_batch_sql,
    with_authenticated_canonical_on_connection, with_read_canonical_on_connection,
};
pub(crate) use read::core_store_error;
#[cfg(test)]
pub(crate) use read::{
    decode_delta_parts, delta_record_len, load_root_on_connection, root_record_len,
    visible_root_on_connection, write_root_on_connection,
};
pub(crate) use write::put_canonical_object_on_connection;
#[cfg(test)]
pub(crate) use write::{authenticate_directory_object, put_object_on_connection};
pub use write::{DeltaRecord, ObjectRecord, PutOutcome, RootId, RootRecord};
