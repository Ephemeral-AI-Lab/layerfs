//! Small named choices that cross the wire or records.
/// The kernel filesystem route a daemon serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KernelRoute {
    /// A kernel FUSE mount.
    Fuse,
    /// A loopback NFS export.
    Nfs,
}
impl KernelRoute {
    /// Its stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fuse => "fuse",
            Self::Nfs => "nfs",
        }
    }
    /// The route named `text`, or `None`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fuse" => Some(Self::Fuse),
            "nfs" => Some(Self::Nfs),
            _ => None,
        }
    }
}
/// A Store's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoreKind {
    /// A SQLite Store file.
    Sqlite,
    /// A PostgreSQL Store (#506).
    Postgresql,
}
impl StoreKind {
    /// Its stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgresql => "postgresql",
        }
    }
    /// The kind named `text`, or `None`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "sqlite" => Some(Self::Sqlite),
            "postgresql" => Some(Self::Postgresql),
            _ => None,
        }
    }
}
