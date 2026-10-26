//! Commit options, merged Commits and conflict reports on the control wire.
//!
//! A conflict report is bounded by the control record: conflicts and hunk
//! lines are written while they fit and `truncated` says that some were left
//! out. Paths and lines are never cut in the middle.
use crate::{
    control::{ControlError, WorkspaceToken},
    control_history::{boolean, head, outcome, put_head, put_outcome, put_token, token},
    wire::{Reader, Writer, LIMIT},
};
use layerfs_content::merge::{Conflict, ConflictKind, Hunk, LwwDecision};
use layerfs_history::{CommitId, CommitOutcome, MAXIMUM_MESSAGE_BYTES};

/// Longest path or line carried.
pub const TEXT_FIELD_LIMIT: usize = 4096;
/// Bytes a report keeps free for its own trailer.
const RESERVE: usize = 1024;

/// One Workspace Commit request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitControl {
    /// Workspace token.
    pub token: WorkspaceToken,
    /// Publish over a moved head instead of merging.
    pub force: bool,
    /// Description of the captured filesystem change.
    pub message: Option<String>,
}

/// A Commit published after merging with a moved head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergedCommit {
    /// The publication.
    pub outcome: CommitOutcome,
    /// Head the Workspace was bound to.
    pub from: Option<CommitId>,
    /// Head it merged with.
    pub head: Option<CommitId>,
    /// `.lww` decisions.
    pub lww: Vec<LwwDecision>,
}

/// A refused Commit: the merge conflicted, or marker blocks remain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictReport {
    /// Head the Workspace now follows.
    pub head: Option<CommitId>,
    /// Head the Workspace was bound to before the merge.
    pub base: Option<CommitId>,
    /// True when files still hold marker blocks (no merge ran).
    pub markers: bool,
    /// Conflicting paths.
    pub conflicts: Vec<Conflict>,
    /// `.lww` decisions.
    pub lww: Vec<LwwDecision>,
    /// Paths the merge changed in the Workspace.
    pub written: Vec<String>,
    /// True when some entries or lines did not fit.
    pub truncated: bool,
}

pub(crate) fn put_commit_control(
    out: &mut Writer,
    value: &CommitControl,
) -> Result<(), ControlError> {
    put_token(out, value.token)?;
    out.byte(u8::from(value.force))?;
    out.byte(u8::from(value.message.is_some()))?;
    if let Some(text) = &value.message {
        if text.len() > MAXIMUM_MESSAGE_BYTES {
            return Err(ControlError("Commit message"));
        }
        out.blob(text.as_bytes())?;
    }
    Ok(())
}

pub(crate) fn commit_control(input: &mut Reader<'_>) -> Result<CommitControl, ControlError> {
    let token = token(input)?;
    let force = boolean(input)?;
    let mut text = || -> Result<Option<String>, ControlError> {
        if boolean(input)? {
            Ok(Some(input.text(MAXIMUM_MESSAGE_BYTES)?))
        } else {
            Ok(None)
        }
    };
    let message = text()?;
    Ok(CommitControl {
        token,
        force,
        message,
    })
}

fn put_text(out: &mut Writer, value: &str) -> Result<(), ControlError> {
    if value.len() > TEXT_FIELD_LIMIT {
        return Err(ControlError("control text field"));
    }
    out.blob(value.as_bytes())
}

fn fits(out: &Writer, extra: usize) -> bool {
    out.0.len().saturating_add(extra).saturating_add(RESERVE) <= LIMIT
}

fn put_lww(out: &mut Writer, values: &[LwwDecision]) -> Result<bool, ControlError> {
    let kept = values
        .iter()
        .scan(0usize, |size, value| {
            *size +=
                value.path.len().min(TEXT_FIELD_LIMIT) + value.rule.len().min(TEXT_FIELD_LIMIT) + 5;
            Some(*size)
        })
        .take_while(|size| fits(out, *size))
        .count();
    out.put(&(kept as u16).to_be_bytes())?;
    for value in &values[..kept] {
        put_text(out, &clip(&value.path))?;
        put_text(out, &clip(&value.rule))?;
        out.byte(u8::from(value.kept_ours) | (u8::from(value.deleted) << 1))?;
    }
    Ok(kept < values.len())
}

fn lww(input: &mut Reader<'_>) -> Result<Vec<LwwDecision>, ControlError> {
    let count = u16::from_be_bytes(input.array()?);
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let path = input.text(TEXT_FIELD_LIMIT)?;
        let rule = input.text(TEXT_FIELD_LIMIT)?;
        let flags = input.byte()?;
        if flags > 3 {
            return Err(ControlError("lww flags"));
        }
        out.push(LwwDecision {
            path,
            kept_ours: flags & 1 != 0,
            deleted: flags & 2 != 0,
            rule,
        });
    }
    Ok(out)
}

