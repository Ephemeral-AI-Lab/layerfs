use super::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_ENTRIES, MAX_LEVEL,
};
use super::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use crate::cdc::FastCdc;
use crate::{encode_bytes_object, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::ops::Range;

include!("rope/model.rs");
include!("rope/build.rs");
include!("rope/read.rs");
include!("rope/diff.rs");
include!("rope/edit.rs");
include!("rope/tree_edit.rs");
include!("rope/validation.rs");
