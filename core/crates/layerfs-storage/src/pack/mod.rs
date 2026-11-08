//! Pack grammars, group framing and record extraction.
//!
//! Entry module: declarations and re-exports only.

pub mod assemble;
pub mod audit;
pub mod layout;

pub use assemble::{
    assemble, assemble_consuming, body_bytes, build_group, control_area, directory_entries,
    frame_group, frame_group_bounded, framed_group_length, framed_length, FULL_TAG,
};
pub use layout::{
    append_fits, assembled_length, body_area_offset, declared_length, directory_capacity,
    directory_entry_len, group_view, parse_header, sized_body_offset, EncodedGroup, GroupCodec,
    GroupView, PackHeader, PackLane, DIRECTORY_ENTRY_LEN, HEADER_LEN, PACK_MAGIC, VERSION_CHUNK,
    VERSION_CHUNK_STORED, VERSION_ORDINARY, VERSION_SINGLETON, VERSION_SINGLETON_STORED,
    VERSION_WHOLE_FILE, VERSION_WHOLE_FILE_STORED, WHOLE_FILE_COMPACT_DROP, WHOLE_FILE_ENTRY_LEN,
};
