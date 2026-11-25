//! A SQLite Store's format from its header alone (#804 §2 "Showing the
//! format"): `application_id` and `user_version`, read from the file's first
//! page as the newest committed WAL frame of page 1 shows it, or the main
//! file when the WAL holds none. It takes no lock, opens no connection and
//! writes nothing, so it works on a Store this build refuses and beside a
//! live writer (a frame being written fails its checksum and is not used).
use crate::backend::sqlite::schema::APPLICATION_ID;
use crate::upgrade::{Format, FormatStatus, UpgradeError, SQLITE_LATEST};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The schema base of each SQLite format's `user_version` range: `+6` with
/// the acquisition tables, `+0/1/2` for the pack layout (rule 4).
const BASES: [(i64, i64); 3] = [(1, 16), (2, 36), (3, 56)];

/// `(schema, acquisition * 3 + layout)` of a `user_version` this family of
/// encodings names: schema 1, 2 and 3, and a later schema on the same `+20`
/// step. Transient (39–41, 45–47) and retired (1–15) values are not.
pub(crate) fn decode(user_version: i64) -> Option<(i64, i64)> {
    if user_version < 16 {
        return None;
    }
    let schema = (user_version - 16) / 20 + 1;
    let offset = (user_version - 16) % 20;
    let base = BASES
        .iter()
        .find(|(s, _)| *s == schema)
        .map_or(16 + 20 * (schema - 1), |(_, base)| *base);
    let (acquisition, layout) = match offset {
        0..=2 => (0, offset),
        6..=8 => (1, offset - 6),
        _ => return None,
    };
    debug_assert_eq!(base + acquisition * 6 + layout, user_version);
    Some((schema, acquisition * 3 + layout))
}

/// What a header says, in the words the refusals use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Header {
    /// Not a LayerFS SQLite Store (another `application_id`, or no header).
    Foreign,
    /// A `user_version` this build knows by schema.
    Schema(i64),
    /// 39–41/45–47: the version that existed for three hours on 11-24 and
    /// that no installed build wrote (rule 10).
    Transient,
    /// 1–15: the retired 2.0 development versions.
    Retired,
    /// Any other value.
    Unrecognized,
}

/// Classifies one header.
pub(crate) fn classify(application_id: i64, user_version: i64) -> Header {
    if application_id != APPLICATION_ID {
        return Header::Foreign;
    }
    match user_version {
        1..=15 => Header::Retired,
        39..=41 | 45..=47 => Header::Transient,
        _ => match decode(user_version) {
            Some((schema, _)) => Header::Schema(schema),
            None => Header::Unrecognized,
        },
    }
}

/// The format and status of one header.
pub(crate) fn format(header: Header) -> Format {
    let (version, status) = match header {
        Header::Schema(SQLITE_LATEST) => (Some(SQLITE_LATEST), FormatStatus::Current),
        Header::Schema(2) => (Some(2), FormatStatus::Outdated),
        Header::Schema(v) if v > SQLITE_LATEST => (Some(v), FormatStatus::Newer),
        Header::Schema(v) => (Some(v), FormatStatus::Unsupported),
        Header::Foreign | Header::Transient | Header::Retired | Header::Unrecognized => {
            (None, FormatStatus::Unsupported)
        }
    };
    Format {
        version,
        latest: SQLITE_LATEST,
        status,
    }
}

/// Why a Store of `header` is not upgraded, by name; `None` for a Store
/// the chain takes (schema 2) or that is current.
pub(crate) fn refusal(header: Header, user_version: i64) -> Option<UpgradeError> {
    let text = match header {
        Header::Schema(2) | Header::Schema(SQLITE_LATEST) => return None,
        Header::Foreign => "not a LayerFS SQLite Store (another application_id)".to_owned(),
        Header::Schema(1) => format!(
            "SQLite Store schema version 1 (user_version {user_version}, a 1.x Store before \
             #91): this build has no upgrade for it"
        ),
        Header::Schema(v) if v > SQLITE_LATEST => format!(
            "SQLite Store format {v} (user_version {user_version}) is newer than this build, \
             which writes format {SQLITE_LATEST}"
        ),
        Header::Schema(v) => format!("SQLite Store format {v} has no upgrade step"),
        Header::Transient => format!(
            "SQLite Store user_version {user_version} is the transient schema of 11-24 that \
             no released build wrote: it has no upgrade step"
        ),
        Header::Retired => format!(
            "SQLite Store user_version {user_version} is a retired development version: it \
             has no upgrade step"
        ),
        Header::Unrecognized => format!(
            "SQLite Store user_version {user_version} is not a format this build recognizes"
        ),
    };
    Some(UpgradeError::Unsupported(text))
}

