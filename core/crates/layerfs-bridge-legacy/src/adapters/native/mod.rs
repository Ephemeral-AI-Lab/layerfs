pub mod client;
pub mod connection;
pub mod payload;
pub mod pipe;
pub mod protocol;
pub mod server;
pub use connection::listen;

/// A fully decoded read-only absence can leave both sides synchronized.
pub fn reusable_inspect_refusal(
    request: &crate::contract::Request,
    error: &crate::contract::Failure,
) -> bool {
    use crate::contract::{Code, Operation};
    matches!(request.operation, Operation::Inspect { .. })
        && matches!(error.code, Code::PathNotFound | Code::NotFound)
        && !error.unknown
        && error.cleanup.is_none()
        && error.history.is_none()
}
