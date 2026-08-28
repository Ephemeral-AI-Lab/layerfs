pub(super) fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

pub(super) fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
