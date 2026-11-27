//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod batch;
pub(crate) mod cache;
pub(crate) mod fetch;
pub(crate) mod file_facts;
pub(crate) mod link_counts;
pub(crate) mod read;
pub(crate) use batch::BaseFiles;
pub use link_counts::DIRECTORY_COUNT_CAPACITY;
pub(crate) use link_counts::REMEMBERED_DIRECTORIES;
pub use read::{BaseContent, BaseRead, BaseStat, BaseView};
