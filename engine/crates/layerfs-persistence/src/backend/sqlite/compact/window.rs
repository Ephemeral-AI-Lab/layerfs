//! The pack copy's windows and the sealed file's read-ahead.
//!
//! One `INSERT ... SELECT` over every retained pack reads the sealed file in
//! the order SQLite stored it: a pack's row, then its payload pages a few
//! dozen pages *behind* that row, then the next row. No kernel reads ahead
//! across that pattern, so a source that is not already in memory is read one
//! pack per device round trip (measured on a 1.26 GB Store written past the
//! cache: 3.4 s for the copy, against 0.55 s once the file is in memory).
//!
//! The copy therefore runs one bounded window of packs at a time, and before
//! each window asks the kernel for the part of the file the window will read,
//! in one sequential request. The request is a hint: it changes no byte and no
//! result, and a file laid out in another order only loses the benefit.
//!
//! The verification reads the fresh file's `pack` rows the same way, by
//! [`staged_windows`]: a row is one page beside its payload, so the rows are
//! spread over the whole file.
//!
//! Neither plan is held: the packs arrive as a stream in pack order (the
//! run's [`super::lengths::PackLengths`]) and a window is used as soon as it
//! is cut.

use super::types::io_error;
use crate::IdSet;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::fs::File;
use std::path::Path;

/// Retained payload bytes one window copies before the next read-ahead.
pub const WINDOW_BYTES: u64 = 64 * 1024 * 1024;

/// A run of dead packs this long ends the window, so it is not read ahead.
pub const GAP_BYTES: u64 = 8 * 1024 * 1024;

/// One run of retained packs and the part of the sealed file it reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Window {
    /// The first retained pack of the run.
    pub first: i64,
    /// The last retained pack of the run.
    pub last: i64,
    /// How many retained packs the run holds.
    pub packs: u64,
    /// Where the run's payload cannot start before, in file bytes.
    pub offset: u64,
    /// How far past `offset` the run's payload can reach.
    pub bytes: u64,
}

/// Cuts the retained packs into windows, in pack order, and hands each to
/// `each` as soon as it is closed: no plan is held, so a Store of any number
/// of packs is cut in the memory of one window.
///
/// `packs` is every pack of the sealed Store with its stored length,
/// ascending, and `retained` the packs the mark keeps. A Store's
/// packs are appended in id order, so pack `p`'s payload starts no earlier
/// than the summed lengths of the packs before it and no later than that plus
/// `slack`, every byte of the file that is not pack payload; a window's range
/// is that bound over its packs.
pub fn windows(
    packs: impl Iterator<Item = HistoryResult<(i64, u64)>>,
    retained: &IdSet,
    slack: u64,
    each: &mut dyn FnMut(Window) -> HistoryResult<()>,
) -> HistoryResult<()> {
    cut(packs, retained, slack, false, each)
}

/// The windows of the file a rebuild writes. That file holds the retained
/// packs and no other, appended in the same order, so its plan is the same cut
/// over the retained packs alone: `slack` is what that file holds beyond them.
pub fn staged_windows(
    packs: impl Iterator<Item = HistoryResult<(i64, u64)>>,
    retained: &IdSet,
    slack: u64,
    each: &mut dyn FnMut(Window) -> HistoryResult<()>,
) -> HistoryResult<()> {
    cut(packs, retained, slack, true, each)
}