/// The text cut at a character boundary within the field limit.
fn clip(value: &str) -> String {
    let mut end = value.len().min(TEXT_FIELD_LIMIT);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub(crate) fn put_merged(out: &mut Writer, value: &MergedCommit) -> Result<(), ControlError> {
    put_outcome(out, &value.outcome)?;
    put_head(out, value.from)?;
    put_head(out, value.head)?;
    put_lww(out, &value.lww).map(|_| ())
}

pub(crate) fn merged(input: &mut Reader<'_>) -> Result<MergedCommit, ControlError> {
    Ok(MergedCommit {
        outcome: outcome(input)?,
        from: head(input)?,
        head: head(input)?,
        lww: lww(input)?,
    })
}

const KINDS: [ConflictKind; 8] = [
    ConflictKind::Content,
    ConflictKind::AddAdd,
    ConflictKind::ModifyDelete,
    ConflictKind::DeleteModify,
    ConflictKind::Type,
    ConflictKind::Binary,
    ConflictKind::Markers,
    ConflictKind::Mode,
];

pub(crate) fn put_lines(out: &mut Writer, lines: &[String]) -> Result<bool, ControlError> {
    let kept = lines
        .iter()
        .scan(0usize, |size, line| {
            *size += line.len().min(TEXT_FIELD_LIMIT) + 2;
            Some(*size)
        })
        .take_while(|size| fits(out, *size))
        .count();
    out.put(&(kept as u16).to_be_bytes())?;
    for line in &lines[..kept] {
        put_text(out, &clip(line))?;
    }
    Ok(kept < lines.len())
}

pub(crate) fn lines(input: &mut Reader<'_>) -> Result<Vec<String>, ControlError> {
    let count = u16::from_be_bytes(input.array()?);
    (0..count).map(|_| input.text(TEXT_FIELD_LIMIT)).collect()
}

pub(crate) fn put_report(out: &mut Writer, value: &ConflictReport) -> Result<(), ControlError> {
    put_head(out, value.head)?;
    put_head(out, value.base)?;
    out.byte(u8::from(value.markers))?;
    let mut truncated = value.truncated;
    let kept = value
        .conflicts
        .iter()
        .scan(0usize, |size, conflict| {
            *size += conflict.path.len().min(TEXT_FIELD_LIMIT) + 8;
            Some(*size)
        })
        .take_while(|size| fits(out, *size))
        .count();
    truncated |= kept < value.conflicts.len();
    out.put(&(kept as u16).to_be_bytes())?;
    for conflict in &value.conflicts[..kept] {
        put_text(out, &clip(&conflict.path))?;
        let kind = KINDS.iter().position(|kind| *kind == conflict.kind);
        out.byte(kind.map_or(0, |index| index as u8 + 1))?;
        let hunks = conflict.hunks.iter().take_while(|_| fits(out, 64)).count();
        truncated |= hunks < conflict.hunks.len();
        out.put(&(hunks as u16).to_be_bytes())?;
        for hunk in &conflict.hunks[..hunks] {
            out.put(&(hunk.line as u32).to_be_bytes())?;
            truncated |= put_lines(out, &hunk.ours)?;
            truncated |= put_lines(out, &hunk.theirs)?;
        }
    }
    truncated |= put_lww(out, &value.lww)?;
    let written = value
        .written
        .iter()
        .scan(0usize, |size, path| {
            *size += path.len().min(TEXT_FIELD_LIMIT) + 2;
            Some(*size)
        })
        .take_while(|size| fits(out, *size))
        .count();
    truncated |= written < value.written.len();
    out.put(&(written as u16).to_be_bytes())?;
    for path in &value.written[..written] {
        put_text(out, &clip(path))?;
    }
    out.byte(u8::from(truncated))
}

pub(crate) fn report(input: &mut Reader<'_>) -> Result<ConflictReport, ControlError> {
    let head_of = head;
    let head = head_of(input)?;
    let base = head_of(input)?;
    let markers = boolean(input)?;
    let count = u16::from_be_bytes(input.array()?);
    let mut conflicts = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let path = input.text(TEXT_FIELD_LIMIT)?;
        let kind = match input.byte()? {
            index @ 1..=8 => KINDS[index as usize - 1],
            _ => return Err(ControlError("conflict kind")),
        };
        let hunks = u16::from_be_bytes(input.array()?);
        let mut list = Vec::with_capacity(hunks as usize);
        for _ in 0..hunks {
            list.push(Hunk {
                line: u32::from_be_bytes(input.array()?) as usize,
                ours: lines(input)?,
                theirs: lines(input)?,
            });
        }
        conflicts.push(Conflict {
            path,
            kind,
            hunks: list,
        });
    }
    let lww = lww(input)?;
    let written = lines(input)?;
    Ok(ConflictReport {
        head,
        base,
        markers,
        conflicts,
        lww,
        written,
        truncated: boolean(input)?,
    })
}
