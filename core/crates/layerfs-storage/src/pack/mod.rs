//! Pack grammars, group framing, placement and record extraction.
//!
//! Entry module: declarations and re-exports only.

pub mod assemble;
pub mod layout;
pub mod placement;

pub use assemble::{
    assemble, assemble_consuming, body_bytes, build_group, control_area, directory_entries,
    frame_group, frame_group_bounded, framed_group_length, framed_length, FULL_TAG,
};
pub use layout::{
    append_fits, assembled_length, body_area_offset, declared_length, directory_capacity,
    directory_entry_len, group_view, pack_capacity, parse_header, EncodedGroup, GroupCodec,
    GroupView, PackHeader, PackLane, DIRECTORY_ENTRY_LEN, HEADER_LEN, PACK_MAGIC, VERSION_NATIVE,
    VERSION_ORDINARY, VERSION_POOLED_METADATA, VERSION_SINGLETON, VERSION_WHOLE_FILE,
    WHOLE_FILE_COMPACT_DROP, WHOLE_FILE_ENTRY_LEN,
};
pub use placement::{LanePlacement, PlacedGroup, SelectedWrite};