fn cut(
    packs: impl Iterator<Item = HistoryResult<(i64, u64)>>,
    retained: &IdSet,
    slack: u64,
    staged: bool,
    each: &mut dyn FnMut(Window) -> HistoryResult<()>,
) -> HistoryResult<()> {
    // The open run: its window so far, the payload offset it ends at and the
    // retained bytes it holds.
    let mut open: Option<(Window, u64, u64)> = None;
    let mut wanted = retained.iter().map(|pack| pack as i64).peekable();
    let mut before = 0u64;
    for pack in packs {
        let (pack, length) = pack?;
        if wanted.peek() != Some(&pack) {
            if !staged {
                before = before.saturating_add(length);
            }
            continue;
        }
        wanted.next();
        if let Some((window, end, held)) = open {
            if held >= WINDOW_BYTES || before.saturating_sub(end) > GAP_BYTES {
                each(closed(window, end, slack))?;
                open = None;
            }
        }
        let end = before.saturating_add(length);
        open = Some(match open {
            Some((window, _, held)) => (
                Window {
                    last: pack,
                    packs: window.packs + 1,
                    ..window
                },
                end,
                held.saturating_add(length),
            ),
            None => (
                Window {
                    first: pack,
                    last: pack,
                    packs: 1,
                    offset: before,
                    bytes: 0,
                },
                end,
                length,
            ),
        });
        before = end;
    }
    if wanted.next().is_some() {
        // A retained pack the Store does not hold.
        return Err(HistoryError::Integrity("mark pack"));
    }
    if let Some((window, end, _)) = open {
        each(closed(window, end, slack))?;
    }
    Ok(())
}

fn closed(window: Window, end: u64, slack: u64) -> Window {
    Window {
        bytes: end.saturating_sub(window.offset).saturating_add(slack),
        ..window
    }
}

/// One file's read-ahead: a second read-only descriptor the kernel is asked
/// through. The file (the sealed Store, or the fresh file under verification)
/// is read `immutable=1`, so SQLite holds no lock on it that closing this
/// descriptor could drop.
pub(crate) struct ReadAhead {
    file: File,
    length: u64,
    /// Everything before this offset has been asked for already.
    asked: u64,
}

impl ReadAhead {
    pub(crate) fn open(path: &Path) -> HistoryResult<Self> {
        let file = File::open(path).map_err(io_error)?;
        let length = file.metadata().map_err(io_error)?.len();
        Ok(Self {
            file,
            length,
            asked: 0,
        })
    }

    /// Asks the kernel to bring the window's part of the file into memory.
    /// Windows arrive in file order, so a byte is asked for once.
    pub(crate) fn ask(&mut self, window: &Window) {
        let start = window.offset.max(self.asked);
        let end = window.offset.saturating_add(window.bytes).min(self.length);
        if start < end {
            advise(&self.file, start, end);
            self.asked = end;
        }
    }
}

/// macOS: one advisory read per gigabyte of the range. The call returns once
/// the range is in memory (about 5 GB/s cold here, about 4 ms per gigabyte
/// when it already is).
#[cfg(target_os = "macos")]
fn advise(file: &File, start: u64, end: u64) {
    use nix::fcntl::fcntl;
    use nix::fcntl::FcntlArg;

    const STEP: u64 = 1 << 30;
    let mut offset = start;
    while offset < end {
        let count = (end - offset).min(STEP);
        let (Ok(ra_offset), Ok(ra_count)) = (
            nix::libc::off_t::try_from(offset),
            nix::libc::c_int::try_from(count),
        ) else {
            return;
        };
        let request = nix::libc::radvisory {
            ra_offset,
            ra_count,
        };
        if fcntl(file, FcntlArg::F_RDADVISE(request)).is_err() {
            return;
        }
        offset += count;
    }
}

/// Linux: `POSIX_FADV_WILLNEED` starts the reads and returns. The kernel caps
/// one request at the device's read-ahead size, which is at least 128 KiB by
/// default, so the range is asked for in steps of that size.
#[cfg(target_os = "linux")]
fn advise(file: &File, start: u64, end: u64) {
    use nix::fcntl::posix_fadvise;
    use nix::fcntl::PosixFadviseAdvice;

    const STEP: u64 = 128 * 1024;
    let mut offset = start;
    while offset < end {
        let count = (end - offset).min(STEP);
        let (Ok(at), Ok(len)) = (
            nix::libc::off_t::try_from(offset),
            nix::libc::off_t::try_from(count),
        ) else {
            return;
        };
        if posix_fadvise(file, at, len, PosixFadviseAdvice::POSIX_FADV_WILLNEED).is_err() {
            return;
        }
        offset += count;
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn advise(_file: &File, _start: u64, _end: u64) {}
