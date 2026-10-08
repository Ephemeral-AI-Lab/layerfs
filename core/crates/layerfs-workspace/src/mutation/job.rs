//! One typed bounded owner job: evaluate over current rows, then publish once.
use crate::attributes;
use crate::create::{self, Fresh};
use crate::eval::Eval;
use crate::remove;
use crate::rename::{self, Move};
use crate::write;
use crate::{BaseFacts, Need, Operation, Refusal, Time, WorkspaceError, WorkspaceResult};
use layerfs_content::ContentError;
use layerfs_overlay::{BaseSource, Changes, Inode, InodeKind, NativeMount, Overlay, Publication};

/// A complete operation input for the SQL owner. It carries data only: the
/// owner performs no provider, content or kernel work while evaluating it.
#[derive(Clone, Debug)]
pub struct NamespaceJob {
    pub(crate) source: BaseSource,
    pub(crate) root: u64,
    pub(crate) operation: Operation,
    pub(crate) now: Time,
    pub(crate) serial: Option<u64>,
    pub(crate) facts: BaseFacts,
}
/// What one owner round decided. Only `Applied` changed state.
#[derive(Clone, Debug)]
pub enum JobOutcome {
    Applied {
        publication: Publication,
        inode: Option<Inode>,
    },
    Unchanged {
        inode: Option<Inode>,
    },
    /// Immutable base facts to fetch outside the owner before the next round.
    Needs(Vec<Need>),
    Refused(Refusal),
}
type Planned = Option<(Option<Changes>, Option<Inode>)>;
/// One evaluated round, before its single publication attempt.
pub(crate) enum Decided {
    Final(JobOutcome),
    Publish {
        changes: Changes,
        inode: Option<Inode>,
    },
}
impl NamespaceJob {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    /// Heap bytes of this input beyond its own size, for admission accounting.
    pub fn charge(&self) -> usize {
        let names = match &self.operation {
            Operation::Rename {
                destination_path, ..
            } => destination_path.as_ref().map_or(0, |path| {
                path.capacity()
                    * (std::mem::size_of::<layerfs_content::filesystem::PathName>() + 255)
            }),
            _ => 0,
        };
        let data = match &self.operation {
            Operation::Write { data, .. }
            | Operation::WriteOpen { data, .. }
            | Operation::StoreOpen { data, .. } => data.len(),
            _ => 0,
        };
        // Operation names and one symlink target are bounded by their grammar.
        self.facts.charge() + names + data + 2 * 255 + 4096
    }
    /// Runs inside one owner job. A refusal or a need publishes nothing; the
    /// single `apply` is the operation's only attempt and is never replayed.
    pub fn perform(&self, db: &Overlay) -> WorkspaceResult<JobOutcome> {
        match self.decide(db, None)? {
            Decided::Final(outcome) => Ok(outcome),
            Decided::Publish { changes, inode } => Ok(JobOutcome::Applied {
                publication: db.apply(self.source, &changes)?,
                inode,
            }),
        }
    }
    /// The evaluation half of one owner round: current rows over supplied
    /// facts, ending in a final answer or the complete values to publish.
    /// Nothing is written here. `native` selects that connection's retained
    /// parent index as the cycle evidence of a directory move.
    pub(crate) fn decide(
        &self,
        db: &Overlay,
        native: Option<NativeMount>,
    ) -> WorkspaceResult<Decided> {
        if self.now.nanoseconds >= 1_000_000_000 {
            return Ok(Decided::Final(JobOutcome::Refused(Refusal::Invalid)));
        }
        let open = self.operation.file();
        if let Some(file) = open {
            if file.route() != self.source.route() {
                return Err(layerfs_overlay::OverlayError::Stale.into());
            }
            db.check_file(file, true)?;
        }
        let mut eval = Eval {
            rows: db.source_rows(self.source)?,
            facts: &self.facts,
            root: self.root,
            open_serial: open.map(|file| file.serial()),
            native: native.map(|mount| (db, mount)),
            needs: Vec::new(),
        };
        let planned = match self.plan(&mut eval) {
            Ok(planned) => planned,
            Err(WorkspaceError::Refused(refusal)) => {
                return Ok(Decided::Final(JobOutcome::Refused(refusal)))
            }
            Err(error) => return Err(error),
        };
        let final_ = |outcome| Ok(Decided::Final(outcome));
        match planned {
            None if eval.needs.is_empty() => {
                Err(ContentError::InvalidRecord("undecided namespace job").into())
            }
            None => final_(JobOutcome::Needs(eval.needs)),
            Some((None, inode)) => final_(JobOutcome::Unchanged { inode }),
            Some((Some(mut changes), inode)) => {
                for change in &mut changes.directory_entries {
                    let name = layerfs_content::filesystem::PathName::from_bytes(&change.name)?;
                    let Some(parent) = eval.inode(change.parent)? else {
                        return final_(JobOutcome::Needs(eval.needs));
                    };
                    let layers = eval.layers(change.parent, &name)?;
                    let Some(inherited) =
                        eval.inherited(change.parent, parent.as_ref(), &name, layers)
                    else {
                        return final_(JobOutcome::Needs(eval.needs));
                    };
                    change.binding = match change.binding {
                        layerfs_overlay::Binding::Bound { serial, .. } => {
                            layerfs_overlay::Binding::Bound { serial, inherited }
                        }
                        layerfs_overlay::Binding::Removed { .. } => {
                            layerfs_overlay::Binding::Removed { inherited }
                        }
                    };
                }
                changes.open = open;
                if matches!(
                    self.operation,
                    Operation::Unlink { .. } | Operation::Rmdir { .. } | Operation::Rename { .. }
                ) {
                    changes.detached = changes
                        .inodes
                        .iter()
                        .find(|i| i.nlink == 0 && i.serial != self.root)
                        .map(|i| i.serial);
                }
                Ok(Decided::Publish { changes, inode })
            }
        }
    }
    fn fresh<'a>(
        &self,
        kind: InodeKind,
        mode: u32,
        target: Option<&'a layerfs_content::filesystem::SymlinkTarget>,
    ) -> WorkspaceResult<Fresh<'a>> {
        Ok(Fresh {
            serial: self
                .serial
                .ok_or(ContentError::InvalidRecord("missing reserved inode serial"))?,
            kind,
            mode,
            target,
        })
    }
    fn plan(&self, eval: &mut Eval<'_>) -> WorkspaceResult<Planned> {
        let now = self.now;
        let created = |planned: Option<(Changes, Inode)>| {
            planned.map(|(changes, inode)| (Some(changes), Some(inode)))
        };
        Ok(match &self.operation {
            Operation::Create { parent, name, mode } => created(create::create(
                eval,
                *parent,
                name,
                self.fresh(InodeKind::File, *mode, None)?,
                now,
            )?),
            Operation::Mkdir { parent, name, mode } => created(create::create(
                eval,
                *parent,
                name,
                self.fresh(InodeKind::Directory, *mode, None)?,
                now,
            )?),
            Operation::Symlink {
                parent,
                name,
                target,
            } => created(create::create(
                eval,
                *parent,
                name,
                self.fresh(InodeKind::Symlink, 0o777, Some(target))?,
                now,
            )?),
            Operation::Link {
                serial,
                parent,
                name,
            } => created(create::link(eval, *serial, *parent, name, now)?),
            Operation::Unlink { parent, name } => {
                remove::remove(eval, *parent, name, false, now)?.map(|c| (Some(c), None))
            }
            Operation::Rmdir { parent, name } => {
                remove::remove(eval, *parent, name, true, now)?.map(|c| (Some(c), None))
            }
            Operation::Rename {
                parent,
                name,
                new_parent,
                new_name,
                replace,
                destination_path,
            } => rename::rename(
                eval,
                Move {
                    parent: *parent,
                    name,
                    new_parent: *new_parent,
                    new_name,
                    replace: *replace,
                    path: destination_path.as_deref(),
                },
                now,
            )?
            .map(|changes| (changes, None)),
            Operation::WriteOpen {
                file,
                position,
                data,
            } => write::write(eval, file.serial(), *position, data, now, false)?
                .map(|(changes, inode)| (changes, Some(inode))),
            Operation::StoreOpen { file, offset, data } => write::write(
                eval,
                file.serial(),
                crate::Position::At(*offset),
                data,
                now,
                true,
            )?
            .map(|(changes, inode)| (changes, Some(inode))),
            Operation::SetOpenAttributes {
                file,
                mode,
                mtime,
                size,
            } => attributes::set(eval, file.serial(), *mode, *mtime, *size, now)?
                .map(|(changes, inode)| (changes, Some(inode))),
            Operation::SetAttributes {
                serial,
                mode,
                mtime,
                size,
            } => attributes::set(eval, *serial, *mode, *mtime, *size, now)?
                .map(|(changes, inode)| (changes, Some(inode))),
            Operation::Write {
                serial,
                position,
                data,
            } => write::write(eval, *serial, *position, data, now, false)?
                .map(|(changes, inode)| (changes, Some(inode))),
        })
    }
}
