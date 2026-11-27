//! Records an entity keeps in the home: the record file (`file`), its
//! fields, generation and observed facts, the one in-flight op intent
//! (`intent`), the process a record names, and ordered tokens.
pub mod fields;
pub mod file;
pub mod generation;
pub mod intent;
pub mod observed;
pub mod process;
pub mod token;
