//! §7 step 6, the part of the verification that goes by pack: the staged
//! file's `pack` rows, the locators that name them, and the packs the build
//! wrote again.
//!
//! Every object's locator names a pack, and the staged file must hold that
//! pack. Asked of SQLite (`foreign_key_check`), that is one lookup in `pack`
//! per object in object identity order: the lookups land on the pack table's
//! pages at random, so a Store whose pack rows' pages exceed the reader's
//! cache pays one page read per object (measured on a Store of 3.0 million
//! packs: 2.4 million page reads, 4.9 s). A cache sized to the Store is not
//! an answer: a compaction's memory is fixed.
//!
//! Here the check is a merge of two ordered streams instead. One plain scan
//! of `object_location`, in its own key order, counts the objects and puts
//! every locator into a sorter of bounded memory ([`crate::history::spill`],
//! [`HELD_BYTES`] held and the rest in an unnamed file beside the Store, 16
//! bytes an object); read back, the locators are in pack, group and record
//! order. The `pack` rows are then read in pack order, a range at a time,
//! and each row takes its locators off the stream: a locator whose pack the
//! file does not hold is the foreign-key failure. Both sides are read
//! forward, so a small cache is enough whatever the Store's size.
//!
//! The same pass checks the packs the build wrote again. Each one's stored
//! bytes are read whole and hashed: they must carry the SHA-256 and length
//! of their row, and both must be the ones the build computed. The pack's
//! own directories then state how many records each group holds (a
//! compressed group is decoded for its count), and the pack's locators,
//! which arrive in group and record order, must be exactly those records:
//! every `(group, record)` named once, none left unnamed and none beyond.
//! The pass holds one pack body and one pack's group counts at a time.

use super::repack::read;
use super::repack::Rewritten;
use super::types::db_error;
use crate::history::spill::Item;
use crate::history::spill::Merge;
use crate::history::spill::Sorter;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_storage::encoding::DecompressionWorkspace;
use layerfs_storage::pack::repack::record_counts;
use rusqlite::params;
use rusqlite::Connection;
use std::cmp::Ordering;
use std::path::Path;

/// Bytes of locators held in memory: 524,288 of them.
const HELD_BYTES: usize = 8 * 1024 * 1024;

/// A staged pack or locator that is not what the build wrote.
const STAGED_PACK: HistoryError = HistoryError::Integrity("staged Store repacked pack");
const STAGED_LOCATOR: HistoryError = HistoryError::Integrity("staged Store repacked locators");
/// A locator that names a pack the staged file does not hold.
const FOREIGN_KEY: HistoryError = HistoryError::Integrity("staged Store foreign key");

/// Where one staged object is: its pack, group and record.
#[derive(Clone, Copy)]
struct Locator {
    pack: i64,
    group: u32,
    record: u32,
}

impl Item for Locator {
    const BYTES: usize = 16;
    fn encode(&self, out: &mut [u8]) {
        out[..8].copy_from_slice(&self.pack.to_le_bytes());
        out[8..12].copy_from_slice(&self.group.to_le_bytes());
        out[12..16].copy_from_slice(&self.record.to_le_bytes());
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            pack: i64::from_le_bytes(bytes[..8].try_into().ok()?),
            group: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            record: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
        })
    }
}

fn by_record(left: &Locator, right: &Locator) -> Ordering {
    (left.pack, left.group, left.record).cmp(&(right.pack, right.group, right.record))
}

/// Every locator of the staged file, to be read back in pack order, and what
/// the one scan of `object_location` counted on the way.
pub struct StagedLocators {
    list: Sorter<Locator>,
    /// The rows of `object_location`.
    pub objects: u64,
    /// The sum of their canonical lengths.
    pub canonical: i64,
}

impl StagedLocators {
    /// Scans the staged file's `object_location` once. The sorter's file,
    /// when it needs one, is in `directory`.
    pub fn read(connection: &Connection, directory: &Path) -> HistoryResult<Self> {
        let mut read = Self {
            list: Sorter::new(directory, HELD_BYTES, by_record),
            objects: 0,
            canonical: 0,
        };
        let mut statement = connection
            .prepare(
                "SELECT pack_id,group_number,record_number,canonical_length FROM object_location",
            )
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        while let Some(row) = rows.next().map_err(db_error)? {
            let number = |column: usize| -> HistoryResult<u32> {
                u32::try_from(row.get::<_, i64>(column).map_err(db_error)?)
                    .map_err(|_| STAGED_LOCATOR)
            };
            read.list.push(Locator {
                pack: row.get(0).map_err(db_error)?,
                group: number(1)?,
                record: number(2)?,
            })?;
            read.objects += 1;
            read.canonical = read
                .canonical
                .saturating_add(row.get::<_, i64>(3).map_err(db_error)?);
        }
        Ok(read)
    }

    /// The most bytes the sorter's file has held.
    pub fn peak_file_bytes(&self) -> u64 {
        self.list.peak_file_bytes()
    }

    /// What that file holds for a staged file of `objects` objects: the
    /// sorter writes its locators out each time it holds [`HELD_BYTES`] of
    /// them, and keeps the rest. The report counts this before the
    /// verification runs.
    pub fn sort_file_bytes(objects: u64) -> u64 {
        let held = (HELD_BYTES / Locator::BYTES) as u64;
        (objects / held).saturating_mul(HELD_BYTES as u64)
    }

