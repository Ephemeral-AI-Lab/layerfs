//! One native mutation round: the ordinary namespace job, published together
//! with the kernel custody that its own reply hands over.
use crate::job::Decided;
use crate::{JobOutcome, MutationPlan, NamespaceJob, Operation, WorkspaceResult};
use layerfs_overlay::{BaseSource, NativeEffect, NativeMount, OpenFile, Overlay};

/// The plan's deciding job bound to one native connection. It is data: the
/// owner evaluates it over current rows and publishes at most once.
#[derive(Clone, Debug)]
pub struct NativeMutationJob {
    mount: NativeMount,
    job: NamespaceJob,
    /// A created regular file is also replied with an open descriptor of
    /// this access mode (CREATE); None is an entry-only reply.
    open: Option<bool>,
}
/// The original owner result. A descriptor is present only for an applied
/// create-and-open; it is usable only when `result` is the applied outcome.
#[derive(Debug)]
pub struct NativeMutationOutcome {
    pub result: WorkspaceResult<JobOutcome>,
    pub file: Option<OpenFile>,
}
impl MutationPlan {
    /// The next deciding job for a native request, while one is due.
    pub fn native_job(&self, mount: NativeMount, open: Option<bool>) -> Option<NativeMutationJob> {
        self.job().map(|job| NativeMutationJob {
            mount,
            job: job.clone(),
            open,
        })
    }
}
impl NativeMutationJob {
    pub const fn source(&self) -> BaseSource {
        self.job.source()
    }
    pub fn charge(&self) -> usize {
        self.job.charge()
    }
    /// The kernel custody an applied reply creates. Every entry-bearing
    /// operation takes exactly one lookup reference on the bound inode.
    fn effect(&self) -> NativeEffect {
        let entry = |serial, parent, directory| NativeEffect::Entry {
            serial,
            parent,
            directory,
        };
        match (&self.job.operation, self.job.serial) {
            (Operation::Create { parent, .. }, Some(serial)) => match self.open {
                Some(writable) => NativeEffect::Open {
                    serial,
                    parent: *parent,
                    writable,
                },
                None => entry(serial, *parent, false),
            },
            (Operation::Mkdir { parent, .. }, Some(serial)) => entry(serial, *parent, true),
            (Operation::Symlink { parent, .. }, Some(serial)) => entry(serial, *parent, false),
            (Operation::Link { serial, parent, .. }, _) => entry(*serial, *parent, false),
            _ => NativeEffect::None,
        }
    }
    /// One owner job. Evaluation reads current rows; the single publication
    /// and its lookup/open acquisition are one transaction, never replayed.
    pub fn perform(&self, db: &Overlay) -> NativeMutationOutcome {
        let mut file = None;
        let result = self
            .job
            .decide(db, Some(self.mount))
            .and_then(|decided| match decided {
                Decided::Final(outcome) => Ok(outcome),
                Decided::Publish { changes, inode } => {
                    let applied =
                        db.apply_native(self.mount, self.job.source(), &changes, self.effect())?;
                    file = applied.file;
                    Ok(JobOutcome::Applied {
                        publication: applied.publication,
                        inode,
                    })
                }
            });
        NativeMutationOutcome { result, file }
    }
}
