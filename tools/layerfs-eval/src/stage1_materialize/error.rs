pub(in crate::stage1_materialize) fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

pub(in crate::stage1_materialize) fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
