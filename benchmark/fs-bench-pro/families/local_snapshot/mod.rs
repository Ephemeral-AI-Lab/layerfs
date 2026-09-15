use super::workspace_common::{Case, Content, Entry, EntryKind, Receipt, MTIME};
use super::{ordinary_workloads, Result};

pub(crate) const FAMILY_ID: &str = "local_snapshot";

/// The #149-scoped 25,000 one-byte-file lifecycle: C1 creates all files,
/// C2 changes the 256 ordinals `97*j`, C3 restores them, then End. Three
/// distinct operations inside one sample, never three repetitions.
pub(crate) const FILE_COUNT: usize = 25_000;
const CASE_ID: &str = "local-snapshot-create-25000-onebyte-v1";

pub(crate) fn ordinal_path(ordinal: usize) -> String {
    format!("f{ordinal:05}")
}

pub(crate) fn original_byte(ordinal: usize) -> u8 {
    (ordinal % 251) as u8
}

pub(crate) fn edited_byte(ordinal: usize) -> u8 {
    ((ordinal + 1) % 251) as u8
}

pub(crate) fn edited_ordinals() -> Vec<usize> {
    (0..256).map(|j| 97 * j).collect()
}

fn file_entry(ordinal: usize, byte: u8) -> Entry {
    Entry {
        path: ordinal_path(ordinal),
        kind: EntryKind::File(Content::Literal(vec![byte])),
        mode: 0o644,
        mtime_seconds: MTIME,
        mtime_nanoseconds: 0,
    }
}

fn root_entry() -> Entry {
    Entry {
        path: ".".into(),
        kind: EntryKind::Directory,
        mode: 0o755,
        mtime_seconds: MTIME,
        mtime_nanoseconds: 0,
    }
}

pub(crate) fn cases() -> Vec<Case> {
    vec![Case {
        id: CASE_ID.into(),
        family: FAMILY_ID,
        tier: FILE_COUNT,
        kind: "local-snapshot-create",
    }]
}

/// The initial namespace is empty: only the 0755 normalized root.
pub(crate) fn fixture(_case: &Case, _seed: u8) -> Result<Vec<Entry>> {
    Ok(vec![root_entry()])
}

/// State after `step` Commits: 0 = input, 1 = C1 (all original), 2 = C2
/// (256 edited), 3 = C3 (restored; payload equality may reuse C1 objects).
pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    let entries = ordinary_workloads::expected(case, seed, step)?;
    Ok(entries)
}

pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt> {
    ordinary_workloads::apply(case, seed, step, verify)
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 1 || rows[0].id != CASE_ID || rows[0].tier != FILE_COUNT {
        return Err("local_snapshot registry must hold exactly the scoped 25k case".into());
    }
    let edited = edited_ordinals();
    if edited.len() != 256 || edited[1] != 97 || edited[255] != 97 * 255 {
        return Err("edited ordinal schedule must be 97*j for j=0..255".into());
    }
    if original_byte(97) == edited_byte(97) {
        return Err("edited byte equation must differ from the original".into());
    }
    let case = &rows[0];
    for (step, files) in [(1_usize, FILE_COUNT), (2, FILE_COUNT), (3, FILE_COUNT)] {
        let state = expected(case, 1, step)?;
        if state.len() != files + 1 {
            return Err(format!("expected step {step} file count").into());
        }
    }
    let describe = |entries: &[Entry]| {
        use std::fmt::Write as _;
        let mut text = String::new();
        for entry in entries {
            if let EntryKind::File(Content::Literal(bytes)) = &entry.kind {
                let _ = writeln!(
                    text,
                    "{}\t{:o}\t{}\t{}\t{}",
                    entry.path, entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds,
                    bytes.first().copied().unwrap_or_default()
                );
            }
        }
        text
    };
    let c1 = expected(case, 1, 1)?;
    let c3 = expected(case, 1, 3)?;
    if describe(&c1) != describe(&c3) {
        return Err("C3 must restore the exact C1 bytes".into());
    }
    let c2 = expected(case, 1, 2)?;
    if describe(&c2) == describe(&c1) {
        return Err("C2 must differ from C1".into());
    }
    let differences = describe(&c2)
        .lines()
        .zip(describe(&c1).lines())
        .filter(|(after, before)| after != before)
        .count();
    if differences != 256 {
        return Err("C2 must differ from C1 at exactly 256 files".into());
    }
    Ok(())
}
