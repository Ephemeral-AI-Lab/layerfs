#[cfg(any())]
mod legacy_tests {
    use super::*;

    include!("tests/setup_and_splice.rs");
    include!("tests/spool_lifecycle.rs");
    include!("tests/namespace_limits.rs");
    include!("tests/checkpoint_reopen.rs");
    include!("tests/resource_limits.rs");
    include!("tests/capacity.rs");
    include!("tests/rollback.rs");
    include!("tests/recovery.rs");
    include!("tests/directory_cursor.rs");
}
