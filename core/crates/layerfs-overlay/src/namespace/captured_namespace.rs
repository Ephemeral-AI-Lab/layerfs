//! Parent-local names, name points and symlink targets of one retained reader.
//! Every job reads its sealed generation or sealed range, never active rows.
use crate::{
    db::{integer, unsigned},
    inode::optional_serial,
    sql, CapturedReader, DirectoryEntry, InodeKind, Overlay, OverlayError, OverlayResult,
    StatementKind, CELL_BYTES,
};

impl Overlay {
    /// At most PAGE_ROWS sealed rows of exactly this parent, strictly after
    /// `after` in binary name order, whiteouts included; an empty reply ends
    /// the parent. The seek prefix is (namespace, sealed generation, parent),
    /// so no row of another parent or generation is visited.
    pub fn reader_parent_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<&[u8]>,
    ) -> OverlayResult<Vec<DirectoryEntry>> {
        self.check_captured_reader(reader)?;
        let after = after.unwrap_or(&[]);
        if after.len() > 255 {
            return Err(OverlayError::Invalid("captured name cursor"));
        }
        self.query(
            StatementKind::Capture,
            sql::SOURCE_NAMES,
            &[
                &reader.capture.route.ns,
                &reader.capture.generation.0,
                &integer(parent)?,
                &after,
            ],
            24 + after.len() as u64,
            |row| {
                Ok(DirectoryEntry {
                    inherited: row.get(3)?,
                    parent: unsigned(row, 0)?,
                    name: row.get(1)?,
                    serial: optional_serial(row.get(2)?, 2)?,
                })
            },
        )
    }
    /// The exact sealed row of one name; a whiteout is a row. None means the
    /// capture has no row for it: lower and active generations never answer.
    pub fn reader_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Option<DirectoryEntry>> {
        self.check_captured_reader(reader)?;
        if name.len() > 255 {
            return Err(OverlayError::Invalid("captured name"));
        }
        self.query(
            StatementKind::Capture,
            sql::CAPTURED_DIRECTORY_ENTRY,
            &[
                &reader.capture.route.ns,
                &integer(parent)?,
                &name,
                &reader.capture.generation.0,
            ],
            32 + name.len() as u64,
            |row| {
                Ok(DirectoryEntry {
                    inherited: row.get(1)?,
                    parent,
                    name: name.to_vec(),
                    serial: optional_serial(row.get(0)?, 0)?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    /// The exact target of a live symlink whose every byte is local to the
    /// reader's sealed range, composed by the same layered read as any captured
    /// window. Missing covers no local row, a removed inode, another kind and a
    /// target the immutable base still supplies; nothing falls back to the
    /// base or to active state.
    pub fn reader_symlink(&self, reader: CapturedReader, serial: u64) -> OverlayResult<Vec<u8>> {
        let read = self
            .read_captured(reader, serial, 0, CELL_BYTES as u32)?
            .ok_or(OverlayError::Missing)?;
        if read.kind != InodeKind::Symlink || read.span.is_some() {
            return Err(OverlayError::Missing);
        }
        // A target is one cell; a longer row is not a representable symlink.
        if read.data.len() as u64 != read.size {
            return Err(OverlayError::Invalid("captured symlink window"));
        }
        Ok(read.data)
    }
    /// Plans of the two reader-bound name statements for this exact reader,
    /// parent and name. Runtime work is observed separately in the Capture family.
    pub fn explain_reader_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Vec<String>> {
        self.check_captured_reader(reader)?;
        if name.len() > 255 {
            return Err(OverlayError::Invalid("captured name"));
        }
        let (ns, generation) = (reader.capture.route.ns, reader.capture.generation.0);
        let parent = integer(parent)?;
        let mut plans = Vec::new();
        for (label, statement, params) in [
            (
                "parent-names",
                sql::SOURCE_NAMES,
                [&ns as &dyn rusqlite::ToSql, &generation, &parent, &name],
            ),
            (
                "name-point",
                sql::CAPTURED_DIRECTORY_ENTRY,
                [&ns, &parent, &name, &generation],
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                &params,
                32 + name.len() as u64,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        Ok(plans)
    }
}
