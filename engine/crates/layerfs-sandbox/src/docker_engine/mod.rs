//! The Engine HTTP client: one-attempt exchanges over the Engine's Unix
//! socket (`http`, `socket`), exact HTTP framing and header grammar (`body`),
//! the standard multiplexed stream header (`mux`) and API version
//! negotiation (`version`). The rest of the Docker backend reads its
//! `Socket`, `Body` and `mux::header` directly.
pub(crate) mod body;
pub(crate) mod http;
pub(crate) mod mux;
pub(crate) mod socket;
mod version;
pub use http::Docker;
