//! The options `machine create` takes, as each platform's profile declares
//! them (#535 §9a).

/// What an option's value is, as the call table reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptKind {
    /// A whole number.
    Int,
    /// Bytes, or a number with a binary unit (`8G`).
    Size,
    /// Free text: a path, an image reference, a user.
    Text,
}

/// One option a platform takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptSpec {
    pub name: &'static str,
    pub kind: OptKind,
}

impl OptKind {
    /// Its stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Size => "size",
            Self::Text => "text",
        }
    }
}

pub(crate) const fn opt(name: &'static str, kind: OptKind) -> OptSpec {
    OptSpec { name, kind }
}

/// The options every created VM takes: its size and its own system image.
pub(crate) const CPUS: OptSpec = opt("cpus", OptKind::Int);
pub(crate) const MEMORY: OptSpec = opt("memory", OptKind::Size);
pub(crate) const DISK: OptSpec = opt("disk", OptKind::Size);
pub(crate) const IMAGE: OptSpec = opt("image", OptKind::Text);

/// The machine OS image; `image` remains the Sandbox image.
pub(crate) const OS_IMAGE: OptSpec = opt("os_image", OptKind::Text);
