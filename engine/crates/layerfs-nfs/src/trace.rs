//! An optional trace of the AppleDouble names one client asks about.
//!
//! On a real client the export sees only the requests that arrive: whether a
//! companion a client cannot read was never asked for, was asked for under a
//! name the export cannot resolve, or was answered and then dropped by the
//! client, is invisible from the outside. This module prints one line for each
//! request that names an AppleDouble (`._`) companion - the `._` names
//! themselves and the main file names of the inodes that have one - with the
//! directory it named them in, the name, the companion the export resolved,
//! which part of its index found it (the companion's own name key or the main
//! inode) or that neither did, and the answer the client gets. A run of a real
//! client with this on is the only way to read its sequence; see
//! `docs/architecture/engine/kernel-routes/nfs.md`.
//!
//! `LAYERFS_NFS_TRACE=1` turns it on; anything else, and no variable at all,
//! leaves it off. It is read once per process and changes no state, so when it
//! is off every hook costs one `OnceLock` load. Lines go to the daemon's
//! standard error, like the release line, and are written under whatever lock
//! the request already holds.
use crate::attributes::Status;
use std::sync::OnceLock;

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether this process traces AppleDouble names.
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("LAYERFS_NFS_TRACE").as_deref() == Ok("1"))
}

/// Which part of the export's index found a companion for a `._` name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// The companion's own name key (`Companions::names`).
    Name,
    /// The main inode the name's main file has (`Companions::inodes`).
    Inode,
    /// Neither: no companion answers that name.
    None,
}
impl Route {
    const fn named(self) -> &'static str {
        match self {
            Route::Name => "name",
            Route::Inode => "inode",
            Route::None => "none",
        }
    }
}

/// How a status is traced: the RFC's own name for it (`Ok`, `NoEnt`, `Exist`,
/// ...), for the answers a companion request gives.
pub fn status(status: Status) -> &'static str {
    match status {
        Status::Ok => "Ok",
        Status::NoEnt => "NoEnt",
        Status::Exist => "Exist",
        Status::Access => "Access",
        Status::Stale => "Stale",
        Status::NotDir => "NotDir",
        Status::NoSpc => "NoSpc",
        Status::Inval => "Inval",
        Status::Perm => "Perm",
        Status::Io => "Io",
        Status::NotSync => "NotSync",
        Status::FBig => "FBig",
        _ => "other",
    }
}

/// One request about a `._` name: `main` is the main inode the export's name
/// index gave it (`None` when it does not know the main file), `found` the
/// companion it resolved, and `route` which key found it.
pub fn companion(
    op: &str,
    directory: u64,
    name: &[u8],
    main: Option<u64>,
    route: Route,
    found: Option<u64>,
    status: &str,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "layerfs-nfs trace: op={op} dir={directory} name={:?} main={main:?} route={} companion={found:?} status={status}",
        String::from_utf8_lossy(name),
        route.named()
    );
}

/// One request about a companion by handle only, where the client named no
/// name: a SETATTR of a `._` file it already holds, which can truncate the
/// body shared by every name of the inode.
pub fn handle(op: &str, companion: u64, detail: &str) {
    if !enabled() {
        return;
    }
    eprintln!("layerfs-nfs trace: op={op} companion={companion} {detail}");
}

/// One request about a main file whose inode has a companion, which is where
/// the export learns (or forgets) the names a `._` name resolves through.
pub fn main_file(
    op: &str,
    directory: u64,
    name: &[u8],
    inode: Option<u64>,
    companion: Option<u64>,
    status: &str,
) {
    if !enabled() {
        return;
    }
    eprintln!(
        "layerfs-nfs trace: op={op} dir={directory} name={:?} inode={inode:?} companion={companion:?} status={status}",
        String::from_utf8_lossy(name)
    );
}
