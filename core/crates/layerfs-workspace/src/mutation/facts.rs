//! Immutable base facts for one owned source, fetched outside the SQL owner.
use crate::{SourceView, WorkspaceError, WorkspaceResult};
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::InodeKind as BaseKind;
use layerfs_content::ContentError;
use layerfs_overlay::{Inode, InodeKind};

/// Facts retained for one job, not a Workspace limit. The set is a cache of
/// immutable data, so exceeding the window restarts it instead of refusing.
const FACT_WINDOW: usize = 768;

/// One base fact an owner evaluation found it could not decide without.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Need {
    Inode(u64),
    Name(u64, PathName),
}
/// Answers about the source's immutable base root. They stay true for as long
/// as the source is owned, because install is fenced behind its release.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BaseFacts {
    inodes: Vec<(u64, Option<Inode>)>,
    names: Vec<(u64, PathName, Option<u64>)>,
}
impl BaseFacts {
    pub(crate) fn inode(&self, serial: u64) -> Option<&Option<Inode>> {
        self.inodes
            .iter()
            .find(|(key, _)| *key == serial)
            .map(|(_, value)| value)
    }
    pub(crate) fn name(&self, parent: u64, name: &PathName) -> Option<Option<u64>> {
        self.names
            .iter()
            .find(|(key, bound, _)| *key == parent && bound == name)
            .map(|(_, _, value)| *value)
    }
    /// Heap bytes retained by this job input, for owner admission accounting.
    pub fn charge(&self) -> usize {
        self.inodes.capacity() * std::mem::size_of::<(u64, Option<Inode>)>()
            + self.names.capacity() * (std::mem::size_of::<(u64, PathName, Option<u64>)>() + 255)
    }
    fn make_room(&mut self) {
        if self.inodes.len() + self.names.len() >= FACT_WINDOW {
            self.inodes.clear();
            self.names.clear();
        }
    }
}
impl SourceView {
    /// The base inode as a complete local value, or None when the base has no
    /// such serial. Directory entry counts come from the directory root page.
    fn base_inode(&self, serial: u64) -> WorkspaceResult<Option<Inode>> {
        let stat = match self.base.stat(serial) {
            Ok(stat) => stat,
            Err(WorkspaceError::Content(ContentError::PathNotFound)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let (kind, entries) = match stat.value.kind {
            BaseKind::RegularFile => (InodeKind::File, 0),
            BaseKind::Symlink => (InodeKind::Symlink, 0),
            BaseKind::Directory => (InodeKind::Directory, self.base.entries(stat.value)?),
        };
        Ok(Some(Inode {
            serial,
            kind,
            mode: u16::try_from(stat.metadata.mode)
                .map_err(|_| ContentError::InvalidRecord("portable metadata"))?,
            mtime_seconds: stat.metadata.mtime_seconds,
            mtime_nanoseconds: stat.metadata.mtime_nanoseconds,
            nlink: stat.value.namespace_ref_count,
            size: stat.logical_len,
            inherited_cutoff: stat.logical_len,
            born: 0,
            entries,
        }))
    }
    fn base_child(&self, parent: u64, name: &PathName) -> WorkspaceResult<Option<u64>> {
        match self.base.child(parent, name) {
            Ok(child) => Ok(Some(child.serial)),
            // An absent or non-directory base parent binds no such name.
            Err(ContentError::PathNotFound | ContentError::WrongLogicalRole) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    /// Provider/content demand for the facts one evaluation asked for. A bound
    /// name also supplies its inode, and the rest of a declared destination
    /// path, so an undisturbed operation settles in two owner jobs.
    pub(crate) fn supply(
        &self,
        facts: &mut BaseFacts,
        needs: Vec<Need>,
        path: Option<&[PathName]>,
    ) -> WorkspaceResult<()> {
        for need in needs {
            facts.make_room();
            match need {
                Need::Inode(serial) => {
                    if facts.inode(serial).is_none() {
                        facts.inodes.push((serial, self.base_inode(serial)?));
                    }
                }
                Need::Name(parent, name) => {
                    if facts.name(parent, &name).is_some() {
                        continue;
                    }
                    let child = self.base_child(parent, &name)?;
                    let rest = path.and_then(|path| {
                        path.iter()
                            .position(|step| *step == name)
                            .map(|at| &path[at + 1..])
                    });
                    facts.names.push((parent, name, child));
                    let Some(mut current) = child else {
                        continue;
                    };
                    if facts.inode(current).is_none() {
                        facts.inodes.push((current, self.base_inode(current)?));
                    }
                    for step in rest.unwrap_or(&[]) {
                        if facts.name(current, step).is_some() {
                            break;
                        }
                        facts.make_room();
                        let next = self.base_child(current, step)?;
                        facts.names.push((current, step.clone(), next));
                        match next {
                            Some(next) => current = next,
                            None => break,
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
