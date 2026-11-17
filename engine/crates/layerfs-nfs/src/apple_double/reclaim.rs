//! Reclamation of the companion slots no name reaches, at the index bound.
use super::Orphan;
use crate::{apple_double_spill::unlink_paths, attributes::Status, kernel::Kernel};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::KernelReadOperation;

impl Kernel {
    /// The companion slots no name reaches any more: one whose main inode
    /// holds no name in the kernel table, and one whose main file the export
    /// never observed and whose name its name index does not hold. Taken under
    /// the kernel table lock and, inside it, the companion lock, so nothing
    /// here copies the namespace; the caller drops them by serial under the
    /// companion lock again.
    pub(super) fn orphans(&self) -> Vec<Orphan> {
        let table = self.table();
        let companions = self.companions();
        companions
            .slots
            .iter()
            .filter_map(|serial| {
                let file = companions.files.get(serial)?;
                if let Some(inode) = file.inode {
                    let live =
                        table.names.contains_key(&inode) || table.aliases.contains_key(&inode);
                    return (!live).then_some(Orphan {
                        serial: *serial,
                        inode: Some(inode),
                        main: None,
                    });
                }
                // A name the index does hold reaches the companion; one it
                // does not hold only makes it a candidate, since that index is
                // lossy. Any of the names it holds reaches it, so one whose
                // main file the export knows keeps it, and the first name whose
                // main it does not know is the one the lookup asks about.
                let mut candidate = None;
                for named in std::iter::once(&file.named).chain(&file.also) {
                    let Some(main) = named.1.strip_prefix(b"._") else {
                        continue;
                    };
                    let main = (named.0, main.to_vec());
                    if table.by_name.contains_key(&main) {
                        return None;
                    }
                    candidate.get_or_insert(main);
                }
                candidate.map(|main| Orphan {
                    serial: *serial,
                    inode: None,
                    main: Some(main),
                })
            })
            .collect()
    }
    /// Drops the companion slots no name reaches any more and answers how many
    /// it dropped. Takes the kernel table lock and, inside it, the companion
    /// lock; the files it retires are unlinked afterwards, outside both. Never
    /// call it under the companion lock.
    ///
    /// A companion whose main inode the export does not know is not dropped on
    /// the name index's word: that index is lossy (its own bound, a name the
    /// export never looked up, a rename on the client side), so each one costs
    /// one read-only lookup of its main file in the engine, run outside both
    /// locks. A main file that is there binds the companion back to its inode,
    /// which keeps its attributes; only a lookup that answers "no such file",
    /// or whose parent directory the engine no longer has (so the file cannot
    /// be there either), makes it an orphan, and one that cannot answer leaves
    /// it as it is.
    ///
    /// A lookup that finds the main file took one engine reference on it, and
    /// the export keeps exactly one per serial: the visit is the entry's, so it
    /// is registered as `Kernel::lookup` registers one (`entry_taken` remembers
    /// the name and forgets a second reference). Without that the name reaches
    /// the companion nowhere, and the next reclamation would read the bound one
    /// as an inode holding no name and drop it.
    pub(super) fn reclaim_orphans(&self) -> usize {
        let (bound, dead) = self.orphans().into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut bound, mut dead), orphan| {
                let Some((parent, main)) = orphan.main else {
                    dead.push((orphan.serial, orphan.inode));
                    return (bound, dead);
                };
                // A name the engine cannot even take, or a lookup it cannot
                // answer, leaves the companion as it is.
                if let Ok(name) = PathName::from_bytes(&main) {
                    let lookup = KernelReadOperation::Lookup { parent, name };
                    match self.observe(parent, None, lookup).map_err(|r| r.status) {
                        Ok(answer) => {
                            self.entry_taken(&answer.stat, parent, &main);
                            bound.push((orphan.serial, answer.stat.serial));
                        }
                        // A parent directory the engine no longer has cannot
                        // hold the main file: the companion is an orphan as
                        // much as one whose file answers "no such file".
                        // A parent directory the engine no longer has cannot
                        // hold the main file: the companion is an orphan as
                        // much as one whose file answers "no such file".
                        Err(Status::NoEnt | Status::Stale | Status::NotDir) => {
                            dead.push((orphan.serial, orphan.inode))
                        }
                        Err(_) => {}
                    }
                }
                (bound, dead)
            },
        );
        let (reclaimed, spilled) = {
            let mut companions = self.companions();
            let mut spilled = Vec::new();
            let mut reclaimed = 0;
            for (serial, main) in bound {
                let Some(mut file) = companions.detach(serial) else {
                    continue;
                };
                if file.inode.is_some() || !file.slotted() {
                    companions.attach(serial, file);
                    continue;
                }
                // Bound back to the inode the engine found: it keeps the slot it
                // took, and from here it follows that inode's names.
                file.inode = Some(main);
                companions.index(serial, &file);
                companions.attach(serial, file);
            }
            for (serial, inode) in dead {
                let still_orphan = companions
                    .files
                    .get(&serial)
                    .is_some_and(|file| file.inode == inode && file.slotted());
                if still_orphan {
                    reclaimed += 1;
                    spilled.extend(companions.take_serial(serial));
                }
            }
            (reclaimed, spilled)
        };
        unlink_paths(spilled);
        reclaimed
    }
}
