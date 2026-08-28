use super::super::schedule_model::{EditKind, EditSpec, ScheduledRow};
use super::synthetic::synthetic_ref;
use crate::legacy_full::RefState;
pub(super) fn synthetic_routing(
    scheduled: &ScheduledRow,
    transition: Option<u8>,
    edit: Option<&EditSpec>,
) -> (Option<RefState>, Option<RefState>, String) {
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
    (pre_ref, post_ref, native_route.to_owned())
}
