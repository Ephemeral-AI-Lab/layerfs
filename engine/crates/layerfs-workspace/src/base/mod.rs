//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod batch;
pub(crate) mod cache;
pub(crate) mod client;
pub(crate) mod file;
pub(crate) mod links;
pub(crate) mod view;
pub(crate) use batch::BaseFiles;
pub use links::DIRECTORY_COUNT_CAPACITY;
pub(crate) use links::REMEMBERED_DIRECTORIES;
pub use view::{BaseContent, BaseRead, BaseStat, BaseView};
