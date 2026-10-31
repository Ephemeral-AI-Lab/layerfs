//! Linux namespace Sandboxes slot; implemented by the namespaces backend.
mod provider;
mod sandbox;

pub use provider::NamespaceProvider;
pub use sandbox::Sandbox;