/// `(application_id, user_version)` of the Store file at `path`.
pub(crate) fn read_header(path: &Path) -> Result<(i64, i64), UpgradeError> {
    let failed =
        |e: std::io::Error| UpgradeError::Failed(format!("reading {}: {e}", path.display()));
    let mut head = [0u8; 100];
    let mut file = std::fs::File::open(path).map_err(failed)?;
    let read = read_full(&mut file, &mut head).map_err(failed)?;
    let main = (read == 100 && &head[..16] == b"SQLite format 3\0").then_some(head);
    let page = match wal_page_one(&with_suffix(path, "-wal")).map_err(failed)? {
        Some(page) => page,
        None => match main {
            Some(head) => head.to_vec(),
            // An empty or foreign file has no SQLite header at all.
            None => return Ok((0, 0)),
        },
    };
    let word = |at: usize| {
        i64::from(i32::from_be_bytes([
            page[at],
            page[at + 1],
            page[at + 2],
            page[at + 3],
        ]))
    };
    Ok((word(68), word(60)))
}

fn read_full(file: &mut std::fs::File, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut done = 0;
    while done < buffer.len() {
        match file.read(&mut buffer[done..])? {
            0 => break,
            n => done += n,
        }
    }
    Ok(done)
}

pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// The image of page 1 in the newest committed frame of the WAL at `path`,
/// if any: frames are valid while their salts match the WAL header and
/// their cumulative checksums verify (SQLite's own recovery rule), and a
/// page image counts only once a later or same commit frame ends it.
fn wal_page_one(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut header = [0u8; 32];
    if read_full(&mut file, &mut header)? < 32 {
        return Ok(None);
    }
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    let magic = be(&header[0..4]);
    let big = match magic {
        0x377f_0682 => false,
        0x377f_0683 => true,
        _ => return Ok(None),
    };
    let page_size = be(&header[8..12]) as usize;
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Ok(None);
    }
    let mut sum = checksum(&header[..24], big, (0, 0));
    if sum != (be(&header[24..28]), be(&header[28..32])) {
        return Ok(None);
    }
    let salts = &header[16..24];
    let mut frame = vec![0u8; 24 + page_size];
    let (mut pending, mut committed) = (None, None);
    while read_full(&mut file, &mut frame)? == frame.len() {
        if &frame[8..16] != salts {
            break;
        }
        sum = checksum(&frame[..8], big, sum);
        sum = checksum(&frame[24..], big, sum);
        if sum != (be(&frame[16..20]), be(&frame[20..24])) {
            break;
        }
        if be(&frame[0..4]) == 1 {
            pending = Some(frame[24..].to_vec());
        }
        if be(&frame[4..8]) != 0 && pending.is_some() {
            committed = pending.take();
        }
    }
    Ok(committed)
}

/// SQLite's WAL checksum over `data` (a multiple of 8 bytes), continuing `s`.
fn checksum(data: &[u8], big: bool, (mut s0, mut s1): (u32, u32)) -> (u32, u32) {
    let word = |b: &[u8]| {
        let bytes = [b[0], b[1], b[2], b[3]];
        if big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    };
    for pair in data.chunks_exact(8) {
        s0 = s0.wrapping_add(word(&pair[0..4])).wrapping_add(s1);
        s1 = s1.wrapping_add(word(&pair[4..8])).wrapping_add(s0);
    }
    (s0, s1)
}
