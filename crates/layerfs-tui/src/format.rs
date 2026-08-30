use layerfs_cli::SemanticAction;

pub(crate) fn display_id(value: &impl ToString) -> String {
    let value = value.to_string();
    if value.get(1..2).is_none_or(|separator| separator != "~") || value.len() <= 24 {
        return value;
    }
    format!("{}…{}", &value[..12], &value[value.len() - 6..])
}

pub(crate) fn short_number(value: &str) -> String {
    if value.get(1..2) == Some("~") {
        display_id(&value)
    } else {
        value
            .rsplit('-')
            .next()
            .unwrap_or(value)
            .trim_start_matches('0')
            .to_owned()
    }
}

pub(crate) fn action_labels(actions: &[SemanticAction]) -> String {
    let values = actions
        .iter()
        .map(|action| match action {
            SemanticAction::Pull => "pull",
            SemanticAction::Fork => "fork",
            SemanticAction::Push => "push",
            SemanticAction::Add => "add / reconcile",
            SemanticAction::Diff => "diff",
            SemanticAction::Materialize => "materialize",
            SemanticAction::Workspace => "workspace",
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        "inspect".into()
    } else {
        values.join(" · ")
    }
}

pub(crate) fn bytes(value: u64) -> String {
    if value >= 1_048_576 {
        format!("{:.1} MiB", value as f64 / 1_048_576.0)
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}

pub(crate) fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.into();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}
