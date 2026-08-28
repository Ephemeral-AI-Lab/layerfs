use crate::object::access::ObjectRead;
use crate::{decode_bytes_object, CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};

include!("decoder/model.rs");
include!("decoder/mapping.rs");
include!("decoder/transition.rs");
include!("decoder/framing.rs");
