use super::artifact::display_error;
use super::schedule_model::{EditSpec, ScheduledRow};
use crate::stage1_fixture::EvalResult;
pub(crate) fn build_rows(edits: &[EditSpec]) -> EvalResult<Vec<ScheduledRow>> {
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
    Ok(rows)
}
