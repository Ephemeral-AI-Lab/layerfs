//! A layout 2 home (2.0): its first call converts it to layout 3, once
//! nothing in it is live. The doctor reads what would refuse that
//! conversion now (the conversion's own survey, read only: stale leases
//! stay) and converts nothing; `home.layout` says the conversion is to
//! come. No shell line ends what refuses it, so these findings carry no
//! `fix`: the message says what to do.
use super::records::Findings;
use crate::migrate::layout_2_blockers;
use std::path::Path;

/// The conversion's blockers as errors, and the conversion still to come.
pub(super) fn check(home: &Path, findings: &mut Findings) {
    let shown = home.display().to_string();
    let blockers = match layout_2_blockers(home) {
        Ok(blockers) => blockers,
        Err(refusal) => {
            findings.error(
                ("home", &shown),
                format!(
                    "this layout 2 home could not be surveyed for its conversion: {}; the first call on the home reports what refuses it",
                    refusal.message
                ),
                String::new(),
            );
            return;
        }
    };
    for (kind, name, why) in &blockers {
        // a lease is held on a Store; a create intent is a project's
        let kind = match *kind {
            "lease" => "store",
            "intent" => "project",
            other => other,
        };
        let (state, what) = why.split_once("; ").unwrap_or((why, ""));
        let what = match what {
            "" => String::new(),
            what => format!("; {what}"),
        };
        findings.error(
            (kind, name),
            format!("{kind} {name} {state}: it refuses this home's conversion to layout 3{what}"),
            String::new(),
        );
    }
}
