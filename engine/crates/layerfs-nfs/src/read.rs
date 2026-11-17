//! What one kernel read answers this export, and what a failed one means.
//!
//! A read is served by one owner job (see [`crate::kernel`]). Its answer is
//! the entry it names and, for a LOOKUP, the directory it was named in, which
//! that job had to read anyway. A refusal keeps the directory row its own job
//! read, when it read one, so a reply's `dir_attributes` are current facts and
//! never this export's last observation: nothing ages such an observation, so
//! a client that loaded it would restart its own `actimeo` on facts that do
//! not change until the export happens to observe the directory again.
use crate::attributes::Status;
use layerfs_mount_ports::{operations::ReadFailure, ports::ServiceError};
use layerfs_overlay::OverlayError;
use layerfs_workspace::ViewStat;

/// One kernel read's answer, as this export uses it.
pub(crate) struct Answer {
    pub stat: ViewStat,
    /// The directory a LOOKUP named the inode in: the same visit read it, so
    /// the reply's directory attributes cost no second owner job.
    pub parent: Option<ViewStat>,
    pub file: Option<u64>,
    pub directory: Option<u64>,
}

/// A read this export refused: the status its reply answers, and the fresh
/// directory facts the refusing job read, when it read one. A LOOKUP that
/// binds no name read its parent's row in the same job, so the failure's
/// `dir_attributes` are those facts and never a last observation that no
/// time bound ages (see [`crate::kernel::Kernel::lookup`]).
pub struct Refused {
    pub status: Status,
    pub directory: Option<ViewStat>,
}
impl Refused {
    /// A refusal no visit decided: no job read a directory row for it.
    pub(crate) const fn plain(status: Status) -> Self {
        Self {
            status,
            directory: None,
        }
    }
}

fn stale(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if matches!(
            error.downcast_ref::<OverlayError>(),
            Some(OverlayError::Stale)
        ) {
            return true;
        }
        current = error.source();
    }
    false
}

/// Whether a failed step's cause chain holds an engine `Stale`.
pub(crate) fn failure_is_stale(error: &ServiceError) -> bool {
    stale(error.as_ref())
}

/// A failed port step: an engine `Stale` is the client's stale handle; any
/// other failure is an I/O error of this request alone.
pub fn failure(error: ServiceError) -> Status {
    if stale(error.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}

pub(crate) fn read_failure(failure: ReadFailure) -> Status {
    if stale(failure.reason.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}
