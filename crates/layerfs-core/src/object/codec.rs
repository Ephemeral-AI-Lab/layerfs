use std::io::{Cursor, Read, Write};

use crate::error::{CoreError, CoreResult};
use crate::identity::{ObjectId, DIGEST_BYTES};
use crate::limits::{
    MAX_CHILD_REFERENCES, MAX_COMPONENT_BYTES, MAX_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES,
};
use crate::object::model::{DirectoryEntry, Object, ObjectKind, ObjectReference};
use crate::CanonicalName;

include!("codec/framing.rs");
include!("codec/encode.rs");
include!("codec/decode.rs");
include!("codec/authentication.rs");
include!("codec/tests.rs");
