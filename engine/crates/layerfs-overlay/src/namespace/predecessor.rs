//! The file a rename put another in place of (#352 F-063).
//!
//! git, editors and package managers replace a file by writing a new one and
//! renaming it over the old: the result is a new inode, so Commit finds no
//! base file of its own to offer its chunks as delta bases. The rename records
//! the replaced serial here instead, for that construction. It is advisory: it
//! offers a delta base, says nothing about bytes, and a predecessor that is
//! gone by Commit offers nothing.
//!
//! When the replaced file is itself fresh (created since the installed base)
//! and a rename put it in place, its own predecessor is inherited, so a chain
//! of replacements between Commits ends at the file the base holds. A
//! replaced file the base holds is the predecessor itself, whatever row it
//! kept from before the install. The replaced file's row is removed either
//! way: one row per file a rename put in place.
use crate::{db::integer, CapturedReader, Overlay, OverlayResult, StatementKind};

/// A file never stands as its own predecessor: a hard-linked file renamed
/// back over the one that replaced it records nothing.
const INHERIT: &str = "INSERT OR REPLACE INTO fs_predecessor(inode,predecessor)
    SELECT ?1,p FROM (SELECT coalesce((SELECT predecessor FROM fs_predecessor
    WHERE inode=?2),?2) AS p) WHERE p<>?1";
const RECORD: &str = "INSERT OR REPLACE INTO fs_predecessor(inode,predecessor) VALUES(?1,?2)";
const RELEASE: &str = "DELETE FROM fs_predecessor WHERE inode=?1";
const READ: &str = "SELECT predecessor FROM fs_predecessor WHERE inode=?1";

impl Overlay {
    /// Inside a rename's job: `moved` now stands where `replaced` stood;
    /// `fresh` says the replaced file was created since the installed base.
    pub(crate) fn record_predecessor(
        &self,
        moved: u64,
        replaced: u64,
        fresh: bool,
    ) -> OverlayResult<()> {
        let (moved, replaced) = (integer(moved)?, integer(replaced)?);
        let statement = if fresh { INHERIT } else { RECORD };
        self.execute(StatementKind::Inode, statement, &[&moved, &replaced], 16)?;
        self.execute(StatementKind::Inode, RELEASE, &[&replaced], 8)?;
        Ok(())
    }

    /// The recorded predecessor of each serial, at most one captured page of
    /// serials per call. A serial no rename put in place has none.
    pub fn captured_predecessors(
        &self,
        reader: CapturedReader,
        serials: &[u64],
    ) -> OverlayResult<Vec<Option<u64>>> {
        self.check_captured_reader(reader)?;
        if serials.len() > crate::PAGE_ROWS {
            return Err(crate::OverlayError::Invalid("captured predecessor window"));
        }
        serials
            .iter()
            .map(|serial| {
                let rows = self.query(
                    StatementKind::Capture,
                    READ,
                    &[&integer(*serial)?],
                    8,
                    |row| row.get::<_, i64>(0),
                )?;
                Ok(rows
                    .into_iter()
                    .next()
                    .and_then(|predecessor| u64::try_from(predecessor).ok()))
            })
            .collect()
    }
}