    /// Starts the pass over the staged packs. `rewritten` is the packs the
    /// build wrote again, ascending, as it computed them.
    pub fn pass<'a>(
        &'a mut self,
        connection: &'a Connection,
        rewritten: &'a mut dyn Iterator<Item = HistoryResult<Rewritten>>,
    ) -> HistoryResult<PackPass<'a>> {
        let units: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='pack_unit'",
                params![],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        Ok(PackPass {
            connection,
            statement: connection
                .prepare(
                    "SELECT pack_id,length,domain FROM pack \
WHERE pack_id BETWEEN ?1 AND ?2 ORDER BY pack_id",
                )
                .map_err(db_error)?,
            locators: self.list.sorted()?,
            next: rewritten.next().transpose()?,
            rewritten,
            units: units == 1,
            decompression: DecompressionWorkspace::new().map_err(|_| STAGED_PACK)?,
            from: Some(i64::MIN),
            scan: PackScan::default(),
        })
    }
}

/// What a [`PackPass`] counted: the staged file's packs, their stored bytes
/// and the metadata-domain part of those.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackScan {
    /// The rows of `pack`.
    pub packs: i64,
    /// The sum of their stored lengths.
    pub stored: i64,
    /// The part of that sum in the metadata domain.
    pub metadata: i64,
    /// The packs read back as written again.
    pub rewritten: u64,
}

/// One pass over the staged file's `pack` rows in pack order. See the module
/// comment.
pub struct PackPass<'a> {
    connection: &'a Connection,
    statement: rusqlite::Statement<'a>,
    locators: Merge<'a, Locator>,
    rewritten: &'a mut dyn Iterator<Item = HistoryResult<Rewritten>>,
    /// The next pack the build wrote again.
    next: Option<Rewritten>,
    units: bool,
    decompression: DecompressionWorkspace,
    /// The first pack identity no range has read yet.
    from: Option<i64>,
    scan: PackScan,
}

impl PackPass<'_> {
    /// Reads every pack row not yet read up to and including `last`. The
    /// ranges of one pass are consecutive, so together they are one scan of
    /// the table; the caller only chooses where to ask the file for more.
    pub fn through(&mut self, last: i64) -> HistoryResult<()> {
        let Some(from) = self.from else {
            return Ok(());
        };
        if last < from {
            return Ok(());
        }
        self.from = last.checked_add(1);
        let mut rows = self
            .statement
            .query(params![from, last])
            .map_err(db_error)?;
        while let Some(row) = rows.next().map_err(db_error)? {
            let pack: i64 = row.get(0).map_err(db_error)?;
            let length: i64 = row.get(1).map_err(db_error)?;
            let domain: i64 = row.get(2).map_err(db_error)?;
            self.scan.packs = self.scan.packs.saturating_add(1);
            self.scan.stored = self.scan.stored.saturating_add(length);
            if domain == 0 {
                self.scan.metadata = self.scan.metadata.saturating_add(length);
            }
            // A locator before this pack names a pack no row was read for.
            if self.locators.peek().is_some_and(|next| next.pack < pack) {
                return Err(FOREIGN_KEY);
            }
            match self.next.take() {
                Some(written) if written.pack_id == pack => {
                    self.next = self.rewritten.next().transpose()?;
                    self.scan.rewritten += 1;
                    read_back(
                        self.connection,
                        &written,
                        self.units,
                        &mut self.decompression,
                        &mut self.locators,
                    )?;
                }
                // The build wrote a pack the file does not hold.
                Some(written) if written.pack_id < pack => return Err(STAGED_PACK),
                other => {
                    self.next = other;
                    while self.locators.peek().is_some_and(|next| next.pack == pack) {
                        self.locators.next()?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads the rest of the table and requires both streams to be used up.
    pub fn finish(mut self) -> HistoryResult<PackScan> {
        self.through(i64::MAX)?;
        if self.next.is_some() {
            return Err(STAGED_PACK);
        }
        if self.locators.peek().is_some() {
            return Err(FOREIGN_KEY);
        }
        Ok(self.scan)
    }
}

/// Reads one rewritten pack back and takes its locators off the stream; they
/// must be exactly the records its directories state.
fn read_back(
    connection: &Connection,
    written: &Rewritten,
    units: bool,
    decompression: &mut DecompressionWorkspace,
    locators: &mut Merge<'_, Locator>,
) -> HistoryResult<()> {
    let stored = read(connection, "main", written.pack_id, units)?.ok_or(STAGED_PACK)?;
    if stored.digest.as_slice() != written.digest.as_slice()
        || stored.body.len() as u64 != written.length
    {
        return Err(STAGED_PACK);
    }
    let counts = record_counts(&stored.body, decompression).map_err(|_| STAGED_PACK)?;
    if counts.iter().sum::<usize>() as u64 != written.records {
        return Err(STAGED_PACK);
    }
    // The next record the pack holds that no locator has named yet.
    let (mut group, mut record) = (0usize, 0usize);
    let mut named = 0u64;
    while locators
        .peek()
        .is_some_and(|next| next.pack == written.pack_id)
    {
        let Some(locator) = locators.next()? else {
            break;
        };
        while counts.get(group).is_some_and(|held| record == *held) {
            group += 1;
            record = 0;
        }
        if group >= counts.len()
            || locator.group as usize != group
            || locator.record as usize != record
        {
            return Err(STAGED_LOCATOR);
        }
        record += 1;
        named += 1;
    }
    if named != written.records {
        return Err(STAGED_LOCATOR);
    }
    Ok(())
}
