mod codec;
mod model;

pub use codec::{
    decode_object, decode_object_from, encode_object, encode_object_to, validate_identity,
    HEADER_LEN, MAGIC,
};
pub use model::{DirectoryEntry, Object, ObjectKind, ObjectReference};
