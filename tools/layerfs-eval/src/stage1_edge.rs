use crate::stage1_fixture::{self, EvalResult};
use layerfs_sdk::{
    Diagnostics, IntegrityMode, LayerFs, NativeMetadata, RefState, RootId,
    PRODUCT_BUFFER_BOUND_BYTES,
};
use std::cell::RefCell;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FILE_PATH: &str = "data/payload.bin";
const INITIAL_BYTES: u64 = 25_165_824;
const MAXIMUM_BYTES: u64 = 25_227_264;
const MAX_USER_FILE_BYTES: u64 = 33_554_431;
const BUFFER_BYTES: usize = 1_048_576;
const REPLACEMENT_BACKING_BYTES: usize = 495_616;
const FIXTURE_VERSION: &str = "apple-edge-v1";
const FIXTURE_MODE: u32 = 0o644;
const FIXTURE_MTIME_SECONDS: u64 = 1_700_000_123;
const FIXTURE_MTIME_NANOSECONDS: u32 = 456_789_123;
const RESET_LIMIT_NS: u128 = 5_000_000_000;
const PREPARATION_LIMIT_NS: u128 = 30_000_000_000;
const CAMPAIGN_LIMIT_NS: u128 = 60_000_000_000;
const FROZEN_NON_RESET_FORECAST_NS: u128 = 45_000_000_000;
const READINESS_SCHEMA: &str = "layerfs-stage1.1-readiness-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disposition {
    Pass,
    Revise,
    Fail,
}

impl Disposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Revise => "REVISE",
            Self::Fail => "FAIL",
        }
    }
}

struct FailureContext {
    row_id: String,
    phase: &'static str,
    started: Option<Instant>,
}

impl Default for FailureContext {
    fn default() -> Self {
        Self {
            row_id: String::new(),
            phase: "admission",
            started: None,
        }
    }
}

thread_local! {
    static FAILURE_CONTEXT: RefCell<FailureContext> = RefCell::new(FailureContext::default());
}

fn begin_failure_context(row_id: &str, phase: &'static str) {
    FAILURE_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        if context.row_id != row_id {
            context.row_id = row_id.to_owned();
            context.started = Some(Instant::now());
        }
        context.phase = phase;
    });
}

fn set_failure_phase(phase: &'static str) {
    FAILURE_CONTEXT.with(|context| context.borrow_mut().phase = phase);
}

fn failure_observation() -> (String, &'static str, u128) {
    FAILURE_CONTEXT.with(|context| {
        let context = context.borrow();
        (
            context.row_id.clone(),
            context.phase,
            context
                .started
                .as_ref()
                .map_or(Duration::ZERO, Instant::elapsed)
                .as_nanos(),
        )
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditKind {
    Overwrite,
    Insert,
    Delete,
    Append,
    Truncate,
}

impl EditKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Overwrite => "overwrite",
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Append => "append",
            Self::Truncate => "truncate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditSpec {
    tag: String,
    serial: u8,
    epoch: u8,
    kind: EditKind,
    size_band: &'static str,
    offset: u64,
    delete_bytes: u64,
    insert_bytes: u64,
    before_bytes: u64,
    after_bytes: u64,
    replacement_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BurstSpec {
    root: u8,
    pattern: &'static str,
    edits: Vec<EditSpec>,
}

type FrozenBurstEdit = (EditKind, u64, u64, u64, u64, u64);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScheduledRow {
    row_index: usize,
    row_id: String,
    row_group: &'static str,
    sequence: u8,
    epoch: u8,
    direction: &'static str,
    operation: &'static str,
    size_band: &'static str,
    edit_index: Option<usize>,
    burst_index: Option<usize>,
    history_session: Option<u8>,
    milestone_root: Option<u8>,
    transition_root: Option<u8>,
}

#[derive(Clone, Debug)]
struct FrozenSchedule {
    edits: Vec<EditSpec>,
    bursts: Vec<BurstSpec>,
    rows: Vec<ScheduledRow>,
    replacement_backing: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Piece {
    Original { offset: u64, length: u64 },
    Inserted { offset: usize, length: u64 },
}

impl Piece {
    fn length(&self) -> u64 {
        match self {
            Self::Original { length, .. } | Self::Inserted { length, .. } => *length,
        }
    }

    fn slice(&self, offset: u64, length: u64) -> EvalResult<Self> {
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.length())
        {
            return Err("piece slice exceeds source".to_owned());
        }
        match self {
            Self::Original { offset: source, .. } => Ok(Self::Original {
                offset: source
                    .checked_add(offset)
                    .ok_or_else(|| "original piece offset overflow".to_owned())?,
                length,
            }),
            Self::Inserted { offset: source, .. } => Ok(Self::Inserted {
                offset: source
                    .checked_add(usize::try_from(offset).map_err(display_error)?)
                    .ok_or_else(|| "inserted piece offset overflow".to_owned())?,
                length,
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PieceTable {
    pieces: Vec<Piece>,
    logical_length: u64,
}

impl PieceTable {
    fn initial() -> Self {
        Self {
            pieces: vec![Piece::Original {
                offset: 0,
                length: INITIAL_BYTES,
            }],
            logical_length: INITIAL_BYTES,
        }
    }

    fn splice(&mut self, edit: &EditSpec) -> EvalResult<()> {
        if self.logical_length != edit.before_bytes {
            return Err(format!(
                "{} before length {} != {}",
                edit.tag, self.logical_length, edit.before_bytes
            ));
        }
        let end = edit
            .offset
            .checked_add(edit.delete_bytes)
            .ok_or_else(|| format!("{} delete range overflow", edit.tag))?;
        if end > self.logical_length {
            return Err(format!("{} delete range exceeds file", edit.tag));
        }
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut logical = 0_u64;
        for piece in &self.pieces {
            let piece_end = logical
                .checked_add(piece.length())
                .ok_or_else(|| "piece-table length overflow".to_owned())?;
            if piece_end <= edit.offset {
                left.push(piece.clone());
            } else if logical >= end {
                right.push(piece.clone());
            } else {
                if logical < edit.offset {
                    left.push(piece.slice(0, edit.offset - logical)?);
                }
                if piece_end > end {
                    right.push(piece.slice(end - logical, piece_end - end)?);
                }
            }
            logical = piece_end;
        }
        if logical != self.logical_length {
            return Err("piece-table stored length mismatch".to_owned());
        }
        if edit.insert_bytes != 0 {
            left.push(Piece::Inserted {
                offset: edit.replacement_offset,
                length: edit.insert_bytes,
            });
        }
        left.extend(right);
        self.pieces = left;
        self.logical_length = self
            .logical_length
            .checked_sub(edit.delete_bytes)
            .and_then(|value| value.checked_add(edit.insert_bytes))
            .ok_or_else(|| "piece-table splice length overflow".to_owned())?;
        self.coalesce()?;
        if self.logical_length != edit.after_bytes {
            return Err(format!(
                "{} after length {} != {}",
                edit.tag, self.logical_length, edit.after_bytes
            ));
        }
        Ok(())
    }

    fn coalesce(&mut self) -> EvalResult<()> {
        let mut output: Vec<Piece> = Vec::with_capacity(self.pieces.len());
        for piece in self.pieces.drain(..) {
            if piece.length() == 0 {
                continue;
            }
            let merged = match (output.last_mut(), &piece) {
                (
                    Some(Piece::Original { offset, length }),
                    Piece::Original {
                        offset: next,
                        length: next_len,
                    },
                ) if offset.checked_add(*length) == Some(*next) => {
                    *length = length
                        .checked_add(*next_len)
                        .ok_or_else(|| "original coalesce overflow".to_owned())?;
                    true
                }
                (
                    Some(Piece::Inserted { offset, length }),
                    Piece::Inserted {
                        offset: next,
                        length: next_len,
                    },
                ) if offset.checked_add(usize::try_from(*length).map_err(display_error)?)
                    == Some(*next) =>
                {
                    *length = length
                        .checked_add(*next_len)
                        .ok_or_else(|| "inserted coalesce overflow".to_owned())?;
                    true
                }
                _ => false,
            };
            if !merged {
                output.push(piece);
            }
        }
        self.pieces = output;
        Ok(())
    }

    fn range(&self, start: u64, length: u64) -> EvalResult<Self> {
        let end = start
            .checked_add(length)
            .ok_or_else(|| "piece-table range overflow".to_owned())?;
        if end > self.logical_length {
            return Err("piece-table range exceeds logical length".to_owned());
        }
        let mut pieces = Vec::new();
        let mut logical = 0_u64;
        for piece in &self.pieces {
            let piece_end = logical
                .checked_add(piece.length())
                .ok_or_else(|| "piece-table range position overflow".to_owned())?;
            let overlap_start = logical.max(start);
            let overlap_end = piece_end.min(end);
            if overlap_start < overlap_end {
                pieces.push(piece.slice(overlap_start - logical, overlap_end - overlap_start)?);
            }
            logical = piece_end;
            if logical >= end {
                break;
            }
        }
        Ok(Self {
            pieces,
            logical_length: length,
        })
    }

    #[cfg(test)]
    fn stream<W: Write>(&self, backing: &[u8], output: &mut W) -> EvalResult<()> {
        let mut scratch = vec![0_u8; BUFFER_BYTES];
        for piece in &self.pieces {
            match piece {
                Piece::Original { offset, length } => {
                    stream_original(*offset, *length, &mut scratch, output)?;
                }
                Piece::Inserted { offset, length } => {
                    let length = usize::try_from(*length).map_err(display_error)?;
                    let end = offset
                        .checked_add(length)
                        .ok_or_else(|| "replacement range overflow".to_owned())?;
                    output
                        .write_all(
                            backing
                                .get(*offset..end)
                                .ok_or_else(|| "replacement range exceeds backing".to_owned())?,
                        )
                        .map_err(io_error)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn stream_original<W: Write>(
    start: u64,
    length: u64,
    scratch: &mut [u8],
    output: &mut W,
) -> EvalResult<()> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| "original oracle range overflow".to_owned())?;
    if end > INITIAL_BYTES || scratch.len() != BUFFER_BYTES {
        return Err("original oracle request exceeds frozen fixture".to_owned());
    }
    let mut position = start;
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        stage1_fixture::fill_retained_buffer(scratch, block);
        let within = usize::try_from(position - block).map_err(display_error)?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(display_error)?;
        output
            .write_all(&scratch[within..within + take])
            .map_err(io_error)?;
        position += take as u64;
    }
    Ok(())
}

fn replacement_bytes(serial: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            serial
                .wrapping_mul(17)
                .wrapping_add((index as u8).wrapping_mul(31))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn push_edit(
    edits: &mut Vec<EditSpec>,
    replacement_backing: &mut Vec<u8>,
    tag: String,
    serial: u8,
    epoch: u8,
    kind: EditKind,
    size_band: &'static str,
    offset: u64,
    delete_bytes: u64,
    insert_bytes: u64,
    before_bytes: u64,
    after_bytes: u64,
) -> EvalResult<()> {
    let replacement_offset = replacement_backing.len();
    replacement_backing.extend(replacement_bytes(
        serial,
        usize::try_from(insert_bytes).map_err(display_error)?,
    ));
    edits.push(EditSpec {
        tag,
        serial,
        epoch,
        kind,
        size_band,
        offset,
        delete_bytes,
        insert_bytes,
        before_bytes,
        after_bytes,
        replacement_offset,
    });
    Ok(())
}

fn frozen_schedule() -> EvalResult<FrozenSchedule> {
    let mut edits = Vec::new();
    let mut replacement_backing = Vec::with_capacity(REPLACEMENT_BACKING_BYTES);

    let physical = [
        (
            EditKind::Overwrite,
            "near-8-kib",
            3_378_088,
            8_191,
            8_191,
            25_165_824,
            25_165_824,
        ),
        (
            EditKind::Insert,
            "near-16-kib",
            4_221_363,
            0,
            16_384,
            25_165_824,
            25_182_208,
        ),
        (
            EditKind::Delete,
            "near-32-kib",
            19_479_758,
            32_769,
            0,
            25_182_208,
            25_149_439,
        ),
        (
            EditKind::Append,
            "near-8-kib",
            25_149_439,
            0,
            8_193,
            25_149_439,
            25_157_632,
        ),
        (
            EditKind::Truncate,
            "near-16-kib",
            25_141_248,
            16_384,
            0,
            25_157_632,
            25_141_248,
        ),
        (
            EditKind::Insert,
            "near-8-kib",
            13_344_955,
            0,
            8_192,
            25_141_248,
            25_149_440,
        ),
        (
            EditKind::Delete,
            "near-16-kib",
            19_223_620,
            16_385,
            0,
            25_149_440,
            25_133_055,
        ),
        (
            EditKind::Append,
            "near-32-kib",
            25_133_055,
            0,
            32_769,
            25_133_055,
            25_165_824,
        ),
        (
            EditKind::Truncate,
            "near-8-kib",
            25_157_632,
            8_192,
            0,
            25_165_824,
            25_157_632,
        ),
        (
            EditKind::Overwrite,
            "near-16-kib",
            2_461_634,
            16_383,
            16_383,
            25_157_632,
            25_157_632,
        ),
        (
            EditKind::Delete,
            "near-8-kib",
            19_138_305,
            8_193,
            0,
            25_157_632,
            25_149_439,
        ),
        (
            EditKind::Append,
            "near-16-kib",
            25_149_439,
            0,
            16_385,
            25_149_439,
            25_165_824,
        ),
        (
            EditKind::Truncate,
            "near-32-kib",
            25_133_056,
            32_768,
            0,
            25_165_824,
            25_133_056,
        ),
        (
            EditKind::Overwrite,
            "near-32-kib",
            9_130_636,
            32_767,
            32_767,
            25_133_056,
            25_133_056,
        ),
        (
            EditKind::Insert,
            "near-32-kib",
            11_257_438,
            0,
            32_768,
            25_133_056,
            25_165_824,
        ),
    ];
    for (index, &(kind, band, offset, delete, insert, before, after)) in physical.iter().enumerate()
    {
        push_edit(
            &mut edits,
            &mut replacement_backing,
            format!("P{:02}", index + 1),
            u8::try_from(index + 1).map_err(display_error)?,
            u8::try_from(index / 5 + 1).map_err(display_error)?,
            kind,
            band,
            offset,
            delete,
            insert,
            before,
            after,
        )?;
    }

    let logical = [
        (
            EditKind::Overwrite,
            "near-8-kib",
            3_167_684,
            8_191,
            8_191,
            25_165_824,
            25_165_824,
        ),
        (
            EditKind::Insert,
            "near-16-kib",
            9_979_080,
            0,
            16_384,
            25_165_824,
            25_182_208,
        ),
        (
            EditKind::Delete,
            "near-32-kib",
            20_965_809,
            32_769,
            0,
            25_182_208,
            25_149_439,
        ),
        (
            EditKind::Append,
            "near-8-kib",
            25_149_439,
            0,
            8_193,
            25_149_439,
            25_157_632,
        ),
        (
            EditKind::Truncate,
            "near-16-kib",
            25_141_248,
            16_384,
            0,
            25_157_632,
            25_141_248,
        ),
        (
            EditKind::Insert,
            "near-8-kib",
            3_990_642,
            0,
            8_192,
            25_141_248,
            25_149_440,
        ),
        (
            EditKind::Delete,
            "near-16-kib",
            16_550_428,
            16_385,
            0,
            25_149_440,
            25_133_055,
        ),
        (
            EditKind::Append,
            "near-32-kib",
            25_133_055,
            0,
            32_769,
            25_133_055,
            25_165_824,
        ),
        (
            EditKind::Truncate,
            "near-8-kib",
            25_157_632,
            8_192,
            0,
            25_165_824,
            25_157_632,
        ),
        (
            EditKind::Overwrite,
            "near-16-kib",
            22_880_155,
            16_383,
            16_383,
            25_157_632,
            25_157_632,
        ),
        (
            EditKind::Delete,
            "near-8-kib",
            4_308_809,
            8_193,
            0,
            25_157_632,
            25_149_439,
        ),
        (
            EditKind::Append,
            "near-16-kib",
            25_149_439,
            0,
            16_385,
            25_149_439,
            25_165_824,
        ),
        (
            EditKind::Truncate,
            "near-32-kib",
            25_133_056,
            32_768,
            0,
            25_165_824,
            25_133_056,
        ),
        (
            EditKind::Overwrite,
            "near-32-kib",
            10_813_201,
            32_767,
            32_767,
            25_133_056,
            25_133_056,
        ),
        (
            EditKind::Insert,
            "near-32-kib",
            19_272_909,
            0,
            32_768,
            25_133_056,
            25_165_824,
        ),
    ];
    for (index, &(kind, band, offset, delete, insert, before, after)) in logical.iter().enumerate()
    {
        let sequence = index + 16;
        push_edit(
            &mut edits,
            &mut replacement_backing,
            format!("L{sequence:02}"),
            u8::try_from(sequence).map_err(display_error)?,
            u8::try_from(index / 5 + 4).map_err(display_error)?,
            kind,
            band,
            offset,
            delete,
            insert,
            before,
            after,
        )?;
    }

    let burst_rows: [(&str, &[FrozenBurstEdit]); 4] = [
        (
            "autosave-hotspot",
            &[
                (
                    EditKind::Overwrite,
                    8_388_611,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_391_683,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_394_755,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_397_827,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_400_899,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_403_971,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_407_043,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    8_410_115,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
            ],
        ),
        (
            "insertion-boundary",
            &[
                (
                    EditKind::Insert,
                    12_582_913,
                    0,
                    16_384,
                    25_165_824,
                    25_182_208,
                ),
                (
                    EditKind::Overwrite,
                    12_595_201,
                    8_192,
                    8_192,
                    25_182_208,
                    25_182_208,
                ),
                (
                    EditKind::Delete,
                    12_591_105,
                    12_288,
                    0,
                    25_182_208,
                    25_169_920,
                ),
            ],
        ),
        (
            "append-rotation",
            &[
                (
                    EditKind::Append,
                    25_169_920,
                    0,
                    8_192,
                    25_169_920,
                    25_178_112,
                ),
                (
                    EditKind::Append,
                    25_178_112,
                    0,
                    16_384,
                    25_178_112,
                    25_194_496,
                ),
                (
                    EditKind::Append,
                    25_194_496,
                    0,
                    32_768,
                    25_194_496,
                    25_227_264,
                ),
                (
                    EditKind::Truncate,
                    25_165_824,
                    61_440,
                    0,
                    25_227_264,
                    25_165_824,
                ),
            ],
        ),
        (
            "alternating-distant",
            &[
                (
                    EditKind::Overwrite,
                    1_048_579,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    24_117_251,
                    8_192,
                    8_192,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    2_097_157,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    23_068_673,
                    8_192,
                    8_192,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    3_145_731,
                    4_096,
                    4_096,
                    25_165_824,
                    25_165_824,
                ),
                (
                    EditKind::Overwrite,
                    22_020_099,
                    8_192,
                    8_192,
                    25_165_824,
                    25_165_824,
                ),
            ],
        ),
    ];
    let mut burst_ranges = Vec::new();
    for (burst_index, (pattern, entries)) in burst_rows.iter().enumerate() {
        let start = edits.len();
        for (sub_index, &(kind, offset, delete, insert, before, after)) in
            entries.iter().enumerate()
        {
            let serial = u8::try_from(edits.len() + 1).map_err(display_error)?;
            push_edit(
                &mut edits,
                &mut replacement_backing,
                format!("R{}.{}", burst_index + 31, sub_index + 1),
                serial,
                7,
                kind,
                "burst",
                offset,
                delete,
                insert,
                before,
                after,
            )?;
        }
        burst_ranges.push((start, edits.len(), *pattern));
    }
    let bursts = burst_ranges
        .into_iter()
        .enumerate()
        .map(|(index, (start, end, pattern))| BurstSpec {
            root: u8::try_from(index + 31).expect("four frozen burst roots fit u8"),
            pattern,
            edits: edits[start..end].to_vec(),
        })
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let mut push_row = |row_id: String,
                        row_group: &'static str,
                        sequence: u8,
                        epoch: u8,
                        direction: &'static str,
                        operation: &'static str,
                        size_band: &'static str,
                        edit_index: Option<usize>,
                        burst_index: Option<usize>,
                        history_session: Option<u8>,
                        milestone_root: Option<u8>,
                        transition_root: Option<u8>| {
        rows.push(ScheduledRow {
            row_index: rows.len(),
            row_id,
            row_group,
            sequence,
            epoch,
            direction,
            operation,
            size_band,
            edit_index,
            burst_index,
            history_session,
            milestone_root,
            transition_root,
        });
    };
    push_row(
        "C00-001".to_owned(),
        "C00",
        0,
        0,
        "witness",
        "admission",
        "NotApplicable",
        None,
        None,
        None,
        None,
        None,
    );
    push_row(
        "C01-001".to_owned(),
        "C01",
        0,
        0,
        "witness",
        "reset",
        "NotApplicable",
        None,
        None,
        None,
        None,
        None,
    );
    push_row(
        "C02-001".to_owned(),
        "C02",
        0,
        0,
        "witness",
        "materialize",
        "NotApplicable",
        None,
        None,
        None,
        None,
        None,
    );
    for epoch in 0..3 {
        for within in 0..5 {
            let index = epoch * 5 + within;
            let edit = &edits[index];
            push_row(
                format!("C03-{:03}", index + 1),
                "C03",
                u8::try_from(index + 1).map_err(display_error)?,
                edit.epoch,
                "physical-to-logical",
                edit.kind.as_str(),
                edit.size_band,
                Some(index),
                None,
                None,
                None,
                Some(u8::try_from(index + 1).map_err(display_error)?),
            );
        }
        push_row(
            format!("C04-{:03}", epoch + 1),
            "C04",
            u8::try_from(epoch + 1).map_err(display_error)?,
            u8::try_from(epoch + 1).map_err(display_error)?,
            "witness",
            "verified-history",
            "NotApplicable",
            None,
            None,
            Some(u8::try_from(epoch + 1).map_err(display_error)?),
            None,
            None,
        );
    }
    for epoch in 0..3 {
        for within in 0..5 {
            let index = 15 + epoch * 5 + within;
            let edit = &edits[index];
            push_row(
                format!("C05-{:03}", index - 14),
                "C05",
                u8::try_from(index + 1).map_err(display_error)?,
                edit.epoch,
                "logical-to-physical",
                edit.kind.as_str(),
                edit.size_band,
                Some(index),
                None,
                None,
                None,
                Some(u8::try_from(index + 1).map_err(display_error)?),
            );
        }
        push_row(
            format!("C06-{:03}", epoch + 1),
            "C06",
            u8::try_from(epoch + 4).map_err(display_error)?,
            u8::try_from(epoch + 4).map_err(display_error)?,
            "witness",
            "verified-history",
            "NotApplicable",
            None,
            None,
            Some(u8::try_from(epoch + 4).map_err(display_error)?),
            None,
            None,
        );
    }
    for index in 0..4 {
        push_row(
            format!("C07-{:03}", index + 1),
            "C07",
            u8::try_from(index + 31).map_err(display_error)?,
            7,
            "burst",
            "burst",
            "burst",
            None,
            Some(index),
            None,
            None,
            Some(u8::try_from(index + 31).map_err(display_error)?),
        );
    }
    for (index, root) in [15_u8, 30, 34].into_iter().enumerate() {
        push_row(
            format!("C08-{:03}", index + 1),
            "C08",
            root,
            8,
            "witness",
            "milestone-materialize",
            "NotApplicable",
            None,
            None,
            None,
            Some(root),
            None,
        );
    }
    push_row(
        "C09-001".to_owned(),
        "C09",
        0,
        9,
        "witness",
        "terminal-resources",
        "NotApplicable",
        None,
        None,
        None,
        None,
        None,
    );

    let schedule = FrozenSchedule {
        edits,
        bursts,
        rows,
        replacement_backing,
    };
    validate_schedule(&schedule)?;
    Ok(schedule)
}

fn validate_schedule(schedule: &FrozenSchedule) -> EvalResult<()> {
    if schedule.rows.len() != 47
        || schedule.edits.len() != 51
        || schedule.bursts.len() != 4
        || schedule.replacement_backing.len() != REPLACEMENT_BACKING_BYTES
    {
        return Err("frozen 47/51/4 population mismatch".to_owned());
    }
    let mut row_ids = schedule
        .rows
        .iter()
        .map(|row| row.row_id.as_str())
        .collect::<Vec<_>>();
    row_ids.sort_unstable();
    if row_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("frozen row IDs are not unique".to_owned());
    }
    if schedule
        .rows
        .iter()
        .enumerate()
        .any(|(index, row)| row.row_index != index)
    {
        return Err("frozen row indices are not ordered".to_owned());
    }
    let transitions = schedule
        .rows
        .iter()
        .filter(|row| row.transition_root.is_some())
        .count();
    if transitions != 34 {
        return Err(format!("frozen transition count is {transitions}, not 34"));
    }
    let mut table = PieceTable::initial();
    let mut snapshots = vec![table.clone()];
    let mut maximum = table.logical_length;
    for edit in &schedule.edits[..30] {
        table.splice(edit)?;
        maximum = maximum.max(table.logical_length);
        snapshots.push(table.clone());
    }
    for burst in &schedule.bursts {
        for edit in &burst.edits {
            table.splice(edit)?;
            maximum = maximum.max(table.logical_length);
        }
        snapshots.push(table.clone());
    }
    let descriptor_total = snapshots.iter().try_fold(0_usize, |total, snapshot| {
        total
            .checked_add(snapshot.pieces.len())
            .ok_or_else(|| "snapshot descriptor count overflow".to_owned())
    })?;
    if snapshots.len() != 35
        || maximum != MAXIMUM_BYTES
        || maximum > MAX_USER_FILE_BYTES
        || table.logical_length != INITIAL_BYTES
        || table.pieces.len() > 103
        || descriptor_total > 1_315
    {
        return Err(format!(
            "oracle bounds mismatch: snapshots={} max={} terminal={} live_descriptors={} snapshot_descriptors={descriptor_total}",
            snapshots.len(), maximum, table.logical_length, table.pieces.len()
        ));
    }
    Ok(())
}

fn oracle_snapshots(schedule: &FrozenSchedule) -> EvalResult<Vec<PieceTable>> {
    let mut table = PieceTable::initial();
    let mut snapshots = vec![table.clone()];
    for edit in &schedule.edits[..30] {
        table.splice(edit)?;
        snapshots.push(table.clone());
    }
    for burst in &schedule.bursts {
        for edit in &burst.edits {
            table.splice(edit)?;
        }
        snapshots.push(table.clone());
    }
    Ok(snapshots)
}

#[derive(Clone, Debug)]
struct FixtureMaster {
    raw_digest: String,
    root: RootId,
    generation: u64,
    store_id: String,
    profile: String,
    apfs_identity: String,
    fixture_blake3: String,
    preparation_wall_ns: u128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EngineDelta {
    transactions_started: u64,
    transactions_committed: u64,
    transactions_rolled_back: u64,
    statements: u64,
    admission_transactions_started: u64,
    admission_transactions_committed: u64,
    admission_transactions_rolled_back: u64,
    admission_statements: u64,
    integrity_transactions_started: u64,
    integrity_transactions_committed: u64,
    integrity_transactions_rolled_back: u64,
    integrity_statements: u64,
    busy_events: u64,
    locked_events: u64,
    objects_validated: u64,
    objects_created: u64,
    objects_reused: u64,
    object_bytes_read: u64,
    object_bytes_written: u64,
    range_bytes_requested: u64,
    range_bytes_returned: u64,
    logical_object_bytes: u64,
    logical_root_bytes: u64,
    logical_delta_bytes: u64,
    retained_union_scrubs: u64,
    root_verifications: u64,
    root_verification_objects: u64,
    root_verification_bytes: u64,
    fetched_rows: u64,
    fetched_row_authentication_passes: u64,
    fetched_row_role_decode_passes: u64,
    new_object_authentication_passes: u64,
    incumbent_authentication_passes: u64,
    payload_batch_queries: u64,
    payload_batch_references: u64,
    payload_batch_maximum: u64,
    put_lookup_statements: u64,
    put_insert_statements: u64,
    created_rows: u64,
    reused_rows: u64,
    publication_transactions_started: u64,
    publication_transactions_rolled_back: u64,
    publication_commits: u64,
    publication_closure_passes: u64,
    namespace_graph_verification_passes: u64,
    scratch_tables: u64,
    scratch_statements: u64,
    scratch_rows: u64,
    scratch_high_water_bytes: u64,
    retained_roots_validated: u64,
}

impl EngineDelta {
    fn between(before: &Diagnostics, after: &Diagnostics) -> EvalResult<Self> {
        macro_rules! delta {
            ($field:ident) => {
                after.$field.checked_sub(before.$field).ok_or_else(|| {
                    format!("engine counter {} moved backward", stringify!($field))
                })?
            };
        }
        Ok(Self {
            transactions_started: delta!(transactions_started),
            transactions_committed: delta!(transactions_committed),
            transactions_rolled_back: delta!(transactions_rolled_back),
            statements: delta!(statements),
            admission_transactions_started: delta!(admission_transactions_started),
            admission_transactions_committed: delta!(admission_transactions_committed),
            admission_transactions_rolled_back: delta!(admission_transactions_rolled_back),
            admission_statements: delta!(admission_statements),
            integrity_transactions_started: delta!(integrity_transactions_started),
            integrity_transactions_committed: delta!(integrity_transactions_committed),
            integrity_transactions_rolled_back: delta!(integrity_transactions_rolled_back),
            integrity_statements: delta!(integrity_statements),
            busy_events: delta!(busy_events),
            locked_events: delta!(locked_events),
            objects_validated: delta!(objects_validated),
            objects_created: delta!(objects_created),
            objects_reused: delta!(objects_reused),
            object_bytes_read: delta!(object_bytes_read),
            object_bytes_written: delta!(object_bytes_written),
            range_bytes_requested: delta!(range_bytes_requested),
            range_bytes_returned: delta!(range_bytes_returned),
            logical_object_bytes: delta!(logical_object_bytes),
            logical_root_bytes: delta!(logical_root_bytes),
            logical_delta_bytes: delta!(logical_delta_bytes),
            retained_union_scrubs: delta!(retained_union_scrubs),
            root_verifications: delta!(root_verifications),
            root_verification_objects: delta!(root_verification_objects),
            root_verification_bytes: delta!(root_verification_bytes),
            fetched_rows: delta!(fetched_rows),
            fetched_row_authentication_passes: delta!(fetched_row_authentication_passes),
            fetched_row_role_decode_passes: delta!(fetched_row_role_decode_passes),
            new_object_authentication_passes: delta!(new_object_authentication_passes),
            incumbent_authentication_passes: delta!(incumbent_authentication_passes),
            payload_batch_queries: delta!(payload_batch_queries),
            payload_batch_references: delta!(payload_batch_references),
            payload_batch_maximum: after
                .payload_batch_maximum
                .max(before.payload_batch_maximum),
            put_lookup_statements: delta!(put_lookup_statements),
            put_insert_statements: delta!(put_insert_statements),
            created_rows: delta!(created_rows),
            reused_rows: delta!(reused_rows),
            publication_transactions_started: delta!(publication_transactions_started),
            publication_transactions_rolled_back: delta!(publication_transactions_rolled_back),
            publication_commits: delta!(publication_commits),
            publication_closure_passes: delta!(publication_closure_passes),
            namespace_graph_verification_passes: delta!(namespace_graph_verification_passes),
            scratch_tables: delta!(scratch_tables),
            scratch_statements: delta!(scratch_statements),
            scratch_rows: delta!(scratch_rows),
            scratch_high_water_bytes: after
                .scratch_high_water_bytes
                .max(before.scratch_high_water_bytes),
            retained_roots_validated: delta!(retained_roots_validated),
        })
    }

    fn verify_common(self) -> EvalResult<()> {
        if self.fetched_row_authentication_passes > self.fetched_rows {
            return Err("fetched authentication passes <= fetched rows".to_owned());
        }
        if self.fetched_rows != self.fetched_row_role_decode_passes {
            return Err("fetched_rows = fetched_row_role_decode_passes".to_owned());
        }
        if self.new_object_authentication_passes
            != self
                .created_rows
                .checked_add(self.reused_rows)
                .ok_or_else(|| "new-object equation overflow".to_owned())?
            || self.new_object_authentication_passes != self.put_lookup_statements
        {
            return Err(
                "new_object_authentication_passes = created_rows + reused_rows = put_lookup_statements"
                    .to_owned(),
            );
        }
        if self.incumbent_authentication_passes != self.reused_rows {
            return Err("incumbent_authentication_passes = reused_rows".to_owned());
        }
        if self.put_insert_statements != self.created_rows
            || self.objects_created != self.created_rows
            || self.objects_reused != self.reused_rows
        {
            return Err("put/created/reused row equations".to_owned());
        }
        let expected_validated = self
            .fetched_row_role_decode_passes
            .checked_add(self.new_object_authentication_passes)
            .and_then(|value| value.checked_add(self.incumbent_authentication_passes))
            .ok_or_else(|| "objects_validated equation overflow".to_owned())?;
        if self.objects_validated != expected_validated {
            return Err(
                "objects_validated = fetched role decode + new auth + incumbent auth".to_owned(),
            );
        }
        if self.payload_batch_maximum > 64 {
            return Err("payload_batch_maximum <= 64".to_owned());
        }
        if self.admission_transactions_started
            != self
                .admission_transactions_committed
                .checked_add(self.admission_transactions_rolled_back)
                .ok_or_else(|| "admission transaction equation overflow".to_owned())?
            || self.integrity_transactions_started
                != self
                    .integrity_transactions_committed
                    .checked_add(self.integrity_transactions_rolled_back)
                    .ok_or_else(|| "integrity transaction equation overflow".to_owned())?
            || self.publication_transactions_started
                != self
                    .publication_commits
                    .checked_add(self.publication_transactions_rolled_back)
                    .ok_or_else(|| "publication transaction equation overflow".to_owned())?
        {
            return Err("admission/integrity/publication transaction closure".to_owned());
        }
        if self.object_bytes_written != self.logical_object_bytes
            || self.range_bytes_requested != self.range_bytes_returned
        {
            return Err("phase storage/range byte equations".to_owned());
        }
        Ok(())
    }

    fn verify_verified(self) -> EvalResult<()> {
        self.verify_common()?;
        if self.fetched_rows != self.fetched_row_authentication_passes {
            return Err("Verified fetched_rows = fetched_row_authentication_passes".to_owned());
        }
        Ok(())
    }

    fn verify_trusted(self) -> EvalResult<()> {
        self.verify_common()
    }

    fn verify_trusted_transition(self) -> EvalResult<()> {
        self.verify_trusted()?;
        self.verify_transition_work()
    }

    fn verify_transition_work(self) -> EvalResult<()> {
        if self.transactions_started != 1
            || self.transactions_committed != 1
            || self.transactions_rolled_back != 0
            || self.publication_transactions_started != 1
            || self.publication_transactions_rolled_back != 0
            || self.publication_commits != 1
        {
            return Err(
                "one writer transaction and one publication COMMIT per transition".to_owned(),
            );
        }
        Ok(())
    }

    fn verify_read_only(self) -> EvalResult<()> {
        self.verify_verified()?;
        self.verify_read_only_work()
    }

    fn verify_trusted_read_only(self) -> EvalResult<()> {
        self.verify_trusted()?;
        if self.fetched_row_authentication_passes != 0 {
            return Err("Trusted read-only fetched_row_authentication_passes = 0".to_owned());
        }
        self.verify_read_only_work()
    }

    fn verify_read_only_work(self) -> EvalResult<()> {
        if self.transactions_started != 0
            || self.transactions_committed != 0
            || self.transactions_rolled_back != 0
            || self.publication_transactions_started != 0
            || self.publication_transactions_rolled_back != 0
            || self.publication_commits != 0
            || self.object_bytes_written != 0
            || self.logical_object_bytes != 0
            || self.logical_root_bytes != 0
            || self.logical_delta_bytes != 0
        {
            return Err("historical read/reconstruction has zero writer work".to_owned());
        }
        Ok(())
    }

    fn combine(mut self, source: Self) -> EvalResult<Self> {
        macro_rules! add {
            ($field:ident) => {
                self.$field = self
                    .$field
                    .checked_add(source.$field)
                    .ok_or_else(|| format!("phase counter {} overflow", stringify!($field)))?;
            };
        }
        add!(transactions_started);
        add!(transactions_committed);
        add!(transactions_rolled_back);
        add!(statements);
        add!(admission_transactions_started);
        add!(admission_transactions_committed);
        add!(admission_transactions_rolled_back);
        add!(admission_statements);
        add!(integrity_transactions_started);
        add!(integrity_transactions_committed);
        add!(integrity_transactions_rolled_back);
        add!(integrity_statements);
        add!(busy_events);
        add!(locked_events);
        add!(objects_validated);
        add!(objects_created);
        add!(objects_reused);
        add!(object_bytes_read);
        add!(object_bytes_written);
        add!(range_bytes_requested);
        add!(range_bytes_returned);
        add!(logical_object_bytes);
        add!(logical_root_bytes);
        add!(logical_delta_bytes);
        add!(retained_union_scrubs);
        add!(root_verifications);
        add!(root_verification_objects);
        add!(root_verification_bytes);
        add!(fetched_rows);
        add!(fetched_row_authentication_passes);
        add!(fetched_row_role_decode_passes);
        add!(new_object_authentication_passes);
        add!(incumbent_authentication_passes);
        add!(payload_batch_queries);
        add!(payload_batch_references);
        self.payload_batch_maximum = self.payload_batch_maximum.max(source.payload_batch_maximum);
        add!(put_lookup_statements);
        add!(put_insert_statements);
        add!(created_rows);
        add!(reused_rows);
        add!(publication_transactions_started);
        add!(publication_transactions_rolled_back);
        add!(publication_commits);
        add!(publication_closure_passes);
        add!(namespace_graph_verification_passes);
        add!(scratch_tables);
        add!(scratch_statements);
        add!(scratch_rows);
        self.scratch_high_water_bytes = self
            .scratch_high_water_bytes
            .max(source.scratch_high_water_bytes);
        add!(retained_roots_validated);
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhaseCounterDelta {
    name: &'static str,
    engine: EngineDelta,
    q_before_bytes: u64,
    q_after_bytes: u64,
    q_high_water_bytes: u64,
    active_connections: u64,
    operation_scratch_tables: u64,
    operation_scratch_statements: u64,
    operation_scratch_rows: u64,
    operation_scratch_high_water_bytes: u64,
}

impl PhaseCounterDelta {
    fn between(name: &'static str, before: &Diagnostics, after: &Diagnostics) -> EvalResult<Self> {
        let engine = EngineDelta::between(before, after)?;
        engine.verify_common()?;
        if before.operation_q_current_bytes != 0
            || after.operation_q_current_bytes != 0
            || after.operation_q_high_water_bytes > 8_388_608
        {
            return Err(format!("{name} phase Q closure"));
        }
        Ok(Self {
            name,
            engine,
            q_before_bytes: before.operation_q_current_bytes,
            q_after_bytes: after.operation_q_current_bytes,
            q_high_water_bytes: after.operation_q_high_water_bytes,
            active_connections: after.active_connections,
            operation_scratch_tables: 0,
            operation_scratch_statements: 0,
            operation_scratch_rows: 0,
            operation_scratch_high_water_bytes: 0,
        })
    }

    fn with_operation_scratch(mut self, operation: &layerfs_sdk::OperationDiagnostics) -> Self {
        self.operation_scratch_tables = operation.scratch_tables;
        self.operation_scratch_statements = operation.scratch_statements;
        self.operation_scratch_rows = operation.scratch_rows;
        self.operation_scratch_high_water_bytes = operation.scratch_high_water_bytes;
        self
    }

    fn operation_only(
        name: &'static str,
        operation: &layerfs_sdk::OperationDiagnostics,
        active_connections: u64,
    ) -> Self {
        Self {
            name,
            engine: EngineDelta::default(),
            q_before_bytes: 0,
            q_after_bytes: operation.operation_q_terminal_bytes,
            q_high_water_bytes: operation.operation_q_high_water_bytes,
            active_connections,
            operation_scratch_tables: operation.scratch_tables,
            operation_scratch_statements: operation.scratch_statements,
            operation_scratch_rows: operation.scratch_rows,
            operation_scratch_high_water_bytes: operation.scratch_high_water_bytes,
        }
    }
}

fn verify_phase_partition(phases: &[PhaseCounterDelta], aggregate: EngineDelta) -> EvalResult<()> {
    let combined = phases
        .iter()
        .try_fold(EngineDelta::default(), |total, phase| {
            total.combine(phase.engine)
        })?;
    if combined != aggregate {
        return Err(format!(
            "phase engine deltas do not sum to retained aggregate: phases={combined:?} aggregate={aggregate:?}"
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
struct ContentCounters {
    cdc_bytes_scanned: u64,
    payload_bytes_written: u64,
    unaffected_payload_reads: u64,
    unaffected_payload_writes: u64,
    rope_nodes_read: u64,
    rope_nodes_emitted: u64,
    content_directory_nodes_emitted: u64,
}

fn content_counters(operation: &layerfs_sdk::OperationDiagnostics) -> EvalResult<ContentCounters> {
    let payload_bytes_read = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "metadata payload reads exceed aggregate reads".to_owned())?;
    let payload_bytes_written = operation
        .content_payload_bytes_written()
        .ok_or_else(|| "metadata payload writes exceed aggregate writes".to_owned())?;
    let cdc_bytes_scanned = operation
        .rope
        .cdc_bytes_scanned
        .checked_sub(operation.metadata_rope.cdc_bytes_scanned)
        .ok_or_else(|| "metadata CDC exceeds aggregate CDC".to_owned())?;
    Ok(ContentCounters {
        cdc_bytes_scanned,
        payload_bytes_written,
        unaffected_payload_reads: payload_bytes_read,
        unaffected_payload_writes: payload_bytes_written
            .checked_sub(cdc_bytes_scanned)
            .ok_or_else(|| "content payload writes are below content CDC input".to_owned())?,
        rope_nodes_read: operation
            .rope
            .nodes_read
            .checked_sub(operation.metadata_rope.nodes_read)
            .ok_or_else(|| "metadata rope reads exceed aggregate reads".to_owned())?,
        rope_nodes_emitted: operation
            .rope
            .nodes_created
            .checked_sub(operation.metadata_rope.nodes_created)
            .ok_or_else(|| "metadata rope emissions exceed aggregate emissions".to_owned())?,
        content_directory_nodes_emitted: operation.namespace.nodes_created,
    })
}

fn verify_locality(
    operation: &layerfs_sdk::OperationDiagnostics,
    replacement_bytes: u64,
    tree_level: u8,
) -> EvalResult<ContentCounters> {
    let counters = content_counters(operation)?;
    let read_bound = 16_u64
        .checked_mul(u64::from(tree_level) + 1)
        .ok_or_else(|| "rope read bound overflow".to_owned())?;
    let emitted_bound = read_bound
        .checked_add(replacement_bytes.div_ceil(8_192))
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| "rope emission bound overflow".to_owned())?;
    if counters.cdc_bytes_scanned != replacement_bytes
        || counters.payload_bytes_written != replacement_bytes
        || counters.unaffected_payload_reads != 0
        || counters.unaffected_payload_writes != 0
        || counters.content_directory_nodes_emitted != 0
        || counters.rope_nodes_read > read_bound
        || counters.rope_nodes_emitted > emitted_bound
    {
        return Err(format!(
            "locality equation failed: replacement={replacement_bytes} H={tree_level} counters={counters:?} read_bound={read_bound} emitted_bound={emitted_bound}"
        ));
    }
    Ok(counters)
}

fn verify_burst_locality(
    operation: &layerfs_sdk::OperationDiagnostics,
    edits: &[EditSpec],
    steps: &[layerfs_sdk::ManagedReplayStep],
) -> EvalResult<ContentCounters> {
    let replacement_bytes = edits.iter().try_fold(0_u64, |total, edit| {
        total
            .checked_add(edit.insert_bytes)
            .ok_or_else(|| "burst replacement bytes overflow".to_owned())
    })?;
    let counters = content_counters(operation)?;
    if steps.len() != edits.len() {
        return Err("burst aggregate step count".to_owned());
    }
    let mut exact = ContentCounters::default();
    for (edit, step) in edits.iter().zip(steps) {
        let tree_level = step
            .tree_level_before
            .ok_or_else(|| format!("{} missing actual H", edit.tag))?;
        let one = verify_locality(&step.counters, edit.insert_bytes, tree_level)?;
        exact.cdc_bytes_scanned = exact
            .cdc_bytes_scanned
            .checked_add(one.cdc_bytes_scanned)
            .ok_or_else(|| "burst exact CDC sum overflow".to_owned())?;
        exact.payload_bytes_written = exact
            .payload_bytes_written
            .checked_add(one.payload_bytes_written)
            .ok_or_else(|| "burst exact payload sum overflow".to_owned())?;
        exact.unaffected_payload_reads = exact
            .unaffected_payload_reads
            .checked_add(one.unaffected_payload_reads)
            .ok_or_else(|| "burst exact unaffected-read sum overflow".to_owned())?;
        exact.unaffected_payload_writes = exact
            .unaffected_payload_writes
            .checked_add(one.unaffected_payload_writes)
            .ok_or_else(|| "burst exact unaffected-write sum overflow".to_owned())?;
        exact.rope_nodes_read = exact
            .rope_nodes_read
            .checked_add(one.rope_nodes_read)
            .ok_or_else(|| "burst exact node-read sum overflow".to_owned())?;
        exact.rope_nodes_emitted = exact
            .rope_nodes_emitted
            .checked_add(one.rope_nodes_emitted)
            .ok_or_else(|| "burst exact node-emission sum overflow".to_owned())?;
        exact.content_directory_nodes_emitted = exact
            .content_directory_nodes_emitted
            .checked_add(one.content_directory_nodes_emitted)
            .ok_or_else(|| "burst exact directory-node sum overflow".to_owned())?;
    }
    if counters.cdc_bytes_scanned != replacement_bytes
        || counters.payload_bytes_written != replacement_bytes
        || counters.unaffected_payload_reads != 0
        || counters.unaffected_payload_writes != 0
        || counters.content_directory_nodes_emitted != 0
        || counters.cdc_bytes_scanned != exact.cdc_bytes_scanned
        || counters.payload_bytes_written != exact.payload_bytes_written
        || counters.unaffected_payload_reads != exact.unaffected_payload_reads
        || counters.unaffected_payload_writes != exact.unaffected_payload_writes
        || counters.rope_nodes_read != exact.rope_nodes_read
        || counters.rope_nodes_emitted != exact.rope_nodes_emitted
        || counters.content_directory_nodes_emitted != exact.content_directory_nodes_emitted
    {
        return Err(format!(
            "burst locality exact aggregate failed: replacement={replacement_bytes} edits={} aggregate={counters:?} exact_steps={exact:?}",
            edits.len(),
        ));
    }
    Ok(counters)
}

#[derive(Clone, Debug)]
struct Phase {
    name: &'static str,
    wall_ns: u128,
}

#[derive(Clone, Debug)]
struct Unavailable {
    field: String,
    availability: &'static str,
    reason: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct ResourceObservation {
    rss_current_bytes: Option<u64>,
    rss_peak_bytes: u64,
    fd_current: u64,
    active_store_connections: u64,
    child_processes: u64,
    owned_temp_entries: Option<u64>,
    residue_entries: u64,
}

#[derive(Clone, Debug)]
struct OracleReceipt {
    logical_length: u64,
    content_digest: String,
    physical_bytes_exact: Option<bool>,
    canonical_bytes_exact: Option<bool>,
    metadata_exact: Option<bool>,
    historical_roots_exact: Option<bool>,
    route_exact: Option<bool>,
}

impl Default for OracleReceipt {
    fn default() -> Self {
        Self {
            logical_length: INITIAL_BYTES,
            content_digest: String::new(),
            physical_bytes_exact: None,
            canonical_bytes_exact: None,
            metadata_exact: None,
            historical_roots_exact: None,
            route_exact: None,
        }
    }
}

#[derive(Clone, Debug)]
struct SubEditReceipt {
    edit: EditSpec,
    native_wall_ns: u128,
    physical_oracle_wall_ns: u128,
    native_route: String,
    native_bytes_read: u64,
    native_bytes_written: u64,
    native_patch_bytes: u64,
    native_suffix_bytes_shifted: u64,
    native_clone_attempts: u64,
    native_clone_successes: u64,
    native_clone_fallbacks: u64,
    native_full_fallback_files: u64,
    tree_level_before: Option<u8>,
    locality: Option<ContentCounters>,
}

#[derive(Clone, Debug)]
struct HistoryProbeReceipt {
    root_index: usize,
    ordinal: u8,
    start: u64,
    length: u64,
    wall_ns: u128,
    engine: EngineDelta,
    operation: layerfs_sdk::OperationDiagnostics,
}

#[derive(Clone, Debug)]
struct RowReceipt {
    schedule: ScheduledRow,
    status: &'static str,
    before_bytes: u64,
    after_bytes: u64,
    edit: Option<EditSpec>,
    sub_edits: Vec<SubEditReceipt>,
    history_probes: Vec<HistoryProbeReceipt>,
    pre_ref: Option<RefState>,
    post_ref: Option<RefState>,
    native_route: String,
    tree_level_before: Option<u8>,
    phases: Vec<Phase>,
    phase_counters: Vec<PhaseCounterDelta>,
    row_wall_ns: u128,
    row_residual_ns: u128,
    engine: Option<EngineDelta>,
    operation: Option<layerfs_sdk::OperationDiagnostics>,
    storage_before: Option<Diagnostics>,
    storage_after: Option<Diagnostics>,
    resources: ResourceObservation,
    oracle: OracleReceipt,
    unavailable: Vec<Unavailable>,
    error: Option<(String, String, String, String, Option<String>)>,
    custody: Option<String>,
}

impl RowReceipt {
    fn json(&self) -> EvalResult<String> {
        let edit = self
            .edit
            .as_ref()
            .map(edit_json)
            .transpose()?
            .unwrap_or_else(|| "null".to_owned());
        let sub_edits = self
            .sub_edits
            .iter()
            .map(sub_edit_json)
            .collect::<EvalResult<Vec<_>>>()?
            .join(",");
        let history_probes = self
            .history_probes
            .iter()
            .map(history_probe_json)
            .collect::<EvalResult<Vec<_>>>()?
            .join(",");
        let phases = self
            .phases
            .iter()
            .map(|phase| {
                format!(
                    "{{\"name\":\"{}\",\"wall_ns\":{}}}",
                    phase.name, phase.wall_ns
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let phase_counters = self
            .phase_counters
            .iter()
            .map(phase_counter_json)
            .collect::<Vec<_>>()
            .join(",");
        let mut unavailable_values = self.unavailable.clone();
        if self.engine.is_none() && self.operation.is_none() {
            for field in [
                "counters.transactions_started",
                "counters.transactions_committed",
                "counters.transactions_rolled_back",
                "counters.statements",
                "counters.admission_transactions_started",
                "counters.admission_transactions_committed",
                "counters.admission_transactions_rolled_back",
                "counters.admission_statements",
                "counters.integrity_transactions_started",
                "counters.integrity_transactions_committed",
                "counters.integrity_transactions_rolled_back",
                "counters.integrity_statements",
                "counters.busy_events",
                "counters.locked_events",
                "counters.objects_validated",
                "counters.objects_created",
                "counters.objects_reused",
                "counters.object_bytes_read",
                "counters.object_bytes_written",
                "counters.fetched_rows",
                "counters.fetched_row_authentication_passes",
                "counters.fetched_row_role_decode_passes",
                "counters.new_object_authentication_passes",
                "counters.incumbent_authentication_passes",
                "counters.payload_batch_queries",
                "counters.payload_batch_references",
                "counters.payload_batch_maximum",
                "counters.put_lookup_statements",
                "counters.put_insert_statements",
                "counters.created_rows",
                "counters.reused_rows",
                "counters.publication_transactions_started",
                "counters.publication_transactions_rolled_back",
                "counters.publication_commits",
                "counters.publication_closure_passes",
                "counters.namespace_graph_verification_passes",
                "counters.scratch_tables",
                "counters.scratch_statements",
                "counters.scratch_rows",
                "counters.scratch_high_water_bytes",
                "counters.retained_roots_validated",
                "counters.cdc_bytes_scanned",
                "counters.payload_bytes_written",
                "counters.unaffected_payload_reads",
                "counters.unaffected_payload_writes",
                "counters.rope_nodes_read",
                "counters.rope_nodes_emitted",
                "counters.content_directory_nodes_emitted",
                "counters.workspace_materializations",
                "counters.workspace_reuses",
                "counters.rematerializations",
                "counters.descriptor_resets",
                "native.bytes_read",
                "native.bytes_written",
                "native.patch_bytes",
                "native.suffix_bytes_shifted",
                "native.clone_attempts",
                "native.clone_successes",
                "native.clone_fallbacks",
                "native.full_fallback_files",
                "native.files_created",
                "native.files_replaced",
                "native.files_removed",
                "resources.operation_q_current_bytes",
                "resources.operation_q_high_water_bytes",
                "resources.operation_q_terminal_bytes",
            ] {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "row has no product operation".to_owned(),
                });
            }
        }
        if self.storage_after.is_none() {
            for field in [
                "storage.database_bytes",
                "storage.logical_engine_bytes",
                "storage.database_growth_bytes",
                "storage.canonical_object_bytes_written",
                "storage.physical_to_canonical_amplification",
            ] {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "row has no Store storage observation".to_owned(),
                });
            }
        } else if self
            .storage_before
            .as_ref()
            .zip(self.storage_after.as_ref())
            .is_some_and(|(before, after)| {
                after.object_bytes_written == before.object_bytes_written
            })
        {
            unavailable_values.push(Unavailable {
                field: "storage.physical_to_canonical_amplification".to_owned(),
                availability: "NotApplicable",
                reason: "row wrote no canonical object bytes".to_owned(),
            });
        }
        for (field, value) in [
            (
                "oracle.physical_bytes_exact",
                self.oracle.physical_bytes_exact,
            ),
            (
                "oracle.canonical_bytes_exact",
                self.oracle.canonical_bytes_exact,
            ),
            ("oracle.metadata_exact", self.oracle.metadata_exact),
            (
                "oracle.historical_roots_exact",
                self.oracle.historical_roots_exact,
            ),
            ("oracle.route_exact", self.oracle.route_exact),
        ] {
            if value.is_none() {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "oracle is not applicable to this scheduled row".to_owned(),
                });
            }
        }
        if self.tree_level_before.is_none() {
            unavailable_values.push(Unavailable {
                field: "tree_level_before".to_owned(),
                availability: "NotApplicable",
                reason: "row is not one individual canonical content edit".to_owned(),
            });
        }
        if self.resources.rss_current_bytes.is_none() {
            unavailable_values.push(Unavailable {
                field: "resources.rss_current_bytes".to_owned(),
                availability: "Unavailable",
                reason: "per-row observer uses getrusage peak; current RSS is sampled only by decisive external observers".to_owned(),
            });
        }
        if self.operation.is_none() && self.resources.owned_temp_entries.is_none() {
            unavailable_values.push(Unavailable {
                field: "resources.owned_temp_entries".to_owned(),
                availability: "NotApplicable",
                reason: "row has no product operation or terminal owned-residue observation"
                    .to_owned(),
            });
        }
        let unavailable = unavailable_values
            .iter()
            .map(|value| {
                format!(
                    "{{\"field\":\"{}\",\"availability\":\"{}\",\"reason\":\"{}\"}}",
                    json_escape(&value.field),
                    value.availability,
                    json_escape(&value.reason)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let error = self.error.as_ref().map_or_else(
            || "null".to_owned(),
            |(class, message, phase, equation, stderr_sha256)| {
                format!(
                    concat!(
                        "{{\"class\":\"{}\",\"message\":\"{}\",",
                        "\"phase\":\"{}\",\"first_failed_equation\":\"{}\",",
                        "\"stderr_sha256\":{}}}"
                    ),
                    json_escape(class),
                    json_escape(message),
                    json_escape(phase),
                    json_escape(equation),
                    stderr_sha256.as_ref().map_or_else(
                        || "null".to_owned(),
                        |value| format!("\"{}\"", json_escape(value)),
                    ),
                )
            },
        );
        let custody = self
            .custody
            .as_ref()
            .map_or_else(|| "".to_owned(), |value| format!(",\"custody\":{value}"));
        Ok(format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1.1-row-v1\",\"row_index\":{},",
                "\"row_id\":\"{}\",\"row_group\":\"{}\",\"sequence\":{},",
                "\"epoch\":{},\"direction\":\"{}\",\"operation\":\"{}\",",
                "\"size_band\":\"{}\",\"status\":\"{}\",",
                "\"before_bytes\":{},\"after_bytes\":{},\"edit\":{},",
                "\"sub_edits\":[{}],\"history_probes\":[{}],",
                "\"pre_ref\":{},\"post_ref\":{},",
                "\"native_route\":\"{}\",\"tree_level_before\":{},\"phases\":[{}],",
                "\"phase_counters\":[{}],",
                "\"row_wall_ns\":{},\"row_residual_ns\":{},",
                "\"counters\":{},\"native\":{},\"storage\":{},",
                "\"resources\":{},\"oracle\":{},\"unavailable\":[{}],",
                "\"error\":{}{} }}\n"
            ),
            self.schedule.row_index,
            self.schedule.row_id,
            self.schedule.row_group,
            self.schedule.sequence,
            self.schedule.epoch,
            self.schedule.direction,
            self.schedule.operation,
            self.schedule.size_band,
            self.status,
            self.before_bytes,
            self.after_bytes,
            edit,
            sub_edits,
            history_probes,
            ref_json(self.pre_ref.as_ref()),
            ref_json(self.post_ref.as_ref()),
            self.native_route,
            self.tree_level_before
                .map_or_else(|| "null".to_owned(), |value| value.to_string()),
            phases,
            phase_counters,
            self.row_wall_ns,
            self.row_residual_ns,
            counters_json(self.engine, self.operation.as_ref())?,
            native_json(self.operation.as_ref()),
            storage_json(self.storage_before.as_ref(), self.storage_after.as_ref()),
            resources_json(&self.resources, self.operation.as_ref()),
            oracle_json(&self.oracle),
            unavailable,
            error,
            custody,
        ))
    }
}

fn history_probe_json(probe: &HistoryProbeReceipt) -> EvalResult<String> {
    let content = content_counters(&probe.operation)?;
    let non_payload_rows = probe
        .engine
        .fetched_rows
        .checked_sub(probe.engine.payload_batch_references)
        .ok_or_else(|| "history probe payload rows exceed fetched rows".to_owned())?;
    let non_payload_statements = probe
        .engine
        .statements
        .checked_sub(probe.engine.payload_batch_queries)
        .ok_or_else(|| "history probe payload queries exceed statements".to_owned())?;
    Ok(format!(
        concat!(
            "{{\"root\":\"R{}\",\"ordinal\":{},\"start\":{},\"length\":{},",
            "\"wall_ns\":{},\"namespace_nodes_read\":{},",
            "\"inode_table_nodes_read\":{},\"rope_nodes_read\":{},",
            "\"payload_bytes_read\":{},\"payload_batch_queries\":{},",
            "\"payload_batch_references\":{},\"non_payload_statements\":{},",
            "\"non_payload_rows\":{},\"fetched_rows\":{},",
            "\"authentication_passes\":{},\"role_decode_passes\":{},",
            "\"engine_counters\":{}}}"
        ),
        probe.root_index,
        probe.ordinal,
        probe.start,
        probe.length,
        probe.wall_ns,
        probe.operation.namespace.nodes_read,
        probe.operation.inode_table.nodes_read,
        content.rope_nodes_read,
        probe.operation.rope.payload_bytes_read,
        probe.engine.payload_batch_queries,
        probe.engine.payload_batch_references,
        non_payload_statements,
        non_payload_rows,
        probe.engine.fetched_rows,
        probe.engine.fetched_row_authentication_passes,
        probe.engine.fetched_row_role_decode_passes,
        counters_json(Some(probe.engine), Some(&probe.operation))?,
    ))
}

fn phase_counter_json(phase: &PhaseCounterDelta) -> String {
    let value = phase.engine;
    format!(
        concat!(
            "{{\"name\":\"{}\",\"transactions_started\":{},",
            "\"transactions_committed\":{},\"transactions_rolled_back\":{},",
            "\"statements\":{},\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"busy_events\":{},\"locked_events\":{},",
            "\"objects_validated\":{},\"objects_created\":{},\"objects_reused\":{},",
            "\"object_bytes_read\":{},\"object_bytes_written\":{},",
            "\"range_bytes_requested\":{},\"range_bytes_returned\":{},",
            "\"logical_object_bytes\":{},\"logical_root_bytes\":{},",
            "\"logical_delta_bytes\":{},\"retained_union_scrubs\":{},",
            "\"root_verifications\":{},\"root_verification_objects\":{},",
            "\"root_verification_bytes\":{},\"fetched_rows\":{},",
            "\"fetched_row_authentication_passes\":{},",
            "\"fetched_row_role_decode_passes\":{},",
            "\"new_object_authentication_passes\":{},",
            "\"incumbent_authentication_passes\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"put_lookup_statements\":{},",
            "\"put_insert_statements\":{},\"created_rows\":{},\"reused_rows\":{},",
            "\"publication_transactions_started\":{},",
            "\"publication_transactions_rolled_back\":{},",
            "\"publication_commits\":{},\"publication_closure_passes\":{},",
            "\"namespace_graph_verification_passes\":{},\"scratch_tables\":{},",
            "\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_high_water_bytes\":{},\"retained_roots_validated\":{},",
            "\"q_before_bytes\":{},",
            "\"q_after_bytes\":{},\"q_high_water_bytes\":{},",
            "\"active_connections\":{},\"operation_scratch_tables\":{},",
            "\"operation_scratch_statements\":{},\"operation_scratch_rows\":{},",
            "\"operation_scratch_high_water_bytes\":{}}}"
        ),
        phase.name,
        value.transactions_started,
        value.transactions_committed,
        value.transactions_rolled_back,
        value.statements,
        value.admission_transactions_started,
        value.admission_transactions_committed,
        value.admission_transactions_rolled_back,
        value.admission_statements,
        value.integrity_transactions_started,
        value.integrity_transactions_committed,
        value.integrity_transactions_rolled_back,
        value.integrity_statements,
        value.busy_events,
        value.locked_events,
        value.objects_validated,
        value.objects_created,
        value.objects_reused,
        value.object_bytes_read,
        value.object_bytes_written,
        value.range_bytes_requested,
        value.range_bytes_returned,
        value.logical_object_bytes,
        value.logical_root_bytes,
        value.logical_delta_bytes,
        value.retained_union_scrubs,
        value.root_verifications,
        value.root_verification_objects,
        value.root_verification_bytes,
        value.fetched_rows,
        value.fetched_row_authentication_passes,
        value.fetched_row_role_decode_passes,
        value.new_object_authentication_passes,
        value.incumbent_authentication_passes,
        value.payload_batch_queries,
        value.payload_batch_references,
        value.payload_batch_maximum,
        value.put_lookup_statements,
        value.put_insert_statements,
        value.created_rows,
        value.reused_rows,
        value.publication_transactions_started,
        value.publication_transactions_rolled_back,
        value.publication_commits,
        value.publication_closure_passes,
        value.namespace_graph_verification_passes,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_high_water_bytes,
        value.retained_roots_validated,
        phase.q_before_bytes,
        phase.q_after_bytes,
        phase.q_high_water_bytes,
        phase.active_connections,
        phase.operation_scratch_tables,
        phase.operation_scratch_statements,
        phase.operation_scratch_rows,
        phase.operation_scratch_high_water_bytes,
    )
}

fn edit_json(edit: &EditSpec) -> EvalResult<String> {
    let bytes = replacement_bytes(
        edit.serial,
        usize::try_from(edit.insert_bytes).map_err(display_error)?,
    );
    Ok(format!(
        concat!(
            "{{\"tag\":\"{}\",\"offset\":{},\"delete_bytes\":{},",
            "\"insert_bytes\":{},\"replacement_digest\":\"{}\"}}"
        ),
        edit.tag,
        edit.offset,
        edit.delete_bytes,
        edit.insert_bytes,
        blake3::hash(&bytes).to_hex(),
    ))
}

fn sub_edit_json(receipt: &SubEditReceipt) -> EvalResult<String> {
    let replacement = replacement_bytes(
        receipt.edit.serial,
        usize::try_from(receipt.edit.insert_bytes).map_err(display_error)?,
    );
    Ok(format!(
        concat!(
            "{{\"tag\":\"{}\",\"offset\":{},\"delete_bytes\":{},",
            "\"insert_bytes\":{},\"replacement_digest\":\"{}\",",
            "\"before_bytes\":{},\"after_bytes\":{},",
            "\"native_wall_ns\":{},\"physical_oracle_wall_ns\":{},",
            "\"native_route\":\"{}\",\"native_bytes_read\":{},",
            "\"native_bytes_written\":{},\"native_patch_bytes\":{},",
            "\"native_suffix_bytes_shifted\":{},\"native_clone_attempts\":{},",
            "\"native_clone_successes\":{},\"native_clone_fallbacks\":{},",
            "\"native_full_fallback_files\":{},\"tree_level_before\":{},",
            "\"cdc_bytes_scanned\":{},\"payload_bytes_written\":{},",
            "\"unaffected_payload_reads\":{},\"unaffected_payload_writes\":{},",
            "\"rope_nodes_read\":{},\"rope_nodes_emitted\":{},",
            "\"content_directory_nodes_emitted\":{}}}"
        ),
        receipt.edit.tag,
        receipt.edit.offset,
        receipt.edit.delete_bytes,
        receipt.edit.insert_bytes,
        blake3::hash(&replacement).to_hex(),
        receipt.edit.before_bytes,
        receipt.edit.after_bytes,
        receipt.native_wall_ns,
        receipt.physical_oracle_wall_ns,
        receipt.native_route,
        receipt.native_bytes_read,
        receipt.native_bytes_written,
        receipt.native_patch_bytes,
        receipt.native_suffix_bytes_shifted,
        receipt.native_clone_attempts,
        receipt.native_clone_successes,
        receipt.native_clone_fallbacks,
        receipt.native_full_fallback_files,
        receipt
            .tree_level_before
            .map_or_else(|| "null".to_owned(), |value| value.to_string()),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.cdc_bytes_scanned.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.payload_bytes_written.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.unaffected_payload_reads.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.unaffected_payload_writes.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.rope_nodes_read.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.rope_nodes_emitted.to_string()
        ),
        receipt.locality.map_or_else(
            || "null".to_owned(),
            |value| value.content_directory_nodes_emitted.to_string()
        ),
    ))
}

fn ref_json(reference: Option<&RefState>) -> String {
    reference.map_or_else(
        || "null".to_owned(),
        |value| {
            format!(
                "{{\"name\":\"{}\",\"generation\":{},\"root\":\"{}\"}}",
                json_escape(&value.name),
                value.generation,
                value.root
            )
        },
    )
}

fn counters_json(
    engine: Option<EngineDelta>,
    operation: Option<&layerfs_sdk::OperationDiagnostics>,
) -> EvalResult<String> {
    if engine.is_none() && operation.is_none() {
        return Ok(format!(
            "{{{}}}",
            [
                "transactions_started",
                "transactions_committed",
                "transactions_rolled_back",
                "statements",
                "admission_transactions_started",
                "admission_transactions_committed",
                "admission_transactions_rolled_back",
                "admission_statements",
                "integrity_transactions_started",
                "integrity_transactions_committed",
                "integrity_transactions_rolled_back",
                "integrity_statements",
                "busy_events",
                "locked_events",
                "objects_validated",
                "objects_created",
                "objects_reused",
                "object_bytes_read",
                "object_bytes_written",
                "fetched_rows",
                "fetched_row_authentication_passes",
                "fetched_row_role_decode_passes",
                "new_object_authentication_passes",
                "incumbent_authentication_passes",
                "payload_batch_queries",
                "payload_batch_references",
                "payload_batch_maximum",
                "put_lookup_statements",
                "put_insert_statements",
                "created_rows",
                "reused_rows",
                "publication_transactions_started",
                "publication_transactions_rolled_back",
                "publication_commits",
                "publication_closure_passes",
                "namespace_graph_verification_passes",
                "scratch_tables",
                "scratch_statements",
                "scratch_rows",
                "scratch_high_water_bytes",
                "retained_roots_validated",
                "cdc_bytes_scanned",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "rope_nodes_read",
                "rope_nodes_emitted",
                "content_directory_nodes_emitted",
                "workspace_materializations",
                "workspace_reuses",
                "rematerializations",
                "descriptor_resets",
            ]
            .into_iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
        ));
    }
    let e = engine.unwrap_or_default();
    let c = operation
        .map(content_counters)
        .transpose()?
        .unwrap_or_default();
    let o = operation.copied().unwrap_or_default();
    let (scratch_tables, scratch_statements, scratch_rows, scratch_high_water_bytes) =
        joined_scratch_counts(e, o)?;
    Ok(format!(
        concat!(
            "{{\"transactions_started\":{},\"transactions_committed\":{},",
            "\"transactions_rolled_back\":{},\"statements\":{},",
            "\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"busy_events\":{},\"locked_events\":{},\"objects_validated\":{},",
            "\"objects_created\":{},\"objects_reused\":{},",
            "\"object_bytes_read\":{},\"object_bytes_written\":{},",
            "\"fetched_rows\":{},\"fetched_row_authentication_passes\":{},",
            "\"fetched_row_role_decode_passes\":{},",
            "\"new_object_authentication_passes\":{},",
            "\"incumbent_authentication_passes\":{},",
            "\"payload_batch_queries\":{},\"payload_batch_references\":{},",
            "\"payload_batch_maximum\":{},\"put_lookup_statements\":{},",
            "\"put_insert_statements\":{},\"created_rows\":{},\"reused_rows\":{},",
            "\"publication_transactions_started\":{},",
            "\"publication_transactions_rolled_back\":{},",
            "\"publication_commits\":{},\"publication_closure_passes\":{},",
            "\"namespace_graph_verification_passes\":{},\"scratch_tables\":{},",
            "\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_high_water_bytes\":{},\"retained_roots_validated\":{},",
            "\"cdc_bytes_scanned\":{},",
            "\"payload_bytes_written\":{},\"unaffected_payload_reads\":{},",
            "\"unaffected_payload_writes\":{},\"rope_nodes_read\":{},",
            "\"rope_nodes_emitted\":{},\"content_directory_nodes_emitted\":{},",
            "\"workspace_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{},\"descriptor_resets\":{}}}"
        ),
        e.transactions_started,
        e.transactions_committed,
        e.transactions_rolled_back,
        e.statements,
        e.admission_transactions_started,
        e.admission_transactions_committed,
        e.admission_transactions_rolled_back,
        e.admission_statements,
        e.integrity_transactions_started,
        e.integrity_transactions_committed,
        e.integrity_transactions_rolled_back,
        e.integrity_statements,
        e.busy_events,
        e.locked_events,
        e.objects_validated,
        e.objects_created,
        e.objects_reused,
        e.object_bytes_read,
        e.object_bytes_written,
        e.fetched_rows,
        e.fetched_row_authentication_passes,
        e.fetched_row_role_decode_passes,
        e.new_object_authentication_passes,
        e.incumbent_authentication_passes,
        e.payload_batch_queries,
        e.payload_batch_references,
        e.payload_batch_maximum,
        e.put_lookup_statements,
        e.put_insert_statements,
        e.created_rows,
        e.reused_rows,
        e.publication_transactions_started,
        e.publication_transactions_rolled_back,
        e.publication_commits,
        e.publication_closure_passes,
        e.namespace_graph_verification_passes,
        scratch_tables,
        scratch_statements,
        scratch_rows,
        scratch_high_water_bytes,
        e.retained_roots_validated,
        c.cdc_bytes_scanned,
        c.payload_bytes_written,
        c.unaffected_payload_reads,
        c.unaffected_payload_writes,
        c.rope_nodes_read,
        c.rope_nodes_emitted,
        c.content_directory_nodes_emitted,
        o.workspace_materializations,
        o.workspace_reuses,
        o.rematerializations,
        o.descriptor_resets,
    ))
}

fn joined_scratch_counts(
    engine: EngineDelta,
    operation: layerfs_sdk::OperationDiagnostics,
) -> EvalResult<(u64, u64, u64, u64)> {
    Ok((
        engine
            .scratch_tables
            .checked_add(operation.scratch_tables)
            .ok_or_else(|| "combined scratch tables overflow".to_owned())?,
        engine
            .scratch_statements
            .checked_add(operation.scratch_statements)
            .ok_or_else(|| "combined scratch statements overflow".to_owned())?,
        engine
            .scratch_rows
            .checked_add(operation.scratch_rows)
            .ok_or_else(|| "combined scratch rows overflow".to_owned())?,
        engine
            .scratch_high_water_bytes
            .max(operation.scratch_high_water_bytes),
    ))
}

fn native_json(operation: Option<&layerfs_sdk::OperationDiagnostics>) -> String {
    if operation.is_none() {
        return format!(
            "{{{}}}",
            [
                "bytes_read",
                "bytes_written",
                "patch_bytes",
                "suffix_bytes_shifted",
                "clone_attempts",
                "clone_successes",
                "clone_fallbacks",
                "full_fallback_files",
                "files_created",
                "files_replaced",
                "files_removed",
                "sync_regular_calls",
                "sync_directory_calls",
            ]
            .into_iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
        );
    }
    let value = operation.copied().unwrap_or_default();
    format!(
        concat!(
            "{{\"bytes_read\":{},\"bytes_written\":{},\"patch_bytes\":{},",
            "\"suffix_bytes_shifted\":{},\"clone_attempts\":{},",
            "\"clone_successes\":{},\"clone_fallbacks\":{},",
            "\"full_fallback_files\":{},\"files_created\":{},",
            "\"files_replaced\":{},\"files_removed\":{},",
            "\"sync_regular_calls\":null,\"sync_directory_calls\":null}}"
        ),
        value.native.bytes_read,
        value.native.bytes_written,
        value.native.patch_bytes,
        value.native.suffix_bytes_shifted,
        value.native.clone_attempts,
        value.native.clone_successes,
        value.native.clone_fallbacks,
        value.full_fallback_files,
        value.native.create_calls,
        value.native.replace_calls,
        value.native.remove_calls,
    )
}

fn storage_json(before: Option<&Diagnostics>, after: Option<&Diagnostics>) -> String {
    let database_before = before.and_then(|value| value.database_bytes);
    let database_after = after.and_then(|value| value.database_bytes);
    let engine_after = after.and_then(|value| value.logical_engine_bytes);
    let database_growth = database_before
        .zip(database_after)
        .and_then(|(before, after)| after.checked_sub(before));
    let canonical = before.zip(after).and_then(|(before, after)| {
        after
            .object_bytes_written
            .checked_sub(before.object_bytes_written)
    });
    let amplification = database_growth
        .zip(canonical)
        .and_then(|(database, canonical)| {
            (canonical != 0).then_some(database as f64 / canonical as f64)
        });
    format!(
        concat!(
            "{{\"database_bytes\":{},\"logical_engine_bytes\":{},",
            "\"rollback_journal_bytes\":null,\"temporary_file_bytes\":null,",
            "\"database_growth_bytes\":{},\"canonical_object_bytes_written\":{},",
            "\"physical_to_canonical_amplification\":{}}}"
        ),
        option_u64_json(database_after),
        option_u64_json(engine_after),
        option_u64_json(database_growth),
        option_u64_json(canonical),
        amplification.map_or_else(|| "null".to_owned(), |value| format!("{value:.9}")),
    )
}

fn resources_json(
    resources: &ResourceObservation,
    operation: Option<&layerfs_sdk::OperationDiagnostics>,
) -> String {
    let operation_value = operation.copied().unwrap_or_default();
    let q_current = operation.map(|_| operation_value.operation_q_current_bytes);
    let q_high = operation.map(|_| operation_value.operation_q_high_water_bytes);
    let q_terminal = operation.map(|_| operation_value.operation_q_terminal_bytes);
    let owned_temp = operation
        .map(|_| operation_value.owned_temp_current)
        .or(resources.owned_temp_entries);
    format!(
        concat!(
            "{{\"rss_current_bytes\":{},\"rss_peak_bytes\":{},",
            "\"operation_q_current_bytes\":{},\"operation_q_high_water_bytes\":{},",
            "\"operation_q_terminal_bytes\":{},\"fd_current\":{},",
            "\"active_store_connections\":{},\"child_processes\":{},",
            "\"owned_temp_entries\":{},\"residue_entries\":{},",
            "\"largest_buffer_bytes\":{},\"page_size\":4096,",
            "\"cache_pages\":1280,\"cache_spill_pages\":1280,",
            "\"network_operations\":0}}"
        ),
        option_u64_json(resources.rss_current_bytes),
        resources.rss_peak_bytes,
        option_u64_json(q_current),
        option_u64_json(q_high),
        option_u64_json(q_terminal),
        resources.fd_current,
        resources.active_store_connections,
        resources.child_processes,
        option_u64_json(owned_temp),
        resources.residue_entries,
        PRODUCT_BUFFER_BOUND_BYTES,
    )
}

fn oracle_json(oracle: &OracleReceipt) -> String {
    format!(
        concat!(
            "{{\"logical_length\":{},\"content_digest\":\"{}\",",
            "\"physical_bytes_exact\":{},\"canonical_bytes_exact\":{},",
            "\"metadata_exact\":{},\"historical_roots_exact\":{},",
            "\"route_exact\":{}}}"
        ),
        oracle.logical_length,
        oracle.content_digest,
        option_bool_json(oracle.physical_bytes_exact),
        option_bool_json(oracle.canonical_bytes_exact),
        option_bool_json(oracle.metadata_exact),
        option_bool_json(oracle.historical_roots_exact),
        option_bool_json(oracle.route_exact),
    )
}

fn option_u64_json(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn option_bool_json(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "null",
    }
}

struct PieceCursor<'a> {
    table: &'a PieceTable,
    backing: &'a [u8],
    piece_index: usize,
    within_piece: u64,
    position: u64,
    original_scratch: Vec<u8>,
    original_scratch_block: Option<u64>,
    original_blocks_generated: u64,
}

impl<'a> PieceCursor<'a> {
    fn new(table: &'a PieceTable, backing: &'a [u8]) -> Self {
        Self {
            table,
            backing,
            piece_index: 0,
            within_piece: 0,
            position: 0,
            original_scratch: vec![0_u8; BUFFER_BYTES],
            original_scratch_block: None,
            original_blocks_generated: 0,
        }
    }

    fn read_exact_expected(&mut self, mut output: &mut [u8]) -> EvalResult<()> {
        while !output.is_empty() {
            let piece = self
                .table
                .pieces
                .get(self.piece_index)
                .ok_or_else(|| "physical/canonical stream exceeded oracle".to_owned())?;
            let remaining = piece
                .length()
                .checked_sub(self.within_piece)
                .ok_or_else(|| "piece cursor underflow".to_owned())?;
            let take = output
                .len()
                .min(usize::try_from(remaining).map_err(display_error)?);
            match piece {
                Piece::Original { offset, .. } => {
                    let source = offset
                        .checked_add(self.within_piece)
                        .ok_or_else(|| "original cursor overflow".to_owned())?;
                    let block = source / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
                    if self.original_scratch_block != Some(block) {
                        stage1_fixture::fill_retained_buffer(&mut self.original_scratch, block);
                        self.original_scratch_block = Some(block);
                        self.original_blocks_generated = self
                            .original_blocks_generated
                            .checked_add(1)
                            .ok_or_else(|| "original oracle block count overflow".to_owned())?;
                    }
                    let within = usize::try_from(source - block).map_err(display_error)?;
                    let block_take = take.min(BUFFER_BYTES - within);
                    output[..block_take]
                        .copy_from_slice(&self.original_scratch[within..within + block_take]);
                    self.advance(block_take)?;
                    output = &mut output[block_take..];
                }
                Piece::Inserted { offset, .. } => {
                    let start = offset
                        .checked_add(usize::try_from(self.within_piece).map_err(display_error)?)
                        .ok_or_else(|| "inserted cursor overflow".to_owned())?;
                    let end = start
                        .checked_add(take)
                        .ok_or_else(|| "inserted cursor end overflow".to_owned())?;
                    output[..take].copy_from_slice(
                        self.backing
                            .get(start..end)
                            .ok_or_else(|| "inserted cursor exceeds backing".to_owned())?,
                    );
                    self.advance(take)?;
                    output = &mut output[take..];
                }
            }
        }
        Ok(())
    }

    fn advance(&mut self, bytes: usize) -> EvalResult<()> {
        let bytes = u64::try_from(bytes).map_err(display_error)?;
        self.within_piece = self
            .within_piece
            .checked_add(bytes)
            .ok_or_else(|| "piece cursor offset overflow".to_owned())?;
        self.position = self
            .position
            .checked_add(bytes)
            .ok_or_else(|| "piece cursor position overflow".to_owned())?;
        if self.within_piece
            == self
                .table
                .pieces
                .get(self.piece_index)
                .ok_or_else(|| "piece cursor index overflow".to_owned())?
                .length()
        {
            self.piece_index += 1;
            self.within_piece = 0;
        }
        Ok(())
    }

    fn finish(&self) -> EvalResult<()> {
        if self.position != self.table.logical_length
            || self.piece_index != self.table.pieces.len()
            || self.within_piece != 0
        {
            return Err(format!(
                "oracle stream length mismatch: position={} expected={} piece={}/{} within={}",
                self.position,
                self.table.logical_length,
                self.piece_index,
                self.table.pieces.len(),
                self.within_piece
            ));
        }
        Ok(())
    }
}

struct PieceCompareWriter<'a> {
    cursor: PieceCursor<'a>,
    expected: Vec<u8>,
    hasher: blake3::Hasher,
}

impl<'a> PieceCompareWriter<'a> {
    fn new(table: &'a PieceTable, backing: &'a [u8]) -> Self {
        Self {
            cursor: PieceCursor::new(table, backing),
            expected: vec![0_u8; BUFFER_BYTES],
            hasher: blake3::Hasher::new(),
        }
    }

    fn finish(self) -> EvalResult<String> {
        self.cursor.finish()?;
        Ok(self.hasher.finalize().to_hex().to_string())
    }
}

impl Write for PieceCompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.expected.len() {
            return Err(std::io::Error::other(
                "product write exceeds 1 MiB oracle buffer",
            ));
        }
        self.cursor
            .read_exact_expected(&mut self.expected[..bytes.len()])
            .map_err(std::io::Error::other)?;
        if self.expected[..bytes.len()] != *bytes {
            return Err(std::io::Error::other("independent byte oracle mismatch"));
        }
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn compare_managed(
    managed: &layerfs_sdk::ManagedWorkspace,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<(String, layerfs_sdk::OperationDiagnostics)> {
    let mut sink = PieceCompareWriter::new(table, backing);
    let counters = managed
        .read_to(FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let digest = sink.finish()?;
    Ok((digest, counters))
}

fn compare_canonical(
    fs: &LayerFs,
    root: RootId,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<(String, layerfs_sdk::OperationDiagnostics)> {
    let mut sink = PieceCompareWriter::new(table, backing);
    let counters = fs
        .read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let digest = sink.finish()?;
    Ok((digest, counters))
}

fn compare_canonical_range(
    fs: &LayerFs,
    root: RootId,
    table: &PieceTable,
    backing: &[u8],
    start: u64,
    length: u64,
) -> EvalResult<layerfs_sdk::OperationDiagnostics> {
    let range = table.range(start, length)?;
    let mut sink = PieceCompareWriter::new(&range, backing);
    let counters = fs
        .read_range(
            root,
            FILE_PATH,
            start
                ..start
                    .checked_add(length)
                    .ok_or_else(|| "canonical range overflow".to_owned())?,
            &mut sink,
        )
        .map_err(display_error)?;
    sink.finish()?;
    Ok(counters)
}

fn compare_external(
    external: &layerfs_sdk::ExternalWorkspace,
    table: &PieceTable,
    backing: &[u8],
) -> EvalResult<String> {
    let mut file = File::open(external.path().join(FILE_PATH)).map_err(io_error)?;
    let mut sink = PieceCompareWriter::new(table, backing);
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        sink.write_all(&buffer[..read]).map_err(io_error)?;
    }
    sink.finish()
}

fn metadata_exact(actual: &NativeMetadata, expected: &NativeMetadata) -> bool {
    actual == expected
}

fn verify_supported_metadata(metadata: &NativeMetadata, label: &str) -> EvalResult<()> {
    if metadata.mode != FIXTURE_MODE
        || metadata.mtime_nanoseconds >= 1_000_000_000
        || !metadata.xattrs.is_empty()
        || metadata.acl.is_some()
        || metadata.bsd_flags != 0
    {
        return Err(format!(
            "{label} mode/xattr/ACL/BSD-flags frozen supported invariant"
        ));
    }
    Ok(())
}

fn metadata_receipt_json(metadata: &NativeMetadata) -> String {
    let xattrs = metadata
        .xattrs
        .iter()
        .map(|(name, value)| {
            format!(
                "{{\"name_hex\":\"{}\",\"value_hex\":\"{}\"}}",
                hex(&name),
                hex(&value)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let acl_hex = metadata
        .acl
        .as_ref()
        .map_or_else(|| "null".to_owned(), |acl| format!("\"{}\"", hex(acl)));
    format!(
        concat!(
            "{{\"mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},",
            "\"xattr_count\":{},\"xattrs\":[{}],",
            "\"acl_present\":{},\"acl_hex\":{},\"bsd_flags\":{}}}"
        ),
        metadata.mode,
        metadata.mtime_seconds,
        metadata.mtime_nanoseconds,
        metadata.xattrs.len(),
        xattrs,
        metadata.acl.is_some(),
        acl_hex,
        metadata.bsd_flags,
    )
}

fn native_route_name(route: Option<layerfs_sdk::NativeRoute>) -> &'static str {
    match route {
        None => "NotApplicable",
        Some(layerfs_sdk::NativeRoute::ExactNoop) => "ExactNoop",
        Some(layerfs_sdk::NativeRoute::ClonePatch) => "ClonePatch",
        Some(layerfs_sdk::NativeRoute::CloneShift) => "CloneShift",
        Some(layerfs_sdk::NativeRoute::InPlacePatch) => "InPlacePatch",
        Some(layerfs_sdk::NativeRoute::InPlaceShift) => "InPlaceShift",
        Some(layerfs_sdk::NativeRoute::FullFallback) => "FullFallback",
        Some(layerfs_sdk::NativeRoute::MaterializeStream)
        | Some(layerfs_sdk::NativeRoute::NativeDurableOutput)
        | Some(layerfs_sdk::NativeRoute::CaptureStream)
        | Some(layerfs_sdk::NativeRoute::Rename)
        | Some(layerfs_sdk::NativeRoute::ProtectedExactNoop) => "NotApplicable",
    }
}

fn verify_native_edit(
    edit: &EditSpec,
    operation: &layerfs_sdk::OperationDiagnostics,
) -> EvalResult<()> {
    let native = operation.native;
    if edit.delete_bytes == edit.insert_bytes {
        if !matches!(
            native.route,
            Some(layerfs_sdk::NativeRoute::ClonePatch | layerfs_sdk::NativeRoute::InPlacePatch)
        ) || native.patch_bytes != edit.insert_bytes
            || native.suffix_bytes_shifted != 0
            || native.bytes_written != edit.insert_bytes
            || native.clone_attempts != 1
            || (native.route == Some(layerfs_sdk::NativeRoute::ClonePatch)
                && (native.clone_successes != 1 || native.clone_fallbacks != 0))
            || (native.route == Some(layerfs_sdk::NativeRoute::InPlacePatch)
                && (native.clone_successes != 0 || native.clone_fallbacks != 1))
        {
            return Err(format!("{} same-length native route equation", edit.tag));
        }
    } else {
        let suffix = edit
            .before_bytes
            .checked_sub(
                edit.offset
                    .checked_add(edit.delete_bytes)
                    .ok_or_else(|| "native suffix equation overflow".to_owned())?,
            )
            .ok_or_else(|| "native suffix equation underflow".to_owned())?;
        if native.route != Some(layerfs_sdk::NativeRoute::InPlaceShift)
            || native.suffix_bytes_shifted != suffix
            || native.bytes_read != suffix
            || native.bytes_written
                != suffix
                    .checked_add(edit.insert_bytes)
                    .ok_or_else(|| "native write equation overflow".to_owned())?
        {
            return Err(format!(
                "{} count-changing native read=S; write=S+B equation",
                edit.tag
            ));
        }
    }
    if operation.operation_q_terminal_bytes != 0
        || operation.operation_q_high_water_bytes > 8_388_608
    {
        return Err(format!("{} operation Q closure", edit.tag));
    }
    Ok(())
}

fn verify_refresh(
    edit: &EditSpec,
    operation: &layerfs_sdk::OperationDiagnostics,
) -> EvalResult<()> {
    if edit.kind == EditKind::Overwrite {
        if !matches!(
            operation.native.route,
            Some(layerfs_sdk::NativeRoute::ClonePatch | layerfs_sdk::NativeRoute::InPlacePatch)
        ) || operation.full_fallback_files != 0
            || operation.rematerializations != 0
        {
            return Err(format!("{} same-size refresh route", edit.tag));
        }
    } else {
        let suffix = edit
            .before_bytes
            .checked_sub(
                edit.offset
                    .checked_add(edit.delete_bytes)
                    .ok_or_else(|| "refresh suffix equation overflow".to_owned())?,
            )
            .ok_or_else(|| "refresh suffix equation underflow".to_owned())?;
        if !matches!(
            operation.native.route,
            Some(layerfs_sdk::NativeRoute::CloneShift | layerfs_sdk::NativeRoute::InPlaceShift)
        ) || operation.full_fallback_files != 0
            || operation.rematerializations != 0
            || operation.native.suffix_bytes_shifted != suffix
            || operation.native.bytes_read != suffix
            || operation.native.bytes_written
                != suffix
                    .checked_add(edit.insert_bytes)
                    .ok_or_else(|| "refresh write equation overflow".to_owned())?
            || operation.native.patch_bytes != edit.insert_bytes
            || (operation.native.route == Some(layerfs_sdk::NativeRoute::CloneShift)
                && (operation.native.clone_attempts != 1
                    || operation.native.clone_successes != 1
                    || operation.native.clone_fallbacks != 0))
            || (operation.native.route == Some(layerfs_sdk::NativeRoute::InPlaceShift)
                && (operation.native.clone_successes != 0
                    || operation.native.clone_attempts != operation.native.clone_fallbacks))
        {
            return Err(format!(
                "{} count-changing shift read=S; write=S+B route",
                edit.tag
            ));
        }
    }
    if operation.workspace_reuses != 1
        || operation.operation_q_terminal_bytes != 0
        || operation.operation_q_high_water_bytes > 8_388_608
    {
        return Err(format!("{} refresh reuse/Q closure", edit.tag));
    }
    Ok(())
}

fn verify_storage_transition(before: &Diagnostics, after: &Diagnostics) -> EvalResult<()> {
    let (before_database, after_database) = before
        .database_bytes
        .zip(after.database_bytes)
        .ok_or_else(|| "transition database_bytes observation is required".to_owned())?;
    let (before_engine, after_engine) = before
        .logical_engine_bytes
        .zip(after.logical_engine_bytes)
        .ok_or_else(|| "transition logical_engine_bytes observation is required".to_owned())?;
    if after_database < before_database
        || after_engine < before_engine
        || after.object_bytes_written < before.object_bytes_written
    {
        return Err(format!(
            concat!(
                "transition storage monotonicity database={}->{} ",
                "logical_engine={}->{} object_bytes_written={}->{}"
            ),
            before_database,
            after_database,
            before_engine,
            after_engine,
            before.object_bytes_written,
            after.object_bytes_written
        ));
    }
    Ok(())
}

fn combine_physical_checkpoint(
    native: layerfs_sdk::OperationDiagnostics,
    checkpoint: layerfs_sdk::OperationDiagnostics,
) -> EvalResult<layerfs_sdk::OperationDiagnostics> {
    native.merge(checkpoint).map_err(display_error)
}

fn combine_logical_refresh(
    mut logical: layerfs_sdk::OperationDiagnostics,
    refresh: layerfs_sdk::OperationDiagnostics,
) -> EvalResult<layerfs_sdk::OperationDiagnostics> {
    logical.native = refresh.native;
    logical.workspace_reuses = refresh.workspace_reuses;
    logical.rematerializations = refresh.rematerializations;
    logical.full_fallback_files = refresh.full_fallback_files;
    logical.root_diff_nodes = refresh.root_diff_nodes;
    logical.changed_paths = refresh.changed_paths;
    logical.plan_rows = refresh.plan_rows;
    logical.plan_scratch_high_water_bytes = refresh.plan_scratch_high_water_bytes;
    logical.scratch_tables = refresh.scratch_tables;
    logical.scratch_statements = refresh.scratch_statements;
    logical.scratch_rows = refresh.scratch_rows;
    logical.scratch_high_water_bytes = refresh.scratch_high_water_bytes;
    logical.operation_q_current_bytes = logical
        .operation_q_current_bytes
        .max(refresh.operation_q_current_bytes);
    logical.operation_q_high_water_bytes = logical
        .operation_q_high_water_bytes
        .max(refresh.operation_q_high_water_bytes);
    logical.operation_q_terminal_bytes = logical
        .operation_q_terminal_bytes
        .max(refresh.operation_q_terminal_bytes);
    Ok(logical)
}

fn unavailable_defaults() -> Vec<Unavailable> {
    vec![
        Unavailable {
            field: "native.sync_regular_calls".to_owned(),
            availability: "Unavailable",
            reason: "product exposes only aggregate sync_calls".to_owned(),
        },
        Unavailable {
            field: "native.sync_directory_calls".to_owned(),
            availability: "Unavailable",
            reason: "product exposes only aggregate sync_calls".to_owned(),
        },
        Unavailable {
            field: "storage.rollback_journal_bytes".to_owned(),
            availability: "Unavailable",
            reason: "not continuously observed".to_owned(),
        },
        Unavailable {
            field: "storage.temporary_file_bytes".to_owned(),
            availability: "Unavailable",
            reason: "product storage observation does not expose a continuous peak".to_owned(),
        },
    ]
}

fn row_residual(row_wall_ns: u128, phases: &[Phase]) -> EvalResult<u128> {
    let attributed = phases.iter().try_fold(0_u128, |total, phase| {
        total
            .checked_add(phase.wall_ns)
            .ok_or_else(|| "row phase sum overflow".to_owned())
    })?;
    row_wall_ns
        .checked_sub(attributed)
        .ok_or_else(|| "row phase sum exceeds row wall".to_owned())
}

fn observe_row_resources(
    residue_root: Option<&Path>,
    active_store_connections: u64,
) -> EvalResult<ResourceObservation> {
    Ok(ResourceObservation {
        rss_current_bytes: None,
        rss_peak_bytes: maximum_rss_bytes()?,
        fd_current: fd_count()?,
        active_store_connections,
        child_processes: 0,
        owned_temp_entries: None,
        residue_entries: residue_root.map(residue_count).transpose()?.unwrap_or(0),
    })
}

fn observe_external_resources(
    residue_root: Option<&Path>,
    store: Option<&Path>,
) -> EvalResult<ResourceObservation> {
    Ok(ResourceObservation {
        rss_current_bytes: Some(current_rss_bytes()?),
        rss_peak_bytes: maximum_rss_bytes()?,
        fd_current: fd_count()?,
        active_store_connections: open_store_connection_count(store)?,
        child_processes: child_process_count()?,
        owned_temp_entries: None,
        residue_entries: residue_root.map(residue_count).transpose()?.unwrap_or(0),
    })
}

fn fd_count() -> EvalResult<u64> {
    Ok(fs::read_dir("/dev/fd").map_err(io_error)?.count() as u64)
}

fn open_store_connection_count(store: Option<&Path>) -> EvalResult<u64> {
    let Some(store) = store else {
        return Ok(0);
    };
    let store = store
        .canonicalize()
        .unwrap_or_else(|_| store.to_path_buf())
        .display()
        .to_string();
    let pid = std::process::id().to_string();
    let output = command_output("/usr/sbin/lsof", &["-Fn", "-p", &pid])?;
    Ok(output
        .lines()
        .filter(|line| {
            line.starts_with('n')
                && line.strip_prefix('n').is_some_and(|path| {
                    path.starts_with(&store)
                        && path.contains("generation-")
                        && path.ends_with(".sqlite")
                })
        })
        .count() as u64)
}

fn current_rss_bytes() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/bin/ps", &["-o", "rss=", "-p", &pid])?;
    output
        .trim()
        .parse::<u64>()
        .map_err(display_error)?
        .checked_mul(1_024)
        .ok_or_else(|| "RSS conversion overflow".to_owned())
}

#[cfg(target_os = "macos")]
fn maximum_rss_bytes() -> EvalResult<u64> {
    use std::ffi::c_int;

    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        seconds: i64,
        microseconds: i64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        user: TimeVal,
        system: TimeVal,
        maximum_resident_set_bytes: i64,
        remaining: [i64; 13],
    }

    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
    }

    let mut usage = RUsage::default();
    // SAFETY: usage is a live Darwin-compatible rusage buffer for this call.
    if unsafe { getrusage(0, &mut usage) } != 0 || usage.maximum_resident_set_bytes < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(usage.maximum_resident_set_bytes as u64)
}

#[cfg(not(target_os = "macos"))]
fn maximum_rss_bytes() -> EvalResult<u64> {
    current_rss_bytes()
}

fn child_process_count() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("/usr/bin/pgrep")
        .args(["-P", &pid])
        .output()
        .map_err(io_error)?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)
            .map_err(display_error)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u64)
    } else if output.status.code() == Some(1) {
        Ok(0)
    } else {
        Err(format!("pgrep exited {}", output.status))
    }
}

fn residue_count(root: &Path) -> EvalResult<u64> {
    if !root.exists() {
        return Ok(0);
    }
    let mut count = 0_u64;
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with("-journal")
                || name.ends_with("-wal")
                || name.ends_with("-shm")
                || name == "CURRENT.tmp"
                || name.starts_with(".layerfs-")
            {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| "residue count overflow".to_owned())?;
            }
            if entry.file_type().map_err(io_error)?.is_dir() {
                stack.push(path);
            }
        }
    }
    Ok(count)
}

fn fixture_root() -> PathBuf {
    stage1_fixture::workspace_root().join("target/layerfs-stage1-fixtures/apple-edge-v1")
}

fn readiness_path() -> PathBuf {
    stage1_fixture::workspace_root().join("target/layerfs-stage1-apple-edge-readiness.json")
}

pub fn prepare() -> EvalResult<()> {
    let destination = fixture_root();
    if destination.exists() {
        let master = read_master(&destination)?;
        verify_fixture(&destination, &master, true)?;
        println!(
            "stage1.1-prepare status=PASS fixture={} reused=true wall_ns={}",
            destination.display(),
            master.preparation_wall_ns
        );
        return Ok(());
    }
    let started = Instant::now();
    let parent = destination
        .parent()
        .ok_or_else(|| "fixture has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let temporary = parent.join(format!(
        ".apple-edge-v1-preparing-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    fs::create_dir(&temporary).map_err(io_error)?;
    let result = prepare_into(&temporary, started);
    match result {
        Ok(master) => {
            fs::rename(&temporary, &destination).map_err(io_error)?;
            stage1_fixture::sync_directory(parent)?;
            println!(
                "stage1.1-prepare status=PASS fixture={} reused=false wall_ns={}",
                destination.display(),
                master.preparation_wall_ns
            );
            Ok(())
        }
        Err(error) => {
            let failure = parent.join(format!("apple-edge-v1-preparation-failure-{}", unix_ns()?));
            let _ = fs::rename(&temporary, &failure);
            Err(format!(
                "{error}; preparation evidence preserved at {}",
                failure.display()
            ))
        }
    }
}

fn prepare_into(root: &Path, started: Instant) -> EvalResult<FixtureMaster> {
    let source = root.join("source-native/data/payload.bin");
    fs::create_dir_all(
        source
            .parent()
            .ok_or_else(|| "source has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let raw_digest = generate_source(&source)?;
    let base = root.join("bases/base");
    fs::create_dir_all(
        base.parent()
            .ok_or_else(|| "base has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    let opened = LayerFs::open(&base).map_err(display_error)?;
    let store_id = hex(&opened.fs.store_id().map_err(display_error)?);
    let capture_source = root.join("capture-source");
    let mut external = opened
        .fs
        .materialize_external(opened.head, &capture_source)
        .map_err(display_error)?;
    let native_file = external.path().join(FILE_PATH);
    fs::create_dir_all(
        native_file
            .parent()
            .ok_or_else(|| "native file has no parent".to_owned())?,
    )
    .map_err(io_error)?;
    copy_file_bounded(&source, &native_file)?;
    set_fixture_metadata(&native_file)?;
    let root_id = external.capture_quiescent().map_err(display_error)?;
    let ref_state = opened.fs.current_head("main").map_err(display_error)?;
    if ref_state.root != root_id {
        return Err("fixture capture did not publish exact root".to_owned());
    }
    let diagnostics = opened.fs.counter_snapshot().map_err(display_error)?;
    validate_profile(&diagnostics)?;
    compare_canonical_source(&opened.fs, root_id, &source)?;
    drop(external);
    fs::remove_dir_all(&capture_source).map_err(io_error)?;
    drop(opened);

    let reopened = LayerFs::open(&base).map_err(display_error)?;
    if reopened.ref_state != ref_state
        || hex(&reopened.fs.store_id().map_err(display_error)?) != store_id
    {
        return Err("fresh Verified fixture reopen changed authority".to_owned());
    }
    compare_canonical_source(&reopened.fs, root_id, &source)?;
    validate_profile(&reopened.fs.counter_snapshot().map_err(display_error)?)?;
    drop(reopened);

    let apfs_identity = stage1_fixture::assert_apfs(root)?;
    let mut master = FixtureMaster {
        raw_digest,
        root: root_id,
        generation: ref_state.generation,
        store_id,
        profile: "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0".to_owned(),
        apfs_identity,
        fixture_blake3: String::new(),
        preparation_wall_ns: 0,
    };
    master.fixture_blake3 = stage1_fixture::tree_digest(root, Some(Path::new("master.json")))?;
    master.preparation_wall_ns = started.elapsed().as_nanos();
    if master.preparation_wall_ns > PREPARATION_LIMIT_NS {
        return Err(format!(
            "fixture preparation {}ns exceeds {}ns",
            master.preparation_wall_ns, PREPARATION_LIMIT_NS
        ));
    }
    durable_write(&root.join("master.json"), &master_json(&master))?;
    stage1_fixture::seal_tree(root)?;
    verify_fixture(root, &master, true)?;
    Ok(master)
}

fn generate_source(path: &Path) -> EvalResult<String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut offset = 0_u64;
    while offset < INITIAL_BYTES {
        stage1_fixture::fill_retained_buffer(&mut buffer, offset);
        let take = usize::try_from((INITIAL_BYTES - offset).min(BUFFER_BYTES as u64))
            .map_err(display_error)?;
        file.write_all(&buffer[..take]).map_err(io_error)?;
        hasher.update(&buffer[..take]);
        offset += take as u64;
    }
    file.sync_all().map_err(io_error)?;
    set_fixture_metadata(path)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn set_fixture_metadata(path: &Path) -> EvalResult<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(FIXTURE_MODE)).map_err(io_error)?;
    let modified = UNIX_EPOCH
        .checked_add(Duration::new(
            FIXTURE_MTIME_SECONDS,
            FIXTURE_MTIME_NANOSECONDS,
        ))
        .ok_or_else(|| "fixture mtime overflow".to_owned())?;
    File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_times(FileTimes::new().set_modified(modified)))
        .map_err(io_error)
}

fn copy_file_bounded(source: &Path, destination: &Path) -> EvalResult<()> {
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io_error)?;
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let read = input.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(io_error)?;
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "copy length overflow".to_owned())?;
    }
    if total != INITIAL_BYTES {
        return Err(format!("fixture copy wrote {total} bytes"));
    }
    output.sync_all().map_err(io_error)
}

fn compare_canonical_source(fs: &LayerFs, root: RootId, source: &Path) -> EvalResult<()> {
    let input = File::open(source).map_err(io_error)?;
    let mut sink = FileCompareWriter::new(input);
    fs.read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    sink.finish(INITIAL_BYTES)
}

struct FileCompareWriter<R> {
    input: R,
    compared: u64,
}

impl<R: Read> FileCompareWriter<R> {
    fn new(input: R) -> Self {
        Self { input, compared: 0 }
    }

    fn finish(mut self, expected: u64) -> EvalResult<()> {
        let mut extra = [0_u8; 1];
        if self.compared != expected || self.input.read(&mut extra).map_err(io_error)? != 0 {
            return Err("canonical/source comparison length mismatch".to_owned());
        }
        Ok(())
    }
}

impl<R: Read> Write for FileCompareWriter<R> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut expected = vec![0_u8; bytes.len()];
        self.input.read_exact(&mut expected)?;
        if expected != bytes {
            return Err(std::io::Error::other("canonical/source byte mismatch"));
        }
        self.compared = self
            .compared
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("comparison length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn master_json(master: &FixtureMaster) -> String {
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-master-v1\",",
            "\"fixture_version\":\"{}\",\"file_path\":\"{}\",",
            "\"initial_bytes\":{},\"maximum_bytes\":{},\"terminal_bytes\":{},",
            "\"mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},",
            "\"raw_digest\":\"{}\",\"root\":\"{}\",\"generation\":{},",
            "\"store_id\":\"{}\",\"profile\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"fixture_blake3\":\"{}\",",
            "\"preparation_wall_ns\":{}}}\n"
        ),
        FIXTURE_VERSION,
        FILE_PATH,
        INITIAL_BYTES,
        MAXIMUM_BYTES,
        INITIAL_BYTES,
        FIXTURE_MODE,
        FIXTURE_MTIME_SECONDS,
        FIXTURE_MTIME_NANOSECONDS,
        master.raw_digest,
        master.root,
        master.generation,
        master.store_id,
        json_escape(&master.profile),
        json_escape(&master.apfs_identity),
        master.fixture_blake3,
        master.preparation_wall_ns,
    )
}

fn read_master(root: &Path) -> EvalResult<FixtureMaster> {
    let json = fs::read_to_string(root.join("master.json")).map_err(io_error)?;
    if json_string(&json, "schema")? != "layerfs-stage1.1-master-v1"
        || json_string(&json, "fixture_version")? != FIXTURE_VERSION
        || json_string(&json, "file_path")? != FILE_PATH
        || json_u128(&json, "initial_bytes")? != u128::from(INITIAL_BYTES)
        || json_u128(&json, "maximum_bytes")? != u128::from(MAXIMUM_BYTES)
        || json_u128(&json, "terminal_bytes")? != u128::from(INITIAL_BYTES)
        || json_u128(&json, "mode")? != u128::from(FIXTURE_MODE)
        || json_u128(&json, "mtime_seconds")? != u128::from(FIXTURE_MTIME_SECONDS)
        || json_u128(&json, "mtime_nanoseconds")? != u128::from(FIXTURE_MTIME_NANOSECONDS)
    {
        return Err("fixture master frozen constants mismatch".to_owned());
    }
    Ok(FixtureMaster {
        raw_digest: json_string(&json, "raw_digest")?,
        root: RootId::from_str(&json_string(&json, "root")?).map_err(display_error)?,
        generation: u64::try_from(json_u128(&json, "generation")?).map_err(display_error)?,
        store_id: json_string(&json, "store_id")?,
        profile: json_string(&json, "profile")?,
        apfs_identity: json_string(&json, "apfs_identity")?,
        fixture_blake3: json_string(&json, "fixture_blake3")?,
        preparation_wall_ns: json_u128(&json, "preparation_wall_ns")?,
    })
}

fn verify_fixture(root: &Path, master: &FixtureMaster, full: bool) -> EvalResult<()> {
    stage1_fixture::verify_sealed(root)?;
    if master.preparation_wall_ns > PREPARATION_LIMIT_NS
        || master.profile != "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0"
        || stage1_fixture::assert_apfs(root)? != master.apfs_identity
    {
        return Err("fixture master custody/profile mismatch".to_owned());
    }
    let source = root.join("source-native/data/payload.bin");
    let metadata = fs::metadata(&source).map_err(io_error)?;
    if metadata.len() != INITIAL_BYTES || metadata.permissions().mode() & 0o777 != 0o444 {
        return Err("fixture source size/seal mismatch".to_owned());
    }
    if full && stage1_fixture::hash_file(&source)? != master.raw_digest {
        return Err("fixture source digest mismatch".to_owned());
    }
    if stage1_fixture::tree_digest(root, Some(Path::new("master.json")))? != master.fixture_blake3 {
        return Err("fixture tree digest mismatch".to_owned());
    }
    let opened = LayerFs::open(&root.join("bases/base")).map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("fixture Store authority mismatch".to_owned());
    }
    validate_profile(&opened.fs.counter_snapshot().map_err(display_error)?)?;
    if full {
        compare_canonical_source(&opened.fs, master.root, &source)?;
    }
    drop(opened);
    Ok(())
}

fn validate_profile(diagnostics: &Diagnostics) -> EvalResult<()> {
    if diagnostics.page_size != 4_096
        || diagnostics.cache_pages != 1_280
        || diagnostics.cache_spill_pages != 1_280
    {
        return Err(format!(
            "Store profile mismatch: page={} cache={} spill={}",
            diagnostics.page_size, diagnostics.cache_pages, diagnostics.cache_spill_pages
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct SourceIdentity {
    git_commit: String,
    dirty_tree: bool,
    tree_blake3: String,
    manifest_sha256: String,
    executable_path: PathBuf,
    executable_sha256: String,
    executable_blake3: String,
}

pub fn readiness() -> EvalResult<()> {
    if cfg!(debug_assertions) {
        return Err("Stage 1.1 readiness requires the release evaluator".to_owned());
    }
    let schedule = frozen_schedule()?;
    let schedule_json = schedule_json(&schedule)?;
    let root = fixture_root();
    let master = read_master(&root)?;
    verify_fixture(&root, &master, true)?;
    let source = source_identity()?;
    let reset = stage1_fixture::workspace_root().join(format!(
        "target/.layerfs-stage1.1-readiness-reset-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let reset_started = Instant::now();
    stage1_fixture::clone_directory(&root.join("bases/base"), &reset)?;
    stage1_fixture::make_writable(&reset)?;
    let opened = LayerFs::open_with_integrity(&reset, IntegrityMode::TrustedLocalDev)
        .map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("readiness reset authority mismatch".to_owned());
    }
    validate_profile(&opened.fs.counter_snapshot().map_err(display_error)?)?;
    drop(opened);
    stage1_fixture::make_writable(&reset)?;
    fs::remove_dir_all(&reset).map_err(io_error)?;
    let reset_wall_ns = reset_started.elapsed().as_nanos();
    if reset_wall_ns > RESET_LIMIT_NS {
        return Err(format!(
            "readiness reset {reset_wall_ns}ns exceeds {RESET_LIMIT_NS}ns"
        ));
    }
    let forecast = FROZEN_NON_RESET_FORECAST_NS
        .checked_add(reset_wall_ns)
        .ok_or_else(|| "readiness forecast overflow".to_owned())?;
    if forecast >= CAMPAIGN_LIMIT_NS {
        return Err(format!(
            "readiness forecast {forecast}ns does not leave sub-60s reserve"
        ));
    }
    let path = readiness_path();
    let schedule_sha256 = sha256_bytes(schedule_json.as_bytes())?;
    let master_sha256 = sha256_file(&root.join("master.json"))?;
    let json = format!(
        concat!(
            "{{\"schema\":\"{}\",\"status\":\"PASS\",",
            "\"measured_rows_started\":false,\"run_directory_exists\":false,",
            "\"expected_rows\":47,\"edit_suboperations\":51,\"transitions\":34,",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"fixture_master_sha256\":\"{}\",",
            "\"fixture_blake3\":\"{}\",\"schedule_sha256\":\"{}\",",
            "\"store_id\":\"{}\",\"profile\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"reset_wall_ns\":{},",
            "\"reset_limit_ns\":{},\"forecast_non_reset_wall_ns\":{},",
            "\"forecast_campaign_wall_ns\":{},\"forecast_reserve_ns\":{},",
            "\"hard_limit_ns\":{},\"git_commit\":\"{}\",\"dirty_tree\":{}}}\n"
        ),
        READINESS_SCHEMA,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        master_sha256,
        master.fixture_blake3,
        schedule_sha256,
        master.store_id,
        json_escape(&master.profile),
        json_escape(&master.apfs_identity),
        reset_wall_ns,
        RESET_LIMIT_NS,
        FROZEN_NON_RESET_FORECAST_NS,
        forecast,
        CAMPAIGN_LIMIT_NS - forecast,
        CAMPAIGN_LIMIT_NS,
        source.git_commit,
        source.dirty_tree,
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    if path.exists() {
        let preserved = path.with_file_name(format!(
            "layerfs-stage1-apple-edge-readiness-preserved-{}.json",
            unix_ns()?
        ));
        fs::rename(&path, &preserved).map_err(io_error)?;
    }
    durable_write(&path, &json)?;
    println!(
        "stage1.1-readiness status=PASS receipt={} reset_wall_ns={} forecast_campaign_wall_ns={} measured_rows_started=false",
        path.display(), reset_wall_ns, forecast
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_physical_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    managed: &mut layerfs_sdk::ManagedWorkspace,
    edit: &EditSpec,
    table: &PieceTable,
    roots: &mut Vec<RefState>,
    metadata: &mut Vec<NativeMetadata>,
    work: &Path,
) -> EvalResult<()> {
    let id = format!("C03-{:03}", edit.serial);
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let pre_ref = roots
        .last()
        .cloned()
        .ok_or_else(|| "physical transition has no pre-ref".to_owned())?;
    let before_storage = fs.diagnostics().map_err(display_error)?;
    let replacement = campaign
        .schedule
        .replacement_backing
        .get(
            edit.replacement_offset
                ..edit
                    .replacement_offset
                    .checked_add(usize::try_from(edit.insert_bytes).map_err(display_error)?)
                    .ok_or_else(|| "physical replacement range overflow".to_owned())?,
        )
        .ok_or_else(|| "physical replacement exceeds backing".to_owned())?;

    set_failure_phase("native_edit");
    let native_started = Instant::now();
    let native = managed
        .replace_observed(FILE_PATH, edit.offset, edit.delete_bytes, replacement)
        .map_err(display_error)?;
    let native_wall = native_started.elapsed().as_nanos();
    verify_native_edit(edit, &native)?;

    set_failure_phase("live_physical_oracle");
    let physical_started = Instant::now();
    let (physical_digest, _) =
        compare_managed(managed, table, &campaign.schedule.replacement_backing)?;
    let current_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&current_metadata, &edit.tag)?;
    let physical_wall = physical_started.elapsed().as_nanos();
    campaign.physical_oracles += 1;

    set_failure_phase("durable_checkpoint");
    let checkpoint_started = Instant::now();
    let (post_ref, checkpoint) = managed.checkpoint_observed().map_err(display_error)?;
    let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
    if post_ref.name != "main"
        || post_ref.generation != pre_ref.generation + 1
        || fs.current_head("main").map_err(display_error)? != post_ref
        || checkpoint.workspace_reuses != 1
        || checkpoint.rematerializations != 0
        || checkpoint.descriptor_resets != 1
        || checkpoint.operation_q_terminal_bytes != 0
    {
        return Err(format!("{} exact RefState/checkpoint closure", edit.tag));
    }
    let tree_level = checkpoint
        .rope
        .tree_level_before
        .ok_or_else(|| format!("{} checkpoint missing actual H", edit.tag))?;
    verify_locality(&checkpoint, edit.insert_bytes, tree_level)?;
    let after_checkpoint = fs.counter_snapshot().map_err(display_error)?;

    set_failure_phase("canonical_witness");
    let canonical_started = Instant::now();
    let (canonical_digest, _) = compare_canonical(
        fs,
        post_ref.root,
        table,
        &campaign.schedule.replacement_backing,
    )?;
    let canonical_wall = canonical_started.elapsed().as_nanos();
    if canonical_digest != physical_digest {
        return Err(format!("{} physical digest = canonical digest", edit.tag));
    }
    let after_witness = fs.counter_snapshot().map_err(display_error)?;
    campaign.canonical_transitions += 1;

    set_failure_phase("counter_snapshot");
    let counter_started = Instant::now();
    let after_storage = fs.diagnostics().map_err(display_error)?;
    verify_storage_transition(&before_storage, &after_storage)?;
    let engine = EngineDelta::between(&before_storage, &after_storage)?;
    engine.verify_trusted_transition()?;
    let checkpoint_engine =
        PhaseCounterDelta::between("checkpoint", &before_storage, &after_checkpoint)?;
    checkpoint_engine.engine.verify_trusted_transition()?;
    let witness_engine =
        PhaseCounterDelta::between("canonical_witness", &after_checkpoint, &after_witness)?;
    witness_engine.engine.verify_trusted_read_only()?;
    let storage_engine =
        PhaseCounterDelta::between("storage_observation", &after_witness, &after_storage)?;
    storage_engine.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        PhaseCounterDelta::operation_only(
            "native_edit",
            &native,
            before_storage.active_connections,
        ),
        checkpoint_engine,
        witness_engine,
        storage_engine,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let operation = combine_physical_checkpoint(native, checkpoint)?;
    let resources = observe_row_resources(Some(work), after_storage.active_connections)?;
    let counter_wall = counter_started.elapsed().as_nanos();
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "native_edit",
            wall_ns: native_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: physical_wall,
        },
        Phase {
            name: "durable_checkpoint",
            wall_ns: checkpoint_wall,
        },
        Phase {
            name: "canonical_witness",
            wall_ns: canonical_wall,
        },
        Phase {
            name: "counter_snapshot",
            wall_ns: counter_wall,
        },
    ];
    roots.push(post_ref.clone());
    metadata.push(current_metadata);
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: edit.before_bytes,
        after_bytes: edit.after_bytes,
        edit: Some(edit.clone()),
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(pre_ref),
        post_ref: Some(post_ref),
        native_route: native_route_name(operation.native.route).to_owned(),
        tree_level_before: Some(tree_level),
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before_storage),
        storage_after: Some(after_storage),
        resources,
        oracle: OracleReceipt {
            logical_length: edit.after_bytes,
            content_digest: canonical_digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })
}

fn history_root_indices(session: u8) -> EvalResult<&'static [usize]> {
    match session {
        1 => Ok(&[0, 5]),
        2 => Ok(&[0, 5, 10]),
        3 => Ok(&[0, 5, 10, 15]),
        4 => Ok(&[0, 15, 20]),
        5 => Ok(&[0, 15, 20, 25]),
        6 => Ok(&[0, 15, 20, 25, 30]),
        _ => Err(format!("invalid history session {session}")),
    }
}

fn history_custody_json(session: u8) -> EvalResult<String> {
    let indices = history_root_indices(session)?;
    Ok(format!(
        "{{\"head\":\"R{}\",\"roots\":[{}]}}",
        usize::from(session) * 5,
        indices
            .iter()
            .map(|root| format!("\"R{root}\""))
            .collect::<Vec<_>>()
            .join(",")
    ))
}

#[allow(clippy::too_many_arguments)]
fn run_history_row(
    campaign: &mut Campaign<'_>,
    retained: &LayerFs,
    store: &Path,
    roots: &[RefState],
    snapshots: &[PieceTable],
    backing: &[u8],
    session: u8,
    work: &Path,
) -> EvalResult<()> {
    let id = if session <= 3 {
        format!("C04-{session:03}")
    } else {
        format!("C06-{:03}", session - 3)
    };
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    set_failure_phase("verified_open");
    let verified_started = Instant::now();
    let opened = LayerFs::open(store).map_err(display_error)?;
    let verified_wall = verified_started.elapsed().as_nanos();
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    let head_index = usize::from(session) * 5;
    if opened.ref_state != roots[head_index] {
        return Err(format!("history session {session} recovered exact head"));
    }
    let before = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("history_read");
    let history_started = Instant::now();
    let mut operation = layerfs_sdk::OperationDiagnostics::default();
    let mut history_probes = Vec::new();
    for &root_index in history_root_indices(session)? {
        let table = snapshots
            .get(root_index)
            .ok_or_else(|| format!("missing oracle snapshot R{root_index}"))?;
        let probe_length = 65_536_u64;
        let middle = table.logical_length / 2 - probe_length / 2;
        let end = table
            .logical_length
            .checked_sub(probe_length)
            .ok_or_else(|| "history end probe underflow".to_owned())?;
        for (ordinal, start) in [0, middle, end].into_iter().enumerate() {
            let probe_before = opened.fs.counter_snapshot().map_err(display_error)?;
            let probe_started = Instant::now();
            let counters = compare_canonical_range(
                &opened.fs,
                roots[root_index].root,
                table,
                backing,
                start,
                probe_length,
            )?;
            let probe_wall_ns = probe_started.elapsed().as_nanos();
            let probe_after = opened.fs.counter_snapshot().map_err(display_error)?;
            let probe_engine = EngineDelta::between(&probe_before, &probe_after)?;
            probe_engine.verify_read_only()?;
            let content = content_counters(&counters)?;
            if counters.rope.payload_bytes_read != probe_length
                || counters.native != Default::default()
                || content.cdc_bytes_scanned != 0
                || content.payload_bytes_written != 0
                || (ordinal == 0
                    && (counters.namespace.nodes_read == 0 || counters.inode_table.nodes_read == 0))
                || (ordinal != 0
                    && (counters.namespace.nodes_read != 0 || counters.inode_table.nodes_read != 0))
            {
                return Err(format!(
                    "history R{root_index} probe {} plan/payload/read-only equation",
                    ordinal + 1
                ));
            }
            history_probes.push(HistoryProbeReceipt {
                root_index,
                ordinal: u8::try_from(ordinal + 1).map_err(display_error)?,
                start,
                length: probe_length,
                wall_ns: probe_wall_ns,
                engine: probe_engine,
                operation: counters,
            });
            operation = operation.merge(counters).map_err(display_error)?;
        }
    }
    let digest = campaign
        .root_digests
        .get(head_index)
        .cloned()
        .ok_or_else(|| format!("missing retained full-byte digest R{head_index}"))?;
    let history_wall = history_started.elapsed().as_nanos();
    let after_history = opened.fs.counter_snapshot().map_err(display_error)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    let engine_start = Diagnostics::default();
    let engine = EngineDelta::between(&engine_start, &after)?;
    engine.verify_read_only()?;
    let open_engine = PhaseCounterDelta::between("verified_open", &engine_start, &after_open)?;
    open_engine.engine.verify_read_only()?;
    let storage_before = PhaseCounterDelta::between("storage_observation", &after_open, &before)?;
    storage_before.engine.verify_read_only()?;
    let history_engine = PhaseCounterDelta::between("history_read", &before, &after_history)?;
    history_engine.engine.verify_read_only()?;
    let storage_after = PhaseCounterDelta::between("storage_observation", &after_history, &after)?;
    storage_after.engine.verify_read_only()?;
    let probe_engine = history_probes
        .iter()
        .try_fold(EngineDelta::default(), |aggregate, probe| {
            aggregate.combine(probe.engine)
        })?;
    let probe_operation = history_probes.iter().try_fold(
        layerfs_sdk::OperationDiagnostics::default(),
        |aggregate, probe| aggregate.merge(probe.operation).map_err(display_error),
    )?;
    if probe_engine != history_engine.engine
        || counters_json(Some(probe_engine), Some(&probe_operation))?
            != counters_json(Some(history_engine.engine), Some(&operation))?
    {
        return Err(format!(
            "history session {session} probe counters sum to retained row"
        ));
    }
    let phase_counters = vec![open_engine, storage_before, history_engine, storage_after];
    verify_phase_partition(&phase_counters, engine)?;
    if operation.native.bytes_read != 0
        || operation.native.bytes_written != 0
        || content_counters(&operation)?.cdc_bytes_scanned != 0
    {
        return Err(format!("history session {session} uses no native/CDC work"));
    }
    let active_connections = after
        .active_connections
        .checked_add(
            retained
                .counter_snapshot()
                .map_err(display_error)?
                .active_connections,
        )
        .ok_or_else(|| "history active connection count overflow".to_owned())?;
    let resources = if session == 1 {
        let external = observe_external_resources(Some(work), Some(store))?;
        if external.active_store_connections != active_connections {
            return Err("history SDK/external active connection equality".to_owned());
        }
        external
    } else {
        observe_row_resources(Some(work), active_connections)?
    };
    drop(opened);
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "verified_open",
            wall_ns: verified_wall,
        },
        Phase {
            name: "history_read",
            wall_ns: history_wall,
        },
    ];
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: snapshots[head_index].logical_length,
        after_bytes: snapshots[head_index].logical_length,
        edit: None,
        sub_edits: Vec::new(),
        history_probes,
        pre_ref: Some(roots[head_index].clone()),
        post_ref: Some(roots[head_index].clone()),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before),
        storage_after: Some(after),
        resources,
        oracle: OracleReceipt {
            logical_length: snapshots[head_index].logical_length,
            content_digest: digest,
            canonical_bytes_exact: Some(true),
            historical_roots_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(history_custody_json(session)?),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_logical_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    managed: &mut layerfs_sdk::ManagedWorkspace,
    edit: &EditSpec,
    table: &PieceTable,
    roots: &mut Vec<RefState>,
    metadata: &mut Vec<NativeMetadata>,
    work: &Path,
) -> EvalResult<()> {
    let id = format!("C05-{:03}", edit.serial - 15);
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let pre_ref = roots
        .last()
        .cloned()
        .ok_or_else(|| "logical transition has no pre-ref".to_owned())?;
    let before_storage = fs.diagnostics().map_err(display_error)?;
    let prior_metadata = metadata
        .last()
        .cloned()
        .ok_or_else(|| "logical transition has no metadata oracle".to_owned())?;
    let replacement = campaign
        .schedule
        .replacement_backing
        .get(
            edit.replacement_offset
                ..edit
                    .replacement_offset
                    .checked_add(usize::try_from(edit.insert_bytes).map_err(display_error)?)
                    .ok_or_else(|| "logical replacement range overflow".to_owned())?,
        )
        .ok_or_else(|| "logical replacement exceeds backing".to_owned())?;

    set_failure_phase("direct_logical_edit");
    let logical_started = Instant::now();
    let (accepted, logical) = fs
        .replace_range_for_refresh_observed(
            &pre_ref,
            FILE_PATH,
            edit.offset,
            edit.delete_bytes,
            Cursor::new(replacement),
        )
        .map_err(display_error)?;
    let post_ref = accepted.after().clone();
    let logical_wall = logical_started.elapsed().as_nanos();
    if post_ref.generation != pre_ref.generation + 1
        || fs.current_head("main").map_err(display_error)? != post_ref
    {
        return Err(format!("{} direct logical RefState", edit.tag));
    }
    let tree_level = logical
        .rope
        .tree_level_before
        .ok_or_else(|| format!("{} logical edit missing actual H", edit.tag))?;
    verify_locality(&logical, edit.insert_bytes, tree_level)?;
    let after_logical = fs.counter_snapshot().map_err(display_error)?;

    set_failure_phase("changed_root_refresh");
    let refresh_started = Instant::now();
    let refresh = managed.refresh_splice(&accepted).map_err(display_error)?;
    let refresh_wall = refresh_started.elapsed().as_nanos();
    verify_refresh(edit, &refresh)?;
    let after_refresh = fs.counter_snapshot().map_err(display_error)?;

    set_failure_phase("live_physical_oracle");
    let physical_started = Instant::now();
    let (physical_digest, _) =
        compare_managed(managed, table, &campaign.schedule.replacement_backing)?;
    let actual_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&actual_metadata, &edit.tag)?;
    if !metadata_exact(&actual_metadata, &prior_metadata) {
        return Err(format!("{} refresh preserved exact metadata", edit.tag));
    }
    let physical_wall = physical_started.elapsed().as_nanos();
    campaign.physical_oracles += 1;

    set_failure_phase("canonical_witness");
    let canonical_started = Instant::now();
    let (canonical_digest, _) = compare_canonical(
        fs,
        post_ref.root,
        table,
        &campaign.schedule.replacement_backing,
    )?;
    let canonical_wall = canonical_started.elapsed().as_nanos();
    if canonical_digest != physical_digest {
        return Err(format!("{} logical/physical canonical digest", edit.tag));
    }
    let after_witness = fs.counter_snapshot().map_err(display_error)?;
    campaign.canonical_transitions += 1;

    set_failure_phase("counter_snapshot");
    let counter_started = Instant::now();
    let after_storage = fs.diagnostics().map_err(display_error)?;
    verify_storage_transition(&before_storage, &after_storage)?;
    let engine = EngineDelta::between(&before_storage, &after_storage)?;
    engine.verify_trusted_transition()?;
    let logical_engine =
        PhaseCounterDelta::between("logical_edit", &before_storage, &after_logical)?;
    logical_engine.engine.verify_trusted_transition()?;
    let refresh_engine =
        PhaseCounterDelta::between("apfs_refresh", &after_logical, &after_refresh)?
            .with_operation_scratch(&refresh);
    refresh_engine.engine.verify_trusted_read_only()?;
    let witness_engine =
        PhaseCounterDelta::between("canonical_witness", &after_refresh, &after_witness)?;
    witness_engine.engine.verify_trusted_read_only()?;
    let storage_engine =
        PhaseCounterDelta::between("storage_observation", &after_witness, &after_storage)?;
    storage_engine.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        logical_engine,
        refresh_engine,
        witness_engine,
        storage_engine,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let operation = combine_logical_refresh(logical, refresh)?;
    let resources = observe_row_resources(Some(work), after_storage.active_connections)?;
    let counter_wall = counter_started.elapsed().as_nanos();
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "direct_logical_edit",
            wall_ns: logical_wall,
        },
        Phase {
            name: "changed_root_refresh",
            wall_ns: refresh_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: physical_wall,
        },
        Phase {
            name: "canonical_witness",
            wall_ns: canonical_wall,
        },
        Phase {
            name: "counter_snapshot",
            wall_ns: counter_wall,
        },
    ];
    roots.push(post_ref.clone());
    metadata.push(prior_metadata);
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: edit.before_bytes,
        after_bytes: edit.after_bytes,
        edit: Some(edit.clone()),
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(pre_ref),
        post_ref: Some(post_ref),
        native_route: native_route_name(operation.native.route).to_owned(),
        tree_level_before: Some(tree_level),
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before_storage),
        storage_after: Some(after_storage),
        resources,
        oracle: OracleReceipt {
            logical_length: edit.after_bytes,
            content_digest: canonical_digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_burst_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    managed: &mut layerfs_sdk::ManagedWorkspace,
    burst: &BurstSpec,
    before_table: &PieceTable,
    expected_table: &PieceTable,
    roots: &mut Vec<RefState>,
    metadata: &mut Vec<NativeMetadata>,
    work: &Path,
) -> EvalResult<()> {
    let id = format!("C07-{:03}", burst.root - 30);
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let pre_ref = roots
        .last()
        .cloned()
        .ok_or_else(|| "burst has no pre-ref".to_owned())?;
    let before_storage = fs.diagnostics().map_err(display_error)?;
    let mut table = before_table.clone();
    let mut native_aggregate = layerfs_sdk::OperationDiagnostics::default();
    let mut sub_edits = Vec::new();
    let mut native_wall = 0_u128;
    let mut physical_wall = 0_u128;
    for edit in &burst.edits {
        let replacement = campaign
            .schedule
            .replacement_backing
            .get(
                edit.replacement_offset
                    ..edit
                        .replacement_offset
                        .checked_add(usize::try_from(edit.insert_bytes).map_err(display_error)?)
                        .ok_or_else(|| "burst replacement range overflow".to_owned())?,
            )
            .ok_or_else(|| "burst replacement exceeds backing".to_owned())?;
        set_failure_phase("native_edit");
        let native_started = Instant::now();
        let native = managed
            .replace_observed(FILE_PATH, edit.offset, edit.delete_bytes, replacement)
            .map_err(display_error)?;
        let one_native_wall = native_started.elapsed().as_nanos();
        native_wall = native_wall
            .checked_add(one_native_wall)
            .ok_or_else(|| "burst native wall overflow".to_owned())?;
        verify_native_edit(edit, &native)?;
        table.splice(edit)?;
        set_failure_phase("live_physical_oracle");
        let oracle_started = Instant::now();
        compare_managed(managed, &table, &campaign.schedule.replacement_backing)?;
        let one_oracle_wall = oracle_started.elapsed().as_nanos();
        physical_wall = physical_wall
            .checked_add(one_oracle_wall)
            .ok_or_else(|| "burst oracle wall overflow".to_owned())?;
        campaign.physical_oracles += 1;
        sub_edits.push(SubEditReceipt {
            edit: edit.clone(),
            native_wall_ns: one_native_wall,
            physical_oracle_wall_ns: one_oracle_wall,
            native_route: native_route_name(native.native.route).to_owned(),
            native_bytes_read: native.native.bytes_read,
            native_bytes_written: native.native.bytes_written,
            native_patch_bytes: native.native.patch_bytes,
            native_suffix_bytes_shifted: native.native.suffix_bytes_shifted,
            native_clone_attempts: native.native.clone_attempts,
            native_clone_successes: native.native.clone_successes,
            native_clone_fallbacks: native.native.clone_fallbacks,
            native_full_fallback_files: native.full_fallback_files,
            tree_level_before: None,
            locality: None,
        });
        native_aggregate = native_aggregate.merge(native).map_err(display_error)?;
    }
    if &table != expected_table {
        return Err(format!("R{} ordered burst oracle table", burst.root));
    }
    let current_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&current_metadata, &format!("R{} burst", burst.root))?;

    set_failure_phase("durable_checkpoint");
    let checkpoint_started = Instant::now();
    let (post_ref, checkpoint, replay_steps) = managed
        .checkpoint_observed_detailed()
        .map_err(display_error)?;
    let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
    if post_ref.generation != pre_ref.generation + 1
        || fs.current_head("main").map_err(display_error)? != post_ref
        || checkpoint.workspace_reuses != 1
        || checkpoint.rematerializations != 0
        || checkpoint.descriptor_resets != 1
    {
        return Err(format!("R{} burst checkpoint closure", burst.root));
    }
    if replay_steps.len() != burst.edits.len() {
        return Err(format!(
            "R{} replay step count {} != {}",
            burst.root,
            replay_steps.len(),
            burst.edits.len()
        ));
    }
    for ((edit, receipt), step) in burst
        .edits
        .iter()
        .zip(sub_edits.iter_mut())
        .zip(replay_steps.iter())
    {
        let step_level = step
            .tree_level_before
            .ok_or_else(|| format!("{} missing replay tree level", edit.tag))?;
        let locality = verify_locality(&step.counters, edit.insert_bytes, step_level)?;
        receipt.tree_level_before = Some(step_level);
        receipt.locality = Some(locality);
    }
    verify_burst_locality(&checkpoint, &burst.edits, &replay_steps)?;
    let after_checkpoint = fs.counter_snapshot().map_err(display_error)?;

    set_failure_phase("canonical_witness");
    let canonical_started = Instant::now();
    let (digest, _) = compare_canonical(
        fs,
        post_ref.root,
        expected_table,
        &campaign.schedule.replacement_backing,
    )?;
    let canonical_wall = canonical_started.elapsed().as_nanos();
    let after_witness = fs.counter_snapshot().map_err(display_error)?;
    campaign.canonical_transitions += 1;

    set_failure_phase("counter_snapshot");
    let counter_started = Instant::now();
    let after_storage = fs.diagnostics().map_err(display_error)?;
    verify_storage_transition(&before_storage, &after_storage)?;
    let engine = EngineDelta::between(&before_storage, &after_storage)?;
    engine.verify_trusted_transition()?;
    let checkpoint_engine =
        PhaseCounterDelta::between("checkpoint", &before_storage, &after_checkpoint)?;
    checkpoint_engine.engine.verify_trusted_transition()?;
    let witness_engine =
        PhaseCounterDelta::between("canonical_witness", &after_checkpoint, &after_witness)?;
    witness_engine.engine.verify_trusted_read_only()?;
    let storage_engine =
        PhaseCounterDelta::between("storage_observation", &after_witness, &after_storage)?;
    storage_engine.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        PhaseCounterDelta::operation_only(
            "native_edit",
            &native_aggregate,
            before_storage.active_connections,
        ),
        checkpoint_engine,
        witness_engine,
        storage_engine,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let operation = combine_physical_checkpoint(native_aggregate, checkpoint)?;
    let resources = observe_row_resources(Some(work), after_storage.active_connections)?;
    let counter_wall = counter_started.elapsed().as_nanos();
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "native_edit",
            wall_ns: native_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: physical_wall,
        },
        Phase {
            name: "durable_checkpoint",
            wall_ns: checkpoint_wall,
        },
        Phase {
            name: "canonical_witness",
            wall_ns: canonical_wall,
        },
        Phase {
            name: "counter_snapshot",
            wall_ns: counter_wall,
        },
    ];
    roots.push(post_ref.clone());
    metadata.push(current_metadata);
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: burst.edits[0].before_bytes,
        after_bytes: burst
            .edits
            .last()
            .ok_or_else(|| "empty burst".to_owned())?
            .after_bytes,
        edit: None,
        sub_edits,
        history_probes: Vec::new(),
        pre_ref: Some(pre_ref),
        post_ref: Some(post_ref),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before_storage),
        storage_after: Some(after_storage),
        resources,
        oracle: OracleReceipt {
            logical_length: expected_table.logical_length,
            content_digest: digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_milestone_row(
    campaign: &mut Campaign<'_>,
    retained: &LayerFs,
    store: &Path,
    root_index: u8,
    roots: &[RefState],
    metadata: &[NativeMetadata],
    snapshots: &[PieceTable],
    backing: &[u8],
    managed: &mut Option<layerfs_sdk::ManagedWorkspace>,
    converted: &mut Option<layerfs_sdk::ExternalWorkspace>,
    work: &Path,
) -> EvalResult<()> {
    let ordinal = match root_index {
        15 => 1,
        30 => 2,
        34 => 3,
        _ => return Err(format!("invalid materialization root R{root_index}")),
    };
    let id = format!("C08-{ordinal:03}");
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let mut phases = Vec::new();
    let mut live_digest = None;
    let mut live_extra_user_files = None;
    let mut live_metadata_receipt = None;
    if root_index == 34 {
        set_failure_phase("live_physical_oracle");
        let live_started = Instant::now();
        let external = managed
            .take()
            .ok_or_else(|| "R34 managed workspace already converted".to_owned())?
            .into_external()
            .map_err(display_error)?;
        verify_single_file_destination(external.path())?;
        live_extra_user_files = Some(0_u8);
        let digest = compare_external(&external, &snapshots[34], backing)?;
        let live_metadata = external.read_metadata(FILE_PATH).map_err(display_error)?;
        verify_supported_metadata(&live_metadata, "live R34")?;
        if !metadata_exact(&live_metadata, &metadata[34]) {
            return Err("live R34 metadata = retained R34 metadata".to_owned());
        }
        live_metadata_receipt = Some(live_metadata);
        live_digest = Some(digest);
        *converted = Some(external);
        phases.push(Phase {
            name: "live_physical_oracle",
            wall_ns: live_started.elapsed().as_nanos(),
        });
    }

    set_failure_phase("verified_open");
    let verified_started = Instant::now();
    let opened = LayerFs::open(store).map_err(display_error)?;
    let verified_wall = verified_started.elapsed().as_nanos();
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    phases.push(Phase {
        name: "verified_open",
        wall_ns: verified_wall,
    });
    let before = opened.fs.diagnostics().map_err(display_error)?;
    let destination = work.join(format!("milestone-R{root_index}"));
    set_failure_phase("milestone_materialization");
    let materialize_started = Instant::now();
    let (mut external, mut operation) = opened
        .fs
        .materialize_external_observed(roots[usize::from(root_index)].root, &destination)
        .map_err(display_error)?;
    let materialize_wall = materialize_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "milestone_materialization",
        wall_ns: materialize_wall,
    });
    set_failure_phase("metadata_oracle");
    let oracle_started = Instant::now();
    let digest = compare_external(&external, &snapshots[usize::from(root_index)], backing)?;
    let actual_metadata = external.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&actual_metadata, &format!("fresh R{root_index}"))?;
    if !metadata_exact(&actual_metadata, &metadata[usize::from(root_index)])
        || live_digest.as_ref().is_some_and(|live| live != &digest)
    {
        return Err(format!(
            "R{root_index} materialization byte/metadata oracle"
        ));
    }
    verify_single_file_destination(&destination)?;
    let oracle_wall = oracle_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "metadata_oracle",
        wall_ns: oracle_wall,
    });
    let after_materialize = opened.fs.counter_snapshot().map_err(display_error)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("explicit_cleanup");
    let cleanup_started = Instant::now();
    let cleanup = external.discard_observed().map_err(display_error)?;
    operation = operation.merge(cleanup).map_err(display_error)?;
    drop(external);
    fs::remove_dir_all(&destination).map_err(io_error)?;
    if destination.exists() {
        return Err(format!("R{root_index} milestone cleanup residue = 0"));
    }
    let cleanup_wall = cleanup_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "explicit_cleanup",
        wall_ns: cleanup_wall,
    });
    let engine_start = Diagnostics::default();
    let engine = EngineDelta::between(&engine_start, &after)?;
    engine.verify_read_only()?;
    let open_engine = PhaseCounterDelta::between("verified_open", &engine_start, &after_open)?;
    open_engine.engine.verify_read_only()?;
    let storage_before = PhaseCounterDelta::between("storage_observation", &after_open, &before)?;
    storage_before.engine.verify_read_only()?;
    let materialize_engine =
        PhaseCounterDelta::between("materialization", &before, &after_materialize)?
            .with_operation_scratch(&operation);
    materialize_engine.engine.verify_read_only()?;
    let storage_after =
        PhaseCounterDelta::between("storage_observation", &after_materialize, &after)?;
    storage_after.engine.verify_read_only()?;
    let phase_counters = vec![
        open_engine,
        storage_before,
        materialize_engine,
        storage_after,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let active_connections = after
        .active_connections
        .checked_add(
            retained
                .counter_snapshot()
                .map_err(display_error)?
                .active_connections,
        )
        .ok_or_else(|| "milestone active connection count overflow".to_owned())?;
    let resources = observe_row_resources(Some(work), active_connections)?;
    drop(opened);
    let row_wall = row_started.elapsed().as_nanos();
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: snapshots[usize::from(root_index)].logical_length,
        after_bytes: snapshots[usize::from(root_index)].logical_length,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(roots[usize::from(root_index)].clone()),
        post_ref: Some(roots[usize::from(root_index)].clone()),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before),
        storage_after: Some(after),
        resources,
        oracle: OracleReceipt {
            logical_length: snapshots[usize::from(root_index)].logical_length,
            content_digest: digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            historical_roots_exact: Some(true),
            route_exact: Some(true),
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(format!(
            concat!(
                "{{\"milestone_root\":\"R{}\",",
                "\"extra_user_files\":0,\"fresh_extra_user_files\":0,",
                "\"live_extra_user_files\":{},\"cleanup_residue_entries\":0,",
                "\"metadata\":{},\"retained_metadata\":{},",
                "\"fresh_metadata\":{},\"live_metadata\":{}}}"
            ),
            root_index,
            live_extra_user_files.map_or_else(|| "null".to_owned(), |value| value.to_string()),
            metadata_receipt_json(&actual_metadata),
            metadata_receipt_json(&metadata[usize::from(root_index)]),
            metadata_receipt_json(&actual_metadata),
            live_metadata_receipt
                .as_ref()
                .map_or_else(|| "null".to_owned(), metadata_receipt_json,),
        )),
    })
}

fn verify_single_file_destination(destination: &Path) -> EvalResult<()> {
    let data = destination.join("data");
    let payload = data.join("payload.bin");
    if !data.is_dir() || !payload.is_file() {
        return Err("milestone destination is missing data/payload.bin".to_owned());
    }
    let root_entries = fs::read_dir(destination)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    let data_entries = fs::read_dir(&data)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    if root_entries.len() != 1
        || root_entries[0].file_name() != "data"
        || data_entries.len() != 1
        || data_entries[0].file_name() != "payload.bin"
        || fs::symlink_metadata(&payload)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
    {
        return Err("milestone destination extra user files = 0".to_owned());
    }
    Ok(())
}

fn terminal_work_residue_count(work: &Path) -> EvalResult<u64> {
    let entries = fs::read_dir(work)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    if !entries
        .iter()
        .any(|entry| entry.file_name() == "store" && entry.path().is_dir())
    {
        return Err("terminal work inventory is missing Store".to_owned());
    }
    Ok(entries
        .iter()
        .filter(|entry| entry.file_name() != "store")
        .count() as u64)
}

fn run_terminal_row(
    campaign: &mut Campaign<'_>,
    fs: LayerFs,
    mut converted: Option<layerfs_sdk::ExternalWorkspace>,
    work: &Path,
    fixture: &Path,
    master: &FixtureMaster,
) -> EvalResult<()> {
    let schedule = campaign.scheduled("C09-001")?;
    let row_started = Instant::now();
    set_failure_phase("explicit_cleanup");
    let cleanup_started = Instant::now();
    let cleanup = converted.as_mut().map_or_else(
        || Ok(layerfs_sdk::OperationDiagnostics::default()),
        |external| external.discard_observed().map_err(display_error),
    )?;
    drop(converted);
    drop(fs);
    let store = work.join("store");
    let mut before_cleanup = observe_external_resources(Some(work), Some(&store))?;
    before_cleanup.residue_entries = before_cleanup
        .residue_entries
        .checked_add(terminal_work_residue_count(work)?)
        .ok_or_else(|| "terminal residue count overflow".to_owned())?;
    if before_cleanup.active_store_connections != 0
        || before_cleanup.fd_current != campaign.fd_baseline
        || before_cleanup.child_processes != 0
        || before_cleanup.residue_entries != 0
    {
        return Err(format!(
            concat!(
                "pre-deletion terminal closure connections={} fd={}/{} ",
                "children={} residue={}"
            ),
            before_cleanup.active_store_connections,
            campaign.fd_baseline,
            before_cleanup.fd_current,
            before_cleanup.child_processes,
            before_cleanup.residue_entries,
        ));
    }
    if work.exists() {
        stage1_fixture::make_writable(work)?;
        fs::remove_dir_all(work).map_err(io_error)?;
    }
    verify_fixture(fixture, master, true)?;
    let cleanup_wall = cleanup_started.elapsed().as_nanos();
    let mut resources = observe_external_resources(Some(work), None)?;
    resources.owned_temp_entries = Some(0);
    if campaign.rss_peak_bytes.max(resources.rss_peak_bytes) > 33_554_432
        || campaign.q_high_water_bytes > 8_388_608
        || campaign.q_maximum_terminal_bytes != 0
        || campaign.store_connection_high_water > 2
        || resources.active_store_connections != 0
        || resources.fd_current != campaign.fd_baseline
        || resources.child_processes != 0
        || resources.residue_entries != 0
    {
        return Err(format!(
            concat!(
                "terminal resource closure rss={} q={}/{} connections={}/{} ",
                "fd={}/{} children={} residue={}"
            ),
            campaign.rss_peak_bytes.max(resources.rss_peak_bytes),
            campaign.q_high_water_bytes,
            campaign.q_maximum_terminal_bytes,
            campaign.store_connection_high_water,
            resources.active_store_connections,
            campaign.fd_baseline,
            resources.fd_current,
            resources.child_processes,
            resources.residue_entries,
        ));
    }
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![Phase {
        name: "explicit_cleanup",
        wall_ns: cleanup_wall,
    }];
    let phase_counters = vec![PhaseCounterDelta::operation_only(
        "explicit_cleanup",
        &cleanup,
        0,
    )];
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(EngineDelta::default()),
        operation: Some(cleanup),
        storage_before: None,
        storage_after: None,
        resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: String::new(),
            ..OracleReceipt::default()
        },
        unavailable: {
            let mut unavailable = unavailable_defaults();
            unavailable.push(Unavailable {
                field: "oracle.content_digest".to_owned(),
                availability: "NotApplicable",
                reason: "workspace authority was discarded before terminal resource observation"
                    .to_owned(),
            });
            unavailable
        },
        error: None,
        custody: Some(format!(
            concat!(
                "{{\"pre_cleanup_active_store_connections\":{},",
                "\"pre_cleanup_fd_count\":{},\"pre_cleanup_child_processes\":{},",
                "\"pre_cleanup_residue_entries\":{},",
                "\"post_cleanup_active_store_connections\":{},",
                "\"post_cleanup_fd_count\":{},\"post_cleanup_child_processes\":{},",
                "\"post_cleanup_residue_entries\":{},",
                "\"fixture_unchanged\":true}}"
            ),
            before_cleanup.active_store_connections,
            before_cleanup.fd_current,
            before_cleanup.child_processes,
            before_cleanup.residue_entries,
            resources.active_store_connections,
            resources.fd_current,
            resources.child_processes,
            resources.residue_entries,
        )),
    })
}

fn admit_readiness(
    json: &str,
    source: &SourceIdentity,
    master: &FixtureMaster,
    schedule: &str,
) -> EvalResult<()> {
    if json_string(json, "schema")? != READINESS_SCHEMA
        || json_string(json, "status")? != "PASS"
        || json_bool(json, "measured_rows_started")?
        || json_bool(json, "run_directory_exists")?
        || json_u128(json, "expected_rows")? != 47
        || json_u128(json, "edit_suboperations")? != 51
        || json_u128(json, "transitions")? != 34
        || json_u128(json, "reset_wall_ns")? > RESET_LIMIT_NS
        || json_u128(json, "forecast_campaign_wall_ns")? >= CAMPAIGN_LIMIT_NS
        || json_u128(json, "hard_limit_ns")? != CAMPAIGN_LIMIT_NS
        || json_string(json, "source_tree_blake3")? != source.tree_blake3
        || json_string(json, "source_manifest_sha256")? != source.manifest_sha256
        || json_string(json, "executable_path")? != source.executable_path.display().to_string()
        || json_string(json, "executable_sha256")? != source.executable_sha256
        || json_string(json, "executable_blake3")? != source.executable_blake3
        || json_string(json, "fixture_master_sha256")?
            != sha256_file(&fixture_root().join("master.json"))?
        || json_string(json, "fixture_blake3")? != master.fixture_blake3
        || json_string(json, "schedule_sha256")? != sha256_bytes(schedule.as_bytes())?
        || json_string(json, "store_id")? != master.store_id
        || json_string(json, "profile")? != master.profile
        || json_string(json, "apfs_identity")? != master.apfs_identity
        || json_string(json, "git_commit")? != source.git_commit
        || json_bool(json, "dirty_tree")? != source.dirty_tree
    {
        return Err(
            "readiness receipt does not bind this exact source/executable/fixture/schedule"
                .to_owned(),
        );
    }
    Ok(())
}

fn environment_json(source: &SourceIdentity, master: &FixtureMaster) -> String {
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-environment-v1\",",
            "\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"release_executable_path\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"store_id\":\"{}\",",
            "\"profile\":\"{}\",\"network_operations\":0,",
            "\"product_operation_child_processes\":0,",
            "\"command\":\"layerfs-eval stage1 run apple-edge <new-run-directory>\"}}\n"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        json_escape(&master.apfs_identity),
        master.store_id,
        json_escape(&master.profile),
    )
}

#[derive(Clone, Debug)]
struct ParsedRow {
    json: String,
    row_id: String,
    row_group: String,
    operation: String,
    size_band: String,
    native_route: String,
    status: String,
    before_bytes: u64,
    after_bytes: u64,
    row_wall_ns: u128,
    row_residual_ns: u128,
}

fn revision_length(schedule: &FrozenSchedule, revision: u8) -> EvalResult<u64> {
    match revision {
        0 => Ok(INITIAL_BYTES),
        1..=30 => schedule
            .edits
            .get(usize::from(revision - 1))
            .map(|edit| edit.after_bytes)
            .ok_or_else(|| format!("missing frozen revision R{revision}")),
        31..=34 => schedule
            .bursts
            .get(usize::from(revision - 31))
            .and_then(|burst| burst.edits.last())
            .map(|edit| edit.after_bytes)
            .ok_or_else(|| format!("missing frozen burst revision R{revision}")),
        _ => Err(format!("invalid frozen revision R{revision}")),
    }
}

fn scheduled_lengths(schedule: &FrozenSchedule, row: &ScheduledRow) -> EvalResult<(u64, u64)> {
    if let Some(index) = row.edit_index {
        let edit = schedule
            .edits
            .get(index)
            .ok_or_else(|| format!("{} missing edit {index}", row.row_id))?;
        Ok((edit.before_bytes, edit.after_bytes))
    } else if let Some(index) = row.burst_index {
        let burst = schedule
            .bursts
            .get(index)
            .ok_or_else(|| format!("{} missing burst {index}", row.row_id))?;
        Ok((
            burst
                .edits
                .first()
                .ok_or_else(|| format!("{} empty burst", row.row_id))?
                .before_bytes,
            burst
                .edits
                .last()
                .ok_or_else(|| format!("{} empty burst", row.row_id))?
                .after_bytes,
        ))
    } else if let Some(session) = row.history_session {
        let length = revision_length(schedule, session * 5)?;
        Ok((length, length))
    } else if let Some(root) = row.milestone_root {
        let length = revision_length(schedule, root)?;
        Ok((length, length))
    } else {
        Ok((INITIAL_BYTES, INITIAL_BYTES))
    }
}

fn parse_rows(path: &Path, schedule: &FrozenSchedule) -> EvalResult<Vec<ParsedRow>> {
    let contents = fs::read_to_string(path).map_err(io_error)?;
    if !contents.ends_with('\n') {
        return Err("rows.jsonl is not newline terminated".to_owned());
    }
    let mut rows = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        let expected = schedule
            .rows
            .get(index)
            .ok_or_else(|| "rows.jsonl has too many rows".to_owned())?;
        let status = json_top_level_string(line, "status")?;
        let (expected_before, expected_after) = scheduled_lengths(schedule, expected)?;
        if json_top_level_string(line, "schema")? != "layerfs-stage1.1-row-v1"
            || json_top_level_u128(line, "row_index")? != index as u128
            || json_top_level_string(line, "row_id")? != expected.row_id
            || json_top_level_string(line, "row_group")? != expected.row_group
            || json_top_level_u128(line, "sequence")? != u128::from(expected.sequence)
            || json_top_level_u128(line, "epoch")? != u128::from(expected.epoch)
            || json_top_level_string(line, "direction")? != expected.direction
            || json_top_level_string(line, "operation")? != expected.operation
            || json_top_level_string(line, "size_band")? != expected.size_band
            || json_top_level_u128(line, "before_bytes")? != u128::from(expected_before)
            || json_top_level_u128(line, "after_bytes")? != u128::from(expected_after)
            || !matches!(status.as_str(), "PASS" | "REVISE" | "FAIL")
        {
            return Err(format!("invalid retained row at index {index}"));
        }
        let top_level = json_object_member_names(line)?;
        for key in [
            "schema",
            "row_index",
            "row_id",
            "row_group",
            "sequence",
            "epoch",
            "direction",
            "operation",
            "size_band",
            "status",
            "before_bytes",
            "after_bytes",
            "edit",
            "sub_edits",
            "history_probes",
            "pre_ref",
            "post_ref",
            "native_route",
            "tree_level_before",
            "phases",
            "phase_counters",
            "row_wall_ns",
            "row_residual_ns",
            "counters",
            "native",
            "storage",
            "resources",
            "oracle",
            "unavailable",
            "error",
        ] {
            if !top_level.iter().any(|actual| actual == key) {
                return Err(format!(
                    "row {} missing common field {key}",
                    expected.row_id
                ));
            }
        }
        rows.push(ParsedRow {
            json: line.to_owned(),
            row_id: expected.row_id.clone(),
            row_group: json_top_level_string(line, "row_group")?,
            operation: json_top_level_string(line, "operation")?,
            size_band: json_top_level_string(line, "size_band")?,
            native_route: json_top_level_string(line, "native_route")?,
            status,
            before_bytes: u64::try_from(json_top_level_u128(line, "before_bytes")?)
                .map_err(display_error)?,
            after_bytes: u64::try_from(json_top_level_u128(line, "after_bytes")?)
                .map_err(display_error)?,
            row_wall_ns: json_top_level_u128(line, "row_wall_ns")?,
            row_residual_ns: json_top_level_u128(line, "row_residual_ns")?,
        });
    }
    if rows.len() != 47 {
        return Err(format!("rows.jsonl contains {} rows, not 47", rows.len()));
    }
    Ok(rows)
}

fn phase_wall(json: &str, name: &str) -> EvalResult<u128> {
    let needle = format!("\"name\":\"{name}\",\"wall_ns\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing phase {name}"))?;
    parse_digits(&json[start..], &format!("phase {name}"))
}

fn json_all_u128(json: &str, key: &str) -> EvalResult<Vec<u128>> {
    let needle = format!("\"{key}\":");
    let mut values = Vec::new();
    let mut remaining = json;
    while let Some(offset) = remaining.find(&needle) {
        let start = offset + needle.len();
        values.push(parse_digits(&remaining[start..], key)?);
        remaining = &remaining[start..];
    }
    Ok(values)
}

fn parse_digits(value: &str, label: &str) -> EvalResult<u128> {
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("invalid integer for {label}"));
    }
    digits.parse().map_err(display_error)
}

fn json_object<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":{{");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len() - 1)
        .ok_or_else(|| format!("missing JSON object {key}"))?;
    let bytes = json.as_bytes();
    let mut depth = 0_u32;
    let mut string = false;
    let mut escaped = false;
    for (relative, byte) in bytes[start..].iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("JSON object {key} depth underflow"))?;
                if depth == 0 {
                    return Ok(&json[start..start + relative + 1]);
                }
            }
            _ => {}
        }
    }
    Err(format!("unterminated JSON object {key}"))
}

fn row_optional_u128(row: &ParsedRow, key: &str) -> EvalResult<Option<u128>> {
    let object = if matches!(
        key,
        "transactions_started"
            | "transactions_committed"
            | "transactions_rolled_back"
            | "statements"
            | "admission_transactions_started"
            | "admission_transactions_committed"
            | "admission_transactions_rolled_back"
            | "admission_statements"
            | "integrity_transactions_started"
            | "integrity_transactions_committed"
            | "integrity_transactions_rolled_back"
            | "integrity_statements"
            | "busy_events"
            | "locked_events"
            | "objects_validated"
            | "objects_created"
            | "objects_reused"
            | "object_bytes_read"
            | "object_bytes_written"
            | "fetched_rows"
            | "fetched_row_authentication_passes"
            | "fetched_row_role_decode_passes"
            | "new_object_authentication_passes"
            | "incumbent_authentication_passes"
            | "payload_batch_queries"
            | "payload_batch_references"
            | "payload_batch_maximum"
            | "put_lookup_statements"
            | "put_insert_statements"
            | "created_rows"
            | "reused_rows"
            | "publication_transactions_started"
            | "publication_transactions_rolled_back"
            | "publication_commits"
            | "publication_closure_passes"
            | "namespace_graph_verification_passes"
            | "scratch_tables"
            | "scratch_statements"
            | "scratch_rows"
            | "scratch_high_water_bytes"
            | "retained_roots_validated"
            | "cdc_bytes_scanned"
            | "payload_bytes_written"
            | "unaffected_payload_reads"
            | "unaffected_payload_writes"
            | "rope_nodes_read"
            | "rope_nodes_emitted"
            | "content_directory_nodes_emitted"
            | "workspace_materializations"
            | "workspace_reuses"
            | "rematerializations"
            | "descriptor_resets"
    ) {
        "counters"
    } else if matches!(
        key,
        "bytes_read"
            | "bytes_written"
            | "patch_bytes"
            | "suffix_bytes_shifted"
            | "clone_attempts"
            | "clone_successes"
            | "clone_fallbacks"
            | "full_fallback_files"
            | "files_created"
            | "files_replaced"
            | "files_removed"
            | "sync_regular_calls"
            | "sync_directory_calls"
    ) {
        "native"
    } else if matches!(
        key,
        "database_bytes"
            | "logical_engine_bytes"
            | "rollback_journal_bytes"
            | "temporary_file_bytes"
            | "database_growth_bytes"
            | "canonical_object_bytes_written"
            | "physical_to_canonical_amplification"
    ) {
        "storage"
    } else if matches!(
        key,
        "rss_current_bytes"
            | "rss_peak_bytes"
            | "operation_q_current_bytes"
            | "operation_q_high_water_bytes"
            | "operation_q_terminal_bytes"
            | "fd_current"
            | "active_store_connections"
            | "child_processes"
            | "owned_temp_entries"
            | "residue_entries"
            | "largest_buffer_bytes"
            | "page_size"
            | "cache_pages"
            | "cache_spill_pages"
            | "network_operations"
    ) {
        "resources"
    } else {
        return json_optional_u128(&row.json, key);
    };
    json_optional_u128(json_object(&row.json, object)?, key)
}

fn row_u128(row: &ParsedRow, key: &str) -> EvalResult<u128> {
    row_optional_u128(row, key)?.ok_or_else(|| format!("{} has null {key}", row.row_id))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedRefState {
    name: String,
    generation: u64,
    root: String,
}

fn row_ref(row: &ParsedRow, key: &str) -> EvalResult<ParsedRefState> {
    let object = json_object(&row.json, key)?;
    Ok(ParsedRefState {
        name: json_string(object, "name")?,
        generation: u64::try_from(json_u128(object, "generation")?).map_err(display_error)?,
        root: json_string(object, "root")?,
    })
}

fn validate_ref_chain(rows: &[ParsedRow], schedule: &FrozenSchedule) -> EvalResult<()> {
    let initial = rows
        .iter()
        .find(|row| row.row_group == "C02")
        .ok_or_else(|| "ref chain missing C02".to_owned())?;
    let mut previous = row_ref(initial, "post_ref")?;
    if previous.name != "main" || previous.generation != 1 {
        return Err("R0 RefState name=main; generation=1".to_owned());
    }
    let transitions = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    if transitions.len() != 34 {
        return Err(format!(
            "RefState transition rows {} != 34",
            transitions.len()
        ));
    }
    for (revision, row) in transitions.into_iter().enumerate() {
        let expected_revision = u8::try_from(revision + 1).map_err(display_error)?;
        if schedule.rows[row.schedule_index(schedule)?].transition_root != Some(expected_revision) {
            return Err(format!(
                "{} scheduled transition root R{expected_revision}",
                row.row_id
            ));
        }
        let pre = row_ref(row, "pre_ref")?;
        let post = row_ref(row, "post_ref")?;
        if pre != previous
            || pre.name != "main"
            || post.name != "main"
            || post.generation != pre.generation + 1
            || post.root == pre.root
        {
            return Err(format!(
                "{} RefState chain pre=previous; generation+1; name=main",
                row.row_id
            ));
        }
        previous = post;
    }
    Ok(())
}

impl ParsedRow {
    fn schedule_index(&self, schedule: &FrozenSchedule) -> EvalResult<usize> {
        schedule
            .rows
            .iter()
            .position(|row| row.row_id == self.row_id)
            .ok_or_else(|| format!("{} missing from schedule", self.row_id))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AuthenticationValidation {
    applicable_rows: u64,
    fetched_authentication_failures: u64,
    fetched_role_decode_failures: u64,
    new_object_equation_failures: u64,
    incumbent_equation_failures: u64,
    payload_batch_maximum: u64,
}

fn validate_authentication(rows: &[ParsedRow]) -> EvalResult<AuthenticationValidation> {
    let mut result = AuthenticationValidation::default();
    for row in rows {
        let Some(fetched) = row_optional_u128(row, "fetched_rows")? else {
            continue;
        };
        result.applicable_rows += 1;
        let authentication = row_u128(row, "fetched_row_authentication_passes")?;
        let role_decode = row_u128(row, "fetched_row_role_decode_passes")?;
        let new_auth = row_u128(row, "new_object_authentication_passes")?;
        let created = row_u128(row, "created_rows")?;
        let reused = row_u128(row, "reused_rows")?;
        let put_lookup = row_u128(row, "put_lookup_statements")?;
        let put_insert = row_u128(row, "put_insert_statements")?;
        let incumbent = row_u128(row, "incumbent_authentication_passes")?;
        let objects_validated = row_u128(row, "objects_validated")?;
        let objects_created = row_u128(row, "objects_created")?;
        let objects_reused = row_u128(row, "objects_reused")?;
        let payload_max = row_u128(row, "payload_batch_maximum")?;
        let trusted = matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07");
        if (row.row_group == "C02" && authentication != 0)
            || (trusted && authentication > fetched)
            || (!trusted && fetched != authentication)
        {
            result.fetched_authentication_failures += 1;
        }
        if fetched != role_decode {
            result.fetched_role_decode_failures += 1;
        }
        if new_auth != created + reused || new_auth != put_lookup {
            result.new_object_equation_failures += 1;
        }
        if incumbent != reused {
            result.incumbent_equation_failures += 1;
        }
        if objects_validated != role_decode + new_auth + incumbent {
            return Err(format!(
                "{} objects_validated authentication equation",
                row.row_id
            ));
        }
        if put_insert != created || objects_created != created || objects_reused != reused {
            return Err(format!(
                "{} put_insert=created; objects_created=created; objects_reused=reused",
                row.row_id
            ));
        }
        let transaction = (
            row_u128(row, "transactions_started")?,
            row_u128(row, "transactions_committed")?,
            row_u128(row, "transactions_rolled_back")?,
            row_u128(row, "publication_transactions_started")?,
            row_u128(row, "publication_commits")?,
            row_u128(row, "publication_transactions_rolled_back")?,
        );
        if matches!(row.row_group.as_str(), "C03" | "C05" | "C07") {
            if transaction != (1, 1, 0, 1, 1, 0) {
                return Err(format!("{} one transition transaction/COMMIT", row.row_id));
            }
        } else if transaction != (0, 0, 0, 0, 0, 0) {
            return Err(format!("{} read-only transaction closure", row.row_id));
        }
        if row_u128(row, "admission_transactions_started")?
            != row_u128(row, "admission_transactions_committed")?
                + row_u128(row, "admission_transactions_rolled_back")?
            || row_u128(row, "integrity_transactions_started")?
                != row_u128(row, "integrity_transactions_committed")?
                    + row_u128(row, "integrity_transactions_rolled_back")?
        {
            return Err(format!(
                "{} admission/integrity transaction closure",
                row.row_id
            ));
        }
        result.payload_batch_maximum = result
            .payload_batch_maximum
            .max(u64::try_from(payload_max).map_err(display_error)?);
    }
    if result.fetched_authentication_failures != 0
        || result.fetched_role_decode_failures != 0
        || result.new_object_equation_failures != 0
        || result.incumbent_equation_failures != 0
        || result.payload_batch_maximum > 64
    {
        return Err(format!("row authentication closure failed: {result:?}"));
    }
    Ok(result)
}

fn json_array_objects<'a>(json: &'a str, key: &str) -> EvalResult<Vec<&'a str>> {
    let needle = format!("\"{key}\":[");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON array {key}"))?;
    let bytes = json.as_bytes();
    let mut objects = Vec::new();
    let mut depth = 0_u32;
    let mut object_start = None;
    let mut string = false;
    let mut escaped = false;
    for (relative, byte) in bytes[start..].iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' => {
                if depth == 0 {
                    object_start = Some(start + relative);
                }
                depth += 1;
            }
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("JSON array {key} object underflow"))?;
                if depth == 0 {
                    let begin = object_start
                        .take()
                        .ok_or_else(|| format!("JSON array {key} missing object start"))?;
                    objects.push(&json[begin..start + relative + 1]);
                }
            }
            b']' if depth == 0 => return Ok(objects),
            _ => {}
        }
    }
    Err(format!("unterminated JSON array {key}"))
}

fn validate_locality_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    let individual = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .collect::<Vec<_>>();
    if individual.len() != 30 {
        return Err(format!(
            "individual locality rows {} != 30",
            individual.len()
        ));
    }
    for row in individual {
        let tree_level =
            u8::try_from(json_u128(&row.json, "tree_level_before")?).map_err(display_error)?;
        let replacement = json_u128(json_object(&row.json, "edit")?, "insert_bytes")?;
        let read_bound = 16_u128 * (u128::from(tree_level) + 1);
        let emitted_bound = read_bound + replacement.div_ceil(8_192) + 2;
        if row_u128(row, "cdc_bytes_scanned")? != replacement
            || row_u128(row, "payload_bytes_written")? != replacement
            || row_u128(row, "unaffected_payload_reads")? != 0
            || row_u128(row, "unaffected_payload_writes")? != 0
            || row_u128(row, "content_directory_nodes_emitted")? != 0
            || row_u128(row, "rope_nodes_read")? > read_bound
            || row_u128(row, "rope_nodes_emitted")? > emitted_bound
        {
            return Err(format!("{} retained locality/H equation", row.row_id));
        }
    }
    let mut subedit_count = 0_usize;
    for row in rows.iter().filter(|row| row.row_group == "C07") {
        let mut exact = ContentCounters::default();
        let mut native_exact = [0_u128; 8];
        for subedit in json_array_objects(&row.json, "sub_edits")? {
            subedit_count += 1;
            let required = [
                "tag",
                "offset",
                "delete_bytes",
                "insert_bytes",
                "replacement_digest",
                "before_bytes",
                "after_bytes",
                "native_wall_ns",
                "physical_oracle_wall_ns",
                "native_route",
                "native_bytes_read",
                "native_bytes_written",
                "native_patch_bytes",
                "native_suffix_bytes_shifted",
                "native_clone_attempts",
                "native_clone_successes",
                "native_clone_fallbacks",
                "native_full_fallback_files",
                "tree_level_before",
                "cdc_bytes_scanned",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "rope_nodes_read",
                "rope_nodes_emitted",
                "content_directory_nodes_emitted",
            ];
            if json_object_member_names(subedit)? != required {
                return Err(format!("{} exact flattened sub-edit schema", row.row_id));
            }
            let replacement = json_u128(subedit, "insert_bytes")?;
            let delete = json_u128(subedit, "delete_bytes")?;
            let before = json_u128(subedit, "before_bytes")?;
            let offset = json_u128(subedit, "offset")?;
            let route = json_string(subedit, "native_route")?;
            let native_read = json_u128(subedit, "native_bytes_read")?;
            let native_written = json_u128(subedit, "native_bytes_written")?;
            let native_patch = json_u128(subedit, "native_patch_bytes")?;
            let native_suffix = json_u128(subedit, "native_suffix_bytes_shifted")?;
            let clone_attempts = json_u128(subedit, "native_clone_attempts")?;
            let clone_successes = json_u128(subedit, "native_clone_successes")?;
            let clone_fallbacks = json_u128(subedit, "native_clone_fallbacks")?;
            let full_fallbacks = json_u128(subedit, "native_full_fallback_files")?;
            for (total, value) in native_exact.iter_mut().zip([
                native_read,
                native_written,
                native_patch,
                native_suffix,
                clone_attempts,
                clone_successes,
                clone_fallbacks,
                full_fallbacks,
            ]) {
                *total = total
                    .checked_add(value)
                    .ok_or_else(|| format!("{} native sub-edit sum overflow", row.row_id))?;
            }
            if delete == replacement {
                if !matches!(route.as_str(), "ClonePatch" | "InPlacePatch")
                    || native_read != 0
                    || native_written != replacement
                    || native_patch != replacement
                    || native_suffix != 0
                    || clone_attempts != 1
                    || (route == "ClonePatch" && (clone_successes != 1 || clone_fallbacks != 0))
                    || (route == "InPlacePatch" && (clone_successes != 0 || clone_fallbacks != 1))
                    || full_fallbacks != 0
                {
                    return Err(format!(
                        "{} exact sub-edit native patch equation",
                        row.row_id
                    ));
                }
            } else {
                let suffix = before
                    .checked_sub(
                        offset
                            .checked_add(delete)
                            .ok_or_else(|| "sub-edit native suffix overflow".to_owned())?,
                    )
                    .ok_or_else(|| "sub-edit native suffix underflow".to_owned())?;
                if route != "InPlaceShift"
                    || native_read != suffix
                    || native_written != suffix + replacement
                    || native_patch != replacement
                    || native_suffix != suffix
                    || clone_attempts != 0
                    || clone_successes != 0
                    || clone_fallbacks != 0
                    || full_fallbacks != 0
                {
                    return Err(format!(
                        "{} exact sub-edit native shift equation",
                        row.row_id
                    ));
                }
            }
            let tree_level = json_u128(subedit, "tree_level_before")?;
            let read_bound = 16 * (tree_level + 1);
            let emitted_bound = read_bound + replacement.div_ceil(8_192) + 2;
            let cdc = json_u128(subedit, "cdc_bytes_scanned")?;
            let payload = json_u128(subedit, "payload_bytes_written")?;
            let unaffected_reads = json_u128(subedit, "unaffected_payload_reads")?;
            let unaffected_writes = json_u128(subedit, "unaffected_payload_writes")?;
            let nodes_read = json_u128(subedit, "rope_nodes_read")?;
            let nodes_emitted = json_u128(subedit, "rope_nodes_emitted")?;
            let directory_nodes = json_u128(subedit, "content_directory_nodes_emitted")?;
            if cdc != replacement
                || payload != replacement
                || unaffected_reads != 0
                || unaffected_writes != 0
                || nodes_read > read_bound
                || nodes_emitted > emitted_bound
                || directory_nodes != 0
            {
                return Err(format!(
                    "{} retained sub-edit locality/H equation",
                    row.row_id
                ));
            }
            exact.cdc_bytes_scanned += u64::try_from(cdc).map_err(display_error)?;
            exact.payload_bytes_written += u64::try_from(payload).map_err(display_error)?;
            exact.unaffected_payload_reads +=
                u64::try_from(unaffected_reads).map_err(display_error)?;
            exact.unaffected_payload_writes +=
                u64::try_from(unaffected_writes).map_err(display_error)?;
            exact.rope_nodes_read += u64::try_from(nodes_read).map_err(display_error)?;
            exact.rope_nodes_emitted += u64::try_from(nodes_emitted).map_err(display_error)?;
            exact.content_directory_nodes_emitted +=
                u64::try_from(directory_nodes).map_err(display_error)?;
        }
        if row_u128(row, "cdc_bytes_scanned")? != u128::from(exact.cdc_bytes_scanned)
            || row_u128(row, "payload_bytes_written")? != u128::from(exact.payload_bytes_written)
            || row_u128(row, "unaffected_payload_reads")?
                != u128::from(exact.unaffected_payload_reads)
            || row_u128(row, "unaffected_payload_writes")?
                != u128::from(exact.unaffected_payload_writes)
            || row_u128(row, "rope_nodes_read")? != u128::from(exact.rope_nodes_read)
            || row_u128(row, "rope_nodes_emitted")? != u128::from(exact.rope_nodes_emitted)
            || row_u128(row, "content_directory_nodes_emitted")?
                != u128::from(exact.content_directory_nodes_emitted)
        {
            return Err(format!("{} retained exact sub-edit aggregate", row.row_id));
        }
        let native = json_object(&row.json, "native")?;
        for (key, expected) in [
            "bytes_read",
            "bytes_written",
            "patch_bytes",
            "suffix_bytes_shifted",
            "clone_attempts",
            "clone_successes",
            "clone_fallbacks",
            "full_fallback_files",
        ]
        .into_iter()
        .zip(native_exact)
        {
            if json_u128(native, key)? != expected {
                return Err(format!("{} native {key} sub-edit aggregate", row.row_id));
            }
        }
    }
    if subedit_count != 21 {
        return Err(format!(
            "retained sub-edit locality rows {subedit_count} != 21"
        ));
    }
    Ok(())
}

fn validate_phase_counter_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    const ADDITIVE: &[&str] = &[
        "transactions_started",
        "transactions_committed",
        "transactions_rolled_back",
        "statements",
        "admission_transactions_started",
        "admission_transactions_committed",
        "admission_transactions_rolled_back",
        "admission_statements",
        "integrity_transactions_started",
        "integrity_transactions_committed",
        "integrity_transactions_rolled_back",
        "integrity_statements",
        "busy_events",
        "locked_events",
        "objects_validated",
        "objects_created",
        "objects_reused",
        "object_bytes_read",
        "object_bytes_written",
        "fetched_rows",
        "fetched_row_authentication_passes",
        "fetched_row_role_decode_passes",
        "new_object_authentication_passes",
        "incumbent_authentication_passes",
        "payload_batch_queries",
        "payload_batch_references",
        "put_lookup_statements",
        "put_insert_statements",
        "created_rows",
        "reused_rows",
        "publication_transactions_started",
        "publication_transactions_rolled_back",
        "publication_commits",
        "publication_closure_passes",
        "namespace_graph_verification_passes",
        "retained_roots_validated",
    ];
    for row in rows {
        let expected: &[&str] = match row.row_group.as_str() {
            "C02" => &[
                "store_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C03" | "C07" => &[
                "native_edit",
                "checkpoint",
                "canonical_witness",
                "storage_observation",
            ],
            "C04" | "C06" => &[
                "verified_open",
                "storage_observation",
                "history_read",
                "storage_observation",
            ],
            "C05" => &[
                "logical_edit",
                "apfs_refresh",
                "canonical_witness",
                "storage_observation",
            ],
            "C08" => &[
                "verified_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C09" => &["explicit_cleanup"],
            _ => &[],
        };
        let phases = json_array_objects(&row.json, "phase_counters")?;
        let names = phases
            .iter()
            .map(|phase| json_string(phase, "name"))
            .collect::<EvalResult<Vec<_>>>()?;
        if names != expected {
            return Err(format!(
                "{} phase counter names {names:?} != {expected:?}",
                row.row_id
            ));
        }
        if expected.is_empty() {
            continue;
        }
        for phase in &phases {
            let fetched = json_u128(phase, "fetched_rows")?;
            let authenticated = json_u128(phase, "fetched_row_authentication_passes")?;
            let decoded = json_u128(phase, "fetched_row_role_decode_passes")?;
            let created = json_u128(phase, "created_rows")?;
            let reused = json_u128(phase, "reused_rows")?;
            let new = json_u128(phase, "new_object_authentication_passes")?;
            let incumbent = json_u128(phase, "incumbent_authentication_passes")?;
            let retained_scrubs = json_u128(phase, "retained_union_scrubs")?;
            let retained_roots = json_u128(phase, "retained_roots_validated")?;
            let namespace_graphs = json_u128(phase, "namespace_graph_verification_passes")?;
            let name = json_string(phase, "name")?;
            let trusted = matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07");
            let trusted_read_only = row.row_group == "C02"
                || matches!(name.as_str(), "canonical_witness" | "apfs_refresh");
            if ((trusted_read_only && authenticated != 0)
                || (trusted && authenticated > fetched)
                || (!trusted && fetched != authenticated))
                || fetched != decoded
                || new != created + reused
                || new != json_u128(phase, "put_lookup_statements")?
                || incumbent != reused
                || json_u128(phase, "put_insert_statements")? != created
                || json_u128(phase, "objects_created")? != created
                || json_u128(phase, "objects_reused")? != reused
                || json_u128(phase, "objects_validated")? != decoded + new + incumbent
                || json_u128(phase, "object_bytes_written")?
                    != json_u128(phase, "logical_object_bytes")?
                || json_u128(phase, "range_bytes_requested")?
                    != json_u128(phase, "range_bytes_returned")?
                || json_u128(phase, "payload_batch_maximum")? > 64
                || json_u128(phase, "admission_transactions_started")?
                    != json_u128(phase, "admission_transactions_committed")?
                        + json_u128(phase, "admission_transactions_rolled_back")?
                || json_u128(phase, "integrity_transactions_started")?
                    != json_u128(phase, "integrity_transactions_committed")?
                        + json_u128(phase, "integrity_transactions_rolled_back")?
                || json_u128(phase, "publication_transactions_started")?
                    != json_u128(phase, "publication_commits")?
                        + json_u128(phase, "publication_transactions_rolled_back")?
                || (retained_scrubs != 0 && retained_roots != namespace_graphs)
                || json_u128(phase, "q_before_bytes")? != 0
                || json_u128(phase, "q_after_bytes")? != 0
                || json_u128(phase, "q_high_water_bytes")? > 8_388_608
                || json_u128(phase, "active_connections")? > 2
            {
                return Err(format!("{} phase counter equation", row.row_id));
            }
        }
        for key in ADDITIVE {
            let phase_sum = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, key)?)
                    .ok_or_else(|| format!("{} phase {key} sum overflow", row.row_id))
            })?;
            if phase_sum != row_u128(row, key)? {
                return Err(format!(
                    "{} phase {key} sum {phase_sum} != row {}",
                    row.row_id,
                    row_u128(row, key)?
                ));
            }
        }
        {
            let key = "payload_batch_maximum";
            let phase_max = phases
                .iter()
                .map(|phase| json_u128(phase, key))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .max()
                .unwrap_or(0);
            if phase_max != row_u128(row, key)? {
                return Err(format!("{} phase {key} maximum", row.row_id));
            }
        }
        for (engine_key, operation_key) in [
            ("scratch_tables", "operation_scratch_tables"),
            ("scratch_statements", "operation_scratch_statements"),
            ("scratch_rows", "operation_scratch_rows"),
        ] {
            let engine = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, engine_key)?)
                    .ok_or_else(|| format!("{} phase {engine_key} overflow", row.row_id))
            })?;
            let operation = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, operation_key)?)
                    .ok_or_else(|| format!("{} phase {operation_key} overflow", row.row_id))
            })?;
            let combined = engine
                .checked_add(operation)
                .ok_or_else(|| format!("{} combined {engine_key} overflow", row.row_id))?;
            if combined != row_u128(row, engine_key)? {
                return Err(format!(
                    "{} phase Engine/VFS {engine_key} aggregate",
                    row.row_id
                ));
            }
        }
        let engine_scratch_high = phases
            .iter()
            .map(|phase| json_u128(phase, "scratch_high_water_bytes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        let operation_scratch_high = phases
            .iter()
            .map(|phase| json_u128(phase, "operation_scratch_high_water_bytes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        if engine_scratch_high.max(operation_scratch_high)
            != row_u128(row, "scratch_high_water_bytes")?
        {
            return Err(format!(
                "{} phase Engine/VFS scratch_high_water_bytes aggregate",
                row.row_id
            ));
        }
    }
    Ok(())
}

fn validate_refresh_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    let refreshes = rows
        .iter()
        .filter(|row| row.row_group == "C05")
        .collect::<Vec<_>>();
    if refreshes.len() != 15 {
        return Err(format!("logical refresh rows {} != 15", refreshes.len()));
    }
    let mut patches = 0_usize;
    let mut shifts = 0_usize;
    for row in refreshes {
        let edit = json_object(&row.json, "edit")?;
        let offset = json_u128(edit, "offset")?;
        let deleted = json_u128(edit, "delete_bytes")?;
        let inserted = json_u128(edit, "insert_bytes")?;
        if row_u128(row, "full_fallback_files")? != 0 {
            return Err(format!("{} unexpectedly used FullFallback", row.row_id));
        }
        if deleted == inserted {
            patches += 1;
            if !matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
                || row_u128(row, "suffix_bytes_shifted")? != 0
                || row_u128(row, "patch_bytes")? != inserted
            {
                return Err(format!("{} retained patch route equation", row.row_id));
            }
            continue;
        }
        shifts += 1;
        let suffix = u128::from(row.before_bytes)
            .checked_sub(
                offset
                    .checked_add(deleted)
                    .ok_or_else(|| "refresh suffix overflow".to_owned())?,
            )
            .ok_or_else(|| "refresh suffix underflow".to_owned())?;
        if !matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
            || row_u128(row, "suffix_bytes_shifted")? != suffix
            || row_u128(row, "bytes_read")? != suffix
            || row_u128(row, "bytes_written")? != suffix + inserted
            || row_u128(row, "patch_bytes")? != inserted
        {
            return Err(format!("{} retained shift byte equation", row.row_id));
        }
        let attempts = row_u128(row, "clone_attempts")?;
        let successes = row_u128(row, "clone_successes")?;
        let fallbacks = row_u128(row, "clone_fallbacks")?;
        if row.native_route == "CloneShift" {
            if (attempts, successes, fallbacks) != (1, 1, 0) {
                return Err(format!("{} retained CloneShift equation", row.row_id));
            }
        } else if successes != 0 || attempts != fallbacks || attempts > 1 {
            return Err(format!("{} retained InPlaceShift equation", row.row_id));
        }
    }
    if patches != 3 || shifts != 12 {
        return Err(format!(
            "refresh route population {patches}/3 patch {shifts}/12 shift"
        ));
    }
    Ok(())
}

fn validate_availability_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    for row in rows {
        let unavailable = json_array_objects(&row.json, "unavailable")?;
        let has_record = |field: &str| -> bool {
            unavailable.iter().any(|record| {
                json_string(record, "field").as_deref() == Ok(field)
                    && matches!(
                        json_string(record, "availability").as_deref(),
                        Ok("Unavailable" | "NotApplicable")
                    )
            })
        };
        for (object, fields) in [
            (
                "counters",
                &[
                    "transactions_started",
                    "transactions_committed",
                    "transactions_rolled_back",
                    "statements",
                    "admission_transactions_started",
                    "admission_transactions_committed",
                    "admission_transactions_rolled_back",
                    "admission_statements",
                    "integrity_transactions_started",
                    "integrity_transactions_committed",
                    "integrity_transactions_rolled_back",
                    "integrity_statements",
                    "busy_events",
                    "locked_events",
                    "objects_validated",
                    "objects_created",
                    "objects_reused",
                    "object_bytes_read",
                    "object_bytes_written",
                    "fetched_rows",
                    "fetched_row_authentication_passes",
                    "fetched_row_role_decode_passes",
                    "new_object_authentication_passes",
                    "incumbent_authentication_passes",
                    "payload_batch_queries",
                    "payload_batch_references",
                    "payload_batch_maximum",
                    "put_lookup_statements",
                    "put_insert_statements",
                    "created_rows",
                    "reused_rows",
                    "publication_transactions_started",
                    "publication_transactions_rolled_back",
                    "publication_commits",
                    "publication_closure_passes",
                    "namespace_graph_verification_passes",
                    "scratch_tables",
                    "scratch_statements",
                    "scratch_rows",
                    "scratch_high_water_bytes",
                    "retained_roots_validated",
                    "cdc_bytes_scanned",
                    "payload_bytes_written",
                    "unaffected_payload_reads",
                    "unaffected_payload_writes",
                    "rope_nodes_read",
                    "rope_nodes_emitted",
                    "content_directory_nodes_emitted",
                    "workspace_materializations",
                    "workspace_reuses",
                    "rematerializations",
                    "descriptor_resets",
                ][..],
            ),
            (
                "native",
                &[
                    "bytes_read",
                    "bytes_written",
                    "patch_bytes",
                    "suffix_bytes_shifted",
                    "clone_attempts",
                    "clone_successes",
                    "clone_fallbacks",
                    "full_fallback_files",
                    "files_created",
                    "files_replaced",
                    "files_removed",
                    "sync_regular_calls",
                    "sync_directory_calls",
                ],
            ),
            (
                "storage",
                &[
                    "database_bytes",
                    "logical_engine_bytes",
                    "rollback_journal_bytes",
                    "temporary_file_bytes",
                    "database_growth_bytes",
                    "canonical_object_bytes_written",
                    "physical_to_canonical_amplification",
                ],
            ),
            (
                "resources",
                &[
                    "rss_current_bytes",
                    "operation_q_current_bytes",
                    "operation_q_high_water_bytes",
                    "operation_q_terminal_bytes",
                    "owned_temp_entries",
                ],
            ),
            (
                "oracle",
                &[
                    "physical_bytes_exact",
                    "canonical_bytes_exact",
                    "metadata_exact",
                    "historical_roots_exact",
                    "route_exact",
                ],
            ),
        ] {
            let scoped = json_object(&row.json, object)?;
            for field in fields {
                if scoped.contains(&format!("\"{field}\":null"))
                    && !has_record(&format!("{object}.{field}"))
                {
                    return Err(format!(
                        "{} null {object}.{field} lacks availability record",
                        row.row_id
                    ));
                }
            }
        }
        if json_top_level_value(&row.json, "tree_level_before")?.starts_with("null")
            && !has_record("tree_level_before")
        {
            return Err(format!(
                "{} null tree_level_before lacks availability record",
                row.row_id
            ));
        }
    }
    Ok(())
}

fn validate_metadata_receipt(metadata: &str, label: &str) -> EvalResult<()> {
    let xattrs = json_array_objects(metadata, "xattrs")?;
    let acl_present = json_bool(metadata, "acl_present")?;
    json_i64(metadata, "mtime_seconds")?;
    if json_u128(metadata, "mode")? != u128::from(FIXTURE_MODE)
        || json_u128(metadata, "mtime_nanoseconds")? >= 1_000_000_000
        || json_u128(metadata, "xattr_count")? != xattrs.len() as u128
        || !xattrs.is_empty()
        || acl_present
        || !metadata.contains("\"acl_hex\":null")
        || json_u128(metadata, "bsd_flags")? != 0
    {
        return Err(format!("{label} exact supported metadata receipt"));
    }
    Ok(())
}

fn validate_history_rows(rows: &[ParsedRow]) -> EvalResult<usize> {
    let root_rows = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    if root_rows.len() != 35 {
        return Err(format!(
            "retained root digest rows {} != 35",
            root_rows.len()
        ));
    }
    let root_digests = root_rows
        .iter()
        .map(|row| json_string(json_object(&row.json, "oracle")?, "content_digest"))
        .collect::<EvalResult<Vec<_>>>()?;
    if root_digests
        .iter()
        .any(|digest| digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("retained root digest custody".to_owned());
    }
    let schedule = frozen_schedule()?;
    let snapshots = oracle_snapshots(&schedule)?;
    let sessions = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .collect::<Vec<_>>();
    if sessions.len() != 6 {
        return Err(format!("retained history sessions {} != 6", sessions.len()));
    }
    let mut selected = Vec::new();
    let mut probe_count = 0_usize;
    for (index, row) in sessions.into_iter().enumerate() {
        let session = u8::try_from(index + 1).map_err(display_error)?;
        if json_object(&row.json, "custody")? != history_custody_json(session)?
            || !json_bool(json_object(&row.json, "oracle")?, "historical_roots_exact")?
            || json_string(json_object(&row.json, "oracle")?, "content_digest")?
                != root_digests[usize::from(session) * 5]
        {
            return Err(format!(
                "{} exact retained history-root receipt",
                row.row_id
            ));
        }
        let phase = json_array_objects(&row.json, "phase_counters")?
            .into_iter()
            .find(|phase| json_string(phase, "name").as_deref() == Ok("history_read"))
            .ok_or_else(|| format!("{} missing history_read phase counters", row.row_id))?;
        let probes = json_array_objects(&row.json, "history_probes")?;
        let expected_roots = history_root_indices(session)?;
        if probes.len() != expected_roots.len() * 3 {
            return Err(format!("{} exact history probe population", row.row_id));
        }
        let mut wall_sum = 0_u128;
        for (probe_index, probe) in probes.iter().enumerate() {
            let root_index = expected_roots[probe_index / 3];
            let ordinal = probe_index % 3 + 1;
            let logical_length = snapshots[root_index].logical_length;
            let start = match ordinal {
                1 => 0,
                2 => logical_length / 2 - 32_768,
                3 => logical_length - 65_536,
                _ => unreachable!(),
            };
            let counters = json_object(probe, "engine_counters")?;
            let fetched = json_u128(probe, "fetched_rows")?;
            let payload_references = json_u128(probe, "payload_batch_references")?;
            let statements = json_u128(counters, "statements")?;
            let payload_queries = json_u128(probe, "payload_batch_queries")?;
            if json_string(probe, "root")? != format!("R{root_index}")
                || json_u128(probe, "ordinal")? != ordinal as u128
                || json_u128(probe, "start")? != u128::from(start)
                || json_u128(probe, "length")? != 65_536
                || json_u128(probe, "payload_bytes_read")? != 65_536
                || fetched != json_u128(probe, "authentication_passes")?
                || fetched != json_u128(probe, "role_decode_passes")?
                || json_u128(probe, "non_payload_rows")?
                    != fetched.checked_sub(payload_references).ok_or_else(|| {
                        format!("{} probe payload rows exceed fetched rows", row.row_id)
                    })?
                || json_u128(probe, "non_payload_statements")?
                    != statements.checked_sub(payload_queries).ok_or_else(|| {
                        format!("{} probe payload queries exceed statements", row.row_id)
                    })?
                || (ordinal == 1
                    && (json_u128(probe, "namespace_nodes_read")? == 0
                        || json_u128(probe, "inode_table_nodes_read")? == 0))
                || (ordinal != 1
                    && (json_u128(probe, "namespace_nodes_read")? != 0
                        || json_u128(probe, "inode_table_nodes_read")? != 0))
                || json_u128(counters, "transactions_started")? != 0
                || json_u128(counters, "transactions_committed")? != 0
                || json_u128(counters, "publication_transactions_started")? != 0
                || json_u128(counters, "publication_transactions_rolled_back")? != 0
                || json_u128(counters, "publication_commits")? != 0
                || json_u128(counters, "object_bytes_written")? != 0
                || json_u128(counters, "cdc_bytes_scanned")? != 0
                || json_u128(counters, "payload_bytes_written")? != 0
            {
                return Err(format!("{} exact ordered history probe", row.row_id));
            }
            wall_sum = wall_sum
                .checked_add(json_u128(probe, "wall_ns")?)
                .ok_or_else(|| format!("{} probe wall overflow", row.row_id))?;
        }
        if wall_sum > phase_wall(&row.json, "history_read")? {
            return Err(format!("{} probe walls exceed history phase", row.row_id));
        }
        for key in [
            "transactions_started",
            "transactions_committed",
            "transactions_rolled_back",
            "statements",
            "admission_transactions_started",
            "admission_transactions_committed",
            "admission_transactions_rolled_back",
            "admission_statements",
            "integrity_transactions_started",
            "integrity_transactions_committed",
            "integrity_transactions_rolled_back",
            "integrity_statements",
            "busy_events",
            "locked_events",
            "objects_validated",
            "objects_created",
            "objects_reused",
            "object_bytes_read",
            "object_bytes_written",
            "fetched_rows",
            "fetched_row_authentication_passes",
            "fetched_row_role_decode_passes",
            "new_object_authentication_passes",
            "incumbent_authentication_passes",
            "payload_batch_queries",
            "payload_batch_references",
            "put_lookup_statements",
            "put_insert_statements",
            "created_rows",
            "reused_rows",
            "publication_transactions_started",
            "publication_transactions_rolled_back",
            "publication_commits",
            "publication_closure_passes",
            "namespace_graph_verification_passes",
            "scratch_tables",
            "scratch_statements",
            "scratch_rows",
            "retained_roots_validated",
        ] {
            let sum = probes.iter().try_fold(0_u128, |sum, probe| {
                sum.checked_add(json_u128(json_object(probe, "engine_counters")?, key)?)
                    .ok_or_else(|| format!("{} probe {key} overflow", row.row_id))
            })?;
            if sum != json_u128(phase, key)? {
                return Err(format!("{} probe {key} sum", row.row_id));
            }
        }
        let payload_max = probes
            .iter()
            .map(|probe| {
                json_u128(
                    json_object(probe, "engine_counters")?,
                    "payload_batch_maximum",
                )
            })
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        if payload_max != json_u128(phase, "payload_batch_maximum")? {
            return Err(format!("{} probe payload batch maximum", row.row_id));
        }
        probe_count += probes.len();
        selected.extend_from_slice(history_root_indices(session)?);
    }
    if probe_count != 63 {
        return Err(format!("retained history probes {probe_count} != 63"));
    }
    selected.sort_unstable();
    selected.dedup();
    if selected != [0, 5, 10, 15, 20, 25, 30] {
        return Err(format!("retained selected history roots {selected:?}"));
    }
    let milestones = rows
        .iter()
        .filter(|row| row.row_group == "C08")
        .collect::<Vec<_>>();
    if milestones.len() != 3 {
        return Err(format!("retained milestone rows {} != 3", milestones.len()));
    }
    for (row, root) in milestones.into_iter().zip([15_u8, 30, 34]) {
        let oracle = json_object(&row.json, "oracle")?;
        let custody = json_object(&row.json, "custody")?;
        let metadata = json_object(custody, "metadata")?;
        let retained_metadata = json_object(custody, "retained_metadata")?;
        let fresh_metadata = json_object(custody, "fresh_metadata")?;
        if json_string(custody, "milestone_root")? != format!("R{root}")
            || json_u128(custody, "extra_user_files")? != 0
            || json_u128(custody, "fresh_extra_user_files")? != 0
            || json_u128(custody, "cleanup_residue_entries")? != 0
            || metadata != fresh_metadata
            || fresh_metadata != retained_metadata
            || !json_bool(oracle, "physical_bytes_exact")?
            || !json_bool(oracle, "canonical_bytes_exact")?
            || !json_bool(oracle, "metadata_exact")?
            || !json_bool(oracle, "historical_roots_exact")?
        {
            return Err(format!("{} exact milestone/history receipt", row.row_id));
        }
        validate_metadata_receipt(fresh_metadata, &format!("R{root}"))?;
        if root == 34 {
            if json_u128(custody, "live_extra_user_files")? != 0
                || json_object(custody, "live_metadata")? != fresh_metadata
            {
                return Err("R34 live/fresh tree and metadata receipt".to_owned());
            }
        } else if !custody.contains("\"live_extra_user_files\":null")
            || !custody.contains("\"live_metadata\":null")
        {
            return Err(format!("R{root} live custody is not applicable"));
        }
    }
    Ok(selected.len() + 1)
}

#[derive(Clone, Debug)]
struct Statistics {
    raw_ns: Vec<u128>,
    sorted_ns: Vec<u128>,
    minimum_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    maximum_ns: u128,
    range_ns: u128,
    sum_ns: u128,
}

fn statistics(raw: Vec<u128>) -> EvalResult<Statistics> {
    if raw.is_empty() {
        return Err("statistics population is empty".to_owned());
    }
    let mut sorted = raw.clone();
    sorted.sort_unstable();
    let n = sorted.len();
    let p50_index = (n * 50).div_ceil(100).saturating_sub(1);
    let p95_index = (n * 95).div_ceil(100).saturating_sub(1);
    let minimum_ns = sorted[0];
    let maximum_ns = sorted[n - 1];
    Ok(Statistics {
        raw_ns: raw,
        sorted_ns: sorted.clone(),
        minimum_ns,
        p50_ns: sorted[p50_index],
        p95_ns: sorted[p95_index],
        maximum_ns,
        range_ns: maximum_ns - minimum_ns,
        sum_ns: sorted.iter().try_fold(0_u128, |total, value| {
            total
                .checked_add(*value)
                .ok_or_else(|| "statistics sum overflow".to_owned())
        })?,
    })
}

fn stats_json(stats: &Statistics) -> String {
    format!(
        concat!(
            "{{\"n\":{},\"raw_ns\":{},\"sorted_ns\":{},",
            "\"minimum_ns\":{},\"p50_ns\":{},\"p95_ns\":{},",
            "\"maximum_ns\":{},\"range_ns\":{},\"sum_ns\":{}}}"
        ),
        stats.raw_ns.len(),
        u128_array_json(&stats.raw_ns),
        u128_array_json(&stats.sorted_ns),
        stats.minimum_ns,
        stats.p50_ns,
        stats.p95_ns,
        stats.maximum_ns,
        stats.range_ns,
        stats.sum_ns,
    )
}

fn u128_array_json(values: &[u128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn row_phase_stats(rows: &[ParsedRow], group: &str, phase: &str) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group)
            .map(|row| phase_wall(&row.json, phase))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}

fn filtered_phase_stats(
    rows: &[ParsedRow],
    group: &str,
    phase: &str,
    predicate: impl Fn(&ParsedRow) -> bool,
) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group && predicate(row))
            .map(|row| phase_wall(&row.json, phase))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}

fn combined_phase_stats(
    rows: &[ParsedRow],
    group: &str,
    first: &str,
    second: &str,
    predicate: impl Fn(&ParsedRow) -> bool,
) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| row.row_group == group && predicate(row))
            .map(|row| {
                phase_wall(&row.json, first)?
                    .checked_add(phase_wall(&row.json, second)?)
                    .ok_or_else(|| "combined phase wall overflow".to_owned())
            })
            .collect::<EvalResult<Vec<_>>>()?,
    )
}

fn roots_from_rows(rows: &[ParsedRow]) -> EvalResult<Vec<String>> {
    let mut roots = Vec::new();
    let initial = rows
        .iter()
        .find(|row| row.row_group == "C02")
        .ok_or_else(|| "missing C02 root".to_owned())?;
    roots.push(json_string(
        initial
            .json
            .split_once("\"post_ref\":")
            .ok_or_else(|| "C02 missing post_ref".to_owned())?
            .1,
        "root",
    )?);
    for row in rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
    {
        roots.push(json_string(
            row.json
                .split_once("\"post_ref\":")
                .ok_or_else(|| format!("{} missing post_ref", row.row_id))?
                .1,
            "root",
        )?);
    }
    if roots.len() != 35 {
        return Err(format!("retained root count {} != 35", roots.len()));
    }
    Ok(roots)
}

fn derive_disposition(rows: &[ParsedRow]) -> Disposition {
    if rows.iter().any(|row| row.status == "FAIL") {
        Disposition::Fail
    } else if rows.iter().any(|row| row.status == "REVISE") {
        Disposition::Revise
    } else {
        Disposition::Pass
    }
}

fn finalize_reports(
    campaign: &mut Campaign<'_>,
    source: &SourceIdentity,
    master: &FixtureMaster,
    schedule: &FrozenSchedule,
) -> EvalResult<Disposition> {
    let rows = parse_rows(&campaign.run.join("rows.jsonl"), schedule)?;
    let disposition = derive_disposition(&rows);
    if disposition == Disposition::Fail {
        return Err("hard-gate failed row cannot be promoted to PASS".to_owned());
    }
    validate_ref_chain(&rows, schedule)?;
    validate_authentication(&rows)?;
    validate_locality_rows(&rows)?;
    validate_availability_rows(&rows)?;
    validate_history_rows(&rows)?;
    let preliminary_complete = campaign.started.elapsed().as_nanos();
    let preliminary_time = campaign_time(campaign, preliminary_complete, disposition);
    durable_write(&campaign.run.join("campaign-time.txt"), &preliminary_time)?;
    let preliminary_campaign_sha = sha256_file(&campaign.run.join("campaign-time.txt"))?;
    let preliminary_json = summary_json(
        campaign,
        &rows,
        source,
        master,
        preliminary_complete,
        &preliminary_campaign_sha,
    )?;
    let preliminary_md = summary_markdown(campaign, &rows, source, master, preliminary_complete)?;
    validate_summary_pair(&preliminary_json, &preliminary_md)?;
    durable_write(&campaign.run.join("summary.json"), &preliminary_json)?;
    durable_write(&campaign.run.join("summary.md"), &preliminary_md)?;
    let complete_wall = campaign.started.elapsed().as_nanos();
    if complete_wall >= CAMPAIGN_LIMIT_NS {
        return Err("complete_wall_ns < 60,000,000,000".to_owned());
    }
    let final_time = campaign_time(campaign, complete_wall, disposition);
    validate_campaign_time(&final_time)?;
    durable_replace(&campaign.run.join("campaign-time.txt"), &final_time)?;
    let campaign_sha = sha256_file(&campaign.run.join("campaign-time.txt"))?;
    let final_json = summary_json(
        campaign,
        &rows,
        source,
        master,
        complete_wall,
        &campaign_sha,
    )?;
    let final_md = summary_markdown(campaign, &rows, source, master, complete_wall)?;
    validate_summary_pair(&final_json, &final_md)?;
    durable_replace(&campaign.run.join("summary.json"), &final_json)?;
    durable_replace(&campaign.run.join("summary.md"), &final_md)?;
    if parse_rows(&campaign.run.join("rows.jsonl"), schedule)?.len() != 47 {
        return Err("final rows revalidation".to_owned());
    }
    Ok(disposition)
}

fn campaign_time(
    campaign: &Campaign<'_>,
    complete_wall_ns: u128,
    disposition: Disposition,
) -> String {
    let outside_rows = complete_wall_ns.saturating_sub(campaign.row_wall_sum_ns);
    format!(
        concat!(
            "schema=layerfs-stage1.1-campaign-time-v1\nstatus={}\n",
            "started_unix_ns={}\ncompleted_unix_ns={}\ncomplete_wall_ns={}\n",
            "row_wall_sum_ns={}\noutside_rows_wall_ns={}\ntimer_residual_ns=0\n",
            "hard_limit_ns=60000000000\nrows_expected=47\nrows_valid=47\n",
            "edit_suboperations_expected=51\nedit_suboperations_observed=51\n",
            "transitions_expected=34\ntransitions_observed=34\n"
        ),
        disposition.as_str(),
        campaign.started_unix_ns,
        campaign.started_unix_ns.saturating_add(complete_wall_ns),
        complete_wall_ns,
        campaign.row_wall_sum_ns,
        outside_rows,
    )
}

fn validate_campaign_time(contents: &str) -> EvalResult<()> {
    if !contents.ends_with('\n') || contents.ends_with("\n\n") {
        return Err("campaign-time.txt must have exactly one trailing newline".to_owned());
    }
    validate_timer_equation(contents)?;
    if campaign_time_value(contents, "hard_limit_ns")? != CAMPAIGN_LIMIT_NS
        || campaign_time_value(contents, "rows_expected")? != 47
        || campaign_time_value(contents, "rows_valid")? != 47
        || campaign_time_value(contents, "edit_suboperations_expected")? != 51
        || campaign_time_value(contents, "edit_suboperations_observed")? != 51
        || campaign_time_value(contents, "transitions_expected")? != 34
        || campaign_time_value(contents, "transitions_observed")? != 34
    {
        return Err("campaign-time timer/population equation".to_owned());
    }
    Ok(())
}

fn campaign_time_value(contents: &str, key: &str) -> EvalResult<u128> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .ok_or_else(|| format!("campaign-time missing {key}"))?
        .parse()
        .map_err(display_error)
}

fn validate_timer_equation(contents: &str) -> EvalResult<()> {
    let complete = campaign_time_value(contents, "complete_wall_ns")?;
    let rows = campaign_time_value(contents, "row_wall_sum_ns")?;
    let outside = campaign_time_value(contents, "outside_rows_wall_ns")?;
    let residual = campaign_time_value(contents, "timer_residual_ns")?;
    if complete
        != rows
            .checked_add(outside)
            .and_then(|sum| sum.checked_add(residual))
            .ok_or_else(|| "campaign timer equation overflow".to_owned())?
    {
        return Err("campaign timer equation".to_owned());
    }
    Ok(())
}

fn sum_key(rows: &[ParsedRow], group: Option<&str>, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| group.is_none_or(|group| row.row_group == group))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_optional_u128(row, key)?.unwrap_or(0))
                .ok_or_else(|| format!("{key} sum overflow"))
        })
}

fn maximum_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    rows.iter()
        .map(|row| row_optional_u128(row, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .max()
        .ok_or_else(|| format!("no rows for maximum {key}"))
}

fn sum_locality_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    ["C03", "C05", "C07"]
        .into_iter()
        .try_fold(0_u128, |total, group| {
            total
                .checked_add(sum_key(rows, Some(group), key)?)
                .ok_or_else(|| format!("locality {key} sum overflow"))
        })
}

fn maximum_locality_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    ["C03", "C05", "C07"]
        .into_iter()
        .map(|group| maximum_group_key(rows, group, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| format!("no locality rows for maximum {key}"))
}

fn physical_by_kind_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.operation == kind)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.operation == kind
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.operation == kind
            })?;
        values.push(format!(
            "\"{kind}\":{{\"native_edit\":{},\"durable_checkpoint\":{},\"edit_plus_checkpoint\":{}}}",
            stats_json(&native),
            stats_json(&checkpoint),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn physical_by_size_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for band in ["near-8-kib", "near-16-kib", "near-32-kib"] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.size_band == band)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.size_band == band
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.size_band == band
            })?;
        values.push(format!(
            "\"{band}\":{{\"native_edit\":{},\"durable_checkpoint\":{},\"edit_plus_checkpoint\":{}}}",
            stats_json(&native),
            stats_json(&checkpoint),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn logical_by_kind_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
            row.operation == kind
        })?;
        let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == kind
        })?;
        let combined = combined_phase_stats(
            rows,
            "C05",
            "direct_logical_edit",
            "changed_root_refresh",
            |row| row.operation == kind,
        )?;
        values.push(format!(
            "\"{kind}\":{{\"direct_logical_edit\":{},\"changed_root_refresh\":{},\"logical_edit_plus_refresh\":{}}}",
            stats_json(&logical),
            stats_json(&refresh),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn logical_by_size_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for band in ["near-8-kib", "near-16-kib", "near-32-kib"] {
        let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
            row.size_band == band
        })?;
        let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.size_band == band
        })?;
        let combined = combined_phase_stats(
            rows,
            "C05",
            "direct_logical_edit",
            "changed_root_refresh",
            |row| row.size_band == band,
        )?;
        values.push(format!(
            "\"{band}\":{{\"direct_logical_edit\":{},\"changed_root_refresh\":{},\"logical_edit_plus_refresh\":{}}}",
            stats_json(&logical),
            stats_json(&refresh),
            stats_json(&combined)
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn route_stats(rows: &[ParsedRow], route: &str, operation: Option<&str>) -> EvalResult<Statistics> {
    filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
        (route == "Patch" && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
            || route == "Shift"
                && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
            || row.native_route == route)
            && operation.is_none_or(|operation| row.operation == operation)
    })
}

fn root_json(roots: &[String]) -> String {
    [0_usize, 5, 10, 15, 20, 25, 30, 31, 32, 33, 34]
        .into_iter()
        .map(|index| format!("\"R{index}\":\"{}\"", roots[index]))
        .collect::<Vec<_>>()
        .join(",")
}

fn count_change_amplification_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for kind in ["insert", "delete", "append", "truncate"] {
        let selected = rows
            .iter()
            .filter(|row| row.row_group == "C03" && row.operation == kind)
            .collect::<Vec<_>>();
        let suffix = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "suffix_bytes_shifted")?)
                .ok_or_else(|| "suffix amplification sum overflow".to_owned())
        })?;
        let read = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_read")?)
                .ok_or_else(|| "native read amplification sum overflow".to_owned())
        })?;
        let written = selected.iter().try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "native write amplification sum overflow".to_owned())
        })?;
        let logical_change = selected
            .iter()
            .map(|row| u128::from(row.before_bytes.abs_diff(row.after_bytes)))
            .sum::<u128>();
        let amplification = if logical_change == 0 {
            0.0
        } else {
            (read + written) as f64 / logical_change as f64
        };
        values.push(format!(
            "\"{kind}\":{{\"n\":{},\"suffix_bytes_shifted\":{suffix},\"native_bytes_read\":{read},\"native_bytes_written\":{written},\"logical_change_bytes\":{logical_change},\"amplification\":{amplification:.9}}}",
            selected.len()
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn materialization_by_root_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for (index, root) in [15_u8, 30, 34].into_iter().enumerate() {
        let row = rows
            .iter()
            .filter(|row| row.row_group == "C08")
            .nth(index)
            .ok_or_else(|| format!("missing C08 materialization R{root}"))?;
        let wall = phase_wall(&row.json, "milestone_materialization")?;
        let oracle = json_object(&row.json, "oracle")?;
        let custody = json_object(&row.json, "custody")?;
        let exact_bytes = json_bool(oracle, "physical_bytes_exact")?
            && json_bool(oracle, "canonical_bytes_exact")?;
        let metadata_exact = json_bool(oracle, "metadata_exact")?;
        let extra_user_files = json_u128(custody, "extra_user_files")?;
        let cleanup_exact = json_u128(custody, "cleanup_residue_entries")? == 0;
        let metadata_receipt = json_object(custody, "fresh_metadata")?;
        values.push(format!(
            "\"R{root}\":{{\"logical_bytes\":{},\"wall\":{},\"native_bytes_written\":{},\"exact_bytes\":{exact_bytes},\"metadata_exact\":{metadata_exact},\"metadata\":{metadata_receipt},\"extra_user_files\":{extra_user_files},\"cleanup_exact\":{cleanup_exact}}}",
            row.after_bytes,
            stats_json(&statistics(vec![wall])?),
            row_u128(row, "bytes_written")?,
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

fn storage_by_root_range_json(rows: &[ParsedRow]) -> EvalResult<String> {
    let mut values = Vec::new();
    for (label, group, transitions) in [
        ("R0_to_R15", "C03", 15),
        ("R15_to_R30", "C05", 15),
        ("R30_to_R34", "C07", 4),
    ] {
        let canonical = range_sum(rows, group, "canonical_object_bytes_written")?;
        let database = range_sum(rows, group, "database_growth_bytes")?;
        let amplification = if canonical == 0 {
            0.0
        } else {
            database as f64 / canonical as f64
        };
        values.push(format!(
            "\"{label}\":{{\"transitions\":{transitions},\"canonical_bytes_written\":{canonical},\"database_growth_bytes\":{database},\"amplification\":{amplification:.9}}}"
        ));
    }
    Ok(format!("{{{}}}", values.join(",")))
}

#[derive(Clone, Debug, Default)]
struct PhaseAttribution {
    name: &'static str,
    rows: usize,
    statements: u128,
    fetched_rows: u128,
    authentication_passes: u128,
    role_decode_passes: u128,
    object_bytes_read: u128,
    object_bytes_written: u128,
    transactions: u128,
    commits: u128,
    publication_commits: u128,
    retained_union_scrubs: u128,
    scratch_tables: u128,
    operation_scratch_tables: u128,
    q_high_water_bytes: u128,
    active_connections: u128,
}

fn phase_attributions(rows: &[ParsedRow]) -> EvalResult<Vec<PhaseAttribution>> {
    let mut output = Vec::new();
    for name in [
        "store_open",
        "materialization",
        "checkpoint",
        "logical_edit",
        "apfs_refresh",
        "canonical_witness",
        "verified_open",
        "history_read",
        "storage_observation",
    ] {
        let phases = rows
            .iter()
            .map(|row| json_array_objects(&row.json, "phase_counters"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|phase| json_string(phase, "name").as_deref() == Ok(name))
            .collect::<Vec<_>>();
        if phases.is_empty() {
            return Err(format!("phase attribution {name} is empty"));
        }
        let sum = |key: &str| -> EvalResult<u128> {
            phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, key)?)
                    .ok_or_else(|| format!("phase attribution {name}.{key} overflow"))
            })
        };
        let maximum = |key: &str| -> EvalResult<u128> {
            phases
                .iter()
                .map(|phase| json_u128(phase, key))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .max()
                .ok_or_else(|| format!("phase attribution {name}.{key} maximum"))
        };
        output.push(PhaseAttribution {
            name,
            rows: phases.len(),
            statements: sum("statements")?,
            fetched_rows: sum("fetched_rows")?,
            authentication_passes: sum("fetched_row_authentication_passes")?,
            role_decode_passes: sum("fetched_row_role_decode_passes")?,
            object_bytes_read: sum("object_bytes_read")?,
            object_bytes_written: sum("object_bytes_written")?,
            transactions: sum("transactions_started")?,
            commits: sum("transactions_committed")?,
            publication_commits: sum("publication_commits")?,
            retained_union_scrubs: sum("retained_union_scrubs")?,
            scratch_tables: sum("scratch_tables")?,
            operation_scratch_tables: sum("operation_scratch_tables")?,
            q_high_water_bytes: maximum("q_high_water_bytes")?,
            active_connections: maximum("active_connections")?,
        });
    }
    Ok(output)
}

fn phase_attribution_json(values: &[PhaseAttribution]) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|value| format!(
                concat!(
                    "\"{}\":{{\"rows\":{},\"statements\":{},\"fetched_rows\":{},",
                    "\"authentication_passes\":{},\"role_decode_passes\":{},",
                    "\"object_bytes_read\":{},\"object_bytes_written\":{},",
                    "\"transactions\":{},\"commits\":{},\"publication_commits\":{},",
                    "\"retained_union_scrubs\":{},\"scratch_tables\":{},",
                    "\"operation_scratch_tables\":{},",
                    "\"q_high_water_bytes\":{},\"active_connections\":{}}}"
                ),
                value.name,
                value.rows,
                value.statements,
                value.fetched_rows,
                value.authentication_passes,
                value.role_decode_passes,
                value.object_bytes_read,
                value.object_bytes_written,
                value.transactions,
                value.commits,
                value.publication_commits,
                value.retained_union_scrubs,
                value.scratch_tables,
                value.operation_scratch_tables,
                value.q_high_water_bytes,
                value.active_connections,
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn history_probe_stats(rows: &[ParsedRow], ordinal: u8) -> EvalResult<Statistics> {
    statistics(
        rows.iter()
            .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
            .map(|row| json_array_objects(&row.json, "history_probes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .filter(|probe| json_u128(probe, "ordinal") == Ok(u128::from(ordinal)))
            .map(|probe| json_u128(probe, "wall_ns"))
            .collect::<EvalResult<Vec<_>>>()?,
    )
}

fn history_probe_sum(rows: &[ParsedRow], ordinal: u8, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .map(|row| json_array_objects(&row.json, "history_probes"))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .filter(|probe| json_u128(probe, "ordinal") == Ok(u128::from(ordinal)))
        .try_fold(0_u128, |total, probe| {
            total
                .checked_add(json_u128(probe, key)?)
                .ok_or_else(|| format!("history probe {key} sum overflow"))
        })
}

const OPTIMIZATION_BASELINE: &str = "layerfs-stage1-apple-edge-20260825-attempt-007";
const OPTIMIZATION_BASELINE_ROWS_SHA256: &str =
    "86707e36958b4e46fa2739280e7e4a6038c1fcb7693ee71ef8d7fffdb44b590e";
const OPTIMIZATION_BASELINE_SUMMARY_SHA256: &str =
    "bc5594658fb5a7973c3cfe6e3d648f1a17d95f2f3c1e433680da612e1e9d5888";

#[derive(Clone, Debug)]
struct VerifiedOpenComparison {
    root: &'static str,
    before_ns: u128,
    after_ns: u128,
    retained_union_scrubs: u128,
    namespace_graphs: u128,
    fetched_rows: u128,
    object_bytes_read: u128,
    scratch_tables: u128,
}

#[derive(Clone, Debug)]
struct OptimizationComparison {
    baseline_path: String,
    baseline_complete_wall_ns: u128,
    current_complete_wall_ns: u128,
    baseline_counter_snapshot_ns: u128,
    current_counter_snapshot_ns: u128,
    baseline_history_read_ns: u128,
    current_history_read_ns: u128,
    verified_open: Vec<VerifiedOpenComparison>,
    baseline_append_truncate: Statistics,
    current_append_truncate: Statistics,
    baseline_materialization: Statistics,
    current_materialization: Statistics,
    baseline_clone_shift: usize,
    baseline_in_place_shift: usize,
    current_clone_shift: usize,
    current_in_place_shift: usize,
}

fn optimization_comparison(
    rows: &[ParsedRow],
    current_complete_wall_ns: u128,
) -> EvalResult<OptimizationComparison> {
    let baseline = stage1_fixture::workspace_root()
        .join("target")
        .join(OPTIMIZATION_BASELINE);
    let baseline_rows_path = baseline.join("rows.jsonl");
    let baseline_summary_path = baseline.join("summary.json");
    #[cfg(test)]
    match (baseline_rows_path.exists(), baseline_summary_path.exists()) {
        (false, false) => {
            return synthetic_optimization_comparison(rows, current_complete_wall_ns, &baseline);
        }
        (true, true) => {}
        _ => return Err("incomplete accepted attempt-007 test baseline".to_owned()),
    }
    if sha256_file(&baseline_rows_path)? != OPTIMIZATION_BASELINE_ROWS_SHA256
        || sha256_file(&baseline_summary_path)? != OPTIMIZATION_BASELINE_SUMMARY_SHA256
    {
        return Err("accepted attempt-007 optimization baseline custody".to_owned());
    }
    let baseline_rows = fs::read_to_string(&baseline_rows_path)
        .map_err(io_error)?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if baseline_rows.len() != 47 {
        return Err("accepted attempt-007 baseline rows != 47".to_owned());
    }
    let baseline_summary = fs::read_to_string(&baseline_summary_path).map_err(io_error)?;
    let baseline_complete_wall_ns =
        json_u128(json_object(&baseline_summary, "walls_ns")?, "complete_wall")?;
    let baseline_phase = |row_id: &str, phase: &str| -> EvalResult<u128> {
        baseline_rows
            .iter()
            .find(|row| json_string(row, "row_id").as_deref() == Ok(row_id))
            .ok_or_else(|| format!("baseline missing {row_id}"))
            .and_then(|row| phase_wall(row, phase))
    };
    let current_phase = |row_id: &str, phase: &str| -> EvalResult<u128> {
        rows.iter()
            .find(|row| row.row_id == row_id)
            .ok_or_else(|| format!("current rows missing {row_id}"))
            .and_then(|row| phase_wall(&row.json, phase))
    };
    let baseline_stats =
        |group: &str, phase: &str, operation: Option<&str>| -> EvalResult<Statistics> {
            statistics(
                baseline_rows
                    .iter()
                    .filter(|row| json_string(row, "row_group").as_deref() == Ok(group))
                    .filter(|row| {
                        operation.is_none_or(|expected| {
                            json_string(row, "operation").as_deref() == Ok(expected)
                        })
                    })
                    .map(|row| phase_wall(row, phase))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        };
    let baseline_counter_snapshot_ns = baseline_rows
        .iter()
        .filter(|row| {
            matches!(
                json_string(row, "row_group").as_deref(),
                Ok("C03" | "C05" | "C07")
            )
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(row, "counter_snapshot")?)
                .ok_or_else(|| "baseline counter snapshot wall overflow".to_owned())
        })?;
    let current_counter_snapshot_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "counter_snapshot")?)
                .ok_or_else(|| "current counter snapshot wall overflow".to_owned())
        })?;
    let baseline_history_read_ns = baseline_rows
        .iter()
        .filter(|row| matches!(json_string(row, "row_group").as_deref(), Ok("C04" | "C06")))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(row, "history_read")?)
                .ok_or_else(|| "baseline history wall overflow".to_owned())
        })?;
    let current_history_read_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "history_read")?)
                .ok_or_else(|| "current history wall overflow".to_owned())
        })?;
    let mut baseline_append_truncate =
        baseline_stats("C05", "changed_root_refresh", Some("append"))?.raw_ns;
    baseline_append_truncate
        .extend(baseline_stats("C05", "changed_root_refresh", Some("truncate"))?.raw_ns);
    let mut current_append_truncate =
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "append"
        })?
        .raw_ns;
    current_append_truncate.extend(
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "truncate"
        })?
        .raw_ns,
    );
    let verified_open_rows = [
        ("R5", "C04-001"),
        ("R15", "C04-003"),
        ("R30", "C06-003"),
        ("R34", "C08-001"),
    ];
    let verified_open = verified_open_rows
        .into_iter()
        .map(|(root, row_id)| {
            let row = rows
                .iter()
                .find(|row| row.row_id == row_id)
                .ok_or_else(|| format!("current rows missing {root} scrub"))?;
            let phase = json_array_objects(&row.json, "phase_counters")?
                .into_iter()
                .find(|phase| json_string(phase, "name").as_deref() == Ok("verified_open"))
                .ok_or_else(|| format!("{root} scrub missing verified_open phase counters"))?;
            let retained_union_scrubs = json_u128(phase, "retained_union_scrubs")?;
            let scratch_tables = json_u128(phase, "scratch_tables")?;
            if retained_union_scrubs != 1 || scratch_tables != 2 {
                return Err(format!(
                    "{root} optimization row must be the one-scrub/two-scratch open"
                ));
            }
            Ok(VerifiedOpenComparison {
                root,
                before_ns: baseline_phase(row_id, "verified_open")?,
                after_ns: current_phase(row_id, "verified_open")?,
                retained_union_scrubs,
                namespace_graphs: json_u128(phase, "namespace_graph_verification_passes")?,
                fetched_rows: json_u128(phase, "fetched_rows")?,
                object_bytes_read: json_u128(phase, "object_bytes_read")?,
                scratch_tables,
            })
        })
        .collect::<EvalResult<Vec<_>>>()?;
    Ok(OptimizationComparison {
        baseline_path: absolute_path(&baseline),
        baseline_complete_wall_ns,
        current_complete_wall_ns,
        baseline_counter_snapshot_ns,
        current_counter_snapshot_ns,
        baseline_history_read_ns,
        current_history_read_ns,
        verified_open,
        baseline_append_truncate: statistics(baseline_append_truncate)?,
        current_append_truncate: statistics(current_append_truncate)?,
        baseline_materialization: baseline_stats("C08", "milestone_materialization", None)?,
        current_materialization: row_phase_stats(rows, "C08", "milestone_materialization")?,
        baseline_clone_shift: baseline_rows
            .iter()
            .filter(|row| json_string(row, "row_group").as_deref() == Ok("C05"))
            .filter(|row| json_string(row, "native_route").as_deref() == Ok("CloneShift"))
            .count(),
        baseline_in_place_shift: baseline_rows
            .iter()
            .filter(|row| json_string(row, "row_group").as_deref() == Ok("C05"))
            .filter(|row| json_string(row, "native_route").as_deref() == Ok("InPlaceShift"))
            .count(),
        current_clone_shift: rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
            .count(),
        current_in_place_shift: rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
            .count(),
    })
}

#[cfg(test)]
fn synthetic_optimization_comparison(
    rows: &[ParsedRow],
    current_complete_wall_ns: u128,
    baseline: &Path,
) -> EvalResult<OptimizationComparison> {
    let current_counter_snapshot_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "counter_snapshot")?)
                .ok_or_else(|| "synthetic counter snapshot wall overflow".to_owned())
        })?;
    let current_history_read_ns = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, "history_read")?)
                .ok_or_else(|| "synthetic history wall overflow".to_owned())
        })?;
    let mut append_truncate = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
        row.operation == "append"
    })?
    .raw_ns;
    append_truncate.extend(
        filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == "truncate"
        })?
        .raw_ns,
    );
    let current_append_truncate = statistics(append_truncate)?;
    let current_materialization = row_phase_stats(rows, "C08", "milestone_materialization")?;
    let verified_open = [
        ("R5", "C04-001"),
        ("R15", "C04-003"),
        ("R30", "C06-003"),
        ("R34", "C08-001"),
    ]
    .into_iter()
    .map(|(root, row_id)| {
        let row = rows
            .iter()
            .find(|row| row.row_id == row_id)
            .ok_or_else(|| format!("synthetic rows missing {row_id}"))?;
        let phase = json_array_objects(&row.json, "phase_counters")?
            .into_iter()
            .find(|phase| json_string(phase, "name").as_deref() == Ok("verified_open"))
            .ok_or_else(|| format!("synthetic rows missing {row_id} verified_open"))?;
        let retained_union_scrubs = json_u128(phase, "retained_union_scrubs")?;
        let scratch_tables = json_u128(phase, "scratch_tables")?;
        if retained_union_scrubs != 1 || scratch_tables != 2 {
            return Err(format!(
                "{root} synthetic optimization row must be the one-scrub/two-scratch open"
            ));
        }
        let after_ns = phase_wall(&row.json, "verified_open")?;
        Ok(VerifiedOpenComparison {
            root,
            before_ns: if root == "R34" {
                1_406_344_708
            } else {
                after_ns
            },
            after_ns,
            retained_union_scrubs,
            namespace_graphs: json_u128(phase, "namespace_graph_verification_passes")?,
            fetched_rows: json_u128(phase, "fetched_rows")?,
            object_bytes_read: json_u128(phase, "object_bytes_read")?,
            scratch_tables,
        })
    })
    .collect::<EvalResult<Vec<_>>>()?;
    let current_clone_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
        .count();
    let current_in_place_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
        .count();
    Ok(OptimizationComparison {
        baseline_path: absolute_path(baseline),
        baseline_complete_wall_ns: current_complete_wall_ns,
        current_complete_wall_ns,
        baseline_counter_snapshot_ns: current_counter_snapshot_ns,
        current_counter_snapshot_ns,
        baseline_history_read_ns: current_history_read_ns,
        current_history_read_ns,
        verified_open,
        baseline_append_truncate: current_append_truncate.clone(),
        current_append_truncate,
        baseline_materialization: current_materialization.clone(),
        current_materialization,
        baseline_clone_shift: current_clone_shift,
        baseline_in_place_shift: current_in_place_shift,
        current_clone_shift,
        current_in_place_shift,
    })
}

fn signed_gain(before: u128, after: u128) -> EvalResult<i128> {
    let before = i128::try_from(before).map_err(display_error)?;
    let after = i128::try_from(after).map_err(display_error)?;
    before
        .checked_sub(after)
        .ok_or_else(|| "optimization gain overflow".to_owned())
}

fn optimization_json(value: &OptimizationComparison) -> EvalResult<String> {
    let verified = value
        .verified_open
        .iter()
        .map(|receipt| -> EvalResult<String> {
            Ok(format!(
                concat!(
                    "\"{}\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{},",
                    "\"retained_union_scrubs\":{},\"namespace_graphs\":{},",
                    "\"fetched_rows\":{},\"object_bytes_read\":{},\"scratch_tables\":{}}}"
                ),
                receipt.root,
                receipt.before_ns,
                receipt.after_ns,
                signed_gain(receipt.before_ns, receipt.after_ns)?,
                receipt.retained_union_scrubs,
                receipt.namespace_graphs,
                receipt.fetched_rows,
                receipt.object_bytes_read,
                receipt.scratch_tables,
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    Ok(format!(
        concat!(
            "{{\"baseline_run\":\"{}\",\"baseline_rows_sha256\":\"{}\",",
            "\"baseline_summary_sha256\":\"{}\",",
            "\"complete_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"counter_snapshot_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"history_read_wall\":{{\"before_ns\":{},\"after_ns\":{},\"gain_ns\":{}}},",
            "\"verified_open_by_root\":{{{}}},",
            "\"append_truncate_refresh\":{{\"before\":{},\"after\":{}}},",
            "\"milestone_materialization\":{{\"before\":{},\"after\":{}}},",
            "\"shift_routes\":{{\"before_clone\":{},\"before_in_place\":{},",
            "\"after_clone\":{},\"after_in_place\":{}}}}}"
        ),
        json_escape(&value.baseline_path),
        OPTIMIZATION_BASELINE_ROWS_SHA256,
        OPTIMIZATION_BASELINE_SUMMARY_SHA256,
        value.baseline_complete_wall_ns,
        value.current_complete_wall_ns,
        signed_gain(
            value.baseline_complete_wall_ns,
            value.current_complete_wall_ns
        )?,
        value.baseline_counter_snapshot_ns,
        value.current_counter_snapshot_ns,
        signed_gain(
            value.baseline_counter_snapshot_ns,
            value.current_counter_snapshot_ns,
        )?,
        value.baseline_history_read_ns,
        value.current_history_read_ns,
        signed_gain(
            value.baseline_history_read_ns,
            value.current_history_read_ns
        )?,
        verified,
        stats_json(&value.baseline_append_truncate),
        stats_json(&value.current_append_truncate),
        stats_json(&value.baseline_materialization),
        stats_json(&value.current_materialization),
        value.baseline_clone_shift,
        value.baseline_in_place_shift,
        value.current_clone_shift,
        value.current_in_place_shift,
    ))
}

fn first_group_value(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .find(|row| row.row_group == group)
        .ok_or_else(|| format!("missing row group {group}"))
        .and_then(|row| row_u128(row, key))
}

fn last_group_value(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .rev()
        .find(|row| row.row_group == group)
        .ok_or_else(|| format!("missing row group {group}"))
        .and_then(|row| row_u128(row, key))
}

fn summary_json(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    source: &SourceIdentity,
    master: &FixtureMaster,
    complete_wall_ns: u128,
    campaign_time_sha256: &str,
) -> EvalResult<String> {
    let disposition = derive_disposition(rows);
    validate_ref_chain(rows, campaign.schedule)?;
    let authentication_validation = validate_authentication(rows)?;
    validate_locality_rows(rows)?;
    validate_phase_counter_rows(rows)?;
    validate_refresh_rows(rows)?;
    validate_availability_rows(rows)?;
    let selected_history_roots_passed = validate_history_rows(rows)?;
    let roots = roots_from_rows(rows)?;
    let physical_native = row_phase_stats(rows, "C03", "native_edit")?;
    let physical_checkpoint = row_phase_stats(rows, "C03", "durable_checkpoint")?;
    let physical_combined =
        combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |_| true)?;
    let logical_edit = row_phase_stats(rows, "C05", "direct_logical_edit")?;
    let logical_refresh = row_phase_stats(rows, "C05", "changed_root_refresh")?;
    let logical_combined = combined_phase_stats(
        rows,
        "C05",
        "direct_logical_edit",
        "changed_root_refresh",
        |_| true,
    )?;
    let c03_oracle = row_phase_stats(rows, "C03", "live_physical_oracle")?;
    let c05_oracle = row_phase_stats(rows, "C05", "live_physical_oracle")?;
    let burst_stats = row_phase_stats(rows, "C07", "durable_checkpoint")?;
    let burst_oracles = statistics(
        rows.iter()
            .filter(|row| row.row_group == "C07")
            .flat_map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns").unwrap_or_default())
            .collect(),
    )?;
    if burst_oracles.raw_ns.len() != 21 {
        return Err("burst physical-oracle population != 21".to_owned());
    }
    let history = statistics(
        rows.iter()
            .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
            .map(|row| phase_wall(&row.json, "verified_open"))
            .collect::<EvalResult<Vec<_>>>()?,
    )?;
    let first_history_probes = history_probe_stats(rows, 1)?;
    let second_history_probes = history_probe_stats(rows, 2)?;
    let third_history_probes = history_probe_stats(rows, 3)?;
    let phase_attribution = phase_attributions(rows)?;
    let optimization = optimization_comparison(rows, complete_wall_ns)?;
    let milestones = row_phase_stats(rows, "C08", "milestone_materialization")?;
    let patch = route_stats(rows, "Patch", None)?;
    let shift = route_stats(rows, "Shift", None)?;
    let insert_shift = route_stats(rows, "Shift", Some("insert"))?;
    let delete_shift = route_stats(rows, "Shift", Some("delete"))?;
    let append_shift = route_stats(rows, "Shift", Some("append"))?;
    let truncate_shift = route_stats(rows, "Shift", Some("truncate"))?;
    let clone_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "ClonePatch")
        .collect::<Vec<_>>();
    let in_place_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlacePatch")
        .collect::<Vec<_>>();
    let clone_shift_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
        .collect::<Vec<_>>();
    let in_place_shift_rows = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
        .collect::<Vec<_>>();
    let clone_stats = (!clone_rows.is_empty())
        .then(|| {
            statistics(
                clone_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let in_place_stats = (!in_place_rows.is_empty())
        .then(|| {
            statistics(
                in_place_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let clone_shift_stats = (!clone_shift_rows.is_empty())
        .then(|| {
            statistics(
                clone_shift_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let in_place_shift_stats = (!in_place_shift_rows.is_empty())
        .then(|| {
            statistics(
                in_place_shift_rows
                    .iter()
                    .map(|row| phase_wall(&row.json, "changed_root_refresh"))
                    .collect::<EvalResult<Vec<_>>>()?,
            )
        })
        .transpose()?;
    let full_fallback_count = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "FullFallback")
        .count();
    let row_wall_sum = rows.iter().try_fold(0_u128, |total, row| {
        total
            .checked_add(row.row_wall_ns)
            .ok_or_else(|| "row wall sum overflow".to_owned())
    })?;
    if row_wall_sum != campaign.row_wall_sum_ns {
        return Err("summary row wall sum derives exactly from rows.jsonl".to_owned());
    }
    let outside_rows = complete_wall_ns
        .checked_sub(row_wall_sum)
        .ok_or_else(|| "complete wall below row wall sum".to_owned())?;
    let cdc_physical = sum_key(rows, Some("C03"), "cdc_bytes_scanned")?;
    let cdc_logical = sum_key(rows, Some("C05"), "cdc_bytes_scanned")?;
    let cdc_bursts = sum_key(rows, Some("C07"), "cdc_bytes_scanned")?;
    let cdc_total = cdc_physical + cdc_logical + cdc_bursts;
    if cdc_total != REPLACEMENT_BACKING_BYTES as u128 {
        return Err("canonical CDC total = 495,616".to_owned());
    }
    let transactions = sum_key(rows, None, "transactions_started")?;
    let commits = sum_key(rows, None, "transactions_committed")?;
    let rollbacks = sum_key(rows, None, "transactions_rolled_back")?;
    let publications = sum_key(rows, None, "publication_commits")?;
    if (transactions, commits, rollbacks, publications) != (34, 34, 0, 34) {
        return Err("summary transaction/COMMIT closure".to_owned());
    }
    let initial_database = row_u128(
        rows.iter()
            .find(|row| row.row_group == "C02")
            .ok_or_else(|| "missing C02 storage observation".to_owned())?,
        "database_bytes",
    )?;
    let terminal_database = row_u128(
        rows.iter()
            .rev()
            .find(|row| row.row_group == "C07")
            .ok_or_else(|| "missing terminal C07 storage observation".to_owned())?,
        "database_bytes",
    )?;
    let database_growth = terminal_database.saturating_sub(initial_database);
    let canonical_written = sum_key(rows, None, "canonical_object_bytes_written")?;
    let initial_logical_engine = first_group_value(rows, "C02", "logical_engine_bytes")?;
    let terminal_logical_engine = last_group_value(rows, "C07", "logical_engine_bytes")?;
    if terminal_database < initial_database || terminal_logical_engine < initial_logical_engine {
        return Err("summary storage monotonicity".to_owned());
    }
    let rss_peak = maximum_key(rows, "rss_peak_bytes")?;
    let q_high_water = maximum_key(rows, "operation_q_high_water_bytes")?;
    let q_terminal = maximum_key(rows, "operation_q_terminal_bytes")?;
    let connection_high_water = maximum_key(rows, "active_store_connections")?;
    let c09 = rows
        .iter()
        .find(|row| row.row_group == "C09")
        .ok_or_else(|| "missing C09 terminal row".to_owned())?;
    let connections_terminal = row_u128(c09, "active_store_connections")?;
    let fd_terminal = row_u128(c09, "fd_current")?;
    let child_peak = maximum_key(rows, "child_processes")?;
    let child_terminal = row_u128(c09, "child_processes")?;
    let owned_temp_terminal = row_u128(c09, "owned_temp_entries")?;
    let residue_terminal = row_u128(c09, "residue_entries")?;
    let network_operations = maximum_key(rows, "network_operations")?;
    let live_rematerializations = sum_key(rows, None, "rematerializations")?;
    let pre_cleanup_residue = json_u128(&c09.json, "pre_cleanup_residue_entries")?;
    let pre_cleanup_connections = json_u128(&c09.json, "pre_cleanup_active_store_connections")?;
    let fd_baseline = json_u128(&c09.json, "pre_cleanup_fd_count")?;
    if rss_peak > 33_554_432
        || q_high_water > 8_388_608
        || q_terminal != 0
        || connection_high_water > 2
        || connections_terminal != 0
        || fd_terminal != fd_baseline
        || child_peak != 0
        || child_terminal != 0
        || owned_temp_terminal != 0
        || residue_terminal != 0
        || pre_cleanup_residue != 0
        || pre_cleanup_connections != 0
        || network_operations != 0
        || live_rematerializations != 0
    {
        return Err("rows-derived summary resource closure".to_owned());
    }
    let physical_oracles_passed = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .filter(|row| json_bool(&row.json, "physical_bytes_exact") == Ok(true))
        .count()
        + rows
            .iter()
            .filter(|row| row.row_group == "C07")
            .map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns"))
            .collect::<EvalResult<Vec<_>>>()?
            .iter()
            .map(Vec::len)
            .sum::<usize>();
    let transition_rows = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    let burst_suboperations = rows
        .iter()
        .filter(|row| row.row_group == "C07")
        .map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns"))
        .collect::<EvalResult<Vec<_>>>()?
        .iter()
        .map(Vec::len)
        .sum::<usize>();
    let observed_edit_suboperations = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .count()
        + burst_suboperations;
    let history_session_count = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .count();
    let witness_materializations = rows.iter().filter(|row| row.row_group == "C08").count();
    let live_workspace_materializations = sum_key(rows, None, "workspace_materializations")?;
    let workspace_reuses = sum_key(rows, None, "workspace_reuses")?;
    let canonical_transitions_passed = transition_rows
        .iter()
        .filter(|row| json_bool(&row.json, "canonical_bytes_exact") == Ok(true))
        .count();
    let route_labels_exact = transition_rows
        .iter()
        .all(|row| json_bool(&row.json, "route_exact") == Ok(true));
    let save_bursts_passed = rows
        .iter()
        .filter(|row| row.row_group == "C07" && row.status == "PASS")
        .count();
    let r34 = rows
        .iter()
        .find(|row| row.row_id == "C08-003")
        .ok_or_else(|| "missing R34 terminal witness".to_owned())?;
    let r34_oracle = json_object(&r34.json, "oracle")?;
    let terminal_length_exact = json_u128(r34_oracle, "logical_length")?
        == u128::from(INITIAL_BYTES)
        && json_bool(r34_oracle, "physical_bytes_exact")?
        && json_bool(r34_oracle, "canonical_bytes_exact")?;
    let fixture_unchanged = json_bool(&c09.json, "fixture_unchanged")?;
    if physical_oracles_passed != 51
        || canonical_transitions_passed != 34
        || !route_labels_exact
        || save_bursts_passed != 4
        || selected_history_roots_passed != 8
        || !terminal_length_exact
        || !fixture_unchanged
        || rows.len() != 47
        || observed_edit_suboperations != 51
        || transition_rows.len() != 34
        || burst_suboperations != 21
        || history_session_count != 6
        || witness_materializations != 3
        || live_workspace_materializations != 1
        || workspace_reuses != 34
    {
        return Err("rows-derived summary correctness closure".to_owned());
    }
    let artifacts = format!(
        concat!(
            "\"environment_sha256\":\"{}\",\"master_sha256\":\"{}\",",
            "\"readiness_sha256\":\"{}\",\"schedule_sha256\":\"{}\",",
            "\"rows_sha256\":\"{}\",\"rows_line_count\":47,",
            "\"campaign_time_sha256\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\",",
            "\"source_tree_blake3\":\"{}\",",
            "\"source_manifest_sha256\":\"{}\""
        ),
        sha256_file(&campaign.run.join("environment.json"))?,
        sha256_file(&campaign.run.join("master.json"))?,
        sha256_file(&campaign.run.join("readiness.json"))?,
        sha256_file(&campaign.run.join("schedule.json"))?,
        sha256_file(&campaign.run.join("rows.jsonl"))?,
        campaign_time_sha256,
        source.executable_sha256,
        source.executable_blake3,
        source.tree_blake3,
        source.manifest_sha256,
    );
    let by_row_group = ["C00", "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09"]
        .into_iter()
        .map(|group| {
            let values = rows
                .iter()
                .filter(|row| row.row_group == group)
                .map(|row| row.row_residual_ns)
                .collect::<Vec<_>>();
            let maximum = values.iter().copied().max().unwrap_or(0);
            let sum = values.iter().copied().sum::<u128>();
            format!("\"{group}\":{{\"rows\":{},\"maximum_residual_ns\":{maximum},\"sum_residual_ns\":{sum}}}", values.len())
        })
        .collect::<Vec<_>>()
        .join(",");
    let max_residual = rows
        .iter()
        .map(|row| row.row_residual_ns)
        .max()
        .unwrap_or(0);
    let residual_sum = rows.iter().map(|row| row.row_residual_ns).sum::<u128>();
    let by_root = rows
        .iter()
        .filter(|row| row.row_group == "C07")
        .zip(&campaign.schedule.bursts)
        .map(|(row, burst)| -> EvalResult<String> {
            Ok(format!(
                "\"R{}\":{{\"pattern\":\"{}\",\"checkpoint\":{}}}",
                burst.root,
                burst.pattern,
                stats_json(&statistics(vec![phase_wall(
                    &row.json,
                    "durable_checkpoint"
                )?])?)
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    let admission_wall = sum_row_walls(rows, "C00")?;
    let reset_wall = sum_row_walls(rows, "C01")?;
    let store_open_wall = sum_phase(rows, "C02", "store_open")?;
    let initial_materialization_wall = sum_row_walls(rows, "C02")?
        .checked_sub(store_open_wall)
        .ok_or_else(|| "C02 named wall underflow".to_owned())?;
    let cleanup_wall = sum_row_walls(rows, "C09")?;
    let failure_ledger = preserved_failure_ledger(campaign.run)?;
    let failure_ledger = failure_ledger_json(&failure_ledger);
    let mut summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-summary-v1\",\"status\":\"PASS\",",
            "\"source\":{{\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"tree_blake3\":\"{}\",\"manifest_sha256\":\"{}\",",
            "\"release_executable_path\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\"}},",
            "\"fixture\":{{\"master_path\":\"{}\",\"master_sha256\":\"{}\",",
            "\"fixture_blake3\":\"{}\",\"apfs_identity\":\"{}\",",
            "\"initial_bytes\":{},\"maximum_bytes\":{},\"terminal_bytes\":{},",
            "\"master_unchanged\":true}},",
            "\"population\":{{\"expected_rows\":47,\"valid_rows\":{},",
            "\"expected_edit_suboperations\":51,\"observed_edit_suboperations\":{},",
            "\"expected_transitions\":34,\"observed_transitions\":{},",
            "\"measured_workflows\":1}},",
            "\"roots\":{{{}}},",
            "\"walls_ns\":{{\"complete_wall\":{},\"row_wall_sum\":{},",
            "\"outside_rows_wall\":{},\"timer_residual\":0,",
            "\"admission\":{},\"reset\":{},\"store_open\":{},",
            "\"initial_materialization\":{},\"physical_phase\":{},",
            "\"physical_history_phase\":{},\"logical_refresh_phase\":{},",
            "\"logical_history_phase\":{},\"burst_phase\":{},",
            "\"milestone_materialization_phase\":{},\"cleanup\":{},",
            "\"artifact_write\":{}}},",
            "\"physical_to_logical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"native_edit\":{},\"durable_checkpoint\":{},",
            "\"edit_plus_checkpoint\":{},\"count_change_amplification\":{},",
            "\"physical_oracle\":{}}},",
            "\"logical_to_physical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"direct_logical_edit\":{},\"changed_root_refresh\":{},",
            "\"logical_edit_plus_refresh\":{},\"physical_oracle\":{}}},",
            "\"refresh_routes\":{{\"clone_patch\":{},\"in_place_patch\":{},",
            "\"patch_aggregate\":{},\"clone_shift\":{},\"in_place_shift\":{},",
            "\"shift_aggregate\":{},\"insert_shift\":{},\"delete_shift\":{},",
            "\"append_shift\":{},\"truncate_shift\":{},\"full_fallback_count\":{}}},",
            "\"bursts\":{{\"by_root\":{{{}}},\"aggregate\":{},",
            "\"suboperation_count\":{},\"checkpoint_count\":{},",
            "\"transaction_count\":{}}},",
            "\"history\":{{\"sessions\":{},\"aggregate\":{},",
            "\"selected_roots\":{},\"verified_open_count\":{},",
            "\"probe_count\":63,\"first_probe\":{},\"second_probe\":{},",
            "\"third_probe\":{},\"first_probe_non_payload_rows\":{},",
            "\"warm_probe_non_payload_rows\":{}}},",
            "\"materialization\":{{\"initial\":{},\"by_root\":{},",
            "\"milestone_aggregate\":{},\"live_workspace_materializations\":{},",
            "\"witness_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{}}},",
            "\"canonical_locality\":{{\"physical_checkpoints\":{},",
            "\"direct_logical_edits\":{},\"save_bursts\":{},",
            "\"total\":{},\"cdc_bytes_expected\":495616,",
            "\"cdc_bytes_observed\":{},\"payload_bytes_written\":{},",
            "\"unaffected_payload_reads\":{},\"unaffected_payload_writes\":{},",
            "\"maximum_rope_nodes_read\":{},\"maximum_rope_nodes_emitted\":{},",
            "\"content_directory_nodes_emitted\":{},\"payload_batch_maximum\":{}}},",
            "\"transactions\":{{\"expected\":34,\"observed\":{},",
            "\"committed\":{},\"rolled_back\":{},\"publication_commits\":{},",
            "\"publication_transactions_started\":{},",
            "\"publication_transactions_rolled_back\":{},",
            "\"admission_transactions_started\":{},",
            "\"admission_transactions_committed\":{},",
            "\"admission_transactions_rolled_back\":{},\"admission_statements\":{},",
            "\"integrity_transactions_started\":{},",
            "\"integrity_transactions_committed\":{},",
            "\"integrity_transactions_rolled_back\":{},\"integrity_statements\":{},",
            "\"retained_roots_validated\":{},",
            "\"generation_increment_failures\":0}},",
            "\"authentication\":{{\"fetched_authentication_failures\":{},",
            "\"fetched_role_decode_failures\":{},\"new_object_equation_failures\":{},",
            "\"incumbent_equation_failures\":{},\"payload_batch_maximum\":{},",
            "\"phase_attribution\":{}}},",
            "\"storage\":{{\"initial_database_bytes\":{},",
            "\"terminal_database_bytes\":{},\"initial_logical_engine_bytes\":{},",
            "\"terminal_logical_engine_bytes\":{},",
            "\"canonical_object_bytes_written\":{},\"database_growth_bytes\":{},",
            "\"maximum_transition_database_growth_bytes\":{},",
            "\"physical_to_canonical_amplification\":{},",
            "\"scratch_high_water_bytes\":{},\"rollback_journal_bytes\":null,",
            "\"terminal_sidecars\":\"absent\",\"by_root_range\":{}}},",
            "\"resources\":{{\"rss_peak_bytes\":{},\"largest_buffer_bytes\":{},",
            "\"operation_q_high_water_bytes\":{},",
            "\"operation_q_maximum_terminal_bytes\":{},\"page_size\":4096,",
            "\"cache_pages\":1280,\"cache_spill_pages\":1280,",
            "\"store_connection_high_water\":{},\"store_connections_terminal\":{},",
            "\"fd_baseline\":{},\"fd_terminal\":{},",
            "\"product_child_process_peak\":{},\"child_processes_terminal\":{},",
            "\"owned_temp_residue_entries\":{},\"sidecar_residue_entries\":{},",
            "\"live_rematerializations\":{},\"network_operations\":{}}},",
            "\"timer_closure\":{{\"by_row_group\":{{{}}},",
            "\"maximum_row_residual_ns\":{},\"row_residual_sum_ns\":{},",
            "\"complete_wall_ns\":{},\"row_wall_sum_ns\":{},",
            "\"outside_rows_wall_ns\":{},\"timer_residual_ns\":0,",
            "\"hard_limit_ns\":60000000000}},",
            "\"correctness\":{{\"physical_oracles_expected\":51,",
            "\"physical_oracles_passed\":{},\"canonical_transitions_expected\":34,",
            "\"canonical_transitions_passed\":{},\"save_bursts_expected\":4,",
            "\"save_bursts_passed\":{},\"selected_history_roots_expected\":8,",
            "\"selected_history_roots_passed\":{},\"route_labels_exact\":{},",
            "\"terminal_length_exact\":{},\"fixture_unchanged\":{}}},",
            "\"optimization\":{},",
            "\"unavailable\":[",
            "{{\"field\":\"native.sync_regular_calls\",\"availability\":\"Unavailable\",\"reason\":\"product exposes only aggregate sync_calls\"}},",
            "{{\"field\":\"native.sync_directory_calls\",\"availability\":\"Unavailable\",\"reason\":\"product exposes only aggregate sync_calls\"}},",
            "{{\"field\":\"storage.rollback_journal_bytes\",\"availability\":\"Unavailable\",\"reason\":\"not continuously observed\"}},",
            "{{\"field\":\"storage.temporary_file_bytes\",\"availability\":\"Unavailable\",\"reason\":\"not continuously observed\"}}],",
            "\"failures\":[{}],\"artifacts\":{{{}}},",
            "\"disposition_reason\":\"All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.\"}}\n"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        json_escape(&fixture_root().join("master.json").display().to_string()),
        sha256_file(&campaign.run.join("master.json"))?,
        master.fixture_blake3,
        json_escape(&master.apfs_identity),
        INITIAL_BYTES,
        MAXIMUM_BYTES,
        INITIAL_BYTES,
        rows.len(),
        observed_edit_suboperations,
        transition_rows.len(),
        root_json(&roots),
        complete_wall_ns,
        row_wall_sum,
        outside_rows,
        admission_wall,
        reset_wall,
        store_open_wall,
        initial_materialization_wall,
        sum_row_walls(rows, "C03")?,
        sum_row_walls(rows, "C04")?,
        sum_row_walls(rows, "C05")?,
        sum_row_walls(rows, "C06")?,
        sum_row_walls(rows, "C07")?,
        sum_row_walls(rows, "C08")?,
        cleanup_wall,
        outside_rows,
        physical_by_kind_json(rows)?,
        physical_by_size_json(rows)?,
        stats_json(&physical_native),
        stats_json(&physical_checkpoint),
        stats_json(&physical_combined),
        count_change_amplification_json(rows)?,
        stats_json(&c03_oracle),
        logical_by_kind_json(rows)?,
        logical_by_size_json(rows)?,
        stats_json(&logical_edit),
        stats_json(&logical_refresh),
        stats_json(&logical_combined),
        stats_json(&c05_oracle),
        clone_stats.as_ref().map_or_else(|| "null".to_owned(), stats_json),
        in_place_stats.as_ref().map_or_else(|| "null".to_owned(), stats_json),
        stats_json(&patch),
        clone_shift_stats
            .as_ref()
            .map_or_else(|| "null".to_owned(), stats_json),
        in_place_shift_stats
            .as_ref()
            .map_or_else(|| "null".to_owned(), stats_json),
        stats_json(&shift),
        stats_json(&insert_shift),
        stats_json(&delete_shift),
        stats_json(&append_shift),
        stats_json(&truncate_shift),
        full_fallback_count,
        by_root,
        stats_json(&burst_stats),
        burst_suboperations,
        save_bursts_passed,
        sum_key(rows, Some("C07"), "transactions_started")?,
        history_session_count,
        stats_json(&history),
        selected_history_roots_passed,
        history_session_count,
        stats_json(&first_history_probes),
        stats_json(&second_history_probes),
        stats_json(&third_history_probes),
        history_probe_sum(rows, 1, "non_payload_rows")?,
        history_probe_sum(rows, 2, "non_payload_rows")?
            + history_probe_sum(rows, 3, "non_payload_rows")?,
        stats_json(&statistics(vec![sum_phase(rows, "C02", "cold_materialization")?])?),
        materialization_by_root_json(rows)?,
        stats_json(&milestones),
        live_workspace_materializations,
        witness_materializations,
        workspace_reuses,
        live_rematerializations,
        cdc_physical,
        cdc_logical,
        cdc_bursts,
        cdc_total,
        cdc_total,
        sum_locality_key(rows, "payload_bytes_written")?,
        sum_locality_key(rows, "unaffected_payload_reads")?,
        sum_locality_key(rows, "unaffected_payload_writes")?,
        maximum_locality_key(rows, "rope_nodes_read")?,
        maximum_locality_key(rows, "rope_nodes_emitted")?,
        sum_locality_key(rows, "content_directory_nodes_emitted")?,
        maximum_key(rows, "payload_batch_maximum")?,
        transactions,
        commits,
        rollbacks,
        publications,
        sum_key(rows, None, "publication_transactions_started")?,
        sum_key(rows, None, "publication_transactions_rolled_back")?,
        sum_key(rows, None, "admission_transactions_started")?,
        sum_key(rows, None, "admission_transactions_committed")?,
        sum_key(rows, None, "admission_transactions_rolled_back")?,
        sum_key(rows, None, "admission_statements")?,
        sum_key(rows, None, "integrity_transactions_started")?,
        sum_key(rows, None, "integrity_transactions_committed")?,
        sum_key(rows, None, "integrity_transactions_rolled_back")?,
        sum_key(rows, None, "integrity_statements")?,
        sum_key(rows, None, "retained_roots_validated")?,
        authentication_validation.fetched_authentication_failures,
        authentication_validation.fetched_role_decode_failures,
        authentication_validation.new_object_equation_failures,
        authentication_validation.incumbent_equation_failures,
        authentication_validation.payload_batch_maximum,
        phase_attribution_json(&phase_attribution),
        initial_database,
        terminal_database,
        initial_logical_engine,
        terminal_logical_engine,
        canonical_written,
        database_growth,
        maximum_key(rows, "database_growth_bytes")?,
        if canonical_written == 0 { 0.0 } else { database_growth as f64 / canonical_written as f64 },
        maximum_key(rows, "scratch_high_water_bytes")?,
        storage_by_root_range_json(rows)?,
        rss_peak,
        PRODUCT_BUFFER_BOUND_BYTES,
        q_high_water,
        q_terminal,
        connection_high_water,
        connections_terminal,
        fd_baseline,
        fd_terminal,
        child_peak,
        child_terminal,
        owned_temp_terminal,
        residue_terminal,
        live_rematerializations,
        network_operations,
        by_row_group,
        max_residual,
        residual_sum,
        complete_wall_ns,
        row_wall_sum,
        outside_rows,
        physical_oracles_passed,
        canonical_transitions_passed,
        save_bursts_passed,
        selected_history_roots_passed,
        route_labels_exact,
        terminal_length_exact,
        fixture_unchanged,
        optimization_json(&optimization)?,
        failure_ledger,
        artifacts,
    );
    if disposition != Disposition::Pass {
        summary = summary.replacen(
            "\"status\":\"PASS\"",
            &format!("\"status\":\"{}\"", disposition.as_str()),
            1,
        );
        summary = summary.replacen(
            "All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.",
            "All hard gates passed; a retained report-only observation requires source review before PASS.",
            1,
        );
    }
    validate_named_wall_equation(&summary)?;
    validate_summary_json_contract(&summary)?;
    Ok(summary)
}

fn validate_named_wall_equation(summary: &str) -> EvalResult<()> {
    let walls = json_object(summary, "walls_ns")?;
    let complete = json_u128(walls, "complete_wall")?;
    let row_wall = json_u128(walls, "row_wall_sum")?;
    let outside = json_u128(walls, "outside_rows_wall")?;
    let residual = json_u128(walls, "timer_residual")?;
    if complete
        != row_wall
            .checked_add(outside)
            .and_then(|value| value.checked_add(residual))
            .ok_or_else(|| "summary row/outside wall overflow".to_owned())?
    {
        return Err("summary row/outside named wall equation".to_owned());
    }
    let named = [
        "admission",
        "reset",
        "store_open",
        "initial_materialization",
        "physical_phase",
        "physical_history_phase",
        "logical_refresh_phase",
        "logical_history_phase",
        "burst_phase",
        "milestone_materialization_phase",
        "cleanup",
        "artifact_write",
    ]
    .into_iter()
    .try_fold(0_u128, |total, key| {
        total
            .checked_add(json_u128(walls, key)?)
            .ok_or_else(|| "summary named wall overflow".to_owned())
    })?;
    if complete
        != named
            .checked_add(residual)
            .ok_or_else(|| "summary named wall residual overflow".to_owned())?
    {
        return Err("summary complete named wall equation".to_owned());
    }
    Ok(())
}

fn json_object_member_names(object: &str) -> EvalResult<Vec<String>> {
    let bytes = object.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("JSON object does not start with {".to_owned());
    }
    let mut names = Vec::new();
    let mut index = 1_usize;
    let mut object_depth = 1_u32;
    let mut array_depth = 0_u32;
    let mut expects_key = true;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index + 1;
                index = start;
                let mut escaped = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        break;
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    return Err("unterminated JSON string".to_owned());
                }
                if object_depth == 1 && array_depth == 0 && expects_key {
                    let key = &object[start..index];
                    if key.contains('\\') {
                        return Err("escaped summary JSON key is unsupported".to_owned());
                    }
                    names.push(key.to_owned());
                    expects_key = false;
                }
            }
            b'{' => object_depth += 1,
            b'}' => {
                object_depth = object_depth
                    .checked_sub(1)
                    .ok_or_else(|| "summary JSON object depth underflow".to_owned())?;
                if object_depth == 0 {
                    return Ok(names);
                }
            }
            b'[' => array_depth += 1,
            b']' => {
                array_depth = array_depth
                    .checked_sub(1)
                    .ok_or_else(|| "summary JSON array depth underflow".to_owned())?;
            }
            b',' if object_depth == 1 && array_depth == 0 => expects_key = true,
            _ => {}
        }
        index += 1;
    }
    Err("unterminated summary JSON object".to_owned())
}

fn json_top_level_value<'a>(json: &'a str, expected: &str) -> EvalResult<&'a str> {
    let bytes = json.as_bytes();
    if bytes.first() != Some(&b'{') {
        return Err("JSON object does not start with {".to_owned());
    }
    let mut index = 1_usize;
    loop {
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b',')
        {
            index += 1;
        }
        if bytes.get(index) == Some(&b'}') {
            return Err(format!("missing top-level JSON value {expected}"));
        }
        if bytes.get(index) != Some(&b'"') {
            return Err("invalid top-level JSON key".to_owned());
        }
        let key_start = index + 1;
        index = key_start;
        while bytes.get(index).is_some_and(|byte| *byte != b'"') {
            if bytes[index] == b'\\' {
                return Err("escaped top-level JSON key is unsupported".to_owned());
            }
            index += 1;
        }
        if bytes.get(index) != Some(&b'"') {
            return Err("unterminated top-level JSON key".to_owned());
        }
        let key = &json[key_start..index];
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if bytes.get(index) != Some(&b':') {
            return Err("top-level JSON key lacks colon".to_owned());
        }
        index += 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        if key == expected {
            return Ok(&json[index..]);
        }

        let mut object_depth = 0_u32;
        let mut array_depth = 0_u32;
        let mut string = false;
        let mut escaped = false;
        loop {
            let byte = *bytes
                .get(index)
                .ok_or_else(|| "unterminated top-level JSON value".to_owned())?;
            if string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    string = false;
                }
                index += 1;
                continue;
            }
            if object_depth == 0 && array_depth == 0 && matches!(byte, b',' | b'}') {
                break;
            }
            match byte {
                b'"' => string = true,
                b'{' => object_depth += 1,
                b'}' => {
                    object_depth = object_depth
                        .checked_sub(1)
                        .ok_or_else(|| "top-level JSON object depth underflow".to_owned())?;
                }
                b'[' => array_depth += 1,
                b']' => {
                    array_depth = array_depth
                        .checked_sub(1)
                        .ok_or_else(|| "top-level JSON array depth underflow".to_owned())?;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

fn json_top_level_string(json: &str, key: &str) -> EvalResult<String> {
    let value = json_top_level_value(json, key)?;
    if !value.starts_with('"') {
        return Err(format!("invalid top-level JSON string {key}"));
    }
    json_string(&format!("{{\"value\":{value}"), "value")
}

fn json_top_level_u128(json: &str, key: &str) -> EvalResult<u128> {
    parse_digits(json_top_level_value(json, key)?, key)
}

fn require_json_keys(json: &str, object: Option<&str>, expected: &[&str]) -> EvalResult<()> {
    let value = object.map_or(Ok(json), |key| json_object(json, key))?;
    let actual = json_object_member_names(value)?;
    if actual != expected {
        return Err(format!(
            "summary JSON {} keys {actual:?} != {expected:?}",
            object.unwrap_or("top-level")
        ));
    }
    Ok(())
}

fn validate_summary_json_contract(json: &str) -> EvalResult<()> {
    require_json_keys(
        json,
        None,
        &[
            "schema",
            "status",
            "source",
            "fixture",
            "population",
            "roots",
            "walls_ns",
            "physical_to_logical",
            "logical_to_physical",
            "refresh_routes",
            "bursts",
            "history",
            "materialization",
            "canonical_locality",
            "transactions",
            "authentication",
            "storage",
            "resources",
            "timer_closure",
            "correctness",
            "optimization",
            "unavailable",
            "failures",
            "artifacts",
            "disposition_reason",
        ],
    )?;
    for (object, keys) in [
        (
            "source",
            &[
                "git_commit",
                "dirty_tree",
                "tree_blake3",
                "manifest_sha256",
                "release_executable_path",
                "release_executable_sha256",
                "release_executable_blake3",
            ][..],
        ),
        (
            "fixture",
            &[
                "master_path",
                "master_sha256",
                "fixture_blake3",
                "apfs_identity",
                "initial_bytes",
                "maximum_bytes",
                "terminal_bytes",
                "master_unchanged",
            ],
        ),
        (
            "population",
            &[
                "expected_rows",
                "valid_rows",
                "expected_edit_suboperations",
                "observed_edit_suboperations",
                "expected_transitions",
                "observed_transitions",
                "measured_workflows",
            ],
        ),
        (
            "roots",
            &[
                "R0", "R5", "R10", "R15", "R20", "R25", "R30", "R31", "R32", "R33", "R34",
            ],
        ),
        (
            "walls_ns",
            &[
                "complete_wall",
                "row_wall_sum",
                "outside_rows_wall",
                "timer_residual",
                "admission",
                "reset",
                "store_open",
                "initial_materialization",
                "physical_phase",
                "physical_history_phase",
                "logical_refresh_phase",
                "logical_history_phase",
                "burst_phase",
                "milestone_materialization_phase",
                "cleanup",
                "artifact_write",
            ],
        ),
        (
            "physical_to_logical",
            &[
                "by_kind",
                "by_size_band",
                "native_edit",
                "durable_checkpoint",
                "edit_plus_checkpoint",
                "count_change_amplification",
                "physical_oracle",
            ],
        ),
        (
            "logical_to_physical",
            &[
                "by_kind",
                "by_size_band",
                "direct_logical_edit",
                "changed_root_refresh",
                "logical_edit_plus_refresh",
                "physical_oracle",
            ],
        ),
        (
            "refresh_routes",
            &[
                "clone_patch",
                "in_place_patch",
                "patch_aggregate",
                "clone_shift",
                "in_place_shift",
                "shift_aggregate",
                "insert_shift",
                "delete_shift",
                "append_shift",
                "truncate_shift",
                "full_fallback_count",
            ],
        ),
        (
            "bursts",
            &[
                "by_root",
                "aggregate",
                "suboperation_count",
                "checkpoint_count",
                "transaction_count",
            ],
        ),
        (
            "history",
            &[
                "sessions",
                "aggregate",
                "selected_roots",
                "verified_open_count",
                "probe_count",
                "first_probe",
                "second_probe",
                "third_probe",
                "first_probe_non_payload_rows",
                "warm_probe_non_payload_rows",
            ],
        ),
        (
            "materialization",
            &[
                "initial",
                "by_root",
                "milestone_aggregate",
                "live_workspace_materializations",
                "witness_materializations",
                "workspace_reuses",
                "rematerializations",
            ],
        ),
        (
            "canonical_locality",
            &[
                "physical_checkpoints",
                "direct_logical_edits",
                "save_bursts",
                "total",
                "cdc_bytes_expected",
                "cdc_bytes_observed",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "maximum_rope_nodes_read",
                "maximum_rope_nodes_emitted",
                "content_directory_nodes_emitted",
                "payload_batch_maximum",
            ],
        ),
        (
            "transactions",
            &[
                "expected",
                "observed",
                "committed",
                "rolled_back",
                "publication_commits",
                "publication_transactions_started",
                "publication_transactions_rolled_back",
                "admission_transactions_started",
                "admission_transactions_committed",
                "admission_transactions_rolled_back",
                "admission_statements",
                "integrity_transactions_started",
                "integrity_transactions_committed",
                "integrity_transactions_rolled_back",
                "integrity_statements",
                "retained_roots_validated",
                "generation_increment_failures",
            ],
        ),
        (
            "authentication",
            &[
                "fetched_authentication_failures",
                "fetched_role_decode_failures",
                "new_object_equation_failures",
                "incumbent_equation_failures",
                "payload_batch_maximum",
                "phase_attribution",
            ],
        ),
        (
            "storage",
            &[
                "initial_database_bytes",
                "terminal_database_bytes",
                "initial_logical_engine_bytes",
                "terminal_logical_engine_bytes",
                "canonical_object_bytes_written",
                "database_growth_bytes",
                "maximum_transition_database_growth_bytes",
                "physical_to_canonical_amplification",
                "scratch_high_water_bytes",
                "rollback_journal_bytes",
                "terminal_sidecars",
                "by_root_range",
            ],
        ),
        (
            "resources",
            &[
                "rss_peak_bytes",
                "largest_buffer_bytes",
                "operation_q_high_water_bytes",
                "operation_q_maximum_terminal_bytes",
                "page_size",
                "cache_pages",
                "cache_spill_pages",
                "store_connection_high_water",
                "store_connections_terminal",
                "fd_baseline",
                "fd_terminal",
                "product_child_process_peak",
                "child_processes_terminal",
                "owned_temp_residue_entries",
                "sidecar_residue_entries",
                "live_rematerializations",
                "network_operations",
            ],
        ),
        (
            "timer_closure",
            &[
                "by_row_group",
                "maximum_row_residual_ns",
                "row_residual_sum_ns",
                "complete_wall_ns",
                "row_wall_sum_ns",
                "outside_rows_wall_ns",
                "timer_residual_ns",
                "hard_limit_ns",
            ],
        ),
        (
            "correctness",
            &[
                "physical_oracles_expected",
                "physical_oracles_passed",
                "canonical_transitions_expected",
                "canonical_transitions_passed",
                "save_bursts_expected",
                "save_bursts_passed",
                "selected_history_roots_expected",
                "selected_history_roots_passed",
                "route_labels_exact",
                "terminal_length_exact",
                "fixture_unchanged",
            ],
        ),
        (
            "optimization",
            &[
                "baseline_run",
                "baseline_rows_sha256",
                "baseline_summary_sha256",
                "complete_wall",
                "counter_snapshot_wall",
                "history_read_wall",
                "verified_open_by_root",
                "append_truncate_refresh",
                "milestone_materialization",
                "shift_routes",
            ],
        ),
        (
            "artifacts",
            &[
                "environment_sha256",
                "master_sha256",
                "readiness_sha256",
                "schedule_sha256",
                "rows_sha256",
                "rows_line_count",
                "campaign_time_sha256",
                "release_executable_sha256",
                "release_executable_blake3",
                "source_tree_blake3",
                "source_manifest_sha256",
            ],
        ),
    ] {
        require_json_keys(json, Some(object), keys)?;
    }
    for (parent, map) in [
        (None, "roots"),
        (Some("physical_to_logical"), "by_kind"),
        (Some("physical_to_logical"), "by_size_band"),
        (Some("physical_to_logical"), "count_change_amplification"),
        (Some("logical_to_physical"), "by_kind"),
        (Some("logical_to_physical"), "by_size_band"),
        (Some("bursts"), "by_root"),
        (Some("materialization"), "by_root"),
        (Some("authentication"), "phase_attribution"),
        (Some("optimization"), "verified_open_by_root"),
        (Some("storage"), "by_root_range"),
        (Some("timer_closure"), "by_row_group"),
    ] {
        let scope = parent.map_or(Ok(json), |key| json_object(json, key))?;
        let object = json_object(scope, map)?;
        if json_object_member_names(object)?.is_empty() {
            return Err(format!("summary JSON map {parent:?}.{map} is empty"));
        }
    }
    let phase_attribution = json_object(json_object(json, "authentication")?, "phase_attribution")?;
    if json_object_member_names(phase_attribution)?
        != [
            "store_open",
            "materialization",
            "checkpoint",
            "logical_edit",
            "apfs_refresh",
            "canonical_witness",
            "verified_open",
            "history_read",
            "storage_observation",
        ]
    {
        return Err("summary JSON exact phase attribution population".to_owned());
    }
    let unavailable = json_array_objects(json, "unavailable")?;
    if unavailable.is_empty()
        || unavailable
            .iter()
            .any(|value| json_string(value, "availability").as_deref() != Ok("Unavailable"))
    {
        return Err("summary JSON unavailable availability contract".to_owned());
    }
    Ok(())
}

fn sum_phase(rows: &[ParsedRow], group: &str, phase: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(phase_wall(&row.json, phase)?)
                .ok_or_else(|| format!("{group}/{phase} phase sum overflow"))
        })
}

fn sum_row_walls(rows: &[ParsedRow], group: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row.row_wall_ns)
                .ok_or_else(|| format!("{group} row wall sum overflow"))
        })
}

fn format_ms(ns: u128) -> String {
    let ms = ns as f64 / 1_000_000.0;
    if ms < 1.0 {
        format!("{ms:.6}")
    } else {
        format!("{ms:.3}")
    }
}

fn format_signed_ms(ns: i128) -> String {
    if ns < 0 {
        format!("-{}", format_ms(ns.unsigned_abs()))
    } else {
        format!("+{}", format_ms(ns as u128))
    }
}

fn throughput_mib_s(bytes: u64, ns: u128) -> f64 {
    if ns == 0 {
        0.0
    } else {
        bytes as f64 / 1_048_576.0 / (ns as f64 / 1_000_000_000.0)
    }
}

#[derive(Clone, Debug)]
struct FailureLedgerEntry {
    artifact: String,
    field: String,
    reason: String,
    disposition_impact: String,
}

fn absolute_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn preserved_failure_ledger(current_run: &Path) -> EvalResult<Vec<FailureLedgerEntry>> {
    let target = stage1_fixture::workspace_root().join("target");
    let current = current_run
        .canonicalize()
        .unwrap_or_else(|_| current_run.to_path_buf());
    let mut failures = Vec::new();
    for entry in fs::read_dir(&target).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "layerfs-stage1-fixtures" {
            let fixture_parent = entry.path();
            if fixture_parent.is_dir() {
                for child in fs::read_dir(fixture_parent).map_err(io_error)? {
                    let child = child.map_err(io_error)?;
                    if child
                        .file_name()
                        .to_string_lossy()
                        .starts_with("apple-edge-v1-preparation-failure-")
                    {
                        failures.push(FailureLedgerEntry {
                            artifact: absolute_path(&child.path()),
                            field: "fixture.preparation".to_owned(),
                            reason: "preparation stopped before immutable fixture publication"
                                .to_owned(),
                            disposition_impact: "preserved and superseded by the sealed fixture"
                                .to_owned(),
                        });
                    }
                }
            }
        } else if entry.path().is_dir() && name.starts_with("layerfs-stage1-apple-edge-") {
            let path = entry.path();
            if path.canonicalize().unwrap_or_else(|_| path.clone()) == current {
                continue;
            }
            if name.ends_with("-attempt-006") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "refresh.hard_link_alias_order;summary.md.final_disposition.complete_wall"
                        .to_owned(),
                    reason: "D7 found a non-first hard-link alias could miss AcceptedSplice and use FullFallback, plus a malformed final-wall Markdown code span"
                        .to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by a repaired source".to_owned(),
                });
                continue;
            }
            if name.ends_with("-attempt-010") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "optimization.verified_open_by_root.R34".to_owned(),
                    reason: "D7 found the R34 retained-union comparison used clean reopen C08-003 instead of the full R34-head scrub C08-001".to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by corrected row-derived attribution"
                            .to_owned(),
                });
                continue;
            }
            if name.ends_with("-attempt-011") {
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: "tests.eof_post_visibility_conflict".to_owned(),
                    reason: "D7 found the APFS post-visibility conflict regression depended on observer scheduling during a finite copy window".to_owned(),
                    disposition_impact:
                        "preserved as D7 evidence and superseded by deterministic cfg(test) Apple fault synchronization"
                            .to_owned(),
                });
                continue;
            }
            let stderr = path.join("stderr.txt");
            if stderr.is_file() {
                let reason = fs::read_to_string(&stderr)
                    .map_err(io_error)?
                    .lines()
                    .next()
                    .unwrap_or("unknown retained failure")
                    .to_owned();
                let field = if reason.contains("locality") {
                    "canonical_locality"
                } else if reason.contains("complete_wall") {
                    "walls_ns.complete_wall"
                } else if reason.contains("milestone") {
                    "materialization.C08"
                } else {
                    "campaign.first_failed_equation"
                };
                failures.push(FailureLedgerEntry {
                    artifact: absolute_path(&path),
                    field: field.to_owned(),
                    reason,
                    disposition_impact: "preserved and superseded by a repaired source".to_owned(),
                });
            } else {
                let markdown = fs::read_to_string(path.join("summary.md")).unwrap_or_default();
                if markdown.contains("| `FAIL` |") {
                    failures.push(FailureLedgerEntry {
                        artifact: absolute_path(&path),
                        field: "summary.md.materialization.cleanup".to_owned(),
                        reason:
                            "D7 found C08 cleanup cells contradicting retained destination custody"
                                .to_owned(),
                        disposition_impact:
                            "preserved as D7 evidence and superseded by a repaired source"
                                .to_owned(),
                    });
                }
            }
        }
    }
    #[cfg(test)]
    for (attempt, field, reason) in [
        (
            "attempt-010",
            "optimization.verified_open_by_root.R34",
            "synthetic preserved failure for the self-contained summary contract",
        ),
        (
            "attempt-011",
            "tests.eof_post_visibility_conflict",
            "synthetic preserved failure for the self-contained summary contract",
        ),
    ] {
        if !failures
            .iter()
            .any(|failure| failure.artifact.ends_with(attempt))
        {
            failures.push(FailureLedgerEntry {
                artifact: absolute_path(
                    &target.join(format!("layerfs-stage1-apple-edge-synthetic-{attempt}")),
                ),
                field: field.to_owned(),
                reason: reason.to_owned(),
                disposition_impact: "synthetic unit-test receipt only".to_owned(),
            });
        }
    }
    failures.sort_by(|left, right| left.artifact.cmp(&right.artifact));
    Ok(failures)
}

fn failure_ledger_json(failures: &[FailureLedgerEntry]) -> String {
    failures
        .iter()
        .enumerate()
        .map(|(index, failure)| {
            format!(
                concat!(
                    "{{\"sequence\":{},\"artifact\":\"{}\",",
                    "\"field\":\"{}\",\"availability\":\"failure\",",
                    "\"reason\":\"{}\",\"disposition_impact\":\"{}\"}}"
                ),
                index + 1,
                json_escape(&failure.artifact),
                json_escape(&failure.field),
                json_escape(&failure.reason),
                json_escape(&failure.disposition_impact),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn summary_markdown(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    source: &SourceIdentity,
    master: &FixtureMaster,
    complete_wall_ns: u128,
) -> EvalResult<String> {
    let disposition = derive_disposition(rows);
    validate_ref_chain(rows, campaign.schedule)?;
    let authentication = validate_authentication(rows)?;
    validate_locality_rows(rows)?;
    validate_phase_counter_rows(rows)?;
    validate_refresh_rows(rows)?;
    validate_availability_rows(rows)?;
    let selected_history_roots_passed = validate_history_rows(rows)?;
    let phase_attribution = phase_attributions(rows)?;
    let optimization = optimization_comparison(rows, complete_wall_ns)?;
    let roots = roots_from_rows(rows)?;
    let c09 = rows
        .iter()
        .find(|row| row.row_group == "C09")
        .ok_or_else(|| "missing C09 terminal row".to_owned())?;
    let rss_peak = maximum_key(rows, "rss_peak_bytes")?;
    let q_high_water = maximum_key(rows, "operation_q_high_water_bytes")?;
    let q_terminal = maximum_key(rows, "operation_q_terminal_bytes")?;
    let connection_high_water = maximum_key(rows, "active_store_connections")?;
    let connection_terminal = row_u128(c09, "active_store_connections")?;
    let fd_baseline = json_u128(&c09.json, "pre_cleanup_fd_count")?;
    let fd_terminal = row_u128(c09, "fd_current")?;
    let child_peak = maximum_key(rows, "child_processes")?;
    let child_terminal = row_u128(c09, "child_processes")?;
    let owned_temp_terminal = row_u128(c09, "owned_temp_entries")?;
    let residue_terminal = row_u128(c09, "residue_entries")?;
    let rematerializations = sum_key(rows, None, "rematerializations")?;
    let network_operations = maximum_key(rows, "network_operations")?;
    let physical_oracles = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .filter(|row| json_bool(&row.json, "physical_bytes_exact") == Ok(true))
        .count()
        + rows
            .iter()
            .filter(|row| row.row_group == "C07")
            .map(|row| json_all_u128(&row.json, "physical_oracle_wall_ns"))
            .collect::<EvalResult<Vec<_>>>()?
            .iter()
            .map(Vec::len)
            .sum::<usize>();
    let canonical_transitions = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .filter(|row| json_bool(&row.json, "canonical_bytes_exact") == Ok(true))
        .count();
    let patch_refreshes = rows
        .iter()
        .filter(|row| {
            row.row_group == "C05"
                && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
        })
        .count();
    let fallback_refreshes = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "FullFallback")
        .count();
    let shift_refreshes = rows
        .iter()
        .filter(|row| {
            row.row_group == "C05"
                && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
        })
        .count();
    let mut output = String::new();
    writeln!(
        output,
        "# LayerFS Stage 1.1 — Single-file APFS Edge Result\n\nDisposition: `PASS`\n"
    )
    .map_err(display_error)?;
    writeln!(output, "## 1. Disposition and custody\n").map_err(display_error)?;
    writeln!(output, "| Field | Value |\n|---|---:|").map_err(display_error)?;
    for (field, value) in [
        ("Run directory", campaign.run.display().to_string()),
        ("Git commit", source.git_commit.clone()),
        ("Dirty tree", source.dirty_tree.to_string()),
        ("Source BLAKE3", source.tree_blake3.clone()),
        ("Source manifest SHA-256", source.manifest_sha256.clone()),
        ("Executable SHA-256", source.executable_sha256.clone()),
        ("Executable BLAKE3", source.executable_blake3.clone()),
        ("Fixture BLAKE3", master.fixture_blake3.clone()),
        ("APFS identity", master.apfs_identity.clone()),
        ("StoreId", master.store_id.clone()),
        ("Store profile", master.profile.clone()),
        ("Measured workflows", "1 / 1".to_owned()),
        ("Valid rows", "47 / 47".to_owned()),
        ("Edit/sub-edit operations", "51 / 51".to_owned()),
        ("Durable transitions", "34 / 34".to_owned()),
        ("Initial root", format!("R0={}", roots[0])),
        ("Terminal root", format!("R34={}", roots[34])),
        ("Initial bytes", INITIAL_BYTES.to_string()),
        ("Maximum bytes", MAXIMUM_BYTES.to_string()),
        ("Terminal bytes", INITIAL_BYTES.to_string()),
        (
            "Complete workflow wall",
            format!("{} ms", format_ms(complete_wall_ns)),
        ),
    ] {
        writeln!(output, "| {field} | `{}` |", value.replace('|', "\\|")).map_err(display_error)?;
    }
    writeln!(
        output,
        "\n| Artifact | SHA-256 | Additional identity |\n|---|---|---|"
    )
    .map_err(display_error)?;
    for (name, path, identity) in [
        (
            "environment.json",
            campaign.run.join("environment.json"),
            "—".to_owned(),
        ),
        (
            "master.json",
            campaign.run.join("master.json"),
            format!("fixture BLAKE3 `{}`", master.fixture_blake3),
        ),
        (
            "readiness.json",
            campaign.run.join("readiness.json"),
            "admitted receipt `exact-match`".to_owned(),
        ),
        (
            "schedule.json",
            campaign.run.join("schedule.json"),
            "`47 rows / 51 edit-suboperations / 34 transitions`".to_owned(),
        ),
        (
            "rows.jsonl",
            campaign.run.join("rows.jsonl"),
            "`47 lines / 47 valid`".to_owned(),
        ),
        (
            "campaign-time.txt",
            campaign.run.join("campaign-time.txt"),
            "timer equation `PASS`".to_owned(),
        ),
    ] {
        writeln!(
            output,
            "| `{name}` | `{}` | {identity} |",
            sha256_file(&path)?
        )
        .map_err(display_error)?;
    }
    writeln!(
        output,
        "| release executable | `{}` | BLAKE3 `{}` |\n| Rust/Cargo source tree | manifest SHA-256 `{}` | BLAKE3 `{}` |\n",
        source.executable_sha256,
        source.executable_blake3,
        source.manifest_sha256,
        source.tree_blake3,
    )
    .map_err(display_error)?;

    writeln!(output, "## 2. Overall gate scoreboard\n").map_err(display_error)?;
    writeln!(output, "| Gate | Required | Observed | Status |\n|---|---:|---:|---|\n| Rows | `47` | `{}` | `PASS` |\n| Edit/sub-edit operations | `51` | `51` | `PASS` |\n| Durable transitions | `34` | `{}` | `PASS` |\n| Complete workflow | `<60,000 ms` | `{} ms` | `PASS` |\n| Physical oracles | `51 exact` | `{}` exact | `PASS` |\n| Canonical transition oracles | `34 exact` | `{}` exact | `PASS` |\n| Save bursts | `4 exact` | `{}` exact | `PASS` |\n| Selected historical roots | `8 exact` | `{}` exact | `PASS` |\n| Route labels | exact | `{}` patch / `{}` shift / `{}` FullFallback | `PASS` |\n| Live rematerializations | `0` | `{}` | `PASS` |\n| RSS peak | `<=33,554,432 B` | `{}` | `PASS` |\n| Q structural-reservation high-water | `<=8,388,608 B` | `{}` | `PASS` |\n| Q reservation terminal after every operation | `0` | `{}` | `PASS` |\n| FD baseline/terminal | equal | `{}` / `{}` | `PASS` |\n| Store connections terminal | `0` | `{}` | `PASS` |\n| Owned residue | `0` | `{}` | `PASS` |\n| Network | `0` | `{}` | `PASS` |\n", rows.len(), canonical_transitions, format_ms(complete_wall_ns), physical_oracles, canonical_transitions, rows.iter().filter(|row| row.row_group == "C07" && row.status == "PASS").count(), selected_history_roots_passed, patch_refreshes, shift_refreshes, fallback_refreshes, rematerializations, rss_peak, q_high_water, q_terminal, fd_baseline, fd_terminal, connection_terminal, owned_temp_terminal.max(residue_terminal), network_operations).map_err(display_error)?;

    writeln!(output, "## 3. Physical APFS edit to LayerFS checkpoint\n\n| Operation | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms | Oracle | Status |\n|---|---:|---:|---:|---:|---:|---:|---:|---|---|").map_err(display_error)?;
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.operation == kind)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.operation == kind
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.operation == kind
            })?;
        writeln!(
            output,
            "| {} | 3 | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `3/3` | `PASS` |",
            title(kind),
            format_ms(native.p50_ns),
            format_ms(native.p95_ns),
            format_ms(checkpoint.p50_ns),
            format_ms(checkpoint.p95_ns),
            format_ms(combined.p50_ns),
            format_ms(combined.p95_ns)
        )
        .map_err(display_error)?;
    }
    let native_all = row_phase_stats(rows, "C03", "native_edit")?;
    let checkpoint_all = row_phase_stats(rows, "C03", "durable_checkpoint")?;
    let combined_all =
        combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |_| true)?;
    writeln!(
        output,
        "| **All** | **15** | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `15/15` | `PASS` |\n",
        format_ms(native_all.p50_ns),
        format_ms(native_all.p95_ns),
        format_ms(checkpoint_all.p50_ns),
        format_ms(checkpoint_all.p95_ns),
        format_ms(combined_all.p50_ns),
        format_ms(combined_all.p95_ns)
    )
    .map_err(display_error)?;
    writeln!(output, "| Size band | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms |\n|---|---:|---:|---:|---:|---:|---:|---:|").map_err(display_error)?;
    for (label, band) in [
        ("Near 8 KiB", "near-8-kib"),
        ("Near 16 KiB", "near-16-kib"),
        ("Near 32 KiB", "near-32-kib"),
    ] {
        let native = filtered_phase_stats(rows, "C03", "native_edit", |row| row.size_band == band)?;
        let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
            row.size_band == band
        })?;
        let combined =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                row.size_band == band
            })?;
        writeln!(
            output,
            "| {label} | 5 | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |",
            format_ms(native.p50_ns),
            format_ms(native.p95_ns),
            format_ms(checkpoint.p50_ns),
            format_ms(checkpoint.p95_ns),
            format_ms(combined.p50_ns),
            format_ms(combined.p95_ns)
        )
        .map_err(display_error)?;
    }

    writeln!(output, "\n## 4. Physical count-changing amplification\n\n| Seq | Operation | Offset | Suffix B | Replacement B | Native read B | Native write B | Equation | Route | Status |\n|---:|---|---:|---:|---:|---:|---:|---|---|---|").map_err(display_error)?;
    for row in rows
        .iter()
        .filter(|row| row.row_group == "C03" && row.operation != "overwrite")
    {
        let offset = json_u128(&row.json, "offset")?;
        let delete = json_u128(&row.json, "delete_bytes")?;
        let insert = json_u128(&row.json, "insert_bytes")?;
        let suffix = u128::from(row.before_bytes).saturating_sub(offset + delete);
        writeln!(output, "| `{}` | `{}` | `{offset}` | `{suffix}` | `{insert}` | `{}` | `{}` | `read=S; write=S+B` | `{}` | `PASS` |", json_u128(&row.json, "sequence")?, row.operation, row_u128(row, "bytes_read")?, row_u128(row, "bytes_written")?, row.native_route).map_err(display_error)?;
    }
    writeln!(output, "\n| Kind | n | Suffix shifted B | Native read B | Native write B | Amplification |\n|---|---:|---:|---:|---:|---:|").map_err(display_error)?;
    for kind in ["insert", "delete", "append", "truncate"] {
        let selected = rows
            .iter()
            .filter(|row| row.row_group == "C03" && row.operation == kind)
            .collect::<Vec<_>>();
        let shifted = selected
            .iter()
            .map(|row| row_u128(row, "suffix_bytes_shifted"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .sum::<u128>();
        let read = selected
            .iter()
            .map(|row| row_u128(row, "bytes_read"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .sum::<u128>();
        let written = selected
            .iter()
            .map(|row| row_u128(row, "bytes_written"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .sum::<u128>();
        let logical = selected
            .iter()
            .map(|row| u128::from(row.before_bytes.abs_diff(row.after_bytes)))
            .sum::<u128>();
        let ratio = if logical == 0 {
            0.0
        } else {
            (read + written) as f64 / logical as f64
        };
        writeln!(
            output,
            "| {} | 3 | `{shifted}` | `{read}` | `{written}` | `{ratio:.3}` |",
            title(kind)
        )
        .map_err(display_error)?;
    }

    writeln!(output, "\n## 5. Logical LayerFS edit to physical APFS refresh\n\n| Operation | n | Logical p50 ms | Logical p95 ms | Route class | Refresh p50 ms | Refresh p95 ms | End-to-end p50 ms | End-to-end p95 ms | Oracle |\n|---|---:|---:|---:|---|---:|---:|---:|---:|---|").map_err(display_error)?;
    for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
        let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
            row.operation == kind
        })?;
        let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
            row.operation == kind
        })?;
        let combined = combined_phase_stats(
            rows,
            "C05",
            "direct_logical_edit",
            "changed_root_refresh",
            |row| row.operation == kind,
        )?;
        writeln!(
            output,
            "| {} | 3 | `{}` | `{}` | {} | `{}` | `{}` | `{}` | `{}` | `3/3` |",
            title(kind),
            format_ms(logical.p50_ns),
            format_ms(logical.p95_ns),
            if kind == "overwrite" {
                "Patch"
            } else {
                "Shift"
            },
            format_ms(refresh.p50_ns),
            format_ms(refresh.p95_ns),
            format_ms(combined.p50_ns),
            format_ms(combined.p95_ns)
        )
        .map_err(display_error)?;
    }

    writeln!(output, "\n## 6. Refresh-route summary\n\n| Route | Required count | Observed | p50 ms | p95 ms | Physical B | Rematerializations | Status |\n|---|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
    let clone = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "ClonePatch")
        .count();
    let in_place = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlacePatch")
        .count();
    let clone_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
        .count();
    let in_place_shift = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
        .count();
    for (label, required, observed, stats, bytes) in [
        (
            "ClonePatch",
            "0..3",
            clone,
            optional_route_stats(rows, "ClonePatch")?,
            sum_route_bytes(rows, "ClonePatch", None)?,
        ),
        (
            "InPlacePatch",
            "0..3",
            in_place,
            optional_route_stats(rows, "InPlacePatch")?,
            sum_route_bytes(rows, "InPlacePatch", None)?,
        ),
        (
            "Patch aggregate",
            "3",
            3,
            Some(route_stats(rows, "Patch", None)?),
            sum_patch_bytes(rows)?,
        ),
        (
            "CloneShift",
            "0..12",
            clone_shift,
            optional_route_stats(rows, "CloneShift")?,
            sum_route_bytes(rows, "CloneShift", None)?,
        ),
        (
            "InPlaceShift",
            "0..12",
            in_place_shift,
            optional_route_stats(rows, "InPlaceShift")?,
            sum_route_bytes(rows, "InPlaceShift", None)?,
        ),
        (
            "Shift aggregate",
            "12",
            shift_refreshes,
            Some(route_stats(rows, "Shift", None)?),
            sum_route_bytes(rows, "Shift", None)?,
        ),
        (
            "Insert Shift",
            "3",
            3,
            Some(route_stats(rows, "Shift", Some("insert"))?),
            sum_route_bytes(rows, "Shift", Some("insert"))?,
        ),
        (
            "Delete Shift",
            "3",
            3,
            Some(route_stats(rows, "Shift", Some("delete"))?),
            sum_route_bytes(rows, "Shift", Some("delete"))?,
        ),
        (
            "Append Shift",
            "3",
            3,
            Some(route_stats(rows, "Shift", Some("append"))?),
            sum_route_bytes(rows, "Shift", Some("append"))?,
        ),
        (
            "Truncate Shift",
            "3",
            3,
            Some(route_stats(rows, "Shift", Some("truncate"))?),
            sum_route_bytes(rows, "Shift", Some("truncate"))?,
        ),
        (
            "FullFallback",
            "0",
            fallback_refreshes,
            optional_route_stats(rows, "FullFallback")?,
            sum_route_bytes(rows, "FullFallback", None)?,
        ),
    ] {
        writeln!(
            output,
            "| {label} | `{required}` | `{observed}` | `{}` | `{}` | `{bytes}` | `0` | `PASS` |",
            stats
                .as_ref()
                .map_or_else(|| "N/A".to_owned(), |value| format_ms(value.p50_ns)),
            stats
                .as_ref()
                .map_or_else(|| "N/A".to_owned(), |value| format_ms(value.p95_ns))
        )
        .map_err(display_error)?;
    }

    writeln!(output, "\n## 7. Canonical locality\n\n| Population | Transitions | CDC expected B | CDC observed B | Unaffected reads B | Unaffected writes B | Max nodes read | Max nodes emitted | Status |\n|---|---:|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
    for (label, group, transitions, expected) in [
        ("Physical checkpoints", "C03", 15, 172_032_u128),
        ("Direct logical edits", "C05", 15, 172_032),
        ("Save bursts", "C07", 4, 151_552),
    ] {
        writeln!(output, "| {label} | {transitions} | `{expected}` | `{}` | `{}` | `{}` | `{}` | `{}` | `PASS` |", sum_key(rows, Some(group), "cdc_bytes_scanned")?, sum_key(rows, Some(group), "unaffected_payload_reads")?, sum_key(rows, Some(group), "unaffected_payload_writes")?, maximum_group_key(rows, group, "rope_nodes_read")?, maximum_group_key(rows, group, "rope_nodes_emitted")?).map_err(display_error)?;
    }
    writeln!(
        output,
        "| **Total** | **34** | `495616` | `{}` | **0** | **0** | `{}` | `{}` | `PASS` |",
        sum_locality_key(rows, "cdc_bytes_scanned")?,
        maximum_locality_key(rows, "rope_nodes_read")?,
        maximum_locality_key(rows, "rope_nodes_emitted")?
    )
    .map_err(display_error)?;

    writeln!(output, "\n## 8. Multi-edit save bursts\n\n| Root | Pattern | Sub-edits | Native ms | Oracle ms | Checkpoint ms | Row ms | Transactions | COMMITs | Final B | Status |\n|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
    for (index, row) in rows.iter().filter(|row| row.row_group == "C07").enumerate() {
        let root = index + 31;
        let sub_count = json_all_u128(&row.json, "native_wall_ns")?.len();
        let native = json_all_u128(&row.json, "native_wall_ns")?
            .into_iter()
            .sum::<u128>();
        let oracle = json_all_u128(&row.json, "physical_oracle_wall_ns")?
            .into_iter()
            .sum::<u128>();
        writeln!(
            output,
            "| R{root} | {} | {sub_count} | `{}` | `{}` | `{}` | `{}` | 1 | 1 | {} | `PASS` |",
            campaign.schedule.bursts[index].pattern,
            format_ms(native),
            format_ms(oracle),
            format_ms(phase_wall(&row.json, "durable_checkpoint")?),
            format_ms(row.row_wall_ns),
            row.after_bytes
        )
        .map_err(display_error)?;
    }
    writeln!(
        output,
        "| **Total** | — | **21** | `{}` | `{}` | `{}` | `{}` | **4** | **4** | — | `PASS` |",
        format_ms(sum_subfield(rows, "C07", "native_wall_ns")?),
        format_ms(sum_subfield(rows, "C07", "physical_oracle_wall_ns")?),
        format_ms(sum_phase(rows, "C07", "durable_checkpoint")?),
        format_ms(sum_row_walls(rows, "C07")?)
    )
    .map_err(display_error)?;

    writeln!(output, "\n## 9. Fresh Verified history sessions\n\n| Session | Head | Roots checked | Open/scrub ms | Objects authenticated | Bytes authenticated | Probe B | Writer tx | Native writes | Status |\n|---:|---|---|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
    for (index, row) in rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .enumerate()
    {
        let session = index + 1;
        let head = session * 5;
        writeln!(
            output,
            "| {session} | R{head} | {} | `{}` | `{}` | `{}` | `{}` | 0 | 0 | `PASS` |",
            history_root_indices(session as u8)?
                .iter()
                .map(|root| format!("R{root}"))
                .collect::<Vec<_>>()
                .join(","),
            format_ms(phase_wall(&row.json, "verified_open")?),
            row_u128(row, "fetched_rows")?,
            row_u128(row, "object_bytes_read")?,
            history_root_indices(session as u8)?.len() * 3 * 65_536
        )
        .map_err(display_error)?;
    }
    writeln!(output, "\n| Probe ordinal | n | p50 ms | p95 ms | Non-payload rows | Payload rows | Cache classification |\n|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
    for (ordinal, classification) in [
        (1_u8, "first root/path resolution"),
        (2_u8, "exact root/path plan hit"),
        (3_u8, "exact root/path plan hit"),
    ] {
        let stats = history_probe_stats(rows, ordinal)?;
        writeln!(
            output,
            "| {ordinal} | {} | `{}` | `{}` | `{}` | `{}` | {classification} |",
            stats.raw_ns.len(),
            format_ms(stats.p50_ns),
            format_ms(stats.p95_ns),
            history_probe_sum(rows, ordinal, "non_payload_rows")?,
            history_probe_sum(rows, ordinal, "payload_batch_references")?,
        )
        .map_err(display_error)?;
    }

    writeln!(output, "\n## 10. Materialization and reconstruction\n\n| Root | Purpose | Logical B | Wall ms | MiB/s | Native write B | Exact bytes | Metadata | Cleanup |\n|---:|---|---:|---:|---:|---:|---|---|---|").map_err(display_error)?;
    let c02 = rows
        .iter()
        .find(|row| row.row_group == "C02")
        .ok_or_else(|| "missing C02 materialization row".to_owned())?;
    let cold = phase_wall(&c02.json, "cold_materialization")?;
    writeln!(output, "| R0 | Initial cold managed | {INITIAL_BYTES} | `{}` | `{:.3}` | `{}` | `PASS` | `PASS` | retained live |", format_ms(cold), throughput_mib_s(INITIAL_BYTES, cold), json_u128(&c02.json, "bytes_written")?).map_err(display_error)?;
    for (index, row) in rows.iter().filter(|row| row.row_group == "C08").enumerate() {
        let root = [15, 30, 34][index];
        let purpose = [
            "Physical-chain milestone",
            "Logical-refresh milestone",
            "Burst-chain milestone",
        ][index];
        let wall = phase_wall(&row.json, "milestone_materialization")?;
        let oracle = json_object(&row.json, "oracle")?;
        let custody = json_object(&row.json, "custody")?;
        let exact = json_bool(oracle, "physical_bytes_exact")?
            && json_bool(oracle, "canonical_bytes_exact")?;
        let metadata = json_bool(oracle, "metadata_exact")?;
        let cleanup = json_u128(custody, "cleanup_residue_entries")? == 0;
        writeln!(
            output,
            "| R{root} | {purpose} | {} | `{}` | `{:.3}` | `{}` | `{}` | `{}` | `{}` |",
            row.after_bytes,
            format_ms(wall),
            throughput_mib_s(row.after_bytes, wall),
            row_u128(row, "bytes_written")?,
            if exact { "PASS" } else { "FAIL" },
            if metadata { "PASS" } else { "FAIL" },
            if cleanup { "PASS" } else { "FAIL" },
        )
        .map_err(display_error)?;
    }
    let r34 = rows
        .iter()
        .find(|row| row.row_id == "C08-003")
        .ok_or_else(|| "missing C08-003 metadata receipt".to_owned())?;
    let r34_metadata = json_object(json_object(&r34.json, "custody")?, "fresh_metadata")?;
    writeln!(
        output,
        "\nR34 exact metadata receipt: mode=`{:#o}`; mtime=`{}.{:09}`; xattrs=`{}`; ACL present=`{}`; BSD flags=`{}`. This is the observed R34 value, not the initial fixture mtime.",
        json_u128(r34_metadata, "mode")?,
        json_i64(r34_metadata, "mtime_seconds")?,
        json_u128(r34_metadata, "mtime_nanoseconds")?,
        json_u128(r34_metadata, "xattr_count")?,
        json_bool(r34_metadata, "acl_present")?,
        json_u128(r34_metadata, "bsd_flags")?,
    )
    .map_err(display_error)?;

    writeln!(output, "\n## 11. Transaction and authentication closure\n\n| Equation | Required | Observed/failures | Status |\n|---|---:|---:|---|\n| Generation increment | `34/34` | `{}/0` | `PASS` |\n| Writer transactions | `34` | `{}` | `PASS` |\n| Committed transactions | `34` | `{}` | `PASS` |\n| Rolled-back transactions | `0` | `{}` | `PASS` |\n| Publication COMMITs | `34` | `{}` | `PASS` |\n| Verified fetched = authentication; Trusted read-only authentication = 0; Trusted transition authentication <= fetched | every applicable row | `{}` failures | `PASS` |\n| fetched = role decode | every applicable row | `{}` failures | `PASS` |\n| new auth = created + reused | every publication | `{}` failures | `PASS` |\n| incumbent auth = reused | every publication | `{}` failures | `PASS` |\n| Payload batch maximum | `<=64` | `{}` | `PASS` |", rows.iter().filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07")).count(), sum_key(rows, None, "transactions_started")?, sum_key(rows, None, "transactions_committed")?, sum_key(rows, None, "transactions_rolled_back")?, sum_key(rows, None, "publication_commits")?, authentication.fetched_authentication_failures, authentication.fetched_role_decode_failures, authentication.new_object_equation_failures, authentication.incumbent_equation_failures, authentication.payload_batch_maximum).map_err(display_error)?;
    writeln!(output, "\n| SQL boundary | Started | Committed | Rolled back | Statements/roots | Status |\n|---|---:|---:|---:|---:|---|\n| Publication visibility | `{}` | `{}` | `{}` | `34 COMMITs` | `PASS` |\n| Open admission | `{}` | `{}` | `{}` | `{}` statements | `PASS` |\n| Live Verified integrity | `{}` | `{}` | `{}` | `{}` statements | `PASS` |\n| Disk-backed retained-root validation | N/A | N/A | N/A | `{}` roots | `PASS` |", sum_key(rows, None, "publication_transactions_started")?, sum_key(rows, None, "publication_commits")?, sum_key(rows, None, "publication_transactions_rolled_back")?, sum_key(rows, None, "admission_transactions_started")?, sum_key(rows, None, "admission_transactions_committed")?, sum_key(rows, None, "admission_transactions_rolled_back")?, sum_key(rows, None, "admission_statements")?, sum_key(rows, None, "integrity_transactions_started")?, sum_key(rows, None, "integrity_transactions_committed")?, sum_key(rows, None, "integrity_transactions_rolled_back")?, sum_key(rows, None, "integrity_statements")?, sum_key(rows, None, "retained_roots_validated")?).map_err(display_error)?;
    writeln!(output, "\n| Counter phase | Rows | Statements | Fetched/auth/role | Object read B | Object write B | Tx/COMMIT | Scrubs | Engine/VFS scratch tables | Q structural-reservation high B | Connections |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|").map_err(display_error)?;
    for phase in &phase_attribution {
        writeln!(
            output,
            "| `{}` | {} | `{}` | `{}/{}/{}` | `{}` | `{}` | `{}/{}` | `{}` | `{}/{}` | `{}` | `{}` |",
            phase.name,
            phase.rows,
            phase.statements,
            phase.fetched_rows,
            phase.authentication_passes,
            phase.role_decode_passes,
            phase.object_bytes_read,
            phase.object_bytes_written,
            phase.transactions,
            phase.publication_commits,
            phase.retained_union_scrubs,
            phase.scratch_tables,
            phase.operation_scratch_tables,
            phase.q_high_water_bytes,
            phase.active_connections,
        )
        .map_err(display_error)?;
    }

    let initial_logical_engine = first_group_value(rows, "C02", "logical_engine_bytes")?;
    let terminal_logical_engine = last_group_value(rows, "C07", "logical_engine_bytes")?;
    writeln!(output, "\n## 12. Storage growth and amplification\n\n| Metric | Initial | Terminal/peak | Delta | Status |\n|---|---:|---:|---:|---|\n| SQLite database B | `{}` | `{}` | `{}` | report |\n| Logical Engine B | `{}` | `{}` | `{}` | report |\n| Canonical object B written | 0 | `{}` | `{}` | report |\n| Physical DB/canonical amplification | N/A | `{:.3}` | N/A | report |\n| Maximum transition DB growth B | N/A | `{}` | N/A | report |\n| Scratch high-water B | 0 | `{}` | N/A | `PASS` |\n| Rollback journal peak B | N/A | `Unavailable` | N/A | `PASS` |\n| Terminal journal/WAL/SHM | absent | `absent` | N/A | `PASS` |", storage_initial(rows)?, storage_terminal(rows)?, storage_terminal(rows)?.saturating_sub(storage_initial(rows)?), initial_logical_engine, terminal_logical_engine, terminal_logical_engine - initial_logical_engine, sum_key(rows, None, "canonical_object_bytes_written")?, sum_key(rows, None, "canonical_object_bytes_written")?, storage_amplification(rows)?, maximum_key(rows, "database_growth_bytes")?, maximum_key(rows, "scratch_high_water_bytes")?).map_err(display_error)?;
    writeln!(output, "\n| Root range | Transitions | Canonical B written | DB growth B | Amplification |\n|---|---:|---:|---:|---:|\n| R0→R15 | 15 | `{}` | `{}` | `{:.3}` |\n| R15→R30 | 15 | `{}` | `{}` | `{:.3}` |\n| R30→R34 | 4 | `{}` | `{}` | `{:.3}` |", range_sum(rows, "C03", "canonical_object_bytes_written")?, range_sum(rows, "C03", "database_growth_bytes")?, range_amplification(rows, "C03")?, range_sum(rows, "C05", "canonical_object_bytes_written")?, range_sum(rows, "C05", "database_growth_bytes")?, range_amplification(rows, "C05")?, range_sum(rows, "C07", "canonical_object_bytes_written")?, range_sum(rows, "C07", "database_growth_bytes")?, range_amplification(rows, "C07")?).map_err(display_error)?;

    writeln!(output, "\n## 13. Resource closure\n\n| Resource | Hard gate | Observed | Status |\n|---|---:|---:|---|\n| RSS peak B | `<=33,554,432` | `{}` | `PASS` |\n| Largest product-buffer structural bound B | `<=1,048,576` | `{PRODUCT_BUFFER_BOUND_BYTES}` | `PASS` |\n| Q structural-reservation high-water B | `<=8,388,608` | `{}` | `PASS` |\n| Q reservation terminal after every operation B | `0` | `{}` | `PASS` |\n| Store cache pages | `1,280` | `1,280` | `PASS` |\n| Store spill pages | `1,280` | `1,280` | `PASS` |\n| Store connection high-water | `<=2` | `{}` | `PASS` |\n| Store connections terminal | `0` | `{}` | `PASS` |\n| FD baseline/terminal | equal | `{}/{}` | `PASS` |\n| Product child-process peak | `0` | `{}` | `PASS` |\n| Terminal child processes | `0` | `{}` | `PASS` |\n| Owned temp residue | `0` | `{}` | `PASS` |\n| Journal/WAL/SHM residue | `0` | `{}` | `PASS` |\n| Live rematerializations | `0` | `{}` | `PASS` |", rss_peak, q_high_water, q_terminal, connection_high_water, connection_terminal, fd_baseline, fd_terminal, child_peak, child_terminal, owned_temp_terminal, residue_terminal, rematerializations).map_err(display_error)?;

    writeln!(output, "\n## 14. Timer closure\n\n| Row group | Rows | Maximum residual ns | Sum residual ns | Status |\n|---|---:|---:|---:|---|").map_err(display_error)?;
    for (label, group) in [
        ("C03 physical/checkpoint", "C03"),
        ("C04 native-history", "C04"),
        ("C05 logical/refresh", "C05"),
        ("C06 logical-history", "C06"),
        ("C07 bursts", "C07"),
        ("C08 materialization", "C08"),
    ] {
        let selected = rows
            .iter()
            .filter(|row| row.row_group == group)
            .collect::<Vec<_>>();
        writeln!(
            output,
            "| {label} | {} | `{}` | `{}` | `PASS` |",
            selected.len(),
            selected
                .iter()
                .map(|row| row.row_residual_ns)
                .max()
                .unwrap_or(0),
            selected.iter().map(|row| row.row_residual_ns).sum::<u128>()
        )
        .map_err(display_error)?;
    }
    writeln!(output, "| Complete workflow | 1 | `0` | `0` | `PASS` |\n\nComplete wall: `{complete_wall_ns} ns / {} ms`\nPreferred planning range: `<40–45 s`\nHard gate: `<60 s`\n\nTerminal receipt rewrites outside the accounted wall: `campaign-time.txt`, `summary.json`, and `summary.md`; their final digests are recorded only in the terminal handoff after close.", format_ms(complete_wall_ns)).map_err(display_error)?;

    let failures = preserved_failure_ledger(campaign.run)?;
    writeln!(output, "\n## 15. Preserved failures and unavailable observations\n\n| Sequence | Artifact/row | Field | Availability/failure | Reason | Disposition impact |\n|---:|---|---|---|---|---|\n| 1 | all applicable rows | `native.sync_regular_calls` | `Unavailable` | product exposes only aggregate sync calls | none; no hard split-sync gate |\n| 2 | all applicable rows | `native.sync_directory_calls` | `Unavailable` | product exposes only aggregate sync calls | none; no hard split-sync gate |\n| 3 | all applicable rows | `storage.rollback_journal_bytes` | `Unavailable` | not continuously observed | terminal sidecar absence passed |\n| 4 | all applicable rows | `storage.temporary_file_bytes` | `Unavailable` | not continuously observed | terminal residue absence passed |").map_err(display_error)?;
    for (index, failure) in failures.iter().enumerate() {
        writeln!(
            output,
            "| {} | `{}` | `{}` | `failure` | {} | {} |",
            index + 5,
            failure.artifact.replace('|', "\\|"),
            failure.field.replace('|', "\\|"),
            failure.reason.replace('|', "\\|"),
            failure.disposition_impact.replace('|', "\\|"),
        )
        .map_err(display_error)?;
    }
    writeln!(
        output,
        "\nPreserved failed attempts: `{}`\nSuperseded attempts: `{}`\nDeleted or overwritten attempts: `0`",
        failures.len(),
        failures.len(),
    )
    .map_err(display_error)?;

    writeln!(output, "\n## 16. Final disposition\n\nPost-PASS optimization baseline: `{}` (rows SHA-256 `{}`).\n\n| Optimization metric | Attempt-007 before ms | Current after ms | Absolute gain ms | Owner |\n|---|---:|---:|---:|---|\n| Complete campaign wall | `{}` | `{}` | `{}` | product + evaluator |\n| Transition counter/resource snapshots | `{}` | `{}` | `{}` | evaluator |\n| History read/oracle wall | `{}` | `{}` | `{}` | evaluator |\n| Append/truncate refresh p50 | `{}` | `{}` | `{}` | product EOF splice |\n| Milestone materialization p50 | `{}` | `{}` | `{}` | product read/materialize |",
        optimization.baseline_path,
        OPTIMIZATION_BASELINE_ROWS_SHA256,
        format_ms(optimization.baseline_complete_wall_ns),
        format_ms(optimization.current_complete_wall_ns),
        format_signed_ms(signed_gain(optimization.baseline_complete_wall_ns, optimization.current_complete_wall_ns)?),
        format_ms(optimization.baseline_counter_snapshot_ns),
        format_ms(optimization.current_counter_snapshot_ns),
        format_signed_ms(signed_gain(optimization.baseline_counter_snapshot_ns, optimization.current_counter_snapshot_ns)?),
        format_ms(optimization.baseline_history_read_ns),
        format_ms(optimization.current_history_read_ns),
        format_signed_ms(signed_gain(optimization.baseline_history_read_ns, optimization.current_history_read_ns)?),
        format_ms(optimization.baseline_append_truncate.p50_ns),
        format_ms(optimization.current_append_truncate.p50_ns),
        format_signed_ms(signed_gain(optimization.baseline_append_truncate.p50_ns, optimization.current_append_truncate.p50_ns)?),
        format_ms(optimization.baseline_materialization.p50_ns),
        format_ms(optimization.current_materialization.p50_ns),
        format_signed_ms(signed_gain(optimization.baseline_materialization.p50_ns, optimization.current_materialization.p50_ns)?),
    ).map_err(display_error)?;
    for receipt in &optimization.verified_open {
        writeln!(
            output,
            "| Verified open {} | `{}` | `{}` | `{}` | product retained-union scrub; current scrub/graphs/fetched/object B/scratch=`{}/{}/{}/{}/{}` |",
            receipt.root,
            format_ms(receipt.before_ns),
            format_ms(receipt.after_ns),
            format_signed_ms(signed_gain(receipt.before_ns, receipt.after_ns)?),
            receipt.retained_union_scrubs,
            receipt.namespace_graphs,
            receipt.fetched_rows,
            receipt.object_bytes_read,
            receipt.scratch_tables,
        )
        .map_err(display_error)?;
    }
    writeln!(output, "\nShift-route mix changed from CloneShift/InPlaceShift `{}/{}` to `{}/{}`; append/truncate EOF splices retain exact InPlaceShift durability and zero FullFallback.\n\nResult: `PASS`\n\n| Category | Result | Decisive evidence |\n|---|---|---|\n| Correctness | `PASS` | `{}/51 physical; {}/34 canonical; {}/8 selected history` |\n| Durability | `PASS` | `{}` transactions / `{}` COMMITs / exact RefState rotation |\n| Locality | `PASS` | `{}` CDC B; zero unaffected canonical suffix; node bounds exact |\n| Physical routes | `PASS` | `{}` patch / `{}` shift / `{}` FullFallback refreshes |\n| Resources | `PASS` | `RSS/Q/FD/connections/residue closed` |\n| Custody | `PASS` | `source/executable/fixture/rows bound by digest` |\n| Complete wall | `PASS` | `{} ms < 60 s` |\n\nReason: All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.\n", optimization.baseline_clone_shift, optimization.baseline_in_place_shift, optimization.current_clone_shift, optimization.current_in_place_shift, physical_oracles, canonical_transitions, selected_history_roots_passed, sum_key(rows, None, "transactions_started")?, sum_key(rows, None, "publication_commits")?, sum_locality_key(rows, "cdc_bytes_scanned")?, patch_refreshes, shift_refreshes, fallback_refreshes, format_ms(complete_wall_ns)).map_err(display_error)?;
    if disposition != Disposition::Pass {
        output = output.replacen(
            "Disposition: `PASS`",
            &format!("Disposition: `{}`", disposition.as_str()),
            1,
        );
        output = output.replacen(
            "Result: `PASS`",
            &format!("Result: `{}`", disposition.as_str()),
            1,
        );
        output = output.replacen(
            "Reason: All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.",
            "Reason: All hard gates passed; a retained report-only observation requires source review before PASS.",
            1,
        );
    }
    validate_summary_markdown_contract(&output)?;
    Ok(output)
}

const SUMMARY_HEADINGS: [&str; 17] = [
    "# LayerFS Stage 1.1 — Single-file APFS Edge Result",
    "## 1. Disposition and custody",
    "## 2. Overall gate scoreboard",
    "## 3. Physical APFS edit to LayerFS checkpoint",
    "## 4. Physical count-changing amplification",
    "## 5. Logical LayerFS edit to physical APFS refresh",
    "## 6. Refresh-route summary",
    "## 7. Canonical locality",
    "## 8. Multi-edit save bursts",
    "## 9. Fresh Verified history sessions",
    "## 10. Materialization and reconstruction",
    "## 11. Transaction and authentication closure",
    "## 12. Storage growth and amplification",
    "## 13. Resource closure",
    "## 14. Timer closure",
    "## 15. Preserved failures and unavailable observations",
    "## 16. Final disposition",
];

const SUMMARY_TABLE_HEADERS: [&str; 23] = [
    "| Field | Value |",
    "| Artifact | SHA-256 | Additional identity |",
    "| Gate | Required | Observed | Status |",
    "| Operation | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms | Oracle | Status |",
    "| Size band | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms |",
    "| Seq | Operation | Offset | Suffix B | Replacement B | Native read B | Native write B | Equation | Route | Status |",
    "| Kind | n | Suffix shifted B | Native read B | Native write B | Amplification |",
    "| Operation | n | Logical p50 ms | Logical p95 ms | Route class | Refresh p50 ms | Refresh p95 ms | End-to-end p50 ms | End-to-end p95 ms | Oracle |",
    "| Route | Required count | Observed | p50 ms | p95 ms | Physical B | Rematerializations | Status |",
    "| Population | Transitions | CDC expected B | CDC observed B | Unaffected reads B | Unaffected writes B | Max nodes read | Max nodes emitted | Status |",
    "| Root | Pattern | Sub-edits | Native ms | Oracle ms | Checkpoint ms | Row ms | Transactions | COMMITs | Final B | Status |",
    "| Session | Head | Roots checked | Open/scrub ms | Objects authenticated | Bytes authenticated | Probe B | Writer tx | Native writes | Status |",
    "| Probe ordinal | n | p50 ms | p95 ms | Non-payload rows | Payload rows | Cache classification |",
    "| Root | Purpose | Logical B | Wall ms | MiB/s | Native write B | Exact bytes | Metadata | Cleanup |",
    "| Equation | Required | Observed/failures | Status |",
    "| Counter phase | Rows | Statements | Fetched/auth/role | Object read B | Object write B | Tx/COMMIT | Scrubs | Engine/VFS scratch tables | Q structural-reservation high B | Connections |",
    "| Metric | Initial | Terminal/peak | Delta | Status |",
    "| Root range | Transitions | Canonical B written | DB growth B | Amplification |",
    "| Resource | Hard gate | Observed | Status |",
    "| Row group | Rows | Maximum residual ns | Sum residual ns | Status |",
    "| Sequence | Artifact/row | Field | Availability/failure | Reason | Disposition impact |",
    "| Optimization metric | Attempt-007 before ms | Current after ms | Absolute gain ms | Owner |",
    "| Category | Result | Decisive evidence |",
];

fn validate_summary_headings(markdown: &str) -> EvalResult<()> {
    let actual = markdown
        .lines()
        .filter(|line| line.starts_with('#'))
        .collect::<Vec<_>>();
    if actual != SUMMARY_HEADINGS {
        return Err(format!("summary heading order mismatch: {actual:?}"));
    }
    Ok(())
}

fn validate_summary_markdown_contract(markdown: &str) -> EvalResult<()> {
    validate_summary_headings(markdown)?;
    for header in SUMMARY_TABLE_HEADERS {
        if !markdown.contains(header) {
            return Err(format!(
                "summary Markdown missing required table header {header}"
            ));
        }
    }
    if markdown.contains("Disposition: `PASS`") && markdown.contains("| `FAIL` |") {
        return Err("PASS summary Markdown contains a hard-gate FAIL cell".to_owned());
    }
    Ok(())
}

fn validate_summary_pair(json: &str, markdown: &str) -> EvalResult<()> {
    let status = json_string(json, "status")?;
    if !markdown.contains(&format!("Disposition: `{status}`"))
        || !markdown.contains(&format!("Result: `{status}`"))
    {
        return Err("summary JSON/Markdown disposition mismatch".to_owned());
    }
    let materialization = json_object(json, "materialization")?;
    let by_root = json_object(materialization, "by_root")?;
    for (root, purpose) in [
        ("R15", "Physical-chain milestone"),
        ("R30", "Logical-refresh milestone"),
        ("R34", "Burst-chain milestone"),
    ] {
        let receipt = json_object(by_root, root)?;
        let cleanup = json_bool(receipt, "cleanup_exact")?;
        let line = markdown
            .lines()
            .find(|line| line.starts_with(&format!("| {root} | {purpose}")))
            .ok_or_else(|| format!("summary Markdown missing {root} materialization row"))?;
        if line.ends_with("| `PASS` |") != cleanup {
            return Err(format!("{root} JSON/Markdown cleanup mismatch"));
        }
    }
    let failures = json_array_objects(json, "failures")?;
    for failure in &failures {
        let artifact = json_string(failure, "artifact")?;
        if !markdown.contains(&artifact) {
            return Err(format!("failure ledger missing from Markdown: {artifact}"));
        }
    }
    if !markdown.contains(&format!("Preserved failed attempts: `{}`", failures.len())) {
        return Err("summary JSON/Markdown failure-ledger count mismatch".to_owned());
    }
    let optimization = json_object(json, "optimization")?;
    let complete = json_object(optimization, "complete_wall")?;
    let before = json_u128(complete, "before_ns")?;
    let after = json_u128(complete, "after_ns")?;
    if !markdown.contains(&format!(
        "| Complete campaign wall | `{}` | `{}` |",
        format_ms(before),
        format_ms(after)
    )) {
        return Err("optimization JSON/Markdown complete-wall mismatch".to_owned());
    }
    let verified = json_object(optimization, "verified_open_by_root")?;
    for root in ["R5", "R15", "R30", "R34"] {
        let receipt = json_object(verified, root)?;
        if !markdown.contains(&format!(
            "| Verified open {root} | `{}` | `{}` |",
            format_ms(json_u128(receipt, "before_ns")?),
            format_ms(json_u128(receipt, "after_ns")?),
        )) {
            return Err(format!("optimization JSON/Markdown {root} mismatch"));
        }
        let counters = format!(
            "current scrub/graphs/fetched/object B/scratch=`{}/{}/{}/{}/{}`",
            json_u128(receipt, "retained_union_scrubs")?,
            json_u128(receipt, "namespace_graphs")?,
            json_u128(receipt, "fetched_rows")?,
            json_u128(receipt, "object_bytes_read")?,
            json_u128(receipt, "scratch_tables")?,
        );
        if !markdown.contains(&counters) {
            return Err(format!(
                "optimization JSON/Markdown {root} scrub-counter mismatch"
            ));
        }
    }
    Ok(())
}

fn title(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn optional_route_stats(rows: &[ParsedRow], route: &str) -> EvalResult<Option<Statistics>> {
    let selected = rows
        .iter()
        .filter(|row| row.row_group == "C05" && row.native_route == route)
        .map(|row| phase_wall(&row.json, "changed_root_refresh"))
        .collect::<EvalResult<Vec<_>>>()?;
    if selected.is_empty() {
        Ok(None)
    } else {
        statistics(selected).map(Some)
    }
}

fn sum_route_bytes(rows: &[ParsedRow], route: &str, operation: Option<&str>) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| {
            row.row_group == "C05"
                && (row.native_route == route
                    || route == "Shift"
                        && matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift"))
                && operation.is_none_or(|operation| row.operation == operation)
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "route physical bytes overflow".to_owned())
        })
}

fn sum_patch_bytes(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| {
            row.row_group == "C05"
                && matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
        })
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_u128(row, "bytes_written")?)
                .ok_or_else(|| "patch bytes overflow".to_owned())
        })
}

fn maximum_group_key(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .map(|row| row_u128(row, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| format!("no {group} values for {key}"))
}

fn sum_subfield(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| row.row_group == group)
        .try_fold(0_u128, |total, row| {
            json_all_u128(&row.json, key)?
                .into_iter()
                .try_fold(total, |total, value| {
                    total
                        .checked_add(value)
                        .ok_or_else(|| format!("{key} subfield sum overflow"))
                })
        })
}

fn storage_initial(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .find(|row| row.row_group == "C02")
        .map(|row| row_u128(row, "database_bytes"))
        .transpose()?
        .ok_or_else(|| "initial database bytes unavailable".to_owned())
}

fn storage_terminal(rows: &[ParsedRow]) -> EvalResult<u128> {
    rows.iter()
        .rev()
        .find(|row| row.row_group == "C07")
        .map(|row| row_u128(row, "database_bytes"))
        .transpose()?
        .ok_or_else(|| "terminal database bytes unavailable".to_owned())
}

fn storage_amplification(rows: &[ParsedRow]) -> EvalResult<f64> {
    let canonical = sum_key(rows, None, "canonical_object_bytes_written")?;
    Ok(if canonical == 0 {
        0.0
    } else {
        storage_terminal(rows)?.saturating_sub(storage_initial(rows)?) as f64 / canonical as f64
    })
}

fn range_sum(rows: &[ParsedRow], group: &str, key: &str) -> EvalResult<u128> {
    sum_key(rows, Some(group), key)
}

fn range_amplification(rows: &[ParsedRow], group: &str) -> EvalResult<f64> {
    let canonical = range_sum(rows, group, "canonical_object_bytes_written")?;
    Ok(if canonical == 0 {
        0.0
    } else {
        range_sum(rows, group, "database_growth_bytes")? as f64 / canonical as f64
    })
}

fn rust_cargo_source_paths() -> EvalResult<Vec<String>> {
    let listed = command_bytes(
        "git",
        &[
            "ls-files",
            "-co",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
            "Cargo.toml",
            ":(glob)**/Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    let mut paths = listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn source_identity() -> EvalResult<SourceIdentity> {
    let root = stage1_fixture::workspace_root();
    let git_commit = command_output("git", &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let dirty_tree = !command_output("git", &["status", "--porcelain"])?
        .trim()
        .is_empty();
    let paths = rust_cargo_source_paths()?;
    let mut tree = blake3::Hasher::new();
    let mut manifest = String::new();
    for path in paths {
        let bytes = fs::read(root.join(&path)).map_err(io_error)?;
        tree.update(path.as_bytes());
        tree.update(&[0]);
        tree.update(&bytes);
        manifest.push_str(&sha256_bytes(&bytes)?);
        manifest.push_str("  ");
        manifest.push_str(&path);
        manifest.push('\n');
    }
    let executable_path = std::env::current_exe().map_err(io_error)?;
    Ok(SourceIdentity {
        git_commit,
        dirty_tree,
        tree_blake3: tree.finalize().to_hex().to_string(),
        manifest_sha256: sha256_bytes(manifest.as_bytes())?,
        executable_sha256: sha256_file(&executable_path)?,
        executable_blake3: stage1_fixture::hash_file(&executable_path)?,
        executable_path,
    })
}

fn schedule_json(schedule: &FrozenSchedule) -> EvalResult<String> {
    let edits = schedule
        .edits
        .iter()
        .map(|edit| {
            let replacement = replacement_bytes(
                edit.serial,
                usize::try_from(edit.insert_bytes).expect("frozen insert length fits usize"),
            );
            format!(
                concat!(
                    "{{\"tag\":\"{}\",\"serial\":{},\"epoch\":{},",
                    "\"kind\":\"{}\",\"size_band\":\"{}\",\"offset\":{},",
                    "\"delete_bytes\":{},\"insert_bytes\":{},\"before_bytes\":{},",
                    "\"after_bytes\":{},\"replacement_offset\":{},",
                    "\"replacement_digest\":\"{}\"}}"
                ),
                edit.tag,
                edit.serial,
                edit.epoch,
                edit.kind.as_str(),
                edit.size_band,
                edit.offset,
                edit.delete_bytes,
                edit.insert_bytes,
                edit.before_bytes,
                edit.after_bytes,
                edit.replacement_offset,
                blake3::hash(&replacement).to_hex(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let rows = schedule
        .rows
        .iter()
        .map(|row| {
            let pre_ref_slot = row.transition_root.map(|root| format!("R{}", root - 1));
            let post_ref_slot = row.transition_root.map(|root| format!("R{root}"));
            format!(
                concat!(
                    "{{\"row_index\":{},\"row_id\":\"{}\",\"row_group\":\"{}\",",
                    "\"sequence\":{},\"epoch\":{},\"direction\":\"{}\",",
                    "\"operation\":\"{}\",\"size_band\":\"{}\",",
                    "\"edit_index\":{},\"burst_index\":{},\"history_session\":{},",
                    "\"milestone_root\":{},\"transition_root\":{},",
                    "\"pre_ref_slot\":{},\"post_ref_slot\":{}}}"
                ),
                row.row_index,
                row.row_id,
                row.row_group,
                row.sequence,
                row.epoch,
                row.direction,
                row.operation,
                row.size_band,
                option_usize_json(row.edit_index),
                option_usize_json(row.burst_index),
                option_u8_json(row.history_session),
                option_u8_json(row.milestone_root),
                option_u8_json(row.transition_root),
                pre_ref_slot.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\"")),
                post_ref_slot.map_or_else(|| "null".to_owned(), |value| format!("\"{value}\"")),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-schedule-v1\",",
            "\"row_count\":47,\"edit_suboperation_count\":51,",
            "\"transition_count\":34,\"snapshot_count\":35,",
            "\"replacement_backing_bytes\":{},",
            "\"replacement_generator\":\"tag_serial*17+index*31 modulo 256\",",
            "\"initial_generator\":\"stage1_fixture::fill_retained_buffer\",",
            "\"row_order\":\"execution-order-with-history-after-each-five-edit-epoch\",",
            "\"edits\":[{}],\"rows\":[{}]}}\n"
        ),
        schedule.replacement_backing.len(),
        edits,
        rows,
    ))
}

struct Campaign<'a> {
    run: &'a Path,
    started: Instant,
    started_unix_ns: u128,
    rows: File,
    schedule: &'a FrozenSchedule,
    next_row: usize,
    row_wall_sum_ns: u128,
    fd_baseline: u64,
    rss_peak_bytes: u64,
    q_high_water_bytes: u64,
    q_maximum_terminal_bytes: u64,
    store_connection_high_water: u64,
    physical_oracles: u64,
    canonical_transitions: u64,
    workspace_materializations: u64,
    rematerializations: u64,
    root_digests: Vec<String>,
}

fn enforce_campaign_limit(started: Instant) -> EvalResult<()> {
    if started.elapsed().as_nanos() >= CAMPAIGN_LIMIT_NS {
        Err("complete_wall_ns < 60,000,000,000".to_owned())
    } else {
        Ok(())
    }
}

impl Campaign<'_> {
    fn scheduled(&self, id: &str) -> EvalResult<ScheduledRow> {
        if enforce_campaign_limit(self.started).is_err() {
            begin_failure_context("__between_rows__", "time_budget");
            return Err("complete_wall_ns < 60,000,000,000".to_owned());
        }
        begin_failure_context(id, "admission");
        let row = self
            .schedule
            .rows
            .get(self.next_row)
            .ok_or_else(|| format!("no scheduled row remains for {id}"))?;
        if row.row_id != id {
            return Err(format!(
                "row order mismatch: expected {}, got {id}",
                row.row_id
            ));
        }
        Ok(row.clone())
    }

    fn append(&mut self, receipt: RowReceipt) -> EvalResult<()> {
        enforce_campaign_limit(self.started)?;
        if receipt.schedule.row_index != self.next_row {
            return Err(format!(
                "append row index {} != {}",
                receipt.schedule.row_index, self.next_row
            ));
        }
        let retained_digest = (receipt.status == "PASS"
            && matches!(receipt.schedule.row_group, "C02" | "C03" | "C05" | "C07"))
        .then(|| receipt.oracle.content_digest.clone());
        if retained_digest.as_ref().is_some_and(String::is_empty) {
            return Err("retained root digest is empty".to_owned());
        }
        let json = receipt.json()?;
        self.rows.write_all(json.as_bytes()).map_err(io_error)?;
        self.rows.sync_all().map_err(io_error)?;
        self.row_wall_sum_ns = self
            .row_wall_sum_ns
            .checked_add(receipt.row_wall_ns)
            .ok_or_else(|| "row wall sum overflow".to_owned())?;
        self.rss_peak_bytes = self.rss_peak_bytes.max(receipt.resources.rss_peak_bytes);
        self.store_connection_high_water = self
            .store_connection_high_water
            .max(receipt.resources.active_store_connections);
        if let Some(operation) = receipt.operation {
            self.q_high_water_bytes = self
                .q_high_water_bytes
                .max(operation.operation_q_high_water_bytes);
            self.q_maximum_terminal_bytes = self
                .q_maximum_terminal_bytes
                .max(operation.operation_q_terminal_bytes);
            self.workspace_materializations = self
                .workspace_materializations
                .checked_add(operation.workspace_materializations)
                .ok_or_else(|| "workspace materialization count overflow".to_owned())?;
            self.rematerializations = self
                .rematerializations
                .checked_add(operation.rematerializations)
                .ok_or_else(|| "rematerialization count overflow".to_owned())?;
        }
        self.next_row += 1;
        if let Some(digest) = retained_digest {
            self.root_digests.push(digest);
        }
        Ok(())
    }
}

pub fn run(run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("refusing to overwrite {}", run.display()));
    }
    if cfg!(debug_assertions) {
        return Err("Stage 1.1 campaign requires the release evaluator".to_owned());
    }
    let failure_started = Instant::now();
    let failure_started_unix_ns = unix_ns()?;
    match run_inner(run) {
        Ok(Disposition::Pass) => Ok(()),
        Ok(Disposition::Revise) => Err(format!(
            "Stage 1.1 REVISE artifact preserved at {}",
            run.display()
        )),
        Ok(Disposition::Fail) => Err("Stage 1.1 FAIL disposition".to_owned()),
        Err(error) => {
            if run.exists() {
                let stderr = format!("{error}\n");
                let path = run.join("stderr.txt");
                if path.exists() {
                    let _ = durable_replace(&path, &stderr);
                } else {
                    let _ = durable_write(&path, &stderr);
                }
                let _ = append_failed_row(run, &error, &path);
                let _ = write_failure_artifacts(
                    run,
                    &error,
                    failure_started_unix_ns,
                    failure_started.elapsed().as_nanos(),
                );
            }
            Err(error)
        }
    }
}

fn append_failed_row(run: &Path, error: &str, stderr: &Path) -> EvalResult<()> {
    let schedule = frozen_schedule()?;
    let rows_path = run.join("rows.jsonl");
    let existing = fs::read_to_string(&rows_path).unwrap_or_default();
    let index = existing.lines().count();
    let scheduled = match schedule.rows.get(index) {
        Some(row) => row.clone(),
        None => return Ok(()),
    };
    let (context_row_id, phase, row_wall_ns) = failure_observation();
    if context_row_id == "__between_rows__" {
        return Ok(());
    }
    if context_row_id != scheduled.row_id {
        return Err(format!(
            "failure context row {context_row_id} != next scheduled {}",
            scheduled.row_id
        ));
    }
    let (before_bytes, after_bytes, edit) = if let Some(edit_index) = scheduled.edit_index {
        let edit = schedule.edits[edit_index].clone();
        (edit.before_bytes, edit.after_bytes, Some(edit))
    } else if let Some(burst_index) = scheduled.burst_index {
        let burst = &schedule.bursts[burst_index];
        (
            burst
                .edits
                .first()
                .map_or(INITIAL_BYTES, |edit| edit.before_bytes),
            burst
                .edits
                .last()
                .map_or(INITIAL_BYTES, |edit| edit.after_bytes),
            None,
        )
    } else {
        (INITIAL_BYTES, INITIAL_BYTES, None)
    };
    let work = run.join(".work");
    let resources =
        observe_external_resources(Some(&work), Some(&work.join("store"))).unwrap_or_default();
    let receipt = RowReceipt {
        schedule: scheduled,
        status: "FAIL",
        before_bytes,
        after_bytes,
        edit,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: vec![Phase {
            name: phase,
            wall_ns: row_wall_ns,
        }],
        phase_counters: Vec::new(),
        row_wall_ns,
        row_residual_ns: 0,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources,
        oracle: OracleReceipt {
            logical_length: after_bytes,
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: Some((
            "EvaluatorOrProductGate".to_owned(),
            error.to_owned(),
            phase.to_owned(),
            error.to_owned(),
            Some(sha256_file(stderr)?),
        )),
        custody: None,
    };
    let mut rows = OpenOptions::new()
        .append(true)
        .open(&rows_path)
        .map_err(io_error)?;
    rows.write_all(receipt.json()?.as_bytes())
        .map_err(io_error)?;
    rows.sync_all().map_err(io_error)
}

fn null_map(keys: &[&str]) -> String {
    format!(
        "{{{}}}",
        keys.iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn optional_artifact_sha256(path: &Path) -> EvalResult<String> {
    if path.is_file() {
        Ok(format!("\"{}\"", sha256_file(path)?))
    } else {
        Ok("null".to_owned())
    }
}

fn failure_summary_json(
    run: &Path,
    error: &str,
    phase: &str,
    population: (usize, usize, usize),
    walls: (u128, u128, u128),
) -> EvalResult<String> {
    let (rows_valid, edit_suboperations_observed, transitions_observed) = population;
    let (complete_wall_ns, row_wall_sum_ns, outside_rows_wall_ns) = walls;
    let kinds = null_map(&["overwrite", "insert", "delete", "append", "truncate"]);
    let bands = null_map(&["near-8-kib", "near-16-kib", "near-32-kib"]);
    let selected_roots = null_map(&[
        "R0", "R5", "R10", "R15", "R20", "R25", "R30", "R31", "R32", "R33", "R34",
    ]);
    let by_row_group = null_map(&[
        "C00", "C01", "C02", "C03", "C04", "C05", "C06", "C07", "C08", "C09",
    ]);
    let authentication = format!(
        concat!(
            "{{\"fetched_authentication_failures\":null,",
            "\"fetched_role_decode_failures\":null,",
            "\"new_object_equation_failures\":null,",
            "\"incumbent_equation_failures\":null,",
            "\"payload_batch_maximum\":null,\"phase_attribution\":{}}}"
        ),
        null_map(&[
            "store_open",
            "materialization",
            "checkpoint",
            "logical_edit",
            "apfs_refresh",
            "canonical_witness",
            "verified_open",
            "history_read",
            "storage_observation",
        ])
    );
    let optimization = format!(
        concat!(
            "{{\"baseline_run\":null,\"baseline_rows_sha256\":null,",
            "\"baseline_summary_sha256\":null,\"complete_wall\":null,",
            "\"counter_snapshot_wall\":null,\"history_read_wall\":null,",
            "\"verified_open_by_root\":{},\"append_truncate_refresh\":null,",
            "\"milestone_materialization\":null,\"shift_routes\":null}}"
        ),
        null_map(&["R5", "R15", "R30", "R34"])
    );
    let artifacts = format!(
        concat!(
            "\"environment_sha256\":{},\"master_sha256\":{},",
            "\"readiness_sha256\":{},\"schedule_sha256\":{},",
            "\"rows_sha256\":\"{}\",\"rows_line_count\":{},",
            "\"campaign_time_sha256\":\"{}\",",
            "\"release_executable_sha256\":null,\"release_executable_blake3\":null,",
            "\"source_tree_blake3\":null,\"source_manifest_sha256\":null"
        ),
        optional_artifact_sha256(&run.join("environment.json"))?,
        optional_artifact_sha256(&run.join("master.json"))?,
        optional_artifact_sha256(&run.join("readiness.json"))?,
        optional_artifact_sha256(&run.join("schedule.json"))?,
        sha256_file(&run.join("rows.jsonl"))?,
        rows_valid,
        sha256_file(&run.join("campaign-time.txt"))?,
    );
    let summary = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-summary-v1\",\"status\":\"FAIL\",",
            "\"source\":{},\"fixture\":{},",
            "\"population\":{{\"expected_rows\":47,\"valid_rows\":{},",
            "\"expected_edit_suboperations\":51,\"observed_edit_suboperations\":{},",
            "\"expected_transitions\":34,\"observed_transitions\":{},",
            "\"measured_workflows\":1}},\"roots\":{},",
            "\"walls_ns\":{{\"complete_wall\":{},\"row_wall_sum\":{},",
            "\"outside_rows_wall\":{},\"timer_residual\":0,",
            "\"admission\":null,\"reset\":null,\"store_open\":null,",
            "\"initial_materialization\":null,\"physical_phase\":null,",
            "\"physical_history_phase\":null,\"logical_refresh_phase\":null,",
            "\"logical_history_phase\":null,\"burst_phase\":null,",
            "\"milestone_materialization_phase\":null,\"cleanup\":null,",
            "\"artifact_write\":null}},",
            "\"physical_to_logical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"native_edit\":null,\"durable_checkpoint\":null,",
            "\"edit_plus_checkpoint\":null,\"count_change_amplification\":{},",
            "\"physical_oracle\":null}},",
            "\"logical_to_physical\":{{\"by_kind\":{},\"by_size_band\":{},",
            "\"direct_logical_edit\":null,\"changed_root_refresh\":null,",
            "\"logical_edit_plus_refresh\":null,\"physical_oracle\":null}},",
            "\"refresh_routes\":{},",
            "\"bursts\":{{\"by_root\":{},\"aggregate\":null,",
            "\"suboperation_count\":null,\"checkpoint_count\":null,",
            "\"transaction_count\":null}},",
            "\"history\":{},",
            "\"materialization\":{{\"initial\":null,\"by_root\":{},",
            "\"milestone_aggregate\":null,\"live_workspace_materializations\":null,",
            "\"witness_materializations\":null,\"workspace_reuses\":null,",
            "\"rematerializations\":null}},",
            "\"canonical_locality\":{},\"transactions\":{},",
            "\"authentication\":{},",
            "\"storage\":{{\"initial_database_bytes\":null,",
            "\"terminal_database_bytes\":null,\"initial_logical_engine_bytes\":null,",
            "\"terminal_logical_engine_bytes\":null,",
            "\"canonical_object_bytes_written\":null,\"database_growth_bytes\":null,",
            "\"maximum_transition_database_growth_bytes\":null,",
            "\"physical_to_canonical_amplification\":null,",
            "\"scratch_high_water_bytes\":null,\"rollback_journal_bytes\":null,",
            "\"terminal_sidecars\":null,\"by_root_range\":{}}},",
            "\"resources\":{},",
            "\"timer_closure\":{{\"by_row_group\":{},",
            "\"maximum_row_residual_ns\":null,\"row_residual_sum_ns\":null,",
            "\"complete_wall_ns\":{},\"row_wall_sum_ns\":{},",
            "\"outside_rows_wall_ns\":{},\"timer_residual_ns\":0,",
            "\"hard_limit_ns\":60000000000}},",
            "\"correctness\":{{\"physical_oracles_expected\":51,",
            "\"physical_oracles_passed\":null,\"canonical_transitions_expected\":34,",
            "\"canonical_transitions_passed\":null,\"save_bursts_expected\":4,",
            "\"save_bursts_passed\":null,\"selected_history_roots_expected\":8,",
            "\"selected_history_roots_passed\":null,\"route_labels_exact\":null,",
            "\"terminal_length_exact\":null,\"fixture_unchanged\":null}},",
            "\"optimization\":{},",
            "\"unavailable\":[{{\"field\":\"summary.remaining_observations\",",
            "\"availability\":\"Unavailable\",",
            "\"reason\":\"campaign stopped at the first failed equation\"}}],",
            "\"failures\":[{{\"phase\":\"{}\",",
            "\"first_failed_equation\":\"{}\"}}],",
            "\"artifacts\":{{{}}},\"disposition_reason\":\"{}\"}}\n"
        ),
        null_map(&[
            "git_commit",
            "dirty_tree",
            "tree_blake3",
            "manifest_sha256",
            "release_executable_path",
            "release_executable_sha256",
            "release_executable_blake3",
        ]),
        null_map(&[
            "master_path",
            "master_sha256",
            "fixture_blake3",
            "apfs_identity",
            "initial_bytes",
            "maximum_bytes",
            "terminal_bytes",
            "master_unchanged",
        ]),
        rows_valid,
        edit_suboperations_observed,
        transitions_observed,
        selected_roots,
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        kinds,
        bands,
        null_map(&["insert", "delete", "append", "truncate"]),
        kinds,
        bands,
        null_map(&[
            "clone_patch",
            "in_place_patch",
            "patch_aggregate",
            "clone_shift",
            "in_place_shift",
            "shift_aggregate",
            "insert_shift",
            "delete_shift",
            "append_shift",
            "truncate_shift",
            "full_fallback_count",
        ]),
        null_map(&["R31", "R32", "R33", "R34"]),
        null_map(&[
            "sessions",
            "aggregate",
            "selected_roots",
            "verified_open_count",
            "probe_count",
            "first_probe",
            "second_probe",
            "third_probe",
            "first_probe_non_payload_rows",
            "warm_probe_non_payload_rows",
        ]),
        null_map(&["R15", "R30", "R34"]),
        null_map(&[
            "physical_checkpoints",
            "direct_logical_edits",
            "save_bursts",
            "total",
            "cdc_bytes_expected",
            "cdc_bytes_observed",
            "payload_bytes_written",
            "unaffected_payload_reads",
            "unaffected_payload_writes",
            "maximum_rope_nodes_read",
            "maximum_rope_nodes_emitted",
            "content_directory_nodes_emitted",
            "payload_batch_maximum",
        ]),
        null_map(&[
            "expected",
            "observed",
            "committed",
            "rolled_back",
            "publication_commits",
            "publication_transactions_started",
            "publication_transactions_rolled_back",
            "admission_transactions_started",
            "admission_transactions_committed",
            "admission_transactions_rolled_back",
            "admission_statements",
            "integrity_transactions_started",
            "integrity_transactions_committed",
            "integrity_transactions_rolled_back",
            "integrity_statements",
            "retained_roots_validated",
            "generation_increment_failures",
        ]),
        authentication,
        null_map(&["R0-R15", "R15-R30", "R30-R34"]),
        null_map(&[
            "rss_peak_bytes",
            "largest_buffer_bytes",
            "operation_q_high_water_bytes",
            "operation_q_maximum_terminal_bytes",
            "page_size",
            "cache_pages",
            "cache_spill_pages",
            "store_connection_high_water",
            "store_connections_terminal",
            "fd_baseline",
            "fd_terminal",
            "product_child_process_peak",
            "child_processes_terminal",
            "owned_temp_residue_entries",
            "sidecar_residue_entries",
            "live_rematerializations",
            "network_operations",
        ]),
        by_row_group,
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        optimization,
        json_escape(phase),
        json_escape(error),
        artifacts,
        json_escape(error),
    );
    validate_summary_json_contract(&summary)?;
    Ok(summary)
}

fn unavailable_markdown_table(header: &str) -> String {
    let columns = header.matches('|').count().saturating_sub(1);
    let separator = format!("|{}|", vec!["---"; columns].join("|"));
    let mut values = vec!["—"; columns];
    if let Some(first) = values.first_mut() {
        *first = "Unavailable";
    }
    format!("{header}\n{separator}\n|{}|", values.join("|"))
}

fn failure_summary_markdown(error: &str, phase: &str) -> EvalResult<String> {
    let tables: [&[usize]; 16] = [
        &[0, 1],
        &[2],
        &[3, 4],
        &[5, 6],
        &[7],
        &[8],
        &[9],
        &[10],
        &[11, 12],
        &[13],
        &[14, 15],
        &[16, 17],
        &[18],
        &[19],
        &[20],
        &[21, 22],
    ];
    let mut markdown = format!("{}\n\nDisposition: `FAIL`\n\n", SUMMARY_HEADINGS[0]);
    for (heading, table_indices) in SUMMARY_HEADINGS[1..].iter().zip(tables) {
        writeln!(
            markdown,
            "{heading}\n\nFailed in `{}` before terminal PASS: `{}`.\n",
            phase, error
        )
        .map_err(display_error)?;
        for index in table_indices {
            writeln!(
                markdown,
                "{}\n",
                unavailable_markdown_table(SUMMARY_TABLE_HEADERS[*index])
            )
            .map_err(display_error)?;
        }
    }
    validate_summary_markdown_contract(&markdown)?;
    Ok(markdown)
}

fn write_failure_artifacts(
    run: &Path,
    error: &str,
    started_unix_ns: u128,
    complete_wall_ns: u128,
) -> EvalResult<()> {
    let rows_path = run.join("rows.jsonl");
    let contents = fs::read_to_string(&rows_path).unwrap_or_default();
    let rows_valid = contents.lines().count();
    let (_, context_phase, _) = failure_observation();
    let first_failed_phase = contents
        .lines()
        .rev()
        .find(|row| json_string(row, "status").as_deref() == Ok("FAIL"))
        .map(|row| json_string(row, "phase"))
        .transpose()?
        .unwrap_or_else(|| context_phase.to_owned());
    let row_wall_sum_ns = contents
        .lines()
        .map(|row| json_u128(row, "row_wall_ns"))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .sum::<u128>();
    let outside_rows_wall_ns = complete_wall_ns
        .checked_sub(row_wall_sum_ns)
        .ok_or_else(|| "failure timer row_wall_sum_ns <= complete_wall_ns".to_owned())?;
    let schedule = frozen_schedule()?;
    let edit_suboperations_observed = schedule
        .rows
        .iter()
        .zip(contents.lines())
        .filter(|(_, receipt)| json_string(receipt, "status").as_deref() == Ok("PASS"))
        .map(|(row, _)| {
            if row.edit_index.is_some() {
                1
            } else {
                row.burst_index
                    .map_or(0, |index| schedule.bursts[index].edits.len())
            }
        })
        .sum::<usize>();
    let transitions_observed = contents
        .lines()
        .filter(|row| {
            matches!(
                json_string(row, "row_group").as_deref(),
                Ok("C03" | "C05" | "C07")
            ) && !row.contains("\"post_ref\":null")
        })
        .count();
    let time = format!(
        concat!(
            "schema=layerfs-stage1.1-campaign-time-v1\nstatus=FAIL\n",
            "started_unix_ns={}\ncompleted_unix_ns={}\ncomplete_wall_ns={}\n",
            "row_wall_sum_ns={}\noutside_rows_wall_ns={}\ntimer_residual_ns=0\n",
            "hard_limit_ns=60000000000\nrows_expected=47\nrows_valid={}\n",
            "edit_suboperations_expected=51\nedit_suboperations_observed={}\n",
            "transitions_expected=34\ntransitions_observed={}\n"
        ),
        started_unix_ns,
        started_unix_ns.saturating_add(complete_wall_ns),
        complete_wall_ns,
        row_wall_sum_ns,
        outside_rows_wall_ns,
        rows_valid,
        edit_suboperations_observed,
        transitions_observed,
    );
    validate_timer_equation(&time)?;
    durable_replace(&run.join("campaign-time.txt"), &time)?;
    let summary = failure_summary_json(
        run,
        error,
        &first_failed_phase,
        (
            rows_valid,
            edit_suboperations_observed,
            transitions_observed,
        ),
        (complete_wall_ns, row_wall_sum_ns, outside_rows_wall_ns),
    )?;
    durable_replace(&run.join("summary.json"), &summary)?;
    let markdown = failure_summary_markdown(error, &first_failed_phase)?;
    durable_replace(&run.join("summary.md"), &markdown)
}

fn run_inner(run: &Path) -> EvalResult<Disposition> {
    let started = Instant::now();
    let started_unix_ns = unix_ns()?;
    let schedule = frozen_schedule()?;
    let schedule_bytes = schedule_json(&schedule)?;
    let fixture = fixture_root();
    let master = read_master(&fixture)?;
    let source = source_identity()?;
    let readiness = fs::read_to_string(readiness_path()).map_err(io_error)?;
    admit_readiness(&readiness, &source, &master, &schedule_bytes)?;

    fs::create_dir(run).map_err(io_error)?;
    stage1_fixture::sync_directory(
        run.parent()
            .ok_or_else(|| "run directory has no parent".to_owned())?,
    )?;
    let run_directory = run.canonicalize().map_err(io_error)?;
    let run = run_directory.as_path();
    let rows = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(run.join("rows.jsonl"))
        .map_err(io_error)?;
    let fd_baseline = fd_count()?;
    let mut campaign = Campaign {
        run,
        started,
        started_unix_ns,
        rows,
        schedule: &schedule,
        next_row: 0,
        row_wall_sum_ns: 0,
        fd_baseline,
        rss_peak_bytes: maximum_rss_bytes()?,
        q_high_water_bytes: 0,
        q_maximum_terminal_bytes: 0,
        store_connection_high_water: 0,
        physical_oracles: 0,
        canonical_transitions: 0,
        workspace_materializations: 0,
        rematerializations: 0,
        root_digests: Vec::with_capacity(35),
    };

    begin_failure_context("C00-001", "admission");
    let c00_started = Instant::now();
    let admission_started = Instant::now();
    verify_fixture(&fixture, &master, true)?;
    durable_write(
        &run.join("environment.json"),
        &environment_json(&source, &master),
    )?;
    durable_write(
        &run.join("master.json"),
        &fs::read_to_string(fixture.join("master.json")).map_err(io_error)?,
    )?;
    durable_write(&run.join("readiness.json"), &readiness)?;
    durable_write(&run.join("schedule.json"), &schedule_bytes)?;
    let admission_wall = admission_started.elapsed().as_nanos();
    let c00_resources = observe_row_resources(Some(&fixture), 0)?;
    let c00_wall = c00_started.elapsed().as_nanos();
    let c00_phases = vec![Phase {
        name: "admission",
        wall_ns: admission_wall,
    }];
    let custody = format!(
        concat!(
            "{{\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"fixture_blake3\":\"{}\",",
            "\"fixture_master_sha256\":\"{}\",\"readiness_sha256\":\"{}\",",
            "\"schedule_sha256\":\"{}\",\"apfs_identity\":\"{}\",",
            "\"store_id\":\"{}\"}}"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        master.fixture_blake3,
        sha256_file(&fixture.join("master.json"))?,
        sha256_bytes(readiness.as_bytes())?,
        sha256_bytes(schedule_bytes.as_bytes())?,
        json_escape(&master.apfs_identity),
        master.store_id,
    );
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C00-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c00_phases.clone(),
        phase_counters: Vec::new(),
        row_wall_ns: c00_wall,
        row_residual_ns: row_residual(c00_wall, &c00_phases)?,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources: c00_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: master.raw_digest.clone(),
            canonical_bytes_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(custody),
    })?;

    let work = run.join(".work");
    fs::create_dir(&work).map_err(io_error)?;
    let store = work.join("store");
    begin_failure_context("C01-001", "reset");
    let c01_started = Instant::now();
    let reset_started = Instant::now();
    stage1_fixture::clone_directory(&fixture.join("bases/base"), &store)?;
    stage1_fixture::make_writable(&store)?;
    let reset_wall = reset_started.elapsed().as_nanos();
    if reset_wall > RESET_LIMIT_NS {
        return Err("reset_wall_ns <= 5,000,000,000".to_owned());
    }
    let c01_resources = observe_row_resources(Some(&work), 0)?;
    let c01_wall = c01_started.elapsed().as_nanos();
    let c01_phases = vec![Phase {
        name: "reset",
        wall_ns: reset_wall,
    }];
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C01-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c01_phases.clone(),
        phase_counters: Vec::new(),
        row_wall_ns: c01_wall,
        row_residual_ns: row_residual(c01_wall, &c01_phases)?,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources: c01_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: master.raw_digest.clone(),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })?;

    begin_failure_context("C02-001", "store_open");
    let c02_started = Instant::now();
    let open_started = Instant::now();
    let opened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev)
        .map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("reset Store RefState/StoreId custody".to_owned());
    }
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    let store_open_wall = open_started.elapsed().as_nanos();
    let before_c02 = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("cold_materialization");
    let materialize_started = Instant::now();
    let (managed, materialize) = opened
        .fs
        .materialize_managed_observed(master.root)
        .map_err(display_error)?;
    let materialize_wall = materialize_started.elapsed().as_nanos();
    if materialize.workspace_materializations != 1
        || materialize.rematerializations != 0
        || materialize.operation_q_terminal_bytes != 0
    {
        return Err("initial managed materialization 1/0/Q=0".to_owned());
    }
    let initial_table = PieceTable::initial();
    set_failure_phase("live_physical_oracle");
    let oracle_started = Instant::now();
    let (initial_digest, _) =
        compare_managed(&managed, &initial_table, &schedule.replacement_backing)?;
    let oracle_wall = oracle_started.elapsed().as_nanos();
    if initial_digest != master.raw_digest {
        return Err("initial managed materialization digest".to_owned());
    }
    let initial_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    if initial_metadata.mode != FIXTURE_MODE
        || initial_metadata.mtime_seconds
            != i64::try_from(FIXTURE_MTIME_SECONDS).map_err(display_error)?
        || initial_metadata.mtime_nanoseconds != FIXTURE_MTIME_NANOSECONDS
        || !initial_metadata.xattrs.is_empty()
        || initial_metadata.acl.is_some()
        || initial_metadata.bsd_flags != 0
    {
        return Err("initial exact Apple metadata".to_owned());
    }
    set_failure_phase("counter_snapshot");
    let after_materialize = opened.fs.counter_snapshot().map_err(display_error)?;
    let after_c02 = opened.fs.diagnostics().map_err(display_error)?;
    let engine_start = Diagnostics::default();
    let c02_engine = EngineDelta::between(&engine_start, &after_c02)?;
    c02_engine.verify_trusted_read_only()?;
    let open_engine = PhaseCounterDelta::between("store_open", &engine_start, &after_open)?;
    open_engine.engine.verify_trusted_read_only()?;
    let storage_before =
        PhaseCounterDelta::between("storage_observation", &after_open, &before_c02)?;
    storage_before.engine.verify_trusted_read_only()?;
    let materialize_engine =
        PhaseCounterDelta::between("materialization", &before_c02, &after_materialize)?
            .with_operation_scratch(&materialize);
    materialize_engine.engine.verify_trusted_read_only()?;
    let storage_after =
        PhaseCounterDelta::between("storage_observation", &after_materialize, &after_c02)?;
    storage_after.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        open_engine,
        storage_before,
        materialize_engine,
        storage_after,
    ];
    verify_phase_partition(&phase_counters, c02_engine)?;
    let c02_resources = observe_row_resources(Some(&work), after_c02.active_connections)?;
    let c02_wall = c02_started.elapsed().as_nanos();
    let c02_phases = vec![
        Phase {
            name: "store_open",
            wall_ns: store_open_wall,
        },
        Phase {
            name: "cold_materialization",
            wall_ns: materialize_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: oracle_wall,
        },
    ];
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C02-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(opened.ref_state.clone()),
        post_ref: Some(opened.ref_state.clone()),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c02_phases.clone(),
        phase_counters,
        row_wall_ns: c02_wall,
        row_residual_ns: row_residual(c02_wall, &c02_phases)?,
        engine: Some(c02_engine),
        operation: Some(materialize),
        storage_before: Some(before_c02),
        storage_after: Some(after_c02),
        resources: c02_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: initial_digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })?;

    let snapshots = oracle_snapshots(&schedule)?;
    let mut roots = vec![opened.ref_state.clone()];
    let mut metadata = vec![initial_metadata];
    let mut managed = Some(managed);

    for epoch in 0..3 {
        for within in 0..5 {
            let index = epoch * 5 + within;
            run_physical_row(
                &mut campaign,
                &opened.fs,
                managed
                    .as_mut()
                    .ok_or_else(|| "managed workspace already converted".to_owned())?,
                &schedule.edits[index],
                &snapshots[index + 1],
                &mut roots,
                &mut metadata,
                &work,
            )?;
        }
        run_history_row(
            &mut campaign,
            &opened.fs,
            &store,
            &roots,
            &snapshots,
            &schedule.replacement_backing,
            u8::try_from(epoch + 1).map_err(display_error)?,
            &work,
        )?;
    }

    for epoch in 0..3 {
        for within in 0..5 {
            let index = 15 + epoch * 5 + within;
            run_logical_row(
                &mut campaign,
                &opened.fs,
                managed
                    .as_mut()
                    .ok_or_else(|| "managed workspace already converted".to_owned())?,
                &schedule.edits[index],
                &snapshots[index + 1],
                &mut roots,
                &mut metadata,
                &work,
            )?;
        }
        run_history_row(
            &mut campaign,
            &opened.fs,
            &store,
            &roots,
            &snapshots,
            &schedule.replacement_backing,
            u8::try_from(epoch + 4).map_err(display_error)?,
            &work,
        )?;
    }

    for index in 0..4 {
        run_burst_row(
            &mut campaign,
            &opened.fs,
            managed
                .as_mut()
                .ok_or_else(|| "managed workspace already converted".to_owned())?,
            &schedule.bursts[index],
            &snapshots[30 + index],
            &snapshots[31 + index],
            &mut roots,
            &mut metadata,
            &work,
        )?;
    }

    let mut converted = None;
    for root in [15_u8, 30, 34] {
        run_milestone_row(
            &mut campaign,
            &opened.fs,
            &store,
            root,
            &roots,
            &metadata,
            &snapshots,
            &schedule.replacement_backing,
            &mut managed,
            &mut converted,
            &work,
        )?;
    }

    run_terminal_row(
        &mut campaign,
        opened.fs,
        converted,
        &work,
        &fixture,
        &master,
    )?;
    if campaign.next_row != 47
        || campaign.physical_oracles != 51
        || campaign.canonical_transitions != 34
        || campaign.workspace_materializations != 1
        || campaign.rematerializations != 0
    {
        return Err(format!(
            "terminal population rows={} physical={} canonical={} materializations={} rematerializations={}",
            campaign.next_row,
            campaign.physical_oracles,
            campaign.canonical_transitions,
            campaign.workspace_materializations,
            campaign.rematerializations
        ));
    }
    campaign.rows.sync_all().map_err(io_error)?;
    begin_failure_context("__between_rows__", "report_validation");
    let disposition = finalize_reports(&mut campaign, &source, &master, &schedule)?;
    println!(
        "stage1.1-run status={} run={} rows=47 operations=51 transitions=34 wall_ns={}",
        disposition.as_str(),
        run.display(),
        campaign.started.elapsed().as_nanos()
    );
    Ok(disposition)
}

fn durable_write(path: &Path, contents: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    if let Some(parent) = path.parent() {
        stage1_fixture::sync_directory(parent)?;
    }
    Ok(())
}

fn durable_replace(path: &Path, contents: &str) -> EvalResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable replacement has no parent".to_owned())?;
    let temporary = parent.join(format!(
        ".stage1.1-rewrite-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)?;
    stage1_fixture::sync_directory(parent)
}

fn sha256_file(path: &Path) -> EvalResult<String> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("shasum failed for {}", path.display()));
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| "shasum returned no SHA-256".to_owned())
}

fn sha256_bytes(bytes: &[u8]) -> EvalResult<String> {
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    child
        .stdin
        .take()
        .ok_or_else(|| "shasum stdin unavailable".to_owned())?
        .write_all(bytes)
        .map_err(io_error)?;
    let output = child.wait_with_output().map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed for bytes".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| "shasum returned no SHA-256".to_owned())
}

fn command_output(program: &str, arguments: &[&str]) -> EvalResult<String> {
    String::from_utf8(command_bytes(program, arguments)?).map_err(display_error)
}

fn command_bytes(program: &str, arguments: &[&str]) -> EvalResult<Vec<u8>> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(stage1_fixture::workspace_root())
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} exited {}",
            arguments.join(" "),
            output.status
        ));
    }
    Ok(output.stdout)
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            value if value.is_control() => format!("\\u{:04x}", value as u32).chars().collect(),
            value => vec![value],
        })
        .collect()
}

fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON string {key}"))?;
    let mut output = String::new();
    let mut escaped = false;
    for character in json[start..].chars() {
        if escaped {
            output.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                _ => return Err(format!("unsupported JSON escape in {key}")),
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok(output);
        } else {
            output.push(character);
        }
    }
    Err(format!("unterminated JSON string {key}"))
}

fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON integer {key}"))?;
    let digits = json[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("invalid JSON integer {key}"));
    }
    digits.parse().map_err(display_error)
}

fn json_i64(json: &str, key: &str) -> EvalResult<i64> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON signed integer {key}"))?;
    let digits = json[start..]
        .chars()
        .enumerate()
        .take_while(|(index, character)| {
            character.is_ascii_digit() || (*index == 0 && *character == '-')
        })
        .map(|(_, character)| character)
        .collect::<String>();
    if digits.is_empty() || digits == "-" {
        return Err(format!("invalid JSON signed integer {key}"));
    }
    digits.parse().map_err(display_error)
}

fn json_optional_u128(json: &str, key: &str) -> EvalResult<Option<u128>> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON optional integer {key}"))?;
    if json[start..].starts_with("null") {
        Ok(None)
    } else {
        parse_digits(&json[start..], key).map(Some)
    }
}

fn json_bool(json: &str, key: &str) -> EvalResult<bool> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON boolean {key}"))?;
    if json[start..].starts_with("true") {
        Ok(true)
    } else if json[start..].starts_with("false") {
        Ok(false)
    } else {
        Err(format!("invalid JSON boolean {key}"))
    }
}

fn option_usize_json(value: Option<usize>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn option_u8_json(value: Option<u8>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn unix_ns() -> EvalResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .map_err(display_error)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_only_phase_preserves_peak_and_actual_connections() {
        let operation = layerfs_sdk::OperationDiagnostics {
            scratch_statements: 6,
            scratch_high_water_bytes: 33_304,
            ..Default::default()
        };
        let native = PhaseCounterDelta::operation_only("native_edit", &operation, 1);
        let cleanup = PhaseCounterDelta::operation_only("explicit_cleanup", &operation, 0);
        assert_eq!(native.active_connections, 1);
        assert_eq!(cleanup.active_connections, 0);
        assert_eq!(native.operation_scratch_statements, 6);
        assert_eq!(native.operation_scratch_high_water_bytes, 33_304);
        let json = phase_counter_json(&native);
        assert!(json.contains("\"active_connections\":1"));
        assert!(json.contains("\"operation_scratch_high_water_bytes\":33304"));
    }

    #[test]
    fn row_join_adds_disjoint_engine_and_vfs_scratch_but_maxes_peak() {
        let engine = EngineDelta {
            scratch_tables: 2,
            scratch_statements: 20_242,
            scratch_rows: 62_540,
            scratch_high_water_bytes: 90_000,
            ..EngineDelta::default()
        };
        let operation = layerfs_sdk::OperationDiagnostics {
            scratch_tables: 1,
            scratch_statements: 21,
            scratch_rows: 4,
            scratch_high_water_bytes: 33_304,
            ..Default::default()
        };
        assert_eq!(
            joined_scratch_counts(engine, operation).unwrap(),
            (3, 20_263, 62_544, 90_000)
        );
        let json = counters_json(Some(engine), Some(&operation)).unwrap();
        assert!(json.contains("\"scratch_tables\":3"));
        assert!(json.contains("\"scratch_statements\":20263"));
        assert!(json.contains("\"scratch_rows\":62544"));
        assert!(json.contains("\"scratch_high_water_bytes\":90000"));
    }

    fn synthetic_root(index: u8) -> RootId {
        RootId::from_bytes(blake3::hash(&[index]).as_bytes()).unwrap()
    }

    fn synthetic_ref(index: u8) -> RefState {
        RefState {
            name: "main".to_owned(),
            generation: u64::from(index) + 1,
            root: synthetic_root(index),
        }
    }

    fn synthetic_root_digest(index: u8) -> String {
        blake3::hash(&[index]).to_hex().to_string()
    }

    fn synthetic_phase(name: &'static str) -> Phase {
        Phase { name, wall_ns: 10 }
    }

    fn synthetic_metadata(root: u8) -> NativeMetadata {
        NativeMetadata {
            mode: FIXTURE_MODE,
            mtime_seconds: FIXTURE_MTIME_SECONDS as i64 + i64::from(root) + 1,
            mtime_nanoseconds: u32::from(root) + 1,
            xattrs: layerfs_sdk::NativeXattrs::new(),
            acl: None,
            bsd_flags: 0,
        }
    }

    fn synthetic_pass_row(schedule: &FrozenSchedule, scheduled: &ScheduledRow) -> RowReceipt {
        let edit = scheduled
            .edit_index
            .map(|index| schedule.edits[index].clone());
        let transition = scheduled.transition_root;
        let (before_bytes, after_bytes) = scheduled_lengths(schedule, scheduled).unwrap();
        let phases = match scheduled.row_group {
            "C00" => vec![synthetic_phase("admission")],
            "C01" => vec![synthetic_phase("reset")],
            "C02" => vec![
                synthetic_phase("store_open"),
                synthetic_phase("cold_materialization"),
                synthetic_phase("live_physical_oracle"),
            ],
            "C03" => vec![
                synthetic_phase("native_edit"),
                synthetic_phase("live_physical_oracle"),
                synthetic_phase("durable_checkpoint"),
                synthetic_phase("canonical_witness"),
                synthetic_phase("counter_snapshot"),
            ],
            "C04" | "C06" => vec![
                synthetic_phase("verified_open"),
                Phase {
                    name: "history_read",
                    wall_ns: 20,
                },
            ],
            "C05" => vec![
                synthetic_phase("direct_logical_edit"),
                synthetic_phase("changed_root_refresh"),
                synthetic_phase("live_physical_oracle"),
                synthetic_phase("canonical_witness"),
                synthetic_phase("counter_snapshot"),
            ],
            "C07" => vec![
                synthetic_phase("native_edit"),
                synthetic_phase("live_physical_oracle"),
                synthetic_phase("durable_checkpoint"),
                synthetic_phase("canonical_witness"),
                synthetic_phase("counter_snapshot"),
            ],
            "C08" => {
                let mut phases = vec![
                    synthetic_phase("verified_open"),
                    synthetic_phase("milestone_materialization"),
                    synthetic_phase("metadata_oracle"),
                    synthetic_phase("explicit_cleanup"),
                ];
                if scheduled.milestone_root == Some(34) {
                    phases.insert(0, synthetic_phase("live_physical_oracle"));
                }
                phases
            }
            "C09" => vec![synthetic_phase("explicit_cleanup")],
            _ => unreachable!(),
        };
        let row_wall_ns = phases.iter().map(|phase| phase.wall_ns).sum::<u128>();
        let mut operation = (!matches!(scheduled.row_group, "C00" | "C01"))
            .then(layerfs_sdk::OperationDiagnostics::default);
        if let Some(value) = operation.as_mut() {
            value.operation_q_current_bytes = layerfs_sdk::OPERATION_Q_BOUND_BYTES;
            value.operation_q_high_water_bytes = layerfs_sdk::OPERATION_Q_BOUND_BYTES;
            if scheduled.row_group == "C02" {
                value.workspace_materializations = 1;
                value.native.bytes_written = INITIAL_BYTES;
                value.scratch_tables = 3;
                value.scratch_statements = 55;
                value.scratch_rows = 6;
                value.scratch_high_water_bytes = 87_088;
            } else if scheduled.row_group == "C08" {
                value.scratch_tables = 1;
                value.scratch_statements = 21;
                value.scratch_rows = 4;
                value.scratch_high_water_bytes = 33_304;
            } else if scheduled.row_group == "C09" {
                value.scratch_statements = 1;
                value.scratch_derived_setup_statements = 1;
            }
            if let Some(edit) = &edit {
                value.rope.cdc_bytes_scanned = edit.insert_bytes;
                value.rope.payload_bytes_written = edit.insert_bytes;
                value.rope.nodes_read = 1;
                value.rope.nodes_created = 1;
                value.rope.tree_level_before = Some(1);
                if scheduled.row_group == "C03" {
                    value.workspace_reuses = 1;
                    value.descriptor_resets = 1;
                    value.native.route = Some(if edit.kind == EditKind::Overwrite {
                        layerfs_sdk::NativeRoute::ClonePatch
                    } else {
                        layerfs_sdk::NativeRoute::InPlaceShift
                    });
                    value.scratch_statements = 3;
                    value.scratch_rows = 3;
                    value.scratch_high_water_bytes = 33_304;
                    value.scratch_operation_statements = 3;
                } else if scheduled.row_group == "C05" {
                    value.workspace_reuses = 1;
                    value.scratch_tables = 1;
                    value.scratch_statements = 11;
                    value.scratch_rows = 6;
                    value.scratch_high_water_bytes = 33_304;
                    value.native.route = Some(if edit.kind == EditKind::Overwrite {
                        value.native.bytes_written = edit.insert_bytes;
                        value.native.patch_bytes = edit.insert_bytes;
                        value.native.clone_attempts = 1;
                        value.native.clone_successes = 1;
                        layerfs_sdk::NativeRoute::ClonePatch
                    } else {
                        let suffix = edit.before_bytes - edit.offset - edit.delete_bytes;
                        value.native.bytes_read = suffix;
                        value.native.bytes_written = suffix + edit.insert_bytes;
                        value.native.patch_bytes = edit.insert_bytes;
                        value.native.suffix_bytes_shifted = suffix;
                        value.native.clone_attempts = 1;
                        value.native.clone_successes = 1;
                        layerfs_sdk::NativeRoute::CloneShift
                    });
                }
            }
            if let Some(session) = scheduled.history_session {
                let roots = history_root_indices(session).unwrap().len() as u64;
                let probes = roots * 3;
                value.namespace.nodes_read = roots;
                value.inode_table.nodes_read = roots;
                value.rope.nodes_read = probes + roots * 2;
                value.rope.payload_bytes_read = probes * 65_536;
            }
        }
        let mut sub_edits = Vec::new();
        if let Some(index) = scheduled.burst_index {
            let burst = &schedule.bursts[index];
            let replacement = burst
                .edits
                .iter()
                .map(|edit| edit.insert_bytes)
                .sum::<u64>();
            let value = operation.as_mut().unwrap();
            value.rope.cdc_bytes_scanned = replacement;
            value.rope.payload_bytes_written = replacement;
            value.rope.nodes_read = burst.edits.len() as u64;
            value.rope.nodes_created = burst.edits.len() as u64;
            value.workspace_reuses = 1;
            value.descriptor_resets = 1;
            value.scratch_statements = burst.edits.len() as u64 * 3;
            value.scratch_rows = burst.edits.len() as u64 * 3;
            value.scratch_high_water_bytes = 33_304;
            value.scratch_operation_statements = burst.edits.len() as u64 * 3;
            for edit in &burst.edits {
                let suffix = edit.before_bytes - edit.offset - edit.delete_bytes;
                let patch = edit.kind == EditKind::Overwrite;
                value.native.bytes_read += if patch { 0 } else { suffix };
                value.native.bytes_written += if patch {
                    edit.insert_bytes
                } else {
                    suffix + edit.insert_bytes
                };
                value.native.patch_bytes += edit.insert_bytes;
                value.native.suffix_bytes_shifted += if patch { 0 } else { suffix };
                value.native.clone_attempts += u64::from(patch);
                value.native.clone_successes += u64::from(patch);
                sub_edits.push(SubEditReceipt {
                    edit: edit.clone(),
                    native_wall_ns: 10,
                    physical_oracle_wall_ns: 10,
                    native_route: if edit.kind == EditKind::Overwrite {
                        "ClonePatch".to_owned()
                    } else {
                        "InPlaceShift".to_owned()
                    },
                    native_bytes_read: if patch { 0 } else { suffix },
                    native_bytes_written: if patch {
                        edit.insert_bytes
                    } else {
                        suffix + edit.insert_bytes
                    },
                    native_patch_bytes: edit.insert_bytes,
                    native_suffix_bytes_shifted: if patch { 0 } else { suffix },
                    native_clone_attempts: u64::from(patch),
                    native_clone_successes: u64::from(patch),
                    native_clone_fallbacks: 0,
                    native_full_fallback_files: 0,
                    tree_level_before: Some(1),
                    locality: Some(ContentCounters {
                        cdc_bytes_scanned: edit.insert_bytes,
                        payload_bytes_written: edit.insert_bytes,
                        rope_nodes_read: 1,
                        rope_nodes_emitted: 1,
                        ..ContentCounters::default()
                    }),
                });
            }
        }
        let engine = operation.as_ref().map(|_| {
            if let Some(session) = scheduled.history_session {
                let roots = history_root_indices(session).unwrap().len() as u64;
                let probes = roots * 3;
                let fetched = probes + roots * 4;
                EngineDelta {
                    statements: fetched,
                    objects_validated: fetched,
                    fetched_rows: fetched,
                    fetched_row_authentication_passes: fetched,
                    fetched_row_role_decode_passes: fetched,
                    payload_batch_queries: probes,
                    payload_batch_references: probes,
                    payload_batch_maximum: 1,
                    retained_union_scrubs: 1,
                    scratch_tables: 2,
                    scratch_statements: 2,
                    scratch_rows: 2,
                    scratch_high_water_bytes: 4_096,
                    ..EngineDelta::default()
                }
            } else if scheduled.row_id == "C08-001" {
                EngineDelta {
                    retained_union_scrubs: 1,
                    scratch_tables: 2,
                    scratch_statements: 2,
                    scratch_rows: 2,
                    scratch_high_water_bytes: 4_096,
                    ..EngineDelta::default()
                }
            } else if transition.is_some() {
                EngineDelta {
                    transactions_started: 1,
                    transactions_committed: 1,
                    publication_transactions_started: 1,
                    publication_commits: 1,
                    ..EngineDelta::default()
                }
            } else {
                EngineDelta::default()
            }
        });
        let serial = scheduled.row_index as u64;
        let storage = operation
            .as_ref()
            .filter(|_| scheduled.row_group != "C09")
            .map(|_| {
                let before = Diagnostics {
                    database_bytes: Some(1_000_000 + serial * 4_096),
                    logical_engine_bytes: Some(900_000 + serial * 2_048),
                    object_bytes_written: serial * 1_024,
                    ..Diagnostics::default()
                };
                let delta = u64::from(transition.is_some()) * 4_096;
                let after = Diagnostics {
                    database_bytes: before.database_bytes.map(|value| value + delta),
                    logical_engine_bytes: before
                        .logical_engine_bytes
                        .map(|value| value + delta / 2),
                    object_bytes_written: before.object_bytes_written
                        + transition.map_or(0, |_| {
                            edit.as_ref().map_or(4_096, |edit| edit.insert_bytes.max(1))
                        }),
                    ..Diagnostics::default()
                };
                (before, after)
            });
        let active_store_connections = match scheduled.row_group {
            "C00" | "C01" | "C09" => 0,
            "C04" | "C06" | "C08" => 2,
            _ => 1,
        };
        let resources = ResourceObservation {
            rss_current_bytes: Some(20_000_000),
            rss_peak_bytes: 20_000_000,
            fd_current: 5,
            active_store_connections,
            child_processes: 0,
            owned_temp_entries: (scheduled.row_group == "C09").then_some(0),
            residue_entries: 0,
        };
        let pre_ref = transition.map(|root| synthetic_ref(root - 1)).or_else(|| {
            scheduled
                .milestone_root
                .map(synthetic_ref)
                .or_else(|| (scheduled.row_group == "C02").then(|| synthetic_ref(0)))
        });
        let post_ref = transition.map(synthetic_ref).or_else(|| pre_ref.clone());
        let native_route = if scheduled.row_group == "C05" {
            if edit
                .as_ref()
                .is_some_and(|edit| edit.kind == EditKind::Overwrite)
            {
                "ClonePatch"
            } else {
                "CloneShift"
            }
        } else if scheduled.row_group == "C03" {
            if edit
                .as_ref()
                .is_some_and(|edit| edit.kind == EditKind::Overwrite)
            {
                "ClonePatch"
            } else {
                "InPlaceShift"
            }
        } else {
            "NotApplicable"
        };
        let phase_names: &[&str] = match scheduled.row_group {
            "C02" => &[
                "store_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C03" | "C07" => &[
                "native_edit",
                "checkpoint",
                "canonical_witness",
                "storage_observation",
            ],
            "C04" | "C06" => &[
                "verified_open",
                "storage_observation",
                "history_read",
                "storage_observation",
            ],
            "C05" => &[
                "logical_edit",
                "apfs_refresh",
                "canonical_witness",
                "storage_observation",
            ],
            "C08" => &[
                "verified_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C09" => &["explicit_cleanup"],
            _ => &[],
        };
        let phase_counters = engine.map_or_else(Vec::new, |engine| {
            phase_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let phase_engine = if scheduled.history_session.is_some() {
                        if *name == "verified_open" {
                            EngineDelta {
                                retained_union_scrubs: 1,
                                scratch_tables: 2,
                                scratch_statements: 2,
                                scratch_rows: 2,
                                scratch_high_water_bytes: 4_096,
                                ..EngineDelta::default()
                            }
                        } else if *name == "history_read" {
                            EngineDelta {
                                retained_union_scrubs: 0,
                                scratch_tables: 0,
                                scratch_statements: 0,
                                scratch_rows: 0,
                                scratch_high_water_bytes: 0,
                                ..engine
                            }
                        } else {
                            EngineDelta::default()
                        }
                    } else if index == usize::from(matches!(scheduled.row_group, "C03" | "C07")) {
                        engine
                    } else {
                        EngineDelta::default()
                    };
                    let operation_scratch_owner = matches!(
                        (scheduled.row_group, *name),
                        ("C02" | "C08", "materialization")
                            | ("C03" | "C07", "native_edit")
                            | ("C05", "apfs_refresh")
                            | ("C09", "explicit_cleanup")
                    );
                    let operation_scratch = operation.as_ref().filter(|_| operation_scratch_owner);
                    PhaseCounterDelta {
                        name,
                        engine: phase_engine,
                        q_before_bytes: 0,
                        q_after_bytes: 0,
                        q_high_water_bytes: layerfs_sdk::OPERATION_Q_BOUND_BYTES,
                        active_connections: active_store_connections,
                        operation_scratch_tables: operation_scratch
                            .map_or(0, |operation| operation.scratch_tables),
                        operation_scratch_statements: operation_scratch
                            .map_or(0, |operation| operation.scratch_statements),
                        operation_scratch_rows: operation_scratch
                            .map_or(0, |operation| operation.scratch_rows),
                        operation_scratch_high_water_bytes: operation_scratch
                            .map_or(0, |operation| operation.scratch_high_water_bytes),
                    }
                })
                .collect()
        });
        let snapshots = oracle_snapshots(schedule).unwrap();
        let history_probes = scheduled.history_session.map_or_else(Vec::new, |session| {
            history_root_indices(session)
                .unwrap()
                .iter()
                .flat_map(|root_index| {
                    let logical_length = snapshots[*root_index].logical_length;
                    (1..=3).map(move |ordinal| {
                        let first = ordinal == 1;
                        let fetched = 1 + u64::from(first) * 4;
                        let mut operation = layerfs_sdk::OperationDiagnostics::default();
                        operation.namespace.nodes_read = u64::from(first);
                        operation.inode_table.nodes_read = u64::from(first);
                        operation.rope.nodes_read = 1 + u64::from(first) * 2;
                        operation.rope.payload_bytes_read = 65_536;
                        let start = match ordinal {
                            1 => 0,
                            2 => logical_length / 2 - 32_768,
                            3 => logical_length - 65_536,
                            _ => unreachable!(),
                        };
                        HistoryProbeReceipt {
                            root_index: *root_index,
                            ordinal,
                            start,
                            length: 65_536,
                            wall_ns: 1,
                            engine: EngineDelta {
                                statements: fetched,
                                objects_validated: fetched,
                                fetched_rows: fetched,
                                fetched_row_authentication_passes: fetched,
                                fetched_row_role_decode_passes: fetched,
                                payload_batch_queries: 1,
                                payload_batch_references: 1,
                                payload_batch_maximum: 1,
                                ..EngineDelta::default()
                            },
                            operation,
                        }
                    })
                })
                .collect()
        });
        RowReceipt {
            schedule: scheduled.clone(),
            status: "PASS",
            before_bytes,
            after_bytes,
            edit,
            sub_edits,
            history_probes,
            pre_ref,
            post_ref,
            native_route: native_route.to_owned(),
            tree_level_before: matches!(scheduled.row_group, "C03" | "C05").then_some(1),
            phases,
            phase_counters,
            row_wall_ns,
            row_residual_ns: 0,
            engine,
            operation,
            storage_before: storage.as_ref().map(|value| value.0),
            storage_after: storage.as_ref().map(|value| value.1),
            resources,
            oracle: OracleReceipt {
                logical_length: after_bytes,
                content_digest: if scheduled.row_group == "C09" {
                    String::new()
                } else {
                    synthetic_root_digest(
                        scheduled
                            .transition_root
                            .or(scheduled.milestone_root)
                            .or_else(|| scheduled.history_session.map(|session| session * 5))
                            .unwrap_or(if scheduled.row_group == "C02" { 0 } else { 34 }),
                    )
                },
                physical_bytes_exact: matches!(scheduled.row_group, "C03" | "C05" | "C07" | "C08").then_some(true),
                canonical_bytes_exact: matches!(scheduled.row_group, "C02" | "C03" | "C05" | "C07" | "C08").then_some(true),
                metadata_exact: matches!(scheduled.row_group, "C02" | "C03" | "C05" | "C07" | "C08").then_some(true),
                historical_roots_exact: matches!(scheduled.row_group, "C04" | "C06" | "C08").then_some(true),
                route_exact: (scheduled.row_group != "C09").then_some(true),
            },
            unavailable: unavailable_defaults(),
            error: None,
            custody: match scheduled.row_group {
                "C04" | "C06" => Some(
                    history_custody_json(scheduled.history_session.unwrap()).unwrap(),
                ),
                "C08" => {
                    let root = scheduled.milestone_root.unwrap();
                    let metadata = metadata_receipt_json(&synthetic_metadata(root));
                    Some(format!(
                        concat!(
                            "{{\"milestone_root\":\"R{}\",\"extra_user_files\":0,",
                            "\"fresh_extra_user_files\":0,\"live_extra_user_files\":{},",
                            "\"cleanup_residue_entries\":0,\"metadata\":{},",
                            "\"retained_metadata\":{},\"fresh_metadata\":{},",
                            "\"live_metadata\":{}}}"
                        ),
                        root,
                        if root == 34 { "0" } else { "null" },
                        metadata,
                        metadata,
                        metadata,
                        if root == 34 { metadata.as_str() } else { "null" },
                    ))
                }
                "C09" => Some("{\"pre_cleanup_active_store_connections\":0,\"pre_cleanup_fd_count\":5,\"pre_cleanup_child_processes\":0,\"pre_cleanup_residue_entries\":0,\"post_cleanup_active_store_connections\":0,\"post_cleanup_fd_count\":5,\"post_cleanup_child_processes\":0,\"post_cleanup_residue_entries\":0,\"fixture_unchanged\":true}".to_owned()),
                _ => None,
            },
        }
    }

    #[test]
    fn schedule_and_piece_table_close_the_frozen_population() {
        let schedule = frozen_schedule().unwrap();
        let snapshots = oracle_snapshots(&schedule).unwrap();
        assert_eq!(schedule.rows.len(), 47);
        assert_eq!(schedule.edits.len(), 51);
        assert_eq!(snapshots.len(), 35);
        assert_eq!(snapshots[15].logical_length, INITIAL_BYTES);
        assert_eq!(snapshots[30].logical_length, INITIAL_BYTES);
        assert_eq!(snapshots[34].logical_length, INITIAL_BYTES);
        assert_eq!(schedule.replacement_backing.len(), 495_616);
        assert_eq!(
            schedule.edits.iter().map(|edit| edit.after_bytes).max(),
            Some(MAXIMUM_BYTES)
        );
    }

    #[test]
    fn piece_table_matches_a_reduced_vec_after_every_splice() {
        let mut table = PieceTable {
            pieces: vec![Piece::Inserted {
                offset: 0,
                length: 32,
            }],
            logical_length: 32,
        };
        let mut backing = (0_u8..64).collect::<Vec<_>>();
        let mut expected = backing[..32].to_vec();
        let edits = [
            ("t1", 4, 3, 5, 32, 34, 32),
            ("t2", 0, 0, 2, 34, 36, 37),
            ("t3", 30, 6, 0, 36, 30, 39),
        ];
        for (serial, &(tag, offset, delete, insert, before, after, replacement_offset)) in
            edits.iter().enumerate()
        {
            let edit = EditSpec {
                tag: tag.to_owned(),
                serial: serial as u8,
                epoch: 0,
                kind: EditKind::Overwrite,
                size_band: "test",
                offset,
                delete_bytes: delete,
                insert_bytes: insert,
                before_bytes: before,
                after_bytes: after,
                replacement_offset,
            };
            table.splice(&edit).unwrap();
            let replacement = backing
                [replacement_offset..replacement_offset + usize::try_from(insert).unwrap()]
                .to_vec();
            expected.splice(
                usize::try_from(offset).unwrap()..usize::try_from(offset + delete).unwrap(),
                replacement,
            );
            let mut actual = Vec::new();
            table.stream(&backing, &mut actual).unwrap();
            assert_eq!(actual, expected);
        }
        backing.clear();
    }

    #[test]
    fn piece_cursor_generates_each_original_mebibyte_once() {
        let table = PieceTable {
            pieces: vec![Piece::Original {
                offset: 0,
                length: BUFFER_BYTES as u64 + 1,
            }],
            logical_length: BUFFER_BYTES as u64 + 1,
        };
        let mut cursor = PieceCursor::new(&table, &[]);
        let mut chunk = vec![0_u8; 4_096];
        let mut expected = vec![0_u8; BUFFER_BYTES];
        stage1_fixture::fill_retained_buffer(&mut expected, 0);
        for index in 0..BUFFER_BYTES / chunk.len() {
            cursor.read_exact_expected(&mut chunk).unwrap();
            assert_eq!(
                chunk,
                expected[index * chunk.len()..(index + 1) * chunk.len()]
            );
        }
        assert_eq!(cursor.original_blocks_generated, 1);
        cursor.read_exact_expected(&mut chunk[..1]).unwrap();
        stage1_fixture::fill_retained_buffer(&mut expected, BUFFER_BYTES as u64);
        assert_eq!(chunk[0], expected[0]);
        assert_eq!(cursor.original_blocks_generated, 2);
        cursor.finish().unwrap();
    }

    #[test]
    fn schedule_json_retains_every_edit_and_row_in_execution_order() {
        let schedule = frozen_schedule().unwrap();
        let json = schedule_json(&schedule).unwrap();
        assert_eq!(json.matches("\"row_id\":").count(), 47);
        assert_eq!(json.matches("\"tag\":").count(), 51);
        assert!(json.find("C03-005").unwrap() < json.find("C04-001").unwrap());
        assert!(json.find("C04-001").unwrap() < json.find("C03-006").unwrap());
        assert_eq!(json.matches("\"pre_ref_slot\":\"R").count(), 34);
        assert_eq!(json.matches("\"post_ref_slot\":\"R").count(), 34);
        assert!(json.contains("\"pre_ref_slot\":\"R0\",\"post_ref_slot\":\"R1\""));
        assert!(json.contains("\"pre_ref_slot\":\"R33\",\"post_ref_slot\":\"R34\""));
    }

    #[test]
    #[ignore = "full 24 MiB x 51 exact differential proof; run once at source closure"]
    fn all_51_exact_edits_match_the_independent_vec_digest_after_every_operation() {
        let schedule = frozen_schedule().unwrap();
        let mut expected = Vec::with_capacity(MAXIMUM_BYTES as usize);
        let mut buffer = vec![0_u8; BUFFER_BYTES];
        let mut offset = 0_u64;
        while offset < INITIAL_BYTES {
            stage1_fixture::fill_retained_buffer(&mut buffer, offset);
            let take = usize::try_from((INITIAL_BYTES - offset).min(BUFFER_BYTES as u64)).unwrap();
            expected.extend_from_slice(&buffer[..take]);
            offset += take as u64;
        }
        let mut table = PieceTable::initial();
        for edit in &schedule.edits {
            let start = usize::try_from(edit.offset).unwrap();
            let end = usize::try_from(edit.offset + edit.delete_bytes).unwrap();
            let replacement_end =
                edit.replacement_offset + usize::try_from(edit.insert_bytes).unwrap();
            expected.splice(
                start..end,
                schedule.replacement_backing[edit.replacement_offset..replacement_end]
                    .iter()
                    .copied(),
            );
            table.splice(edit).unwrap();
            assert_eq!(expected.len() as u64, edit.after_bytes, "{}", edit.tag);
            let mut comparison = PieceCursor::new(&table, &schedule.replacement_backing);
            let mut actual = vec![0_u8; BUFFER_BYTES];
            for chunk in expected.chunks(BUFFER_BYTES) {
                comparison
                    .read_exact_expected(&mut actual[..chunk.len()])
                    .unwrap();
                assert_eq!(&actual[..chunk.len()], chunk, "{}", edit.tag);
            }
            comparison.finish().unwrap();
        }
    }

    #[test]
    fn row_contract_is_valid_json_and_retains_null_unavailable_observations() {
        let schedule = frozen_schedule().unwrap();
        let row = RowReceipt {
            schedule: schedule.rows[0].clone(),
            status: "PASS",
            before_bytes: INITIAL_BYTES,
            after_bytes: INITIAL_BYTES,
            edit: None,
            sub_edits: Vec::new(),
            history_probes: Vec::new(),
            pre_ref: None,
            post_ref: None,
            native_route: "NotApplicable".to_owned(),
            tree_level_before: None,
            phases: Vec::new(),
            phase_counters: Vec::new(),
            row_wall_ns: 0,
            row_residual_ns: 0,
            engine: None,
            operation: None,
            storage_before: None,
            storage_after: None,
            resources: ResourceObservation::default(),
            oracle: OracleReceipt::default(),
            unavailable: unavailable_defaults(),
            error: None,
            custody: None,
        }
        .json()
        .unwrap();
        assert!(row.contains("\"rollback_journal_bytes\":null"));
        assert!(row.contains("\"availability\":\"Unavailable\""));
        assert!(row.contains("\"sync_regular_calls\":null"));
        assert!(row.contains("\"transactions_started\":null"));
        assert!(row.contains("\"availability\":\"NotApplicable\""));
        assert!(row.contains("\"field\":\"oracle.physical_bytes_exact\""));
        let mut child = Command::new("/usr/bin/ruby")
            .args(["-rjson", "-e", "JSON.parse(STDIN.read)"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(row.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "row={} stdout={} stderr={}",
            row,
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn all_47_common_rows_round_trip_in_frozen_order() {
        let schedule = frozen_schedule().unwrap();
        let path = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-row-contract-{}-{}.jsonl",
            std::process::id(),
            unix_ns().unwrap()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        for scheduled in &schedule.rows {
            file.write_all(
                synthetic_pass_row(&schedule, scheduled)
                    .json()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        }
        drop(file);
        let parsed = parse_rows(&path, &schedule).unwrap();
        assert_eq!(parsed.len(), 47);
        assert_eq!(parsed[8].row_id, "C04-001");
        assert_eq!(parsed[9].row_id, "C03-006");
        let contents = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            contents.replacen("\"row_group\":\"C00\"", "\"row_group\":\"C01\"", 1),
        )
        .unwrap();
        assert!(parse_rows(&path, &schedule).is_err());
        let c07 = contents
            .lines()
            .find(|line| line.contains("\"row_id\":\"C07-001\""))
            .unwrap();
        for key in [
            "before_bytes",
            "after_bytes",
            "native_route",
            "tree_level_before",
        ] {
            let value = json_top_level_value(c07, key).unwrap();
            let value_offset = c07.len() - value.len();
            let key_start = c07[..value_offset].rfind(&format!("\"{key}\":")).unwrap() + 1;
            let mut mutated = c07.to_owned();
            mutated.replace_range(key_start..key_start + key.len(), &format!("removed_{key}"));
            fs::write(&path, contents.replacen(c07, &mutated, 1)).unwrap();
            assert!(parse_rows(&path, &schedule).is_err(), "top-level {key}");
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn nearest_rank_statistics_retain_raw_and_sorted_arrays() {
        for (n, p50, p95) in [
            (3, 2, 3),
            (4, 2, 4),
            (5, 3, 5),
            (6, 3, 6),
            (12, 6, 12),
            (15, 8, 15),
            (19, 10, 19),
            (51, 26, 49),
        ] {
            let raw = (1..=n).rev().map(|value| value as u128).collect::<Vec<_>>();
            let stats = statistics(raw.clone()).unwrap();
            assert_eq!(stats.raw_ns, raw);
            assert_eq!(
                stats.sorted_ns,
                (1..=n).map(|value| value as u128).collect::<Vec<_>>()
            );
            assert_eq!(stats.p50_ns, p50);
            assert_eq!(stats.p95_ns, p95);
        }
    }

    #[test]
    fn report_heading_and_campaign_timer_contracts_are_exact() {
        let markdown = SUMMARY_HEADINGS.join("\n\n");
        validate_summary_headings(&markdown).unwrap();
        let timer = concat!(
            "schema=layerfs-stage1.1-campaign-time-v1\n",
            "status=PASS\n",
            "started_unix_ns=1\n",
            "completed_unix_ns=11\n",
            "complete_wall_ns=10\n",
            "row_wall_sum_ns=6\n",
            "outside_rows_wall_ns=4\n",
            "timer_residual_ns=0\n",
            "hard_limit_ns=60000000000\n",
            "rows_expected=47\n",
            "rows_valid=47\n",
            "edit_suboperations_expected=51\n",
            "edit_suboperations_observed=51\n",
            "transitions_expected=34\n",
            "transitions_observed=34\n",
        );
        validate_campaign_time(timer).unwrap();
        assert!(validate_campaign_time(
            &timer.replace("outside_rows_wall_ns=4", "outside_rows_wall_ns=5")
        )
        .is_err());
    }

    #[test]
    fn hard_gate_failures_cannot_be_promoted() {
        let row = |status: &str| ParsedRow {
            json: String::new(),
            row_id: "test".to_owned(),
            row_group: "C00".to_owned(),
            operation: "admission".to_owned(),
            size_band: "not-applicable".to_owned(),
            native_route: "NotApplicable".to_owned(),
            status: status.to_owned(),
            before_bytes: INITIAL_BYTES,
            after_bytes: INITIAL_BYTES,
            row_wall_ns: 0,
            row_residual_ns: 0,
        };
        assert_eq!(
            derive_disposition(&[row("FAIL"), row("REVISE")]),
            Disposition::Fail
        );
        assert_eq!(derive_disposition(&[row("REVISE")]), Disposition::Revise);
        assert_eq!(derive_disposition(&[row("PASS")]), Disposition::Pass);
    }

    #[test]
    fn failed_rows_and_failure_reports_are_schema_valid_and_append_only() {
        let run = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-failure-contract-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
        fs::create_dir(&run).unwrap();
        File::create(run.join("rows.jsonl")).unwrap();
        durable_write(&run.join("stderr.txt"), "first equation\n").unwrap();
        begin_failure_context("C00-001", "admission");
        set_failure_phase("fixture_custody");
        append_failed_row(&run, "first equation", &run.join("stderr.txt")).unwrap();
        let rows = fs::read_to_string(run.join("rows.jsonl")).unwrap();
        let complete_wall_ns = json_u128(&rows, "row_wall_ns").unwrap() + 10;
        write_failure_artifacts(&run, "first equation", 1, complete_wall_ns).unwrap();
        assert_eq!(rows.lines().count(), 1);
        assert_eq!(json_string(&rows, "status").unwrap(), "FAIL");
        assert_eq!(
            json_string(&rows, "first_failed_equation").unwrap(),
            "first equation"
        );
        assert_eq!(json_string(&rows, "phase").unwrap(), "fixture_custody");
        let summary = fs::read_to_string(run.join("summary.json")).unwrap();
        assert_eq!(json_string(&summary, "status").unwrap(), "FAIL");
        let timer = fs::read_to_string(run.join("campaign-time.txt")).unwrap();
        validate_timer_equation(&timer).unwrap();
        assert!(validate_timer_equation(&timer.replacen(
            "outside_rows_wall_ns=10",
            "outside_rows_wall_ns=11",
            1
        ))
        .is_err());
        let markdown = fs::read_to_string(run.join("summary.md")).unwrap();
        validate_summary_headings(&markdown).unwrap();
        fs::remove_dir_all(run).unwrap();
    }

    #[test]
    fn between_rows_budget_failure_does_not_fabricate_the_next_row() {
        let run = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-between-rows-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
        fs::create_dir(&run).unwrap();
        let schedule = frozen_schedule().unwrap();
        fs::write(
            run.join("rows.jsonl"),
            synthetic_pass_row(&schedule, &schedule.rows[0])
                .json()
                .unwrap(),
        )
        .unwrap();
        durable_write(&run.join("stderr.txt"), "time budget\n").unwrap();
        begin_failure_context("__between_rows__", "time_budget");
        append_failed_row(&run, "time budget", &run.join("stderr.txt")).unwrap();
        assert_eq!(
            fs::read_to_string(run.join("rows.jsonl"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert!(enforce_campaign_limit(Instant::now()).is_ok());
        assert!(enforce_campaign_limit(Instant::now() - Duration::from_secs(61)).is_err());
        fs::remove_dir_all(run).unwrap();
    }

    #[test]
    fn storage_observation_is_a_disjoint_phase_counter_owner() {
        let before = Diagnostics::default();
        let product = Diagnostics {
            statements: 5,
            primary_read_statements: 5,
            ..Diagnostics::default()
        };
        let after = Diagnostics {
            statements: 8,
            primary_read_statements: 8,
            ..Diagnostics::default()
        };
        let product_phase = PhaseCounterDelta::between("product", &before, &product).unwrap();
        let storage_phase =
            PhaseCounterDelta::between("storage_observation", &product, &after).unwrap();
        assert_eq!(storage_phase.engine.statements, 3);
        assert!(verify_phase_partition(
            &[product_phase, storage_phase],
            EngineDelta::between(&before, &after).unwrap()
        )
        .is_ok());
        assert!(verify_phase_partition(
            &[product_phase],
            EngineDelta::between(&before, &after).unwrap()
        )
        .is_err());
    }

    #[test]
    fn generated_pass_summary_is_valid_row_derived_and_rejects_resource_mutation() {
        let run = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-summary-contract-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
        fs::create_dir(&run).unwrap();
        for name in [
            "environment.json",
            "master.json",
            "readiness.json",
            "schedule.json",
            "campaign-time.txt",
        ] {
            durable_write(&run.join(name), "{}\n").unwrap();
        }
        let schedule = frozen_schedule().unwrap();
        let mut rows_file = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(run.join("rows.jsonl"))
            .unwrap();
        let mut row_wall_sum_ns = 0_u128;
        for scheduled in &schedule.rows {
            let row = synthetic_pass_row(&schedule, scheduled);
            row_wall_sum_ns += row.row_wall_ns;
            rows_file.write_all(row.json().unwrap().as_bytes()).unwrap();
        }
        rows_file.sync_all().unwrap();
        let rows = parse_rows(&run.join("rows.jsonl"), &schedule).unwrap();
        let executable_path = std::env::current_exe().unwrap();
        let source = SourceIdentity {
            git_commit: "0".repeat(40),
            dirty_tree: true,
            tree_blake3: "1".repeat(64),
            manifest_sha256: "2".repeat(64),
            executable_path,
            executable_sha256: "3".repeat(64),
            executable_blake3: "4".repeat(64),
        };
        let master = FixtureMaster {
            raw_digest: "5".repeat(64),
            root: synthetic_root(0),
            generation: 1,
            store_id: "6".repeat(64),
            profile: "page=4096;cache=1280;spill=1280;DELETE/FULL/FILE/mmap=0".to_owned(),
            apfs_identity: "synthetic-apfs".to_owned(),
            fixture_blake3: "7".repeat(64),
            preparation_wall_ns: 1,
        };
        let campaign = Campaign {
            run: &run,
            started: Instant::now(),
            started_unix_ns: 1,
            rows: rows_file,
            schedule: &schedule,
            next_row: 47,
            row_wall_sum_ns,
            fd_baseline: 5,
            rss_peak_bytes: 20_000_000,
            q_high_water_bytes: layerfs_sdk::OPERATION_Q_BOUND_BYTES,
            q_maximum_terminal_bytes: 0,
            store_connection_high_water: 2,
            physical_oracles: 51,
            canonical_transitions: 34,
            workspace_materializations: 1,
            rematerializations: 0,
            root_digests: (0..35).map(synthetic_root_digest).collect(),
        };
        let complete = row_wall_sum_ns + 1_000;
        let summary = summary_json(
            &campaign,
            &rows,
            &source,
            &master,
            complete,
            &"8".repeat(64),
        )
        .unwrap();
        let optimized_r34 = json_object(
            json_object(&summary, "optimization").unwrap(),
            "verified_open_by_root",
        )
        .and_then(|roots| json_object(roots, "R34"))
        .unwrap();
        assert_eq!(
            json_u128(optimized_r34, "before_ns").unwrap(),
            1_406_344_708
        );
        assert_eq!(
            json_u128(optimized_r34, "after_ns").unwrap(),
            phase_wall(
                &rows
                    .iter()
                    .find(|row| row.row_id == "C08-001")
                    .unwrap()
                    .json,
                "verified_open"
            )
            .unwrap()
        );
        let failures = json_array_objects(&summary, "failures").unwrap();
        for (attempt, field) in [
            ("attempt-010", "optimization.verified_open_by_root.R34"),
            ("attempt-011", "tests.eof_post_visibility_conflict"),
        ] {
            let receipt = failures
                .iter()
                .find(|receipt| {
                    json_string(receipt, "artifact").is_ok_and(|path| path.ends_with(attempt))
                })
                .unwrap();
            assert_eq!(json_string(receipt, "field").unwrap(), field);
            assert!(!json_string(receipt, "reason").unwrap().is_empty());
        }
        assert!(!summary.contains("\"count_change_amplification\":{}"));
        assert!(!summary.contains("\"by_root\":{}"));
        assert!(!summary.contains("\"by_root_range\":{}"));
        assert!(validate_summary_json_contract(&summary.replacen(
            "\"source\":",
            "\"source_missing\":",
            1
        ))
        .is_err());
        let mut child = Command::new("/usr/bin/ruby")
            .args(["-rjson", "-e", "JSON.parse(STDIN.read)"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(summary.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        let markdown = summary_markdown(&campaign, &rows, &source, &master, complete).unwrap();
        validate_summary_headings(&markdown).unwrap();
        validate_summary_pair(&summary, &markdown).unwrap();
        assert!(markdown.contains(&format!(
            "| Complete wall | `PASS` | `{} ms < 60 s` |",
            format_ms(complete)
        )));
        assert!(markdown.contains(&format!("mtime=`{}.", FIXTURE_MTIME_SECONDS + 35)));
        assert!(validate_named_wall_equation(&summary.replacen(
            "\"admission\":",
            "\"admission\":1",
            1
        ))
        .is_err());

        let mut mutated = rows.clone();
        mutated[0].json = mutated[0].json.replacen(
            "\"rss_peak_bytes\":20000000",
            "\"rss_peak_bytes\":40000000",
            1,
        );
        assert!(summary_json(
            &campaign,
            &mutated,
            &source,
            &master,
            complete,
            &"8".repeat(64)
        )
        .is_err());

        let mut bad_r34_scrub = rows.clone();
        let r34_scrub = bad_r34_scrub
            .iter_mut()
            .find(|row| row.row_id == "C08-001")
            .unwrap();
        r34_scrub.json = r34_scrub.json.replacen(
            "\"name\":\"verified_open\",\"transactions_started\":0",
            "\"name\":\"verified_open\",\"retained_union_scrubs\":0,\"transactions_started\":0",
            1,
        );
        assert!(summary_json(
            &campaign,
            &bad_r34_scrub,
            &source,
            &master,
            complete,
            &"8".repeat(64)
        )
        .is_err());

        let transition = rows.iter().position(|row| row.row_id == "C03-001").unwrap();
        let mut bad_ref = rows.clone();
        bad_ref[transition].json = bad_ref[transition].json.replacen(
            "\"pre_ref\":{\"name\":\"main\",\"generation\":1",
            "\"pre_ref\":{\"name\":\"main\",\"generation\":2",
            1,
        );
        assert!(validate_ref_chain(&bad_ref, &schedule).is_err());

        let mut bad_authentication = rows.clone();
        let counters = json_object(&bad_authentication[transition].json, "counters")
            .unwrap()
            .to_owned();
        let mutated = counters.replacen(
            "\"fetched_row_authentication_passes\":0",
            "\"fetched_row_authentication_passes\":1",
            1,
        );
        bad_authentication[transition].json = bad_authentication[transition]
            .json
            .replacen(&counters, &mutated, 1);
        assert!(validate_authentication(&bad_authentication).is_err());
        let mut bad_insert_equation = rows.clone();
        let counters = json_object(&bad_insert_equation[transition].json, "counters")
            .unwrap()
            .to_owned();
        let mutated = counters.replacen(
            "\"put_insert_statements\":0",
            "\"put_insert_statements\":1",
            1,
        );
        bad_insert_equation[transition].json = bad_insert_equation[transition]
            .json
            .replacen(&counters, &mutated, 1);
        assert!(validate_authentication(&bad_insert_equation).is_err());

        let mut bad_phase_partition = rows.clone();
        bad_phase_partition[transition].json = bad_phase_partition[transition].json.replacen(
            "\"name\":\"checkpoint\",\"transactions_started\":1",
            "\"name\":\"checkpoint\",\"transactions_started\":2",
            1,
        );
        assert!(validate_phase_counter_rows(&bad_phase_partition).is_err());
        let history_row = rows.iter().position(|row| row.row_id == "C04-001").unwrap();
        let mut bad_retained_root_equation = rows.clone();
        let verified_phase = json_array_objects(
            &bad_retained_root_equation[history_row].json,
            "phase_counters",
        )
        .unwrap()[0]
            .to_owned();
        let mutated_phase = verified_phase.replacen(
            "\"retained_roots_validated\":0",
            "\"retained_roots_validated\":1",
            1,
        );
        bad_retained_root_equation[history_row].json = bad_retained_root_equation[history_row]
            .json
            .replacen(&verified_phase, &mutated_phase, 1);
        let counters = json_object(&bad_retained_root_equation[history_row].json, "counters")
            .unwrap()
            .to_owned();
        let mutated_counters = counters.replacen(
            "\"retained_roots_validated\":0",
            "\"retained_roots_validated\":1",
            1,
        );
        bad_retained_root_equation[history_row].json = bad_retained_root_equation[history_row]
            .json
            .replacen(&counters, &mutated_counters, 1);
        assert!(validate_phase_counter_rows(&bad_retained_root_equation).is_err());
        let c02 = rows.iter().position(|row| row.row_id == "C02-001").unwrap();
        let mut bad_operation_scratch = rows.clone();
        bad_operation_scratch[c02].json = bad_operation_scratch[c02].json.replacen(
            "\"operation_scratch_tables\":3",
            "\"operation_scratch_tables\":2",
            1,
        );
        assert!(validate_phase_counter_rows(&bad_operation_scratch).is_err());

        let mut bad_availability = rows.clone();
        bad_availability[0].json = bad_availability[0].json.replacen(
            "{\"field\":\"counters.transactions_started\",\"availability\":\"NotApplicable\",\"reason\":\"row has no product operation\"},",
            "",
            1,
        );
        assert!(validate_availability_rows(&bad_availability).is_err());
        let mut bad_tree_availability = rows.clone();
        let record = json_array_objects(&bad_tree_availability[0].json, "unavailable")
            .unwrap()
            .into_iter()
            .find(|record| json_string(record, "field").as_deref() == Ok("tree_level_before"))
            .unwrap()
            .to_owned();
        bad_tree_availability[0].json = bad_tree_availability[0].json.replacen(&record, "{}", 1);
        assert!(validate_availability_rows(&bad_tree_availability).is_err());
        let mut bad_rss_availability = rows.clone();
        bad_rss_availability[0].json = bad_rss_availability[0].json.replacen(
            "\"rss_current_bytes\":20000000",
            "\"rss_current_bytes\":null",
            1,
        );
        assert!(validate_availability_rows(&bad_rss_availability).is_err());

        let mut bad_locality = rows.clone();
        bad_locality[transition].json = bad_locality[transition].json.replacen(
            "\"rope_nodes_read\":1",
            "\"rope_nodes_read\":33",
            1,
        );
        assert!(validate_locality_rows(&bad_locality).is_err());
        let mut bad_payload_read = rows.clone();
        let counters = json_object(&bad_payload_read[transition].json, "counters")
            .unwrap()
            .to_owned();
        let mutated = counters.replacen(
            "\"unaffected_payload_reads\":0",
            "\"unaffected_payload_reads\":1",
            1,
        );
        bad_payload_read[transition].json = bad_payload_read[transition]
            .json
            .replacen(&counters, &mutated, 1);
        assert!(validate_locality_rows(&bad_payload_read).is_err());
        let mut bad_payload_write = rows.clone();
        let counters = json_object(&bad_payload_write[transition].json, "counters")
            .unwrap()
            .to_owned();
        let written = json_u128(&counters, "payload_bytes_written").unwrap();
        let mutated = counters.replacen(
            &format!("\"payload_bytes_written\":{written}"),
            &format!("\"payload_bytes_written\":{}", written + 1),
            1,
        );
        bad_payload_write[transition].json = bad_payload_write[transition]
            .json
            .replacen(&counters, &mutated, 1);
        assert!(validate_locality_rows(&bad_payload_write).is_err());
        let burst = rows.iter().position(|row| row.row_id == "C07-001").unwrap();
        let mut bad_burst_native_aggregate = rows.clone();
        let native = json_object(&bad_burst_native_aggregate[burst].json, "native")
            .unwrap()
            .to_owned();
        let bytes_read = json_u128(&native, "bytes_read").unwrap();
        let mutated = native.replacen(
            &format!("\"bytes_read\":{bytes_read}"),
            &format!("\"bytes_read\":{}", bytes_read + 1),
            1,
        );
        bad_burst_native_aggregate[burst].json = bad_burst_native_aggregate[burst]
            .json
            .replacen(&native, &mutated, 1);
        assert!(validate_locality_rows(&bad_burst_native_aggregate).is_err());

        let logical_insert = rows.iter().position(|row| row.row_id == "C05-002").unwrap();
        let mut bad_refresh_route = rows.clone();
        bad_refresh_route[logical_insert].native_route = "FullFallback".to_owned();
        bad_refresh_route[logical_insert].json = bad_refresh_route[logical_insert].json.replacen(
            "\"native_route\":\"CloneShift\"",
            "\"native_route\":\"FullFallback\"",
            1,
        );
        assert!(validate_refresh_rows(&bad_refresh_route).is_err());

        let history = rows.iter().position(|row| row.row_id == "C04-001").unwrap();
        let mut bad_history = rows.clone();
        bad_history[history].json =
            bad_history[history]
                .json
                .replacen("\"head\":\"R5\"", "\"head\":\"R6\"", 1);
        assert!(validate_history_rows(&bad_history).is_err());
        let mut bad_history_digest = rows.clone();
        let digest = json_string(
            json_object(&bad_history_digest[history].json, "oracle").unwrap(),
            "content_digest",
        )
        .unwrap();
        bad_history_digest[history].json = bad_history_digest[history].json.replacen(
            &format!("\"content_digest\":\"{digest}\""),
            &format!("\"content_digest\":\"{}\"", "f".repeat(64)),
            1,
        );
        assert!(validate_history_rows(&bad_history_digest).is_err());
        let mut bad_history_probe = rows.clone();
        bad_history_probe[history].json = bad_history_probe[history].json.replacen(
            "\"root\":\"R0\",\"ordinal\":1",
            "\"root\":\"R0\",\"ordinal\":2",
            1,
        );
        assert!(validate_history_rows(&bad_history_probe).is_err());
        let milestone = rows.iter().position(|row| row.row_id == "C08-003").unwrap();
        let mut bad_terminal_length = rows.clone();
        let oracle = json_object(&bad_terminal_length[milestone].json, "oracle")
            .unwrap()
            .to_owned();
        let mutated_oracle = oracle.replacen(
            &format!("\"logical_length\":{INITIAL_BYTES}"),
            &format!("\"logical_length\":{}", INITIAL_BYTES + 1),
            1,
        );
        bad_terminal_length[milestone].json =
            bad_terminal_length[milestone]
                .json
                .replacen(&oracle, &mutated_oracle, 1);
        assert!(summary_json(
            &campaign,
            &bad_terminal_length,
            &source,
            &master,
            complete,
            &"8".repeat(64)
        )
        .is_err());
        let mut bad_milestone = rows.clone();
        bad_milestone[milestone].json = bad_milestone[milestone].json.replacen(
            "\"metadata_exact\":true",
            "\"metadata_exact\":false",
            1,
        );
        assert!(validate_history_rows(&bad_milestone).is_err());
        let mut bad_cleanup = rows.clone();
        bad_cleanup[milestone].json = bad_cleanup[milestone].json.replacen(
            "\"cleanup_residue_entries\":0",
            "\"cleanup_residue_entries\":1",
            1,
        );
        assert!(validate_history_rows(&bad_cleanup).is_err());
        let mut bad_live_inventory = rows.clone();
        bad_live_inventory[milestone].json = bad_live_inventory[milestone].json.replacen(
            "\"live_extra_user_files\":0",
            "\"live_extra_user_files\":1",
            1,
        );
        assert!(validate_history_rows(&bad_live_inventory).is_err());
        let mut bad_mtime = rows.clone();
        let expected_mtime = FIXTURE_MTIME_SECONDS + 35;
        bad_mtime[milestone].json = bad_mtime[milestone].json.replacen(
            &format!("\"fresh_metadata\":{{\"mode\":420,\"mtime_seconds\":{expected_mtime}"),
            &format!(
                "\"fresh_metadata\":{{\"mode\":420,\"mtime_seconds\":{}",
                expected_mtime + 1
            ),
            1,
        );
        assert!(validate_history_rows(&bad_mtime).is_err());
        let mut retained_live_residue = rows.clone();
        let first_milestone = retained_live_residue
            .iter()
            .position(|row| row.row_id == "C08-001")
            .unwrap();
        retained_live_residue[first_milestone].json = retained_live_residue[first_milestone]
            .json
            .replacen("\"residue_entries\":0", "\"residue_entries\":7", 1);
        let milestone_markdown = summary_markdown(
            &campaign,
            &retained_live_residue,
            &source,
            &master,
            complete,
        )
        .unwrap();
        assert!(milestone_markdown.lines().any(|line| {
            line.starts_with("| R15 | Physical-chain milestone")
                && line.ends_with("| `PASS` | `PASS` | `PASS` |")
        }));

        let mut revise = rows.clone();
        revise[0].status = "REVISE".to_owned();
        let revise_summary = summary_json(
            &campaign,
            &revise,
            &source,
            &master,
            complete,
            &"8".repeat(64),
        )
        .unwrap();
        assert_eq!(json_string(&revise_summary, "status").unwrap(), "REVISE");
        assert!(
            summary_markdown(&campaign, &revise, &source, &master, complete)
                .unwrap()
                .contains("Disposition: `REVISE`")
        );
        assert!(campaign_time(&campaign, complete, Disposition::Revise).contains("status=REVISE\n"));

        let burst = rows.iter().find(|row| row.row_id == "C07-001").unwrap();
        assert_eq!(row_u128(burst, "rope_nodes_read").unwrap(), 8);
        assert_eq!(
            json_all_u128(&burst.json, "rope_nodes_read").unwrap().len(),
            9
        );
        fs::remove_dir_all(run).unwrap();
    }

    #[test]
    fn residue_and_storage_regressions_fail_before_cleanup_or_null_coercion() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-residue-contract-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("generation.sqlite-wal"), b"wal").unwrap();
        fs::write(root.join("CURRENT.tmp"), b"selector").unwrap();
        fs::create_dir(root.join(".layerfs-owned-temp")).unwrap();
        assert_eq!(residue_count(&root).unwrap(), 3);
        let work = root.join("work");
        fs::create_dir(&work).unwrap();
        fs::create_dir(work.join("store")).unwrap();
        fs::create_dir(work.join("milestone-R34")).unwrap();
        assert_eq!(terminal_work_residue_count(&work).unwrap(), 1);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(residue_count(&root).unwrap(), 0);

        let before = Diagnostics {
            database_bytes: Some(100),
            logical_engine_bytes: Some(90),
            object_bytes_written: 80,
            ..Diagnostics::default()
        };
        let unavailable = Diagnostics {
            database_bytes: None,
            logical_engine_bytes: Some(90),
            object_bytes_written: 80,
            ..Diagnostics::default()
        };
        let regressed = Diagnostics {
            database_bytes: Some(99),
            logical_engine_bytes: Some(89),
            object_bytes_written: 79,
            ..Diagnostics::default()
        };
        assert!(verify_storage_transition(&before, &unavailable).is_err());
        assert!(verify_storage_transition(&before, &regressed).is_err());
    }

    #[test]
    fn live_and_fresh_single_file_inventory_and_metadata_are_independently_gated() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-stage1.1-inventory-{}-{}",
            std::process::id(),
            unix_ns().unwrap()
        ));
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join(FILE_PATH), b"payload").unwrap();
        verify_single_file_destination(&root).unwrap();
        fs::write(root.join("extra"), b"unexpected").unwrap();
        assert!(verify_single_file_destination(&root).is_err());
        fs::remove_dir_all(root).unwrap();

        let mut metadata = synthetic_metadata(34);
        verify_supported_metadata(&metadata, "synthetic R34").unwrap();
        metadata.mode = 0o600;
        assert!(verify_supported_metadata(&metadata, "synthetic R34").is_err());
        metadata = synthetic_metadata(34);
        metadata.xattrs.push(b"user.test", b"value").unwrap();
        assert!(verify_supported_metadata(&metadata, "synthetic R34").is_err());
    }

    #[test]
    fn source_custody_includes_every_workspace_manifest() {
        let paths = rust_cargo_source_paths().unwrap();
        for manifest in [
            "Cargo.toml",
            "Cargo.lock",
            "crates/layerfs-core/Cargo.toml",
            "crates/layerfs-engine/Cargo.toml",
            "crates/layerfs-os/Cargo.toml",
            "crates/layerfs-sdk/Cargo.toml",
            "crates/layerfs-vfs/Cargo.toml",
            "tools/layerfs-eval/Cargo.toml",
        ] {
            assert!(paths.iter().any(|path| path == manifest), "{manifest}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn small_real_apple_routes_cover_both_directions_burst_history_and_metadata() {
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "layerfs-stage1.1-small-routes-{}-{}",
                std::process::id(),
                unix_ns().unwrap()
            ));
        fs::create_dir(&base).unwrap();
        let store = base.join("store");
        let opened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev).unwrap();
        let mut source = opened
            .fs
            .materialize_external(opened.head, &base.join("source"))
            .unwrap();
        let mut expected = (0..64 * 1024)
            .map(|index| (index as u8).wrapping_mul(13))
            .collect::<Vec<_>>();
        fs::write(source.path().join("file"), &expected).unwrap();
        let root0 = source.capture_quiescent().unwrap();
        drop(source);
        fs::remove_dir_all(base.join("source")).unwrap();
        let state0 = opened.fs.current_head("main").unwrap();
        assert_eq!(state0.root, root0);
        let mut managed = opened.fs.materialize_managed(root0).unwrap();

        let native = managed
            .replace_observed("file", 1_003, 0, b"physical-insert")
            .unwrap();
        assert_eq!(
            native.native.route,
            Some(layerfs_sdk::NativeRoute::InPlaceShift)
        );
        expected.splice(1_003..1_003, *b"physical-insert");
        let mut live = Vec::new();
        managed.read_to("file", &mut live).unwrap();
        assert_eq!(live, expected);
        let before = opened.fs.diagnostics().unwrap();
        let (state1, checkpoint) = managed.checkpoint_observed().unwrap();
        let after = opened.fs.diagnostics().unwrap();
        let checkpoint_delta = EngineDelta::between(&before, &after).unwrap();
        checkpoint_delta.verify_trusted_transition().unwrap();
        assert_eq!(checkpoint.descriptor_resets, 1);
        let mut canonical = Vec::new();
        opened
            .fs
            .read_to(state1.root, "file", &mut canonical)
            .unwrap();
        assert_eq!(canonical, expected);

        let (state2, logical) = opened
            .fs
            .replace_range_observed(&state1, "file", 5_007, 4, Cursor::new(*b"LOGI"))
            .unwrap();
        assert_eq!(logical.rope.cdc_bytes_scanned, 4);
        expected.splice(5_007..5_011, *b"LOGI");
        let refresh = managed.refresh(&state2).unwrap();
        assert!(matches!(
            refresh.native.route,
            Some(layerfs_sdk::NativeRoute::ClonePatch | layerfs_sdk::NativeRoute::InPlacePatch)
        ));
        let (accepted, logical) = opened
            .fs
            .replace_range_for_refresh_observed(
                &state2,
                "file",
                7_777,
                3,
                Cursor::new(*b"random-size-change"),
            )
            .unwrap();
        assert_eq!(logical.rope.cdc_bytes_scanned, 18);
        let suffix = expected.len() as u64 - 7_777 - 3;
        expected.splice(7_777..7_780, *b"random-size-change");
        let refresh = managed.refresh_splice(&accepted).unwrap();
        assert!(matches!(
            refresh.native.route,
            Some(layerfs_sdk::NativeRoute::CloneShift | layerfs_sdk::NativeRoute::InPlaceShift)
        ));
        assert_eq!(refresh.full_fallback_files, 0);
        assert_eq!(refresh.native.suffix_bytes_shifted, suffix);
        assert_eq!(refresh.native.bytes_read, suffix);
        assert_eq!(refresh.native.bytes_written, suffix + 18);
        managed.replace("file", 10_001, 0, b"burst").unwrap();
        expected.splice(10_001..10_001, *b"burst");
        managed.replace("file", 20_003, 3, b"B").unwrap();
        expected.splice(20_003..20_006, *b"B");
        let before_burst = opened.fs.diagnostics().unwrap();
        let (state3, burst, steps) = managed.checkpoint_observed_detailed().unwrap();
        let after_burst = opened.fs.diagnostics().unwrap();
        let burst_delta = EngineDelta::between(&before_burst, &after_burst).unwrap();
        burst_delta.verify_trusted_transition().unwrap();
        assert_eq!(burst.descriptor_resets, 1);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].tree_level_before, Some(0));
        verify_locality(&steps[0].counters, 5, 0).unwrap();
        verify_locality(&steps[1].counters, 1, 0).unwrap();
        let retained_metadata = managed.read_metadata("file").unwrap();

        let verified = LayerFs::open(&store).unwrap();
        assert_eq!(opened.fs.counter_snapshot().unwrap().active_connections, 1);
        assert_eq!(
            verified.fs.counter_snapshot().unwrap().active_connections,
            1
        );
        assert_eq!(open_store_connection_count(Some(&store)).unwrap(), 2);
        let mut old = Vec::new();
        verified.fs.read_to(root0, "file", &mut old).unwrap();
        assert_eq!(old.len(), 64 * 1024);
        let mut terminal = Vec::new();
        verified
            .fs
            .read_to(state3.root, "file", &mut terminal)
            .unwrap();
        assert_eq!(terminal, expected);
        let mut witness = verified
            .fs
            .materialize_external(state3.root, &base.join("witness"))
            .unwrap();
        assert_eq!(fs::read(witness.path().join("file")).unwrap(), expected);
        assert_eq!(witness.read_metadata("file").unwrap(), retained_metadata);
        witness.discard().unwrap();
        drop(witness);
        fs::remove_dir_all(base.join("witness")).unwrap();
        drop(verified);
        managed.discard().unwrap();
        drop(opened);
        fs::remove_dir_all(base).unwrap();
    }
}
