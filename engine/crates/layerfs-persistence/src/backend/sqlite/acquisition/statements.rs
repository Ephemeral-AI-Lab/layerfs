//! Closed statement set of SQLite acquisition.
//!
//! Every window bound is written `LIMIT ?n+0`. The engine's planner reads a
//! plainly bound LIMIT, which re-prepares the statement on each execution and
//! lets the plan follow the value. The expression keeps one generic plan: the
//! one `explain` reports.
macro_rules! statement {
    ($name:ident, $file:literal) => {
        pub(crate) const $name: &str = include_str!(concat!(
            "../../../../sql/sqlite/acquisition/",
            $file,
            ".sql"
        ));
    };
}
statement!(BEGIN, "operation/begin");
statement!(CLAIM_EPOCH, "operation/claim_epoch");
statement!(OWNER, "operation/owner");
statement!(CHARGE, "operation/charge");
statement!(CREDIT, "operation/credit");
statement!(ADVANCE, "operation/advance");
statement!(RELEASE, "operation/release");
statement!(PUT_ENTRY, "put/entry");
statement!(BIND_HOST_FILE, "write/bind_host_file");
statement!(ENTRY_DEPENDENCIES, "write/entry_dependencies");
statement!(PUT_ENTRIES, "put/entries");
statement!(PUT_HOST_FILE, "put/host_file");
statement!(UNPLACED_FIRST, "page/unplaced_first");
statement!(UNPLACED_NEXT, "page/unplaced_next");
statement!(PLACE_ENTRY, "write/place_entry");
statement!(PLACEMENT_DEPENDENCIES, "queries/placement_dependencies");
statement!(BIND_HOST_FILES, "queries/bind_host_files");
statement!(PLACE_CHILDREN, "queries/place_children");
statement!(DIRECTORIES, "read/directories");
statement!(DIRECTORY_SIZES, "read/directory_sizes");
statement!(DIRECTORY_PATH, "read/directory_path");
statement!(JOBS, "read/jobs");
statement!(JOB_SIZES, "read/job_sizes");
statement!(JOB, "read/job");
statement!(COMPLETE_FILE, "write/complete_file");
statement!(FILE_ROOTS, "read/file_roots");
statement!(ENTRIES_FIRST, "page/entries_first");
statement!(ENTRIES_NEXT, "page/entries_next");
statement!(BINDINGS_FIRST, "page/bindings_first");
statement!(BINDINGS_NEXT, "page/bindings_next");
statement!(SET_DIRECTORY_ROOT, "write/set_directory_root");
statement!(DISCARD_ENTRIES, "discard/entries");
statement!(DISCARD_HOST_FILE, "discard/host_file");
statement!(DISCARD_ENTRY_KEYS, "discard/entry_keys");
statement!(DISCARD_HOST_FILE_KEYS, "discard/host_file_keys");
statement!(DISCARD_ENTRY_TAIL, "discard/entry_tail");
statement!(DISCARD_HOST_FILE_TAIL, "discard/host_file_tail");
statement!(ABANDONED, "operation/abandoned");
statement!(RELEASE_ABANDONED, "operation/release_abandoned");
